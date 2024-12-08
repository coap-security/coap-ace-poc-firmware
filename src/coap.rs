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

/// Create a tree of CoAP resource as described in this module's documentation out of the
/// individual handler implementations in this module.
///
/// The tree also features a `/.well-known/core` resource listing the other resources.
pub fn create_coap_handler(
    softdevice: &'static nrf_softdevice::Softdevice,
    leds: &'static crate::blink::Leds,
) -> CoapHandler {
    use coap_handler_implementations::HandlerBuilder;
    use coap_handler_implementations::ReportingHandlerBuilder;

    // Going through TypeHandler is not particularly slim on message sizes, given it adds ETag
    // and Block2 unconditionally, but that could be fixed there on the long run (with a somewhat
    // improved MutableWritableMessage, or better bounds on CBOR serialization size)
    let time_handler = coap_handler_implementations::TypeHandler::new_minicbor(Time);

    let identify_handler = Identify(leds);

    let temperature_handler =
        coap_handler_implementations::TypeHandler::new_minicbor_0_24(Temperature { softdevice });

    let leds_handler = coap_handler_implementations::TypeHandler::new_minicbor_0_24(Leds(leds));

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
        .at(&["leds"], leds_handler)
        .at(&["temp"], temperature_handler)
        .at(&["identify"], identify_handler)
        .with_wkc()
}
