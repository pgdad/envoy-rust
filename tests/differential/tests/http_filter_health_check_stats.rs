//! Phase 115 differential acceptance test for fixture
//! `0096-http-filter-health-check-stats`: the stat surface of
//! `envoy.filters.http.health_check`.
//!
//! Two intercepted `/healthz` probes and one `/other` fall-through, then a
//! bilateral absolute stat assertion: `health_check.request_total` and
//! `health_check.ok` count intercepts only, `downstream_rq_total` counts all
//! three requests, and `downstream_rq_2xx` counts ONLY the fall-through —
//! upstream does not count a request the filter answered.
//!
//! `envoy.yaml` and `envoy-rust.yaml` are BYTE-IDENTICAL. Docker-gated and
//! backend-free.

use std::path::PathBuf;

#[tokio::test]
async fn http_filter_health_check_stats_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0096-http-filter-health-check-stats");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
