//! Docker-gated differential test for fixture 0076-accesslog-status-code-filter.
//! Phase 70 (ADR-0140 / ADR-0141) — the FIRST access-log FILTER witness: an
//! `AccessLog` entry carrying `filter.status_code_filter.comparison
//! { op: GE, value: { default_value: 500, runtime_key: unused } }` gates the
//! sink's per-record emission. One HCM listener with a `text_format_source`
//! file sink and TWO `direct_response` routes is probed twice: (1) `GET /log`
//! → 503, which MATCHES `GE 500` and IS logged; (2) `GET /nolog` → 200, which
//! misses the comparison and is SUPPRESSED (`expect_logged: false`, phase 70 —
//! it is excluded from the driver's line-count target). `%RESPONSE_FLAGS%`
//! renders `-` even on the 503: a `direct_response` is HCM-authored, so no
//! upstream failure flag applies (state-1 recon, ADR-0140). `clusters: []`; no
//! backend spawns. Spawns Envoy v1.33 in a container; spawns envoy-rust as a
//! subprocess; drives `kind: http1_access_log_byte_exact`; reads each side's
//! file access-log and asserts every emitted line is byte-identical. Each file
//! holds EXACTLY ONE line (measured):
//!   STATUS=503 PATH=/log FLAGS=-
//! PURE cross-proxy equality (no static literal): both proxies must agree on
//! the KEPT half AND the DROPPED half — a one-sided suppression fails the
//! line-count assertion before the byte compare is reached. H1-only (the H2
//! sink gate + driver arm are wired, but no H2 fixture suppresses yet).

use std::path::PathBuf;

#[tokio::test]
async fn access_log_status_code_filter() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0076-accesslog-status-code-filter");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
