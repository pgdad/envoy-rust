//! Inline JWKS (RSA-only) parsing. A JWKS is `{"keys":[{kty,kid,n,e,...}]}`;
//! we keep only `kty == "RSA"` keys, base64url-decode `n`/`e`, and strip any
//! leading 0x00 byte (aws-lc-rs's PublicKeyComponents rejects leading zeros —
//! §6.2 L1).

use serde::Deserialize;

use crate::base64url;
use crate::error::JwtError;

/// One RSA public key from a JWKS, with `n`/`e` as big-endian bytes (no leading
/// zeros).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RsaKey {
    pub kid: Option<String>,
    pub n: Vec<u8>,
    pub e: Vec<u8>,
}

/// A parsed set of RSA public keys built from an inline JWKS JSON string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JwkSet {
    keys: Vec<RsaKey>,
}

#[derive(Deserialize)]
struct RawJwks {
    keys: Vec<RawJwk>,
}

#[derive(Deserialize)]
struct RawJwk {
    kty: String,
    #[serde(default)]
    kid: Option<String>,
    #[serde(default)]
    n: Option<String>,
    #[serde(default)]
    e: Option<String>,
}

fn strip_leading_zeros(mut v: Vec<u8>) -> Vec<u8> {
    let first = v.iter().position(|&b| b != 0).unwrap_or(v.len());
    v.drain(..first);
    v
}

impl JwkSet {
    /// Parse an inline JWKS JSON string. Errors (`JwtError::InvalidJwks`) on
    /// non-JSON, missing `keys`, an RSA key missing `n`/`e` or with
    /// undecodable `n`/`e`, or an empty resulting RSA key set.
    pub fn parse(jwks_json: &str) -> Result<Self, JwtError> {
        let raw: RawJwks = serde_json::from_str(jwks_json).map_err(|_| JwtError::InvalidJwks)?;
        let mut keys = Vec::new();
        for k in raw.keys {
            if k.kty != "RSA" {
                continue; // ES/oct/etc. defer (§4); skip silently
            }
            let n = base64url::decode(k.n.as_deref().ok_or(JwtError::InvalidJwks)?)
                .map_err(|_| JwtError::InvalidJwks)?;
            let e = base64url::decode(k.e.as_deref().ok_or(JwtError::InvalidJwks)?)
                .map_err(|_| JwtError::InvalidJwks)?;
            keys.push(RsaKey {
                kid: k.kid,
                n: strip_leading_zeros(n),
                e: strip_leading_zeros(e),
            });
        }
        if keys.is_empty() {
            return Err(JwtError::InvalidJwks);
        }
        Ok(JwkSet { keys })
    }

    pub fn keys(&self) -> &[RsaKey] {
        &self.keys
    }
}

#[cfg(test)]
mod tests {
    use super::JwkSet;
    use crate::JwtError;

    // A minimal valid RSA JWKS (n/e are real base64url — small but well-formed JSON).
    // "n" uses "sXche4iX" (8 valid base64url chars, no dots); "e" is "AQAB" => [1,0,1]
    const JWKS: &str = r#"{"keys":[{"kty":"RSA","kid":"k1","use":"sig","alg":"RS256",
        "n":"sXche4iX","e":"AQAB"}]}"#;

    #[test]
    fn parses_rsa_key() {
        let set = JwkSet::parse(JWKS).expect("valid jwks");
        assert_eq!(set.keys().len(), 1);
        assert_eq!(set.keys()[0].kid.as_deref(), Some("k1"));
        assert_eq!(set.keys()[0].e, vec![0x01, 0x00, 0x01]); // "AQAB" => 65537, leading zero already absent
    }

    #[test]
    fn rejects_non_json() {
        assert_eq!(
            JwkSet::parse("not json").unwrap_err(),
            JwtError::InvalidJwks
        );
    }

    #[test]
    fn rejects_empty_keyset() {
        assert_eq!(
            JwkSet::parse(r#"{"keys":[]}"#).unwrap_err(),
            JwtError::InvalidJwks
        );
    }

    #[test]
    fn skips_non_rsa_keys_but_errors_if_none_remain() {
        let only_oct = r#"{"keys":[{"kty":"oct","k":"AAAA"}]}"#;
        assert_eq!(JwkSet::parse(only_oct).unwrap_err(), JwtError::InvalidJwks);
    }
}
