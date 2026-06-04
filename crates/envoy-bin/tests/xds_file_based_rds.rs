//! Phase 20 Task 8 (ADR-0051/0052): in-process backstop for file-based RDS. Boots
//! the real `envoy-bin` binary as a subprocess and exercises the paths the
//! differential fixture (0028) CANNOT — the negative/fatal route-source paths —
//! plus a happy-path replica and an inertness witness.
//!
//! Per ADR-0051/0052 these tests ARE the recorded-divergence proof: envoy-rust
//! treats a missing OR malformed RDS file as a FATAL startup error (L4), treats a
//! `route_config_name` mismatch as fatal (RdsRouteConfigNotFound), treats an RDS
//! route to a cluster in NEITHER list as fatal (UnknownCluster — Envoy would
//! warn-and-serve a 503 route), and enforces exactly-one-of `route_config`/`rds`
//! per HCM (AmbiguousRouteSource / MissingRouteSource). A deliberately-broken
//! Envoy-side fixture is not a thing this project does, so the divergence is
//! recorded HERE.
//!
//! The helper block (`reserve_port`/`wait_ready`/`http1_oneshot`/`admin_get_body`/
//! `scrape_admin_stats`/`assert_stat`/`spawn_backend`/`serve_backend_conn`/
//! `cds_file`/`static_backend_cluster_block`/`write_file`/`write_bootstrap`/
//! `spawn_envoy_bin`/`assert_fatal_startup`) is COPIED VERBATIM from the phase-19
//! LDS backstop (`xds_file_based_lds.rs`), which itself copied from the phase-18
//! CDS backstop. The M18-9 "extract a shared test-support crate" item remains
//! open (now N≥4: the CDS/LDS/RDS backstops + per-fixture consts duplication), so
//! copying is the established, tracked pattern (PLAN C17 / the phase-19
//! carryforward keep the extraction a future hardening task).
//!
//! Eight paths (each boots its own envoy-bin instance):
//!
//!   (i)    happy path — bootstrap with `cds_config` (→ a CDS file defining
//!          `dynamic_backend`) + a STATIC `static_backend` cluster + a STATIC
//!          listener whose HCM is RDS-configured (`rds.route_config_name =
//!          local_route`, `path_config_source` → a temp RDS file). Boot succeeds
//!          → GET /static → 200, GET /dynamic → 200; `/stats` shows the five
//!          `http.<prefix>.rds.local_route.*` names at their L3 values;
//!          `/config_dump` carries a `RoutesConfigDump` whose
//!          `dynamic_route_configs[0].route_config.name == "local_route"`.
//!
//!   (ii)   missing RDS file → process EXITS non-zero (RdsFileError, "reading RDS
//!          file"); the listener port NEVER accepts connections.
//!
//!   (iii)  malformed RDS file → fatal (RdsParseError, "parsing RDS file").
//!
//!   (iv)   `route_config_name` mismatch → fatal (RdsRouteConfigNotFound). The RDS
//!          file defines `other_route`; the HCM wants `local_route`.
//!
//!   (v)    RDS route to a cluster in NEITHER list → fatal (UnknownCluster). The
//!          route targets cluster `nope` (Envoy would warn-and-serve a 503 route).
//!
//!   (vi)   both `route_config` AND `rds` configured → fatal (AmbiguousRouteSource).
//!
//!   (vii)  neither route source → fatal (MissingRouteSource).
//!
//!   (viii) inertness (§5.2) — a CDS-only bootstrap with a static listener whose
//!          HCM carries an INLINE `route_config` (NO rds) → boot succeeds;
//!          `/config_dump` does NOT contain `"RoutesConfigDump"` and `/stats`
//!          carries NO name containing `.rds.`.
//!
//! Boot/harness discipline copied verbatim from the LDS backstop:
//! `tokio::process::Command` + `.kill_on_drop(true)` + `wait_ready` polling.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::io::Write;
use std::net::{SocketAddr, TcpListener as StdListener};
use std::process::Stdio;
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

