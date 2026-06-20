//! 30 Task 7 (ADR-0074) differential acceptance test for fixture
//! 0038-lb-subset — subset load-balancing cross-proxy ROUTE selection,
//! bilaterally asserted. A STATIC cluster `subset_cluster` (default
//! ROUND_ROBIN) carrying an `lb_subset_config` (single selector keys:[stage],
//! fallback_policy: NO_FALLBACK) over two distinguishable echo backends
//! (backend_1 = {stage:prod}, backend_2 = {stage:canary}); three routes each
//! carry a `metadata_match` that narrows the eligible set. The Http1RouteSelect
//! driver drives `/prod`, `/canary`, `/nope` against BOTH upstream Envoy and
//! envoy-rust and asserts: STRONG — per-200-probe cross-proxy identical backend
//! selection AND agreement with the §A oracle (`/prod`->backend_1,
//! `/canary`->backend_2); NO_FALLBACK — `/nope` returns 503 with the fixed
//! `no healthy upstream` body on each side.
//!
//! This differential is LOCALLY observable (a plain request/response with NO
//! file-watch/reload trigger), so the Docker test runs and is authoritative on
//! any host with a Docker daemon.
//!
//! Docker-gated by the differential harness at the cluster level (no per-test
//! cfg gate; the harness skips when `DOCKER_HOST` is unavailable).

use std::path::PathBuf;

#[tokio::test]
async fn lb_subset_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0038-lb-subset");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
