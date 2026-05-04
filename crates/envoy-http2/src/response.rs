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
}
