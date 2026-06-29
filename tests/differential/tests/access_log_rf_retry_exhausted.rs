//! Docker-gated differential test for fixture 0059-accesslog-rf-retry-exhausted.
//! Phase 51 (ADR-0108) — the FOURTH non-`-` `%RESPONSE_FLAGS%` witness: `URX`
//! (UpstreamRetryLimitExceeded), BYTE-EXACT cross-proxy on the H1 retry-limit-
//! exceeded 503 path. A single `/retry-exhausted` route with
//! `retry_policy{retry_on:"5xx",num_retries:1}` to a STRICT_DNS backend that 503s
//! every attempt (harness `--per-path /retry-exhausted=503`): the single retry is
//! consumed and the last upstream 503 is returned downstream verbatim (ADR-0045
//! L9). The FIRST `%RESPONSE_FLAGS%` value NOT 1:1 with a unique
//! `%RESPONSE_CODE_DETAILS%` — the rcd is the shared `via_upstream` (a real
//! upstream 503, already matching Envoy, UNCHANGED), so envoy-rust DERIVES `URX`
//! from a SEPARATE boolean set at the retry-loop limit-exceeded exit (the same
//! gate as `upstream_rq_retry_limit_exceeded`), rendered by the H1 derive wrapper
//! (was `-`).
//! Upstream Envoy v1.33 emits the same here (state-0 recon:
//! {"rc":503,"rcd":"via_upstream","rf":"URX"}).
//! Drives `kind: http1_access_log_byte_exact` (a `GET /retry-exhausted` probe,
//! `expected_status: 503`, json_format {rc, rcd, rf}); asserts the emitted JSON
//! line is byte-identical. PURE cross-proxy equality (deterministic both sides).
//! 0059 is the FIRST access-log fixture needing a real health-aware backend.
//! H1-only (H2 deferred — M45-1).

use std::path::PathBuf;

#[tokio::test]
async fn access_log_rf_retry_exhausted() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0059-accesslog-rf-retry-exhausted");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
