//! Docker-gated differential test for fixture 0061-accesslog-rf-upstream-reset.
//! Phase 53 (ADR-0110) — the SIXTH non-`-` `%RESPONSE_FLAGS%` witness: `UC`
//! (UpstreamConnectionTermination), BYTE-EXACT cross-proxy on the
//! upstream-disconnect-before-headers 503 path. A STRICT_DNS cluster with NO
//! circuit_breakers and NO retry_policy whose single endpoint is a SPAWNED
//! accept-then-close backend (`tcp-echo-server --close-on-accept` via the
//! `{{CLOSE_BACKEND_PORT}}` marker): both proxies DIAL it, the connect
//! completes, the upstream drains the request then closes (graceful FIN, NO
//! response) → the reset synth-503. envoy-rust now (a) returns 503 (Task 2
//! corrected the unvalidated 502) and (b) DERIVES `%RESPONSE_FLAGS%` = `UC`
//! from the reset final-outcome boolean (NOT from `%RESPONSE_CODE_DETAILS%`,
//! which is the shared `via_upstream`). Upstream Envoy v1.33 emits status 503 +
//! `rf:"UC"` here (state-0 recon: byte-stable across 8 repeats + a container
//! restart). Drives `kind: http1_access_log_byte_exact` (a `GET /` probe,
//! `expected_status: 503`, json_format {rc, rf}); asserts the emitted line
//! `{"rc":503,"rf":"UC"}` is byte-identical. The driver asserts status + the
//! access-log line but NOT the response body. H1-only (H2 deferred — M45-1).
//! Backend-spawning → LOCAL-RED on the dev host (bridge-IP flake), GREEN on CI.

use std::path::PathBuf;

#[tokio::test]
async fn access_log_rf_upstream_reset() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0061-accesslog-rf-upstream-reset");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
