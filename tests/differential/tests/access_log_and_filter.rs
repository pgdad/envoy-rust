//! Docker-gated differential test for fixture 0079-accesslog-and-filter.
//! Phase 73 (ADR-0152 / ADR-0153) — the FOURTH access-log FILTER witness (arm
//! #4): an `AccessLog` entry carrying `filter.and_filter.filters` gates the
//! sink's per-record emission on the boolean AND of its nested child predicates.
//! One HCM listener with a `text_format_source` file sink
//! (`STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)%`) filtered on
//! `and_filter { filters: [ header_filter{x-a=1}, header_filter{x-b=1} ] }`, and
//! ONE `direct_response` route (`/x` → 200 `hi`). NB the format renders only
//! STATUS+PATH — the composition gates on the `x-a`/`x-b` request headers (read
//! from the raw request-header slice), but the log LINE does not echo them
//! (envoy-rust's `%REQ(NAME)%` supports only an allow-list; `%REQ(X-A)%` is
//! boot-fatal). Two probes, kept-LAST (ADR-0147): (1) `GET /x` with `x-a:1` only
//! (AND false) → SUPPRESSED (`expect_logged: false`); (2) `GET /x` with
//! `x-a:1 x-b:1` (AND true) → KEPT. Each side's file holds EXACTLY ONE
//! byte-identical line `STATUS=200 PATH=/x`. `clusters: []`; no backend spawns.
//! PURE cross-proxy equality: both proxies must agree on the KEPT half AND the
//! DROPPED half.

use std::path::PathBuf;

#[tokio::test]
async fn access_log_and_filter() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0079-accesslog-and-filter");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
