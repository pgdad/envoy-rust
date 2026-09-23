//! Phase 115 (ADR-0204) differential acceptance test for fixture
//! `0098-http-filter-health-check-transfer-encoding`: a `health_check`
//! intercept of an HTTP/1.1 request composed with a LATER encode-side
//! `header_mutation` that APPENDS `transfer-encoding: gzip`.
//!
//! Upstream's codec owns a headers-only reply's `transfer-encoding`: a `HEAD`
//! goes out `transfer-encoding: chunked` alone, a `GET` `content-length: 0`
//! alone (phase-115 `REVIEW-3.md` I3-1, cell B7). The `transfer-encoding`
//! VALUE discriminates.
//!
//! `envoy.yaml` and `envoy-rust.yaml` are BYTE-IDENTICAL. Docker-gated and
//! backend-free, so fully verifiable on a developer host.

use std::path::PathBuf;

#[tokio::test]
async fn http_filter_health_check_transfer_encoding_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0098-http-filter-health-check-transfer-encoding");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
