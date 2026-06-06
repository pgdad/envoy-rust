#![no_main]
#![forbid(unsafe_code)]

use libfuzzer_sys::fuzz_target;

// Split the input on the first NUL: bytes before = JWKS JSON, after = a token.
// Both surfaces (JwkSet::parse and verify_rs256) must never panic — only return
// a JwtError on malformed input.
fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else { return };
    let (jwks_str, token) = match s.split_once('\u{0}') {
        Some((a, b)) => (a, b),
        None => (s, ""),
    };
    if let Ok(set) = envoy_jwt::JwkSet::parse(jwks_str) {
        let _ = envoy_jwt::verify_rs256(token, &set, "iss", &[], 0);
    }
});
