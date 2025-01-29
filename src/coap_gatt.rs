// SPDX-FileCopyrightText: Copyright 2022-2024 EDF (Électricité de France S.A.)
// SPDX-License-Identifier: BSD-3-Clause
// See README for all details on copyright, authorship and license.
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

use coap_handler::Handler;
use coap_message::error::RenderableOnMinimal;
use coap_message::MinimalWritableMessage;

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
pub struct Connection {
    /// An accessor to a ResourceServer
    rs: &'static crate::Rs,
}

// This will do more once a future version of CoAP-over-GATT is used
impl Connection {
    pub fn new(rs: &'static crate::Rs) -> Self {
        Self { rs }
    }

    /// Call this whenever a BLE write arrives. The response value is what any BLE read should
    /// henceforth produce.
    ///
    /// Note that this passes in data that is primarily supposed to be read as `&mut`. This is to
    /// later allow OSCORE decryption in-place.
    pub fn write(&mut self, written: &mut [u8]) -> heapless::Vec<u8, { crate::MAX_MESSAGE_LEN }> {
        let request = coap_gatt_utils::parse_mut(written).unwrap();

        let mut locked = self
            .rs
            .try_lock()
            // FIXME err properly or become more confident that this never happens
            .expect("Simultaneous access should not happen through single executor");
        let handler = &mut *locked;

        // We have a &mut, but can't tell the handler through the API; maybe an OscoreEdhocHandler
        // should have something extra that takes a &mut parsed message?
        let extracted = handler.extract_request_data(&request);

        coap_gatt_utils::write(|response| {
            // Error handling here is a tad odd: our response has a `.reset()`, but libOSCORE
            // doesn't have the API (in particular it can't rely on its backend to have a
            // reset/rewind), so we have to do separate protect steps.
            //
            // At the same time, we have to do everything in a single .reset()able
            // coap_gatt_utils::write, because the lifetimes of the errors unfortunately may be
            // bound to its buffer.
            //
            // This makes this whole mess even more arcane and verbose than is already generally
            // the trouble with writing servers for coap-handler 0.2.

            match extracted {
                Ok(extracted) => {
                    let rendered = handler.build_response(response, extracted);

                    if let Err(e) = rendered {
                        response.reset();
                        let rendered = e.render(response);

                        if let Err(_) = rendered {
                            response.reset();
                            response.set_code(coap_numbers::code::INTERNAL_SERVER_ERROR);
                        }
                    }
                }
                Err(e) => {
                    let rendered = e.render(response);

                    if let Err(_) = rendered {
                        response.reset();
                        response.set_code(coap_numbers::code::INTERNAL_SERVER_ERROR);
                    }
                }
            };

            use coap_message_utils::ShowMessageExt;
            defmt::info!("Responding with {}", response.show());
        })
    }
}
