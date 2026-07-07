//! Phase 04.1 envoy-bin integration test: spawn `envoy-bin` against a minimal
//! HCM-direct_response config, send a single HTTP/1.1 GET, and assert response
//! shape (status 200, the 5 required headers, body "ok\n"). No Docker.
//!
//! This is the envoy-rust-only backstop so a regression in HCM wiring shows up
//! locally without Docker. The Docker-gated differential test in
//! `tests/differential/tests/http1_direct_response.rs` (Task 16) is the full
//! equivalence gate against upstream Envoy.
//!
//! Mirrors the binary-locate + retry-loop shape from phase 02.2's
//! `tests/tcp_proxy.rs`: ~10 lines of `reserve_port` / `wait_ready` are copied
//! here rather than factored into `tests/common/mod.rs` (over-engineering for
//! a single shared helper across two integration tests).

use std::io::Write;
use std::net::SocketAddr;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

mod common;

use common::{reserve_port, wait_ready};

#[tokio::test(flavor = "multi_thread")]
async fn http1_direct_response_round_trip() {
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
                stat_prefix: ingress_http
                codec_type: HTTP1
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

    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_envoy-bin"))
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

    // Drive a single GET; bracket every fallible step inside an inner block
    // so we can guarantee the subprocess is killed on any failure (mirroring
    // phase 02.2's drop-then-kill ordering, but with explicit stderr capture
    // on failure since header / body assertions can fail in subtle ways).
    let outcome = async {
        let mut stream = TcpStream::connect(listener_addr).await?;
        stream
            .write_all(b"GET /healthz HTTP/1.1\r\nHost: envoy-rust.test\r\n\r\n")
            .await?;

        let mut buf = vec![0u8; 4096];
        let mut total = 0usize;
        loop {
            let n = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buf[total..]))
                .await
                .map_err(|_| {
                    anyhow::anyhow!("read timed out after 5s; got {total} bytes so far")
                })??;
            if n == 0 {
                anyhow::bail!(
                    "EOF before complete response; got {total} bytes: {:?}",
                    &buf[..total]
                );
            }
            total += n;

            let mut hdr_storage = [httparse::EMPTY_HEADER; 32];
            let mut resp = httparse::Response::new(&mut hdr_storage);
            match resp.parse(&buf[..total])? {
                httparse::Status::Complete(headers_end) => {
                    let cl = resp
                        .headers
                        .iter()
                        .find(|h| h.name.eq_ignore_ascii_case("content-length"))
                        .and_then(|h| std::str::from_utf8(h.value).ok())
                        .and_then(|s| s.parse::<usize>().ok())
                        .ok_or_else(|| anyhow::anyhow!("no parseable content-length header"))?;
                    if total < headers_end + cl {
                        // Headers complete but body still pending; loop and read more.
                        if total >= buf.len() {
                            anyhow::bail!("buffer full before body complete");
                        }
                        continue;
                    }

                    // Status line.
                    assert_eq!(resp.code, Some(200), "status code");

                    // Required headers (case-insensitive name comparison).
                    // HCM emits lowercase per `headers::*` constants, but
                    // assert case-insensitively to avoid coupling to that
                    // implementation detail.
                    let names_lc: Vec<String> = resp
                        .headers
                        .iter()
                        .map(|h| h.name.to_ascii_lowercase())
                        .collect();
                    for required in [
                        "server",
                        "date",
                        "content-length",
                        "content-type",
                        "connection",
                    ] {
                        assert!(
                            names_lc.iter().any(|n| n == required),
                            "missing required header {required:?}; got: {names_lc:?}",
                        );
                    }

                    // Content-Length consistency + body bytes.
                    assert_eq!(cl, 3, "content-length should be 3 (\"ok\\n\")");
                    let body = &buf[headers_end..headers_end + cl];
                    assert_eq!(body, b"ok\n", "body bytes");

                    return Ok::<(), anyhow::Error>(());
                }
                httparse::Status::Partial => {
                    if total >= buf.len() {
                        anyhow::bail!("buffer full before headers complete");
                    }
                }
            }
        }
    }
    .await;

    // Always tear down the subprocess. On failure, dump captured stderr to
    // aid post-mortem (the validator can reject a misshapen YAML, the bind
    // can fail, etc.).
    if outcome.is_err()
        && let Some(mut err_pipe) = child.stderr.take()
    {
        let mut stderr_buf = Vec::new();
        let _ = err_pipe.read_to_end(&mut stderr_buf).await;
        eprintln!(
            "envoy-bin stderr:\n{}",
            String::from_utf8_lossy(&stderr_buf)
        );
    }
    child.kill().await.ok();
    let _ = child.wait().await;

    outcome.expect("HTTP/1.1 round-trip");
}
