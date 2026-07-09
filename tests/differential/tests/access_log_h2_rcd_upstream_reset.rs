//! Docker-gated differential test for fixture
//! 0070-accesslog-h2-rcd-upstream-reset.
//! Phase 65 (ADR-0122) — witnesses the DETERMINISTIC H2 upstream-reset
//! `%RESPONSE_CODE_DETAILS%` (`upstream_reset_before_response_started{connection_termination}`)
//! byte-exact cross-proxy, and proves `%RESPONSE_FLAGS%` = `UC` now derives
//! 1:1 from that rcd (the phase-64 boolean discriminator was retired) —
//! CONSUMING carry-forward M64-1. The H2 analogue of fixture 0062 (phase 54).
//! A STRICT_DNS H2-upstream cluster with NO circuit_breakers and NO
//! retry_policy whose single endpoint is the spawned Http2CloseBackend
//! (completes a genuine H2 handshake, then resets the stream without
//! responding). Spawns Envoy v1.33 in a container; spawns envoy-rust as a
//! subprocess; drives `kind: http2_access_log_byte_exact` (the phase-56
//! driver, reused verbatim); reads each side's file access-log and asserts
//! the emitted line is byte-identical (keys sort per ADR-0094 §A):
//!   {"method":"GET","proto":"HTTP/2","rc":503,"rcd":"upstream_reset_before_response_started{connection_termination}","rf":"UC"}
//! PURE cross-proxy equality (no static literal).

use std::path::PathBuf;

#[tokio::test]
async fn access_log_h2_rcd_upstream_reset() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0070-accesslog-h2-rcd-upstream-reset");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
