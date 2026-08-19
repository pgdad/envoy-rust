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

#[cfg(test)]
mod tests {
    use super::*;

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
