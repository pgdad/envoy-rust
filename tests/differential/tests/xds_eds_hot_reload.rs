//! 27 Task 7 (ADR-0067/0068) differential acceptance test for fixture
//! 0035-xds-eds-hot-reload — file-based EDS endpoint HOT-RELOAD, bilaterally
//! asserted. A STATIC-but-EDS cluster `eds_backend` (NO inline
//! `load_assignment`); its single endpoint exists ONLY because each proxy loaded
//! the watched EDS file, and it CHANGES only because each proxy re-reads the file
//! after an atomic-rename rewrite. Three-phase sequence: pre (`GET /probe` ->
//! body `backend: backend_1`) -> atomic-rename reload -> post (`GET /probe` ->
//! body `backend: backend_2`). The same path returns a DIFFERENT body after the
//! rewrite — the bilateral endpoint-swap proof, over TWO distinguishable
//! single-endpoint echo backends.
//!
//! NATIVE-Linux-CI-authoritative: under macOS/Docker-Desktop virtiofs the host
//! bind-mount inotify does not propagate into the container, so the upstream
//! Envoy never observes the reload locally (local verification = the in-process
//! backstop crates/envoy-bin/tests/xds_eds_hot_reload.rs, which also carries the
//! counter / config_dump / bad-reload-taxonomy proofs this data-plane
//! differential cannot see).
//!
//! Docker-gated by the differential harness at the cluster level (no per-test
//! cfg gate; the harness skips when `DOCKER_HOST` is unavailable).

use std::path::PathBuf;

#[tokio::test]
async fn xds_eds_hot_reload_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0035-xds-eds-hot-reload");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
