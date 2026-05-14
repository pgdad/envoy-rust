//! Phase 07.2 envoy-bin integration test: spawn `envoy-bin` against an HCM
//! whose `http_filters` chain is `[HeaderMutation, Router]`, proxying to an
//! in-process HTTP/1.1 echo upstream. Assert the HeaderMutation
//! `request_mutations` stamp (`x-filter-stamp: phase-07`) reaches the backend
//! (echoed back in the response body) and the `response_mutations` stamp
//! (`x-filter-response-stamp: phase-07`) lands on the client-visible response
//! headers. No Docker — the in-process backstop for the Docker-gated fixture
//! `tests/fixtures/0013-http-filter-header-mutation/`.
//!
//! Mirrors `crates/envoy-bin/tests/http1_router_upstream.rs` (the 04.3
//! in-process backstop); the inline upstream here additionally echoes request
//! headers into the response body so the decode-side stamp is observable.

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

/// In-process HTTP/1.1 upstream that echoes the received request headers into
/// the response body as sorted `name: value\n` lines (so the HeaderMutation
/// decode-side stamp is observable in the body). Single request, then closes.
async fn spawn_echo_upstream() -> SocketAddr {
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
        // Parse the request headers via httparse; echo them sorted into the body.
        let mut hdrs = [httparse::EMPTY_HEADER; 32];
        let mut req = httparse::Request::new(&mut hdrs);
        let _ = req.parse(&buf[..total]);
        let mut pairs: Vec<(String, String)> = req
            .headers
            .iter()
            .filter(|h| !h.name.is_empty())
            .map(|h| {
                (
                    h.name.to_ascii_lowercase(),
                    String::from_utf8_lossy(h.value).into_owned(),
                )
            })
            .collect();
        pairs.sort_by(|a, b| a.0.cmp(&b.0));
        let mut body = String::from("headers:\n");
        for (n, v) in &pairs {
            body.push_str("  ");
            body.push_str(n);
            body.push_str(": ");
            body.push_str(v);
            body.push('\n');
        }
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes()).await;
        let _ = stream.shutdown().await;
    });
    addr
}

#[tokio::test(flavor = "multi_thread")]
async fn header_mutation_stamps_request_and_response() {
    let upstream_addr = spawn_echo_upstream().await;
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
                  - name: envoy.filters.http.header_mutation
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.header_mutation.v3.HeaderMutation
                      mutations:
                        request_mutations:
                          - append:
                              header:
                                key: x-filter-stamp
                                value: phase-07
                              append_action: APPEND_IF_EXISTS_OR_ADD
                        response_mutations:
                          - append:
                              header:
                                key: x-filter-response-stamp
                                value: phase-07
                              append_action: APPEND_IF_EXISTS_OR_ADD
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

    let outcome = async {
        let mut stream = TcpStream::connect(listener_addr).await?;
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: envoy-rust.test\r\n\r\n")
            .await?;

        let mut buf = vec![0u8; 8192];
        let mut total = 0usize;
        loop {
            let n = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buf[total..]))
                .await
                .map_err(|_| anyhow::anyhow!("read timed out; got {total} bytes"))??;
            if n == 0 {
                anyhow::bail!("EOF before complete response; got {total} bytes");
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
                        .ok_or_else(|| anyhow::anyhow!("no parseable content-length"))?;
                    if total < headers_end + cl {
                        if total >= buf.len() {
                            anyhow::bail!("buffer full before body complete");
                        }
                        continue;
                    }

                    assert_eq!(resp.code, Some(200), "status code");

                    // Encode-side stamp: x-filter-response-stamp on the headers.
                    let has_resp_stamp = resp.headers.iter().any(|h| {
                        h.name.eq_ignore_ascii_case("x-filter-response-stamp")
                            && h.value == b"phase-07"
                    });
                    assert!(
                        has_resp_stamp,
                        "expected encode-side stamp x-filter-response-stamp: phase-07; \
                         got headers: {:?}",
                        resp.headers
                            .iter()
                            .map(|h| (h.name, String::from_utf8_lossy(h.value)))
                            .collect::<Vec<_>>()
                    );

                    // Decode-side stamp: x-filter-stamp: phase-07 echoed in the
                    // body by the upstream (proves the mutation reached the
                    // backend).
                    let body = &buf[headers_end..headers_end + cl];
                    let needle = b"x-filter-stamp: phase-07";
                    assert!(
                        body.windows(needle.len()).any(|w| w == needle),
                        "expected decode-side stamp echoed in body; got body: {:?}",
                        String::from_utf8_lossy(body)
                    );

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

    outcome.expect("HeaderMutation stamps request + response");
}
