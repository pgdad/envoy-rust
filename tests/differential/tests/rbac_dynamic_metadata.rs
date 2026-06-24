//! Phase 35 differential acceptance test: the FIRST dynamic-metadata CONSUMER.
//! Drive 3 sequential GET / requests through an HCM whose `http_filters` chain
//! is `[envoy.filters.http.header_to_metadata, envoy.filters.http.rbac,
//! envoy.filters.http.router]`. The `header_to_metadata` PRODUCER extracts the
//! `x-tier` request header into dynamic metadata
//! `envoy.filters.http.header_to_metadata:tier`; the `rbac` CONSUMER (`action:
//! ALLOW`, one policy `tier_prod`) requires that metadata to string-match
//! `"prod"` via a `metadata` Permission. Producer-before-consumer chain order
//! is REQUIRED — the consumer reads what the producer wrote in the same decode
//! pass.
//!
//! The present-match + mismatch + absent probe TRIO is the anti-trivial guard:
//!   - probe 1: `x-tier: prod` -> metadata `tier=prod` -> ALLOW match -> 200 + `"ok\n"`
//!   - probe 2: `x-tier: dev`  -> metadata `tier=dev`  -> no match     -> 403 + `"RBAC: access denied"`
//!   - probe 3: (no `x-tier`)  -> key unset           -> no match     -> 403 + `"RBAC: access denied"`
//!
//! The two deny probes reach the SAME metadata-lookup path and FAIL the match
//! (this is not an allow-all). The 403 body is `"RBAC: access denied"`
//! (19 bytes, NO trailing newline) per phase-10 ADR-0034.
//!
//! Docker-gated by the differential harness at the cluster level (no per-test
//! cfg gate; the harness skips when `DOCKER_HOST` is unavailable). This fixture
//! is LOCALLY authoritative (no reload trigger — not Linux-CI-only).

use std::path::PathBuf;

#[tokio::test]
async fn rbac_dynamic_metadata() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0043-http-rbac-dynamic-metadata");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
