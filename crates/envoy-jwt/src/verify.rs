//! RS256 verification + registered-claim validation. Time is INJECTED
//! (`now_unix`) so callers control the clock (runtime passes `SystemTime::now`;
//! tests pass fixed values; the differential fixture uses far-future/past `exp`
//! so the clock is cross-proxy-irrelevant). Pure compute, no I/O, no async.

use aws_lc_rs::rsa::PublicKeyComponents;
use aws_lc_rs::signature::RSA_PKCS1_2048_8192_SHA256;
use serde::Deserialize;

use crate::base64url;
use crate::error::JwtError;
use crate::jwks::JwkSet;

/// The verified + validated JWT a caller may inspect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedJwt {
    /// Always `Some` on the success path (a `None` issuer is rejected by `verify_rs256`).
    pub iss: Option<String>,
    pub aud: Vec<String>,
    pub exp: Option<i64>,
    pub nbf: Option<i64>,
}

#[derive(Deserialize)]
struct Header {
    alg: String,
    #[serde(default)]
    kid: Option<String>,
}

#[derive(Deserialize)]
struct Claims {
    #[serde(default)]
    iss: Option<String>,
    #[serde(default)]
    aud: Option<Aud>,
    #[serde(default)]
    exp: Option<i64>,
    #[serde(default)]
    nbf: Option<i64>,
}

/// `aud` is `string | string[]` per JWT (§6.2 L7).
#[derive(Deserialize)]
#[serde(untagged)]
enum Aud {
    One(String),
    Many(Vec<String>),
}

impl Aud {
    fn into_vec(self) -> Vec<String> {
        match self {
            Aud::One(s) => vec![s],
            Aud::Many(v) => v,
        }
    }
}

