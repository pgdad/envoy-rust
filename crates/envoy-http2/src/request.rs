//! H2 → envoy-Request value translator. See SPEC §3 D3.
//!
//! The adapter consumes an `http::Request<B>` (where `B` is the body type —
//! typically `h2::RecvStream` post-drain into `bytes::Bytes` for the runtime
//! consumer; arbitrary body types for unit tests) and emits an
//! `envoy_http1::codec::Request` value-type. Pseudo-headers map per parent-05
//! SPEC §6 signpost 12:
//!   - `:method` → `Request.method`
//!   - `:path`   → `Request.path` (raw string; query string preserved if present)
//!   - `:authority` → synthesized as `Host: <authority>` row at the bottom of
//!     `Request.headers` (per cross-sub-phase architectural rule 3,
//!     required for the existing 04.x route-walk)
//!   - `:scheme` → ignored (envoy-rust's HCM doesn't dispatch on scheme)

use bytes::Bytes;
use envoy_http1::codec::{HttpVersion, Request};
use http::Request as HttpRequest;

use crate::error::Http2Error;

/// Translate an H2 request (post-body-drain into `Bytes`) into an
/// `envoy_http1::codec::Request` value type. Pseudo-headers are unpacked per
/// the SPEC §6 signpost 12 mapping.
pub fn http_to_envoy_request(req: HttpRequest<Bytes>) -> Result<Request, Http2Error> {
    let (parts, body) = req.into_parts();

    // :method → method (raw string preservation; the envoy_http1::codec::Request
    // carries the method as a String, matching the H1 codec's posture).
    let method = parts.method.as_str().to_string();

    // :path → path. h2 exposes the path through `parts.uri.path_and_query()`;
    // for absolute URIs (http://authority/path) the path component is just
    // `/path`. For path-only URIs it's the same. Preserve the query if present.
    // Defensive default: real H2 always carries `:path`, but if the Uri lacks
    // one, route-walk needs a non-empty path (else `prefix:` matchers would
    // match every rule). "/" is the safe canonical default.
    let path = parts
        .uri
        .path_and_query()
        .map(|pq| pq.as_str().to_string())
        .unwrap_or_else(|| "/".to_string());

    // :authority → Host: row. h2 exposes :authority via `parts.uri.authority()`
    // OR via the `Host:` header (depending on h2-version + handshake details).
    // Prefer authority(); fall back to Host header if present.
    let authority_str: Option<String> = parts
        .uri
        .authority()
        .map(|a| a.as_str().to_string())
        .or_else(|| {
            parts
                .headers
                .get(http::header::HOST)
                .and_then(|hv| hv.to_str().ok())
                .map(str::to_string)
        })
        .filter(|s| !s.is_empty());

    let authority = authority_str.ok_or(Http2Error::MissingAuthority)?;

    // Translate regular headers. h2 delivers names lowercased; preserve as-is.
    // Skip the Host header here (we'll re-add the synthesized one at the bottom).
    let mut headers: Vec<(String, String)> = Vec::with_capacity(parts.headers.len() + 1);
    for (name, value) in parts.headers.iter() {
        if name.as_str().eq_ignore_ascii_case("host") {
            continue;
        }
        let value_str = value
            .to_str()
            .map_err(|_| Http2Error::MalformedH2HeaderBlock)?
            .to_string();
        headers.push((name.as_str().to_string(), value_str));
    }
    headers.push(("host".to_string(), authority));

    Ok(Request {
        method,
        path,
        version: HttpVersion::Http11, // route-walk treats this as H1.1; H2 framing is at the codec edge.
        headers,
        bytes_consumed: 0,
        body: Some(body),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use http::{Method, Request as HttpRequest, Uri};

    /// Build an `http::Request` with the given pseudo-header values + extra
    /// headers, with a body of given bytes. Used by the tests below.
    fn build_request(
        method: &str,
        uri: &str,
        authority: Option<&str>,
        extras: &[(&str, &str)],
        body: Bytes,
    ) -> HttpRequest<Bytes> {
        let mut builder = HttpRequest::builder()
            .method(Method::from_bytes(method.as_bytes()).unwrap())
            .uri(uri.parse::<Uri>().unwrap());
        for (n, v) in extras {
            builder = builder.header(*n, *v);
        }
        let mut req = builder.body(body).unwrap();
        if let Some(a) = authority {
            req.headers_mut()
                .insert(http::header::HOST, a.parse().unwrap());
            // Note: in real H2, :authority is exposed via `request.uri().authority()`
            // when the Uri is in absolute form. Set the Uri appropriately instead:
            *req.uri_mut() = format!("http://{a}{uri}").parse().unwrap();
        }
        req
    }

    #[test]
    fn http_to_envoy_request_lowercases_headers() {
        let req = build_request(
            "GET",
            "/",
            Some("test.example"),
            &[("User-Agent", "testharness"), ("X-Foo", "bar")],
            Bytes::new(),
        );
        let out = http_to_envoy_request(req).expect("translates");
        // h2 lowercases header names on receive; verify our adapter preserves
        // (and that the value is unchanged).
        let names: Vec<&str> = out.headers.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.iter().any(|n| n.eq_ignore_ascii_case("user-agent")));
        assert!(names.iter().any(|n| n.eq_ignore_ascii_case("x-foo")));
        let ua = out
            .headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case("user-agent"))
            .unwrap();
        assert_eq!(ua.1, "testharness");
    }

    #[test]
    fn http_to_envoy_request_synthesizes_host_from_authority() {
        let req = build_request("GET", "/", Some("test.example"), &[], Bytes::new());
        let out = http_to_envoy_request(req).expect("translates");
        let host = out
            .headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case("host"))
            .expect("Host header must be synthesized from :authority");
        assert_eq!(host.1, "test.example");
    }

    #[test]
    fn http_to_envoy_request_missing_authority_returns_error() {
        // No URI authority + no Host header → MissingAuthority.
        let req = HttpRequest::builder()
            .method(Method::GET)
            .uri("/path".parse::<Uri>().unwrap())
            .body(Bytes::new())
            .unwrap();
        let err = http_to_envoy_request(req).expect_err("must fail without authority");
        assert!(matches!(err, Http2Error::MissingAuthority), "got {err:?}");
    }

    #[test]
    fn http_to_envoy_request_non_utf8_header_value_returns_error() {
        // A non-UTF-8 header value should raise MalformedH2HeaderBlock.
        let req = HttpRequest::builder()
            .method(Method::GET)
            .uri("http://test.example/".parse::<Uri>().unwrap())
            .header(
                "x-binary",
                http::HeaderValue::from_bytes(&[0xFF, 0xFE]).unwrap(),
            )
            .body(Bytes::new())
            .unwrap();
        let err = http_to_envoy_request(req).expect_err("must fail on non-UTF-8");
        assert!(
            matches!(err, Http2Error::MalformedH2HeaderBlock),
            "got {err:?}"
        );
    }
}
