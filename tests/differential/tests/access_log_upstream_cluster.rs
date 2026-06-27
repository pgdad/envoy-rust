//! Docker-gated differential test for fixture 0051-accesslog-upstream-cluster.
//! Phase 43 (ADR-0100) — first fixture exercising the `%UPSTREAM_CLUSTER%`
//! access-log command operator, and the first access-log fixture to route to a
//! REAL upstream backend (fixtures 0040-0050 all route via `direct_response`).
//! `%UPSTREAM_CLUSTER%` renders the routed cluster's name (an `Option<String>`
//! shaped exactly like `%UPSTREAM_HOST%`/`%ROUTE_NAME%` — present → the name;
//! absent → the `-` sentinel in a multi-segment leaf, json `null` in a
//! single-operator-typed leaf). For a `route: { cluster: backend }` route the
//! value is the literal `backend`. Spawns Envoy v1.33 in a container; spawns
//! envoy-rust as a subprocess; auto-spawns the shared `Http1EchoBackend` (the
//! `{{HTTP1_BACKEND_PORT}}` marker, like fixture 0008); drives
//! `kind: http1_access_log_byte_exact` (a `GET /` probe routed to the `backend`
//! cluster whose file access-logger carries a `json_format` with
//! `%UPSTREAM_CLUSTER%` in a single-op leaf and a mixed leaf); reads each side's
//! file access-log and asserts the emitted JSON object is byte-identical
//!   {"method":"GET","mixed":"c=backend","proto":"HTTP/1.1","rcd":"via_upstream","uc":"backend"}
//! (live-captured from envoyproxy/envoy:v1.33.0). `%UPSTREAM_HOST%` is excluded
//! (per-side ip:port mismatch — the §6.2-locked decision).

use std::path::PathBuf;

#[tokio::test]
async fn access_log_upstream_cluster() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0051-accesslog-upstream-cluster");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
