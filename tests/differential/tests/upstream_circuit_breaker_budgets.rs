//! Phase 17 (ADR-0046 SPEC / ADR-0047 PLAN) differential acceptance test for
//! fixture 0025-upstream-circuit-breaker-retry-budget — the bilateral budget
//! proof of the phase. Drives three sequential H1 GETs over ONE downstream
//! keep-alive conn (Driver::Http1KeepAlive) against three single-endpoint
//! `STRICT_DNS` clusters that all point at the SAME stateful backend host:port:
//!
//!   1. GET /budget-blocked (cluster budget_zero, max_retries:0 +
//!      track_remaining + retry_on 5xx num_retries:1) — attempt 1 -> backend
//!      503 ("service unavailable\n"); the retry the policy would dispatch is
//!      BLOCKED by the max_retries:0 budget, so the attempt-1 503 is returned
//!      verbatim with x-envoy-attempt-count: 1 and NO x-envoy-overloaded,
//!      `upstream_rq_retry_overflow` ticks.
//!   2. GET /budget-allowed (cluster budget_default, default budgets 3/1024 +
//!      retry_on 5xx num_retries:1) — attempt 1 -> backend 503 ("fail\n"); the
//!      within-cap retry proceeds and attempt 2 -> backend 200 ("ok\n"). Final
//!      200, x-envoy-attempt-count: 2, `upstream_rq_retry_success` ticks (the
//!      L10 control: the budget gate does NOT block a within-cap retry).
//!   3. GET /rq-blocked (cluster rq_zero, max_requests:0, NO retry_policy) —
//!      the request-budget gate rejects BEFORE any upstream connect; byte-exact
//!      "...reset reason: overflow" local-reply 503 + x-envoy-overloaded,
//!      x-envoy-attempt-count: 1, `upstream_rq_pending_overflow` (and
//!      `upstream_rq_5xx` per ADR-0047 L3) tick, backend never contacted.
//!
//! Bilateral assertions: the 503/200/503 status sequence + byte-exact bodies +
//! value-exact x-envoy-attempt-count + the present/absent x-envoy-overloaded
//! disposition on each probe, plus the cumulative per-cluster budget counters,
//! the track_remaining gauges (remaining_retries / remaining_rq), the momentary
//! rq_retry_open / rq_open gauges (0 at rest), and the HCM downstream counters.
//!
//! The backend is the `health-aware-http1-backend` helper, spawned by the
//! harness (`run_fixture`'s `needs_health_aware_backend` gate, keyed on the
//! fixture directory name) with `--per-path /budget-blocked=503 --retry-script
//! /budget-allowed=fail:1`. The retry-script counter is a single global
//! per-path cyclic window (fail:1 -> 503,200,503,200,…): on macOS, Docker
//! Desktop NATs Envoy-in-Docker's source IP to 127.0.0.1 — identical to
//! envoy-rust's — so per-source keying is not viable. The harness drives the
//! two proxies sequentially, so each proxy's consecutive /budget-allowed retry
//! pair lands in its own fresh window and observes the same fail-then-succeed
//! sequence.
//!
//! Docker-gated by the differential harness at the cluster level (no per-test
//! cfg gate; the harness skips when `DOCKER_HOST` is unavailable).

use std::path::PathBuf;

#[tokio::test]
async fn upstream_circuit_breaker_budgets_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0025-upstream-circuit-breaker-retry-budget");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
