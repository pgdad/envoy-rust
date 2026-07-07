//! Phase 05.2 envoy-bin integration test: spawn `envoy-bin` against a minimal
//! HCM-direct_response config with codec_type: HTTP2, send a single H2C GET
//! via h2::client, and assert response shape (status 200, body "ok\n").
//! No Docker.
//!
//! This is the envoy-rust-only backstop so a regression in HCM-on-H2 wiring
//! shows up locally without Docker. The Docker-gated differential test in
//! `tests/differential/tests/http2_direct_response.rs` (Task 12) is the full
//! equivalence gate against upstream Envoy.
//!
//! Mirrors the binary-locate + retry-loop shape from
//! `crates/envoy-bin/tests/http1_direct_response.rs`.

use std::io::Write;
use std::net::SocketAddr;
use std::process::Stdio;
use std::time::Duration;

use tokio::net::TcpStream;

mod common;

use common::{reserve_port, wait_ready};

#[tokio::test(flavor = "multi_thread")]
async fn http2_direct_response_round_trip() {
    let listener_port = reserve_port();
    let yaml = format!(
        r#"
node:
  id: x
  cluster: y
static_resources:
  listeners:
    - name: hcm_listener
      address:
        socket_address:
          address: 127.0.0.1
          port_value: {listener_port}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http2
                codec_type: HTTP2
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: default
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response:
                            status: 200
                            body: {{ inline_string: "ok\n" }}
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
"#
    );

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
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn envoy-bin");

    let listener_addr: SocketAddr = format!("127.0.0.1:{listener_port}").parse().unwrap();
    wait_ready(listener_addr, Duration::from_secs(5))
        .await
        .expect("listener never became ready");

    let outcome = async {
        let tcp = TcpStream::connect(listener_addr).await?;
        let (mut send_request, conn) = h2::client::handshake(tcp).await?;
        tokio::spawn(async move {
            let _ = conn.await;
        });
        let req = http::Request::builder()
            .method("GET")
            .uri("http://envoy-rust.test/")
            .body(())
            .unwrap();
        let (response_fut, _) = send_request.send_request(req, true)?;
        let resp = response_fut.await?;
        let status = resp.status().as_u16();
        let mut body = resp.into_body();
        let mut bytes = bytes::BytesMut::new();
        while let Some(chunk) = body.data().await {
            let chunk = chunk?;
            bytes.extend_from_slice(&chunk);
        }
        Ok::<_, anyhow::Error>((status, bytes.freeze()))
    }
    .await;

    drop(child); // SIGKILL via kill_on_drop(true).

    let (status, body) = outcome.expect("H2 round-trip");
    assert_eq!(status, 200);
    assert_eq!(&body[..], b"ok\n");
}
