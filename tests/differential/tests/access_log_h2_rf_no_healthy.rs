//! Docker-gated differential test for fixture 0065-accesslog-h2-rf-no-healthy.
//! Phase 57 (ADR-0114) — the SECOND H2 `%RESPONSE_FLAGS%` witness: `UH`
//! (NoHealthyUpstream), byte-exact cross-proxy on the H2 `pick()->None`
//! no-healthy-upstream 503 path — the H2 analogue of fixture `0057` (phase
//! 49). Also corrects envoy-rust's H2 no-healthy synth status 502 -> 503 to
//! match Envoy (the dedicated `synth_h2_no_healthy_upstream()` helper,
//! mirroring the H1 `synth_no_healthy_upstream` precedent). A NO_FALLBACK
//! `lb_subset_config` cluster (`subset_selectors: [{ keys: [stage] }]`) with
//! a single route whose `metadata_match` selects the NON-EXISTENT `stage:
//! nonexistent` subset (the fixture-0038/0057 pattern) -> `pick()->None` ->
//! the deterministic 503 `no healthy upstream` synth at ROUTING time (the
//! literal `127.0.0.1:1` endpoint is never dialed; no backend spawns).
//! Spawns Envoy v1.33 in a container; spawns envoy-rust as a subprocess;
//! drives `kind: http2_access_log_byte_exact` (reusing the phase-56 driver
//! verbatim); reads each side's file access-log and asserts the emitted line
//! is byte-identical:
//!   {"method":"GET","proto":"HTTP/2","rc":503,"rcd":"no_healthy_upstream","rf":"UH"}
//! PURE cross-proxy equality (no static literal).

use std::path::PathBuf;

#[tokio::test]
async fn access_log_h2_rf_no_healthy() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0065-accesslog-h2-rf-no-healthy");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
