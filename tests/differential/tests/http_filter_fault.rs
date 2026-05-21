//! Phase 11 differential acceptance test for fixture 0018-http-filter-fault.
//! Drives 4 sequential `GET /` requests over an HTTP/2 listener through an HCM
//! whose `http_filters` chain is
//! `[envoy.filters.http.fault, envoy.filters.http.router]`. The fault filter
//! aborts (503 + body `"fault filter abort"`) any request carrying
//! `x-fault: abort`; other requests pass through to the direct_response 200.
//! Both proxies must produce the deterministic status sequence
//! `[503, 200, 503, 200]`.
//!
//! This is the FIRST HTTP-filter-family fixture on an H2 listener. The 2 abort
//! probes (statuses 1 + 3) exercise the phase-11 D6
//! `decorate_filter_synth_response_h2` helper (Task 4) end-to-end against both
//! proxies — the bilateral demonstration that the 09 REVIEW M2 close is real
//! (the H2 abort header set `{server, content-length, content-type, date}` —
//! NO `connection`, which is H2-forbidden — must match upstream Envoy under
//! `set_equal_modulo_allow_list`). The 2 pass-through probes (statuses 2 + 4)
//! bypass the helper and route to the direct_response.
//!
//! Docker-gated by the differential harness at the cluster level (no per-test
//! cfg gate; the harness skips when `DOCKER_HOST` is unavailable).

use std::path::PathBuf;

#[tokio::test]
async fn http_filter_fault_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0018-http-filter-fault");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
