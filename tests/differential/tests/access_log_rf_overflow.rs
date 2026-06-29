//! Docker-gated differential test for fixture 0058-accesslog-rf-overflow.
//! Phase 50 (ADR-0107) — the THIRD non-`-` `%RESPONSE_FLAGS%` witness: `UO`
//! (UpstreamOverflow), BYTE-EXACT cross-proxy on the circuit-breaker overflow
//! 503 path, AND the FIRST witness of the overflow `%RESPONSE_CODE_DETAILS%`
//! (`upstream_reset_before_response_started{overflow}`). A STATIC cluster with
//! `circuit_breakers.thresholds:[{max_connections:1, max_pending_requests:0}]`
//! and a single dead endpoint (`127.0.0.1:1`, never dialed): the connect-on-miss
//! pending-gate rejects the first `GET /` → the overflow synth-503 BEFORE any
//! connect. envoy-rust now sets `%RESPONSE_CODE_DETAILS%` =
//! `upstream_reset_before_response_started{overflow}` at the retry-loop
//! consumption site (the `outcome:None` overflow discriminator) and
//! DERIVES `%RESPONSE_FLAGS%` = `UO` from it (was `via_upstream`/`-`).
//! Upstream Envoy v1.33 emits the same here (state-0 recon:
//! {"rc":503,"rcd":"upstream_reset_before_response_started{overflow}","rf":"UO"}).
//! Drives `kind: http1_access_log_byte_exact` (a `GET /` probe, `expected_status:
//! 503`, json_format {rc, rcd, rf}); asserts the emitted JSON line is
//! byte-identical. PURE cross-proxy equality (deterministic on both sides).
//! H1-only (H2 deferred — M45-1).

use std::path::PathBuf;

#[tokio::test]
async fn access_log_rf_overflow() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0058-accesslog-rf-overflow");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
