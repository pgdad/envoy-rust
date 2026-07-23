//! Docker-gated differential test for fixture
//! 0082-accesslog-metadata-filter-key-not-found.
//! Phase 74 (ADR-0154 / ADR-0155) — the sibling of fixture 0081, witnessing the
//! `match_if_key_not_found` arm of the `metadata_filter` decision rule. Same
//! shape as 0081 (one HCM listener; an `envoy.filters.http.header_to_metadata`
//! filter mapping request header `x-a` into dynamic metadata `com.example:k`; a
//! `text_format_source` file sink
//! (`STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)% M=%DYNAMIC_METADATA(com.example:k)%`);
//! ONE `direct_response` route `/x` → 200 `hi`) PLUS
//! `match_if_key_not_found: false` on the filter.
//!
//! The `header_to_metadata` rule carries `on_header_present` ONLY — it
//! deliberately OMITS `on_header_missing`, so a request without `x-a` writes
//! NOTHING and `com.example:k` is genuinely ABSENT. (envoy-rust requires a
//! `value` on an `on_header_missing` block, as fixture 0042 supplies; carrying
//! one here would WRITE the key on the no-header probe, so the key would RESOLVE
//! and the probe would be dropped by the VALUE path — the fixture would pass
//! while silently vacating the `match_if_key_not_found` witness. ADR-0155 PV-6.)
//!
//! Two probes, kept-LAST (ADR-0147): (1) `GET /x` with NO `x-a` → the metadata
//! path does not resolve → `match_if_key_not_found: false` → SUPPRESSED;
//! (2) `GET /x` with `x-a: 1` → `k="1"` → the value matcher matches → KEPT. Each
//! side's file holds EXACTLY ONE byte-identical line `STATUS=200 PATH=/x M=1`.
//!
//! This witnesses the `google.protobuf.BoolValue` WRAPPER semantics that
//! `--mode validate` provably cannot reach (SPEC §0 R-0.2/R-0.4): under the
//! ABSENT default (`true`) the identical no-header probe was MEASURED as KEPT;
//! setting the field explicitly to `false` flips it to DROPPED.
//! `clusters: []`; no backend spawns. PURE cross-proxy equality.

use std::path::PathBuf;

#[tokio::test]
async fn access_log_metadata_filter_key_not_found() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0082-accesslog-metadata-filter-key-not-found");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
