//! Implementation of the CoAP-over-GATT protocol
//!
//! This is independent of a Bluetooth implementation, and expects its connection objects to be
//! driven by the actual Bluetooth implementation, calling [Connection::write] when a write request
//! comes in, etc.
//!
//! The current implementation is rather simple, as is the underlying draft
//! draft-amsuess-core-coap-over-gatt-02 (which does not support role reversal, and would need
//! parallel connections on parallel characteristics to implement concurrent or longer-running
//! requests).
//!
//! The module's simplicity is also due to all the message parsing being delegated to the
//! [coap_gatt_utils] module. In fact, this module might move in there over time.

use core::ops::Deref;

use coap_handler::Handler;
use coap_message::{ReadableMessage, MinimalWritableMessage};
use ace_oscore_helpers::resourceserver::ResourceServer;

/// State held inside a single connection
///
/// As coap-over-gatt-02 is practically stateless as long as responses are available immediately
/// (which in this implementation's model they are), this carries no request state at all.
///
/// ## RS and handler factory rationale
///
/// As OSCORE implementation can't quite just sit inside a pre-configured coap-handlers handler,
/// this stores a handler factory separately from a resource server accessor, and invokes libOSCORE
/// before handling the request. This is a bit of a layering violation.
///
/// Ideally, OSCORE should be a pure layer, and the handler wrapped in OSCORE would be a handler
/// again.
///
/// We can't do this because coap-handler only gives us a ReadableMessage for the request, whereas
/// libOSCORE needs it also to be MutableWritable to do its in-place decryption. Also, it requires
/// the response message to allow options-after-payload extensions, but currently that's provided
/// silently (ie. through MutableWritableMessage guarantees exceeding those required by the trait)
/// in the backend we provide.
///
/// ## Fallibility
///
/// RS access may or may not work (depending on whether some other task is just using it). If the
/// RS / handler factory situation above were better, there'd be a fallible accessor to the CoAP
/// handler instead.
///
/// Typically, RS access happens through a platform specific mutex. If that mutex can not be
/// obtained (eg. because the CoAP handler is just being used to concurrently serve a different
/// Bluetooth connection, or a CoAP request on a different transport altogether), it's OK for it to
/// return None: Requests arriving during that time will just receive a 5.03 Service Unavailable
/// response, and clients are free to retry immediately.
pub struct Connection
{
    /// A factory for a CoAP handler
    chf: &'static crate::CoapHandlerFactory,
    /// An accessor to a ResourceServer
    rs: &'static crate::Rs,
}

// This will do more once a future version of CoAP-over-GATT is used
impl Connection
{
    pub fn new(chf: &'static crate::CoapHandlerFactory, rs: &'static crate::Rs) -> Self {
        Self { chf, rs }
    }

    /// Call this whenever a BLE write arrives. The response value is what any BLE read should
    /// henceforth produce.
    ///
    /// Note that this passes in data that is primarily supposed to be read as `&mut`. This is to
    /// later allow OSCORE decryption in-place.
    pub fn write(&mut self, written: &mut [u8]) -> heapless::Vec<u8, { crate::MAX_MESSAGE_LEN }> {
        let mut request = coap_gatt_utils::parse_mut(written).unwrap();

        use coap_message::{ReadableMessage, MessageOption};
        // FIXME: We need to copy things out because ReadableMessage by design only hands out
        // short-lived values (so they can be built in the iterator if need be)
        let mut oscore_option: Option<heapless::Vec::<u8, 16>> = None;
        for o in request.options() {
            if o.number() == coap_numbers::option::OSCORE {
                oscore_option = o.value().try_into()
                    .map_err(|e| {defmt::error!("OSCORE option is too long"); e})
                    .ok();
                break;
            }
        }
        let oscore_option = match &oscore_option {
            Some(o) => liboscore::OscoreOption::parse(&o)
                            .map_err(|e| {defmt::error!("OSCORE option found but parsing failed"); e})
                            .ok(),
            None => None,
        };

        if let Some(oscore_option) = oscore_option {
            // Look it up, lock RS, or 5.03
            if let Some(mut rs) = self.rs.try_lock().ok() {
                if let Some((context, app_claims)) = rs.look_up_context(&oscore_option) {

                    defmt::info!("OSCORE option indicated KID {:?}, found key with claims {:?}", oscore_option.kid(), &app_claims);

                    // The self.rs will actually be locked, because we hold it through `rs` which
                    // goes into the &mut OSCORE context. An advanced version that supports token
                    // upgrades might, rather than passing in a runtime-optional RS, an Either that
                    // can bea &mut to a slot inside the RS that can be upgraded, or an RS through
                    // which something new can be added.
                    let mut handler = self.chf.build(Some(app_claims), &mut self.rs);

                    let (mut correlation, extracted) = liboscore::unprotect_request(
                        &mut request,
                        oscore_option,
                        context,
                        |request| handler.extract_request_data(request),
                    );

                    defmt::info!("OSCORE request processed, building response...");

                    coap_gatt_utils::write(
                        |response|
                            liboscore::protect_response(
                                response,
                                context,
                                &mut correlation,
                                |response|
                                    handler.build_response(response, extracted)
                            )
                        )
                } else {
                    coap_gatt_utils::write(
                        |response| {
                            response.set_code(coap_numbers::code::UNAUTHORIZED);
                            // Could set payload "Security context not found"
                        })
                }
            } else {
                // OSCORE request but the context is busy
                coap_gatt_utils::write(
                    |response| {
                        response.set_code(coap_numbers::code::SERVICE_UNAVAILABLE);
                        response.add_option_uint(coap_numbers::option::MAX_AGE, 0u8);
                    })
            }
        } else {
            // Unprotected requests never have credentials
            let mut handler = self.chf.build(None, &mut self.rs);

            let extracted = handler.extract_request_data(&request);

            coap_gatt_utils::write(
                |response| handler.build_response(response, extracted)
                )
        }
    }
}

