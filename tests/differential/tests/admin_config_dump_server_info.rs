//! Docker-gated differential test for fixture 0014-admin-config-dump-server-info.
//! Phase 08.1 D17.1 — first end-to-end bilateral assertion of the 4 new GET
//! admin endpoints (`/config_dump`, `/server_info`, `/clusters`,
//! `/listeners`) against upstream Envoy v1.33. Drives the new
//! `Driver::AdminScrape { scrapes: [...] }` multi-case shape (08.1 Task 11)
//! against `tests/fixtures/0014-admin-config-dump-server-info/`.

use std::path::PathBuf;

#[tokio::test]
async fn admin_config_dump_server_info() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0014-admin-config-dump-server-info");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
