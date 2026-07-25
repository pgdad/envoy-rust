//! Phase 75.1 differential acceptance test (ADR-0159): the `HeaderMatcher`
//! ABSENCE-SEMANTICS parity witness on the ROUTE path. Drives 22 HTTP/1.1
//! probes across EIGHT header matchers at a backend-free HCM listener
//! (`clusters: []`, `direct_response` only) and requires identical
//! (status, body, header-set-modulo-allow-list) between upstream Envoy
//! v1.33.0 and envoy-rust.
//!
//! This is the FIRST differential witness of `invert_match` AND of
//! `HeaderMatcher.present_match` in the whole fixture corpus. It pins three
//! things at once:
//!   * D1 (= CF-72-1, CLOSED here) — a VALUE matcher + `invert_match` + an
//!     ABSENT header DROPS; the shared engine KEPT it before this phase
//!     (probes p01 / p06 / p09, covering the literal, numeric and
//!     StringMatcher value paths).
//!   * D2 — upstream `present_match: false` means the header must be ABSENT.
//!     Probe p12 is the NON-inverted form: a plain, single-line matcher that
//!     silently matched every request in-tree before this phase.
//!   * P1, THE GUARD — `present_match: true` + `invert_match` + ABSENT is
//!     MEASURED PARITY (both proxies KEEP). Probe `p07-absent-keeps-GUARD`
//!     is load-bearing: a naive uniform "absent => DROP" fix of the shared
//!     engine passes every other probe here and fails only that one.
//!
//! Docker-gated, backend-free (no `{{BACKEND_PORT}}` marker → no backend
//! container spawns). The ACCESS-LOG-path witness for the same rule is
//! sub-phase 75.2 (fixtures 0084 + 0085).

use std::path::PathBuf;

#[tokio::test]
async fn headermatcher_absence_parity_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0083-headermatcher-absence-parity");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
