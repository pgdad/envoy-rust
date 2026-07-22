//! Docker-gated differential test for fixture 0080-accesslog-or-filter.
//! Phase 73 (ADR-0152 / ADR-0153) — the FIFTH access-log FILTER witness (arm #5)
//! AND the depth-2 recursion witness: an `AccessLog` entry carrying
//! `filter.or_filter.filters` gates the sink's per-record emission on the boolean
//! OR of its nested child predicates, where one child is ITSELF an `and_filter`
//! (depth-2, SPEC §0 R-0.5). One HCM listener with a `text_format_source` file
//! sink (`STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)%`) filtered on
//! `or_filter { filters: [ and_filter{[x-a=1, x-b=1]}, header_filter{x-c=1} ] }`,
//! and ONE `direct_response` route (`/x` → 200 `hi`). NB the format renders only
//! STATUS+PATH — the composition gates on the `x-a`/`x-b`/`x-c` request headers,
//! but the log LINE does not echo them (envoy-rust's `%REQ(NAME)%` supports only
//! an allow-list; `%REQ(X-A)%` is boot-fatal). Three probes, kept-LAST
//! (ADR-0147): (1) `GET /x` with `x-a:1` only (nested AND false, leaf false → OR
//! false) → SUPPRESSED; (2) `x-a:1 x-b:1` (nested AND true → OR true) → KEPT;
//! (3) `x-c:1` (leaf true → OR true) → KEPT. Each side's file holds EXACTLY TWO
//! byte-identical lines `STATUS=200 PATH=/x`. `clusters: []`; no backend spawns.
//! PURE cross-proxy equality: both proxies must agree on the KEPT lines AND the
//! DROPPED absence — witnessing OR-of-(nested-AND, leaf) recursion.

use std::path::PathBuf;

#[tokio::test]
async fn access_log_or_filter() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0080-accesslog-or-filter");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
