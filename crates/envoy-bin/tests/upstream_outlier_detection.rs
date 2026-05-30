//! 14.2 Task 8 (D8.3): in-process backstop for outlier detection —
//! consecutive-5xx ejection + base-ejection-time un-ejection, plus the
//! synth-503 wire shape on the no-healthy-upstream window. Mirrors the
//! Docker-gated differential fixture 0022
//! (`0022-upstream-outlier-detection-consecutive-5xx`) at the cheap
//! in-process subprocess scope — this verifies envoy-rust's OWN ejection +
//! un-ejection convergence + the 12.2-landed synth-503 shape, without Docker
//! and without a real Envoy (the cross-proxy parity is fixture 0022's job).
//!
//! Shape (mirrors `upstream_connection_pooling.rs` +
//! `upstream_active_health_check.rs`):
//!   1. Spawn the `health-aware-http1-backend` helper with `--per-path
//!      /fail=500` (serves `server error\n` = 13 bytes on `/fail`; 200 on `/`).
//!   2. Spawn `envoy-bin` with a synthesized bootstrap: single-endpoint cluster
//!      `c1` → the helper, `outlier_detection: { consecutive_5xx: 3,
//!      base_ejection_time: 5s, max_ejection_percent: 100, interval: 1s }`,
//!      `common_lb_config.healthy_panic_threshold: { value: 0 }`, an H1 HCM
//!      listener routing `/fail` + `/` to `c1`, and the admin listener. The
//!      SHORT `base_ejection_time` of 5s (PLAN lock-in #13) keeps the un-eject
//!      direction inside test wall-time.
//!   3. EJECT: one keep-alive conn, 3× `GET /fail` → 500 `server error\n`; the
//!      4th `GET /fail` → synth-503 `no healthy upstream` (19 bytes) with the 5
//!      standard HTTP/1.1 headers present. Scrape admin → `ejections_active == 1`.
//!   4. UN-EJECT: after `base_ejection_time` (5s) + one `interval` tick (1s) the
//!      sweeper un-ejects the endpoint; poll a fresh `GET /` until it serves 200
//!      again (route `/` to the SAME cluster `c1` — no mid-test backend flip).
//!      Scrape admin → `ejections_active == 0`.
//!
//! Per phase-09 REVIEW M3 disposition + SPEC §6.4: uses
//! `tokio::process::Command` with `.kill_on_drop(true)`,
//! `stdout: Stdio::null()`, and `stderr: Stdio::piped()`. Discipline copied
//! verbatim from `upstream_connection_pooling.rs`.
//!
//! The un-eject direction uses a poll-until-converged loop (the
//! `upstream_active_health_check.rs` settle-then-probe precedent generalized to
//! a bounded poll) rather than a bare fixed sleep, so a slow CI box still
//! converges within a generous budget.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::io::Write;
use std::net::{SocketAddr, TcpListener as StdListener};
use std::process::Stdio;
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

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

/// Single-request keep-alive helper: writes one request on the existing stream,
/// reads the status line + headers + `Content-Length`-bounded body so the next
/// request starts cleanly on the same conn. Copied from
/// `upstream_connection_pooling.rs`.
async fn http1_keep_alive_request(
    stream: &mut TcpStream,
    path: &str,
) -> (u16, Vec<(String, String)>, Vec<u8>) {
    let req = format!("GET {path} HTTP/1.1\r\nHost: c1\r\nConnection: keep-alive\r\n\r\n");
    stream.write_all(req.as_bytes()).await.expect("write");

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

/// Fresh-connection (Connection: close) GET — used for the un-eject probe and
/// the synth-503 probe so each probe is independent of any pooled state.
async fn http1_get_close(addr: SocketAddr, path: &str) -> (u16, Vec<(String, String)>, Vec<u8>) {
    let mut stream = tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(addr))
        .await
        .expect("connect timeout")
        .expect("connect");
    let req = format!("GET {path} HTTP/1.1\r\nHost: c1\r\nConnection: close\r\n\r\n");
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

/// 10 REVIEW M1: per-probe standard HTTP/1.1 header roster check (the 5-name
/// pin: `server`, `date`, `content-length`, `content-type`, `connection`).
/// Case-insensitive on header names.
fn assert_5_standard_headers_present(headers: &[(String, String)]) {
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
            "missing standard header {required:?}\nactual: {headers:?}",
        );
    }
}

/// Open a fresh TCP conn to admin, GET `/stats`, parse the `<name>: <value>`
/// text lines into a map (only rows with a numeric value retained). Copied from
/// `upstream_connection_pooling.rs`.
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

/// Spawn the `health-aware-http1-backend` helper with `/fail=500`
/// (`server error\n` = 13 bytes) and the default 200 on `/`.
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
        .arg("/fail=500")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn backend")
}

