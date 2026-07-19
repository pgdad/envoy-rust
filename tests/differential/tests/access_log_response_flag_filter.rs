//! Docker-gated differential test for fixture 0077-accesslog-response-flag-filter.
//! Phase 71 (ADR-0144 / ADR-0145) — the SECOND access-log FILTER witness: an
//! `AccessLog` entry carrying `filter.response_flag_filter.flags: [NR]` gates the
//! sink's per-record emission on the record's single `%RESPONSE_FLAGS%` token.
//! One HCM listener with a `text_format_source` file sink and ONE `direct_response`
//! route is probed twice: (1) `GET /direct` → 503, a HCM-authored direct_response
//! whose `%RESPONSE_FLAGS%` is `-` (∉ [NR]) → SUPPRESSED (`expect_logged: false`);
//! (2) `GET /nowhere` → 404, a no-route synth whose `%RESPONSE_FLAGS%` is `NR`
//! (∈ [NR]) → KEPT. The suppressed probe is FIRST and the kept probe is LAST
//! (CF-70-3 ordering witness). `clusters: []`; no backend spawns. Spawns Envoy
//! v1.33 in a container; spawns envoy-rust as a subprocess; drives
//! `kind: http1_access_log_byte_exact`; reads each side's file access-log and
//! asserts every emitted line is byte-identical. Each file holds EXACTLY ONE
//! line (measured, ADR-0145 PV-6):
//!   STATUS=404 PATH=/nowhere FLAGS=NR
//! PURE cross-proxy equality (no static literal): both proxies must agree on
//! the KEPT half AND the DROPPED half — a one-sided suppression fails the
//! line-count assertion before the byte compare is reached. H1-only (the H2
//! sink gate is wired + unit-tested, but no H2 fixture suppresses yet).

use std::path::PathBuf;

#[tokio::test]
async fn access_log_response_flag_filter() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0077-accesslog-response-flag-filter");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
