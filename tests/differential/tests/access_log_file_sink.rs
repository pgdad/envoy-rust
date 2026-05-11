//! Docker-gated differential test for fixture 0012-access-log-file-sink.
//! Phase 06.2 D4.2.c — first access-log differential fixture. Spawns Envoy
//! v1.33 in a container; spawns envoy-rust as a subprocess; drives
//! `Driver::Http1WithAccessLog` (one `GET /` against the H1 direct_response
//! listener); reads each side's file access-log; diffs per-token per the
//! 15-rule cascade in `expectations.yaml` against the 14-row BEHAVIOR_CONTRACT.md
//! `Access log field mapping` table.

use std::path::PathBuf;

#[tokio::test]
async fn access_log_file_sink() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0012-access-log-file-sink");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
