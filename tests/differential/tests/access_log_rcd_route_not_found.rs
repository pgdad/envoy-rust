//! Docker-gated differential test for fixture 0054-accesslog-rcd-route-not-found.
//! Phase 46 (ADR-0103) — the SECOND FAILURE-path `%RESPONSE_CODE_DETAILS%`
//! witness: `route_not_found`, BYTE-EXACT cross-proxy on the route-miss 404
//! path. A vhost (`domains: ["*"]`) with a SINGLE route matching only
//! `prefix: "/specific"` is probed with `GET /nomatch` → the request matches the
//! vhost but NO route → the route-walk's no-matching-route arm → the
//! deterministic 404 `route_not_found` synth (`synth_404`) at ROUTING time
//! (`clusters: []`; no backend spawns). envoy-rust now SETS
//! `%RESPONSE_CODE_DETAILS%` = `route_not_found` at its H1 no-matching-route
//! `synth_404` arm (`hcm.rs:1553`; was `None` → `null`); upstream Envoy v1.33
//! emits the same string here (state-1 recon:
//! `{"rc":404,"rcd":"route_not_found","rf":"NR"}`). Spawns Envoy v1.33 in a
//! container; spawns envoy-rust as a subprocess; drives
//! `kind: http1_access_log_byte_exact` (a `GET /nomatch` probe whose file
//! access-logger carries a `json_format` with `%RESPONSE_CODE%` /
//! `%RESPONSE_CODE_DETAILS%` / `%REQ(:METHOD)%` / `%PROTOCOL%`); reads each
//! side's file access-log and asserts the emitted JSON object is byte-identical:
//!   {"method":"GET","proto":"HTTP/1.1","rc":404,"rcd":"route_not_found"}
//! The assertion is PURE cross-proxy equality (no static literal — the driver
//! asserts byte-identity between the two proxies; the route-miss synth-404 is
//! deterministic on both sides).

use std::path::PathBuf;

#[tokio::test]
async fn access_log_rcd_route_not_found() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0054-accesslog-rcd-route-not-found");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
