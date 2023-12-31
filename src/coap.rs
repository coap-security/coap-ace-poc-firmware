// SPDX-FileCopyrightText: Copyright 2022 EDF (Électricité de France S.A.)
// SPDX-License-Identifier: BSD-3-Clause
// See README for all details on copyright, authorship and license.
//! CoAP handlers for the demo application
//!
//! This modules's main entry point is [create_coap_handler], which produces a full handler with
//! the resources `/time`, `/leds`, `/temp` and `/identify`, all backed by structs of this module,
//! and `/authz-info`, backed by a resource server.

pub type CoapHandler = impl coap_handler::Handler;

/// Resource handler for the [crate::devicetime] UNIX time tracking.
///
/// Time is read and written as a CBOR unsigned integer indicating seconds from UNIX epoch.
///
/// As the clock is global, it does not need any properties.
///
/// ## Security
///
/// As system time is a critical resource in authorization validation, it should not be left
/// unprotected. It is unprotected in the demo; see the demo's overall documentation for details.
struct Time;

impl coap_handler_implementations::SimpleCBORHandler for Time {
    type Get = u32;
    type Put = u32;
    type Post = ();

    fn get(&mut self) -> Result<Self::Get, u8> {
        crate::devicetime::unixtime().map_err(|_| coap_numbers::code::INTERNAL_SERVER_ERROR)
    }

    fn put(&mut self, representation: &Self::Put) -> u8 {
        crate::devicetime::set_unixtime(*representation);
        coap_numbers::code::CHANGED
    }
}

/// Resource handler for device temperature
///
/// Values are read through GET as CBOR bigfloat (through [BigfloatFixedI32]), which is an easy way
/// to express the underlying sensor's format (quarter degree Celcius) in a self-described way,
/// especially given that this is a constrained device and the peer is not.
struct Temperature {
    softdevice: &'static nrf_softdevice::Softdevice,
}

/// Newtype around fixed::Fixed expressing it as a bigfloat
///
/// One alternative would be to manually construct a float out of this; that'd need:
/// * special handling for the value 0,
/// * a CLZ operation to shift things to the normalized float form, and
///   * either dynamic mantissa length calculations to decide the type (CTZ), or
///   * a good estimate for realistic ranges (2**30°C is pretty much out of spec) that allows
///     picking a fixed float format (half might suffice, with its 10+1 bit mantissa length).
struct BigfloatFixedI32<Frac>(fixed::FixedI32<Frac>);

impl<Frac: typenum::ToInt<i32>> minicbor::encode::Encode for BigfloatFixedI32<Frac> {
    fn encode<W: minicbor::encode::Write>(
        &self,
        e: &mut minicbor::Encoder<W>,
    ) -> Result<(), minicbor::encode::Error<W::Error>> {
        let e = e.tag(minicbor::data::Tag::Bigfloat)?;
        let e = e.array(2)?;
        e.i32(-Frac::to_int())?;
        e.i32(self.0.to_bits())?;
        Ok(())
    }
}

impl coap_handler_implementations::SimpleCBORHandler for Temperature {
    type Get = BigfloatFixedI32<fixed::types::extra::U2>;
    type Put = ();
    type Post = ();

    fn get(&mut self) -> Result<Self::Get, u8> {
        defmt::info!("Reading temperature");
        // Note that this blocks for 50ms according to the docs. If softdevice let us use it as
        // normal in embassy_nrf, we might handle that smarter. (Although coap-handler is not
        // helpful there yet anyway).
        Ok(BigfloatFixedI32(
            nrf_softdevice::temperature_celsius(self.softdevice)
                .map_err(|_| coap_numbers::code::INTERNAL_SERVER_ERROR)?,
        ))
    }
}

