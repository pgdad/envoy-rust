//! gRPC-aware local replies over HTTP/1.1 (sub-phase 110.1).
//!
//! Upstream Envoy rewrites any LOCALLY GENERATED reply when the request that
//! provoked it carried a gRPC `content-type`: the HTTP status becomes `200`,
//! `content-type` becomes `application/grpc`, the body is DROPPED,
//! `content-length` becomes `0`, a `grpc-status` header carries a mapped code,
//! and — only when the original body was non-empty — a `grpc-message` header
//! carries that body percent-encoded.
//!
//! Every rule in this module was MEASURED against the `ENVOY_TARGET.md`-pinned
//! `envoyproxy/envoy:v1.33.0` at the 110.1 PLAN-write; the matrices are
//! tabulated in `docs/envoy-rust/phases/110.1-grpc-local-reply-transform/PLAN.md`.
//!
//! This module is `pub(crate)` ON PURPOSE. `envoy-http2` calls
//! `envoy_http1::build_response` (`crates/envoy-http2/src/hcm.rs:518-522`), so
//! anything reachable from the shared route-decision path would also rewrite
//! HTTP/2 responses while missing H2's own `synth_h2_*` upstream-failure
//! family — a partially-covered family on the H2 wire (the ADR-0049
//! silent-divergence class). HTTP/2 is CF-110-1 and stays out of scope.

use crate::headers;
use crate::response::Response;
use bytes::Bytes;

/// The `content-type` value that, alone, marks a gRPC request.
const GRPC_EXACT: &str = "application/grpc";
/// The prefix form: anything after `+` (including nothing) still counts.
const GRPC_PLUS_PREFIX: &str = "application/grpc+";

/// Does this request carry a gRPC `content-type`?
///
/// MEASURED rule (all 14 cells probed against `envoyproxy/envoy:v1.33.0`):
/// true iff the `content-type` value is EXACTLY `application/grpc` or BEGINS
/// WITH `application/grpc+`. Nothing else — a parameter (`; charset=utf-8`)
/// DEFEATS it, the match is CASE-SENSITIVE on the value, and
/// `application/grpc-web`, `application/grpc-web+proto` and
/// `application/grpcfoo` are all NEGATIVE.
///
/// The header NAME lookup is case-insensitive (`find_header`), as everywhere
/// else in the tree; only the VALUE comparison is byte-exact.
///
/// No trimming happens here. Upstream detects `application/grpc ` (trailing
/// space) because the HTTP codec strips optional whitespace from field values
/// before anything sees them — that is the codec's job, not this matcher's.
pub(crate) fn is_grpc_request(headers: &[(String, String)]) -> bool {
    match headers::find_header(headers, headers::CONTENT_TYPE) {
        Some(value) => value == GRPC_EXACT || value.starts_with(GRPC_PLUS_PREFIX),
        None => false,
    }
}

/// Map an HTTP status onto a gRPC status code.
///
/// MEASURED against `envoyproxy/envoy:v1.33.0`: a SPARSE EIGHT-ENTRY table
/// over a DEFAULT of 2 (UNKNOWN). Only these eight are special —
/// `400→13`, `401→16`, `403→7`, `404→12`, `429→14`, `502→14`, `503→14`,
/// `504→14`. EVERYTHING else maps to 2, including the entire 2xx and 3xx
/// ranges and, counter-intuitively, `500`, `501`, `405`, `408`, `409`, `412`,
/// `413` and `499`.
///
/// Do NOT "improve" this with a range arm (e.g. `500..=599 => 13`). The
/// measurement says otherwise and the full-range sweep in the tests will
/// catch it.
pub(crate) fn http_to_grpc_status(status: u16) -> u8 {
    match status {
        400 => 13,
        401 => 16,
        403 => 7,
        404 => 12,
        429 | 502 | 503 | 504 => 14,
        _ => 2,
    }
}