/// Verify an RS256 JWT against `jwks` and validate `iss`/`aud`/`exp`/`nbf`.
/// `allowed_audiences` empty ⇒ the audience check is skipped (§6.2 L7).
pub fn verify_rs256(
    token: &str,
    jwks: &JwkSet,
    expected_issuer: &str,
    allowed_audiences: &[String],
    now_unix: i64,
) -> Result<VerifiedJwt, JwtError> {
    // 1. Exactly 3 non-empty segments.
    let mut it = token.split('.');
    let (h, p, s) = match (it.next(), it.next(), it.next(), it.next()) {
        (Some(h), Some(p), Some(s), None) if !h.is_empty() && !p.is_empty() && !s.is_empty() => {
            (h, p, s)
        }
        _ => return Err(JwtError::NotInForm),
    };
    // 2. Decode + parse header.
    let header_bytes = base64url::decode(h).map_err(|_| JwtError::NotInForm)?;
    let header: Header =
        serde_json::from_slice(&header_bytes).map_err(|_| JwtError::BadHeaderJson)?;
    // 3. Decode + parse payload.
    let payload_bytes = base64url::decode(p).map_err(|_| JwtError::NotInForm)?;
    let claims: Claims =
        serde_json::from_slice(&payload_bytes).map_err(|_| JwtError::BadPayloadJson)?;
    // 4. Decode signature.
    let sig = base64url::decode(s).map_err(|_| JwtError::NotInForm)?;
    // 5. alg must be RS256 (else NoMatchingKey — Envoy folds unsupported alg here).
    if header.alg != "RS256" {
        return Err(JwtError::NoMatchingKey);
    }
    // 6. Candidate keys: by kid if the header names one, else all keys.
    let candidates: Vec<&crate::jwks::RsaKey> = match &header.kid {
        Some(kid) => jwks
            .keys()
            .iter()
            .filter(|k| k.kid.as_deref() == Some(kid))
            .collect(),
        None => jwks.keys().iter().collect(),
    };
    if candidates.is_empty() {
        return Err(JwtError::NoMatchingKey);
    }
    // 7. Verify the signature over the `header.payload` signing input.
    let signing_input = &token.as_bytes()[..h.len() + 1 + p.len()];
    let verified = candidates.iter().any(|k| {
        PublicKeyComponents {
            n: k.n.as_slice(),
            e: k.e.as_slice(),
        }
        .verify(&RSA_PKCS1_2048_8192_SHA256, signing_input, &sig)
        .is_ok()
    });
    if !verified {
        return Err(JwtError::VerificationFails);
    }
    // 8. Issuer.
    if claims.iss.as_deref() != Some(expected_issuer) {
        return Err(JwtError::IssuerMismatch);
    }
    // 9. exp / nbf (injected clock).
    if let Some(exp) = claims.exp
        && now_unix >= exp
    {
        return Err(JwtError::Expired);
    }
    if let Some(nbf) = claims.nbf
        && now_unix < nbf
    {
        return Err(JwtError::NotYetValid);
    }
    // 10. Audience.
    let aud: Vec<String> = claims.aud.map(Aud::into_vec).unwrap_or_default();
    if !allowed_audiences.is_empty() {
        let ok = aud.iter().any(|a| allowed_audiences.iter().any(|x| x == a));
        if !ok {
            return Err(JwtError::AudienceNotAllowed);
        }
    }
    Ok(VerifiedJwt {
        iss: claims.iss,
        aud,
        exp: claims.exp,
        nbf: claims.nbf,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::JwkSet;
    use crate::test_support::{b64url, keypair, make_token, sign};
    use aws_lc_rs::rsa::PublicKeyComponents;
    use aws_lc_rs::signature::KeyPair;

    const ISS: &str = "testing@secure.istio.io";

    #[test]
    fn valid_token_verifies() {
        let (kp, jwks) = keypair();
        let set = JwkSet::parse(&jwks).unwrap();
        let tok = make_token(
            &kp,
            "RS256",
            r#"{"iss":"testing@secure.istio.io","aud":["a"],"exp":4102444800}"#,
        );
        let v = verify_rs256(&tok, &set, ISS, &["a".to_string()], 1_700_000_000).unwrap();
        assert_eq!(v.iss.as_deref(), Some(ISS));
    }

    #[test]
    fn tampered_signature_fails() {
        let (kp, jwks) = keypair();
        let set = JwkSet::parse(&jwks).unwrap();
        let mut tok = make_token(
            &kp,
            "RS256",
            r#"{"iss":"testing@secure.istio.io","exp":4102444800}"#,
        );
        // Corrupt the signature's FIRST base64url char (its value owns the top
        // 6 bits of signature byte 0, so a guaranteed-different replacement
        // always alters the decoded signature). NOT the last char: a 256-byte
        // RSA signature's final base64url char carries only 2 meaningful bits,
        // so replacing it can be a no-op under non-canonical-tolerant base64url
        // decoding (~1/4 of random keys) — which made the previous
        // `pop()`+`push('A'/'B')` tamper flaky.
        let sig_start = tok.rfind('.').unwrap() + 1;
        let repl = if tok.as_bytes()[sig_start] == b'A' {
            'B'
        } else {
            'A'
        };
        tok.replace_range(sig_start..sig_start + 1, &repl.to_string());
        assert_eq!(
            verify_rs256(&tok, &set, ISS, &[], 1_700_000_000).unwrap_err(),
            JwtError::VerificationFails
        );
    }

    #[test]
    fn expired_and_nbf_and_issuer_and_audience() {
        let (kp, jwks) = keypair();
        let set = JwkSet::parse(&jwks).unwrap();
        // expired
        let t = make_token(
            &kp,
            "RS256",
            r#"{"iss":"testing@secure.istio.io","exp":1500000000}"#,
        );
        assert_eq!(
            verify_rs256(&t, &set, ISS, &[], 1_700_000_000).unwrap_err(),
            JwtError::Expired
        );
        // not yet valid
        let t = make_token(
            &kp,
            "RS256",
            r#"{"iss":"testing@secure.istio.io","exp":4102444800,"nbf":4102444800}"#,
        );
        assert_eq!(
            verify_rs256(&t, &set, ISS, &[], 1_700_000_000).unwrap_err(),
            JwtError::NotYetValid
        );
        // issuer mismatch
        let t = make_token(&kp, "RS256", r#"{"iss":"wrong","exp":4102444800}"#);
        assert_eq!(
            verify_rs256(&t, &set, ISS, &[], 1_700_000_000).unwrap_err(),
            JwtError::IssuerMismatch
        );
        // audience not allowed
        let t = make_token(
            &kp,
            "RS256",
            r#"{"iss":"testing@secure.istio.io","aud":"x","exp":4102444800}"#,
        );
        assert_eq!(
            verify_rs256(&t, &set, ISS, &["y".to_string()], 1_700_000_000).unwrap_err(),
            JwtError::AudienceNotAllowed
        );
    }

    #[test]
    fn structural_and_alg_errors() {
        let (kp, jwks) = keypair();
        let set = JwkSet::parse(&jwks).unwrap();
        assert_eq!(
            verify_rs256("abc", &set, ISS, &[], 0).unwrap_err(),
            JwtError::NotInForm
        );
        assert_eq!(
            verify_rs256("a.b", &set, ISS, &[], 0).unwrap_err(),
            JwtError::NotInForm
        );
        // non-RS256 alg => NoMatchingKey (Envoy folds unsupported alg into key-match)
        let t = make_token(&kp, "HS256", r#"{"iss":"testing@secure.istio.io"}"#);
        assert_eq!(
            verify_rs256(&t, &set, ISS, &[], 1_700_000_000).unwrap_err(),
            JwtError::NoMatchingKey
        );
    }

    #[test]
    fn no_kid_header_matches_any_key() {
        let (kp, _jwks) = keypair();
        let hp = {
            let h = b64url(br#"{"alg":"RS256","typ":"JWT"}"#);
            let p = b64url(br#"{"iss":"testing@secure.istio.io","exp":4102444800}"#);
            format!("{h}.{p}")
        };
        let tok = format!("{}.{}", hp, sign(&kp, &hp));
        // JWKS key has NO kid → no-kid token must still match via the all-keys branch.
        let pk = kp.public_key();
        let comps: PublicKeyComponents<Vec<u8>> = PublicKeyComponents::from(pk);
        let jwks = format!(
            r#"{{"keys":[{{"kty":"RSA","n":"{}","e":"{}"}}]}}"#,
            b64url(&comps.n),
            b64url(&comps.e)
        );
        let set = JwkSet::parse(&jwks).unwrap();
        let v = verify_rs256(&tok, &set, ISS, &[], 1_700_000_000).unwrap();
        assert_eq!(v.iss.as_deref(), Some(ISS));
    }

    #[test]
    fn multi_aud_array_partial_match_allowed() {
        let (kp, jwks) = keypair();
        let set = JwkSet::parse(&jwks).unwrap();
        // token aud = ["a","b"]; allowed = ["b"] → intersection non-empty → OK
        let tok = make_token(
            &kp,
            "RS256",
            r#"{"iss":"testing@secure.istio.io","aud":["a","b"],"exp":4102444800}"#,
        );
        let v = verify_rs256(&tok, &set, ISS, &["b".to_string()], 1_700_000_000).unwrap();
        assert_eq!(v.aud, vec!["a".to_string(), "b".to_string()]);
        // token aud = ["a","b"]; allowed = ["c"] → no intersection → AudienceNotAllowed
        let tok2 = make_token(
            &kp,
            "RS256",
            r#"{"iss":"testing@secure.istio.io","aud":["a","b"],"exp":4102444800}"#,
        );
        assert_eq!(
            verify_rs256(&tok2, &set, ISS, &["c".to_string()], 1_700_000_000).unwrap_err(),
            JwtError::AudienceNotAllowed
        );
    }
}
