//! CoAP handlers for the demo application
//!
//! This modules's main entry point is [create_coap_handler], which produces a full handler with
//! the resources `/time`, `/authz-info`, `/leds`, `/temp` and `/identify`, all backed by structs
//! of this module.

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
        crate::devicetime::unixtime()
            .map_err(|_| coap_numbers::code::INTERNAL_SERVER_ERROR)
    }

    fn put(&mut self, representation: &Self::Put) -> u8 {
        crate::devicetime::set_unixtime(*representation);
        coap_numbers::code::CHANGED
    }
}

/// Resource handler representing the ACE-OSCORE profile (RFC9203) `/authz-info` endpoint,
/// accepting tokens together with id1 and nonce1 values.
struct AuthzInfo;

impl coap_handler::Handler for AuthzInfo {
    // FIXME: actual response
    type RequestData = u8;

    fn extract_request_data(&mut self, request: &impl coap_message::ReadableMessage) -> u8 {
        defmt::info!("Request to authz-info");

        use coap_numbers::code::*;
        use coap_handler_implementations::option_processing::OptionsExt;
        if request.code().into() != POST {
            return METHOD_NOT_ALLOWED;
        }
        if request.options().ignore_elective_others().is_err() {
            return BAD_OPTION;
        }

        // FIXME: also id1, nonce1 in a structure
        let access_token = request.payload();

        // FIXME: *This* is where we need alloc

        use coset::TaggedCborSerializable;
        let envelope = match coset::CoseEncrypt0::from_tagged_slice(&access_token) {
            Ok(e) => e,
            _ => {
                defmt::error!("Request contained undecodable envelope");
                return BAD_REQUEST;
            }
        };
        let iv = envelope.unprotected.iv;
        use ace_oscore_helpers::{aead, aes};
        // FIXME: Defer to the RS's AS associations
        let mut cipher: ace_oscore_helpers::aesccm::RustCryptoCcmCoseCipher::<aes::Aes256,  aead::generic_array::typenum::U16, aead::generic_array::typenum::U13> = ace_oscore_helpers::aesccm::RustCryptoCcmCoseCipher::new(
            aead::generic_array::arr![u8; 'a' as u8, 'b' as u8, 'c' as u8, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32],
            *aead::generic_array::GenericArray::from_slice(&iv),
            );
        let claims = match dcaf::decrypt_access_token(&access_token[1..].to_vec(), &mut cipher, Some(&[])) {
            Ok(c) => c,
            _ => {
                defmt::error!("Request contained undecodable envelope");
                return BAD_REQUEST;
            }
        };
        let application_claims = crate::rs_configuration::ApplicationClaims::process_token(&claims);
        let oscore_material = match ace_oscore_helpers::oscore_claims::extract_oscore(claims) {
            Ok(o) => o,
            _ => {
                defmt::error!("No usable OSCORE material found");
                // FIXME check error code
                return BAD_REQUEST;
            }
        };
        defmt::info!("{:?}", application_claims);
        defmt::info!("{:?}", oscore_material.ms.as_ref().map(|m| m.as_slice()));

        CHANGED
    }
    fn estimate_length(&mut self, _: &u8) -> usize {
        1
    }
    fn build_response(&mut self, response: &mut impl coap_message::MutableWritableMessage, code: u8) {
        response.set_code(code.try_into().map_err(|_| ()).unwrap());
        response.set_payload(b"");
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
    fn encode<W: minicbor::encode::Write>(&self, e: &mut minicbor::Encoder<W>) -> Result<(), minicbor::encode::Error<W::Error>> {
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
        // Note that this blocks for 50ms according to the docs. If softdevice let us use it as
        // normal in embassy_nrf, we might handle that smarter. (Although coap-handler is not
        // helpful there yet anyway).
        Ok(BigfloatFixedI32(nrf_softdevice::temperature_celsius(self.softdevice)
           .map_err(|_| coap_numbers::code::INTERNAL_SERVER_ERROR)?))
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
        use coap_numbers::code::*;
        use coap_handler_implementations::option_processing::OptionsExt;
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
    fn build_response(&mut self, response: &mut impl coap_message::MutableWritableMessage, code: u8) {
        response.set_code(code.try_into().map_err(|_| ()).unwrap());
        response.set_payload(b"");
    }
}

/// Create a tree of CoAP resource as described in this module's documentation out of the
/// individual handler implementations in this module.
///
/// The tree also features a `/.well-known/core` resource listing the other resources.
pub fn create_coap_handler(
        claims: Option<crate::rs_configuration::ApplicationClaims>,
        softdevice: &'static nrf_softdevice::Softdevice,
        leds: &'static crate::blink::Leds,
    ) -> CoapHandler
{
    use coap_handler_implementations::ReportingHandlerBuilder;
    use coap_handler_implementations::HandlerBuilder;

    // Going through SimpleWrapper is not particularly slim on message sizes, given it adds ETag
    // and Block2 unconditionally, but that could be fixed there on the long run (with a somewhat
    // improved MutableWritableMessage, or better bounds on CBOR serialization size)
    let time_handler = coap_handler_implementations::SimpleWrapper::new_minicbor(Time);

    let authzinfo_handler = coap_handler_implementations::wkc::ConstantSingleRecordReport::new(AuthzInfo, &[coap_handler_implementations::wkc::Attribute::ResourceType("ace.ai")]);

    let temperature_handler = coap_handler_implementations::SimpleWrapper::new_minicbor(Temperature { softdevice });
    let leds_handler = coap_handler_implementations::SimpleWrapper::new_minicbor(Leds(leds));
    let identify_handler = coap_handler_implementations::wkc::ConstantSingleRecordReport::new(Identify(leds), &[]);

    // Why isn't SimpleWrapper Reporting?
    let time_handler = coap_handler_implementations::wkc::ConstantSingleRecordReport::new(time_handler, &[coap_handler_implementations::wkc::Attribute::Ct(60)]);
    let temperature_handler = coap_handler_implementations::wkc::ConstantSingleRecordReport::new(temperature_handler, &[coap_handler_implementations::wkc::Attribute::Ct(60)]);
    let leds_handler = coap_handler_implementations::wkc::ConstantSingleRecordReport::new(leds_handler, &[coap_handler_implementations::wkc::Attribute::Ct(60)]);

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
