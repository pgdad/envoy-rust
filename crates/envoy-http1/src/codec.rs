//! HTTP/1.1 request codec — a thin wrapper over `httparse::Request::parse`.

use crate::error::Http1Error;

/// Maximum size of the request headers section, in bytes.
/// Matches phase-02.2's admin tightening (per phase-01 REVIEW I4).
const HEADERS_CAP: usize = 8192;

/// Maximum number of header rows in a single request. Matches httparse's
/// default; sized so an attacker cannot flood the headers vec.
const MAX_HEADERS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpVersion {
    Http10,
    Http11,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Request {
    /// Method as parsed (case preserved). HTTP/1.1 §3.1.1: methods are
    /// case-sensitive — but the HCM in 04.x does not branch on method,
    /// so case-preservation here is just for forward-compat / logging.
    pub method: String,

    /// Request-target as raw bytes. The HCM matches `prefix:` / `path:`
    /// against this byte-for-byte (no normalization).
    pub path: String,

    pub version: HttpVersion,

    /// Header rows in emission order. Names are case-preserved as written
    /// (per the case-preserving storage discipline); use `find_header`
    /// for case-insensitive lookup.
    pub headers: Vec<(String, String)>,

    /// Number of bytes consumed from the input buffer to produce this
    /// request (= the offset to the start of the body, if any).
    pub bytes_consumed: usize,

    /// 04.3 NEW: outgoing request body bytes (for the router-proxy arm in
    /// Task 9 to populate before calling `Client::send_request`). The codec's
    /// `parse_request` (incoming-side) sets this to `None`; only the outgoing-
    /// side caller fills it. `None` is treated as `Bytes::new()` (Content-Length: 0).
    pub body: Option<bytes::Bytes>,
}

impl Request {
    /// 04.3 NEW: byte-length of the outgoing body, for `Content-Length:` and
    /// for the request-wire byte budget pre-allocation. Treats `None` as 0.
    #[allow(dead_code)] // wired up by Task 9's router-proxy arm; used here by send_request
    pub fn body_len_estimate(&self) -> usize {
        self.body.as_ref().map(|b| b.len()).unwrap_or(0)
    }

    #[allow(dead_code)] // wired up by Task 9's router-proxy arm; used here by send_request
    pub(crate) fn body_len_string(&self) -> String {
        self.body_len_estimate().to_string()
    }

    #[allow(dead_code)] // wired up by Task 9's router-proxy arm; used here by send_request
    pub(crate) fn body_bytes(&self) -> Option<&[u8]> {
        self.body.as_ref().map(|b| b.as_ref())
    }
}

pub struct Http1Codec;

impl Http1Codec {
    /// Attempt to parse a single HTTP/1.1 request from `buf`. Returns:
    /// - `Ok(Some(req))` on a fully-parsed request;
    /// - `Ok(None)` if `buf` does not yet contain a complete request
    ///   (caller reads more bytes and retries);
    /// - `Err(Http1Error::HeadersTooLarge)` if the buffer is past the
    ///   headers cap without httparse signaling `Complete`;
    /// - `Err(Http1Error::MalformedRequestLine)` / `MalformedHeader` on
    ///   malformed input.
    pub fn parse_request(buf: &[u8]) -> Result<Option<Request>, Http1Error> {
        let mut headers_storage = [httparse::EMPTY_HEADER; MAX_HEADERS];
        let mut parsed = httparse::Request::new(&mut headers_storage);

        let bytes_consumed = match parsed.parse(buf) {
            Ok(httparse::Status::Complete(n)) => n,
            Ok(httparse::Status::Partial) => {
                if buf.len() > HEADERS_CAP {
                    return Err(Http1Error::HeadersTooLarge { cap: HEADERS_CAP });
                }
                return Ok(None);
            }
            Err(httparse::Error::TooManyHeaders) => {
                return Err(Http1Error::HeadersTooLarge { cap: HEADERS_CAP });
            }
            Err(httparse::Error::HeaderName)
            | Err(httparse::Error::HeaderValue)
            | Err(httparse::Error::NewLine)
            | Err(httparse::Error::Status) => {
                return Err(Http1Error::MalformedHeader);
            }
            Err(httparse::Error::Token) | Err(httparse::Error::Version) => {
                return Err(Http1Error::MalformedRequestLine);
            }
        };

        // httparse guarantees method/path/version are Some on Complete.
        let method = parsed.method.unwrap_or("").to_string();
        let path = parsed.path.unwrap_or("").to_string();
        let version = match parsed.version {
            Some(0) => HttpVersion::Http10,
            Some(1) => HttpVersion::Http11,
            _ => return Err(Http1Error::MalformedRequestLine),
        };

        // Convert borrowed httparse headers into owned String pairs. The
        // filtered iterator's size hint is lossy, so reserve exactly once
        // up front instead of letting `collect` grow the Vec.
        let mut headers: Vec<(String, String)> = Vec::with_capacity(parsed.headers.len());
        for h in parsed.headers.iter() {
            if h.name.is_empty() {
                continue;
            }
            let name = h.name.to_string();
            let value = std::str::from_utf8(h.value)
                .map(str::to_string)
                .unwrap_or_default();
            headers.push((name, value));
        }

        Ok(Some(Request {
            method,
            path,
            version,
            headers,
            bytes_consumed,
            body: None, // 04.3 NEW
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_get_root_with_host() {
        let buf = b"GET /healthz HTTP/1.1\r\nHost: x\r\n\r\n";
        let req = Http1Codec::parse_request(buf)
            .expect("ok")
            .expect("complete");
        assert_eq!(req.method, "GET");
        assert_eq!(req.path, "/healthz");
        assert_eq!(req.version, HttpVersion::Http11);
        assert_eq!(req.bytes_consumed, buf.len());
        assert_eq!(req.headers.len(), 1);
        assert_eq!(req.headers[0].0, "Host");
        assert_eq!(req.headers[0].1, "x");
    }

    #[test]
    fn returns_none_on_partial_request_line() {
        let buf = b"GET /healthz HTTP/";
        assert_eq!(Http1Codec::parse_request(buf).expect("no err"), None);
    }

    #[test]
    fn returns_err_on_malformed_request_line() {
        // Missing path/version after method — httparse returns Err::Token or Err::NewLine.
        let buf = b"GET\r\n\r\n";
        let err = Http1Codec::parse_request(buf).expect_err("malformed");
        // Either MalformedRequestLine (Token/Version) or MalformedHeader (NewLine)
        // is acceptable here — the failure-mode taxonomy isn't load-bearing for
        // the test, only that we reject.
        assert!(
            matches!(
                err,
                Http1Error::MalformedRequestLine | Http1Error::MalformedHeader
            ),
            "got: {:?}",
            err
        );
    }

    #[test]
    fn enforces_headers_cap() {
        // 9 KiB of headers ensures httparse returns Partial on a buffer past
        // the 8 KiB cap; the codec then returns HeadersTooLarge.
        let mut buf = b"GET / HTTP/1.1\r\nHost: x\r\n".to_vec();
        for i in 0..200 {
            buf.extend_from_slice(format!("X-Pad-{i}: {}\r\n", "a".repeat(40)).as_bytes());
        }
        // No trailing CRLF, so httparse keeps returning Partial.
        let err = Http1Codec::parse_request(&buf).expect_err("too large");
        assert!(
            matches!(err, Http1Error::HeadersTooLarge { cap: 8192 }),
            "got: {:?}",
            err
        );
    }

    #[test]
    fn preserves_header_emission_order_and_case() {
        let buf = b"GET / HTTP/1.1\r\nHost: x\r\nX-Foo: 1\r\nX-Bar: 2\r\nX-Foo: 3\r\n\r\n";
        let req = Http1Codec::parse_request(buf)
            .expect("ok")
            .expect("complete");
        let names: Vec<&str> = req.headers.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["Host", "X-Foo", "X-Bar", "X-Foo"]);
        let values: Vec<&str> = req.headers.iter().map(|(_, v)| v.as_str()).collect();
        assert_eq!(values, vec!["x", "1", "2", "3"]);
    }
}
