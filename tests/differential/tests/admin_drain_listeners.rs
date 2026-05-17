//! Docker-gated differential test for fixture 0015-admin-drain-listeners.
//! Phase 08.2 D17.2 — first end-to-end bilateral assertion of the admin
//! `/drain_listeners` POST endpoint introduced in phase 08.2. Drives the
//! 08.2 D16 `Driver::AdminScrape` extensions (`pre_admin_actions` for
//! the drain trigger + `post_admin_assertions` for the wire-level
//! drain effect) against `tests/fixtures/0015-admin-drain-listeners/`.
//!
//! Sequence (Task 7 dispatch arm temporal order):
//!   1. pre_admin_actions:   POST /drain_listeners → 200
//!   2. scrapes:             GET /server_info → 200 (bilateral JSON shape;
//!      the /ready post-drain flip is asymmetric across envoy ↔ envoy-rust
//!      by default — see fixture README "Test driver" item 2 for why)
//!   3. post_admin_assertions: data_plane_connection_refused on the HCM
//!      listener (per-side template-rendered) within the 5s `DRAIN_BUDGET`.
//!
//! See README.md in the fixture directory + BEHAVIOR_CONTRACT.md
//! "Admin-action effect equivalence" subsection (Task 8 lands the
//! subsection alongside this wrapper).

use std::path::PathBuf;

#[tokio::test]
async fn admin_drain_listeners() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0015-admin-drain-listeners");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