/// Single one-shot H1 request over a fresh downstream conn (`Connection: close`):
/// writes the request, reads the status line + headers + `Content-Length`-bounded
/// body. Returns `(status, body)`.
async fn http1_oneshot(hcm: SocketAddr, path: &str) -> (u16, Vec<u8>) {
    let mut stream = tokio::time::timeout(Duration::from_secs(10), TcpStream::connect(hcm))
        .await
        .expect("downstream connect timeout")
        .expect("downstream connect");
    let req = format!("GET {path} HTTP/1.1\r\nHost: dynamic_backend\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes()).await.expect("write");

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
    (status, body)
}

/// Open a fresh TCP conn to admin, GET `path`, read the whole response, split off
/// the body. Returns the raw body bytes.
async fn admin_get_body(admin: SocketAddr, path: &str) -> Vec<u8> {
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
async fn scrape_admin_stats(admin: SocketAddr) -> HashMap<String, u64> {
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

// ── in-process backend ──────────────────────────────────────────────────────

/// Spawn an in-process H1 backend that replies to every request with a 200 whose
/// body is the fixed `body` string. Returns the bound port. The backend serves a
/// keep-alive request loop per connection (honoring `Connection: close`).
async fn spawn_backend(body: &'static str) -> u16 {
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
            tokio::spawn(serve_backend_conn(sock, body));
        }
    });
    port
}

