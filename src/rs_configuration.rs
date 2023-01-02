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
// FIXME switch to AIF
#[derive(defmt::Format)]
pub enum ApplicationClaims {
    Junior,
    Senior,
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
        for (key, value) in claims.rest.iter() {
            match (key, value) {
                (
                    coset::RegisteredLabelWithPrivate::Assigned(coset::iana::CwtClaimName::Scope),
                    ciborium::value::Value::Text(s),
                ) => {
                    // FIXME value
                    let new = match s.as_str() {
                        "r_temp" => ApplicationClaims::Junior,
                        _ => return Err(UnrecognizedCredentials),
                    };
                    if scope.replace(new).is_some() {
                        return Err(UnrecognizedCredentials);
                    }
                }
                _ => (),
            }
        }

        match scope {
            Some(s) => Ok(s),
            None => Err(UnrecognizedCredentials),
        }
    }
}
