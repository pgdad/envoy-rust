//! Phase 24 differential acceptance test for fixture 0032-http-filter-csrf.
//! Drives 5 sequential HTTP/1.1 requests (`Host: csrf.test`) over an HTTP/1.1
//! listener through an HCM whose `http_filters` chain is
//! `[envoy.filters.http.csrf, envoy.filters.http.router]` with a `CsrfPolicy`
//! attached via `typed_per_filter_config` to the single `prefix: "/"` route
//! (proxying to the real `http1-echo-server` upstream cluster per ADR-0061 L8).
//!
//! Both proxies must produce the deterministic status sequence `[200, 403, 200, 200, 403]`:
//!
//!   1. post-same-origin — POST + Origin == target authority → 200, echo body
//!   2. post-evil-origin — POST + disallowed Origin → 403, body "Invalid origin"
//!   3. post-additional  — POST + Origin matching additional_origins → 200, echo
//!   4. get-evil-safe    — GET + disallowed Origin (safe method bypasses) → 200
//!   5. post-no-source   — POST + no Origin/Referer → 403, body "Invalid origin"
//!
//! L8 lock-in (ADR-0061): a `direct_response` route MUST NOT be used — a valid
//! CSRF modify-method request must reach a real upstream to yield a 200; the
//! per-route filter config does not engage on `direct_response`, making the
//! differential trivially-and-incorrectly green. This fixture uses a real
//! upstream cluster (`http1-echo-server` helper, `{{BACKEND_HOST}}` / `{{HTTP1_BACKEND_PORT}}`).
//!
//! The 403 body (`Invalid origin`, 14 bytes, no newline) is a pure function of
//! the CsrfPolicy + the request Origin, so it is byte-exact cross-proxy and is
//! asserted per-probe via `expected_body: { kind: byte_exact }`. The 200 echo
//! bodies are compared cross-proxy via the top-level `equivalence.response_body`
//! (byte_exact). `set_equal_modulo_allow_list` confirms value-exact header parity
//! for present headers and name-set parity for absent headers.
//!
//! Docker-gated by the differential harness at the cluster level (no per-test
//! cfg gate; the harness skips when `DOCKER_HOST` is unavailable).

use std::path::PathBuf;

#[tokio::test]
async fn http_filter_csrf_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0032-http-filter-csrf");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
