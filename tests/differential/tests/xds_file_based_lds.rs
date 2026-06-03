//! Phase 19 (ADR-0050 SPEC) differential acceptance test for fixture
//! 0027-xds-file-based-lds — Envoy's documented canonical LDS+CDS
//! filesystem-dynamic-config topology, bilaterally asserted. The bootstrap
//! carries ZERO static listeners; the listener exists ONLY because each proxy
//! loaded its dynamic_resources.lds_config.path_config_source.path at boot.
//! Probe 1 (GET /static → the static cluster) discriminates LDS-loaded from
//! not-loaded independently of CDS; probe 2 (GET /dynamic → the CDS cluster)
//! proves the §5.7 composition (a request whose listener AND cluster both
//! exist only in dynamic-resource files).
//!
//! Docker-gated by the differential harness at the cluster level (no per-test
//! cfg gate; the harness skips when `DOCKER_HOST` is unavailable).

use std::path::PathBuf;

#[tokio::test]
async fn xds_file_based_lds_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0027-xds-file-based-lds");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