/// Resource handler for number of on LEDs active in idle state
///
/// The number can bet GET or PUT as CBOR unsigned integers.
struct Leds(&'static crate::blink::Leds);

impl coap_handler_implementations::SimpleCBORHandler for Leds {
    type Get = u8;
    type Put = u8;
    type Post = ();

    fn get(&mut self) -> Result<Self::Get, u8> {
        Ok(self.0.idle())
    }

    fn put(&mut self, value: &u8) -> u8 {
        self.0.set_idle(*value);
        coap_numbers::code::CHANGED
    }
}

/// Resource handler for making the LEDs blink in order to identifiy the physical device
///
/// The animation sequence is triggered by an empty POST to this resource.
struct Identify(&'static crate::blink::Leds);

impl coap_handler::Handler for Identify {
    type RequestData = u8;

    fn extract_request_data(&mut self, request: &impl coap_message::ReadableMessage) -> u8 {
        use coap_handler_implementations::option_processing::OptionsExt;
        use coap_numbers::code::*;
        if request.code().into() != POST {
            return METHOD_NOT_ALLOWED;
        }
        if request.options().ignore_elective_others().is_err() || !request.payload().is_empty() {
            return BAD_OPTION;
        }

        self.0.run_identify();

        CHANGED
    }
    fn estimate_length(&mut self, _: &u8) -> usize {
        1
    }
    fn build_response(
        &mut self,
        response: &mut impl coap_message::MutableWritableMessage,
        code: u8,
    ) {
        response.set_code(code.try_into().map_err(|_| ()).unwrap());
        response.set_payload(b"");
    }
}

/// Resource handler that is decided at the time the handler is built
pub struct WithPermissions<H: coap_handler::Handler> {
    handler: H,
    permissions: u8,
    error_handler: UnauthorizedSeeAS,
}

impl<H> coap_handler::Handler for WithPermissions<H>
where
    H: coap_handler::Handler,
{
    // Absence of request data indicates insufficient permissions
    type RequestData = Option<H::RequestData>;

    fn extract_request_data(
        &mut self,
        request: &impl coap_message::ReadableMessage,
    ) -> Self::RequestData {
        let codenumber: u8 = request.code().into();
        let codebit = 1u8.checked_shl((codenumber - 1u8).into());
        if codebit.map(|bit| bit & self.permissions != 0) == Some(true) {
            Some(self.handler.extract_request_data(request))
        } else {
            None
        }
    }
    fn estimate_length(&mut self, data: &Self::RequestData) -> usize {
        data.as_ref()
            .map(|d| self.handler.estimate_length(d))
            .unwrap_or(self.error_handler.estimate_length(&()))
    }
    fn build_response(
        &mut self,
        response: &mut impl coap_message::MutableWritableMessage,
        data: Self::RequestData,
    ) {
        if let Some(data) = data {
            self.handler.build_response(response, data)
        } else {
            self.error_handler.build_response(response, ())
        }
    }
}

/// A handler that sends 4.01 (Unauthorized) and AS Request Creation Hints unconditionally. It only
/// encodes the audience and AS, no scope or other hints.
struct UnauthorizedSeeAS(&'static crate::Rs);

impl coap_handler::Handler for UnauthorizedSeeAS {
    type RequestData = ();

    fn extract_request_data(
        &mut self,
        _: &impl coap_message::ReadableMessage,
    ) -> Self::RequestData {
        // We already know all we need
    }
    fn estimate_length(&mut self, _data: &Self::RequestData) -> usize {
        150
    }
    fn build_response(
        &mut self,
        response: &mut impl coap_message::MutableWritableMessage,
        _data: Self::RequestData,
    ) {
        if let Ok(rs) = self.0.try_lock() {
            response.set_code(coap_numbers::code::UNAUTHORIZED.try_into().ok().unwrap());
            response.add_option_uint(
                coap_numbers::option::CONTENT_FORMAT
                    .try_into()
                    .ok()
                    .unwrap(),
                19u8, /* application/ace+cbor */
            );
            let payload = response.payload_mut_with_len(140);
            let mut writer = windowed_infinity::WindowedInfinity::new(payload, 0);
            let mut encoder = ciborium_ll::Encoder::from(&mut writer);

            let rqh = rs.request_creation_hints();
            rqh.push_to_encoder(&mut encoder)
                .expect("Writing to a WindowedInfinity can not fail");

            let written = writer.get_cursor() as _;
            response.truncate(written);
        } else {
            response.set_code(
                coap_numbers::code::SERVICE_UNAVAILABLE
                    .try_into()
                    .ok()
                    .unwrap(),
            );
            response.set_payload(b"");
        }
    }
}

/// Create a tree of CoAP resource as described in this module's documentation out of the
/// individual handler implementations in this module.
///
/// The tree also features a `/.well-known/core` resource listing the other resources.
pub fn create_coap_handler(
    claims: Option<&crate::rs_configuration::ApplicationClaims>,
    softdevice: &'static nrf_softdevice::Softdevice,
    leds: &'static crate::blink::Leds,
    rs: &'static crate::Rs,
) -> CoapHandler {
    use coap_handler_implementations::HandlerBuilder;
    use coap_handler_implementations::ReportingHandlerBuilder;

    // Going through SimpleWrapper is not particularly slim on message sizes, given it adds ETag
    // and Block2 unconditionally, but that could be fixed there on the long run (with a somewhat
    // improved MutableWritableMessage, or better bounds on CBOR serialization size)
    let time_handler = coap_handler_implementations::SimpleWrapper::new_minicbor(Time);

    let authzinfo_handler =
        ace_oscore_helpers::resourceserver::UnprotectedAuthzInfoEndpoint::new(|| {
            rs.try_lock().ok()
        });
    // FIXME should be provided by UnprotectedAuthzInfoEndpoint
    let authzinfo_handler = coap_handler_implementations::wkc::ConstantSingleRecordReport::new(
        authzinfo_handler,
        &[coap_handler_implementations::wkc::Attribute::ResourceType(
            "ace.ai",
        )],
    );

    let identify_handler = WithPermissions {
        handler: Identify(leds),
        permissions: claims.map(|c| c.scope.identify).unwrap_or(0),
        error_handler: UnauthorizedSeeAS(&rs),
    };

    let temperature_handler = WithPermissions {
        handler: coap_handler_implementations::SimpleWrapper::new_minicbor(Temperature {
            softdevice,
        }),
        permissions: claims.map(|c| c.scope.temp).unwrap_or(0),
        error_handler: UnauthorizedSeeAS(&rs),
    };

    let leds_handler = WithPermissions {
        handler: coap_handler_implementations::SimpleWrapper::new_minicbor(Leds(leds)),
        permissions: claims.map(|c| c.scope.leds).unwrap_or(0),
        error_handler: UnauthorizedSeeAS(&rs),
    };

    // Why isn't SimpleWrapper Reporting?
    let time_handler = coap_handler_implementations::wkc::ConstantSingleRecordReport::new(
        time_handler,
        &[coap_handler_implementations::wkc::Attribute::Ct(60)],
    );
    let temperature_handler = coap_handler_implementations::wkc::ConstantSingleRecordReport::new(
        temperature_handler,
        &[coap_handler_implementations::wkc::Attribute::Ct(60)],
    );
    let leds_handler = coap_handler_implementations::wkc::ConstantSingleRecordReport::new(
        leds_handler,
        &[coap_handler_implementations::wkc::Attribute::Ct(60)],
    );
    let identify_handler =
        coap_handler_implementations::wkc::ConstantSingleRecordReport::new(identify_handler, &[]);

    coap_handler_implementations::new_dispatcher()
        // Fully unprotected in the demo only
        .at(&["time"], time_handler)
        // Fully unprotected by design
        .at(&["authz-info"], authzinfo_handler)
        // FIXME: Go through OSCORE
        .at(&["leds"], leds_handler)
        .at(&["temp"], temperature_handler)
        .at(&["identify"], identify_handler)
        .with_wkc()
}
