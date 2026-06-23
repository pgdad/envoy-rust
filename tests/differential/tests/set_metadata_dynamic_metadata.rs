//! Docker-gated differential test for fixture
//! 0041-http-set-metadata-dynamic-metadata. Phase 33 Task 11 (ADR-0080 / -0081)
//! — the witness for the smallest end-to-end dynamic-metadata loop: the
//! `envoy.filters.http.set_metadata` filter writes a static value into a
//! per-request dynamic-metadata store and the
//! `%DYNAMIC_METADATA(namespace:key)%` access-log command-operator reads it
//! back. Spawns Envoy v1.33 in a container; spawns envoy-rust as a subprocess;
//! drives `Driver::Http1AccessLogByteExact` (present-key + absent-key/namespace
//! probes against an H1 direct_response listener whose `[set_metadata, router]`
//! chain feeds a file access-logger); reads each side's file access-log and
//! asserts every emitted line is byte-identical (whole-line `==`).

use std::path::PathBuf;

#[tokio::test]
async fn set_metadata_dynamic_metadata() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0041-http-set-metadata-dynamic-metadata");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
