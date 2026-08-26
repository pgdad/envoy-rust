//! envoy-Response → H2 SendStream emitter. See SPEC §3 D3.
//!
//! The response-translation surface is split into two pieces:
//!   - `build_http_response(resp)` — translates an `envoy_http1::Response` into
//!     an `http::Response<()>` (status + headers; body is sent separately).
//!     Pure function; testable in isolation.
//!   - `send_envoy_response(send_response, resp, trailers)` — drives the actual
//!     H2 wire emission via `h2::server::SendResponse::send_response` + body
//!     send_data + an optional trailer HEADERS frame. Its end-of-stream fork is
//!     THREE-way (phase 111): END_STREAM may only ride the LAST frame intended,
//!     so an empty body WITH trailers sends no DATA frame at all. Async;
//!     integration-tested via the HCM tests in Task 9.
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

/// Decorate a filter-synth H2 response with the standard response headers —
/// a delegating wrapper over the shared H1/H2 implementation at
/// `envoy_http1::hcm::decorate_filter_synth_response`, called with
/// `connection: None` because `connection` is an H2-forbidden hop-by-hop
/// header stripped by `build_http_response` per `H2_FORBIDDEN_HOP_BY_HOP`
/// (RFC 7540 §8.1.2.2).
///
/// Semantics (see the shared fn's doc for the full ADR-0033 contract):
/// `content-length` always overwritten from `resp.body.len()`; `server` /
/// `date` only-if-missing; `content-type` only-if-missing AND only when the
/// body is non-empty (Envoy v1.33 empirical behaviour, fixture 0031 §6.2).
///
/// Closes the 09 REVIEW M2 implementation arm (phase 11 D6): the H1 writer
/// path has decorated filter-synth responses since 09 ADR-0033 Commit C; this
/// brings the H2 writer path to parity.
pub(crate) fn decorate_filter_synth_response_h2(resp: &mut Response) {
    envoy_http1::hcm::decorate_filter_synth_response(resp, None);
}

/// Translate a trailer block into an `http::HeaderMap` for
/// `h2::SendStream::send_trailers`.
///
/// # No hop-by-hop strip here, deliberately
///
/// `build_http_response` strips `crate::H2_FORBIDDEN_HOP_BY_HOP` from the
/// HEADER block. The trailer block gets no such strip, and that is a MEASURED
/// decision rather than an oversight (phase 111, D-PLAN-4): `h2` rejects
/// exactly `connection` / `transfer-encoding` / `upgrade` / `keep-alive` /
/// `proxy-connection` / `te` != `trailers` on the RECEIVE side too, so an
/// upstream block containing any of them fails in `ClientStream::send_request`'s
/// drain loop and never reaches this function. A strip here would be
/// unreachable, untestable code, which §6.3 forbids. The receive-side
/// asymmetry against upstream Envoy — which drops the block and resets the
/// stream where envoy-rust returns 503 — is banked as CF-111-5.
///
/// `append`, not `insert`: upstream Envoy preserves duplicate trailer names
/// and so must we.
fn build_trailer_map(trailers: &[(String, String)]) -> Result<http::HeaderMap, Http2Error> {
    let mut map = http::HeaderMap::with_capacity(trailers.len());
    for (name, value) in trailers {
        let name_lc = name.to_ascii_lowercase();
        let header_name = HeaderName::from_bytes(name_lc.as_bytes())
            .map_err(|_| Http2Error::MalformedH2HeaderBlock)?;
        let header_value =
            HeaderValue::from_str(value).map_err(|_| Http2Error::MalformedH2HeaderBlock)?;
        map.append(header_name, header_value);
    }
    Ok(map)
}

