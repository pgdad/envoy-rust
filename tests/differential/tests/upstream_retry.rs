//! Phase 16 (ADR-0044 SPEC / ADR-0045 PLAN) differential acceptance test for
//! fixture 0024-upstream-retry-on-5xx — the retry payoff of the phase. Drives
//! two sequential H1 GETs over ONE downstream keep-alive conn
//! (Driver::Http1KeepAlive, extended at 16 Task 7 with the value-exact
//! `require_header_value` field) against a single-endpoint `STRICT_DNS` cluster
//! `backend` whose two routes carry `retry_policy: {retry_on: "5xx",
//! num_retries: 1}`:
//!
//!   1. GET /retry-success  — the stateful retry-script backend serves 503
//!      ("fail\n") on attempt 1 then 200 ("ok\n") on the retry; final 200,
//!      `x-envoy-attempt-count: 2`, `upstream_rq_retry_success` ticks.
//!   2. GET /retry-exhausted — the stateless per-path backend serves 503
//!      ("service unavailable\n") on every attempt; the single retry is
//!      consumed and the LAST upstream 503 is returned verbatim,
//!      `x-envoy-attempt-count: 2`, `upstream_rq_retry_limit_exceeded` ticks.
//!
//! Bilateral assertions: the 200/503 status sequence + byte-exact bodies +
//! value-exact `x-envoy-attempt-count: 2` on both probes, plus the cumulative
//! retry counters (`cluster.backend.upstream_rq_retry: 2`,
//! `upstream_rq_retry_success: 1`, `upstream_rq_retry_limit_exceeded: 1`), the
//! per-attempt `upstream_rq_total: 4`, the completing-only `upstream_rq_5xx: 1`
//! (L5), and the HCM downstream counters (`downstream_rq_2xx: 1`,
//! `downstream_rq_5xx: 1`, `downstream_rq_total: 2`).
//!
//! The backend is the `health-aware-http1-backend` helper, spawned by the
//! harness (`run_fixture`'s `needs_health_aware_backend` gate, keyed on the
//! fixture directory name) with `--retry-script /retry-success=fail:1
//! --per-path /retry-exhausted=503`. The retry-script counter is a single
//! global per-path cyclic window (fail:1 → 503,200,503,200,…): on macOS,
//! Docker Desktop NATs Envoy-in-Docker's source IP to 127.0.0.1 — identical to
//! envoy-rust's — so per-source keying is not viable. The harness drives the
//! two proxies sequentially, so each proxy's consecutive retry pair lands in
//! its own fresh window and observes the same fail-then-succeed sequence.
//!
//! Docker-gated by the differential harness at the cluster level (no per-test
//! cfg gate; the harness skips when `DOCKER_HOST` is unavailable).

use std::path::PathBuf;

#[tokio::test]
async fn upstream_retry_on_5xx_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0024-upstream-retry-on-5xx");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
