//! Phase 69 differential acceptance test for fixture
//! 0075-upstream-grpc-health-check. Drives a single `GET /` on an HTTP/2
//! (h2c prior-knowledge) listener AFTER a 3.5s settle window past active-HC
//! convergence. Both proxies must converge to ejecting the sole endpoint (an
//! active `grpc_health_check` against a reserved-but-unbound port ->
//! ECONNREFUSED -> failure -> Unhealthy after `unhealthy_threshold: 2`;
//! `healthy_panic_threshold: { value: 0 }` disables panic) and return
//! synth-503 with body `no healthy upstream` (19 bytes per ADR-0037).
//!
//! Same downstream observable as fixture 0074 (the TCP-HC ejection); the
//! failure is driven by the active gRPC checker landed in phase 69 (ADR-0138 /
//! ADR-0139). No backend process is spawned — the `DEAD_BACKEND_PORT` harness
//! marker reserves an ephemeral port and binds nothing (mirrors fixture
//! 0074's ADR-0137 PV-2 shape).
//!
//! Docker-gated by the differential harness at the cluster level (no per-test
//! cfg gate; the harness skips when `DOCKER_HOST` is unavailable).

use std::path::PathBuf;

#[tokio::test]
async fn upstream_grpc_health_check_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0075-upstream-grpc-health-check");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
