//! Phase 25.2 differential acceptance test for fixture 0033-http-filter-buffer.
//! Drives 5 sequential HTTP/1.1 requests (`Host: buffer.test`) over an HTTP/1.1
//! listener through an HCM whose `http_filters` chain is
//! `[envoy.filters.http.buffer, envoy.filters.http.router]` with a chain-level
//! `Buffer { max_request_bytes: 10 }` and per-route `BufferPerRoute` overrides
//! (disable on `/disabled`; lowered limit 4 on `/small`), proxying to the real
//! `http1-echo-server` upstream (ADR-0063 finding 8). Both proxies must produce
//! the deterministic status sequence `[200, 413, 200, 413, 200]`:
//!   1. post-within-limit  — POST / 5B  (<=10) → 200, echo body
//!   2. post-over-limit     — POST / 13B (>10) → 413 "Payload Too Large"
//!   3. post-route-disabled — POST /disabled 13B (disabled) → 200, echo body
//!   4. post-route-lowered  — POST /small 5B (>4) → 413 "Payload Too Large"
//!   5. get-no-body         — GET / (no body) → 200 passthrough echo
//!
//! The 413 body (`Payload Too Large`, 17 bytes, no newline) is byte-exact
//! cross-proxy (asserted per-probe); the 200 echo bodies are compared via the
//! top-level `equivalence.response_body` (byte_exact). Docker-gated by the
//! harness (skips when DOCKER_HOST is unavailable).
use std::path::PathBuf;

#[tokio::test]
async fn http_filter_buffer_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0033-http-filter-buffer");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
