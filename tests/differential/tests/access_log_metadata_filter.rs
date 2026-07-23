//! Docker-gated differential test for fixture 0081-accesslog-metadata-filter.
//! Phase 74 (ADR-0154 / ADR-0155) — the SIXTH access-log FILTER witness (arm
//! #6) and the FIRST to gate a sink on DYNAMIC METADATA: an `AccessLog` entry
//! carrying `filter.metadata_filter` emits a record iff the request's dynamic
//! metadata, resolved at `matcher.filter` → `matcher.path[0].key`, matches
//! `matcher.value`. One HCM listener with an
//! `envoy.filters.http.header_to_metadata` filter mapping request header `x-a`
//! into dynamic metadata `com.example:k`, a `text_format_source` file sink
//! (`STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)% M=%DYNAMIC_METADATA(com.example:k)%`)
//! filtered on
//! `metadata_filter { matcher: { filter: com.example, path: [{key: k}], value: { string_match: { exact: "1" } } } }`,
//! and ONE `direct_response` route (`/x` → 200 `hi`). Unlike 0079/0080 the LINE
//! itself echoes the gating value — `%DYNAMIC_METADATA(...)%` is a distinct
//! command operator and is NOT gated by `REQ_ALLOW_LIST` (`%REQ(X-A)%` would be
//! boot-fatal). Two probes, kept-LAST (ADR-0147): (1) `GET /x` with `x-a: 2`
//! (metadata `k="2"` → value mismatch) → SUPPRESSED (`expect_logged: false`);
//! (2) `GET /x` with `x-a: 1` (metadata `k="1"` → value matches) → KEPT. Each
//! side's file holds EXACTLY ONE byte-identical line
//! `STATUS=200 PATH=/x M=1`. `clusters: []`; no backend spawns. PURE
//! cross-proxy equality: both proxies must agree on the KEPT half AND the
//! DROPPED half.

use std::path::PathBuf;

#[tokio::test]
async fn access_log_metadata_filter() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0081-accesslog-metadata-filter");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
