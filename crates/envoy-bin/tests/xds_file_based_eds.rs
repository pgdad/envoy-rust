//! Phase 21 Task 8 (ADR-0053/0054): in-process backstop for file-based EDS. Boots
//! the real `envoy-bin` binary as a subprocess and exercises the paths the
//! differential fixture (0029) CANNOT — the negative/fatal EDS paths (envoy-rust
//! is all-fatal where Envoy warm-503s — L4) plus a happy-path replica and an
//! inertness witness.
//!
//! Per ADR-0053/0054 these tests ARE the recorded-divergence proof. envoy-rust
//! treats EVERY EDS error class as a FATAL startup error (the ADR-0049 all-fatal
//! posture extended to EDS — L4): a missing EDS file (EdsFileError — the ONLY
//! class fatal on BOTH sides), a malformed EDS file (EdsParseError — Envoy
//! warm-503s), a missing/mismatched ClusterLoadAssignment
//! (EdsClusterLoadAssignmentNotFound — Envoy update_rejected + 503), and the
//! exactly-one-of-and-consistent consistency rejects: an inline `load_assignment`
//! on an EDS cluster (LoadAssignmentOnEdsCluster — L6 6a, Envoy ACCEPTS-and-
//! ignores; envoy-rust is stricter), `eds_cluster_config` on a non-EDS cluster
//! (EdsConfigOnNonEdsCluster — L6 6b), and an EDS cluster with neither
//! (MissingEdsClusterConfig — L6 6c). A deliberately-broken Envoy-side fixture is
//! not a thing this project does, so the divergence is recorded HERE.
//!
//! The helper block (`reserve_port`/`wait_ready`/`http1_oneshot`/`admin_get_body`/
//! `scrape_admin_stats`/`assert_stat`/`spawn_backend`/`serve_backend_conn`/
//! `write_file`/`write_bootstrap`/`spawn_envoy_bin`/`assert_fatal_startup`) is
//! COPIED VERBATIM from the phase-20 RDS backstop (`xds_file_based_rds.rs`), which
//! itself copied from the phase-19 LDS / phase-18 CDS backstops. The M18-9
//! "extract a shared test-support crate" item remains open (now N≥5: the
//! CDS/LDS/RDS/EDS backstops), so copying is the established, tracked pattern
//! (PLAN C18 / the phase-20 carryforward keep the extraction a future hardening
//! task).
//!
//! Eight paths (each boots its own envoy-bin instance):
//!
//!   (i)    happy path — bootstrap with a `type: EDS` cluster `eds_backend`
//!          (`eds_cluster_config.eds_config.path_config_source` → a temp EDS file
//!          defining a `eds_backend` ClusterLoadAssignment with one 127.0.0.1
//!          endpoint → an in-process backend) + a STATIC listener whose HCM
//!          INLINE-routes `/` → `eds_backend`. Boot succeeds → GET / → 200 + the
//!          backend body; `/stats` shows `cluster.eds_backend.update_*` 4-name
//!          subset at the L3 values (1/1/0/0) + `cluster.eds_backend.upstream_rq_total
//!          == 1`; `/config_dump` carries an `EndpointsConfigDump` whose
//!          `static_endpoint_configs[0].endpoint_config.cluster_name == "eds_backend"`.
//!          Membership gauges are NOT asserted (L3 narrowing).
//!
//!   (ii)   missing EDS file → process EXITS non-zero (EdsFileError); the L4
//!          agrees-with-Envoy class (missing FILE PATH is fatal on both).
//!
//!   (iii)  malformed EDS file → fatal (EdsParseError) — the L4 envoy-rust-diverges
//!          class (Envoy warm-503s).
//!
//!   (iv)   missing/mismatched ClusterLoadAssignment → fatal
//!          (EdsClusterLoadAssignmentNotFound). The EDS file defines `other_cla`;
//!          the cluster wants `eds_backend` (Envoy update_rejected + 503).
//!
//!   (v)    EDS cluster with an inline `load_assignment` → fatal
//!          (LoadAssignmentOnEdsCluster) — L6 6a (Envoy accepts-and-ignores).
//!
//!   (vi)   STATIC cluster with `eds_cluster_config` → fatal
//!          (EdsConfigOnNonEdsCluster) — L6 6b.
//!
//!   (vii)  EDS cluster with NEITHER `load_assignment` NOR `eds_cluster_config` →
//!          fatal (MissingEdsClusterConfig) — L6 6c.
//!
//!   (viii) inertness (§5.2 / L10) — a STATIC-only bootstrap (no EDS cluster) →
//!          boot succeeds; `/config_dump` does NOT contain `"EndpointsConfigDump"`
//!          and `/stats` carries NO `cluster.<name>.update_*` name.
//!
//! Boot/harness discipline copied verbatim from the RDS backstop:
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
    let req = format!("GET {path} HTTP/1.1\r\nHost: eds_backend\r\nConnection: close\r\n\r\n");
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

