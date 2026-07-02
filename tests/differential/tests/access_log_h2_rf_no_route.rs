//! Docker-gated differential test for fixture 0064-accesslog-h2-rf-no-route.
//! Phase 56 (ADR-0113) — the FIRST H2 access-log differential fixture in the
//! project. Opens `Driver::Http2AccessLogByteExact` (the H2 sibling of the
//! H1-only `Driver::Http1AccessLogByteExact`) and witnesses the FIRST H2
//! `%RESPONSE_FLAGS%` value, `NR` (NoRoute), byte-exact cross-proxy on BOTH
//! the route-miss and host-miss `synth_404` arms — the H2 analogue of
//! fixture 0056 (phase 48). `rc`/`rcd`/`proto`/`method` were already
//! byte-identical on H2 before this phase; the H2 record-build site's
//! previously hard-coded `response_flags: "-"` now derives `"NR"` from
//! `%RESPONSE_CODE_DETAILS%` = `route_not_found`. Spawns Envoy v1.33 in a
//! container; spawns envoy-rust as a subprocess; drives
//! `kind: http2_access_log_byte_exact`; reads each side's file access-log
//! and asserts every emitted line is byte-identical:
//!   {"method":"GET","proto":"HTTP/2","rc":404,"rcd":"route_not_found","rf":"NR"}
//! PURE cross-proxy equality (no static literal).

use std::path::PathBuf;

#[tokio::test]
async fn access_log_h2_rf_no_route() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0064-accesslog-h2-rf-no-route");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
