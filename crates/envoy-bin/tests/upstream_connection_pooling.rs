//! 13.1 Task 8 (D9.3): in-process H1 backstop for the connection-pool
//! reuse + per-class counter math property. Mirrors the bilateral Docker
//! fixture 0020 (`upstream_connection_pooling_and_per_class_counters`)
//! at the cheap in-process subprocess scope.
//!
//! Shape:
//!   1. Spawn the `health-aware-http1-backend` helper (Task 7 keep-alive
//!      extension) with `--per-path /301=301,/404=404,/500=500`.
//!   2. Spawn `envoy-bin` with a synthesized bootstrap pointing a STATIC
//!      backend_cluster at the helper's port; HCM listener + admin
//!      listener on reserved ports.
//!   3. Open ONE downstream H1 keep-alive conn, drive 10 sequential GETs
//!      matching fixture 0020's workload (4× /, 1× /301, 2× /404, 3×
//!      /500), assert per-probe status; on non-2xx additionally assert
//!      the 5 standard HTTP/1.1 headers are present (10 REVIEW M1).
//!   4. GET `/stats` from admin and assert the 9 counter rows fixture
//!      0020 pins, including `cluster.backend_cluster.upstream_cx_total
//!      = 1` (THE pool-reuse property — single upstream conn) +
//!      `cluster.backend_cluster.upstream_cx_http1_total = 1`.
//!
//! Per phase-09 REVIEW M3 disposition + SPEC §6.4: uses
//! `tokio::process::Command` with `.kill_on_drop(true)`,
//! `stdout: Stdio::null()`, and `stderr: Stdio::piped()`. Discipline
//! copied verbatim from `upstream_active_health_check.rs`.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::io::Write;
use std::net::{SocketAddr, TcpListener as StdListener};
use std::process::Stdio;
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const SETTLE_MS: u64 = 200;

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

/// Single-request keep-alive helper: writes one request on the existing
/// stream, reads the status line + headers + `Content-Length`-bounded
/// body so the next request starts cleanly on the same conn.
async fn http1_keep_alive_request(
    stream: &mut TcpStream,
    path: &str,
) -> (u16, Vec<(String, String)>, Vec<u8>) {
    let req =
        format!("GET {path} HTTP/1.1\r\nHost: backend_cluster\r\nConnection: keep-alive\r\n\r\n");
    stream.write_all(req.as_bytes()).await.expect("write");

    // Read until we have the full header block (`\r\n\r\n`).
    let mut buf: Vec<u8> = Vec::with_capacity(2048);
    let head_end = loop {
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos + 4;
        }
        let mut chunk = [0u8; 1024];
        let n = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut chunk))
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

    // Drain exactly `cl` body bytes (extending the read buffer until we
    // have all of them; any pipelined slack stays untouched — the test
    // drives one request at a time on the same conn).
    let body_start = head_end;
    while buf.len() < body_start + cl {
        let mut chunk = [0u8; 1024];
        let n = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut chunk))
            .await
            .expect("body read timeout")
            .expect("body read");
        assert!(n > 0, "EOF before body complete on {path}");
        buf.extend_from_slice(&chunk[..n]);
    }
    let body = buf[body_start..body_start + cl].to_vec();
    (status, headers, body)
}

/// 10 REVIEW M1: per-probe standard HTTP/1.1 header roster check
/// (the 5-name pin: `server`, `date`, `content-length`, `content-type`,
/// `connection`). Case-insensitive on header names so envoy-rust's
/// canonical-case emission stays interchangeable with the fixture
/// expectations.
fn assert_5_standard_headers_present(headers: &[(String, String)], path: &str, status: u16) {
    for required in &[
        "server",
        "date",
        "content-length",
        "content-type",
        "connection",
    ] {
        assert!(
            headers
                .iter()
                .any(|(n, _)| n.eq_ignore_ascii_case(required)),
            "missing standard header {required:?} on {path} ({status})\nactual: {headers:?}",
        );
    }
}

/// Open a fresh TCP conn to admin, GET `/stats`, parse the
/// `<name>: <value>` text lines into a map (only rows with a numeric
/// value are retained).
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

