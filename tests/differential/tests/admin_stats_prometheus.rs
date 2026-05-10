//! Docker-gated differential test for fixture 0011-admin-stats-prometheus.
//! Phase 06.1 D6 — first admin-side differential fixture. Spawns Envoy
//! v1.33 in a container (admin port exposed via the harness's
//! `expose_admin_port = true` branch); spawns envoy-rust as a
//! subprocess; drives `Driver::AdminScrape` (one HCM `GET /` pre-request
//! against the direct_response listener, then `GET /stats/prometheus`
//! against the admin listener); asserts the metric-name set is equal
//! between envoy ↔ envoy-rust modulo the empirically-seeded
//! `allowlist_envoy_only` (signpost 12).

use std::path::PathBuf;

#[tokio::test]
async fn admin_stats_prometheus() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0011-admin-stats-prometheus");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
