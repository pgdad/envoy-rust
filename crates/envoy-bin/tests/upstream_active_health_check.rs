//! In-process backstop for active HTTP health checking, exercised over an
//! HTTP/1.1 listener. Complements the H1 differential fixture 0019 with
//! cheap H1-codec coverage of BOTH convergence directions:
//!   - healthy:   in-process backend `/healthz` → 200 ⇒ after settle, GET / → 200
//!     through to the backend body
//!   - unhealthy: in-process backend `/healthz` → 503 ⇒ after settle, GET / → 503
//!     with body `no healthy upstream` + 5 standard HTTP/1.1 headers
//!
//! Per phase-09 REVIEW M3 disposition + SPEC §6.4: uses
//! `tokio::process::Command` with `.kill_on_drop(true)`, `stdout: Stdio::null()`,
//! and `stderr: Stdio::piped()` for diagnostics. Discipline copied verbatim from
//! the phase-10/11 `http_filter_*.rs` backstop precedents.
//!
//! On the 503 probe the backstop asserts the per-probe standard HTTP/1.1
//! header presence (10 REVIEW M1 lesson; the 5 headers `{server, date,
//! content-length, content-type, connection}`).

#![forbid(unsafe_code)]

use std::io::Write;
use std::net::{SocketAddr, TcpListener as StdListener};
use std::process::Stdio;
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const SETTLE_MS: u64 = 3500;

fn reserve_port() -> u16 {
    let l = StdListener::bind(("127.0.0.1", 0)).unwrap();
    let p = l.local_addr().unwrap().port();
    drop(l);
    p
}

async fn wait_ready(addr: SocketAddr, budget: Duration) -> std::io::Result<()> {
    let deadline = Instant::now() + budget;
    let mut delay = Duration::from_millis(50);
    loop {
        match TcpStream::connect(addr).await {
            Ok(_) => return Ok(()),
            Err(_) if Instant::now() < deadline => {
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_millis(500));
            }
            Err(e) => return Err(e),
        }
    }
}

async fn http1_get(addr: SocketAddr, path: &str) -> (u16, Vec<(String, String)>, Vec<u8>) {
    let mut stream = tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(addr))
        .await
        .expect("connect timeout")
        .expect("connect");
    let req = format!("GET {path} HTTP/1.1\r\nHost: hc_backend\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes()).await.expect("write");
    let mut buf = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut buf))
        .await
        .expect("read timeout")
        .expect("read");
    let head_end = buf
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("\\r\\n\\r\\n");
    let head = std::str::from_utf8(&buf[..head_end]).expect("utf8");
    let mut lines = head.split("\r\n");
    let status_line = lines.next().expect("status");
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| {
            let (n, v) = l.split_once(": ")?;
            Some((n.to_ascii_lowercase(), v.to_string()))
        })
        .collect();
    let body = buf[head_end + 4..].to_vec();
    (status, headers, body)
}

/// Boot envoy-bin with a synthesized bootstrap pointing at `backend_port`.
async fn spawn_envoy_bin(listener_port: u16, backend_port: u16) -> tokio::process::Child {
    let bootstrap = format!(
        r#"
static_resources:
  listeners:
    - name: ingress_http
      address:
        socket_address:
          address: 127.0.0.1
          port_value: {listener_port}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                codec_type: HTTP1
                stat_prefix: ingress_http
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: local
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          route: {{ cluster: hc_backend }}
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: hc_backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      common_lb_config:
        healthy_panic_threshold: {{ value: 0 }}
      health_checks:
        - timeout: 1s
          interval: 1s
          healthy_threshold: 1
          unhealthy_threshold: 1
          http_health_check:
            path: /healthz
            expected_statuses:
              - {{ start: 200, end: 201 }}
      load_assignment:
        cluster_name: hc_backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: {{ socket_address: {{ address: 127.0.0.1, port_value: {backend_port} }} }}
admin:
  address:
    socket_address: {{ address: 127.0.0.1, port_value: 0 }}
"#
    );
    let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
    tmp.write_all(bootstrap.as_bytes())
        .expect("write bootstrap");
    let path = tmp.path().to_path_buf();
    // Persist tempfile by leaking it; the test process exits shortly.
    std::mem::forget(tmp);
    tokio::process::Command::new(env!("CARGO_BIN_EXE_envoy-bin"))
        .arg("-c")
        .arg(&path)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn envoy-bin")
}

async fn spawn_backend(port: u16, healthz_status: u16) -> tokio::process::Child {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .join("tests/helpers/health-aware-http1-backend/Cargo.toml");
    tokio::process::Command::new(env!("CARGO"))
        .arg("run")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(&manifest)
        .arg("--")
        .arg("--port")
        .arg(port.to_string())
        .arg("--healthz-status")
        .arg(healthz_status.to_string())
        .arg("--data-status")
        .arg("200")
        .arg("--data-body")
        .arg("ok\n")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn helper backend")
}

#[tokio::test]
async fn unhealthy_endpoint_returns_synth_503_no_healthy_upstream() {
    let listener_port = reserve_port();
    let backend_port = reserve_port();
    let backend_addr: SocketAddr = format!("127.0.0.1:{backend_port}").parse().unwrap();
    let listener_addr: SocketAddr = format!("127.0.0.1:{listener_port}").parse().unwrap();
    let _backend = spawn_backend(backend_port, 503).await;
    wait_ready(backend_addr, Duration::from_secs(10))
        .await
        .expect("backend ready");
    let _envoy = spawn_envoy_bin(listener_port, backend_port).await;
    wait_ready(listener_addr, Duration::from_secs(10))
        .await
        .expect("envoy-bin ready");

    // Settle past active-HC convergence (≥ interval + timeout + margin).
    tokio::time::sleep(Duration::from_millis(SETTLE_MS)).await;

    let (status, headers, body) = http1_get(listener_addr, "/").await;
    assert_eq!(status, 503, "no-healthy-upstream synth 503");
    assert_eq!(body, b"no healthy upstream", "ADR-0037 body bytes");
    // 10 REVIEW M1: 5 standard HTTP/1.1 header presence assertion.
    for required in [
        "server",
        "date",
        "content-length",
        "content-type",
        "connection",
    ] {
        assert!(
            headers.iter().any(|(n, _)| n == required),
            "missing standard header {required}; got {headers:?}"
        );
    }
    let cl = headers
        .iter()
        .find(|(n, _)| n == "content-length")
        .map(|(_, v)| v.as_str())
        .unwrap();
    assert_eq!(cl, "19", "content-length matches body bytes");
}

#[tokio::test]
async fn healthy_endpoint_passes_through_to_backend() {
    let listener_port = reserve_port();
    let backend_port = reserve_port();
    let backend_addr: SocketAddr = format!("127.0.0.1:{backend_port}").parse().unwrap();
    let listener_addr: SocketAddr = format!("127.0.0.1:{listener_port}").parse().unwrap();
    let _backend = spawn_backend(backend_port, 200).await;
    wait_ready(backend_addr, Duration::from_secs(10))
        .await
        .expect("backend ready");
    let _envoy = spawn_envoy_bin(listener_port, backend_port).await;
    wait_ready(listener_addr, Duration::from_secs(10))
        .await
        .expect("envoy-bin ready");

    // Settle past healthy-convergence (the healthy_threshold=1 transition
    // fires after the first successful probe).
    tokio::time::sleep(Duration::from_millis(SETTLE_MS)).await;

    let (status, _headers, body) = http1_get(listener_addr, "/").await;
    assert_eq!(status, 200, "pass-through to healthy backend");
    assert_eq!(body, b"ok\n", "backend data-path body");
}
