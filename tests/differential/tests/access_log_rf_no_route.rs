//! Docker-gated differential test for fixture 0056-accesslog-rf-no-route.
//! Phase 48 (ADR-0105) — the FIRST non-`-` `%RESPONSE_FLAGS%` witness: `NR`
//! (NoRoute), BYTE-EXACT cross-proxy on the no-route 404 path. A route table
//! with a SINGLE NON-wildcard vhost `domains: ["match.test"]` (one `/specific`
//! direct_response route) is probed TWICE: (1) route-miss `GET /nomatch`
//! (`Host: match.test`) → the no-matching-route synth_404 arm (`hcm.rs:1555`);
//! (2) host-miss `GET /specific` (`Host: nomatch.test`) → the
//! no-matching-virtual_host synth_404 arm (`hcm.rs:1536`). Both are 404
//! `route_not_found` paths (`clusters: []`; no backend spawns). envoy-rust now
//! DERIVES `%RESPONSE_FLAGS%` = `NR` from `route_not_found` at the H1
//! record-build site (`hcm.rs:1225`; was the hard-coded `"-"`); upstream Envoy
//! v1.33 emits the same flag on both arms (state-1 recon:
//! `{"rc":404,"rcd":"route_not_found","rf":"NR"}`). Spawns Envoy v1.33 in a
//! container; spawns envoy-rust as a subprocess; drives
//! `kind: http1_access_log_byte_exact` (the json_format adds `rf:%RESPONSE_FLAGS%`);
//! reads each side's file access-log and asserts every emitted line is
//! byte-identical:
//!   {"method":"GET","proto":"HTTP/1.1","rc":404,"rcd":"route_not_found","rf":"NR"}
//! PURE cross-proxy equality (no static literal). H1-only (H2 deferred — M45-1).

use std::path::PathBuf;

#[tokio::test]
async fn access_log_rf_no_route() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0056-accesslog-rf-no-route");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
