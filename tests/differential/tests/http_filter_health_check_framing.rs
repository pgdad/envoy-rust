//! Phase 115 (ADR-0203) differential acceptance test for fixture
//! `0097-http-filter-health-check-framing`: a `health_check` intercept of an
//! HTTP/1.1 `HEAD` composed with a LATER encode-side `header_mutation` that
//! writes `content-length: 7`.
//!
//! Upstream settles a headers-only reply's framing last, so the mutation's
//! `content-length` is the reply's ONLY framing header — no
//! `transfer-encoding: chunked` beside it (phase-115 `REVIEW-2.md` I2-1, cell
//! 3a). One probe; the header set discriminates.
//!
//! `envoy.yaml` and `envoy-rust.yaml` are BYTE-IDENTICAL. Docker-gated and
//! backend-free, so fully verifiable on a developer host.

use std::path::PathBuf;

#[tokio::test]
async fn http_filter_health_check_framing_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0097-http-filter-health-check-framing");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
