//! Docker-gated differential test for fixture 0050-accesslog-response-code-details.
//! Phase 42 (ADR-0099) — first fixture exercising the `%RESPONSE_CODE_DETAILS%`
//! access-log command operator: it renders Envoy's response-code-details string
//! (an `Option<String>` shaped exactly like `%ROUTE_NAME%`/`%UPSTREAM_HOST%` —
//! present → the string; absent → the `-` sentinel in a multi-segment leaf, json
//! `null` in a single-operator-typed leaf). For a `direct_response` route the
//! value is the literal `direct_response`. Spawns Envoy v1.33 in a container;
//! spawns envoy-rust as a subprocess; drives `kind: http1_access_log_byte_exact`
//! (a `GET /` probe against an H1 `direct_response` listener whose file
//! access-logger carries a `json_format` with `%RESPONSE_CODE_DETAILS%` in a
//! single-op leaf and a mixed leaf); reads each side's file access-log and
//! asserts the emitted JSON object is byte-identical
//!   {"method":"GET","proto":"HTTP/1.1","rcd":"d=direct_response","single_rcd":"direct_response"}
//! (live-captured from envoyproxy/envoy:v1.33.0).

use std::path::PathBuf;

#[tokio::test]
async fn access_log_response_code_details() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0050-accesslog-response-code-details");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
