//! Phase 04.1 differential acceptance test: drive a GET /healthz through an
//! HCM-direct_response listener. Should produce identical (status, body,
//! header-set-modulo-allow-list) between upstream Envoy v1.33.0 and
//! envoy-rust. Docker-gated; in CI this runs on `ubuntu-latest` alongside
//! the phase-00 echo, phase-01 admin_ready, phase-02.2 tcp_proxy, and
//! phase-03 tls_* fixtures.

use std::path::PathBuf;

#[tokio::test]
async fn http1_direct_response_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0007-http1-direct-response");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
