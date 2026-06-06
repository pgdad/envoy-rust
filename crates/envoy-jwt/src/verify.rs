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
    use aws_lc_rs::rsa::{KeySize, PublicKeyComponents};
    use aws_lc_rs::signature::{KeyPair, RsaKeyPair};

    // Build a real RSA-2048 keypair, return (keypair, jwks_json).
    fn keypair() -> (RsaKeyPair, String) {
        let kp = RsaKeyPair::generate(KeySize::Rsa2048).expect("gen");
        let pk = kp.public_key();
        let comps: PublicKeyComponents<Vec<u8>> = PublicKeyComponents::from(pk); // n/e big-endian
        let jwks = format!(
            r#"{{"keys":[{{"kty":"RSA","kid":"k1","n":"{}","e":"{}"}}]}}"#,
            base64_test::b64url(&comps.n),
            base64_test::b64url(&comps.e)
        );
        (kp, jwks)
    }

    fn sign(kp: &RsaKeyPair, header_payload: &str) -> String {
        let mut sig = vec![0u8; kp.public_modulus_len()];
        kp.sign(
            &aws_lc_rs::signature::RSA_PKCS1_SHA256,
            &aws_lc_rs::rand::SystemRandom::new(),
            header_payload.as_bytes(),
            &mut sig,
        )
        .expect("sign");
        base64_test::b64url(&sig)
    }

    // tiny base64url encoder for tests only
    mod base64_test {
        pub fn b64url(b: &[u8]) -> String {
            const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
            let mut out = String::new();
            for chunk in b.chunks(3) {
                let n = chunk.len();
                let b0 = chunk[0] as u32;
                let b1 = if n > 1 { chunk[1] as u32 } else { 0 };
                let b2 = if n > 2 { chunk[2] as u32 } else { 0 };
                let triple = (b0 << 16) | (b1 << 8) | b2;
                out.push(A[((triple >> 18) & 63) as usize] as char);
                out.push(A[((triple >> 12) & 63) as usize] as char);
                if n > 1 {
                    out.push(A[((triple >> 6) & 63) as usize] as char);
                }
                if n > 2 {
                    out.push(A[(triple & 63) as usize] as char);
                }
            }
            out
        }
        pub fn jwt(kid: &str, alg: &str, payload: &str) -> String {
            let h = format!(r#"{{"alg":"{alg}","kid":"{kid}","typ":"JWT"}}"#);
            format!("{}.{}", b64url(h.as_bytes()), b64url(payload.as_bytes()))
        }
    }

    fn make_token(kp: &RsaKeyPair, alg: &str, payload: &str) -> String {
        let hp = base64_test::jwt("k1", alg, payload);
        let sig = sign(kp, &hp);
        format!("{hp}.{sig}")
    }

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
        tok.pop();
        tok.push(if tok.ends_with('A') { 'B' } else { 'A' });
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
            let h = base64_test::b64url(br#"{"alg":"RS256","typ":"JWT"}"#);
            let p = base64_test::b64url(br#"{"iss":"testing@secure.istio.io","exp":4102444800}"#);
            format!("{h}.{p}")
        };
        let tok = format!("{}.{}", hp, sign(&kp, &hp));
        // JWKS key has NO kid → no-kid token must still match via the all-keys branch.
        let pk = kp.public_key();
        let comps: PublicKeyComponents<Vec<u8>> = PublicKeyComponents::from(pk);
        let jwks = format!(
            r#"{{"keys":[{{"kty":"RSA","n":"{}","e":"{}"}}]}}"#,
            base64_test::b64url(&comps.n),
            base64_test::b64url(&comps.e)
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
