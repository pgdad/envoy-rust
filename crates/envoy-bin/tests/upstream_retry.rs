//! Phase 16 Task 8: in-process backstop for the two H1 retry paths landed in
//! Tasks 1-5. Mirrors the boot/harness shape of `upstream_circuit_breaker.rs`
//! and `upstream_outlier_detection.rs` (boot `envoy-bin` + an in-process
//! backend + admin `/stats` scrape) at the cheap in-process subprocess scope.
//!
//! Two paths exercised in ONE sequential test (cumulative-counter discipline
//! from the PLAN — same envoy-bin instance, counters accumulate):
//!
//!   (a) **success path** — GET /retry-success hits the stateful backend:
//!       request 1 → 503 (body `fail\n`), retry (request 2) → 200 (body
//!       `ok\n`). Final response: 200, `x-envoy-attempt-count: 2`.
//!       Stats: `upstream_rq_retry: 1`, `upstream_rq_retry_success: 1`,
//!       `upstream_rq_retry_limit_exceeded: 0`, `upstream_rq_total: 2`,
//!       `upstream_rq_5xx: 0` (retried-away 503 does NOT tick 5xx — L5 lock-in).
//!
//!   (b) **limit-exceeded path** — GET /retry-exhausted hits the always-503
//!       backend: request 1 → 503, retry (request 2) → 503. Final response:
//!       503, `x-envoy-attempt-count: 2`.
//!       Cumulative stats after both probes: `upstream_rq_retry: 2`,
//!       `upstream_rq_retry_limit_exceeded: 1`, `upstream_rq_retry_success: 1`
//!       (from the success probe), `upstream_rq_total: 4`, `upstream_rq_5xx: 1`
//!       (the last 503 on the exhausted path ticks once — completing response).
//!
//! The in-process backend is a tokio `TcpListener` accept loop: a per-path
//! `Arc<AtomicU64>` request counter partitions /retry-success into a
//! 503-then-200 cyclic window (fail:1 — first request 503, second 200); and
//! /retry-exhausted always returns 503. No helper binary is compiled — the
//! entire backend runs inside the test process, keeping boot latency under 1s.
//!
//! H2-upstream-fork test: SKIPPED. The 13.2 `upstream_h2_connection_pooling.rs`
//! backstop has H2 backend machinery (the `http2-echo-server` helper), but that
//! helper is a 200-always server with no retry-script support. Building a new
//! H2 test server with stateful 503-then-200 logic is non-trivial (h2 framing +
//! SETTINGS + flow-control) and out of scope for a backstop. The H2 retry path
//! is structurally covered: the H2 retry loop in `crates/envoy-http2/src/hcm.rs`
//! mirrors Task 4's H1 loop, and the bilateral differential fixture 0024 covers
//! both H1 and the overall counter contract.
//!
//! Per phase-09 REVIEW M3 disposition: uses `tokio::process::Command` with
//! `.kill_on_drop(true)`, `stdout: Stdio::null()`, `stderr: Stdio::piped()`.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::io::Write;
use std::net::{SocketAddr, TcpListener as StdListener};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

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

/// Single H1 request over a fresh downstream conn (Connection: close):
/// writes the request, reads the status line + headers +
/// `Content-Length`-bounded body. Returns `(status, headers, body)`.
async fn http1_oneshot(hcm: SocketAddr, path: &str) -> (u16, Vec<(String, String)>, Vec<u8>) {
    let mut stream = tokio::time::timeout(Duration::from_secs(10), TcpStream::connect(hcm))
        .await
        .expect("downstream connect timeout")
        .expect("downstream connect");
    let req = format!("GET {path} HTTP/1.1\r\nHost: backend\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes()).await.expect("write");

    // Read until we have the full header block (`\r\n\r\n`).
    let mut buf: Vec<u8> = Vec::with_capacity(2048);
    let head_end = loop {
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos + 4;
        }
        let mut chunk = [0u8; 1024];
        let n = tokio::time::timeout(Duration::from_secs(15), stream.read(&mut chunk))
            .await
            .expect("header read timeout")
            .expect("header read");
        assert!(n > 0, "EOF before headers complete on {path}");
        buf.extend_from_slice(&chunk[..n]);
    };

    let head = std::str::from_utf8(&buf[..head_end - 4]).expect("utf8 head");
    let mut lines = head.split("\r\n");
    let status_line = lines.next().expect("status");
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .expect("status code")
        .parse()
        .expect("status numeric");
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| {
            let (n, v) = l.split_once(':')?;
            Some((n.trim().to_string(), v.trim().to_string()))
        })
        .collect();

    let cl: usize = headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case("content-length"))
        .map(|(_, v)| v.parse().expect("content-length numeric"))
        .expect("content-length header present");

    let body_start = head_end;
    while buf.len() < body_start + cl {
        let mut chunk = [0u8; 1024];
        let n = tokio::time::timeout(Duration::from_secs(15), stream.read(&mut chunk))
            .await
            .expect("body read timeout")
            .expect("body read");
        assert!(n > 0, "EOF before body complete on {path}");
        buf.extend_from_slice(&chunk[..n]);
    }
    let body = buf[body_start..body_start + cl].to_vec();
    (status, headers, body)
}

