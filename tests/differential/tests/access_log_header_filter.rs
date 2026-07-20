//! Docker-gated differential test for fixture 0078-accesslog-header-filter.
//! Phase 72 (ADR-0148 / ADR-0149 / ADR-0150) — the THIRD access-log FILTER
//! witness: an `AccessLog` entry carrying `filter.header_filter.header` gates the
//! sink's per-record emission on whether a named REQUEST HEADER matches a
//! `HeaderMatcher`. One HCM listener with a `text_format_source` file sink
//! (`STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)%`) filtered on
//! `header_filter { header: { name: x-log, string_match: { exact: "yes" } } }`.
//! NB: the format renders only STATUS+PATH — the `header_filter` gates on the
//! `x-log` request header (read from the raw request-header slice), but the log
//! LINE does not echo it, because envoy-rust's `%REQ(NAME)%` operator supports
//! only an allow-list of headers (the record carries no arbitrary header map;
//! SPEC §2.2 — no new record field). The keep/drop decision is the witness,
//! and ONE `direct_response` route (`/x` → 200 `hi`) is probed twice:
//! (1) `GET /x` with `x-log: no` (present-MISMATCH) → SUPPRESSED
//! (`expect_logged: false`); (2) `GET /x` with `x-log: yes` (present-match) →
//! KEPT. The DROPPED probe is FIRST and the KEPT probe is LAST (kept-LAST sound
//! convention, ADR-0147). `clusters: []`; no backend spawns. Spawns Envoy v1.33
//! in a container; spawns envoy-rust as a subprocess; drives
//! `kind: http1_access_log_byte_exact`; reads each side's file access-log and
//! asserts every emitted line is byte-identical. Each file holds EXACTLY ONE
//! line (measured, ADR-0149 R-0.4 graceful-stop flush):
//!   STATUS=200 PATH=/x
//! PURE cross-proxy equality (no static literal): both proxies must agree on the
//! KEPT half AND the DROPPED half — a one-sided suppression fails the line-count
//! assertion before the byte compare is reached. H1-only (the H2 sink gate is
//! wired + unit-tested, but no H2 header_filter fixture yet).

use std::path::PathBuf;

#[tokio::test]
async fn access_log_header_filter() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0078-accesslog-header-filter");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
