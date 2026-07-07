//! Shared integration-test harness helpers, consolidated from the per-test
//! copies that every backstop under `tests/` used to carry (the long-deferred
//! M18-9 "extract shared test support" item). Cargo compiles `tests/common/`
//! into each integration-test binary that declares `mod common;` — it is NOT
//! its own test target.
//!
//! Each test binary uses only a subset of these helpers, hence the module-wide
//! `allow(dead_code)`.

#![allow(dead_code)]

use std::collections::HashMap;
use std::io::Write;
use std::net::{SocketAddr, TcpListener as StdListener};
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Reserve an ephemeral port by binding `127.0.0.1:0`, reading the port, and
/// immediately dropping the listener.
pub fn reserve_port() -> u16 {
    let l = StdListener::bind(("127.0.0.1", 0)).unwrap();
    let p = l.local_addr().unwrap().port();
    drop(l);
    p
}

/// Poll-connect `addr` with 50ms→500ms exponential backoff up to `budget`.
/// Returns `Ok(())` on the first successful connect; `Err` on timeout.
pub async fn wait_ready(addr: SocketAddr, budget: Duration) -> std::io::Result<()> {
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

/// Single one-shot H1 request over a fresh downstream conn (`Connection:
/// close`) carrying `Host: <host>`: writes the request, reads the status line
/// + headers + `Content-Length`-bounded body. Returns `(status, headers,
/// body)`; callers that don't need the headers ignore them.
pub async fn http1_oneshot(
    hcm: SocketAddr,
    path: &str,
    host: &str,
) -> (u16, Vec<(String, String)>, Vec<u8>) {
    let mut stream = tokio::time::timeout(Duration::from_secs(10), TcpStream::connect(hcm))
        .await
        .expect("downstream connect timeout")
        .expect("downstream connect");
    let req = format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
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

// ── admin scraping ────────────────────────────────────────────────────────────

/// Open a fresh TCP conn to admin, GET `path`, read the whole response, split off
/// the body. Returns the raw body bytes.
pub async fn admin_get_body(admin: SocketAddr, path: &str) -> Vec<u8> {
    let mut stream = tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(admin))
        .await
        .expect("admin connect timeout")
        .expect("admin connect");
    let req = format!("GET {path} HTTP/1.1\r\nHost: admin\r\nConnection: close\r\n\r\n");
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
    buf[head_end + 4..].to_vec()
}

/// GET admin `/stats` and parse `<name>: <value>` numeric rows into a map.
pub async fn scrape_admin_stats(admin: SocketAddr) -> HashMap<String, u64> {
    let body = admin_get_body(admin, "/stats").await;
    let text = std::str::from_utf8(&body).expect("admin body utf8");
    let mut out = HashMap::new();
    for line in text.lines() {
        if let Some((name, value)) = line.split_once(": ")
            && let Ok(v) = value.trim().parse::<u64>()
        {
            out.insert(name.trim().to_string(), v);
        }
    }
    out
}

/// Assert `stats[name] == expected`, panicking with a diagnostic when the stat
/// is absent or carries a different value.
pub fn assert_stat(stats: &HashMap<String, u64>, name: &str, expected: u64) {
    let actual = stats
        .get(name)
        .copied()
        .unwrap_or_else(|| panic!("stat {name:?} absent; have {} rows", stats.len()));
    assert_eq!(
        actual, expected,
        "stat {name:?}: expected {expected}, got {actual}"
    );
}

/// Poll `/stats` at ~150ms until `stats[name] == expected` or `budget` elapses.
/// The watcher poll cadence is ~1s, so a `budget` of ~8s gives several poll
/// windows of slack. Panics (with the last observed value) on timeout.
pub async fn wait_for_stat(admin: SocketAddr, name: &str, expected: u64, budget: Duration) {
    let deadline = Instant::now() + budget;
    loop {
        let stats = scrape_admin_stats(admin).await;
        let got = stats.get(name).copied();
        if got == Some(expected) {
            return;
        }
        if Instant::now() >= deadline {
            panic!(
                "wait_for_stat({name:?}) timed out after {budget:?}: expected {expected}, last saw {got:?}"
            );
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
}

/// Poll admin `/stats` until `name == expected` or the budget elapses; returns
/// the last observed value. Mirrors the circuit-breaker backstop's bounded-retry
/// convergence for timing-robustness on loaded CI runners.
pub async fn poll_stat_until(
    admin: SocketAddr,
    name: &str,
    expected: u64,
    budget: Duration,
) -> u64 {
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

// ── in-process backends ───────────────────────────────────────────────────────

/// Spawn an in-process H1 backend that replies to every request with a 200 whose
/// body is the fixed `body` string. Returns the bound port. The backend serves a
/// keep-alive request loop per connection (honoring `Connection: close`).
pub async fn spawn_backend(body: &'static str) -> u16 {
    spawn_slow_backend(body, Duration::ZERO).await
}

/// Like `spawn_backend` but sleeps `delay` before responding to each request (the
/// in-flight-isolation tests route to a SLOW backend so a request is reliably
/// mid-flight when a concurrent reload lands).
pub async fn spawn_slow_backend(body: &'static str, delay: Duration) -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind backend");
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let (sock, _peer) = match listener.accept().await {
                Ok(pair) => pair,
                Err(_) => break,
            };
            tokio::spawn(serve_backend_conn(sock, body, delay));
        }
    });
    port
}

/// Per-connection keep-alive request loop for `spawn_backend` /
/// `spawn_slow_backend`: reads one request head at a time, optionally sleeps
/// `delay`, then replies 200 with the fixed `body` (honoring
/// `Connection: close`).
pub async fn serve_backend_conn(mut sock: TcpStream, body: &'static str, delay: Duration) {
    let mut buf: Vec<u8> = Vec::with_capacity(2048);
    loop {
        let head_end = loop {
            if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                break pos + 4;
            }
            let mut chunk = [0u8; 512];
            match sock.read(&mut chunk).await {
                Ok(0) => return,
                Ok(n) => buf.extend_from_slice(&chunk[..n]),
                Err(_) => return,
            }
        };
        let head = std::str::from_utf8(&buf[..head_end]).unwrap_or("");
        let wants_close = head.lines().any(|l| {
            l.to_ascii_lowercase().starts_with("connection:")
                && l.to_ascii_lowercase().contains("close")
        });
        buf.drain(..head_end);

        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }

        let conn = if wants_close { "close" } else { "keep-alive" };
        let resp = format!(
            "HTTP/1.1 200 OK\r\n\
             content-length: {len}\r\n\
             content-type: text/plain\r\n\
             connection: {conn}\r\n\r\n{body}",
            len = body.len(),
        );
        if sock.write_all(resp.as_bytes()).await.is_err() {
            return;
        }
        let _ = sock.flush().await;
        if wants_close {
            return;
        }
    }
}

