//! Phase 37 differential acceptance test: the RBAC `url_path` condition. Drive 3
//! path-varying GET probes through an HCM `[rbac, router]` chain whose RBAC
//! `action: ALLOW` single policy matches `url_path: { path: { exact: "/allowed" } }`:
//!   - probe 1: GET /allowed     -> match              -> 200 + "ok\n"
//!   - probe 2: GET /denied      -> no match           -> 403 + "RBAC: access denied"
//!   - probe 3: GET /allowed?x=1 -> query stripped (ADR-0090 §B) -> match -> 200 + "ok\n"
//!
//! Probe 3 is the load-bearing discriminator: a naive whole-:path compare would 403 it.
//! 403 body is "RBAC: access denied" (19 bytes, no newline, ADR-0034). LOCALLY
//! authoritative (no reload trigger). Docker-gated by the harness at the cluster level.

use std::path::PathBuf;

#[tokio::test]
async fn rbac_url_path() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0045-http-rbac-url-path");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