/// Drive the actual H2 response emission. Sends the response head via
/// `send_response`, then the body via `send_data`, then — phase 111 — the
/// upstream's trailer block via `send_trailers` when the response carries one.
///
/// The end-of-stream fork is THREE-way, and that is a measured requirement
/// rather than a style choice: `h2` returns `UserError::UnexpectedFrameType`
/// for ANY frame sent after END_STREAM, so END_STREAM may only ride the LAST
/// frame we intend to send.
///
/// | body | trailers | frames |
/// |---|---|---|
/// | empty | none | `send_response(head, end_of_stream = true)` |
/// | empty | present | `send_response(head, false)` then `send_trailers` — **no DATA frame** |
/// | non-empty | none | `send_response(head, false)` then `send_data(body, true)` |
/// | non-empty | present | `send_response(head, false)`, `send_data(body, false)`, `send_trailers` |
///
/// The empty-body-with-trailers row is not a corner case: a gRPC trailers-only
/// response has an empty body by construction, which is the whole point of
/// this prerequisite.
///
/// Error mapping note: response-head-send failures surface as
/// `Http2Error::H2StreamAccept` (a misnomer — the variant's name implies
/// stream-accept, but `send_response()` is the server's first wire egress
/// for the stream). Body-write failures surface as `Http2Error::H2BodyRead`
/// (also a misnomer when applied to body WRITE). Future cleanup may
/// rename the variants and/or introduce a single `H2ResponseSend` —
/// defer per SPEC §6 local signpost 21. Trailer-write failures get their own
/// `Http2Error::H2SendTrailers` rather than widening that misnomer further.
pub async fn send_envoy_response(
    mut send_response: h2::server::SendResponse<bytes::Bytes>,
    resp: Response,
    trailers: Option<Vec<(String, String)>>,
) -> Result<(), Http2Error> {
    let head = build_http_response(&resp)?;
    let trailer_map = match trailers {
        Some(t) => Some(build_trailer_map(&t)?),
        None => None,
    };
    let body_empty = resp.body.is_empty();
    let mut send_stream = send_response
        .send_response(
            head,
            /* end_of_stream = */ body_empty && trailer_map.is_none(),
        )
        .map_err(|source| Http2Error::H2StreamAccept { source })?;
    if !body_empty {
        send_stream
            .send_data(resp.body, /* end_of_stream = */ trailer_map.is_none())
            .map_err(|source| Http2Error::H2BodyRead { source })?;
    }
    if let Some(map) = trailer_map {
        send_stream
            .send_trailers(map)
            .map_err(|source| Http2Error::H2SendTrailers { source })?;
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
    fn decorate_h2_omits_content_type_when_body_is_empty() {
        // Empty-body local reply (e.g. CORS preflight 200): content-type must
        // NOT be added, matching Envoy v1.33 empirical behaviour. server/date
        // MUST still be added (unconditional on body size). No connection (H2).
        let mut resp = Response {
            status: 200,
            reason: None,
            headers: Vec::new(),
            body: Bytes::new(),
        };
        super::decorate_filter_synth_response_h2(&mut resp);
        let name = |n: &str| -> Option<&str> {
            resp.headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(n))
                .map(|(_, v)| v.as_str())
        };
        // content-length must be "0".
        assert_eq!(name("content-length"), Some("0"));
        // server and date MUST be added.
        assert!(name("server").is_some(), "server header missing");
        let date = name("date").expect("date header added");
        assert!(!date.is_empty(), "date header empty: {date:?}");
        // H2: NO connection header.
        assert!(
            name("connection").is_none(),
            "connection must NOT be added on H2"
        );
        // content-type MUST NOT be added for empty body.
        assert!(
            name("content-type").is_none(),
            "content-type must NOT be added for empty body; headers: {:?}",
            resp.headers
        );
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

    // ── Phase 111: the trailer emit fork ─────────────────────────────────
    //
    // `build_http_response` sees headers, never FRAMES, and the end-of-stream
    // fork is a property of the frame sequence. These tests therefore drive
    // `send_envoy_response` over a real in-process H2 connection and read back
    // what the client actually observed on the wire.

    /// Drive `send_envoy_response` over a real in-process H2 connection and
    /// return what the client actually observed: status, body bytes, and the
    /// trailer block (empty when none was sent).
    async fn round_trip(
        resp: Response,
        trailers: Option<Vec<(String, String)>>,
    ) -> (u16, Vec<u8>, Vec<(String, String)>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (tcp, _peer) = listener.accept().await.unwrap();
            let mut conn = h2::server::handshake(tcp).await.unwrap();
            if let Some(accepted) = conn.accept().await {
                let (_req, send_response) = accepted.unwrap();
                send_envoy_response(send_response, resp, trailers)
                    .await
                    .expect("send_envoy_response must succeed");
            }
            while conn.accept().await.is_some() {}
        });

        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut send_request, connection) = h2::client::handshake(tcp).await.unwrap();
        let conn_task = tokio::spawn(async move {
            let _ = connection.await;
        });
        let req = http::Request::builder()
            .method("GET")
            .uri("http://probe.local/")
            .body(())
            .unwrap();
        let (response_fut, _tx) = send_request.send_request(req, true).unwrap();
        let response = response_fut.await.unwrap();
        let status = response.status().as_u16();
        let mut body_stream = response.into_body();
        let mut body = Vec::new();
        while let Some(chunk) = body_stream.data().await {
            let chunk = chunk.unwrap();
            body.extend_from_slice(&chunk);
            let _ = body_stream.flow_control().release_capacity(chunk.len());
        }
        // MUST be awaited BEFORE aborting the connection task, or the trailer
        // HEADERS frame is never pumped off the socket and this reads an
        // empty block — a false green on the very cell under test.
        let observed: Vec<(String, String)> = body_stream
            .trailers()
            .await
            .unwrap()
            .map(|map| {
                map.iter()
                    .map(|(n, v)| (n.as_str().to_string(), v.to_str().unwrap().to_string()))
                    .collect()
            })
            .unwrap_or_default();
        conn_task.abort();
        server.abort();
        (status, body, observed)
    }

    fn sorted(mut v: Vec<(String, String)>) -> Vec<(String, String)> {
        v.sort();
        v
    }

    #[tokio::test]
    async fn trailers_follow_a_non_empty_body() {
        let resp = synth_response(200, vec![("content-type", "text/plain")], b"BODY-OK");
        let (status, body, trailers) = round_trip(
            resp,
            Some(vec![
                ("x-trail-a".to_string(), "alpha".to_string()),
                ("x-trail-b".to_string(), "beta".to_string()),
            ]),
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(body, b"BODY-OK");
        assert_eq!(
            sorted(trailers),
            vec![
                ("x-trail-a".to_string(), "alpha".to_string()),
                ("x-trail-b".to_string(), "beta".to_string()),
            ]
        );
    }

    /// The gRPC main case, not a corner: a trailers-only response has an empty
    /// body by construction. Today's `send_response(head, end_of_stream=true)`
    /// branch makes any following frame a `UserError::UnexpectedFrameType`.
    #[tokio::test]
    async fn trailers_follow_an_empty_body_with_no_data_frame() {
        let resp = synth_response(200, vec![("content-type", "application/grpc")], b"");
        let (status, body, trailers) = round_trip(
            resp,
            Some(vec![("grpc-status".to_string(), "0".to_string())]),
        )
        .await;
        assert_eq!(status, 200);
        assert!(body.is_empty(), "expected no DATA frame, got {body:?}");
        assert_eq!(trailers, vec![("grpc-status".to_string(), "0".to_string())]);
    }

    /// PV-6 regression pin: the no-trailers non-empty-body path must be
    /// byte-identical to today.
    #[tokio::test]
    async fn no_trailers_non_empty_body_is_unchanged() {
        let resp = synth_response(200, vec![("content-type", "text/plain")], b"BODY-OK");
        let (status, body, trailers) = round_trip(resp, None).await;
        assert_eq!(status, 200);
        assert_eq!(body, b"BODY-OK");
        assert!(trailers.is_empty(), "got unexpected trailers {trailers:?}");
    }

    /// PV-6 regression pin: the no-trailers EMPTY-body path keeps its
    /// `end_of_stream = true` HEADERS frame.
    #[tokio::test]
    async fn no_trailers_empty_body_is_unchanged() {
        let resp = synth_response(204, vec![], b"");
        let (status, body, trailers) = round_trip(resp, None).await;
        assert_eq!(status, 204);
        assert!(body.is_empty());
        assert!(trailers.is_empty(), "got unexpected trailers {trailers:?}");
    }

    /// PV-3 rows 10-12: upstream Envoy forwards `content-length`,
    /// `te: trailers` and `host` inside a trailer block VERBATIM, and `h2`'s
    /// send-side `check_headers` permits all three. This pins that we do NOT
    /// strip them (D-PLAN-4).
    #[tokio::test]
    async fn trailer_names_envoy_forwards_are_not_stripped() {
        let resp = synth_response(200, vec![("content-type", "text/plain")], b"BODY-OK");
        let (_status, _body, trailers) = round_trip(
            resp,
            Some(vec![
                ("content-length".to_string(), "7".to_string()),
                ("te".to_string(), "trailers".to_string()),
                ("host".to_string(), "example.com".to_string()),
            ]),
        )
        .await;
        assert_eq!(sorted(trailers).len(), 3);
    }

    /// Duplicate trailer names must BOTH reach the wire (upstream Envoy
    /// preserves them — PV-3 row 5). `HeaderMap::append`, not `insert`.
    #[tokio::test]
    async fn duplicate_trailer_names_are_both_emitted() {
        let resp = synth_response(200, vec![("content-type", "text/plain")], b"BODY-OK");
        let (_status, _body, trailers) = round_trip(
            resp,
            Some(vec![
                ("x-multi".to_string(), "one".to_string()),
                ("x-multi".to_string(), "two".to_string()),
            ]),
        )
        .await;
        assert_eq!(
            sorted(trailers),
            vec![
                ("x-multi".to_string(), "one".to_string()),
                ("x-multi".to_string(), "two".to_string()),
            ]
        );
    }
}
