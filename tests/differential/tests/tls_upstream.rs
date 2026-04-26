//! Phase 03.2 differential acceptance test: drive a plaintext payload
//! through envoy / envoy-rust, originating upstream TLS to a single
//! `tls-echo-server` helper. The configured `sni: "envoy-rust.test"` is
//! sent in the upstream ClientHello; the harness CA validates the leaf.
//! Both proxies must produce identical post-handshake bytes. Docker-gated;
//! in CI this runs on `ubuntu-latest` alongside `tls_downstream_fixture`.

use std::path::PathBuf;

#[tokio::test]
async fn tls_upstream_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0005-tls-upstream");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
