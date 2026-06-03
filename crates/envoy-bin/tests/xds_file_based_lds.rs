//! Phase 19 Task 8 (ADR-0050): in-process backstop for file-based LDS. Boots the
//! real `envoy-bin` binary as a subprocess and exercises the paths the
//! differential fixture (0027) CANNOT — the negative/fatal paths and the
//! static/dynamic listener collision — plus a happy-path replica.
//!
//! Per ADR-0050 these tests ARE the recorded-divergence proof: envoy-rust treats
//! a missing OR malformed LDS file as a FATAL startup error (L4), and treats a
//! dynamic-listener route to a cluster in NEITHER list as a FATAL startup error
//! (L6 — Envoy would warn-and-serve a 503 route), deliberately diverging. A
//! deliberately-broken Envoy-side fixture is not a thing this project does, so
//! the divergence is recorded HERE.
//!
//! The helper block (`reserve_port`/`wait_ready`/`http1_oneshot`/`admin_get_body`/
//! `scrape_admin_stats`/`assert_stat`/`spawn_backend`/`write_file`/
//! `spawn_envoy_bin`) is COPIED VERBATIM from the phase-18 CDS backstop
//! (`xds_file_based_cds.rs`); the M18-9 "extract a shared test-support crate"
//! item remains open, so copying is the established pattern (this is a known,
//! tracked duplication).
//!
//! Six paths (each boots its own envoy-bin instance):
//!
//!   (i)   happy path — bootstrap with `dynamic_resources.lds_config` (→ an LDS
//!         file defining `dynamic_listener` on a reserved port) + `cds_config`
//!         (→ a CDS file defining `dynamic_backend`) + ZERO static listeners + a
//!         static `static_backend` cluster. Boot succeeds → GET /static → 200,
//!         GET /dynamic → 200; `/stats` shows the six conditional
//!         `listener_manager.{lds.*,listener_added,total_listeners_active}`
//!         names; `/config_dump` `configs[2]` is the ListenersConfigDump
//!         carrying `dynamic_listener`; `/listeners` carries `dynamic_listener::`.
//!
//!   (ii)  missing LDS file (L4a) — `lds_config.path` → a nonexistent path → the
//!         process EXITS non-zero; the diagnostic carries the LdsFileError text
//!         ("reading LDS file"); the listener port NEVER accepts connections.
//!
//!   (iii) malformed LDS file (L4b) — the LDS file is `resources: [unclosed` →
//!         same fatal-exit triple; the diagnostic carries the LdsParseError text
//!         ("parsing LDS file").
//!
//!   (iv)  unresolved route (L6 — recorded divergence) — the LDS listener routes
//!         to cluster `nope`, present in neither the static nor the CDS list →
//!         fatal exit; the diagnostic carries the `ConfigError::UnknownCluster`
//!         rendering ("unknown cluster 'nope'").
//!
//!   (v)   static/dynamic listener collision (L7 — static wins) — bootstrap has a
//!         STATIC listener named `dynamic_listener` (port A) AND the LDS file
//!         defines `dynamic_listener` (port B). Boot SUCCEEDS; port A serves
//!         (GET /static → 200); port B refuses connections; `listener_added == 1`
//!         (the static one only — the collision-skipped dynamic listener does not
//!         count, since `all_listeners()` = the static listener).
//!
//!   (vi)  inertness (§5.2) — the fixture-0026 topology (CDS configured, NO
//!         lds_config, one static listener) → `/stats` carries NO
//!         `listener_manager.lds.*` names and NO `listener_manager.listener_added`;
//!         `/config_dump` does NOT contain `"ListenersConfigDump"` (the
//!         fixture-0026 compatibility witness, SPEC §5.2).
//!
//! Boot/harness discipline copied verbatim from the CDS backstop:
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

