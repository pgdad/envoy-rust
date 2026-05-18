//! Phase 10 differential acceptance test: drive 4 sequential GET / requests
//! through an HCM whose `http_filters` chain is
//! `[envoy.filters.http.rbac, envoy.filters.http.router]` under `action:
//! ALLOW` with a single policy `pass_with_header` requiring the
//! `x-rbac-pass: yes` request header. Both proxies must produce the
//! deterministic status sequence `[403, 200, 403, 200]`; the 2 deny
//! probes (statuses 1 + 3) carry the upstream-Envoy-parity body
//! `"RBAC: access denied"` (19 bytes, source-hardcoded on upstream
//! Envoy v1.33's `envoy.extensions.filters.http.rbac.v3.RBAC`; envoy-rust
//! matches per phase-10 ADR-0034). Docker-gated by the differential
//! harness at the cluster level (no per-test cfg gate; the harness skips
//! when `DOCKER_HOST` is unavailable).
//!
//! This is the FIRST non-LocalRateLimit bilateral consumer of the H1 HCM
//! `decorate_filter_synth_response` helper landed at phase-09 ADR-0033
//! Commit C `ae2cef0` (`crates/envoy-http1/src/hcm.rs:932`). The 2 deny
//! probes engage the helper end-to-end against both proxies; the 2 allow
//! probes (statuses 2 + 4) bypass the helper and pass through to the
//! direct_response route, demonstrating that the helper is filter-agnostic
//! by design.

use std::path::PathBuf;

#[tokio::test]
async fn http_filter_rbac_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0017-http-filter-rbac");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
