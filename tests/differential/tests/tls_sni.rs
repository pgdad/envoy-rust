//! Phase 03.2 differential acceptance test: drive two SNI-keyed TLS probes
//! through envoy / envoy-rust against a plaintext-upstream backend. Each
//! probe asserts the post-handshake peer cert's SAN/CN matches the expected
//! value (DER substring scan in `drive_tls_probes`). Both proxies must
//! select the same cert for the same SNI for the test to pass. Docker-gated;
//! in CI this runs on `ubuntu-latest` alongside `tls_upstream_fixture`.

use std::path::PathBuf;

#[tokio::test]
async fn tls_sni_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0006-tls-sni");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
