//! Phase 13.2 D9.1-H2 differential acceptance test for fixture
//! 0021-upstream-h2-connection-pooling. Drives 5 sequential single-stream
//! GETs over ONE downstream H2 conn (Driver::Http2KeepAlive — landed at
//! 13.2 Task 5 per ADR-0039 topology pivot) to an H2 upstream cluster,
//! then asserts bilateral upstream_cx_total + upstream_cx_http2_total +
//! upstream_rq_total + downstream_rq_2xx + downstream_rq_total.
//!
//! Topology per ADR-0039 (the PLAN's HTTP1-downstream + H2-upstream-cluster
//! topology was rejected at parse time by the 06.3 D14.3 gate per
//! ADR-0028's H1-listener × H2-cluster dispatch deferral; this fixture
//! pivots downstream to HTTP2 so the value-exact bilateral discriminating
//! observable is preserved under a parse-valid configuration).
//!
//! Combined with 13.1's fixture 0020 (the H1 pool surface + the I2 (a)
//! closure) and 13.2 D7.1's BEHAVIOR_CONTRACT row tightening, this
//! fixture is the H2-pool-reuse half of the I2 (b) full closure surface.
//!
//! Docker-gated by the differential harness at the cluster level (no
//! per-test cfg gate; the harness skips when `DOCKER_HOST` is
//! unavailable). The harness wires `http2-echo-server` as backend via
//! the existing `{{HTTP2_BACKEND_PORT}}` template-marker scan
//! (`differential::run_fixture`'s `needs_http2_backend` block;
//! the 05.3 D6.b precedent — fixture 0010's wiring carries forward).

use std::path::PathBuf;

#[tokio::test]
async fn upstream_h2_connection_pooling_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0021-upstream-h2-connection-pooling");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
