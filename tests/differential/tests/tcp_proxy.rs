//! Phase 02.2 differential acceptance test: drive a payload through a
//! tcp_proxy listener → static cluster → host-local tcp-echo-server backend.
//! Should produce identical bytes between upstream Envoy v1.33.0 and
//! envoy-rust. Docker-gated; in CI this runs on `ubuntu-latest` alongside
//! the phase-00 `echo_fixture` and phase-01 `admin_ready_fixture`.

use std::path::Path;

#[tokio::test]
async fn tcp_proxy_fixture() -> anyhow::Result<()> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures/0003-tcp-proxy");
    differential::run_fixture(&fixture).await
}
