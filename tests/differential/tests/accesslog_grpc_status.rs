//! Fixture 0093 — the `%GRPC_STATUS%` access-log command-operator family
//! (phase 113): `%GRPC_STATUS%`, `%GRPC_STATUS(SNAKE_STRING)%` and
//! `%GRPC_STATUS_NUMBER%` over the HTTP/1.1 local-reply surface.
//!
//! Cluster-free and backend-free (`clusters: []`, every route a
//! `direct_response`), so it is verifiable on a development host rather than
//! CI-only. Twelve probes: eight drive the phase-110 HTTP→gRPC map through the
//! access log — corroborating that landed table through a NEW observable — and
//! four cover the content-type spellings the transform rejects.
//!
//! ⚠ This fixture does NOT witness the operator's request-side gate; see the
//! fixture README and the in-process tests in `crates/envoy-http1/src/hcm.rs`.

use std::path::PathBuf;

#[tokio::test]
async fn accesslog_grpc_status() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0093-accesslog-grpc-status");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
