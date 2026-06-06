//! Hand-rolled base64url (RFC 4648 §5, no padding) decoder. JWTs and JWKS use
//! unpadded URL-safe base64; we avoid a `base64` crate dependency (§5.2).

/// Decode an unpadded base64url string. Rejects `=` padding and any character
/// outside the URL-safe alphabet (`A-Za-z0-9-_`).
pub(crate) fn decode(s: &str) -> Result<Vec<u8>, ()> {
    fn val(c: u8) -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return None,
        } as u32)
    }
    let mut out = Vec::with_capacity(s.len() * 3 / 4 + 3);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;
    for &c in s.as_bytes() {
        buf = (buf << 6) | val(c).ok_or(())?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::decode;

    #[test]
    fn decodes_known_vectors() {
        // "foobar" base64url unpadded = "Zm9vYmFy"
        assert_eq!(decode("Zm9vYmFy").unwrap(), b"foobar");
        // empty
        assert_eq!(decode("").unwrap(), b"");
        // a JWT header {"alg":"RS256","typ":"JWT"} encodes without '=' padding
        let hdr = decode("eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9").unwrap();
        assert_eq!(&hdr, br#"{"alg":"RS256","typ":"JWT"}"#);
    }

    #[test]
    fn rejects_padding_and_non_alphabet() {
        assert!(decode("Zm9vYmFy=").is_err(), "padding rejected");
        assert!(decode("@@@").is_err(), "non-alphabet rejected");
        assert!(decode("a b").is_err(), "space rejected");
    }
}
