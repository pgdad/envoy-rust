//! Docker-gated differential test for fixture 0049-accesslog-route-name.
//! Phase 41 (ADR-0098) — first fixture exercising the `%ROUTE_NAME%` access-log
//! command operator: it renders the matched route's config `name` (an
//! `Option<String>` shaped exactly like `%UPSTREAM_HOST%` — present → the name;
//! absent/unnamed → the `-` sentinel in a multi-segment leaf, json `null` in a
//! single-operator-typed leaf). Spawns Envoy v1.33 in a container; spawns
//! envoy-rust as a subprocess; drives `kind: http1_access_log_byte_exact` (a
//! `GET /` probe against an H1 direct_response listener whose route is NAMED
//! (`name: myroute`) and whose file access-logger carries a `json_format` with
//! `%ROUTE_NAME%` in a single-op leaf and a mixed leaf); reads each side's file
//! access-log and asserts the emitted JSON object is byte-identical
//!   {"method":"GET","proto":"HTTP/1.1","rn":"r=myroute","single_rn":"myroute"}
//! (live-captured from envoyproxy/envoy:v1.33.0).

use std::path::PathBuf;

#[tokio::test]
async fn access_log_route_name() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0049-accesslog-route-name");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
