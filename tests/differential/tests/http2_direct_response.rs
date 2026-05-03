//! Phase 05.2 differential acceptance test: drive an H2C GET / through an
//! HCM-direct_response listener with codec_type: HTTP2. Should produce
//! identical (status, body, header-set-modulo-allow-list) between upstream
//! Envoy v1.33.0 and envoy-rust. Docker-gated; in CI this runs on
//! `ubuntu-latest` alongside the phase-00 echo, phase-01 admin_ready, phase-
//! 02.2 tcp_proxy, phase-03 tls_*, and phase-04 http1_* fixtures.

use std::path::PathBuf;

#[tokio::test]
async fn http2_direct_response_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0009-http2-direct-response");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