/// Spawn a minimal HTTP/1.1 backend that accepts multiple connections in a
/// loop and responds with `HTTP/1.1 200 OK\r\ncontent-length: 3\r\n\r\nok\n`
/// to each request. The task runs until the listener is garbage-collected
/// (it never panics — failures are silently ignored so the probe-side
/// assertions surface the real error instead).
pub async fn spawn_http1_backend() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                // Drain the request (wait for the blank-line terminator).
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
                    content-type: text/plain\r\n\
                    content-length: 3\r\n\
                    connection: close\r\n\
                    \r\n\
                    ok\n";
                let _ = stream.write_all(response).await;
                let _ = stream.shutdown().await;
            });
        }
    });
    addr
}

// ── config writers + spawn ────────────────────────────────────────────────────

/// Write `contents` to `dir/name` and return the absolute path string.
pub fn write_file(dir: &Path, name: &str, contents: &str) -> String {
    let path = dir.join(name);
    std::fs::File::create(&path)
        .unwrap()
        .write_all(contents.as_bytes())
        .unwrap();
    path.to_str().unwrap().to_string()
}

/// Write `bootstrap` to `dir/envoy-rust.yaml` and return the path.
pub fn write_bootstrap(dir: &Path, bootstrap: &str) -> std::path::PathBuf {
    let cfg = dir.join("envoy-rust.yaml");
    std::fs::File::create(&cfg)
        .unwrap()
        .write_all(bootstrap.as_bytes())
        .unwrap();
    cfg
}

/// Spawn `envoy-bin -c <cfg>` with the established stdio discipline.
pub fn spawn_envoy_bin(cfg: &Path) -> tokio::process::Child {
    tokio::process::Command::new(env!("CARGO_BIN_EXE_envoy-bin"))
        .arg("-c")
        .arg(cfg)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn envoy-bin")
}

