//! Phase 26 (ADR-0065/0066) differential acceptance test for fixture
//! 0034-xds-rds-hot-reload — file-based RDS HOT-RELOAD, bilaterally asserted. A
//! STATIC listener whose HCM is RDS-configured (NO inline route_config); the
//! route table exists ONLY because each proxy loaded the watched RDS file, and
//! it CHANGES only because each proxy re-reads the file after an atomic-rename
//! rewrite. Three-phase sequence: pre (`GET /probe` -> "rds-v1") -> atomic-rename
//! reload -> post (`GET /probe` -> "rds-v2"). The same path returns a DIFFERENT
//! `direct_response` body after the rewrite — the bilateral hot-reload proof.
//!
//! NATIVE-Linux-CI-authoritative: under macOS/Docker-Desktop virtiofs the host
//! bind-mount inotify does not propagate into the container, so the upstream
//! Envoy never observes the reload locally (local verification = the in-process
//! backstop crates/envoy-bin/tests/xds_rds_hot_reload.rs).
//!
//! Docker-gated by the differential harness at the cluster level (no per-test
//! cfg gate; the harness skips when `DOCKER_HOST` is unavailable).

use std::path::PathBuf;

#[tokio::test]
async fn xds_rds_hot_reload_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0034-xds-rds-hot-reload");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
