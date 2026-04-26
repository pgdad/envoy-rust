//! Phase 03.1 differential acceptance test: drive a payload through a
//! tcp_proxy listener whose filter chain terminates downstream TLS, with a
//! plaintext upstream backend. Should produce identical bytes between
//! upstream Envoy v1.33.0 and envoy-rust. Docker-gated; in CI this runs on
//! `ubuntu-latest` alongside the phase-00 `echo_fixture`, phase-01
//! `admin_ready_fixture`, and phase-02.2 `tcp_proxy_fixture`.

use std::path::PathBuf;

#[tokio::test]
async fn tls_downstream_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0004-tls-downstream");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
