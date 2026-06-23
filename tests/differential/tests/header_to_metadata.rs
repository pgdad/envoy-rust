//! Docker-gated differential test for fixture
//! 0042-http-header-to-metadata. Phase 34 Task 6 (ADR-0083 / -0084)
//! — the witness for the header_to_metadata HTTP filter: the
//! `envoy.filters.http.header_to_metadata` filter reads an incoming
//! request header (`x-tier`) and writes its value (or a configured
//! fallback) into the per-request dynamic-metadata store under a
//! configured namespace/key. The `%DYNAMIC_METADATA(envoy.lb:tier)%`
//! access-log command-operator reads it back. Spawns Envoy v1.33 in a
//! container; spawns envoy-rust as a subprocess; drives
//! `Driver::Http1AccessLogByteExact` (header-present + header-missing
//! probes against an H1 `direct_response` listener whose
//! `[header_to_metadata, router]` chain feeds a file access-logger);
//! reads each side's file access-log and asserts every emitted line is
//! byte-identical (whole-line `==`).

use std::path::PathBuf;

#[tokio::test]
async fn header_to_metadata() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0042-http-header-to-metadata");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
