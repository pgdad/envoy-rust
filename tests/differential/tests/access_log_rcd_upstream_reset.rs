//! Docker-gated differential test for fixture 0062-accesslog-rcd-upstream-reset.
//! Phase 54 (ADR-0111) — the SEVENTH `%RESPONSE_CODE_DETAILS%` witness and the
//! FIRST deterministic upstream-reset rcd:
//! `upstream_reset_before_response_started{connection_termination}`, BYTE-EXACT
//! cross-proxy on the upstream-disconnect-before-headers 503 path. A STRICT_DNS
//! cluster with NO circuit_breakers and NO retry_policy whose single endpoint is
//! a SPAWNED accept-then-close backend (`tcp-echo-server --close-on-accept` via
//! the `{{CLOSE_BACKEND_PORT}}` marker — reused from fixture 0061): both proxies
//! DIAL it, the connect completes, the upstream drains the request then closes
//! (graceful FIN, NO response) → the reset synth-503. envoy-rust now SETS the
//! deterministic reset rcd (§A, overriding the in-loop `via_upstream`, guarded
//! `!retry_limit_exceeded_for_log`) and DERIVES `%RESPONSE_FLAGS%` = `UC` from it
//! (§B, the phase-50 `{overflow} => "UO"` precedent; the phase-53 `reset_for_log`
//! boolean was retired). Upstream Envoy v1.33 emits status 503 +
//! `{"rc":503,"rcd":"upstream_reset_before_response_started{connection_termination}","rf":"UC"}`
//! here (state-0 recon: byte-stable across 3 probes + a container restart).
//! Drives `kind: http1_access_log_byte_exact` (a `GET /` probe,
//! `expected_status: 503`, json_format {rc, rcd, rf}); asserts the emitted line
//! is byte-identical. The driver asserts status + the access-log line but NOT the
//! response body. H1-only (H2 deferred — M45-1). Backend-spawning → LOCAL-RED on
//! the dev host (bridge-IP flake), GREEN on CI.

use std::path::PathBuf;

#[tokio::test]
async fn access_log_rcd_upstream_reset() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0062-accesslog-rcd-upstream-reset");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
