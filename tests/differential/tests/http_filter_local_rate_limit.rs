//! Phase 09 differential acceptance test: drive 5 sequential GET / requests
//! through an HCM whose `http_filters` chain is
//! `[envoy.filters.http.local_ratelimit, envoy.filters.http.router]` with a
//! token bucket of `max_tokens: 3, tokens_per_fill: 3, fill_interval: 60s`.
//! Both proxies must produce the deterministic status sequence
//! `[200, 200, 200, 429, 429]`; probes 4-5 carry the upstream-Envoy-parity
//! body `"local_rate_limited"` (18 bytes, source-hardcoded on upstream
//! Envoy v1.33's `envoy.filters.http.local_ratelimit`; envoy-rust matches
//! per phase-09 ADR-0033). Docker-gated.
//!
//! Phase-09 ADR-0033 ("Phase-09 SPEC §2.2 revision per upstream Envoy v1.33
//! empirical observation"): the original PLAN lock-in #13 (synth response
//! carries `x-envoy-ratelimited: true` + empty body) was empirically
//! discovered at Task 5 dispatch to diverge from upstream's actual wire
//! shape. Upstream does NOT emit `x-envoy-ratelimited` from local_ratelimit;
//! the body is the source-hardcoded `"local_rate_limited"`. ADR-0033's
//! Commit B amends envoy-rust's `LocalRateLimitFilter::decode_headers` to
//! match; ADR-0033's Commit C adds the H1 HCM's
//! `decorate_filter_synth_response` helper that decorates the synth
//! response with the 5 standard HTTP/1.1 response headers (server / date /
//! content-length / content-type / connection). This fixture asserts the
//! bilateral wire shape per the revised contract.

use std::path::PathBuf;

#[tokio::test]
async fn http_filter_local_rate_limit_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0016-http-filter-local-rate-limit");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
