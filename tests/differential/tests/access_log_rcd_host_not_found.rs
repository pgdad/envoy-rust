//! Docker-gated differential test for fixture 0055-accesslog-rcd-host-not-found.
//! Phase 47 (ADR-0104) — the THIRD FAILURE-path `%RESPONSE_CODE_DETAILS%`
//! witness: `route_not_found`, BYTE-EXACT cross-proxy on the HOST-miss
//! (no-matching-virtual_host) 404 path. A route table with a SINGLE vhost whose
//! NON-wildcard `domains: ["match.test"]` is probed with `GET /` carrying
//! `Host: nomatch.test` → the request matches NO vhost `domains` entry → the
//! route-walk's no-matching-virtual_host arm → the deterministic 404
//! `route_not_found` synth (`synth_404`) at the host-miss arm (`hcm.rs:1535`;
//! `clusters: []`; no backend spawns). envoy-rust now SETS
//! `%RESPONSE_CODE_DETAILS%` = `route_not_found` at its H1 no-matching-virtual_host
//! `synth_404` arm (`hcm.rs:1535`; was `None` → `null`); upstream Envoy v1.33
//! emits the same string here (state-1 recon:
//! `{"rc":404,"rcd":"route_not_found","rf":"NR"}`). Spawns Envoy v1.33 in a
//! container; spawns envoy-rust as a subprocess; drives
//! `kind: http1_access_log_byte_exact` (a `GET /` probe with `Host: nomatch.test`
//! whose file access-logger carries a `json_format` with `%RESPONSE_CODE%` /
//! `%RESPONSE_CODE_DETAILS%` / `%REQ(:METHOD)%` / `%PROTOCOL%`); reads each
//! side's file access-log and asserts the emitted JSON object is byte-identical:
//!   {"method":"GET","proto":"HTTP/1.1","rc":404,"rcd":"route_not_found"}
//! The assertion is PURE cross-proxy equality (no static literal — the driver
//! asserts byte-identity between the two proxies; the host-miss synth-404 is
//! deterministic on both sides). CONSUMES carry-forward M46-1.

use std::path::PathBuf;

#[tokio::test]
async fn access_log_rcd_host_not_found() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0055-accesslog-rcd-host-not-found");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
