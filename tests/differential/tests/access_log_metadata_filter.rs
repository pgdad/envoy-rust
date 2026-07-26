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
//! boot-fatal).
//!
//! `match_if_key_not_found` is deliberately ABSENT from this fixture's filter, so
//! an unresolved key takes its MEASURED default `true` (SPEC §0 R-0.4) — the
//! proto3 `google.protobuf.BoolValue` default that `--mode validate` provably
//! cannot reach. The `header_to_metadata` rule carries `on_header_present` ONLY,
//! deliberately OMITTING `on_header_missing`, so a request without `x-a` writes
//! nothing and the key is genuinely absent (ADR-0155 PV-6 — adding the block
//! would make the key RESOLVE and silently vacate the default-`true` witness
//! while this test stayed green).
//!
//! THREE probes, kept-LAST (ADR-0147): (1) `GET /x` with `x-a: 2` (metadata
//! `k="2"` → resolved, value mismatch) → SUPPRESSED (`expect_logged: false`);
//! (2) `GET /x` with NO `x-a` (key unresolved → the `match_if_key_not_found`
//! DEFAULT `true`) → KEPT; (3) `GET /x` with `x-a: 1` (metadata `k="1"` → value
//! matches) → KEPT. The LAST probe is KEPT, so the driver's ordering-aware
//! `suppression_settle` — which inspects only `probes.last()` — pays the cheap
//! 2 s `CF70_3_SETTLE` rather than the 12 s `CF71_1_SETTLE`. What placing probe 2
//! SECOND buys is separate: it pins the LINE ORDER (`M=-` before `M=1`).
//!
//! Each side's file holds EXACTLY TWO byte-identical lines, in this order:
//! `STATUS=200 PATH=/x M=-` then `STATUS=200 PATH=/x M=1`. They are byte-DISTINCT,
//! so the fixture pins line ORDER as well as count. `clusters: []`; no backend
//! spawns. PURE cross-proxy equality: both proxies must agree on both KEPT lines
//! AND on the DROPPED one.

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
