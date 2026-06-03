//! Phase 18 (ADR-0048 SPEC / ADR-0049 PLAN) differential acceptance test for
//! fixture 0026-xds-file-based-cds — the xDS-family opener and the bilateral
//! file-based-CDS proof of the phase. ONE H1 GET over a downstream keep-alive
//! conn (Driver::Http1KeepAlive) routes through a cluster (`dynamic_backend`)
//! that exists ONLY because each proxy loaded its
//! `dynamic_resources.cds_config.path_config_source.path` (a `.yaml` CDS file,
//! the bare `resources:` envelope with an `@type`-tagged Cluster payload, L1)
//! at boot:
//!
//!   GET / -> routed to `dynamic_backend` (CDS-supplied, absent from
//!   static_resources — the static `clusters:` key is OMITTED entirely, L7) ->
//!   backend 200. A proxy that ignored `dynamic_resources` would have no
//!   `dynamic_backend` cluster and 503 the route (the L6 data-plane
//!   discriminator); the dynamic cluster is structurally identical to fixture
//!   0008's cluster shape (the same STRICT_DNS/ROUND_ROBIN/V4_ONLY cluster
//!   moved into the CDS file).
//!
//! Bilateral assertions: the 200 status, the per-request `expected_body` that
//! asserts the echoed body byte-exact on EACH side independently (with
//! identical request inputs — the `request_headers_to_remove` machinery strips
//! the proxy-injected headers — this pins the data-plane wire shape), the
//! conditional `cluster_manager.cds.{update_attempt,
//! update_success,update_failure,update_rejected}` / `cluster_added` /
//! `active_clusters` counters with values 1/1/0/0/1/1 (single dynamic cluster,
//! zero static — L3), the per-cluster `cluster.dynamic_backend.upstream_{rq,cx}_
//! total` + the HCM `http.ingress_http1.downstream_rq_{total,2xx}` counters, and
//! a `/config_dump` json_shape sub-case (Http1KeepAlive::admin_scrapes) proving
//! the conditional `ClustersConfigDump` at `configs[1]` carries
//! `dynamic_active_clusters[0].cluster.name == "dynamic_backend"` on BOTH sides
//! (L5).
//!
//! Envoy prerequisites encoded in the fixture configs (ADR-0049 lock-ins):
//! `node: { id, cluster }` is REQUIRED when CDS is configured (L12a — Envoy
//! exits otherwise); `validate_clusters: false` is REQUIRED on the route_config
//! (L12b — else Envoy exits with "route: unknown cluster" because the
//! CDS-supplied cluster is not present at config-load time).
//!
//! The backend is the http1-echo-server helper, spawned by the harness keyed on
//! the fixture's `{{HTTP1_BACKEND_PORT}}` marker (which fixture 0026 places ONLY
//! in `cds.yaml`; the harness's backend-launch detection scans the CDS template
//! too — Task 6). The upstream rendition of `cds.yaml` is mounted into the Envoy
//! container at `/etc/envoy-cds/cds.yaml`; the subject reads a host temp path.
//!
//! Docker-gated by the differential harness at the cluster level (no per-test
//! cfg gate; the harness skips when `DOCKER_HOST` is unavailable).

use std::path::PathBuf;

#[tokio::test]
async fn xds_file_based_cds_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0026-xds-file-based-cds");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