async fn serve_backend_conn(mut sock: TcpStream, body: &'static str) {
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

// ── bootstrap / file builders ─────────────────────────────────────────────────

/// The RDS file body (the `rds-envoy-rust.yaml` shape, mirroring fixture 0028's
/// `rds.yaml`): a bare `resources:` envelope carrying one `@type`-tagged
/// `RouteConfiguration` named `route_name`, routing `/static` → `static_cluster`
/// and `/dynamic` → `dynamic_cluster`. envoy-rust's RouteConfiguration /
/// VirtualHost use `deny_unknown_fields`, so NO Envoy-only fields here.
fn rds_file(route_name: &str, static_cluster: &str, dynamic_cluster: &str) -> String {
    format!(
        r#"resources:
  - "@type": type.googleapis.com/envoy.config.route.v3.RouteConfiguration
    name: {route_name}
    virtual_hosts:
      - name: backend_vh
        domains: ["*"]
        routes:
          - match: {{ prefix: "/static" }}
            route: {{ cluster: {static_cluster} }}
          - match: {{ prefix: "/dynamic" }}
            route: {{ cluster: {dynamic_cluster} }}
"#
    )
}

/// The CDS file body (the fixture-0028 envoy-rust shape): one STRICT_DNS
/// `dynamic_backend` pointing at `backend_port`.
fn cds_file(backend_port: u16) -> String {
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

/// A STATIC `static_backend` cluster pointing at `backend_port`, rendered as one
/// `static_resources.clusters` list item (6-space mapping-key indent under `- `).
fn static_backend_cluster_block(backend_port: u16) -> String {
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

/// A STATIC listener named `http1_listener` binding `127.0.0.1:<listener_port>`
/// whose HCM is RDS-configured: NO inline `route_config`; the route table arrives
/// from the RDS file at `rds_path`. `route_config_name` must match a
/// RouteConfiguration name in the RDS file (`local_route` in the happy path).
/// Rendered as one `static_resources.listeners` list item.
fn rds_listener_block(listener_port: u16, route_config_name: &str, rds_path: &str) -> String {
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

/// A STATIC listener named `http1_listener` whose HCM carries an INLINE
/// `route_config` (NO rds) routing `/static` → `static_backend` and `/dynamic` →
/// `dynamic_backend`. The inertness witness (case viii): an HCM with no rds.
fn inline_listener_block(listener_port: u16) -> String {
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
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: backend_vh
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/static" }}
                          route: {{ cluster: static_backend }}
                        - match: {{ prefix: "/dynamic" }}
                          route: {{ cluster: dynamic_backend }}
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#
    )
}

/// A STATIC listener whose HCM declares BOTH an inline `route_config` AND an `rds`
/// block (case vi: AmbiguousRouteSource).
fn ambiguous_listener_block(listener_port: u16, rds_path: &str) -> String {
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
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: backend_vh
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/static" }}
                          route: {{ cluster: static_backend }}
                rds:
                  route_config_name: local_route
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

/// A STATIC listener whose HCM declares NEITHER `route_config` NOR `rds` (case
/// vii: MissingRouteSource).
fn neither_listener_block(listener_port: u16) -> String {
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
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#
    )
}

/// Assemble a bootstrap: admin + optional `dynamic_resources.cds_config` +
/// `static_resources` whose `listeners:` / `clusters:` blocks are supplied by the
/// caller. When `cds_path` is `Some`, the `dynamic_resources.cds_config` sub-block
/// is emitted.
fn bootstrap(
    admin_port: u16,
    listeners_block: &str,
    clusters_block: &str,
    cds_path: Option<&str>,
) -> String {
    let mut dynamic_resources = String::new();
    if let Some(p) = cds_path {
        dynamic_resources.push_str(
            "dynamic_resources:\n  cds_config:\n    resource_api_version: V3\n    path_config_source:\n      path: ",
        );
        dynamic_resources.push_str(p);
        dynamic_resources.push('\n');
    }
    format!(
        r#"node: {{ id: envoy-rust-phase-20-backstop, cluster: envoy-rust-phase-20 }}
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: {admin_port}
{dynamic_resources}static_resources:
  listeners:
{listeners_block}  clusters:
{clusters_block}"#
    )
}

/// Write `contents` to `dir/name` and return the absolute path string.
fn write_file(dir: &std::path::Path, name: &str, contents: &str) -> String {
    let path = dir.join(name);
    std::fs::File::create(&path)
        .unwrap()
        .write_all(contents.as_bytes())
        .unwrap();
    path.to_str().unwrap().to_string()
}

/// Write `bootstrap` to `dir/envoy-rust.yaml` and return the path.
fn write_bootstrap(dir: &std::path::Path, bootstrap: &str) -> std::path::PathBuf {
    let cfg = dir.join("envoy-rust.yaml");
    std::fs::File::create(&cfg)
        .unwrap()
        .write_all(bootstrap.as_bytes())
        .unwrap();
    cfg
}

/// Spawn `envoy-bin -c <cfg>` with the established stdio discipline.
fn spawn_envoy_bin(cfg: &std::path::Path) -> tokio::process::Child {
    tokio::process::Command::new(env!("CARGO_BIN_EXE_envoy-bin"))
        .arg("-c")
        .arg(cfg)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn envoy-bin")
}

// ── (i) happy path ──────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn happy_path_rds_listener_serves_and_reports() {
    let listener_port = reserve_port();
    let admin_port = reserve_port();
    let hcm_addr: SocketAddr = format!("127.0.0.1:{listener_port}").parse().unwrap();
    let admin_addr: SocketAddr = format!("127.0.0.1:{admin_port}").parse().unwrap();
    // One backend; both clusters point at it (distinguished only by Host header).
    let backend_port = spawn_backend("from-backend").await;

    let dir = tempfile::tempdir().unwrap();
    let rds_path = write_file(
        dir.path(),
        "rds.yaml",
        &rds_file("local_route", "static_backend", "dynamic_backend"),
    );
    let cds_path = write_file(dir.path(), "cds.yaml", &cds_file(backend_port));
    let listeners = rds_listener_block(listener_port, "local_route", &rds_path);
    let clusters = static_backend_cluster_block(backend_port);
    let cfg = write_bootstrap(
        dir.path(),
        &bootstrap(admin_port, &listeners, &clusters, Some(&cds_path)),
    );

    let _envoy = spawn_envoy_bin(&cfg);
    wait_ready(hcm_addr, Duration::from_secs(10))
        .await
        .expect("envoy-bin HCM ready");
    wait_ready(admin_addr, Duration::from_secs(10))
        .await
        .expect("envoy-bin admin ready");

    // Data plane: the RDS-configured listener serves BOTH routes.
    let (s_static, b_static) = http1_oneshot(hcm_addr, "/static").await;
    assert_eq!(s_static, 200, "(i) GET /static → 200 via static_backend");
    assert_eq!(b_static, b"from-backend", "(i) /static echoes backend body");
    let (s_dynamic, b_dynamic) = http1_oneshot(hcm_addr, "/dynamic").await;
    assert_eq!(s_dynamic, 200, "(i) GET /dynamic → 200 via dynamic_backend");
    assert_eq!(
        b_dynamic, b"from-backend",
        "(i) /dynamic echoes backend body"
    );

    // /stats: the conditional per-HCM http.ingress_http1.rds.local_route.* family
    // at the L3 values.
    let s = scrape_admin_stats(admin_addr).await;
    assert_stat(&s, "http.ingress_http1.rds.local_route.update_attempt", 1);
    assert_stat(&s, "http.ingress_http1.rds.local_route.update_success", 1);
    assert_stat(&s, "http.ingress_http1.rds.local_route.update_failure", 0);
    assert_stat(&s, "http.ingress_http1.rds.local_route.update_rejected", 0);
    assert_stat(&s, "http.ingress_http1.rds.local_route.config_reload", 1);

    // /config_dump: a RoutesConfigDump entry whose first dynamic_route_configs
    // entry's route_config.name is `local_route`. The entry's configs[] index is
    // version-dependent (no dynamic listener here), so locate it by scanning.
    let dump = admin_get_body(admin_addr, "/config_dump").await;
    let dump_text = std::str::from_utf8(&dump).expect("config_dump utf8");
    assert!(
        dump_text.contains("RoutesConfigDump"),
        "(i) config_dump must contain RoutesConfigDump"
    );
    let json: serde_json::Value = serde_json::from_slice(&dump).expect("config_dump json");
    let configs = json
        .pointer("/configs")
        .and_then(|v| v.as_array())
        .expect("config_dump configs array");
    let routes = configs
        .iter()
        .find(|c| {
            c.pointer("/@type").and_then(|v| v.as_str())
                == Some("type.googleapis.com/envoy.admin.v3.RoutesConfigDump")
        })
        .expect("(i) config_dump must carry a RoutesConfigDump entry");
    assert_eq!(
        routes
            .pointer("/dynamic_route_configs/0/route_config/name")
            .and_then(|v| v.as_str()),
        Some("local_route"),
        "(i) dynamic_route_configs[0].route_config.name must be local_route"
    );
}

// ── (ii) missing RDS file ─────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn missing_rds_file_is_fatal() {
    let listener_port = reserve_port();
    let admin_port = reserve_port();
    let hcm_addr: SocketAddr = format!("127.0.0.1:{listener_port}").parse().unwrap();
    let backend_port = spawn_backend("from-backend").await;

    let dir = tempfile::tempdir().unwrap();
    // Point the rds path_config_source at a path that does NOT exist.
    let missing = dir.path().join("does-not-exist.yaml");
    let cds_path = write_file(dir.path(), "cds.yaml", &cds_file(backend_port));
    let listeners = rds_listener_block(listener_port, "local_route", missing.to_str().unwrap());
    let clusters = static_backend_cluster_block(backend_port);
    let cfg = write_bootstrap(
        dir.path(),
        &bootstrap(admin_port, &listeners, &clusters, Some(&cds_path)),
    );

    assert_fatal_startup(&cfg, hcm_addr, "reading RDS file", "(ii) missing RDS file").await;
}

// ── (iii) malformed RDS file ──────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn malformed_rds_file_is_fatal() {
    let listener_port = reserve_port();
    let admin_port = reserve_port();
    let hcm_addr: SocketAddr = format!("127.0.0.1:{listener_port}").parse().unwrap();
    let backend_port = spawn_backend("from-backend").await;

    let dir = tempfile::tempdir().unwrap();
    // A syntactically-broken RDS file (unclosed flow sequence).
    let rds_path = write_file(dir.path(), "rds.yaml", "resources: [unclosed");
    let cds_path = write_file(dir.path(), "cds.yaml", &cds_file(backend_port));
    let listeners = rds_listener_block(listener_port, "local_route", &rds_path);
    let clusters = static_backend_cluster_block(backend_port);
    let cfg = write_bootstrap(
        dir.path(),
        &bootstrap(admin_port, &listeners, &clusters, Some(&cds_path)),
    );

    assert_fatal_startup(
        &cfg,
        hcm_addr,
        "parsing RDS file",
        "(iii) malformed RDS file",
    )
    .await;
}

