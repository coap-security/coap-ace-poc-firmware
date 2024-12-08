// SPDX-FileCopyrightText: Copyright 2022 EDF (Électricité de France S.A.)
// SPDX-License-Identifier: BSD-3-Clause
// See README for all details on copyright, authorship and license.
//! CoAP handlers for the demo application
//!
//! This modules's main entry point is [create_coap_handler], which produces a full handler with
//! the resources `/time`, `/leds`, `/temp` and `/identify`, all backed by structs of this module,
//! and `/authz-info`, backed by a resource server.

use coap_message::{
    error::RenderableOnMinimal, Code as _, MinimalWritableMessage, MutableWritableMessage,
    OptionNumber as _, ReadableMessage,
};
use coap_message_utils::Error;
use coap_numbers::code::{CHANGED, UNAUTHORIZED};
use coap_numbers::option::CONTENT_FORMAT;

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

impl coap_handler_implementations::TypeRenderable for Time {
    type Get = u32;
    type Put = u32;
    type Post = ();

    fn get(&mut self) -> Result<Self::Get, u8> {
        crate::devicetime::unixtime().map_err(|_| coap_numbers::code::INTERNAL_SERVER_ERROR)
    }

    fn put(&mut self, representation: &Self::Put) -> u8 {
        crate::devicetime::set_unixtime(*representation);
        CHANGED
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

impl<Frac: typenum::ToInt<i32>, C> minicbor::encode::Encode<C> for BigfloatFixedI32<Frac> {
    fn encode<W: minicbor::encode::Write>(
        &self,
        e: &mut minicbor::Encoder<W>,
        _: &mut C,
    ) -> Result<(), minicbor::encode::Error<W::Error>> {
        let e = e.tag(minicbor::data::IanaTag::Bigfloat)?;
        let e = e.array(2)?;
        e.i32(-Frac::to_int())?;
        e.i32(self.0.to_bits())?;
        Ok(())
    }
}

impl coap_handler_implementations::TypeRenderable for Temperature {
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

impl coap_handler_implementations::TypeRenderable for Leds {
    type Get = u8;
    type Put = u8;
    type Post = ();

    fn get(&mut self) -> Result<Self::Get, u8> {
        Ok(self.0.idle())
    }

    fn put(&mut self, value: &u8) -> u8 {
        self.0.set_idle(*value);
        CHANGED
    }
}

/// Resource handler for making the LEDs blink in order to identifiy the physical device
///
/// The animation sequence is triggered by an empty POST to this resource.
struct Identify(&'static crate::blink::Leds);

impl coap_handler::Handler for Identify {
    type RequestData = ();
    type ExtractRequestError = Error;
    type BuildResponseError<M: MinimalWritableMessage> = M::UnionError;

    fn extract_request_data<M: ReadableMessage>(&mut self, request: &M) -> Result<(), Error> {
        use coap_message_utils::OptionsExt;
        use coap_numbers::code::*;
        if request.code().into() != POST {
            return Err(Error::method_not_allowed());
        }
        request.options().ignore_elective_others()?;
        if !request.payload().is_empty() {
            return Err(Error::bad_request());
        }

        self.0.run_identify();

        Ok(())
    }
    fn estimate_length(&mut self, _: &()) -> usize {
        1
    }
    fn build_response<M: MutableWritableMessage>(
        &mut self,
        response: &mut M,
        _: (),
    ) -> Result<(), Self::BuildResponseError<M>> {
        response.set_code(M::Code::new(CHANGED)?);
        Ok(())
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
    type RequestData = Option<Result<H::RequestData, H::ExtractRequestError>>;
    type ExtractRequestError = core::convert::Infallible;
    type BuildResponseError<M: MinimalWritableMessage> = Result<
        Result<
            H::BuildResponseError<M>,
            <H::ExtractRequestError as RenderableOnMinimal>::Error<M::UnionError>,
        >,
        <UnauthorizedSeeAS as coap_handler::Handler>::BuildResponseError<M>,
    >;

    fn extract_request_data<M: ReadableMessage>(
        &mut self,
        request: &M,
    ) -> Result<Self::RequestData, core::convert::Infallible> {
        let codenumber: u8 = request.code().into();
        let codebit = 1u8.checked_shl((codenumber - 1u8).into());
        if codebit.map(|bit| bit & self.permissions != 0) == Some(true) {
            Ok(Some(self.handler.extract_request_data(request)))
        } else {
            Ok(None)
        }
    }
    fn estimate_length(&mut self, data: &Self::RequestData) -> usize {
        data.as_ref()
            .map(|d| match d {
                Ok(d) => self.handler.estimate_length(d),
                // FIXME how do we get an estimate here?
                Err(_) => 1024,
            })
            .unwrap_or(self.error_handler.estimate_length(&()))
    }
    fn build_response<M: MutableWritableMessage>(
        &mut self,
        response: &mut M,
        data: Self::RequestData,
    ) -> Result<(), Self::BuildResponseError<M>> {
        match data {
            Some(Ok(data)) => self
                .handler
                .build_response(response, data)
                .map_err(|e| Ok(Ok(e)))?,
            Some(Err(e)) => e.render(response).map_err(|e| Ok(Err(e)))?,
            None => self
                .error_handler
                .build_response(response, ())
                .map_err(Err)?,
        }
        Ok(())
    }
}

/// A handler that sends 4.01 (Unauthorized) and AS Request Creation Hints unconditionally. It only
/// encodes the audience and AS, no scope or other hints.
// FIXME: This could become RenderableOnMinimal instead
// FIXME: This is only pub because it shows up in CoAP signatures
pub struct UnauthorizedSeeAS(&'static crate::Rs);

impl coap_handler::Handler for UnauthorizedSeeAS {
    type RequestData = ();
    type ExtractRequestError = core::convert::Infallible;
    type BuildResponseError<M: MinimalWritableMessage> = Result<Error, M::UnionError>;

    fn extract_request_data<M: ReadableMessage>(
        &mut self,
        _: &M,
    ) -> Result<Self::RequestData, Self::ExtractRequestError> {
        Ok(())
        // We already know all we need
    }
    fn estimate_length(&mut self, _data: &Self::RequestData) -> usize {
        150
    }
    fn build_response<M: MutableWritableMessage>(
        &mut self,
        response: &mut M,
        _data: Self::RequestData,
    ) -> Result<(), Self::BuildResponseError<M>> {
        if let Ok(rs) = self.0.try_lock() {
            response.set_code(M::Code::new(UNAUTHORIZED).map_err(|e| Err(e.into()))?);
            response
                .add_option_uint(
                    M::OptionNumber::new(CONTENT_FORMAT).map_err(|e| Err(e.into()))?,
                    19u8, /* application/ace+cbor */
                )
                .map_err(|e| Err(e.into()))?;
            let payload = response
                .payload_mut_with_len(140)
                .map_err(|e| Err(e.into()))?;
            let mut writer = windowed_infinity::WindowedInfinity::new(payload, 0);
            let mut encoder = ciborium_ll::Encoder::from(&mut writer);

            let rqh = rs.request_creation_hints();
            rqh.push_to_encoder(&mut encoder)
                .expect("Writing to a WindowedInfinity can not fail");

            let written = writer.cursor() as _;
            response.truncate(written).map_err(|e| Err(e.into()))?;
            Ok(())
        } else {
            Err(Ok(Error::service_unavailable()))
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

    // Going through TypeHandler is not particularly slim on message sizes, given it adds ETag
    // and Block2 unconditionally, but that could be fixed there on the long run (with a somewhat
    // improved MutableWritableMessage, or better bounds on CBOR serialization size)
    let time_handler = coap_handler_implementations::TypeHandler::new_minicbor(Time);

    let identify_handler = WithPermissions {
        handler: Identify(leds),
        permissions: claims.map(|c| c.scope.identify).unwrap_or(0),
        error_handler: UnauthorizedSeeAS(&rs),
    };

    let temperature_handler = WithPermissions {
        handler: coap_handler_implementations::TypeHandler::new_minicbor_0_24(Temperature {
            softdevice,
        }),
        permissions: claims.map(|c| c.scope.temp).unwrap_or(0),
        error_handler: UnauthorizedSeeAS(&rs),
    };

    let leds_handler = WithPermissions {
        handler: coap_handler_implementations::TypeHandler::new_minicbor_0_24(Leds(leds)),
        permissions: claims.map(|c| c.scope.leds).unwrap_or(0),
        error_handler: UnauthorizedSeeAS(&rs),
    };

    // Why isn't TypeHandler Reporting?
    let time_handler = coap_handler_implementations::wkc::ConstantSingleRecordReport::new(
        time_handler,
        &[coap_handler::Attribute::Ct(60)],
    );
    let temperature_handler = coap_handler_implementations::wkc::ConstantSingleRecordReport::new(
        temperature_handler,
        &[coap_handler::Attribute::Ct(60)],
    );
    let leds_handler = coap_handler_implementations::wkc::ConstantSingleRecordReport::new(
        leds_handler,
        &[coap_handler::Attribute::Ct(60)],
    );
    let identify_handler =
        coap_handler_implementations::wkc::ConstantSingleRecordReport::new(identify_handler, &[]);

    coap_handler_implementations::new_dispatcher()
        // Fully unprotected in the demo only
        .at(&["time"], time_handler)
        // FIXME: Go through OSCORE
        .at(&["leds"], leds_handler)
        .at(&["temp"], temperature_handler)
        .at(&["identify"], identify_handler)
        .with_wkc()
}
