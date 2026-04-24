//! Phase 01 differential acceptance test: GET /ready on the admin endpoint
//! should produce identical status + body between upstream Envoy v1.33.0 and
//! envoy-rust. Docker-gated; in CI this runs on `ubuntu-latest` alongside the
//! phase-00 `echo_fixture` test.

use std::path::PathBuf;

#[tokio::test]
async fn admin_ready_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0002-static-admin-ready");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
