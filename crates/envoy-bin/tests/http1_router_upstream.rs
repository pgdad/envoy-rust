//! Phase 04.3 envoy-bin integration test: spawn `envoy-bin` against an HCM
//! `route: { cluster: backend }` config pointed at an in-process tokio HTTP/1.1
//! upstream, send a single GET, assert status + the 6 required response
//! headers (server, date, content-length, content-type, connection,
//! x-envoy-upstream-service-time) and the body bytes. No Docker.
//!
//! Sibling of phase 04.1's `http1_direct_response.rs` (HCM YAML + envoy-bin
//! subprocess + httparse parsing) and phase 03.2's `tls_upstream.rs`
//! (in-process upstream alongside envoy-bin). The Docker-gated equivalent
//! lands in `tests/differential/tests/http1_router_upstream.rs` (Task 15).

#![forbid(unsafe_code)]

use std::io::Write;
use std::net::{SocketAddr, TcpListener as StdListener};
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

fn reserve_port() -> u16 {
    let l = StdListener::bind(("127.0.0.1", 0)).unwrap();
    let p = l.local_addr().unwrap().port();
    drop(l);
    p
}

async fn wait_ready(addr: SocketAddr, budget: Duration) {
    let deadline = std::time::Instant::now() + budget;
    let mut delay = Duration::from_millis(50);
    loop {
        match TcpStream::connect(addr).await {
            Ok(_) => return,
            Err(_) if std::time::Instant::now() < deadline => {
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_millis(500));
            }
            Err(e) => panic!("listener never became ready at {addr}: {e}"),
        }
    }
}

// Minimal in-process HTTP/1.1 upstream: single accept, drain request bytes
// until \r\n\r\n (cap 8 KiB), respond with a fixed 200 + 5-byte body, close.
// Deliberately NOT replicating http1-echo-server's deterministic-echo body
// shape — that's the differential harness's job in Task 15.
async fn spawn_http1_upstream() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let Ok((mut stream, _)) = listener.accept().await else {
            return;
        };
        let mut buf = vec![0u8; 8192];
        let mut total = 0usize;
        loop {
            let Ok(n) = stream.read(&mut buf[total..]).await else {
                return;
            };
            if n == 0 {
                return;
            }
            total += n;
            if buf[..total].windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
            if total >= buf.len() {
                return;
            }
        }
        let response = b"HTTP/1.1 200 OK\r\n\
            Content-Type: text/plain\r\n\
            Content-Length: 5\r\n\
            Connection: close\r\n\
            \r\n\
            hello";
        let _ = stream.write_all(response).await;
        let _ = stream.shutdown().await;
    });
    addr
}

#[tokio::test(flavor = "multi_thread")]
async fn proxies_get_through_router_to_http1_echo_backend() {
    let upstream_addr = spawn_http1_upstream().await;
    let upstream_port = upstream_addr.port();
    let listener_port = reserve_port();

    let yaml = format!(
        r#"
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
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: {upstream_port}
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
    wait_ready(listener_addr, Duration::from_secs(5)).await;

    // Bracket fallible steps so the subprocess is always torn down + stderr
    // dumped on failure (mirrors http1_direct_response.rs).
    let outcome = async {
        let mut stream = TcpStream::connect(listener_addr).await?;
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: envoy-rust.test\r\n\r\n")
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
                        if total >= buf.len() {
                            anyhow::bail!("buffer full before body complete");
                        }
                        continue;
                    }

                    assert_eq!(resp.code, Some(200), "status code");

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
                        "x-envoy-upstream-service-time",
                    ] {
                        assert!(
                            names_lc.iter().any(|n| n == required),
                            "missing required header {required:?}; got: {names_lc:?}",
                        );
                    }

                    assert_eq!(cl, 5, "content-length should match upstream body");
                    let body = &buf[headers_end..headers_end + cl];
                    assert_eq!(body, b"hello", "body bytes");

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

    outcome.expect("HTTP/1.1 router-proxy round-trip");
}