/// The LDS file body (the `lds-envoy-rust.yaml` shape): one `dynamic_listener`
/// binding `127.0.0.1:<listener_port>`, routing `/static` → `static_backend` and
/// `/dynamic` → `dynamic_backend`, via an `ingress_http1` HCM + router. No
/// `generate_request_id` / `request_headers_to_remove` (envoy-rust's parser
/// rejects them).
fn lds_file(listener_port: u16, static_cluster: &str, dynamic_cluster: &str) -> String {
    format!(
        r#"resources:
  - "@type": type.googleapis.com/envoy.config.listener.v3.Listener
    name: dynamic_listener
    address: {{ socket_address: {{ address: 127.0.0.1, port_value: {listener_port} }} }}
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
                        route: {{ cluster: {static_cluster} }}
                      - match: {{ prefix: "/dynamic" }}
                        route: {{ cluster: {dynamic_cluster} }}
              http_filters:
                - name: envoy.filters.http.router
                  typed_config:
                    "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#
    )
}

/// The CDS file body (the fixture-0026 envoy-rust shape): one STRICT_DNS
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

/// A STATIC listener named `dynamic_listener` (the collision name) binding
/// `127.0.0.1:<listener_port>` and routing `/static` → `static_backend` via an
/// `ingress_http1` HCM, rendered as one `static_resources.listeners` list item.
fn static_listener_block(listener_port: u16) -> String {
    format!(
        r#"    - name: dynamic_listener
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
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#
    )
}

