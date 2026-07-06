//! Docker-gated differential test for fixture
//! 0067-accesslog-h2-urx-retry-exhausted.
//! Phase 61 (ADR-0118) — the FOURTH H2 `%RESPONSE_FLAGS%` witness: `URX`
//! (UpstreamRetryLimitExceeded), byte-exact cross-proxy on the H2
//! retry-limit-exceeded 503 path — the H2 analogue of fixture `0059`
//! (phase 51). A `STRICT_DNS` plain-H1-upstream cluster with
//! `retry_policy:{retry_on:"5xx",num_retries:1}` against an always-503
//! health-aware backend — both attempts 503, the retry budget of 1
//! consumed, the last 503 surfaced downstream verbatim. Spawns Envoy v1.33
//! in a container; spawns envoy-rust as a subprocess; drives
//! `kind: http2_access_log_byte_exact` (reusing the phase-56 driver
//! verbatim); reads each side's file access-log and asserts the emitted
//! line is byte-identical:
//!   {"method":"GET","proto":"HTTP/2","rc":503,"rcd":"via_upstream","rf":"URX"}
//! PURE cross-proxy equality (no static literal). UNLIKE fixtures
//! 0058/0065, no status-code correction was needed this phase — only rf
//! changes (rcd was already the correct `via_upstream`).

use std::path::PathBuf;

#[tokio::test]
async fn access_log_h2_urx_retry_exhausted() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0067-accesslog-h2-urx-retry-exhausted");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
