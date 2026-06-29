//! Docker-gated differential test for fixture 0060-accesslog-rf-connect-failure.
//! Phase 52 (ADR-0109) — the FIFTH non-`-` `%RESPONSE_FLAGS%` witness: `UF`
//! (UpstreamConnectionFailure), BYTE-EXACT cross-proxy on the upstream-connect-
//! refused 503 path. A STATIC cluster with NO circuit_breakers and NO
//! retry_policy and a single dead endpoint (`127.0.0.1:1`): both proxies DIAL
//! it, the kernel refuses the connect → the connect-failure synth-503.
//! envoy-rust now (a) returns 503 (Task 1 corrected the unvalidated 502) and
//! (b) DERIVES `%RESPONSE_FLAGS%` = `UF` from the connect-failure final-outcome
//! boolean (NOT from `%RESPONSE_CODE_DETAILS%`, which — like the response body —
//! carries the non-deterministic OS transport-failure reason and is NOT logged
//! / NOT compared). Upstream Envoy v1.33 emits status 503 + `rf:"UF"` here
//! (state-0 recon: byte-stable across 8 repeats + a container restart). Drives
//! `kind: http1_access_log_byte_exact` (a `GET /` probe, `expected_status: 503`,
//! json_format {rc, rf}); asserts the emitted line `{"rc":503,"rf":"UF"}` is
//! byte-identical. The driver asserts status + the access-log line but NOT the
//! response body. H1-only (H2 deferred — M45-1).

use std::path::PathBuf;

#[tokio::test]
async fn access_log_rf_connect_failure() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0060-accesslog-rf-connect-failure");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
