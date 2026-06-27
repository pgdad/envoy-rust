//! Docker-gated differential test for fixture 0047-accesslog-json-nested.
//! Phase 39 (ADR-0094) — first fixture exercising the RECURSIVE `json_format`
//! access-log output mode (nested objects + lists as `google.protobuf.Struct`
//! values). Spawns Envoy v1.33 in a container; spawns envoy-rust as a
//! subprocess; drives `kind: http1_access_log_byte_exact` (a `GET /` probe
//! against an H1 direct_response listener whose file access-logger carries a
//! NESTED `json_format` of deterministic operators); reads each side's file
//! access-log and asserts the emitted JSON object is byte-identical (keys sorted
//! at EVERY object level, list order preserved, at-depth typed values, compact
//! separators, trailing `\n` — ADR-0094 §A/§B/§C/§E/§H).

use std::path::PathBuf;

#[tokio::test]
async fn access_log_json_nested() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0047-accesslog-json-nested");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
