//! Docker-gated differential test for fixture 0048-accesslog-omit-empty.
//! Phase 40 (ADR-0096) — first fixture exercising the `omit_empty_values` knob:
//! when `true`, the command-operator engine renders an absent operator as the
//! EMPTY STRING `""` instead of the `-` sentinel in MULTI-SEGMENT values (it does
//! NOT drop keys; a single-operator-typed absent value stays `null`). Spawns
//! Envoy v1.33 in a container; spawns envoy-rust as a subprocess; drives
//! `kind: http1_access_log_byte_exact` (a `GET /` probe against an H1
//! direct_response listener whose file access-logger carries a `json_format`
//! with `omit_empty_values: true` and deterministic operators); reads each side's
//! file access-log and asserts the emitted JSON object is byte-identical
//! (the `-`→`""` swap on the multi-segment leaves, the `null` carve-out on the
//! single-op leaf, no key dropped — ADR-0096 §A/§B/§C).

use std::path::PathBuf;

#[tokio::test]
async fn access_log_omit_empty() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0048-accesslog-omit-empty");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
