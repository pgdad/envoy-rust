//! Phase 23 differential acceptance test for fixture 0031-http-filter-cors.
//! Drives 4 sequential HTTP/1.1 requests (`Host: cors.test`) over an HTTP/1.1
//! listener through an HCM whose `http_filters` chain is
//! `[envoy.filters.http.cors, envoy.filters.http.router]` with a `CorsPolicy`
//! attached via `typed_per_filter_config` to the single `prefix: "/"` route
//! (proxying to the real `http1-echo-server` upstream cluster per ADR-0058 L6).
//!
//! Both proxies must produce the deterministic status sequence `[200, 200, 200, 200]`:
//!
//!   1. preflight  — OPTIONS + allowed origin + ACRM → 200, empty body, CORS headers
//!   2. allowed    — GET + allowed origin → 200, echo body, access-control-allow-origin
//!   3. evil       — GET + disallowed origin → 200, echo body, no access-control-*
//!   4. no-origin  — GET (no Origin) → 200, echo body, no access-control-*
//!
//! L6 lock-in (ADR-0058): a `direct_response` route MUST NOT be used — Envoy's
//! CORS filter does not engage on `direct_response` routes, making the
//! differential trivially-and-incorrectly green. This fixture uses a real
//! upstream cluster (`http1-echo-server` helper, `{{BACKEND_HOST}}` / `{{HTTP1_BACKEND_PORT}}`).
//!
//! The `access-control-*` response header values are a pure function of the
//! CorsPolicy + the request Origin, so they are byte-exact cross-proxy and are
//! NOT on the harness allow-list (BEHAVIOR_CONTRACT.md). `set_equal_modulo_allow_list`
//! confirms value-exact parity for present headers and name-set parity for
//! absent headers (probes 3/4).
//!
//! Docker-gated by the differential harness at the cluster level (no per-test
//! cfg gate; the harness skips when `DOCKER_HOST` is unavailable).

use std::path::PathBuf;

#[tokio::test]
async fn http_filter_cors_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0031-http-filter-cors");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
