//! 28 Task 7 (ADR-0070) differential acceptance test for fixture
//! 0036-lb-ring-hash — RING_HASH consistent-hashing LB cross-proxy selection,
//! bilaterally asserted. A STATIC `lb_policy: RING_HASH` cluster `ring_cluster`
//! with two distinguishable echo backends; the route action carries a
//! `hash_policy` keyed on the `x-hash-key` request header. The Http1HashSweep
//! driver sweeps 16 distinct `x-hash-key` values against BOTH upstream Envoy
//! and envoy-rust and asserts: STRONG — per-key cross-proxy identical backend
//! selection (the locked xxHash64 ring reproduced end-to-end); SPREAD — both
//! backends are selected over the sweep on each side; STABILITY — a repeated
//! key hits the same backend on each proxy.
//!
//! Unlike phases 26/27 this differential is LOCALLY observable (a plain
//! request/response with NO file-watch/reload trigger), so the Docker test runs
//! and is authoritative on any host with a Docker daemon.
//!
//! Docker-gated by the differential harness at the cluster level (no per-test
//! cfg gate; the harness skips when `DOCKER_HOST` is unavailable).

use std::path::PathBuf;

#[tokio::test]
async fn lb_ring_hash_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0036-lb-ring-hash");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
