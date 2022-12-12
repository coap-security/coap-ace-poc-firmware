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

/// State held inside a single connection
///
/// As coap-over-gatt-02 is practically stateless as long as responses are available immediately
/// (which in this implementation's model they are), this carries no request state at all.
pub struct Connection<H, DH, A>
where
    H: coap_handler::Handler,
    DH: core::ops::DerefMut<Target=H>,
    A: Fn() -> Option<DH>,
{
    accessor: A,
}

// This will do more once a future version of CoAP-over-GATT is used
impl<H, DH, A> Connection<H, DH, A>
where
    H: coap_handler::Handler,
    DH: core::ops::DerefMut<Target=H>,
    A: Fn() -> Option<DH>,
{
    /// Create a new connection
    ///
    /// The accessor is the connection's way of obtaining an exclusive reference to a CoAP handler,
    /// typically through a platform specific mutex. If that mutex can not be obtained (eg. because
    /// the CoAP handler is just being used to concurrently serve a different Bluetooth connection,
    /// or a CoAP request on a different transport altogether), it's OK for it to return None:
    /// Requests arriving during that time will just receive a 5.03 Service Unavailable response,
    /// and clients are free to retry immediately.
    pub fn new(accessor: A) -> Self {
        Self { accessor }
    }

    /// Call this whenever a BLE write arrives. The response value is what any BLE read should
    /// henceforth produce.
    pub fn write(&mut self, written: &[u8]) -> heapless::Vec<u8, 200> {
        let mut request = coap_gatt_implementations::parse(written).unwrap();

        let handler = (self.accessor)();

        if let Some(mut handler) = handler {
            let extracted = handler.extract_request_data(&request);

            coap_gatt_implementations::write(
                |response| handler.build_response(response, extracted)
                )
        } else {
            use coap_message::MinimalWritableMessage;
            coap_gatt_implementations::write(
                |response| {
                    response.set_code(coap_numbers::code::SERVICE_UNAVAILABLE);
                    response.add_option_uint(coap_numbers::option::MAX_AGE, 0u8);
                })
        }
    }
}

