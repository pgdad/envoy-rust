//! Docker-gated differential test for fixture
//! 0085-headermatcher-absence-accesslog-present-polarity.
//!
//! Sub-phase 75.2 (ADR-0156 / ADR-0157 / ADR-0158 / ADR-0161) — the **D2**
//! cross-proxy witness for the `HeaderMatcher` ABSENCE rule on the ACCESS-LOG
//! path, and the sibling of fixture 0084 (which witnesses D1). Two fixtures
//! rather than one is a MEASURED constraint, not a preference: the byte-exact
//! access-log driver takes exactly ONE log file per side (`AccessLogPaths` in
//! `tests/differential/src/lib.rs` is two `String` fields under
//! `deny_unknown_fields`, and only the envoy-side parent dir is bind-mounted), so
//! one sink per fixture is the only shape available — ADR-0158. This mirrors the
//! existing sibling pair 0081 / 0082.
//!
//! Shape: one H1 HCM listener; ONE `FileAccessLog` sink with
//! `text_format_source` `STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)%`, gated by
//! `header_filter { header: { name: x-a, present_match: false } }` — a plain,
//! NON-inverted, single-line matcher; ONE `direct_response` route `/x` → 200
//! `hi`; `clusters: []`, no backend spawns.
//!
//! THE MEASURED RULE (landed by sub-phase 75.1): upstream `present_match: false`
//! means **the header must be ABSENT** — `(present == want) ^ invert_match`.
//! Before 75.1 the in-tree engine modelled this arm as UNCONDITIONALLY TRUE, so
//! the matcher silently matched every request here and only header-absent
//! requests upstream. **D2 is strictly worse than D1** because it needs no
//! `invert_match` to fire, and before phase 75 it had NO behavioral test anywhere
//! in the tree.
//!
//! Two probes, ordered so the LAST is KEPT (ADR-0147):
//! (1) `GET /x` with `x-a: v` → **DROPPED — the D2 cell.** `(true == false)` is
//!     false. A pre-75.1 tree KEPT it, writing TWO lines against upstream's ONE.
//! (2) `GET /x` with NO `x-a` → KEPT. `(false == false)` is true.
//!
//! Each side's file holds EXACTLY ONE line, byte-identical ACROSS THE TWO
//! PROXIES: `STATUS=200 PATH=/x`. Because the LAST probe is KEPT, the driver's
//! ordering-aware `suppression_settle` charges the cheap 2 s `CF70_3_SETTLE`
//! instead of the 12 s `CF71_1_SETTLE` (it inspects only `probes.last()`).
//!
//! CONFLATION TRAP — `HeaderMatcher.present_match` (this fixture) and
//! `ValueMatcher.present_match` (RBAC / access-log METADATA, fixture 0044) are
//! DIFFERENT fields on DIFFERENT messages with DIFFERENT measured rules. For the
//! `ValueMatcher` one, `present_match: false` NEVER matches — that rule is
//! CORRECT and must NOT be "fixed" to match this one. After 75.1 the two agree in
//! three of four cells and differ in exactly one: ABSENT with `want = false`,
//! where `ValueMatcher` yields `false` and `HeaderMatcher` yields `true`.
//!
//! The line deliberately does NOT echo `x-a`: `%REQ(NAME)%` is ALLOW-LIST gated in
//! envoy-rust, so `%REQ(X-A)%` would be BOOT-FATAL. PURE cross-proxy equality:
//! both proxies must agree on the kept line AND on the ABSENCE of a line for the
//! dropped probe.

use std::path::PathBuf;

#[tokio::test]
async fn headermatcher_absence_accesslog_present_polarity() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0085-headermatcher-absence-accesslog-present-polarity");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
