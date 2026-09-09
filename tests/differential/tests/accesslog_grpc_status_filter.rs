//! Fixture 0094 — the `grpc_status_filter` access-log FILTER arm (phase 114),
//! the SEVENTH of the twelve `AccessLogFilter` oneof arms.
//!
//! Cluster-free and backend-free (`clusters: []`, every route a
//! `direct_response`), so it is verifiable on a development host rather than
//! CI-only. Eight probes on eight distinct paths; five are kept.
//!
//! ⚠ Probes 5, 6 and 8 are what make this fixture non-vacuous. They are
//! requests the phase-113 `%GRPC_STATUS%` gate renders `-` for, and the filter
//! keeps them anyway — so an implementation that reuses
//! `AccessLogRecord.grpc_status` goes RED on all three.

use std::path::PathBuf;

#[tokio::test]
async fn accesslog_grpc_status_filter() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0094-accesslog-grpc-status-filter");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