/// Boot envoy-bin with a STATIC single-endpoint cluster `c1` carrying
/// `outlier_detection` (consecutive_5xx=3, base_ejection_time=5s,
/// max_ejection_percent=100, interval=1s) + `healthy_panic_threshold=0`. Both
/// `/fail` and `/` route to `c1` (PLAN simplification — no mid-test backend
/// flip). `base_ejection_time` is the SHORT 5s backstop value (vs the fixture's
/// 60s) so the un-eject direction converges in test wall-time.
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
                        - match: {{ prefix: "/fail" }}
                          route: {{ cluster: c1 }}
                        - match: {{ prefix: "/" }}
                          route: {{ cluster: c1 }}
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: c1
      type: STATIC
      lb_policy: ROUND_ROBIN
      outlier_detection:
        consecutive_5xx: 3
        base_ejection_time: 5s
        max_ejection_percent: 100
        interval: 1s
      common_lb_config:
        healthy_panic_threshold: {{ value: 0 }}
      load_assignment:
        cluster_name: c1
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

/// THE in-process backstop for outlier-detection eject + un-eject convergence —
/// see fixture 0022 for the bilateral Docker check.
#[tokio::test(flavor = "multi_thread")]
async fn outlier_detection_ejects_then_un_ejects() {
    let hcm_port = reserve_port();
    let admin_port = reserve_port();
    let backend_port = reserve_port();
    let hcm_addr: SocketAddr = format!("127.0.0.1:{hcm_port}").parse().unwrap();
    let admin_addr: SocketAddr = format!("127.0.0.1:{admin_port}").parse().unwrap();
    let backend_addr: SocketAddr = format!("127.0.0.1:{backend_port}").parse().unwrap();

    let _backend = spawn_backend(backend_port).await;
    // 30s budget matches the differential harness's backend readiness deadline
    // (cold `cargo run --manifest-path` of the helper may take >10s).
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

    // --- EJECT direction -------------------------------------------------
    // 3× GET /fail on one keep-alive conn → each 500 `server error\n` (13B).
    // consecutive_5xx=3 ⇒ the counter crosses on the 3rd 500 and the endpoint
    // is ejected (max_ejection_percent=100 ⇒ cap = floor(1*100/100) = 1, so the
    // single endpoint CAN be ejected).
    let mut down = tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(hcm_addr))
        .await
        .expect("downstream connect timeout")
        .expect("downstream connect");
    for i in 0..3 {
        let (status, _headers, body) = http1_keep_alive_request(&mut down, "/fail").await;
        assert_eq!(status, 500, "GET /fail #{i} should be backend 500");
        assert_eq!(body, b"server error\n", "GET /fail #{i} body bytes");
    }
    drop(down);

    // The 4th request now finds no healthy upstream (the only endpoint is
    // ejected) ⇒ the 12.2-landed synth-503 (`no healthy upstream`, 19 bytes,
    // 5 standard headers). Fresh conn — the prior keep-alive conn is gone.
    let (status, headers, body) = http1_get_close(hcm_addr, "/fail").await;
    assert_eq!(status, 503, "post-ejection synth 503 no-healthy-upstream");
    assert_eq!(
        body, b"no healthy upstream",
        "ADR-0037 synth-503 body bytes"
    );
    assert_5_standard_headers_present(&headers);
    let cl = headers
        .iter()
        .find(|(n, _)| n == "content-length")
        .map(|(_, v)| v.as_str())
        .unwrap();
    assert_eq!(cl, "19", "synth-503 content-length matches 19-byte body");

    let stats = scrape_admin_stats(admin_addr).await;
    assert_stat(&stats, "cluster.c1.outlier_detection.ejections_active", 1);

    // --- UN-EJECT direction ----------------------------------------------
    // The sweeper ticks every `interval` (1s) and un-ejects once
    // `now - eject_time >= base_ejection_time` (5s). Poll a fresh `GET /` until
    // it converges back to 200 (the endpoint re-enters the pick set and the
    // backend serves 200 on `/`), up to a generous budget. Prefer the
    // poll-until-converged pattern over a bare fixed sleep so a slow box still
    // converges (the active-HC backstop's settle-then-probe precedent,
    // generalized to a bounded poll).
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut last_status = 0u16;
    loop {
        let (s, _h, _b) = http1_get_close(hcm_addr, "/").await;
        last_status = s;
        if s == 200 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "GET / did not converge to 200 after un-eject window; last status {last_status}"
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    assert_eq!(last_status, 200, "un-ejected endpoint serves GET / → 200");

    // The gauge is back to 0 after un-eject (per-endpoint counters reset).
    let stats = scrape_admin_stats(admin_addr).await;
    assert_stat(&stats, "cluster.c1.outlier_detection.ejections_active", 0);
}