/// Open a fresh TCP conn to admin, GET `/stats`, parse `<name>: <value>` lines
/// into a map (only numeric rows retained). Mirrors `upstream_circuit_breaker.rs`.
async fn scrape_admin_stats(admin: SocketAddr) -> HashMap<String, u64> {
    let mut stream = tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(admin))
        .await
        .expect("admin connect timeout")
        .expect("admin connect");
    let req = "GET /stats HTTP/1.1\r\nHost: admin\r\nConnection: close\r\n\r\n";
    stream.write_all(req.as_bytes()).await.expect("admin write");
    let mut buf = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut buf))
        .await
        .expect("admin read timeout")
        .expect("admin read");
    let head_end = buf
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("admin head terminator");
    let body = std::str::from_utf8(&buf[head_end + 4..]).expect("admin body utf8");
    let mut out = HashMap::new();
    for line in body.lines() {
        if let Some((name, value)) = line.split_once(": ")
            && let Ok(v) = value.trim().parse::<u64>()
        {
            out.insert(name.trim().to_string(), v);
        }
    }
    out
}

fn assert_stat(stats: &HashMap<String, u64>, name: &str, expected: u64) {
    let actual = stats
        .get(name)
        .copied()
        .unwrap_or_else(|| panic!("stat {name:?} absent; have {} rows", stats.len()));
    assert_eq!(
        actual, expected,
        "stat {name:?}: expected {expected}, got {actual}"
    );
}

fn assert_header_value(headers: &[(String, String)], name: &str, expected: &str, ctx: &str) {
    let val = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.trim().to_string())
        .unwrap_or_else(|| panic!("header {name:?} absent on {ctx}\nactual: {headers:?}",));
    assert_eq!(
        val, expected,
        "header {name:?} on {ctx}: expected {expected:?}, got {val:?}"
    );
}

