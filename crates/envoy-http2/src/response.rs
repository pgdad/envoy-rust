//! envoy-Response → H2 SendStream emitter. See SPEC §3 D3.
//!
//! The response-translation surface is split into two pieces:
//!   - `build_http_response(resp)` — translates an `envoy_http1::Response` into
//!     an `http::Response<()>` (status + headers; body is sent separately).
//!     Pure function; testable in isolation.
//!   - `send_envoy_response(send_response, resp)` — drives the actual H2 wire
//!     emission via `h2::server::SendResponse::send_response` + body
//!     send_data. Async; integration-tested via the HCM tests in Task 9.
//!
//! H2-forbidden hop-by-hop headers (RFC 7540 §8.1.2.2: connection,
//! transfer-encoding, upgrade, keep-alive, proxy-connection) are stripped
//! defensively in `build_http_response` per cross-sub-phase architectural
//! rule 4. Header names are emitted lowercase per RFC 7540 §8.1.2 (the h2
//! crate would reject uppercase names; defense-in-depth).

use envoy_http1::Response;
use http::{HeaderName, HeaderValue, Response as HttpResponse, StatusCode};

use crate::error::Http2Error;

// H2-forbidden hop-by-hop headers: crate::H2_FORBIDDEN_HOP_BY_HOP (lib.rs).
// Per Task 2 review I2: consolidated from per-module duplicates into a single
// crate-level constant. See lib.rs for the canonical definition + rationale.

/// Translate an `envoy_http1::Response` into an `http::Response<()>` carrying
/// the status + headers (with H2-forbidden headers stripped). The body is
/// sent separately via `h2::SendStream::send_data` in `send_envoy_response`.
pub fn build_http_response(resp: &Response) -> Result<HttpResponse<()>, Http2Error> {
    let status = StatusCode::from_u16(resp.status).map_err(|_| Http2Error::BadStatusCode {
        status: resp.status,
    })?;
    let mut builder = HttpResponse::builder().status(status);
    // resp.reason intentionally dropped — H2 has no reason-phrase
    // (RFC 7540 §8.1.2.4: only :status pseudo-header).
    for (name, value) in &resp.headers {
        let name_lc = name.to_ascii_lowercase();
        if crate::H2_FORBIDDEN_HOP_BY_HOP.contains(&name_lc.as_str()) {
            continue;
        }
        let header_name = HeaderName::from_bytes(name_lc.as_bytes())
            .map_err(|_| Http2Error::MalformedH2HeaderBlock)?;
        let header_value =
            HeaderValue::from_str(value).map_err(|_| Http2Error::MalformedH2HeaderBlock)?;
        builder = builder.header(header_name, header_value);
    }
    builder
        .body(())
        .map_err(|_| Http2Error::MalformedH2HeaderBlock)
}

/// Decorate a filter-synth H2 response with the standard response headers,
/// symmetric to H1's `decorate_filter_synth_response` (`crates/envoy-http1/src/hcm.rs:968`)
/// — minus `connection`, which is an H2-forbidden hop-by-hop header stripped by
/// `build_http_response` per `H2_FORBIDDEN_HOP_BY_HOP` (RFC 7540 §8.1.2.2).
///
/// Adds `content-length` always (overwritten from `resp.body.len()`); adds
/// `server` / `date` / `content-type` only-if-missing (a filter that sets its
/// own value wins). Closes the 09 REVIEW M2 implementation arm (phase 11 D6):
/// the H1 writer path has decorated filter-synth responses since 09 ADR-0033
/// Commit C; this brings the H2 writer path to parity.
pub(crate) fn decorate_filter_synth_response_h2(resp: &mut Response) {
    // content-length: always derived from body.len(); overwrite if present.
    let cl_value = resp.body.len().to_string();
    let mut cl_set = false;
    for (k, v) in resp.headers.iter_mut() {
        if k.eq_ignore_ascii_case("content-length") {
            *v = cl_value.clone();
            cl_set = true;
            break;
        }
    }
    if !cl_set {
        resp.headers.push(("content-length".to_string(), cl_value));
    }
    // server / date / content-type: add only-if-missing. NO connection (H2-forbidden).
    let standards: [(&str, String); 3] = [
        ("server", "envoy-rust".to_string()),
        ("date", envoy_http1::date::now_imf_fixdate()),
        ("content-type", "text/plain".to_string()),
    ];
    for (name, value) in standards {
        if !resp
            .headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case(name))
        {
            resp.headers.push((name.to_string(), value));
        }
    }
}