// ── (iv) route_config_name mismatch ───────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn rds_route_config_name_mismatch_is_fatal() {
    let listener_port = reserve_port();
    let admin_port = reserve_port();
    let hcm_addr: SocketAddr = format!("127.0.0.1:{listener_port}").parse().unwrap();
    let backend_port = spawn_backend("from-backend").await;

    let dir = tempfile::tempdir().unwrap();
    // The RDS file defines `other_route`; the HCM's rds.route_config_name wants
    // `local_route` → RdsRouteConfigNotFound (fatal startup).
    let rds_path = write_file(
        dir.path(),
        "rds.yaml",
        &rds_file("other_route", "static_backend", "dynamic_backend"),
    );
    let cds_path = write_file(dir.path(), "cds.yaml", &cds_file(backend_port));
    let listeners = rds_listener_block(listener_port, "local_route", &rds_path);
    let clusters = static_backend_cluster_block(backend_port);
    let cfg = write_bootstrap(
        dir.path(),
        &bootstrap(admin_port, &listeners, &clusters, Some(&cds_path)),
    );

    assert_fatal_startup(
        &cfg,
        hcm_addr,
        "not found",
        "(iv) RDS route_config_name mismatch",
    )
    .await;
}

// ── (v) RDS route to unknown cluster (recorded divergence) ────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn rds_route_to_unknown_cluster_is_fatal() {
    let listener_port = reserve_port();
    let admin_port = reserve_port();
    let hcm_addr: SocketAddr = format!("127.0.0.1:{listener_port}").parse().unwrap();
    let backend_port = spawn_backend("from-backend").await;

    let dir = tempfile::tempdir().unwrap();
    // The RDS route's `/dynamic` targets cluster `nope`, present in NEITHER the
    // static list (only `static_backend`) NOR the CDS list (only `dynamic_backend`)
    // → UnknownCluster (envoy-rust fails startup where Envoy warn-and-serves a 503).
    let rds_path = write_file(
        dir.path(),
        "rds.yaml",
        &rds_file("local_route", "static_backend", "nope"),
    );
    let cds_path = write_file(dir.path(), "cds.yaml", &cds_file(backend_port));
    let listeners = rds_listener_block(listener_port, "local_route", &rds_path);
    let clusters = static_backend_cluster_block(backend_port);
    let cfg = write_bootstrap(
        dir.path(),
        &bootstrap(admin_port, &listeners, &clusters, Some(&cds_path)),
    );

    assert_fatal_startup(
        &cfg,
        hcm_addr,
        "unknown cluster 'nope'",
        "(v) RDS route to unknown cluster",
    )
    .await;
}

