//! Docker-gated differential test for fixture 0053-accesslog-rcd-no-healthy.
//! Phase 45 (ADR-0102) — the FIRST FAILURE-path `%RESPONSE_CODE_DETAILS%`
//! witness: `no_healthy_upstream`, BYTE-EXACT cross-proxy on the
//! no-healthy-upstream 503 path. A NO_FALLBACK `lb_subset_config` cluster
//! (`subset_selectors: [{ keys: [stage] }]`) with a single route whose
//! `metadata_match` selects the NON-EXISTENT `stage: nonexistent` subset (the
//! fixture-0038 `/nope` pattern) → `pick()->None` → the deterministic 503
//! `no healthy upstream` synth at ROUTING time (the literal `127.0.0.1:1`
//! endpoint is never dialed; no backend spawns). envoy-rust now SETS
//! `%RESPONSE_CODE_DETAILS%` = `no_healthy_upstream` at its H1 no-healthy synth
//! arm (was `None` → `null`); upstream Envoy v1.33 emits the same string here
//! (state-1 recon: `{"rc":503,"rcd":"no_healthy_upstream","rf":"UH"}`). Spawns
//! Envoy v1.33 in a container; spawns envoy-rust as a subprocess; drives
//! `kind: http1_access_log_byte_exact` (a `GET /` probe whose file access-logger
//! carries a `json_format` with `%RESPONSE_CODE%` / `%RESPONSE_CODE_DETAILS%` /
//! `%REQ(:METHOD)%` / `%PROTOCOL%`); reads each side's file access-log and
//! asserts the emitted JSON object is byte-identical:
//!   {"method":"GET","proto":"HTTP/1.1","rc":503,"rcd":"no_healthy_upstream"}
//! The assertion is PURE cross-proxy equality (no static literal — the driver
//! asserts byte-identity between the two proxies; the no-healthy synth-503 is
//! deterministic on both sides).

use std::path::PathBuf;

#[tokio::test]
async fn access_log_rcd_no_healthy() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0053-accesslog-rcd-no-healthy");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
