//! Fixture 0099 — the `not_health_check_filter` access-log FILTER arm (phase
//! 116), the EIGHTH of the twelve `AccessLogFilter` oneof arms.
//!
//! Cluster-free and backend-free (`clusters: []`, one `direct_response`
//! route), so it is verifiable on a development host rather than CI-only. Six
//! probes; two suppressed, four kept.
//!
//! ⚠ Probes 5 and 6 are what make this fixture non-vacuous against the
//! plausible wrong implementations. Probe 5 is a `/healthz` that RBAC answers
//! before the health_check filter runs; probe 6 carries upstream's
//! health-checker user-agent. Both are KEPT, so an arm keyed on the request
//! instead of the filter's decision goes RED.

use std::path::PathBuf;

#[tokio::test]
async fn accesslog_not_health_check_filter() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0099-accesslog-not-health-check-filter");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