/// Spawn the `health-aware-http1-backend` helper (Task 7 keep-alive
/// extension) with the per-class status mapping fixture 0020 uses.
async fn spawn_backend(port: u16) -> tokio::process::Child {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .join("tests/helpers/health-aware-http1-backend/Cargo.toml");
    tokio::process::Command::new(env!("CARGO"))
        .arg("run")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(&manifest)
        .arg("--")
        .arg("--port")
        .arg(port.to_string())
        .arg("--per-path")
        .arg("/301=301,/404=404,/500=500")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn backend")
}

/// Boot envoy-bin with a STATIC bootstrap pointing `backend_cluster` at
/// `backend_port`. Admin listener bound at the reserved `admin_port`.
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
                      routes:
                        - match: {{ prefix: "/" }}
                          route: {{ cluster: backend_cluster }}
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend_cluster
      type: STATIC
      lb_policy: ROUND_ROBIN
      circuit_breakers:
        thresholds:
          - priority: DEFAULT
            max_connections: 4
      load_assignment:
        cluster_name: backend_cluster
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

/// THE in-process backstop for the H1-pool-reuse + per-class counter
/// property — see fixture 0020 for the bilateral Docker check.
#[tokio::test(flavor = "multi_thread")]
async fn pool_reuses_upstream_conn_and_counts_per_class() {
    let hcm_port = reserve_port();
    let admin_port = reserve_port();
    let backend_port = reserve_port();
    let hcm_addr: SocketAddr = format!("127.0.0.1:{hcm_port}").parse().unwrap();
    let admin_addr: SocketAddr = format!("127.0.0.1:{admin_port}").parse().unwrap();
    let backend_addr: SocketAddr = format!("127.0.0.1:{backend_port}").parse().unwrap();

    let _backend = spawn_backend(backend_port).await;
    // 12.2 state-5 review Cluster B I2: 30s budget matches the
    // differential harness's `HealthAwareHttp1Backend::spawn` readiness
    // deadline (cold cargo build of the helper takes >10s; the
    // `cargo run --manifest-path` step compiles on first hit).
    wait_ready(backend_addr, Duration::from_secs(30))
        .await
        .expect("backend ready");

    let _envoy = spawn_envoy_bin(hcm_port, admin_port, backend_port).await;
    wait_ready(hcm_addr, Duration::from_secs(10))
        .await
        .expect("envoy-bin HCM ready");
    wait_ready(admin_addr, Duration::from_secs(10))
        .await
        .expect("envoy-bin admin ready");

    // One downstream H1 keep-alive conn drives all 10 requests.
    let mut down = tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(hcm_addr))
        .await
        .expect("downstream connect timeout")
        .expect("downstream connect");

    let workload: &[(&str, u16)] = &[
        ("/", 200),
        ("/", 200),
        ("/", 200),
        ("/", 200),
        ("/301", 301),
        ("/404", 404),
        ("/404", 404),
        ("/500", 500),
        ("/500", 500),
        ("/500", 500),
    ];
    for (path, expected) in workload {
        let (status, headers, _body) = http1_keep_alive_request(&mut down, path).await;
        assert_eq!(
            status, *expected,
            "expected status {expected} for GET {path}, got {status}"
        );
        if !(200..300).contains(&status) {
            assert_5_standard_headers_present(&headers, path, status);
        }
    }
    drop(down);

    // Settle for the stat increments to flush.
    tokio::time::sleep(Duration::from_millis(SETTLE_MS)).await;

    let stats = scrape_admin_stats(admin_addr).await;

    // Per-class downstream HCM counters (5).
    assert_stat(&stats, "http.ingress_http.downstream_rq_2xx", 4);
    assert_stat(&stats, "http.ingress_http.downstream_rq_3xx", 1);
    assert_stat(&stats, "http.ingress_http.downstream_rq_4xx", 2);
    assert_stat(&stats, "http.ingress_http.downstream_rq_5xx", 3);
    assert_stat(&stats, "http.ingress_http.downstream_rq_total", 10);

    // Cluster-side upstream counters (4): the H1-pool-reuse property +
    // the only per-class cluster counter (5xx) envoy-rust currently
    // tracks.
    assert_stat(&stats, "cluster.backend_cluster.upstream_rq_5xx", 3);
    assert_stat(&stats, "cluster.backend_cluster.upstream_rq_total", 10);
    // THE pool-reuse property — single upstream conn for 10 requests.
    assert_stat(&stats, "cluster.backend_cluster.upstream_cx_total", 1);
    assert_stat(&stats, "cluster.backend_cluster.upstream_cx_http1_total", 1);
}
