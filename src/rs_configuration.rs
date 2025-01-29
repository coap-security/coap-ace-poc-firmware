// SPDX-FileCopyrightText: Copyright 2022-2024 EDF (Électricité de France S.A.)
// SPDX-License-Identifier: BSD-3-Clause
// See README for all details on copyright, authorship and license.
/// The PoC's roles.
///
/// This contains the distilled version of a token's claims that are relevant to (i.e. are being
/// evaluated by) the application.
///
/// It does not encode details such as the audience or the issuer, but does still represent them:
/// They were checked before construction.
///
/// It also does not encode the technical details on how the peer identifies in the security
/// protocol: These are stored inside the RS's token pool, and already processed there.
#[derive(defmt::Format)]
pub struct ApplicationClaims {
    pub scope: Permissions,
    pub exp: u32,
}

impl ApplicationClaims {
    pub fn valid(&self) -> bool {
        let now = crate::devicetime::unixtime();
        if let Ok(now) = now {
            if self.exp >= now {
                defmt::info!("Token is good for another {} seconds", self.exp - now);
                true
            } else {
                defmt::info!("Token has expired for {} seconds", now - self.exp);
                false
            }
        } else {
            // It's highly unlikely that the current time is inaccessible, but in that
            // case, let's assume it's expired
            false
        }
    }
}

/// The pre-parsed AIF.
///
/// Struct members correspond to URI-local-part Toid, values to REST-method-set Tperm.
///
/// Note that this is custom and manual; a better solution would be deriving this struct and the
/// match in its parsing function from a description of the CoAP tree.
#[derive(defmt::Format, Default)]
pub struct Permissions {
    /// Permissions on `/temp`
    pub temp: u8,
    /// Permissions on `/identify`
    pub identify: u8,
    /// Permissions on `/leds`
    pub leds: u8,
}

impl Permissions {
    fn parse(input: &[u8]) -> Result<Self, minicbor::decode::Error> {
        let mut decoder = minicbor::Decoder::new(input);
        let mut parsed = Self::default();
        for item in decoder.array_iter::<(&str, u8)>()? {
            let (path, perms) = item?;
            match path {
                "/temp" => {
                    parsed.temp = perms;
                }
                "/identify" => {
                    parsed.identify = perms;
                }
                "/leds" => {
                    parsed.leds = perms;
                }
                _ => (),
            }
        }
        Ok(parsed)
    }
}

/// Error type indicating that a token contains credentials not for us, and/or contains claims that
/// are not understood.
#[derive(defmt::Format)]
pub struct UnrecognizedCredentials;

impl<'a> TryFrom<&'a coset::cwt::ClaimsSet> for ApplicationClaims {
    type Error = UnrecognizedCredentials;

    /// Digest a claims set into the properties relevant to the application.
    ///
    /// Before calling this, it needs to be verified that the claims set was decrypted from (and
    /// claimed to be) the AS relevant to this system, and for the audience that represents this
    /// system. That is typically done right after decryption.
    fn try_from(claims: &coset::cwt::ClaimsSet) -> Result<Self, UnrecognizedCredentials> {
        // Verify that the token applies to us.

        let mut scope = None;
        let exp = match claims.expiration_time {
            Some(coset::cwt::Timestamp::WholeSeconds(n)) => n.try_into().ok(),
            _ => None,
        };
        for (key, value) in claims.rest.iter() {
            match (key, value) {
                (
                    coset::RegisteredLabelWithPrivate::Assigned(coset::iana::CwtClaimName::Scope),
                    ciborium::value::Value::Bytes(s),
                ) => {
                    // FIXME value goes back and forth between J/S and AIF
                    let new = Permissions::parse(s.as_slice()).map_err(|_e| {
                        // Not reporting value, see https://gitlab.com/twittner/minicbor/-/issues/41
                        defmt::info!("Unparsable scope claim, rejecting.");
                        UnrecognizedCredentials
                    })?;
                    if scope.replace(new).is_some() {
                        // Double key
                        defmt::info!("Duplicate scope claim, rejecting.");
                        return Err(UnrecognizedCredentials);
                    }
                }
                _ => (),
            }
        }

        let Some(scope) = scope else {
            defmt::info!("No scope set, rejecting.");
            return Err(UnrecognizedCredentials);
        };

        let Some(exp) = exp else {
            // Let's not even get started with infinite credentials
            defmt::info!("No expiry set, rejecting.");
            return Err(UnrecognizedCredentials);
        };

        let appclaims = ApplicationClaims { scope, exp };

        if !appclaims.valid() {
            defmt::info!("Token recognized, but validity test failed.");
            return Err(UnrecognizedCredentials);
        }

        defmt::info!("Token accepted: {:?}", appclaims);
        Ok(appclaims)
    }
}