/// Percent-encode a local-reply body for the `grpc-message` header.
///
/// MEASURED rule against `envoyproxy/envoy:v1.33.0`: a byte passes through
/// UNCHANGED iff it is in `0x20..=0x7D` AND is not `%` (0x25). Every other
/// byte — every byte `< 0x20`, every byte `>= 0x7E`, and `%` itself — becomes
/// `%` followed by TWO UPPERCASE hex digits. Multi-byte UTF-8 is encoded PER
/// BYTE, so `é` (0xC3 0xA9) becomes `%C3%A9`.
///
/// Note the UPPER boundary: `}` (0x7D) passes through but `~` (0x7E) is
/// ESCAPED to `%7E`. The parent phase-110 SPEC stated the range as
/// `0x20..=0x7E`; that was MEASURED FALSE at the 110.1 PLAN-write.
///
/// The output is always ASCII, so building it as a `String` is sound: every
/// pushed byte is either an ASCII pass-through or one of `%0123456789ABCDEF`.
pub(crate) fn grpc_message_encode(body: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    // Most local-reply bodies are plain ASCII prose, so the common case is a
    // 1:1 copy; reserving `body.len()` avoids a realloc for those.
    let mut out = String::with_capacity(body.len());
    for &byte in body {
        if (0x20..=0x7D).contains(&byte) && byte != b'%' {
            out.push(byte as char);
        } else {
            out.push('%');
            out.push(HEX[usize::from(byte >> 4)] as char);
            out.push(HEX[usize::from(byte & 0x0F)] as char);
        }
    }
    out
}

