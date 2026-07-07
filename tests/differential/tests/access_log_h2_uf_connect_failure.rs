//! Docker-gated differential test for fixture
//! 0068-accesslog-h2-uf-connect-failure.
//! Phase 63 (ADR-0120) — the FIFTH H2 `%RESPONSE_FLAGS%` witness: `UF`
//! (UpstreamConnectionFailure), byte-exact cross-proxy on the H2
//! upstream-connect-refused 503 path — the H2 analogue of fixture `0060`
//! (phase 52). A STATIC H2-upstream cluster with NO circuit_breakers and NO
//! retry_policy and a single dead endpoint (`127.0.0.1:1`): both proxies
//! DIAL it, the kernel refuses the connect → the connect-failure synth-503.
//! envoy-rust now (a) returns 503 (Task 1 corrected the unvalidated 502) and
//! (b) DERIVES `%RESPONSE_FLAGS%` = `UF` from the connect-failure
//! final-outcome boolean (NOT from `%RESPONSE_CODE_DETAILS%`, which — like
//! the response body — carries the non-deterministic OS transport-failure
//! reason and is NOT logged / NOT compared). Spawns Envoy v1.33 in a
//! container; spawns envoy-rust as a subprocess; drives
//! `kind: http2_access_log_byte_exact` (reusing the phase-56 driver
//! verbatim); reads each side's file access-log and asserts the emitted
//! line is byte-identical:
//!   {"method":"GET","proto":"HTTP/2","rc":503,"rf":"UF"}
//! PURE cross-proxy equality (no static literal).

use std::path::PathBuf;

#[tokio::test]
async fn access_log_h2_uf_connect_failure() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0068-accesslog-h2-uf-connect-failure");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
