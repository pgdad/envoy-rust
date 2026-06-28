//! Docker-gated differential test for fixture 0057-accesslog-rf-no-healthy.
//! Phase 49 (ADR-0106) — the SECOND non-`-` `%RESPONSE_FLAGS%` witness: `UH`
//! (NoHealthyUpstream), BYTE-EXACT cross-proxy on the no-healthy-upstream 503
//! path. A NO_FALLBACK `lb_subset_config` cluster (`subset_selectors:
//! [{ keys: [stage] }]`) with a single route whose `metadata_match` selects the
//! NON-EXISTENT `stage: nonexistent` subset (the fixture-0038 `/nope` pattern)
//! → `pick()->None` → the deterministic 503 `no healthy upstream` synth at
//! ROUTING time (the literal `127.0.0.1:1` endpoint is never dialed; no backend
//! spawns). envoy-rust now DERIVES `%RESPONSE_FLAGS%` = `UH` from
//! `%RESPONSE_CODE_DETAILS%` = `no_healthy_upstream` at the H1 record-build site
//! (`hcm.rs:1232`; was the no-flags sentinel `"-"`); upstream Envoy v1.33 emits
//! the same flag here (state-0 recon: `{"rc":503,"rcd":"no_healthy_upstream",
//! "rf":"UH"}`). Spawns Envoy v1.33 in a container; spawns envoy-rust as a
//! subprocess; drives `kind: http1_access_log_byte_exact` (a `GET /` probe whose
//! file access-logger carries a `json_format` with `%RESPONSE_CODE%` /
//! `%RESPONSE_CODE_DETAILS%` / `%RESPONSE_FLAGS%` / `%REQ(:METHOD)%` /
//! `%PROTOCOL%`); reads each side's file access-log and asserts the emitted JSON
//! object is byte-identical:
//!   {"method":"GET","proto":"HTTP/1.1","rc":503,"rcd":"no_healthy_upstream","rf":"UH"}
//! PURE cross-proxy equality (no static literal — the no-healthy synth-503 is
//! deterministic on both sides). H1-only (H2 deferred — M45-1).

use std::path::PathBuf;

#[tokio::test]
async fn access_log_rf_no_healthy() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0057-accesslog-rf-no-healthy");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