/// Assemble a bootstrap: admin + optional `dynamic_resources.{lds,cds}_config` +
/// `static_resources` whose `listeners:` / `clusters:` blocks are supplied by the
/// caller (either may be empty). When `lds_path` / `cds_path` are `Some`, the
/// corresponding `dynamic_resources` sub-block is emitted.
fn bootstrap(
    admin_port: u16,
    listeners_block: &str,
    clusters_block: &str,
    lds_path: Option<&str>,
    cds_path: Option<&str>,
) -> String {
    let mut dynamic_resources = String::new();
    if lds_path.is_some() || cds_path.is_some() {
        dynamic_resources.push_str("dynamic_resources:\n");
        if let Some(p) = lds_path {
            dynamic_resources.push_str(&format!(
                "  lds_config:\n    resource_api_version: V3\n    path_config_source:\n      path: {p}\n"
            ));
        }
        if let Some(p) = cds_path {
            dynamic_resources.push_str(&format!(
                "  cds_config:\n    resource_api_version: V3\n    path_config_source:\n      path: {p}\n"
            ));
        }
    }
    format!(
        r#"node: {{ id: envoy-rust-phase-19-backstop, cluster: envoy-rust-phase-19 }}
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
async fn happy_path_dynamic_listener_serves_and_reports() {
    let listener_port = reserve_port();
    let admin_port = reserve_port();
    let hcm_addr: SocketAddr = format!("127.0.0.1:{listener_port}").parse().unwrap();
    let admin_addr: SocketAddr = format!("127.0.0.1:{admin_port}").parse().unwrap();
    // One backend; both clusters point at it (distinguished only by Host header).
    let backend_port = spawn_backend("from-backend").await;

    let dir = tempfile::tempdir().unwrap();
    let lds_path = write_file(
        dir.path(),
        "lds.yaml",
        &lds_file(listener_port, "static_backend", "dynamic_backend"),
    );
    let cds_path = write_file(dir.path(), "cds.yaml", &cds_file(backend_port));
    let clusters = static_backend_cluster_block(backend_port);
    let cfg = write_bootstrap(
        dir.path(),
        &bootstrap(
            admin_port,
            "", // ZERO static listeners — the listener arrives from the LDS file.
            &clusters,
            Some(&lds_path),
            Some(&cds_path),
        ),
    );

    let _envoy = spawn_envoy_bin(&cfg);
    wait_ready(hcm_addr, Duration::from_secs(10))
        .await
        .expect("envoy-bin HCM ready");
    wait_ready(admin_addr, Duration::from_secs(10))
        .await
        .expect("envoy-bin admin ready");

    // Data plane: the dynamic listener serves BOTH routes.
    let (s_static, _) = http1_oneshot(hcm_addr, "/static").await;
    assert_eq!(s_static, 200, "(i) GET /static → 200 via static_backend");
    let (s_dynamic, _) = http1_oneshot(hcm_addr, "/dynamic").await;
    assert_eq!(s_dynamic, 200, "(i) GET /dynamic → 200 via dynamic_backend");

    // /stats: the conditional listener_manager.lds.* family + listener_added +
    // total_listeners_active (L3).
    let s = scrape_admin_stats(admin_addr).await;
    assert_stat(&s, "listener_manager.lds.update_attempt", 1);
    assert_stat(&s, "listener_manager.lds.update_success", 1);
    assert_stat(&s, "listener_manager.lds.update_failure", 0);
    assert_stat(&s, "listener_manager.lds.update_rejected", 0);
    assert_stat(&s, "listener_manager.listener_added", 1);
    assert_stat(&s, "listener_manager.total_listeners_active", 1);

    // /config_dump: with BOTH lds_config AND cds_config, the order is
    // Bootstrap[0], Clusters[1], Listeners[2]. The ListenersConfigDump lands at
    // configs[2]; its first dynamic_listeners entry's name is `dynamic_listener`.
    let dump = admin_get_body(admin_addr, "/config_dump").await;
    let dump_text = std::str::from_utf8(&dump).expect("config_dump utf8");
    assert!(
        dump_text.contains("ListenersConfigDump"),
        "(i) config_dump must contain ListenersConfigDump"
    );
    assert!(
        dump_text.contains("dynamic_listener"),
        "(i) config_dump must contain dynamic_listener"
    );
    let json: serde_json::Value = serde_json::from_slice(&dump).expect("config_dump json");
    assert_eq!(
        json.pointer("/configs/2/@type").and_then(|v| v.as_str()),
        Some("type.googleapis.com/envoy.admin.v3.ListenersConfigDump"),
        "(i) config_dump configs[2] must be the ListenersConfigDump"
    );
    assert_eq!(
        json.pointer("/configs/2/dynamic_listeners/0/name")
            .and_then(|v| v.as_str()),
        Some("dynamic_listener"),
        "(i) config_dump dynamic_listeners[0].name must be dynamic_listener"
    );

    // /listeners: one line per listener, `<name>::<address>:<port>`.
    let listeners = admin_get_body(admin_addr, "/listeners").await;
    let listeners_text = std::str::from_utf8(&listeners).expect("listeners utf8");
    assert!(
        listeners_text.contains("dynamic_listener::"),
        "(i) /listeners must contain `dynamic_listener::`, got:\n{listeners_text}"
    );
}

// ── (ii) missing LDS file (L4a) ───────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn missing_lds_file_is_fatal() {
    let listener_port = reserve_port();
    let admin_port = reserve_port();
    let hcm_addr: SocketAddr = format!("127.0.0.1:{listener_port}").parse().unwrap();
    let backend_port = spawn_backend("from-backend").await;

    let dir = tempfile::tempdir().unwrap();
    // Point lds_config at a path that does NOT exist (never written).
    let missing = dir.path().join("does-not-exist.yaml");
    let cds_path = write_file(dir.path(), "cds.yaml", &cds_file(backend_port));
    let clusters = static_backend_cluster_block(backend_port);
    let cfg = write_bootstrap(
        dir.path(),
        &bootstrap(
            admin_port,
            "",
            &clusters,
            Some(missing.to_str().unwrap()),
            Some(&cds_path),
        ),
    );

    assert_fatal_startup(&cfg, hcm_addr, "reading LDS file", "(ii) missing LDS file").await;
}

// ── (iii) malformed LDS file (L4b) ────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn malformed_lds_file_is_fatal() {
    let listener_port = reserve_port();
    let admin_port = reserve_port();
    let hcm_addr: SocketAddr = format!("127.0.0.1:{listener_port}").parse().unwrap();
    let backend_port = spawn_backend("from-backend").await;

    let dir = tempfile::tempdir().unwrap();
    // A syntactically-broken LDS file (unclosed flow sequence).
    let lds_path = write_file(dir.path(), "lds.yaml", "resources: [unclosed");
    let cds_path = write_file(dir.path(), "cds.yaml", &cds_file(backend_port));
    let clusters = static_backend_cluster_block(backend_port);
    let cfg = write_bootstrap(
        dir.path(),
        &bootstrap(admin_port, "", &clusters, Some(&lds_path), Some(&cds_path)),
    );

    assert_fatal_startup(
        &cfg,
        hcm_addr,
        "parsing LDS file",
        "(iii) malformed LDS file",
    )
    .await;
}

