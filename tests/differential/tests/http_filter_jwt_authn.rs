//! Phase 22 differential acceptance test for fixture 0030-http-filter-jwt-authn.
//! Drives 5 sequential `GET /` requests (`Host: envoy.test`) over an HTTP/1.1
//! listener through an HCM whose `http_filters` chain is
//! `[envoy.filters.http.jwt_authn, envoy.filters.http.router]` with a single
//! provider `provider1` (issuer `testing@secure.istio.io`, audiences
//! `[jwt-fixture-aud]`, inline RSA-2048 JWKS `kid k1`) required on the
//! `prefix: "/"` route. Both proxies must produce the deterministic status
//! sequence `[200, 401, 401, 401, 403]`:
//!
//!   1. valid          — 200 / `"ok\n"`                          (RS256 verify OK)
//!   2. missing         — 401 / `"Jwt is missing"`               (no Authorization)
//!   3. tampered        — 401 / `"Jwt verification fails"`       (bad signature)
//!   4. expired         — 401 / `"Jwt is expired"`               (exp in 2017)
//!   5. wrong-audience  — 403 / `"Audiences in Jwt are not allowed"`
//!
//! This is the FIRST crypto-in-a-filter differential fixture (RS256 verify via
//! the new `envoy-jwt` crate over `aws-lc-rs`). The bodies are byte-exact
//! cross-proxy (upstream Envoy v1.33 source-hardcodes them; envoy-rust matches
//! in `crates/envoy-filter/src/jwt_authn.rs`). The `www-authenticate` header
//! (`Bearer realm="http://envoy.test/"`, with a `, error="invalid_token"`
//! suffix on the non-missing failures) is value-exact cross-proxy because both
//! proxies receive the same `Host` + path, so the realm is identical — it is
//! NOT on the harness allow-list (SPEC §6.2 L3 lock; ADR-0055/0056). The
//! tokens + JWKS are committed static with far-future/past `exp` (zero clock
//! sensitivity).
//!
//! Docker-gated by the differential harness at the cluster level (no per-test
//! cfg gate; the harness skips when `DOCKER_HOST` is unavailable).

use std::path::PathBuf;

#[tokio::test]
async fn http_filter_jwt_authn_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0030-http-filter-jwt-authn");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
