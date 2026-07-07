//! Docker-gated differential test for fixture
//! 0069-accesslog-h2-uc-upstream-reset.
//! Phase 64 (ADR-0121) — the SIXTH and FINAL H2 `%RESPONSE_FLAGS%` witness:
//! `UC` (UpstreamConnectionTermination), byte-exact cross-proxy on the H2
//! upstream-disconnect-before-headers 503 path — closing carry-forward
//! M56-1. A STRICT_DNS H2-upstream cluster with NO circuit_breakers and NO
//! retry_policy whose single endpoint is the spawned Http2CloseBackend
//! (completes a genuine H2 handshake, then resets the stream without
//! responding): both proxies DIAL it, the handshake completes, the reset
//! fires post-connect -> the reset synth-503. envoy-rust now (a) returns
//! 503 (Task 1 corrected the unvalidated 502) and (b) DERIVES
//! `%RESPONSE_FLAGS%` = `UC` from the reset final-outcome boolean (NOT from
//! `%RESPONSE_CODE_DETAILS%`, which stays the shared `via_upstream` and is
//! NOT logged/compared this phase — M64-1). Spawns Envoy v1.33 in a
//! container; spawns envoy-rust as a subprocess; drives
//! `kind: http2_access_log_byte_exact` (reusing the phase-56 driver
//! verbatim); reads each side's file access-log and asserts the emitted
//! line is byte-identical:
//!   {"method":"GET","proto":"HTTP/2","rc":503,"rf":"UC"}
//! PURE cross-proxy equality (no static literal).

use std::path::PathBuf;

#[tokio::test]
async fn access_log_h2_uc_upstream_reset() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0069-accesslog-h2-uc-upstream-reset");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
