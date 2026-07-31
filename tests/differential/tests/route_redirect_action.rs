//! Sub-phase 76.2 differential acceptance test: the `Route.redirect` action.
//! Drives 18 HTTP/1.1 probes at a backend-free HCM listener (`clusters: []`,
//! `redirect:` routes only) and requires identical (status, body,
//! header-set-modulo-allow-list) between upstream Envoy v1.33.0 and envoy-rust.
//!
//! This is the FIRST differential witness of `Route.redirect` in the corpus. It
//! pins the whole `location` construction rule set measured against
//! `envoyproxy/envoy:v1.33.0`:
//!   * the AUTHORITY ASYMMETRY — `host_redirect` DROPS the request's original
//!     port (probe `q01` vs `r01`) while a scheme-only change PRESERVES it;
//!     `port_redirect` overrides both and does NOT normalise a redundant `:443`.
//!   * the QUERY rule — preserved by default even when `path_redirect` replaces
//!     the path wholesale (`r04`), dropped by `strip_query` (`r08`, `r13`).
//!   * all five `response_code` values on the wire (301/302/303/307/308).
//!
//! `location` is deliberately NOT on the harness's 3-entry `HEADER_ALLOW_LIST`,
//! so `diff_headers` compares it VALUE-EXACT. That comparison IS this fixture's
//! entire witness — adding `location` to the allow-list would silently vacate
//! it. Both proxies listen on different ports, so `location` is only
//! byte-comparable because its authority comes from the `Host:` header, not the
//! socket; probes `q01`/`q03` send `:1234`, matching neither listen port, to
//! prove exactly that.
//!
//! Docker-gated, backend-free (no `{{BACKEND_PORT}}` marker → no backend
//! container spawns), and therefore FULLY verifiable on a developer host.

use std::path::PathBuf;

#[tokio::test]
async fn route_redirect_action_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0086-route-redirect-action");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