// ── (vi) both route_config AND rds → AmbiguousRouteSource ──────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn both_route_config_and_rds_is_fatal() {
    let listener_port = reserve_port();
    let admin_port = reserve_port();
    let hcm_addr: SocketAddr = format!("127.0.0.1:{listener_port}").parse().unwrap();
    let backend_port = spawn_backend("from-backend").await;

    let dir = tempfile::tempdir().unwrap();
    let rds_path = write_file(
        dir.path(),
        "rds.yaml",
        &rds_file("local_route", "static_backend", "dynamic_backend"),
    );
    let cds_path = write_file(dir.path(), "cds.yaml", &cds_file(backend_port));
    let listeners = ambiguous_listener_block(listener_port, &rds_path);
    let clusters = static_backend_cluster_block(backend_port);
    let cfg = write_bootstrap(
        dir.path(),
        &bootstrap(admin_port, &listeners, &clusters, Some(&cds_path)),
    );

    assert_fatal_startup(
        &cfg,
        hcm_addr,
        "ambiguous route source",
        "(vi) both route_config and rds",
    )
    .await;
}

// ── (vii) neither route source → MissingRouteSource ────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn neither_route_source_is_fatal() {
    let listener_port = reserve_port();
    let admin_port = reserve_port();
    let hcm_addr: SocketAddr = format!("127.0.0.1:{listener_port}").parse().unwrap();
    let backend_port = spawn_backend("from-backend").await;

    let dir = tempfile::tempdir().unwrap();
    let cds_path = write_file(dir.path(), "cds.yaml", &cds_file(backend_port));
    let listeners = neither_listener_block(listener_port);
    let clusters = static_backend_cluster_block(backend_port);
    let cfg = write_bootstrap(
        dir.path(),
        &bootstrap(admin_port, &listeners, &clusters, Some(&cds_path)),
    );

    assert_fatal_startup(
        &cfg,
        hcm_addr,
        "missing route source",
        "(vii) neither route source",
    )
    .await;
}