/// Poll admin `/stats` until `name == expected` or the budget elapses; returns
/// the last observed value. Mirrors the circuit-breaker backstop's bounded-retry
/// convergence for timing-robustness on loaded CI runners.
async fn poll_stat_until(admin: SocketAddr, name: &str, expected: u64, budget: Duration) -> u64 {
    let deadline = Instant::now() + budget;
    loop {
        let stats = scrape_admin_stats(admin).await;
        let last = stats.get(name).copied().unwrap_or(0);
        if last == expected || Instant::now() >= deadline {
            return last;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// Spawn a stateful in-process H1 keep-alive backend:
///
/// - `/retry-success`: uses a cyclic window (fail:1) — global request counter
///   partitions requests into windows of length 2; the first in each window
///   returns 503 (`fail\n`), the second returns 200 (`ok\n`). Mirrors the
///   `health-aware-http1-backend --retry-script /retry-success=fail:1` logic.
/// - `/retry-exhausted`: always returns 503 (`service unavailable\n`).
/// - Any other path: 200 (`ok\n`).
///
/// Responses carry `Connection: keep-alive` so the envoy-rust H1 pool can
/// reuse the upstream connection across retry attempts on the same TCP
/// connection. The retry loop acquires from the same pool-held stream; the
/// backend's keep-alive loop serves the retry request on that same conn.
async fn spawn_stateful_backend() -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind backend");
    let port = listener.local_addr().unwrap().port();
    // Global request counter for /retry-success (NOT source-IP keyed — the
    // proxy opens connections from 127.0.0.1 just like any other client).
    let retry_success_ctr = Arc::new(AtomicU64::new(0));
    tokio::spawn(async move {
        loop {
            let (sock, _peer) = match listener.accept().await {
                Ok(pair) => pair,
                Err(_) => break,
            };
            let retry_success_ctr = retry_success_ctr.clone();
            tokio::spawn(serve_backend_conn(sock, retry_success_ctr));
        }
    });
    port
}

/// Serve one upstream TCP connection with a keep-alive request loop.
/// Each request is read (until `\r\n\r\n`), dispatched, and responded to;
/// if the client signals `Connection: close` the loop exits. Mirrors the
/// `health-aware-http1-backend::serve` keep-alive loop shape so the pool can
/// reuse the connection for subsequent retry attempts.
async fn serve_backend_conn(mut sock: tokio::net::TcpStream, retry_success_ctr: Arc<AtomicU64>) {
    let mut buf: Vec<u8> = Vec::with_capacity(4096);
    loop {
        // Read until we have a full request head (`\r\n\r\n`).
        let head_end = loop {
            if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                break pos + 4;
            }
            let mut chunk = [0u8; 1024];
            match sock.read(&mut chunk).await {
                Ok(0) => return, // Clean EOF between requests.
                Ok(n) => buf.extend_from_slice(&chunk[..n]),
                Err(_) => return,
            }
        };

        // Parse the request path and Connection header from the head.
        let head_str = std::str::from_utf8(&buf[..head_end]).unwrap_or("");
        let path = head_str
            .lines()
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .unwrap_or("/")
            .to_string();
        let request_wants_close = head_str.lines().any(|l| {
            l.to_ascii_lowercase().starts_with("connection:")
                && l.to_ascii_lowercase().contains("close")
        });

        // Consume the request head from the buffer; any pipelined bytes carry
        // forward.
        buf.drain(..head_end);

        let (status, body): (u16, &[u8]) = if path.starts_with("/retry-success") {
            // Cyclic fail:1 window: idx 0 → 503, idx 1 → 200, idx 2 → 503, …
            let idx = retry_success_ctr.fetch_add(1, Ordering::Relaxed);
            if idx.is_multiple_of(2) {
                (503, b"fail\n")
            } else {
                (200, b"ok\n")
            }
        } else if path.starts_with("/retry-exhausted") {
            (503, b"service unavailable\n")
        } else {
            (200, b"ok\n")
        };

        let reason = match status {
            200 => "OK",
            503 => "Service Unavailable",
            _ => "OK",
        };
        let conn_value = if request_wants_close {
            "close"
        } else {
            "keep-alive"
        };
        let resp = format!(
            "HTTP/1.1 {status} {reason}\r\n\
             content-length: {len}\r\n\
             content-type: text/plain\r\n\
             connection: {conn_value}\r\n\r\n",
            len = body.len(),
        );
        if sock.write_all(resp.as_bytes()).await.is_err() {
            return;
        }
        if !body.is_empty() && sock.write_all(body).await.is_err() {
            return;
        }
        if sock.flush().await.is_err() {
            return;
        }
        if request_wants_close {
            return;
        }
    }
}

/// Boot `envoy-bin` with a STATIC `backend` cluster pointing at `backend_port`,
/// HCM listener on `hcm_port` (HTTP1), admin on `admin_port`.
///
/// The vhost has `include_attempt_count_in_response: true`. Two routes:
/// - `/retry-success` → cluster `backend` with `retry_policy: {retry_on: "5xx", num_retries: 1}`
/// - `/retry-exhausted` → cluster `backend` with `retry_policy: {retry_on: "5xx", num_retries: 1}`
async fn spawn_envoy_bin(
    hcm_port: u16,
    admin_port: u16,
    backend_port: u16,
) -> tokio::process::Child {
    let bootstrap = format!(
        r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: {admin_port}
static_resources:
  listeners:
    - name: ingress_http
      address:
        socket_address:
          address: 127.0.0.1
          port_value: {hcm_port}
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
                      include_attempt_count_in_response: true
                      routes:
                        - match: {{ prefix: "/retry-success" }}
                          route:
                            cluster: backend
                            retry_policy: {{ retry_on: "5xx", num_retries: 1 }}
                        - match: {{ prefix: "/retry-exhausted" }}
                          route:
                            cluster: backend
                            retry_policy: {{ retry_on: "5xx", num_retries: 1 }}
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
                  address: {{ socket_address: {{ address: 127.0.0.1, port_value: {backend_port} }} }}
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

/// THE in-process backstop for the H1 retry success + limit-exceeded paths.
///
/// Probe sequence (both paths on ONE envoy-bin instance; counters are
/// cumulative):
///
///   (a) GET /retry-success → backend serves 503 (attempt 1), retry → 200
///       (attempt 2); downstream response: 200 + `x-envoy-attempt-count: 2`.
///       Stats at this point: `upstream_rq_retry=1`, `upstream_rq_retry_success=1`,
///       `upstream_rq_retry_limit_exceeded=0`, `upstream_rq_total=2`, `upstream_rq_5xx=0`.
///
///   (b) GET /retry-exhausted → backend serves 503 (attempt 1), retry → 503
///       (attempt 2); downstream response: 503 + `x-envoy-attempt-count: 2`.
///       Cumulative stats: `upstream_rq_retry=2`, `upstream_rq_retry_limit_exceeded=1`,
///       `upstream_rq_retry_success=1`, `upstream_rq_total=4`, `upstream_rq_5xx=1`.
#[tokio::test(flavor = "multi_thread")]
async fn retry_success_and_limit_exceeded_paths() {
    let hcm_port = reserve_port();
    let admin_port = reserve_port();
    let hcm_addr: SocketAddr = format!("127.0.0.1:{hcm_port}").parse().unwrap();
    let admin_addr: SocketAddr = format!("127.0.0.1:{admin_port}").parse().unwrap();

    let backend_port = spawn_stateful_backend().await;

    let _envoy = spawn_envoy_bin(hcm_port, admin_port, backend_port).await;
    wait_ready(hcm_addr, Duration::from_secs(15))
        .await
        .expect("envoy-bin HCM ready");
    wait_ready(admin_addr, Duration::from_secs(15))
        .await
        .expect("envoy-bin admin ready");

    // -------------------------------------------------------------------------
    // (a) Success path: /retry-success → 503 on attempt 1, 200 on retry.
    // -------------------------------------------------------------------------
    let (status_a, headers_a, body_a) = http1_oneshot(hcm_addr, "/retry-success").await;
    assert_eq!(
        status_a, 200,
        "/retry-success: expected final 200 after retry"
    );
    assert_eq!(body_a, b"ok\n", "/retry-success: expected body `ok\\n`");
    assert_header_value(&headers_a, "x-envoy-attempt-count", "2", "/retry-success");

    // Wait for the anchor stat to land, then capture the full snapshot.
    poll_stat_until(
        admin_addr,
        "cluster.backend.upstream_rq_retry_success",
        1,
        Duration::from_secs(5),
    )
    .await;
    let stats_a = scrape_admin_stats(admin_addr).await;

    assert_stat(&stats_a, "cluster.backend.upstream_rq_retry", 1);
    assert_stat(&stats_a, "cluster.backend.upstream_rq_retry_success", 1);
    assert_stat(
        &stats_a,
        "cluster.backend.upstream_rq_retry_limit_exceeded",
        0,
    );
    assert_stat(&stats_a, "cluster.backend.upstream_rq_total", 2);
    // L5 lock-in: the retried-away 503 does NOT tick upstream_rq_5xx;
    // only the completing response (200 here) is counted, and 200 is not 5xx.
    assert_stat(&stats_a, "cluster.backend.upstream_rq_5xx", 0);

    // -------------------------------------------------------------------------
    // (b) Limit-exceeded path: /retry-exhausted → 503 on attempt 1, 503 on
    // retry; limit exceeded, downstream gets the final 503.
    // -------------------------------------------------------------------------
    let (status_b, headers_b, body_b) = http1_oneshot(hcm_addr, "/retry-exhausted").await;
    assert_eq!(
        status_b, 503,
        "/retry-exhausted: expected final 503 after retry exhaustion"
    );
    assert_eq!(
        body_b, b"service unavailable\n",
        "/retry-exhausted: expected body `service unavailable\\n`",
    );
    assert_header_value(&headers_b, "x-envoy-attempt-count", "2", "/retry-exhausted");

    // Wait for the anchor stat to land, then capture the full snapshot.
    poll_stat_until(
        admin_addr,
        "cluster.backend.upstream_rq_retry_limit_exceeded",
        1,
        Duration::from_secs(5),
    )
    .await;
    let stats_b = scrape_admin_stats(admin_addr).await;

    // Cumulative after both probes.
    assert_stat(&stats_b, "cluster.backend.upstream_rq_retry", 2);
    assert_stat(
        &stats_b,
        "cluster.backend.upstream_rq_retry_limit_exceeded",
        1,
    );
    assert_stat(&stats_b, "cluster.backend.upstream_rq_retry_success", 1);
    assert_stat(&stats_b, "cluster.backend.upstream_rq_total", 4);
    // The COMPLETING (last) 503 on the exhausted path ticks upstream_rq_5xx once.
    assert_stat(&stats_b, "cluster.backend.upstream_rq_5xx", 1);
}
