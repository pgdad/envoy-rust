//! Phase 115 differential acceptance test for fixture
//! `0095-http-filter-health-check`: `envoy.filters.http.health_check` in
//! non-pass-through mode.
//!
//! Ten HTTP/1.1 probes at a backend-free, CLUSTER-FREE HCM listener whose chain
//! is two health_check filters ahead of the router, with a `direct_response`
//! catch-all answering `MAIN`. Every probe answers 200, so the body decides: an
//! intercepted probe is empty, a fall-through is `MAIN`. The cells witness that
//! `:path` includes the query string, that exact matching is exact and
//! case-sensitive, that the filter is method-agnostic, that the
//! `x-envoy-upstream-healthchecked-cluster` value is the bootstrap
//! `node.cluster` rather than a request echo, and that a two-entry matcher list
//! folds as AND.
//!
//! `envoy.yaml` and `envoy-rust.yaml` are BYTE-IDENTICAL. Docker-gated and
//! backend-free, so fully verifiable on a developer host.

use std::path::PathBuf;

#[tokio::test]
async fn http_filter_health_check_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0095-http-filter-health-check");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
