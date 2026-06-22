//! Docker-gated differential test for fixture 0040-accesslog-command-operators.
//! Phase 32 Task 6 (ADR-0079) — first fixture exercising the CUSTOM access-log
//! `log_format` (command-operator formatter, phase 32 Tasks 1-5). Spawns Envoy
//! v1.33 in a container; spawns envoy-rust as a subprocess; drives
//! `Driver::Http1AccessLogByteExact` (a sequence of H1 `GET /` probes against
//! an H1 direct_response listener whose file access-logger carries a custom
//! deterministic-operators-only format); reads each side's file access-log and
//! asserts every emitted line is byte-identical (whole-line `==`, not the
//! per-token comparison fixture 0012 uses).

use std::path::PathBuf;

#[tokio::test]
async fn access_log_command_operators() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0040-accesslog-command-operators");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
