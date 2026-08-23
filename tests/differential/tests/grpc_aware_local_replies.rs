//! Sub-phase 110.2 differential acceptance test: gRPC-aware LOCAL REPLIES over
//! HTTP/1.1 — the cross-proxy witness for the transform sibling 110.1 landed
//! and proved only in-process.
//!
//! 32 HTTP/1.1 probes at a backend-free, CLUSTER-FREE HCM listener
//! (`clusters: []`, `direct_response` + one `redirect:` route). A request whose
//! `content-type` is EXACTLY `application/grpc` or begins with
//! `application/grpc+` turns any LOCALLY GENERATED reply into HTTP 200 +
//! `content-type: application/grpc` + `content-length: 0`, body DROPPED, with a
//! `grpc-status` header carrying a mapped code and — only when the original
//! body was non-empty — a `grpc-message` header carrying that body
//! percent-encoded.
//!
//! The mapping is SPARSE: `400`->13, `401`->16, `403`->7, `404`->12, `429`->14,
//! `502`->14, `503`->14, `504`->14, and EVERYTHING ELSE -> 2 (UNKNOWN) —
//! including the whole 2xx/3xx range and, counter-intuitively, `500` and `405`,
//! both of which this fixture probes. Detection is byte-exact and
//! CASE-SENSITIVE on the VALUE: `APPLICATION/GRPC` does not match, a
//! `; charset=utf-8` parameter DEFEATS it, and neither `application/grpc-web`
//! nor `application/grpcfoo` is a match — the two traps a naive `starts_with`
//! falls into.
//!
//! `grpc-status`, `grpc-message`, `content-type`, `content-length` and
//! `location` are all OUTSIDE the harness's 3-entry `HEADER_ALLOW_LIST`, so
//! `diff_headers` compares every one of them VALUE-EXACT across the two
//! proxies. That comparison is this fixture's entire witness.
//!
//! `envoy.yaml` and `envoy-rust.yaml` are BYTE-IDENTICAL. Backend-free (no
//! `{{BACKEND_IP}}` marker, so no backend container spawns), therefore fully
//! verifiable on a developer host rather than CI-authoritative.

use std::path::PathBuf;

#[tokio::test]
async fn grpc_aware_local_replies() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0089-grpc-aware-local-replies");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
