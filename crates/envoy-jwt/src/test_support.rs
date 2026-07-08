//! Shared RS256 test-token scaffolding: real keypair generation, PKCS1-SHA256
//! signing, unpadded base64url encoding, and `header.payload.signature` token
//! assembly. Compiled only for this crate's own unit tests (`cfg(test)`) or
//! when a downstream crate opts in via the `test-util` feature (the
//! `envoy-filter` jwt_authn tests previously carried a byte-for-byte copy of
//! these helpers). NOT part of the production API: the default (no-feature)
//! build does not include this module.

use aws_lc_rs::rsa::{KeySize, PublicKeyComponents};
use aws_lc_rs::signature::KeyPair;
pub use aws_lc_rs::signature::RsaKeyPair;

/// Tiny base64url (RFC 4648 §5, unpadded) encoder for tests only.
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

/// Build a real RSA-2048 keypair, return (keypair, jwks_json). The JWKS holds
/// the single public key under kid `"k1"`.
pub fn keypair() -> (RsaKeyPair, String) {
    let kp = RsaKeyPair::generate(KeySize::Rsa2048).expect("gen");
    let pk = kp.public_key();
    let comps: PublicKeyComponents<Vec<u8>> = PublicKeyComponents::from(pk); // n/e big-endian
    let jwks = format!(
        r#"{{"keys":[{{"kty":"RSA","kid":"k1","n":"{}","e":"{}"}}]}}"#,
        b64url(&comps.n),
        b64url(&comps.e)
    );
    (kp, jwks)
}

/// RS256-sign the `header.payload` signing input, return the base64url
/// signature segment.
pub fn sign(kp: &RsaKeyPair, header_payload: &str) -> String {
    let mut sig = vec![0u8; kp.public_modulus_len()];
    kp.sign(
        &aws_lc_rs::signature::RSA_PKCS1_SHA256,
        &aws_lc_rs::rand::SystemRandom::new(),
        header_payload.as_bytes(),
        &mut sig,
    )
    .expect("sign");
    b64url(&sig)
}

/// Assemble a signed `header.payload.signature` token whose header is
/// `{"alg":"<alg>","kid":"k1","typ":"JWT"}` (kid matches [`keypair`]'s JWKS).
pub fn make_token(kp: &RsaKeyPair, alg: &str, payload: &str) -> String {
    let h = format!(r#"{{"alg":"{alg}","kid":"k1","typ":"JWT"}}"#);
    let hp = format!("{}.{}", b64url(h.as_bytes()), b64url(payload.as_bytes()));
    let sig = sign(kp, &hp);
    format!("{hp}.{sig}")
}
