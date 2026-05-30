//! Phase 14.2 D8.1 differential acceptance test for fixture
//! 0022-upstream-outlier-detection-consecutive-5xx. Drives 4 sequential
//! `GET /fail` over ONE downstream H1 keep-alive conn (Driver::Http1KeepAlive,
//! extended at 14.2 Task 6 with per-request body + header assertions). The
//! configurable-status backend returns 500 ("server error\n", 13 bytes) on
//! `/fail`, so the cluster's `outlier_detection.consecutive_5xx: 3` detector
//! ticks 1 -> 2 -> 3 across requests 1-3 and ejects the sole endpoint on
//! request 3. Request 4 then finds no healthy endpoint (panic disabled via
//! `healthy_panic_threshold: { value: 0 }`) and receives the 12.2
//! no-healthy-upstream synth-503 ("no healthy upstream", 19 bytes).
//!
//! Bilateral assertions: the 500,500,500,503 status sequence + byte-exact
//! bodies + the presence (reqs 1-3) vs absence (req 4) of
//! `x-envoy-upstream-service-time`, plus the consecutive-5xx
//! outlier-detection ejection counters
//! (`cluster.backend_cluster.outlier_detection.ejections_*`). The remaining
//! Envoy-only outlier-detection stat names are listed under
//! `allowlist_envoy_only` in the fixture's expectations.yaml.
//!
//! Docker-gated by the differential harness at the cluster level (no per-test
//! cfg gate; the harness skips when `DOCKER_HOST` is unavailable). The
//! per-path status mapping `/fail=500` is wired by the harness backend spawn,
//! keyed on the fixture directory name.

use std::path::PathBuf;

#[tokio::test]
async fn upstream_outlier_detection_consecutive_5xx_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0022-upstream-outlier-detection-consecutive-5xx");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
