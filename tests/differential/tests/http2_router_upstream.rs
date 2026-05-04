//! Docker-gated differential test for fixture 0010-http2-router-upstream.
//! Mirrors the 04.3-landed `tests/differential/tests/http1_router_upstream.rs`
//! and 05.2-landed `tests/differential/tests/http2_direct_response.rs`.
//! Spawns Envoy v1.33 in a container; spawns envoy-rust as a subprocess;
//! spawns http2-echo-server; drives a single H2C `GET /` request; asserts
//! byte-exact body equivalence under HEADER_ALLOW_LIST per SPEC §3 D7.

use std::path::PathBuf;

#[tokio::test]
async fn http2_router_upstream() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0010-http2-router-upstream");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
