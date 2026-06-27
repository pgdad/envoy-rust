//! Docker-gated differential test for fixture 0052-accesslog-upstream-host.
//! Phase 44 (ADR-0101) — first fixture exercising the `%UPSTREAM_HOST%`
//! access-log command operator as a BYTE-EXACT cross-proxy differential.
//! `%UPSTREAM_HOST%` renders the resolved upstream endpoint `<ip>:<port>` — the
//! host the request was actually proxied to. It has been implemented since
//! phase 06 (envoy-rust renders it via `SocketAddr::to_string()` = `<ip>:<port>`,
//! IPv4 unbracketed, = Envoy's format); the phase-44 §6.2 format-match recon
//! PROVED no `src/` change is needed, so this is a FIXTURE-ONLY witness. Fixture
//! 0051 EXCLUDED `%UPSTREAM_HOST%` because its STRICT_DNS `{{BACKEND_HOST}}`
//! cluster splits per-side (host-gateway IP vs 127.0.0.1); 0052 closes that gap
//! by routing through a `{{BACKEND_IP}}` shared-host-LAN-IP STATIC cluster
//! (precedent fixture 0036) so BOTH proxies dial the SAME `<ip>:<port>` and
//! render the SAME `%UPSTREAM_HOST%`. Spawns Envoy v1.33 in a container; spawns
//! envoy-rust as a subprocess; auto-spawns the shared `Http1EchoBackend` (the
//! `{{HTTP1_BACKEND_PORT}}` marker, like fixture 0008); drives
//! `kind: http1_access_log_byte_exact` (a `GET /` probe routed to the `backend`
//! cluster whose file access-logger carries a `json_format` with
//! `%UPSTREAM_HOST%` plus the deterministic anchors `%UPSTREAM_CLUSTER%` /
//! `%RESPONSE_CODE_DETAILS%` / `%REQ(:METHOD)%` / `%PROTOCOL%`); reads each
//! side's file access-log and asserts the emitted JSON object is byte-identical
//!   {"method":"GET","proto":"HTTP/1.1","rcd":"via_upstream","uc":"backend","uh":"<ip>:<port>"}
//! The assertion is PURE cross-proxy equality (no static literal — the `uh`
//! `<ip>:<port>` is dynamic per CI run but SHARED across the two proxies).

use std::path::PathBuf;

#[tokio::test]
async fn access_log_upstream_host() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0052-accesslog-upstream-host");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
