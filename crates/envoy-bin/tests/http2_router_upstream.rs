//! In-process integration backstop for the 05.3 H2-on-H2 router round-trip
//! (mirrors the 04.3 H1 router-upstream backstop at
//! `crates/envoy-bin/tests/http1_router_upstream.rs` and the 05.2 H2 direct-
//! response backstop at `crates/envoy-bin/tests/http2_direct_response.rs`).
//! Spawns envoy-bin via CARGO_BIN_EXE_envoy-bin against an HCM-HTTP2-listener
//! config that points its `backend` cluster at an in-test-spawned
//! http2-echo-server; drives a single H2C `GET /` via h2::client; asserts the
//! parsed response.
//!
//! This test is non-Docker — runs anywhere with the binaries built. Skipped
//! gracefully if either binary is missing.

use std::io::Write;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result};

mod common;

use common::reserve_port;

fn locate_http2_echo_server() -> Result<PathBuf> {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest
        .parent()
        .and_then(|p| p.parent())
        .context("walking to workspace root")?;
    let target_dir = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root.join("target"));
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let mut bin = target_dir.join(profile).join("http2-echo-server");
    if cfg!(windows) {
        bin.set_extension("exe");
    }
    Ok(bin)
}

#[tokio::test(flavor = "multi_thread")]
async fn http2_router_upstream_in_process() -> Result<()> {
    // Locate http2-echo-server. Skip if not built.
    let helper_bin = locate_http2_echo_server()?;
    if !helper_bin.exists() {
        eprintln!(
            "skipping http2_router_upstream_in_process — http2-echo-server not built at {}",
            helper_bin.display()
        );
        return Ok(());
    }

    // Spawn http2-echo-server.
    let helper_port = reserve_port();
    let mut helper_child = tokio::process::Command::new(&helper_bin)
        .arg("--port")
        .arg(helper_port.to_string())
        .env("RUST_LOG", "warn")
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("spawning {} --port {helper_port}", helper_bin.display()))?;

    // Wait for h2 handshake readiness.
    let helper_addr: std::net::SocketAddr = format!("127.0.0.1:{helper_port}").parse()?;
    wait_h2_ready(helper_addr).await?;

    // Build the envoy-rust config pointing at the helper.
    let envoy_port = reserve_port();
    let envoy_yaml = format!(
        r#"node: {{ id: backstop, cluster: envoy-rust-05-3 }}
static_resources:
  listeners:
    - name: l
      address: {{ socket_address: {{ address: 127.0.0.1, port_value: {envoy_port} }} }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP2
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          route: {{ cluster: backend }}
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: {{ socket_address: {{ address: 127.0.0.1, port_value: {helper_port} }} }}
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.HttpProtocolOptions":
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http2_protocol_options: {{}}
"#
    );

    let config_path = tempfile::Builder::new()
        .prefix("envoy-rust-")
        .suffix(".yaml")
        .tempfile()?;
    config_path.as_file().write_all(envoy_yaml.as_bytes())?;

    // Spawn envoy-bin.
    let envoy_bin = PathBuf::from(env!("CARGO_BIN_EXE_envoy-bin"));
    let mut envoy_child = tokio::process::Command::new(&envoy_bin)
        .arg("--config-path")
        .arg(config_path.path())
        .env("RUST_LOG", "warn")
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("spawning {} --config-path", envoy_bin.display()))?;

    let envoy_addr: std::net::SocketAddr = format!("127.0.0.1:{envoy_port}").parse()?;
    wait_h2_ready(envoy_addr).await?;

    // Drive a single H2C GET / against envoy-rust.
    let tcp = tokio::net::TcpStream::connect(envoy_addr).await?;
    let (mut send_request, conn) = h2::client::handshake(tcp).await?;
    tokio::spawn(async move {
        let _ = conn.await;
    });
    let req = http::Request::builder()
        .method("GET")
        .uri("http://envoy-rust.test/")
        .body(())?;
    let (response_fut, _) = send_request
        .send_request(req, true)
        .map_err(|e| anyhow::anyhow!("send_request: {e}"))?;
    let resp = response_fut
        .await
        .map_err(|e| anyhow::anyhow!("response: {e}"))?;
    assert_eq!(resp.status().as_u16(), 200);

    // Drain body.
    let (_parts, mut body) = resp.into_parts();
    let mut body_bytes = bytes::BytesMut::new();
    while let Some(chunk_result) = body.data().await {
        let chunk = chunk_result.map_err(|e| anyhow::anyhow!("body: {e}"))?;
        body_bytes.extend_from_slice(&chunk);
        let _ = body.flow_control().release_capacity(chunk.len());
    }
    let body_str = std::str::from_utf8(&body_bytes).context("response body is not valid UTF-8")?;
    assert!(
        body_str.starts_with("method: GET\n"),
        "expected echo body shape, got: {body_str}"
    );
    assert!(
        body_str.contains(":authority: envoy-rust.test\n"),
        "expected :authority in echo body, got: {body_str}"
    );

    // Cleanup.
    let _ = envoy_child.start_kill();
    let _ = helper_child.start_kill();
    Ok(())
}

async fn wait_h2_ready(addr: std::net::SocketAddr) -> Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    let mut delay = Duration::from_millis(20);
    loop {
        let attempt = async {
            let tcp = tokio::net::TcpStream::connect(addr).await?;
            let (_send, conn) = h2::client::handshake(tcp).await?;
            tokio::spawn(async move {
                let _ = conn.await;
            });
            anyhow::Ok(())
        };
        match tokio::time::timeout(Duration::from_millis(500), attempt).await {
            Ok(Ok(())) => return Ok(()),
            _ if tokio::time::Instant::now() >= deadline => {
                anyhow::bail!("not h2-ready on {addr} within 3s");
            }
            _ => {
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_millis(200));
            }
        }
    }
}
