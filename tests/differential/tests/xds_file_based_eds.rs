//! Phase 21 (ADR-0053 SPEC / ADR-0054 reconciliation) differential acceptance
//! test for fixture 0029-xds-file-based-eds — file-based EDS, the xDS-family
//! 4th member that completes the CDS+LDS+RDS+EDS filesystem set, bilaterally
//! asserted. A STATIC cluster `eds_backend` declared `type: EDS` with NO inline
//! `load_assignment`; its endpoints exist ONLY because each proxy loaded the EDS
//! file (`eds_cluster_config.eds_config.path_config_source.path`) at boot. The
//! probe (GET / -> the EDS-supplied cluster) is the data-plane witness:
//! endpoints-loaded vs not (a cluster with no endpoints would 503). Verifies the
//! per-cluster `cluster.eds_backend.update_*` stat subset and the
//! `/config_dump?include_eds` `EndpointsConfigDump` (static_endpoint_configs,
//! per-side index).
//!
//! Docker-gated by the differential harness at the cluster level (no per-test
//! cfg gate; the harness skips when `DOCKER_HOST` is unavailable). The EDS
//! backend address is a numeric IP rendered per-side (L1/L9): the Envoy side
//! uses the runtime-discovered host-gateway IP, envoy-rust uses 127.0.0.1.

use std::path::PathBuf;

#[tokio::test]
async fn xds_file_based_eds_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0029-xds-file-based-eds");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