/// Rewrite a LOCALLY GENERATED response into upstream Envoy's gRPC shape, if
/// and only if the request that provoked it carried a gRPC `content-type`.
///
/// MEASURED shape (`envoyproxy/envoy:v1.33.0`): status becomes `200`,
/// `content-type` becomes `application/grpc`, the body is DROPPED,
/// `content-length` becomes `0`, `grpc-status` carries the mapped code, and
/// `grpc-message` carries the percent-encoded ORIGINAL body — but ONLY when
/// that body was non-empty; for an empty body the header is ABSENT ENTIRELY,
/// not present-and-empty.
///
/// MEASURED header order, reproduced exactly on three independent cases (a
/// bodied `direct_response`, a `redirect:` route and a circuit-breaker
/// overflow): pass-through headers first in their ORIGINAL relative order,
/// then `content-type`, `grpc-status`, `[grpc-message]`, then `date`,
/// `server`, `connection`, then `content-length: 0`. `location` and
/// `x-envoy-overloaded` are pass-throughs and both SURVIVE — the rule is
/// general, not a `location` special case.
///
/// `serialize_response_head` (`response.rs`) emits headers in vector order
/// verbatim, so vector position IS wire order.
///
/// # This function must only ever see a LOCAL reply
///
/// It has no way to tell a synth from a proxied upstream response — that is
/// the CALLER's job (non-goal 4 / CF-110-2). The tokio seam gates on
/// `outgoing_local`; the io_uring seam relies on `write_owned` being
/// exclusively the local-reply funnel there.
///
/// # Not installed on the shared path, deliberately
///
/// This is NOT called from `synth_with`, from any `synth_*` wrapper, or from
/// `build_response`/`build_response_in`, because `envoy-http2` calls
/// `envoy_http1::build_response` (`crates/envoy-http2/src/hcm.rs:518-522`).
/// See the module doc.
pub(crate) fn apply_grpc_local_reply(resp: &mut Response, req_headers: &[(String, String)]) {
    if !is_grpc_request(req_headers) {
        return;
    }
    // IDEMPOTENCE GUARD. The transform ALWAYS emits `grpc-status`, so its
    // presence is an exact sentinel for "this reply has already been
    // transformed". Without this, a second application would classify the
    // emitted `grpc-status`/`grpc-message` as pass-throughs, keep them, and
    // then append a SECOND `grpc-status` — one derived from the already-
    // rewritten `200` and therefore carrying the default `2` rather than the
    // original status's code — while double-encoding is a standing hazard for
    // `grpc-message` (`%25` -> `%2525` -> `%252525`).
    //
    // No local reply carries `grpc-status` before this point: the whole synth
    // family in `hcm.rs` contains no `grpc` string at all outside its tests.
    // The two wire funnels are mutually exclusive workers, so today nothing
    // reaches this function twice; the guard keeps that a property of the
    // FUNCTION rather than of the current call graph.
    if headers::find_header(&resp.headers, headers::GRPC_STATUS).is_some() {
        return;
    }

    let grpc_status = http_to_grpc_status(resp.status);
    let grpc_message = if resp.body.is_empty() {
        None
    } else {
        Some(grpc_message_encode(&resp.body))
    };

    // Partition the original headers. `content-type` and `content-length` are
    // REWRITTEN (so their originals are dropped here); `date`, `server` and
    // `connection` are RE-ORDERED to sit after the gRPC block; everything else
    // is a pass-through that keeps its original relative position.
    let mut passthrough: Vec<(String, String)> = Vec::with_capacity(resp.headers.len());
    let mut date: Option<(String, String)> = None;
    let mut server: Option<(String, String)> = None;
    let mut connection: Option<(String, String)> = None;

    for (name, value) in std::mem::take(&mut resp.headers) {
        if name.eq_ignore_ascii_case(headers::CONTENT_TYPE)
            || name.eq_ignore_ascii_case(headers::CONTENT_LENGTH)
        {
            // Both are REWRITTEN below, so the originals are dropped here.
            continue;
        }
        if date.is_none() && name.eq_ignore_ascii_case(headers::DATE) {
            date = Some((name, value));
        } else if server.is_none() && name.eq_ignore_ascii_case(headers::SERVER) {
            server = Some((name, value));
        } else if connection.is_none() && name.eq_ignore_ascii_case(headers::CONNECTION) {
            connection = Some((name, value));
        } else {
            passthrough.push((name, value));
        }
    }

    let mut out = passthrough;
    out.push((
        headers::CONTENT_TYPE.to_string(),
        headers::GRPC_CONTENT_TYPE.to_string(),
    ));
    out.push((headers::GRPC_STATUS.to_string(), grpc_status.to_string()));
    if let Some(message) = grpc_message {
        out.push((headers::GRPC_MESSAGE.to_string(), message));
    }
    out.extend(date);
    out.extend(server);
    out.extend(connection);
    out.push((headers::CONTENT_LENGTH.to_string(), "0".to_string()));

    resp.status = 200;
    // `None` makes `serialize_response_head` fall back to `canonical_reason(200)`
    // and emit `HTTP/1.1 200 OK`, matching the measured status line.
    resp.reason = None;
    resp.headers = out;
    resp.body = Bytes::new();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::response::Response;
    use bytes::Bytes;

    fn hdrs(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(n, v)| ((*n).to_string(), (*v).to_string()))
            .collect()
    }

    /// The MEASURED detection matrix, all 14 cells, from the 110.1 PLAN-write
    /// probe against the pinned image. Detection fires iff the `content-type`
    /// value is EXACTLY `application/grpc` or BEGINS WITH `application/grpc+`.
    ///
    /// Two traps live here and both are directly witnessed below: a naive
    /// `starts_with("application/grpc")` wrongly accepts `application/grpcfoo`
    /// and `application/grpc-web`; a case-insensitive or parameter-tolerant
    /// match wrongly accepts `APPLICATION/GRPC` and
    /// `application/grpc; charset=utf-8`.
    #[test]
    fn detection_matrix_matches_upstream() {
        let cells: &[(&str, bool)] = &[
            ("application/grpc", true),
            ("application/grpc+proto", true),
            ("application/grpc+json", true),
            ("application/grpc+", true),
            ("application/grpc; charset=utf-8", false),
            ("application/grpc;charset=utf-8", false),
            ("APPLICATION/GRPC", false),
            ("Application/Grpc", false),
            ("application/grpc-web", false),
            ("application/grpc-web+proto", false),
            ("application/grpcfoo", false),
            ("application/json", false),
            ("", false),
        ];
        for (value, expected) in cells {
            assert_eq!(
                is_grpc_request(&hdrs(&[("content-type", value)])),
                *expected,
                "content-type {value:?} must detect as {expected}"
            );
        }
    }

    /// An ABSENT `content-type` is the 14th measured cell and is NOT detected.
    #[test]
    fn absent_content_type_is_not_grpc() {
        assert!(!is_grpc_request(&hdrs(&[("host", "x")])));
        assert!(!is_grpc_request(&[]));
    }

    /// Header-NAME lookup stays case-insensitive (as everywhere else in the
    /// tree, via `find_header`'s `eq_ignore_ascii_case`) even though the VALUE
    /// comparison is byte-exact.
    #[test]
    fn header_name_lookup_is_case_insensitive() {
        assert!(is_grpc_request(&hdrs(&[(
            "Content-Type",
            "application/grpc"
        )])));
        assert!(is_grpc_request(&hdrs(&[(
            "CONTENT-TYPE",
            "application/grpc"
        )])));
    }

    /// MEASURED: `application/grpc ` WITH a trailing space IS detected upstream
    /// — but that is the HTTP codec stripping optional trailing whitespace
    /// (OWS) from the field value before anything sees it, NOT a tolerance in
    /// the matcher. This test pins that we deliberately do NOT build
    /// trailing-space tolerance into the comparison: by the time a value
    /// reaches here the codec has already trimmed it, so an UNTRIMMED value
    /// with a trailing space must NOT match.
    #[test]
    fn trailing_space_tolerance_is_deliberately_absent() {
        assert!(!is_grpc_request(&hdrs(&[(
            "content-type",
            "application/grpc "
        )])));
    }

    fn resp_with(status: u16, headers: &[(&str, &str)], body: &[u8]) -> Response {
        Response {
            status,
            reason: None,
            headers: hdrs(headers),
            body: Bytes::copy_from_slice(body),
        }
    }

    fn names(resp: &Response) -> Vec<String> {
        resp.headers.iter().map(|(n, _)| n.clone()).collect()
    }

    fn value<'a>(resp: &'a Response, name: &str) -> Option<&'a str> {
        crate::headers::find_header(&resp.headers, name)
    }

    /// The wire shape of a bodied local reply, MEASURED on a `direct_response`
    /// 503 with body `B503`: status 200, `content-type: application/grpc`,
    /// `grpc-status: 14`, `grpc-message: B503`, `content-length: 0`, and the
    /// body DROPPED. Header ORDER is pinned to the measured wire order.
    #[test]
    fn bodied_local_reply_takes_the_measured_wire_shape() {
        let mut resp = resp_with(
            503,
            &[
                ("server", "envoy-rust"),
                ("date", "Mon, 17 Aug 2026 19:00:00 GMT"),
                ("content-length", "4"),
                ("content-type", "text/plain"),
                ("connection", "close"),
            ],
            b"B503",
        );
        apply_grpc_local_reply(&mut resp, &hdrs(&[("content-type", "application/grpc")]));

        assert_eq!(resp.status, 200);
        assert_eq!(
            resp.reason, None,
            "reason must fall back to the canonical 200 OK"
        );
        assert!(resp.body.is_empty(), "the body must be DROPPED");
        assert_eq!(
            names(&resp),
            vec![
                "content-type",
                "grpc-status",
                "grpc-message",
                "date",
                "server",
                "connection",
                "content-length",
            ],
            "MEASURED upstream order for a bodied local reply"
        );
        assert_eq!(value(&resp, "content-type"), Some("application/grpc"));
        assert_eq!(value(&resp, "grpc-status"), Some("14"));
        assert_eq!(value(&resp, "grpc-message"), Some("B503"));
        assert_eq!(value(&resp, "content-length"), Some("0"));
        assert_eq!(value(&resp, "date"), Some("Mon, 17 Aug 2026 19:00:00 GMT"));
        assert_eq!(value(&resp, "server"), Some("envoy-rust"));
        assert_eq!(value(&resp, "connection"), Some("close"));
    }

    /// MEASURED on both an empty-body `direct_response` and the HCM's own
    /// unmatched-path 404: `grpc-message` is ABSENT ENTIRELY, not present with
    /// an empty value. This is the cell a "always set the header" implementation
    /// gets wrong, and it is a header-NAME-SET difference, which is exactly what
    /// the differential harness's `diff_headers` compares.
    #[test]
    fn empty_body_omits_grpc_message_entirely() {
        let mut resp = resp_with(
            404,
            &[
                ("server", "envoy-rust"),
                ("date", "D"),
                ("content-length", "0"),
                ("content-type", "text/plain"),
                ("connection", "close"),
            ],
            b"",
        );
        apply_grpc_local_reply(&mut resp, &hdrs(&[("content-type", "application/grpc")]));

        assert_eq!(resp.status, 200);
        assert_eq!(value(&resp, "grpc-status"), Some("12"));
        assert!(
            !resp.headers.iter().any(|(n, _)| n == "grpc-message"),
            "grpc-message must be ABSENT, not empty: {:?}",
            names(&resp)
        );
        assert_eq!(
            names(&resp),
            vec![
                "content-type",
                "grpc-status",
                "date",
                "server",
                "connection",
                "content-length",
            ]
        );
    }

    /// MEASURED on a `redirect:` route: the `location` header SURVIVES the
    /// transform, in its ORIGINAL leading position, and the reply still becomes
    /// 200 + `application/grpc` + `grpc-status: 2` + `content-length: 0`.
    /// Note `synth_redirect` emits NO `content-type` at all, so this also pins
    /// the "original had no content-type" branch.
    #[test]
    fn redirect_keeps_location_and_still_transforms() {
        let mut resp = resp_with(
            301,
            &[
                ("location", "http://example.com/a"),
                ("date", "D"),
                ("server", "envoy-rust"),
                ("connection", "close"),
                ("content-length", "0"),
            ],
            b"",
        );
        apply_grpc_local_reply(&mut resp, &hdrs(&[("content-type", "application/grpc")]));

        assert_eq!(resp.status, 200);
        assert_eq!(
            names(&resp),
            vec![
                "location",
                "content-type",
                "grpc-status",
                "date",
                "server",
                "connection",
                "content-length",
            ],
            "MEASURED upstream order for a redirect local reply"
        );
        assert_eq!(value(&resp, "location"), Some("http://example.com/a"));
        assert_eq!(value(&resp, "grpc-status"), Some("2"));
        assert_eq!(value(&resp, "content-type"), Some("application/grpc"));
        assert_eq!(value(&resp, "content-length"), Some("0"));
    }

    /// MEASURED on a circuit-breaker overflow: `x-envoy-overloaded` survives in
    /// its ORIGINAL leading position too. The pass-through rule is GENERAL —
    /// it is not a `location` special case.
    #[test]
    fn arbitrary_pass_through_headers_survive_in_original_position() {
        let mut resp = resp_with(
            503,
            &[
                ("x-envoy-overloaded", "true"),
                ("server", "envoy-rust"),
                ("date", "D"),
                ("content-length", "81"),
                ("content-type", "text/plain"),
                ("connection", "close"),
            ],
            b"upstream connect error or disconnect/reset before headers. reset reason: overflow",
        );
        apply_grpc_local_reply(&mut resp, &hdrs(&[("content-type", "application/grpc")]));

        assert_eq!(
            names(&resp),
            vec![
                "x-envoy-overloaded",
                "content-type",
                "grpc-status",
                "grpc-message",
                "date",
                "server",
                "connection",
                "content-length",
            ],
            "MEASURED upstream order for the overflow local reply"
        );
        assert_eq!(value(&resp, "x-envoy-overloaded"), Some("true"));
        assert_eq!(
            value(&resp, "grpc-message"),
            Some(
                "upstream connect error or disconnect/reset before headers. reset reason: overflow"
            )
        );
    }

    /// The body is percent-encoded into `grpc-message` per Task 3's rule.
    #[test]
    fn grpc_message_carries_the_percent_encoded_body() {
        let mut resp = resp_with(400, &[("content-type", "text/plain")], b"a b\nc %25 ~");
        apply_grpc_local_reply(
            &mut resp,
            &hdrs(&[("content-type", "application/grpc+proto")]),
        );
        assert_eq!(value(&resp, "grpc-status"), Some("13"));
        assert_eq!(value(&resp, "grpc-message"), Some("a b%0Ac %2525 %7E"));
    }

    /// A NON-gRPC request leaves the response BYTE-FOR-BYTE untouched. This is
    /// the paired control for every cell above and the guard on non-goal 4
    /// (proxied responses must not be transformed).
    #[test]
    fn non_grpc_request_is_a_total_no_op() {
        let original = resp_with(
            404,
            &[
                ("server", "envoy-rust"),
                ("date", "D"),
                ("content-length", "4"),
                ("content-type", "text/plain"),
                ("connection", "close"),
            ],
            b"B404",
        );
        for ct in [
            "application/json",
            "application/grpc-web",
            "application/grpcfoo",
            "APPLICATION/GRPC",
            "application/grpc; charset=utf-8",
        ] {
            let mut resp = original.clone();
            apply_grpc_local_reply(&mut resp, &hdrs(&[("content-type", ct)]));
            assert_eq!(resp, original, "content-type {ct:?} must not transform");
        }
        // ...and with no content-type at all.
        let mut resp = original.clone();
        apply_grpc_local_reply(&mut resp, &hdrs(&[("host", "x")]));
        assert_eq!(resp, original);
    }

    /// The transform is IDEMPOTENT: applying it twice yields the same response
    /// as applying it once. This matters because the two wire funnels are
    /// separate code paths and a future refactor could route one reply through
    /// both; a non-idempotent transform would double-encode `grpc-message`
    /// (`%25` -> `%2525` -> `%252525`) and corrupt it silently.
    #[test]
    fn transform_is_idempotent() {
        let req = hdrs(&[("content-type", "application/grpc")]);
        let mut once = resp_with(503, &[("content-type", "text/plain")], b"100% done");
        apply_grpc_local_reply(&mut once, &req);
        let mut twice = once.clone();
        apply_grpc_local_reply(&mut twice, &req);
        assert_eq!(
            twice, once,
            "applying the transform twice must change nothing"
        );
    }

    /// The MEASURED encoder, on the exact nine bodies probed at the 110.1
    /// PLAN-write. Bodies were supplied to upstream as `inline_bytes` (base64)
    /// so the source bytes are exact, and each was probed WITH and WITHOUT the
    /// gRPC content-type so the control gave the byte-exact original.
    ///
    /// The DISCRIMINATING cells, each of which a plausible hand-rolled encoder
    /// gets wrong:
    ///   * `~` (0x7E) IS ESCAPED to `%7E`. The parent phase-110 SPEC claimed
    ///     `0x20..=0x7E` passes through; that was MEASURED FALSE.
    ///   * `}` (0x7D) PASSES THROUGH — it is the true upper bound.
    ///   * `%` becomes `%25`, so the input `%25` renders as `%2525`.
    ///   * multi-byte UTF-8 is encoded PER BYTE (`é` -> `%C3%A9`).
    ///   * hex digits are UPPERCASE.
    #[test]
    fn encoder_matches_upstream_on_every_measured_body() {
        let cells: &[(&[u8], &str)] = &[
            (
                b"a b\ncontrol\ttab \xc3\xa9 %25 end",
                "a b%0Acontrol%09tab %C3%A9 %2525 end",
            ),
            (b"q\"b s\\l t~t d\x7fd", "q\"b s\\l t%7Et d%7Fd"),
            (
                b"  ~ +,/:;=?@[]{}|^`<>#&*()",
                "  %7E +,/:;=?@[]{}|^`<>#&*()",
            ),
            (b"~", "%7E"),
            (b"\x7f", "%7F"),
            (b"%25", "%2525"),
            (b"\"\\", "\"\\"),
            (b"}~", "}%7E"),
            (b"\x1f ", "%1F "),
        ];
        for (input, expected) in cells {
            assert_eq!(
                grpc_message_encode(input),
                *expected,
                "encoding {input:?} must produce {expected:?}"
            );
        }
    }

    /// The rule as a property over EVERY single byte: pass through iff the byte
    /// is in `0x20..=0x7D` AND is not `%` (0x25); otherwise `%` + two UPPERCASE
    /// hex digits. Sweeping all 256 byte values pins both boundaries (0x1F/0x20
    /// at the bottom, 0x7D/0x7E at the top) and the `%` carve-out, so an
    /// off-by-one in either direction is impossible to land.
    #[test]
    fn encoder_rule_holds_for_every_byte_value() {
        for byte in 0u8..=255u8 {
            let got = grpc_message_encode(&[byte]);
            let expected = if (0x20..=0x7D).contains(&byte) && byte != b'%' {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            };
            assert_eq!(got, expected, "byte 0x{byte:02X} encoded wrongly");
        }
    }

    /// An empty body encodes to an empty string. (Whether the HEADER is emitted
    /// at all for an empty body is the transform's decision, pinned in Task 4 —
    /// upstream OMITS it entirely rather than sending an empty value.)
    #[test]
    fn empty_body_encodes_to_empty_string() {
        assert_eq!(grpc_message_encode(b""), "");
    }

    /// Hex digits are UPPERCASE, not lowercase — a `{:02x}` slip is the single
    /// most likely encoder bug and it is invisible in the ASCII-only cells.
    #[test]
    fn hex_digits_are_uppercase() {
        assert_eq!(grpc_message_encode(b"\xab\xcd\xef"), "%AB%CD%EF");
        assert_eq!(grpc_message_encode(&[0x0a, 0x1b, 0x7f]), "%0A%1B%7F");
    }

    /// The MEASURED mapping matrix — a SPARSE EIGHT-ENTRY table over a DEFAULT
    /// of 2 (UNKNOWN). All 20 cells were probed against the pinned image at
    /// the 110.1 PLAN-write, each as a `direct_response` at its own distinct
    /// path with a paired non-gRPC control.
    ///
    /// The counter-intuitive cells are the point of this test: `500`, `501`,
    /// `405`, `408`, `409`, `412`, `413` and `499` all map to 2, NOT to 13/14.
    #[test]
    fn status_mapping_matches_upstream() {
        let cells: &[(u16, u8)] = &[
            (200, 2),
            (201, 2),
            (204, 2),
            (301, 2),
            (400, 13),
            (401, 16),
            (403, 7),
            (404, 12),
            (405, 2),
            (408, 2),
            (409, 2),
            (412, 2),
            (413, 2),
            (429, 14),
            (499, 2),
            (500, 2),
            (501, 2),
            (502, 14),
            (503, 14),
            (504, 14),
        ];
        for (http, grpc) in cells {
            assert_eq!(
                http_to_grpc_status(*http),
                *grpc,
                "HTTP {http} must map to grpc-status {grpc}"
            );
        }
    }

    /// The table is SPARSE: exactly eight statuses in the whole `u16` range are
    /// special, and every other one — all 65528 of them — is 2. Sweeping the
    /// full range is what makes a "helpful" extra arm (e.g. `500 => 13`, or a
    /// `4xx => 13` range arm) impossible to add unnoticed.
    #[test]
    fn every_other_status_in_the_whole_u16_range_is_unknown() {
        let special: [u16; 8] = [400, 401, 403, 404, 429, 502, 503, 504];
        let mut specials_seen = 0usize;
        for status in u16::MIN..=u16::MAX {
            if special.contains(&status) {
                specials_seen += 1;
                assert_ne!(
                    http_to_grpc_status(status),
                    2,
                    "special status {status} must not fall through to the default arm"
                );
            } else {
                assert_eq!(
                    http_to_grpc_status(status),
                    2,
                    "status {status} must map to the default 2 (UNKNOWN)"
                );
            }
        }
        assert_eq!(
            specials_seen, 8,
            "the special table must have exactly 8 entries"
        );
    }

    /// First-match-wins: `find_header` returns the first matching name.
    #[test]
    fn first_content_type_wins() {
        assert!(is_grpc_request(&hdrs(&[
            ("content-type", "application/grpc"),
            ("content-type", "application/json"),
        ])));
        assert!(!is_grpc_request(&hdrs(&[
            ("content-type", "application/json"),
            ("content-type", "application/grpc"),
        ])));
    }
}
