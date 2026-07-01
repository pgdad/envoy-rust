//! Docker-gated differential test for fixture
//! 0063-accesslog-rf-overflow-request-budget.
//! Phase 55 (ADR-0112) — witnesses the request-budget (`max_requests`)
//! overflow arm's access-log rendering byte-exact, closing carry-forward
//! M50-C. Phase 50 (ADR-0107) tagged BOTH the pool-overflow arms
//! (`hcm.rs:508`/`:515`) and the request-budget arm (`hcm.rs:951`-`:952`)
//! with the identical rcd string
//! `upstream_reset_before_response_started{overflow}`, feeding the same `UO`
//! %RESPONSE_FLAGS% derive arm (`hcm.rs:1385`) — but fixture `0058` (phase
//! 50) exercises ONLY the pool-overflow arm. A STRICT_DNS cluster with
//! `circuit_breakers.thresholds.max_requests:0` and a single REACHABLE
//! endpoint (the `{{HTTP1_BACKEND_PORT}}`-spawned `Http1EchoBackend`, the
//! same marker pair as fixture 0051): the request-budget gate rejects every
//! request before any pool/backend dispatch → the overflow synth-503.
//! Reachability is load-bearing — a dead endpoint here would surface a real
//! connect failure (UF, the pre-existing ADR-0047 prefetch divergence)
//! instead of the overflow disposition (UO). Upstream Envoy v1.33 emits the
//! same output here (state-1/state-2 recon:
//! {"rc":503,"rcd":"upstream_reset_before_response_started{overflow}","rf":"UO"}
//! — byte-identical, reconfirmed against a freshly-built envoy-rust binary).
//! Drives `kind: http1_access_log_byte_exact` (a `GET /` probe,
//! `expected_status: 503`, json_format {rc, rcd, rf}); asserts the emitted
//! JSON line is byte-identical. PURE cross-proxy equality (deterministic on
//! both sides). H1-only (H2 deferred — M45-1). NO `crates/` change this
//! phase.

use std::path::PathBuf;

#[tokio::test]
async fn access_log_rf_overflow_request_budget() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0063-accesslog-rf-overflow-request-budget");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