/// ATOMICALLY replace a watched file's contents: writes a SAME-DIR sibling temp
/// (`<target>.reload-tmp`) then `std::fs::rename`s it over `target`.
/// The §6.2 watcher detects the change by the file's mtime stepping forward; an
/// atomic rename guarantees the watcher only ever stats a COMPLETE file (an
/// in-place truncate-rewrite could expose a half-written file AND — depending on
/// timing — might not even tick the mtime). The sibling is on the SAME fs so the
/// rename is atomic.
pub fn atomic_rename_over(target: &Path, new_contents: &str) {
    let tmp = target.with_extension("reload-tmp");
    std::fs::File::create(&tmp)
        .unwrap()
        .write_all(new_contents.as_bytes())
        .unwrap();
    std::fs::rename(&tmp, target).expect("atomic rename over target");
}

/// Dump a spawned envoy-bin's stderr to the test log, then reap it.
pub async fn dump_stderr_and_kill(child: &mut tokio::process::Child) {
    // KILL FIRST: while the child is alive it holds the write end of the stderr
    // pipe open, so `read_to_end` on a live child blocks forever (the pipe never
    // EOFs). Killing first closes the write end, so the buffered stderr drains
    // then EOFs. The `timeout` is a belt-and-suspenders guard so this can never
    // hang again even if the kill races.
    child.kill().await.ok();
    if let Some(mut err_pipe) = child.stderr.take() {
        let mut stderr_buf = Vec::new();
        let _ = tokio::time::timeout(
            Duration::from_secs(5),
            err_pipe.read_to_end(&mut stderr_buf),
        )
        .await;
        eprintln!(
            "envoy-bin stderr:\n{}",
            String::from_utf8_lossy(&stderr_buf)
        );
    }
    let _ = child.wait().await;
}

// ── shared YAML block builders (byte-identical copies only) ───────────────────

/// A STATIC listener named `http1_listener` binding `127.0.0.1:<listener_port>`
/// whose HCM is RDS-configured (`stat_prefix: ingress_http1`; NO inline
/// `route_config`; the route table arrives from the RDS file at `rds_path`).
/// `route_config_name` must match a RouteConfiguration name in the RDS file.
pub fn rds_listener_block(listener_port: u16, route_config_name: &str, rds_path: &str) -> String {
    format!(
        r#"    - name: http1_listener
      address:
        socket_address:
          address: 127.0.0.1
          port_value: {listener_port}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http1
                codec_type: HTTP1
                rds:
                  route_config_name: {route_config_name}
                  config_source:
                    resource_api_version: V3
                    path_config_source:
                      path: {rds_path}
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#
    )
}

/// A `type: EDS` cluster `eds_backend` whose endpoints arrive from the EDS file at
/// `eds_path` (NO inline `load_assignment`, NO HC / OD — a PLAIN EDS cluster).
pub fn eds_cluster_block(eds_path: &str) -> String {
    format!(
        r#"    - name: eds_backend
      type: EDS
      lb_policy: ROUND_ROBIN
      eds_cluster_config:
        eds_config:
          resource_api_version: V3
          path_config_source:
            path: {eds_path}
"#
    )
}

/// A STATIC `static_backend` cluster pointing at `backend_port`, rendered as one
/// `static_resources.clusters` list item (6-space mapping-key indent under `- `).
pub fn static_backend_cluster_block(backend_port: u16) -> String {
    format!(
        r#"    - name: static_backend
      type: STRICT_DNS
      dns_lookup_family: V4_ONLY
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: static_backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: {{ address: 127.0.0.1, port_value: {backend_port} }}
"#
    )
}

/// The CDS file body (the fixture envoy-rust shape): one STRICT_DNS
/// `dynamic_backend` pointing at `backend_port`.
pub fn cds_file(backend_port: u16) -> String {
    format!(
        r#"resources:
  - "@type": type.googleapis.com/envoy.config.cluster.v3.Cluster
    name: dynamic_backend
    type: STRICT_DNS
    dns_lookup_family: V4_ONLY
    lb_policy: ROUND_ROBIN
    load_assignment:
      cluster_name: dynamic_backend
      endpoints:
        - lb_endpoints:
            - endpoint:
                address:
                  socket_address: {{ address: 127.0.0.1, port_value: {backend_port} }}
"#
    )
}
