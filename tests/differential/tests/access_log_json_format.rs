//! Docker-gated differential test for fixture 0046-accesslog-json-format.
//! Phase 38 (ADR-0092) — first fixture exercising the `json_format` access-log
//! output mode. Spawns Envoy v1.33 in a container; spawns envoy-rust as a
//! subprocess; drives `kind: http1_access_log_byte_exact` (a `GET /` probe
//! against an H1 direct_response listener whose file access-logger carries a
//! `json_format` map of deterministic operators); reads each side's file
//! access-log and asserts the emitted JSON object is byte-identical (sorted
//! keys, typed number/string/null values, compact separators, trailing `\n`).

use std::path::PathBuf;

#[tokio::test]
async fn access_log_json_format() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0046-accesslog-json-format");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