/// The EDS file body (the fixture-0029 `eds.yaml` shape): a bare `resources:`
/// envelope carrying one `@type`-tagged `ClusterLoadAssignment` named `cla_name`,
/// with one numeric-IP (127.0.0.1) endpoint at `backend_port`. The endpoint
/// address MUST be a numeric IP (L1). No Envoy-only fields (the shared-template
/// shape).
fn eds_file(cla_name: &str, backend_port: u16) -> String {
    format!(
        r#"resources:
  - "@type": type.googleapis.com/envoy.config.endpoint.v3.ClusterLoadAssignment
    cluster_name: {cla_name}
    endpoints:
      - lb_endpoints:
          - endpoint:
              address:
                socket_address: {{ address: 127.0.0.1, port_value: {backend_port} }}
"#
    )
}

/// A `type: EDS` cluster named `eds_backend` whose endpoints arrive from the EDS
/// file at `eds_path` (NO inline `load_assignment`). Rendered as one
/// `static_resources.clusters` list item (6-space mapping-key indent under `- `).
fn eds_cluster_block(eds_path: &str) -> String {
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

/// A `type: EDS` cluster that ALSO carries an inline `load_assignment` (case v:
/// LoadAssignmentOnEdsCluster — L6 6a). The inline endpoint is a valid 127.0.0.1
/// endpoint, so the rejection is purely the exactly-one-of consistency check.
fn eds_cluster_with_inline_block(eds_path: &str, backend_port: u16) -> String {
    format!(
        r#"    - name: eds_backend
      type: EDS
      lb_policy: ROUND_ROBIN
      eds_cluster_config:
        eds_config:
          resource_api_version: V3
          path_config_source:
            path: {eds_path}
      load_assignment:
        cluster_name: eds_backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: {{ address: 127.0.0.1, port_value: {backend_port} }}
"#
    )
}

/// A STATIC cluster that wrongly carries an `eds_cluster_config` (case vi:
/// EdsConfigOnNonEdsCluster — L6 6b).
fn static_cluster_with_eds_config_block(eds_path: &str, backend_port: u16) -> String {
    format!(
        r#"    - name: eds_backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      eds_cluster_config:
        eds_config:
          resource_api_version: V3
          path_config_source:
            path: {eds_path}
      load_assignment:
        cluster_name: eds_backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: {{ address: 127.0.0.1, port_value: {backend_port} }}
"#
    )
}

/// A `type: EDS` cluster with NEITHER `load_assignment` NOR `eds_cluster_config`
/// (case vii: MissingEdsClusterConfig — L6 6c).
fn eds_cluster_neither_block() -> String {
    r#"    - name: eds_backend
      type: EDS
      lb_policy: ROUND_ROBIN
"#
    .to_string()
}

/// A plain STATIC cluster named `static_backend` pointing at `backend_port` (the
/// inertness witness's only cluster — case viii). Rendered as one
/// `static_resources.clusters` list item.
fn static_backend_cluster_block(backend_port: u16) -> String {
    format!(
        r#"    - name: static_backend
      type: STATIC
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
/// whose HCM INLINE-routes `/` → `cluster_name`. Rendered as one
/// `static_resources.listeners` list item.
fn inline_listener_block(listener_port: u16, cluster_name: &str) -> String {
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
                        - match: {{ prefix: "/" }}
                          route: {{ cluster: {cluster_name} }}
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#
    )
}

/// Assemble a bootstrap: admin + `static_resources` whose `listeners:` /
/// `clusters:` blocks are supplied by the caller. No `dynamic_resources` — the EDS
/// cluster is STATIC-but-EDS (its `load_assignment` arrives from the EDS file at
/// boot via `eds_cluster_config`, NOT from CDS).
fn bootstrap(admin_port: u16, listeners_block: &str, clusters_block: &str) -> String {
    format!(
        r#"node: {{ id: envoy-rust-phase-21-backstop, cluster: envoy-rust-phase-21 }}
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: {admin_port}
static_resources:
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
async fn happy_path_eds_cluster_serves_and_reports() {
    let listener_port = reserve_port();
    let admin_port = reserve_port();
    let hcm_addr: SocketAddr = format!("127.0.0.1:{listener_port}").parse().unwrap();
    let admin_addr: SocketAddr = format!("127.0.0.1:{admin_port}").parse().unwrap();
    let backend_port = spawn_backend("from-backend").await;

    let dir = tempfile::tempdir().unwrap();
    let eds_path = write_file(
        dir.path(),
        "eds.yaml",
        &eds_file("eds_backend", backend_port),
    );
    let listeners = inline_listener_block(listener_port, "eds_backend");
    let clusters = eds_cluster_block(&eds_path);
    let cfg = write_bootstrap(dir.path(), &bootstrap(admin_port, &listeners, &clusters));

    let _envoy = spawn_envoy_bin(&cfg);
    wait_ready(hcm_addr, Duration::from_secs(10))
        .await
        .expect("envoy-bin HCM ready");
    wait_ready(admin_addr, Duration::from_secs(10))
        .await
        .expect("envoy-bin admin ready");

    // Data plane: the EDS-supplied endpoint serves `/`.
    let (status, body) = http1_oneshot(hcm_addr, "/").await;
    assert_eq!(status, 200, "(i) GET / → 200 via eds_backend");
    assert_eq!(body, b"from-backend", "(i) / echoes backend body");

    // /stats: the conditional per-cluster cluster.eds_backend.update_* 4-name
    // subset at the L3 values (1/1/0/0); the data-plane witness upstream_rq_total
    // == 1. Membership gauges are NOT asserted (L3 narrowing).
    let s = scrape_admin_stats(admin_addr).await;
    assert_stat(&s, "cluster.eds_backend.update_attempt", 1);
    assert_stat(&s, "cluster.eds_backend.update_success", 1);
    assert_stat(&s, "cluster.eds_backend.update_failure", 0);
    assert_stat(&s, "cluster.eds_backend.update_empty", 0);
    assert_stat(&s, "cluster.eds_backend.upstream_rq_total", 1);

    // /config_dump: an EndpointsConfigDump entry whose first static_endpoint_configs
    // entry's endpoint_config.cluster_name is `eds_backend`.
    let dump = admin_get_body(admin_addr, "/config_dump").await;
    let dump_text = std::str::from_utf8(&dump).expect("config_dump utf8");
    assert!(
        dump_text.contains("EndpointsConfigDump"),
        "(i) config_dump must contain EndpointsConfigDump"
    );
    let json: serde_json::Value = serde_json::from_slice(&dump).expect("config_dump json");
    let configs = json
        .pointer("/configs")
        .and_then(|v| v.as_array())
        .expect("config_dump configs array");
    let endpoints = configs
        .iter()
        .find(|c| {
            c.pointer("/@type").and_then(|v| v.as_str())
                == Some("type.googleapis.com/envoy.admin.v3.EndpointsConfigDump")
        })
        .expect("(i) config_dump must carry an EndpointsConfigDump entry");
    assert_eq!(
        endpoints
            .pointer("/static_endpoint_configs/0/endpoint_config/cluster_name")
            .and_then(|v| v.as_str()),
        Some("eds_backend"),
        "(i) static_endpoint_configs[0].endpoint_config.cluster_name must be eds_backend"
    );
}

// ── (ii) missing EDS file ─────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn missing_eds_file_is_fatal() {
    let listener_port = reserve_port();
    let admin_port = reserve_port();
    let hcm_addr: SocketAddr = format!("127.0.0.1:{listener_port}").parse().unwrap();

    let dir = tempfile::tempdir().unwrap();
    // Point the eds_cluster_config path at a file that does NOT exist. (No backend
    // is spawned: startup fails before any endpoint is consulted.)
    let missing = dir.path().join("does-not-exist.yaml");
    let listeners = inline_listener_block(listener_port, "eds_backend");
    let clusters = eds_cluster_block(missing.to_str().unwrap());
    let cfg = write_bootstrap(dir.path(), &bootstrap(admin_port, &listeners, &clusters));

    assert_fatal_startup(&cfg, hcm_addr, "EDS file error", "(ii) missing EDS file").await;
}

// ── (iii) malformed EDS file ──────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn malformed_eds_file_is_fatal() {
    let listener_port = reserve_port();
    let admin_port = reserve_port();
    let hcm_addr: SocketAddr = format!("127.0.0.1:{listener_port}").parse().unwrap();

    let dir = tempfile::tempdir().unwrap();
    // A syntactically-broken EDS file (unclosed flow sequence).
    let eds_path = write_file(dir.path(), "eds.yaml", "resources: [unclosed");
    let listeners = inline_listener_block(listener_port, "eds_backend");
    let clusters = eds_cluster_block(&eds_path);
    let cfg = write_bootstrap(dir.path(), &bootstrap(admin_port, &listeners, &clusters));

    assert_fatal_startup(
        &cfg,
        hcm_addr,
        "EDS file parse error",
        "(iii) malformed EDS file",
    )
    .await;
}

// ── (iv) missing/mismatched ClusterLoadAssignment ──────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn eds_cla_mismatch_is_fatal() {
    let listener_port = reserve_port();
    let admin_port = reserve_port();
    let hcm_addr: SocketAddr = format!("127.0.0.1:{listener_port}").parse().unwrap();
    let backend_port = spawn_backend("from-backend").await;

    let dir = tempfile::tempdir().unwrap();
    // The EDS file defines `other_cla`; the cluster's selection key is `eds_backend`
    // → EdsClusterLoadAssignmentNotFound (fatal startup).
    let eds_path = write_file(dir.path(), "eds.yaml", &eds_file("other_cla", backend_port));
    let listeners = inline_listener_block(listener_port, "eds_backend");
    let clusters = eds_cluster_block(&eds_path);
    let cfg = write_bootstrap(dir.path(), &bootstrap(admin_port, &listeners, &clusters));

    assert_fatal_startup(
        &cfg,
        hcm_addr,
        "ClusterLoadAssignment",
        "(iv) EDS CLA mismatch",
    )
    .await;
}

// ── (v) EDS cluster with an inline load_assignment → LoadAssignmentOnEdsCluster ─

#[tokio::test(flavor = "multi_thread")]
async fn eds_cluster_with_inline_load_assignment_is_fatal() {
    let listener_port = reserve_port();
    let admin_port = reserve_port();
    let hcm_addr: SocketAddr = format!("127.0.0.1:{listener_port}").parse().unwrap();
    let backend_port = spawn_backend("from-backend").await;

    let dir = tempfile::tempdir().unwrap();
    let eds_path = write_file(
        dir.path(),
        "eds.yaml",
        &eds_file("eds_backend", backend_port),
    );
    let listeners = inline_listener_block(listener_port, "eds_backend");
    let clusters = eds_cluster_with_inline_block(&eds_path, backend_port);
    let cfg = write_bootstrap(dir.path(), &bootstrap(admin_port, &listeners, &clusters));

    assert_fatal_startup(
        &cfg,
        hcm_addr,
        "must not carry an inline `load_assignment`",
        "(v) EDS cluster with inline load_assignment",
    )
    .await;
}

// ── (vi) STATIC cluster with eds_cluster_config → EdsConfigOnNonEdsCluster ──────

#[tokio::test(flavor = "multi_thread")]
async fn static_cluster_with_eds_config_is_fatal() {
    let listener_port = reserve_port();
    let admin_port = reserve_port();
    let hcm_addr: SocketAddr = format!("127.0.0.1:{listener_port}").parse().unwrap();
    let backend_port = spawn_backend("from-backend").await;

    let dir = tempfile::tempdir().unwrap();
    let eds_path = write_file(
        dir.path(),
        "eds.yaml",
        &eds_file("eds_backend", backend_port),
    );
    let listeners = inline_listener_block(listener_port, "eds_backend");
    let clusters = static_cluster_with_eds_config_block(&eds_path, backend_port);
    let cfg = write_bootstrap(dir.path(), &bootstrap(admin_port, &listeners, &clusters));

    assert_fatal_startup(
        &cfg,
        hcm_addr,
        "`eds_cluster_config` set on a non-EDS cluster",
        "(vi) STATIC cluster with eds_cluster_config",
    )
    .await;
}

// ── (vii) EDS cluster with neither → MissingEdsClusterConfig ───────────────────

#[tokio::test(flavor = "multi_thread")]
async fn eds_cluster_with_neither_is_fatal() {
    let listener_port = reserve_port();
    let admin_port = reserve_port();
    let hcm_addr: SocketAddr = format!("127.0.0.1:{listener_port}").parse().unwrap();

    let dir = tempfile::tempdir().unwrap();
    let listeners = inline_listener_block(listener_port, "eds_backend");
    let clusters = eds_cluster_neither_block();
    let cfg = write_bootstrap(dir.path(), &bootstrap(admin_port, &listeners, &clusters));

    assert_fatal_startup(
        &cfg,
        hcm_addr,
        "requires `eds_cluster_config`",
        "(vii) EDS cluster with neither",
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
    // subscriber writes to STDOUT (not stderr), so the EdsFileError / EdsParseError
    // / EdsClusterLoadAssignmentNotFound / LoadAssignmentOnEdsCluster /
    // EdsConfigOnNonEdsCluster / MissingEdsClusterConfig message text lands on
    // stdout. Scan the combined streams so the assertion does not depend on which
    // fd is used.
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
        "{ctx}: envoy-bin must exit NON-ZERO on an EDS load error, got {exit:?}"
    );

    // The listener never accepted: a connect attempt fails now (process is gone).
    let connect = TcpStream::connect(hcm_addr).await;
    assert!(
        connect.is_err(),
        "{ctx}: listener port {hcm_addr} must NEVER accept a connection on fatal startup"
    );

    // The diagnostic carries the specific EDS error text.
    let combined = format!("{out}{err}");
    assert!(
        combined.contains(needle),
        "{ctx}: process diagnostic must contain {needle:?}\nstdout+stderr was:\n{combined}"
    );
}

// ── (viii) inertness witness (§5.2 / L10) ──────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn no_eds_is_inert() {
    let listener_port = reserve_port();
    let admin_port = reserve_port();
    let hcm_addr: SocketAddr = format!("127.0.0.1:{listener_port}").parse().unwrap();
    let admin_addr: SocketAddr = format!("127.0.0.1:{admin_port}").parse().unwrap();
    let backend_port = spawn_backend("from-backend").await;

    let dir = tempfile::tempdir().unwrap();
    // A STATIC-only bootstrap: one plain STATIC cluster, ONE static listener whose
    // HCM INLINE-routes `/` → `static_backend`. No EDS cluster anywhere — the
    // inertness witness.
    let listeners = inline_listener_block(listener_port, "static_backend");
    let clusters = static_backend_cluster_block(backend_port);
    let cfg = write_bootstrap(dir.path(), &bootstrap(admin_port, &listeners, &clusters));

    let _envoy = spawn_envoy_bin(&cfg);
    wait_ready(hcm_addr, Duration::from_secs(10))
        .await
        .expect("envoy-bin HCM ready");
    wait_ready(admin_addr, Duration::from_secs(10))
        .await
        .expect("envoy-bin admin ready");

    // /stats carries NO name matching `cluster.<name>.update_*` (the EDS family is
    // conditionally registered ONLY for `type: EDS` clusters — L10).
    let s = scrape_admin_stats(admin_addr).await;
    let update_names: Vec<&String> = s
        .keys()
        .filter(|k| k.starts_with("cluster.") && k.contains(".update_"))
        .collect();
    assert!(
        update_names.is_empty(),
        "(viii) inertness: NO `cluster.<name>.update_*` stats expected, found {update_names:?}"
    );

    // /config_dump does NOT contain EndpointsConfigDump (the inertness witness, §5.2).
    let dump = admin_get_body(admin_addr, "/config_dump").await;
    let dump_text = std::str::from_utf8(&dump).expect("config_dump utf8");
    assert!(
        !dump_text.contains("EndpointsConfigDump"),
        "(viii) inertness: config_dump must NOT contain EndpointsConfigDump"
    );
}