// ── (iv) unresolved route (L6 — recorded divergence) ──────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn lds_route_to_unknown_cluster_is_fatal() {
    let listener_port = reserve_port();
    let admin_port = reserve_port();
    let hcm_addr: SocketAddr = format!("127.0.0.1:{listener_port}").parse().unwrap();
    let backend_port = spawn_backend("from-backend").await;

    let dir = tempfile::tempdir().unwrap();
    // The LDS listener's `/dynamic` route targets cluster `nope`, present in
    // NEITHER the static list (only `static_backend`) NOR the CDS list (only
    // `dynamic_backend`). The §5.7 post-merge re-validation raises UnknownCluster
    // (L6: envoy-rust fails startup where Envoy would warn-and-serve a 503 route).
    let lds_path = write_file(
        dir.path(),
        "lds.yaml",
        &lds_file(listener_port, "static_backend", "nope"),
    );
    let cds_path = write_file(dir.path(), "cds.yaml", &cds_file(backend_port));
    let clusters = static_backend_cluster_block(backend_port);
    let cfg = write_bootstrap(
        dir.path(),
        &bootstrap(admin_port, "", &clusters, Some(&lds_path), Some(&cds_path)),
    );

    assert_fatal_startup(
        &cfg,
        hcm_addr,
        "unknown cluster 'nope'",
        "(iv) LDS route to unknown cluster",
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
    // subscriber writes to STDOUT (not stderr), so the LdsFileError /
    // LdsParseError / UnknownCluster message text lands on stdout. Scan the
    // combined streams so the assertion does not depend on which fd is used.
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
        "{ctx}: envoy-bin must exit NON-ZERO on an LDS load error, got {exit:?}"
    );

    // The listener never accepted: a connect attempt fails now (process is gone).
    let connect = TcpStream::connect(hcm_addr).await;
    assert!(
        connect.is_err(),
        "{ctx}: listener port {hcm_addr} must NEVER accept a connection on fatal startup"
    );

    // The diagnostic carries the specific LDS error text.
    let combined = format!("{out}{err}");
    assert!(
        combined.contains(needle),
        "{ctx}: process diagnostic must contain {needle:?}\nstdout+stderr was:\n{combined}"
    );
}

// ── (v) static/dynamic listener collision (L7 — static wins) ──────────────────

