//! Docker-gated differential test for fixture
//! 0084-headermatcher-absence-accesslog.
//!
//! Sub-phase 75.2 (ADR-0156 / ADR-0157 / ADR-0158 / ADR-0161) — the **D1**
//! cross-proxy witness for the `HeaderMatcher` ABSENCE rule on the ACCESS-LOG
//! path, i.e. the SECOND consumer of the shared matching engine, reached through
//! the ADR-0150 `HeaderMatch` trait seam (`LogFilter::Header { matcher }` in
//! `crates/envoy-accesslog/src/filter.rs` dispatches to
//! `impl envoy_accesslog::HeaderMatch for HeaderMatcher` in
//! `crates/envoy-config/src/matcher.rs`, whose trait object is injected by
//! `compile_access_log_filter` in `crates/envoy-http1/src/hcm.rs`). The ROUTE-path
//! witness of the same rule is fixture 0083; the D2 sibling is fixture 0085.
//!
//! Shape: one H1 HCM listener; ONE `FileAccessLog` sink with
//! `text_format_source` `STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)%`, gated by
//! `header_filter { header: { name: x-a, exact_match: "v", invert_match: true } }`;
//! ONE `direct_response` route `prefix: "/"` → 200 `hi`; `clusters: []`, no backend
//! spawns.
//!
//! THE MEASURED RULE (landed by sub-phase 75.1): `present_match(want)` is the ONLY
//! mode evaluated with the header ABSENT — `(present == want) ^ invert_match`.
//! EVERY value mode short-circuits to `false` when the header is absent, and
//! `invert_match` is NOT applied: upstream treats a missing header as an
//! unconditional value no-match that inversion does not resurrect. An EMPTY header
//! VALUE counts as PRESENT.
//!
//! Three probes, ordered so the LAST is KEPT (ADR-0147), **each on its own path**:
//! (1) `GET /absent` with NO `x-a` → **DROPPED — the D1 cell.** Before 75.1 the
//!     in-tree engine computed `false ^ true` = KEEP, so envoy-rust wrote TWO lines
//!     against upstream's ONE and this fixture would be RED. That is why 75.2 was
//!     gated behind 75.1.
//! (2) `GET /valmatch` with `x-a: v` → DROPPED (value matches, `invert_match` flips
//!     it to a drop). Together with probe 3 this pins both polarities of
//!     `invert_match` on a PRESENT header. It is NOT what makes probe 1's silence
//!     attributable — the line COUNT already does that, and the distinct paths now
//!     attribute the kept line directly.
//! (3) `GET /valmiss` with `x-a: zzz` → KEPT (value does not match, `invert_match`
//!     flips it to a keep).
//!
//! Each side's file holds EXACTLY ONE line, byte-identical ACROSS THE TWO PROXIES:
//! `STATUS=200 PATH=/valmiss`. Because the LAST probe is KEPT, the driver's
//! ordering-aware `suppression_settle` charges the cheap 2 s `CF70_3_SETTLE`
//! instead of the 12 s `CF71_1_SETTLE` (it inspects only `probes.last()`).
//!
//! THE DISTINCT PATHS ARE LOAD-BEARING (review finding I-1, closed at the §5.2
//! state-3 re-entry). This driver asserts only a per-side line COUNT plus
//! whole-line cross-proxy equality — no per-probe assertion, no expected-line
//! field. While all three probes shared `path: /x` they rendered the byte-identical
//! `STATUS=200 PATH=/x`, so an engine with the `invert_match` XOR removed left this
//! fixture GREEN (MEASURED; FOUR in-process assertions RED) because dropping the XOR
//! merely SWAPS which probe is kept and the count stays 1. With `PATH=` naming the
//! probe that mutation now REDs here. Do NOT collapse the paths.
//!
//! The line deliberately does NOT echo `x-a`: envoy-rust's `%REQ(NAME)%` operator
//! is ALLOW-LIST gated (`REQ_ALLOW_LIST`,
//! `crates/envoy-accesslog/src/command_operator.rs`), so `%REQ(X-A)%` would be
//! BOOT-FATAL. The witness is the keep/drop LINE COUNT plus whole-line
//! cross-proxy equality — the same design fixture 0078 uses.
//!
//! PURE cross-proxy equality: there is no static expected-line field on this
//! driver. Both proxies must agree on the kept line AND on the ABSENCE of a line
//! for each dropped probe; a one-sided keep fails the line-count assertion before
//! the byte compare is reached.

use std::path::PathBuf;

#[tokio::test]
async fn headermatcher_absence_accesslog() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0084-headermatcher-absence-accesslog");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
