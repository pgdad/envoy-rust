//! Phase 31 differential acceptance test for fixture 0039-http-filter-cdn-loop.
//! Drives 5 sequential HTTP/1.1 requests (`Host: cdn.test`) over an HTTP/1.1
//! listener through an HCM whose `http_filters` chain is
//! `[envoy.filters.http.cdn_loop, envoy.filters.http.router]` with a
//! `CdnLoopConfig { cdn_id: "mycdn.example", max_allowed_occurrences: 0 }`,
//! proxying the single `prefix: "/"` route to the real `http1-echo-server`
//! upstream cluster (the 0013-header-mutation / 0032-csrf pattern).
//!
//! Both proxies must produce the deterministic status sequence `[200, 502, 200, 400, 200]`:
//!
//!   1. no-header      — no CDN-Loop → append bare → forward `cdn-loop: mycdn.example` → 200, echo
//!   2. self-loop      — CDN-Loop: mycdn.example → count=1 > 0 → 502 loop body
//!   3. foreign-append — CDN-Loop: othercdn.example → append comma-only → forward `cdn-loop: othercdn.example,mycdn.example` → 200, echo
//!   4. malformed      — CDN-Loop: "abc (unterminated quoted-string id) → 400 malformed body
//!   5. trailing-comma — CDN-Loop: othercdn.example, → empty entry preserved → forward `cdn-loop: othercdn.example,,mycdn.example` → 200, echo
//!
//! This is the phase's STRONG cross-proxy byte-exact differential. The append
//! probes (1/3/5) observe the FORWARDED `cdn-loop` header via the echo body (the
//! http1-echo-server reflects received request headers as sorted, lowercase
//! `  name: value` lines, so the mutated CDN-Loop surfaces independent of wire
//! casing). The top-level `equivalence.response_body` (byte_exact) confirms both
//! proxies forwarded an IDENTICAL upstream request — i.e. the appended CDN-Loop
//! byte-shape (comma-only join; empty entries preserved, ADR-0077 §6.2-LOCKED)
//! matches live Envoy byte-for-byte.
//!
//! The reject probes' local-reply bodies (502 `The server has detected a loop
//! between CDNs.`, 44 bytes; 400 `Invalid CDN-Loop header in request.`, 35 bytes;
//! both no newline) are pure functions of the filter, byte-exact cross-proxy,
//! and asserted per-probe via `expected_body: { kind: byte_exact }`.
//! `set_equal_modulo_allow_list` confirms value-exact header parity for present
//! headers and name-set parity for absent headers.
//!
//! Docker-gated by the differential harness at the cluster level (no per-test
//! cfg gate; the harness skips when `DOCKER_HOST` is unavailable).

use std::path::PathBuf;

#[tokio::test]
async fn http_filter_cdn_loop_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0039-http-filter-cdn-loop");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