#[tokio::test(flavor = "multi_thread")]
async fn static_dynamic_listener_collision_static_wins() {
    let static_port = reserve_port(); // port A — the STATIC dynamic_listener
    let dynamic_port = reserve_port(); // port B — the LDS dynamic_listener (skipped)
    let admin_port = reserve_port();
    let static_addr: SocketAddr = format!("127.0.0.1:{static_port}").parse().unwrap();
    let dynamic_addr: SocketAddr = format!("127.0.0.1:{dynamic_port}").parse().unwrap();
    let admin_addr: SocketAddr = format!("127.0.0.1:{admin_port}").parse().unwrap();
    let backend_port = spawn_backend("from-backend").await;

    let dir = tempfile::tempdir().unwrap();
    // The LDS file defines `dynamic_listener` on port B (routing /dynamic to the
    // CDS dynamic_backend, /static to static_backend).
    let lds_path = write_file(
        dir.path(),
        "lds.yaml",
        &lds_file(dynamic_port, "static_backend", "dynamic_backend"),
    );
    let cds_path = write_file(dir.path(), "cds.yaml", &cds_file(backend_port));
    // The bootstrap ALSO defines a STATIC listener named `dynamic_listener` on
    // port A (routing /static → static_backend). Static wins the name collision.
    let listeners = static_listener_block(static_port);
    let clusters = static_backend_cluster_block(backend_port);
    let cfg = write_bootstrap(
        dir.path(),
        &bootstrap(
            admin_port,
            &listeners,
            &clusters,
            Some(&lds_path),
            Some(&cds_path),
        ),
    );

    let _envoy = spawn_envoy_bin(&cfg);
    wait_ready(static_addr, Duration::from_secs(10))
        .await
        .expect("envoy-bin static listener (port A) ready");
    wait_ready(admin_addr, Duration::from_secs(10))
        .await
        .expect("envoy-bin admin ready");

    // Port A (the static listener) serves: GET /static → 200.
    let (status, _) = http1_oneshot(static_addr, "/static").await;
    assert_eq!(
        status, 200,
        "(v) collision: the STATIC listener on port A must serve /static"
    );

    // Port B has nothing bound: the dynamic listener was skipped (static won), so
    // `all_listeners().next()` = the static listener and port B refuses connects.
    let refused = TcpStream::connect(dynamic_addr).await;
    assert!(
        refused.is_err(),
        "(v) collision: port B ({dynamic_addr}) must refuse connects (dynamic listener skipped)"
    );

    // Stats: listener_added == 1 (the static listener ONLY — the collision-skipped
    // dynamic listener does not count, since all_listeners() = the static one).
    let s = scrape_admin_stats(admin_addr).await;
    assert_stat(&s, "listener_manager.listener_added", 1);
    assert_stat(&s, "listener_manager.total_listeners_active", 1);
}

// ── (vi) inertness witness (§5.2) ─────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn no_lds_config_is_inert() {
    let listener_port = reserve_port();
    let admin_port = reserve_port();
    let hcm_addr: SocketAddr = format!("127.0.0.1:{listener_port}").parse().unwrap();
    let admin_addr: SocketAddr = format!("127.0.0.1:{admin_port}").parse().unwrap();
    let backend_port = spawn_backend("from-backend").await;

    let dir = tempfile::tempdir().unwrap();
    // The fixture-0026 topology: CDS configured, NO lds_config, ONE static
    // listener (named `dynamic_listener` for reuse; the name is immaterial here).
    let cds_path = write_file(dir.path(), "cds.yaml", &cds_file(backend_port));
    let listeners = static_listener_block(listener_port);
    let clusters = static_backend_cluster_block(backend_port);
    let cfg = write_bootstrap(
        dir.path(),
        &bootstrap(
            admin_port,
            &listeners,
            &clusters,
            None, // NO lds_config — the inertness witness.
            Some(&cds_path),
        ),
    );

    let _envoy = spawn_envoy_bin(&cfg);
    wait_ready(hcm_addr, Duration::from_secs(10))
        .await
        .expect("envoy-bin HCM ready");
    wait_ready(admin_addr, Duration::from_secs(10))
        .await
        .expect("envoy-bin admin ready");

    // /stats carries NO listener_manager.lds.* names AND no listener_added.
    let s = scrape_admin_stats(admin_addr).await;
    let lds_names: Vec<&String> = s
        .keys()
        .filter(|k| k.starts_with("listener_manager.lds."))
        .collect();
    assert!(
        lds_names.is_empty(),
        "(vi) inertness: NO listener_manager.lds.* stats expected, found {lds_names:?}"
    );
    assert!(
        !s.contains_key("listener_manager.listener_added"),
        "(vi) inertness: listener_manager.listener_added must be ABSENT (no lds_config)"
    );

    // /config_dump does NOT contain ListenersConfigDump (the fixture-0026
    // compatibility witness, SPEC §5.2).
    let dump = admin_get_body(admin_addr, "/config_dump").await;
    let dump_text = std::str::from_utf8(&dump).expect("config_dump utf8");
    assert!(
        !dump_text.contains("ListenersConfigDump"),
        "(vi) inertness: config_dump must NOT contain ListenersConfigDump"
    );
}
