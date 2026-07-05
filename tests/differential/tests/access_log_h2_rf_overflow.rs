//! Docker-gated differential test for fixture 0066-accesslog-h2-rf-overflow.
//! Phase 58 (ADR-0115) — the THIRD H2 `%RESPONSE_FLAGS%` witness: `UO`
//! (UpstreamOverflow), byte-exact cross-proxy on the H2 pool/circuit-breaker
//! overflow 503 path — the H2 analogue of fixture `0058` (phase 50). A
//! STATIC cluster with `circuit_breakers.thresholds:[{max_connections:1,
//! max_pending_requests:0}]` + a literal dead endpoint `127.0.0.1:1` (NEVER
//! dialed: the H2 pool's connect-on-miss pending-gate rejects the first
//! request with the deterministic overflow synth-503 BEFORE any connect).
//! Spawns Envoy v1.33 in a container; spawns envoy-rust as a subprocess;
//! drives `kind: http2_access_log_byte_exact` (reusing the phase-56 driver
//! verbatim); reads each side's file access-log and asserts the emitted line
//! is byte-identical:
//!   {"method":"GET","proto":"HTTP/2","rc":503,"rcd":"upstream_reset_before_response_started{overflow}","rf":"UO"}
//! PURE cross-proxy equality (no static literal). UNLIKE fixtures 0058/0065,
//! no status-code correction was needed this phase — only rcd/rf change.

use std::path::PathBuf;

#[tokio::test]
async fn access_log_h2_rf_overflow() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0066-accesslog-h2-rf-overflow");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
