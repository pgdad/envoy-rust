//! Phase 15 (ADR-0043) differential acceptance test for fixture
//! 0023-upstream-circuit-breaker-max-pending-requests. Drives ONE downstream
//! H1 GET (Driver::Http1KeepAlive) against a cluster configured with
//! `circuit_breakers.thresholds: [{ max_connections: 1, max_pending_requests: 0 }]`,
//! then asserts the bilateral `max_pending_requests:0` reject path: a 503 with
//! the byte-exact 81-byte "...reset reason: overflow" body + the
//! `x-envoy-overloaded` header, plus the post-settle stats
//! `cluster.backend_cluster.upstream_rq_pending_overflow: 1` +
//! `upstream_cx_overflow: 0` + `upstream_cx_total: 0` +
//! `circuit_breakers.default.cx_open: 0` (the pending-gate fires before the
//! cap-check, so no connection demand reaches the cap and the backend is never
//! contacted — §0.C findings 1+3).
//!
//! Timing-robust: ONE GET, no concurrency, no slow backend (lock-in #11). The
//! SPEC's `Driver::Http1Concurrent` + `--hold-ms` knob are DROPPED per ADR-0043.
//!
//! Docker-gated by the differential harness at the cluster level (no per-test
//! cfg gate; the harness skips when `DOCKER_HOST` is unavailable).

use std::path::PathBuf;

#[tokio::test]
async fn upstream_circuit_breaker_max_pending_requests_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0023-upstream-circuit-breaker-max-pending-requests");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
