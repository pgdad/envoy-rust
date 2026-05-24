//! Phase 13.1 D9.1 differential acceptance test for fixture
//! 0020-upstream-connection-pooling-and-per-class-counters. Drives 10
//! sequential GETs over ONE downstream H1 keep-alive conn (Driver::
//! Http1KeepAlive) spanning 2xx/3xx/4xx/5xx status classes, then asserts
//! bilateral per-class downstream_rq_{2,3,4,5}xx + cluster
//! upstream_rq_{2,3,4,5}xx + downstream_rq_total + upstream_rq_total +
//! upstream_cx_total + upstream_cx_http1_total.
//!
//! Closes 06.3 REVIEW I2 (a) — the wire-level per-class counter property.
//! The H1 pool (landed at 13.1 Tasks 3-4) coalesces all 10 upstream
//! requests onto ONE upstream conn, the discriminating-observable per
//! parent-13 SPEC §6.2 item-iv (with a per-call `Client::connect`
//! regression `upstream_cx_total` would read 10, not 1).
//!
//! Docker-gated by the differential harness at the cluster level (no
//! per-test cfg gate; the harness skips when `DOCKER_HOST` is
//! unavailable). Per-path status mapping
//! `/301=301,/404=404,/500=500` is wired by the harness via
//! `HealthAwareHttp1Backend::spawn_with_per_path` keyed on the fixture
//! directory name (13.1 Task 7 wiring).

use std::path::PathBuf;

#[tokio::test]
async fn upstream_connection_pooling_and_per_class_counters_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0020-upstream-connection-pooling-and-per-class-counters");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
