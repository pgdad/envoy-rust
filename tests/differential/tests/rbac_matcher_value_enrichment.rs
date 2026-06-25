//! Phase 36 differential acceptance test: RBAC matcher-VALUE enrichment.
//! Drive 4 sequential GET / probes through an HCM whose http_filters chain is
//! `[header_to_metadata, rbac, router]`, exercising BOTH phase-36 features:
//! F1 `present_match` (probes c/d) and F2 `safe_regex` (probes a/b), all
//! ANCHORED `^(prod|staging)$` per ADR-0088 §A3b. The rbac consumer has two
//! OR'd ALLOW policies (f2_regex on metadata `tier`; f1_present on metadata
//! `present_probe`); each probe's headers select which policy fires. Byte-exact
//! cross-proxy: match -> 200 + "ok\n"; miss -> 403 + "RBAC: access denied" (19B).
//! Docker-gated by the differential harness at the cluster level.

use std::path::PathBuf;

#[tokio::test]
async fn rbac_matcher_value_enrichment() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0044-http-rbac-matcher-value-enrichment");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
