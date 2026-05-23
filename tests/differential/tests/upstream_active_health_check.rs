//! Phase 12.2 differential acceptance test for fixture
//! 0019-upstream-active-health-check. Drives a single `GET /` on an HTTP/1.1
//! listener AFTER a 3.5s settle window past active-HC convergence. Both
//! proxies must converge to ejecting the sole endpoint (active HC probes
//! `/healthz` -> 503 -> Unhealthy; `healthy_panic_threshold: { value: 0 }`
//! disables panic) and return synth-503 with body `no healthy upstream`
//! (19 bytes per ADR-0037).
//!
//! This is the FIRST Upstream-robustness-family differential fixture and
//! the FIRST one to drive synth-503 from the no-healthy-upstream arm
//! bilaterally (the `hcm.rs:582` arm). The 06.3 REVIEW I2 synthetic-backend
//! harness primitive (`HealthAwareHttp1Backend`) lands at Task 4 / D7.1
//! and is exercised end-to-end here.
//!
//! Docker-gated by the differential harness at the cluster level (no per-test
//! cfg gate; the harness skips when `DOCKER_HOST` is unavailable).

use std::path::PathBuf;

#[tokio::test]
async fn upstream_active_health_check_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0019-upstream-active-health-check");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