/// Shared negative-path assertion: boot envoy-bin against `cfg`, expect a non-zero
/// exit within a budget, expect the combined stdout+stderr to contain `needle`,
/// and expect the listener `hcm_addr` to NEVER accept a connection.
async fn assert_fatal_startup(
    cfg: &std::path::Path,
    hcm_addr: SocketAddr,
    needle: &str,
    ctx: &str,
) {
    // Pipe BOTH stdout and stderr: the fatal `run()` error is surfaced via the
    // process's `tracing` ERROR diagnostic; the `tracing_subscriber::fmt`
    // subscriber writes to STDOUT (not stderr), so the RdsFileError / RdsParseError
    // / RdsRouteConfigNotFound / UnknownCluster / Ambiguous/MissingRouteSource
    // message text lands on stdout. Scan the combined streams so the assertion does
    // not depend on which fd is used.
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_envoy-bin"))
        .arg("-c")
        .arg(cfg)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn envoy-bin");

    let mut stdout_handle = child.stdout.take().expect("stdout piped");
    let mut stderr_handle = child.stderr.take().expect("stderr piped");
    let drain_out = async move {
        let mut s = String::new();
        let _ = stdout_handle.read_to_string(&mut s).await;
        s
    };
    let drain_err = async move {
        let mut s = String::new();
        let _ = stderr_handle.read_to_string(&mut s).await;
        s
    };

    // The process must exit NON-ZERO within a budget (fatal startup, L4).
    let (exit_res, out, err) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(child.wait(), drain_out, drain_err)
    })
    .await
    .unwrap_or_else(|_| panic!("{ctx}: envoy-bin did not exit within 10s (expected fatal)"));
    let exit = exit_res.expect("wait child");
    assert!(
        !exit.success(),
        "{ctx}: envoy-bin must exit NON-ZERO on an RDS load error, got {exit:?}"
    );

    // The listener never accepted: a connect attempt fails now (process is gone).
    let connect = TcpStream::connect(hcm_addr).await;
    assert!(
        connect.is_err(),
        "{ctx}: listener port {hcm_addr} must NEVER accept a connection on fatal startup"
    );

    // The diagnostic carries the specific RDS error text.
    let combined = format!("{out}{err}");
    assert!(
        combined.contains(needle),
        "{ctx}: process diagnostic must contain {needle:?}\nstdout+stderr was:\n{combined}"
    );
}

// ── (viii) inertness witness (§5.2) ───────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn no_rds_is_inert() {
    let listener_port = reserve_port();
    let admin_port = reserve_port();
    let hcm_addr: SocketAddr = format!("127.0.0.1:{listener_port}").parse().unwrap();
    let admin_addr: SocketAddr = format!("127.0.0.1:{admin_port}").parse().unwrap();
    let backend_port = spawn_backend("from-backend").await;

    let dir = tempfile::tempdir().unwrap();
    // CDS configured, ONE static listener whose HCM carries an INLINE route_config
    // (NO rds) — the inertness witness.
    let cds_path = write_file(dir.path(), "cds.yaml", &cds_file(backend_port));
    let listeners = inline_listener_block(listener_port);
    let clusters = static_backend_cluster_block(backend_port);
    let cfg = write_bootstrap(
        dir.path(),
        &bootstrap(admin_port, &listeners, &clusters, Some(&cds_path)),
    );

    let _envoy = spawn_envoy_bin(&cfg);
    wait_ready(hcm_addr, Duration::from_secs(10))
        .await
        .expect("envoy-bin HCM ready");
    wait_ready(admin_addr, Duration::from_secs(10))
        .await
        .expect("envoy-bin admin ready");

    // /stats carries NO name containing `.rds.`.
    let s = scrape_admin_stats(admin_addr).await;
    let rds_names: Vec<&String> = s.keys().filter(|k| k.contains(".rds.")).collect();
    assert!(
        rds_names.is_empty(),
        "(viii) inertness: NO `.rds.` stats expected, found {rds_names:?}"
    );

    // /config_dump does NOT contain RoutesConfigDump (the inertness witness, §5.2).
    let dump = admin_get_body(admin_addr, "/config_dump").await;
    let dump_text = std::str::from_utf8(&dump).expect("config_dump utf8");
    assert!(
        !dump_text.contains("RoutesConfigDump"),
        "(viii) inertness: config_dump must NOT contain RoutesConfigDump"
    );
}
