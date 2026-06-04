//! Phase 20 (ADR-0051 SPEC) differential acceptance test for fixture
//! 0028-xds-file-based-rds — file-based RDS, the xDS-family continuation that
//! completes the CDS+LDS+RDS filesystem triad, bilaterally asserted. A STATIC
//! listener whose HCM is RDS-configured (NO inline route_config); the route
//! table exists ONLY because each proxy loaded the RDS file
//! (`rds.config_source.path_config_source.path`) at boot. Probe 1 (GET /static
//! → the static cluster) discriminates RDS-loaded from not-loaded independently
//! of CDS; probe 2 (GET /dynamic → the CDS cluster) proves the §5.7
//! cluster-before-route-revalidation composition (a request whose route table
//! AND cluster both arrive from dynamic-resource files).
//!
//! Docker-gated by the differential harness at the cluster level (no per-test
//! cfg gate; the harness skips when `DOCKER_HOST` is unavailable).

use std::path::PathBuf;

#[tokio::test]
async fn xds_file_based_rds_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0028-xds-file-based-rds");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
