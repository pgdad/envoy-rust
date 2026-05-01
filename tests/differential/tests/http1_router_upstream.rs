//! Phase 04.3 differential acceptance test: drive a GET / through an HCM
//! `route: { cluster: backend }` listener whose backend points at a host-side
//! `http1-echo-server` helper. Both proxies must produce identical (status,
//! body, header-set-modulo-allow-list) — the body is the helper's
//! deterministic echo shape (alphabetically-sorted lowercase header names +
//! verbatim values + verbatim body bytes). Docker-gated; in CI this runs on
//! `ubuntu-latest` alongside the phase-04.1 `http1_direct_response` fixture.

use std::path::PathBuf;

#[tokio::test]
async fn http1_router_upstream_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0008-http1-router-upstream");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