/// Drive the actual H2 response emission. Sends the response head via
/// `send_response`, then the body via `send_data(end_of_stream=true)`.
///
/// Error mapping note: response-head-send failures surface as
/// `Http2Error::H2StreamAccept` (a misnomer — the variant's name implies
/// stream-accept, but `send_response()` is the server's first wire egress
/// for the stream). Body-write failures surface as `Http2Error::H2BodyRead`
/// (also a misnomer when applied to body WRITE). Future cleanup may
/// rename the variants and/or introduce a single `H2ResponseSend` —
/// defer per SPEC §6 local signpost 21.
pub async fn send_envoy_response(
    mut send_response: h2::server::SendResponse<bytes::Bytes>,
    resp: Response,
) -> Result<(), Http2Error> {
    let head = build_http_response(&resp)?;
    let mut send_stream = send_response
        .send_response(head, /* end_of_stream = */ resp.body.is_empty())
        .map_err(|source| Http2Error::H2StreamAccept { source })?;
    if !resp.body.is_empty() {
        send_stream
            .send_data(resp.body, /* end_of_stream = */ true)
            .map_err(|source| Http2Error::H2BodyRead { source })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use envoy_http1::Response;

    fn synth_response(status: u16, headers: Vec<(&str, &str)>, body: &[u8]) -> Response {
        Response {
            status,
            reason: None,
            headers: headers
                .into_iter()
                .map(|(n, v)| (n.to_string(), v.to_string()))
                .collect(),
            body: Bytes::copy_from_slice(body),
        }
    }

    #[test]
    fn envoy_response_to_http2_strips_h2_forbidden_headers() {
        let resp = synth_response(
            200,
            vec![
                ("server", "envoy-rust"),
                ("connection", "close"),
                ("transfer-encoding", "chunked"),
                ("upgrade", "h2c"),
                ("keep-alive", "timeout=5"),
                ("proxy-connection", "keep-alive"),
                ("content-type", "text/plain"),
            ],
            b"ok",
        );
        let http_resp = build_http_response(&resp).expect("builds");
        let names: Vec<&str> = http_resp
            .headers()
            .iter()
            .map(|(n, _)| n.as_str())
            .collect();
        for forbidden in &[
            "connection",
            "transfer-encoding",
            "upgrade",
            "keep-alive",
            "proxy-connection",
        ] {
            assert!(
                !names.iter().any(|n| n.eq_ignore_ascii_case(forbidden)),
                "expected `{forbidden}` to be stripped, but found in {names:?}"
            );
        }
        assert!(names.iter().any(|n| n.eq_ignore_ascii_case("server")));
        assert!(names.iter().any(|n| n.eq_ignore_ascii_case("content-type")));
    }

    #[test]
    fn envoy_response_to_http2_preserves_status_and_body() {
        let resp = synth_response(418, vec![("content-type", "text/plain")], b"teapot");
        let http_resp = build_http_response(&resp).expect("builds");
        assert_eq!(http_resp.status().as_u16(), 418);
        // body() returns the unit body for an http::Response<()> (the actual
        // body bytes are sent via h2::SendStream::send_data; here we verify
        // build_http_response correctly carries the status + headers, and
        // we delegate the body-write check to the integration test).
        assert!(http_resp.headers().contains_key(http::header::CONTENT_TYPE));
    }

    #[test]
    fn build_http_response_rejects_invalid_status_code() {
        // status 99 is below 100 → StatusCode::from_u16 fails → BadStatusCode.
        let resp = synth_response(99, vec![], b"");
        let err = build_http_response(&resp).expect_err("must fail on invalid status");
        assert!(
            matches!(err, Http2Error::BadStatusCode { status: 99 }),
            "got {err:?}"
        );
    }

    #[test]
    fn build_http_response_rejects_invalid_header_name() {
        // A non-token byte ("é" = 0xC3 0xA9 in UTF-8; `é` is not a valid
        // HTTP token character) in the header NAME causes
        // HeaderName::from_bytes to fail → MalformedH2HeaderBlock.
        let resp = synth_response(200, vec![("héllo", "v")], b"");
        let err = build_http_response(&resp).expect_err("must fail on invalid header name");
        assert!(
            matches!(err, Http2Error::MalformedH2HeaderBlock),
            "got {err:?}"
        );
    }

    #[test]
    fn decorate_h2_adds_standard_headers_when_filter_provides_none() {
        let mut resp = Response {
            status: 503,
            reason: None,
            headers: Vec::new(),
            body: Bytes::from_static(b"fault filter abort"),
        };
        super::decorate_filter_synth_response_h2(&mut resp);
        let name = |n: &str| -> Option<&str> {
            resp.headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(n))
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(name("content-length"), Some("18"));
        assert_eq!(name("server"), Some("envoy-rust"));
        assert_eq!(name("content-type"), Some("text/plain"));
        let date = name("date").expect("date header added");
        assert!(!date.is_empty(), "date empty: {date:?}");
        // H2: NO connection header (H2-forbidden hop-by-hop).
        assert!(
            name("connection").is_none(),
            "connection must NOT be added on H2"
        );
        // 4 standard headers; no more, no fewer (filter contributed 0).
        assert_eq!(resp.headers.len(), 4, "headers: {:?}", resp.headers);
    }

    #[test]
    fn decorate_h2_preserves_filter_headers_and_overwrites_content_length() {
        let mut resp = Response {
            status: 503,
            reason: None,
            headers: vec![
                ("server".to_string(), "my-proxy".to_string()),
                ("content-length".to_string(), "10".to_string()),
                ("x-fault-policy".to_string(), "phase-11".to_string()),
            ],
            body: Bytes::from_static(b"fault filter abort"),
        };
        super::decorate_filter_synth_response_h2(&mut resp);
        let name = |n: &str| -> Option<String> {
            resp.headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(n))
                .map(|(_, v)| v.clone())
        };
        // Filter's server wins (only-if-missing for server).
        assert_eq!(name("server").as_deref(), Some("my-proxy"));
        // content-length always overwritten to body.len() = 18.
        assert_eq!(name("content-length").as_deref(), Some("18"));
        // date + content-type added (filter didn't provide).
        assert!(name("date").is_some());
        assert_eq!(name("content-type").as_deref(), Some("text/plain"));
        // Non-standard header preserved verbatim.
        assert_eq!(name("x-fault-policy").as_deref(), Some("phase-11"));
        // Still no connection.
        assert!(name("connection").is_none());
    }
}
