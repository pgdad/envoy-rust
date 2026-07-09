//! Phase 66 backstop: boot the real `envoy-bin` with a
//! `envoy.filters.network.direct_response` listener and assert the observable
//! contract the differential fixture 0071 cannot see in-process.
//!
//! The real cross-proxy assertion is the Docker-gated
//! `tests/differential/tests/network_filter_direct_response.rs`.

use std::io::Write;
use std::net::SocketAddr;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

mod common;
use common::reserve_port;

fn spawn_envoy_bin(yaml: &str) -> (tokio::process::Child, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("envoy-rust.yaml");
    std::fs::File::create(&cfg)
        .unwrap()
        .write_all(yaml.as_bytes())
        .unwrap();
    let child = tokio::process::Command::new(env!("CARGO_BIN_EXE_envoy-bin"))
        .arg("-c")
        .arg(&cfg)
        .env("ENVOY_RUST_LOG", "warn")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn envoy-bin");
    (child, dir)
}

fn cfg_for(port: u16, response_block: &str) -> String {
    format!(
        r#"
static_resources:
  listeners:
    - name: dr_listener
      address:
        socket_address:
          address: 127.0.0.1
          port_value: {port}
      filter_chains:
        - filters:
            - name: envoy.filters.network.direct_response
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.direct_response.v3.Config
{response_block}
"#
    )
}

/// Connect-with-retry: `direct_response` closes every connection, so the
/// shared `wait_ready` helper's probe is itself a full exchange. Retry until
/// the listener is up.
async fn connect_ready(addr: SocketAddr) -> TcpStream {
    for _ in 0..100 {
        if let Ok(s) = TcpStream::connect(addr).await {
            return s;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("listener {addr} never became ready");
}

#[tokio::test(flavor = "multi_thread")]
async fn direct_response_writes_payload_then_clean_eof() {
    let port = reserve_port();
    let yaml = cfg_for(
        port,
        "                response:\n                  inline_string: \"hello-0071\\n\"",
    );
    let (_child, _dir) = spawn_envoy_bin(&yaml);
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();

    let mut s = connect_ready(addr).await;
    let mut out = Vec::new();
    s.read_to_end(&mut out).await.expect("clean EOF, not RST");
    assert_eq!(out, b"hello-0071\n");
}

#[tokio::test(flavor = "multi_thread")]
async fn direct_response_ignores_client_input() {
    // SPEC §0 R-0.5: a client that writes first still receives the payload.
    let port = reserve_port();
    let yaml = cfg_for(
        port,
        "                response:\n                  inline_string: \"hello-0071\\n\"",
    );
    let (_child, _dir) = spawn_envoy_bin(&yaml);
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();

    let mut s = connect_ready(addr).await;
    s.write_all(b"PING-NEVER-READ\n").await.unwrap();
    let mut out = Vec::new();
    s.read_to_end(&mut out).await.expect("clean EOF");
    assert_eq!(out, b"hello-0071\n");
}

#[tokio::test(flavor = "multi_thread")]
async fn direct_response_with_omitted_response_writes_zero_bytes() {
    // SPEC §0 R-0.7: `response` omitted -> zero-byte write + clean close.
    let port = reserve_port();
    let yaml = cfg_for(port, "");
    let (_child, _dir) = spawn_envoy_bin(&yaml);
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();

    let mut s = connect_ready(addr).await;
    let mut out = Vec::new();
    s.read_to_end(&mut out).await.expect("clean EOF");
    assert!(out.is_empty(), "expected zero bytes, got {out:?}");
}
