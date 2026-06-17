//! 26 Task 8: in-process RDS hot-reload BACKSTOP — the deterministic LOCAL
//! complement to the `0034-xds-rds-hot-reload` differential fixture.
//!
//! The differential proves RDS hot-reload bilaterally, but ONLY on native-Linux
//! CI (under Docker Desktop virtiofs the upstream Envoy can't observe a bind-mount
//! reload) and via probe responses only. THIS backstop boots the real `envoy-bin`
//! binary as a NATIVE subprocess (NOT a container — the §6.2 watcher is POLL-based
//! mtime at ~1s cadence, so a native subprocess DOES observe a host-side
//! atomic-rename), performs reloads by atomic-renaming the RDS file, and scrapes
//! admin stats to assert the things the differential can't cleanly drive:
//!
//!   1. happy reload — flips the live route, ticks the §6.2-LOCKED counter
//!      taxonomy `1/1/0/0/1` → `2/2/0/0/2`, and the new table is reflected in
//!      `/config_dump` (P6).
//!   2. malformed-YAML reload — WARM-REJECTS (keeps last-good), ticks
//!      attempt + `update_failure`.
//!   3. `route_config_name`-absent reload — WARM-REJECTS, ticks
//!      attempt + `update_rejected`.
//!   4. unknown-cluster reload — WARM-REJECTS, ticks attempt + `update_rejected`.
//!      THE RECORDED DIVERGENCE: real Envoy ACCEPTS and serves a 503
//!      (`no_cluster`); envoy-rust re-validates and rejects (see the test comment).
//!   5. in-flight isolation (§5.4 / P7) — a request snapshotting the route table
//!      at entry completes 200 under the OLD table across a concurrent reload.
//!
//! The per-HCM RDS counters are `http.<stat_prefix>.rds.<route_config_name>.<name>`
//! for the 5 names `update_attempt / update_success / update_failure /
//! update_rejected / config_reload`. With `stat_prefix: ingress_http1` and
//! `route_config_name: local_route` → `http.ingress_http1.rds.local_route.*`.
//! `update_attempt` ALWAYS ticks (even on a rejected reload), so "wait until
//! `update_attempt == 2`" is the bounded convergence signal for a rejected reload.
//!
//! The helper block (`reserve_port`/`wait_ready`/`http1_oneshot`/`admin_get_body`/
//! `scrape_admin_stats`/`assert_stat`/`spawn_backend`/`serve_backend_conn`/
//! `write_file`/`write_bootstrap`/`spawn_envoy_bin` + the `bootstrap`/
//! `rds_listener_block`/`static_cluster_block` config builders) is COPIED from the
//! phase-20 RDS backstop (`xds_file_based_rds.rs`), which itself copied from the
//! LDS/CDS backstops. The "extract a shared test-support crate" item remains
//! deliberately deferred (now N≥5), so copying is the established, tracked pattern.
//!
//! Boot/harness discipline: `tokio::process::Command` + `.kill_on_drop(true)` +
//! `wait_ready` polling. The spawned echo backend is an IN-PROCESS tokio listener
//! (no helper binary to pre-build); only `CARGO_BIN_EXE_envoy-bin` needs building.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::io::Write;
use std::net::{SocketAddr, TcpListener as StdListener};
use std::path::Path;
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
/// body. Returns `(status, body)`. The `Host` header is `*`-domain-agnostic here
/// (every vhost in this backstop is `domains: ["*"]`, so the hardcoded Host always
/// matches — no custom-Host variant is needed).
async fn http1_oneshot(hcm: SocketAddr, path: &str) -> (u16, Vec<u8>) {
    let mut stream = tokio::time::timeout(Duration::from_secs(10), TcpStream::connect(hcm))
        .await
        .expect("downstream connect timeout")
        .expect("downstream connect");
    let req = format!("GET {path} HTTP/1.1\r\nHost: backend\r\nConnection: close\r\n\r\n");
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
    spawn_slow_backend(body, Duration::ZERO).await
}

/// 26 Task 8: like `spawn_backend` but sleeps `delay` before responding to each
/// request. The in-flight-isolation test (§5.4) routes to a SLOW backend so a
/// request is reliably mid-flight when the concurrent reload lands.
async fn spawn_slow_backend(body: &'static str, delay: Duration) -> u16 {
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

async fn serve_backend_conn(mut sock: TcpStream, body: &'static str, delay: Duration) {
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

// ── rds file builders ─────────────────────────────────────────────────────────

/// 26 Task 8: an RDS file body — RouteConfiguration `local_route`, one vhost
/// `domains: ["*"]`, route `match {prefix: "/probe"}` → `route: { cluster }`.
/// envoy-rust's RouteConfiguration / VirtualHost use `deny_unknown_fields`, so NO
/// Envoy-only fields here.
fn rds_routing_to(cluster: &str) -> String {
    rds_named_route(cluster, "local_route", "/probe")
}

/// 26 Task 8: a RouteConfiguration with a caller-chosen name + prefix → cluster.
/// The name-mismatch variant uses a name OTHER than `local_route`; the in-flight
/// test uses a `/slow` prefix.
fn rds_named_route(cluster: &str, route_config_name: &str, prefix: &str) -> String {
    format!(
        r#"resources:
  - "@type": type.googleapis.com/envoy.config.route.v3.RouteConfiguration
    name: {route_config_name}
    virtual_hosts:
      - name: backend_vh
        domains: ["*"]
        routes:
          - match: {{ prefix: "{prefix}" }}
            route: {{ cluster: {cluster} }}
"#
    )
}

/// A STATIC cluster `name` pointing at `backend_port`, rendered as one
/// `static_resources.clusters` list item (6-space mapping-key indent under `- `).
fn static_cluster_block(name: &str, backend_port: u16) -> String {
    format!(
        r#"    - name: {name}
      type: STRICT_DNS
      dns_lookup_family: V4_ONLY
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: {name}
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: {{ address: 127.0.0.1, port_value: {backend_port} }}
"#
    )
}

/// A STATIC listener named `http1_listener` binding `127.0.0.1:<listener_port>`
/// whose HCM is RDS-configured (`stat_prefix: ingress_http1`; NO inline
/// `route_config`; the route table arrives from the RDS file at `rds_path`).
/// `route_config_name` must match a RouteConfiguration name in the RDS file.
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

/// Assemble a bootstrap: admin + `static_resources` whose `listeners:` /
/// `clusters:` blocks are supplied by the caller. NO CDS — both clusters are
/// static (this backstop distinguishes them by `cluster.<name>.upstream_rq_total`,
/// not by a CDS vs static split).
fn bootstrap(admin_port: u16, listeners_block: &str, clusters_block: &str) -> String {
    format!(
        r#"node: {{ id: envoy-rust-phase-26-backstop, cluster: envoy-rust-phase-26 }}
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
fn write_file(dir: &Path, name: &str, contents: &str) -> String {
    let path = dir.join(name);
    std::fs::File::create(&path)
        .unwrap()
        .write_all(contents.as_bytes())
        .unwrap();
    path.to_str().unwrap().to_string()
}

/// Write `bootstrap` to `dir/envoy-rust.yaml` and return the path.
fn write_bootstrap(dir: &Path, bootstrap: &str) -> std::path::PathBuf {
    let cfg = dir.join("envoy-rust.yaml");
    std::fs::File::create(&cfg)
        .unwrap()
        .write_all(bootstrap.as_bytes())
        .unwrap();
    cfg
}

/// Spawn `envoy-bin -c <cfg>` with the established stdio discipline.
fn spawn_envoy_bin(cfg: &Path) -> tokio::process::Child {
    tokio::process::Command::new(env!("CARGO_BIN_EXE_envoy-bin"))
        .arg("-c")
        .arg(cfg)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn envoy-bin")
}

// ── hot-reload-specific helpers ───────────────────────────────────────────────

/// 26 Task 8: ATOMICALLY replace the RDS file's contents. Writes a SAME-DIR
/// sibling temp (`<target>.reload-tmp`) then `std::fs::rename`s it over `target`.
/// The §6.2 watcher detects the change by the file's mtime stepping forward; an
/// atomic rename guarantees the watcher only ever stats a COMPLETE file (an
/// in-place truncate-rewrite could expose a half-written file AND — depending on
/// timing — might not even tick the mtime). The sibling is on the SAME fs so the
/// rename is atomic.
fn atomic_rename_rds(target: &Path, new_contents: &str) {
    let tmp = target.with_extension("reload-tmp");
    std::fs::File::create(&tmp)
        .unwrap()
        .write_all(new_contents.as_bytes())
        .unwrap();
    std::fs::rename(&tmp, target).expect("atomic rename rds over target");
}

/// 26 Task 8: poll `/stats` at ~150ms until `stats[name] == expected` or `budget`
/// elapses. The bounded convergence signal — the watcher poll cadence is ~1s, so
/// a `budget` of ~8s gives several poll windows of slack. Panics (with the last
/// observed value) on timeout so a hung reload fails the test loudly rather than
/// proceeding against a stale table.
async fn wait_for_stat(admin: SocketAddr, name: &str, expected: u64, budget: Duration) {
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

/// Shared setup: a bootstrap with a STATIC H1 RDS-configured listener (initial rds
/// routing `/probe` → `backend_a`) + TWO static clusters `backend_a` / `backend_b`,
/// both pointing at ONE spawned echo backend (distinguished by
/// `cluster.<name>.upstream_rq_total`). Returns the live HCM/admin addrs, the rds
/// file path, the spawned child (kept alive by the caller), and the tempdir
/// (kept alive so the rds file survives the test).
struct Harness {
    hcm: SocketAddr,
    admin: SocketAddr,
    rds_path: std::path::PathBuf,
    _child: tokio::process::Child,
    _dir: tempfile::TempDir,
}

/// 26 Task 8: boot a two-cluster harness whose initial rds routes `/probe` (and,
/// when `extra_cluster` is `Some((name, port))`, that named cluster is appended to
/// the static set so the in-flight test can route `/slow` to a slow backend).
async fn boot_harness(initial_rds: &str, extra_cluster: Option<(&str, u16)>) -> Harness {
    let listener_port = reserve_port();
    let admin_port = reserve_port();
    let hcm: SocketAddr = format!("127.0.0.1:{listener_port}").parse().unwrap();
    let admin: SocketAddr = format!("127.0.0.1:{admin_port}").parse().unwrap();

    let dir = tempfile::tempdir().unwrap();
    // The rds file lives in the host's real fs tempdir so the watcher polls its
    // mtime. `boot_harness` reuses ONE backend port for both backend_a/backend_b —
    // the routing flip is observed via the per-cluster upstream_rq_total, not via
    // distinct bodies.
    let backend_port = spawn_backend("from-backend").await;
    let rds_path_str = write_file(dir.path(), "rds.yaml", initial_rds);
    let rds_path = std::path::PathBuf::from(&rds_path_str);

    let listeners = rds_listener_block(listener_port, "local_route", &rds_path_str);
    let mut clusters = static_cluster_block("backend_a", backend_port);
    clusters.push_str(&static_cluster_block("backend_b", backend_port));
    if let Some((name, port)) = extra_cluster {
        clusters.push_str(&static_cluster_block(name, port));
    }
    let cfg = write_bootstrap(dir.path(), &bootstrap(admin_port, &listeners, &clusters));

    let child = spawn_envoy_bin(&cfg);
    wait_ready(hcm, Duration::from_secs(10))
        .await
        .expect("envoy-bin HCM ready");
    wait_ready(admin, Duration::from_secs(10))
        .await
        .expect("envoy-bin admin ready");

    Harness {
        hcm,
        admin,
        rds_path,
        _child: child,
        _dir: dir,
    }
}

const C_ATTEMPT: &str = "http.ingress_http1.rds.local_route.update_attempt";
const C_SUCCESS: &str = "http.ingress_http1.rds.local_route.update_success";
const C_FAILURE: &str = "http.ingress_http1.rds.local_route.update_failure";
const C_REJECTED: &str = "http.ingress_http1.rds.local_route.update_rejected";
const C_RELOAD: &str = "http.ingress_http1.rds.local_route.config_reload";

/// Assert the full 5-name rds counter family in one shot.
fn assert_rds_counters(
    stats: &HashMap<String, u64>,
    attempt: u64,
    success: u64,
    failure: u64,
    rejected: u64,
    reload: u64,
) {
    assert_stat(stats, C_ATTEMPT, attempt);
    assert_stat(stats, C_SUCCESS, success);
    assert_stat(stats, C_FAILURE, failure);
    assert_stat(stats, C_REJECTED, rejected);
    assert_stat(stats, C_RELOAD, reload);
}

// ── 1: happy reload flips route + ticks counters + reflects in config_dump ────

#[tokio::test(flavor = "multi_thread")]
async fn happy_reload_flips_route_and_ticks_counters() {
    let h = boot_harness(&rds_routing_to("backend_a"), None).await;

    // Initial table routes /probe → backend_a.
    let (status, body) = http1_oneshot(h.hcm, "/probe").await;
    assert_eq!(status, 200, "initial /probe → 200 via backend_a");
    assert_eq!(body, b"from-backend");

    let s0 = scrape_admin_stats(h.admin).await;
    assert_stat(&s0, "cluster.backend_a.upstream_rq_total", 1);
    // Initial-load counter values per the §6.2-LOCKED taxonomy.
    assert_rds_counters(&s0, 1, 1, 0, 0, 1);

    // Atomic-rename the rds file → routing /probe → backend_b. The watcher (poll
    // cadence ~1s) observes the mtime step and runs the §6.2 reload pipeline.
    atomic_rename_rds(&h.rds_path, &rds_routing_to("backend_b"));
    wait_for_stat(h.admin, C_SUCCESS, 2, Duration::from_secs(8)).await;

    // The live table now routes /probe → backend_b.
    let (status, body) = http1_oneshot(h.hcm, "/probe").await;
    assert_eq!(status, 200, "post-reload /probe → 200 via backend_b");
    assert_eq!(body, b"from-backend");

    let s1 = scrape_admin_stats(h.admin).await;
    assert!(
        s1.get("cluster.backend_b.upstream_rq_total")
            .copied()
            .unwrap_or(0)
            >= 1,
        "post-reload request landed on backend_b"
    );
    // One successful reload ticks attempt + success + config_reload by 1.
    assert_rds_counters(&s1, 2, 2, 0, 0, 2);

    // P6: /config_dump reflects the live reloaded table — walk the RoutesConfigDump
    // entry's dynamic_route_configs[0] down to the first route's cluster.
    let dump = admin_get_body(h.admin, "/config_dump").await;
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
        .expect("config_dump must carry a RoutesConfigDump entry");
    let cluster = routes
        .pointer("/dynamic_route_configs/0/route_config/virtual_hosts/0/routes/0/route/cluster")
        .and_then(|v| v.as_str());
    assert_eq!(
        cluster,
        Some("backend_b"),
        "config_dump live table must route /probe → backend_b after reload"
    );
}

// ── 2: malformed reload warm-rejects (update_failure), keeps last-good ────────

#[tokio::test(flavor = "multi_thread")]
async fn malformed_reload_warm_rejects_and_keeps_last_good() {
    let h = boot_harness(&rds_routing_to("backend_a"), None).await;

    let (status, _) = http1_oneshot(h.hcm, "/probe").await;
    assert_eq!(status, 200, "initial /probe → 200 via backend_a");

    // Atomic-rename → a syntactically-broken RDS file (unclosed flow sequence).
    // The reload reparse fails (IO/parse class) → WARM-REJECT: keep last-good,
    // tick attempt + update_failure. update_attempt ALWAYS ticks, so == 2 is the
    // convergence signal for this rejected reload.
    atomic_rename_rds(&h.rds_path, "resources: [unclosed");
    wait_for_stat(h.admin, C_ATTEMPT, 2, Duration::from_secs(8)).await;

    let s = scrape_admin_stats(h.admin).await;
    // attempt + failure ticked; success/rejected/config_reload unchanged.
    assert_rds_counters(&s, 2, 1, 1, 0, 1);

    // Last-good kept: /probe still routes to backend_a (NOT backend_b).
    let (status, _) = http1_oneshot(h.hcm, "/probe").await;
    assert_eq!(
        status, 200,
        "post-warm-reject /probe → 200 (last-good kept)"
    );
    let s2 = scrape_admin_stats(h.admin).await;
    assert_stat(&s2, "cluster.backend_a.upstream_rq_total", 2);
    assert_eq!(
        s2.get("cluster.backend_b.upstream_rq_total")
            .copied()
            .unwrap_or(0),
        0,
        "no request ever routed to backend_b (reload was rejected)"
    );
}

// ── 3: route_config_name-absent reload warm-rejects (update_rejected) ─────────

#[tokio::test(flavor = "multi_thread")]
async fn name_absent_reload_warm_rejects_and_keeps_last_good() {
    let h = boot_harness(&rds_routing_to("backend_a"), None).await;

    let (status, _) = http1_oneshot(h.hcm, "/probe").await;
    assert_eq!(status, 200, "initial /probe → 200 via backend_a");

    // Atomic-rename → a RouteConfiguration named `other_route` (the HCM's rds wants
    // `local_route`). The name is absent from the reloaded file → WARM-REJECT:
    // keep last-good, tick attempt + update_rejected.
    atomic_rename_rds(
        &h.rds_path,
        &rds_named_route("backend_b", "other_route", "/probe"),
    );
    wait_for_stat(h.admin, C_ATTEMPT, 2, Duration::from_secs(8)).await;

    let s = scrape_admin_stats(h.admin).await;
    // attempt + rejected ticked; success/failure/config_reload unchanged.
    assert_rds_counters(&s, 2, 1, 0, 1, 1);

    // Last-good kept: /probe still routes to backend_a.
    let (status, _) = http1_oneshot(h.hcm, "/probe").await;
    assert_eq!(
        status, 200,
        "post-warm-reject /probe → 200 (last-good kept)"
    );
    let s2 = scrape_admin_stats(h.admin).await;
    assert_stat(&s2, "cluster.backend_a.upstream_rq_total", 2);
    assert_eq!(
        s2.get("cluster.backend_b.upstream_rq_total")
            .copied()
            .unwrap_or(0),
        0,
        "no request ever routed to backend_b (reload was rejected)"
    );
}

// ── 4: unknown-cluster reload warm-rejects (update_rejected) — DIVERGENCE ──────

#[tokio::test(flavor = "multi_thread")]
async fn unknown_cluster_reload_warm_rejects_recorded_divergence() {
    let h = boot_harness(&rds_routing_to("backend_a"), None).await;

    let (status, _) = http1_oneshot(h.hcm, "/probe").await;
    assert_eq!(status, 200, "initial /probe → 200 via backend_a");

    // RECORDED DIVERGENCE (ADR-0066): the reloaded route targets cluster `nope`,
    // present in NEITHER the static set (backend_a/backend_b) NOR any CDS set.
    //   - Real Envoy ACCEPTS this update and serves a 503 (`no_cluster`) at request
    //     time for the unresolved route.
    //   - envoy-rust re-validates the reloaded table against the live cluster set
    //     and WARM-REJECTS it (update_rejected), because the request path resolves
    //     a route's cluster via `cluster_mgr.get(name).expect(...)` — installing an
    //     unknown-cluster route would PANIC the proxy on the next matching request.
    // So envoy-rust keeps the last-good table and ticks attempt + update_rejected.
    atomic_rename_rds(&h.rds_path, &rds_routing_to("nope"));
    wait_for_stat(h.admin, C_ATTEMPT, 2, Duration::from_secs(8)).await;

    let s = scrape_admin_stats(h.admin).await;
    // attempt + rejected ticked (NOT failure — unknown-cluster is a rejection).
    assert_rds_counters(&s, 2, 1, 0, 1, 1);

    // Last-good kept: /probe still routes to backend_a (and never panics).
    let (status, _) = http1_oneshot(h.hcm, "/probe").await;
    assert_eq!(
        status, 200,
        "post-warm-reject /probe → 200 (last-good kept)"
    );
    let s2 = scrape_admin_stats(h.admin).await;
    assert_stat(&s2, "cluster.backend_a.upstream_rq_total", 2);
}

// ── 5: in-flight request completes under the OLD table (§5.4 / P7) ─────────────

#[tokio::test(flavor = "multi_thread")]
async fn in_flight_request_completes_under_old_table() {
    // A SLOW backend (2s response delay) reachable via cluster `backend_slow`; the
    // initial rds routes /slow → backend_slow. A generous delay keeps the request
    // reliably mid-flight across the reload (the §5.4 read-once is already
    // unit-tested as `route_table_handle_swap_is_read_once`; this corroborates it
    // end-to-end).
    let slow_port = spawn_slow_backend("from-slow", Duration::from_secs(2)).await;
    let h = boot_harness(
        &rds_named_route("backend_slow", "local_route", "/slow"),
        Some(("backend_slow", slow_port)),
    )
    .await;

    // Confirm /slow serves (and warm the route) before exercising the swap. This
    // also seeds update_attempt/success at 1/1 for the convergence wait below.
    // (It costs one 2s round-trip but makes the test deterministic.)
    let (warm_status, warm_body) = http1_oneshot(h.hcm, "/slow").await;
    assert_eq!(warm_status, 200, "warm /slow → 200 via backend_slow");
    assert_eq!(warm_body, b"from-slow");

    // Start an in-flight /slow request WITHOUT awaiting it.
    let hcm = h.hcm;
    let inflight = tokio::spawn(async move { http1_oneshot(hcm, "/slow").await });

    // Give it a moment to connect + send headers + reach the slow backend, so it
    // has surely SNAPSHOTTED the (old) route table at entry before the reload.
    tokio::time::sleep(Duration::from_millis(400)).await;

    // Atomic-rename → a new table that routes /probe → backend_a and DROPS /slow.
    // A request entering AFTER this swap would 404 /slow; the in-flight one,
    // having read the route handle once at entry, must complete under the OLD
    // table.
    atomic_rename_rds(&h.rds_path, &rds_routing_to("backend_a"));
    // Convergence: the reload succeeded (table is valid) → update_success == 2.
    wait_for_stat(h.admin, C_SUCCESS, 2, Duration::from_secs(8)).await;

    // The in-flight request completes 200 under the OLD table — no panic, no
    // disruption. End-to-end confirmation of the Task-2 read-once snapshot.
    let (status, body) = tokio::time::timeout(Duration::from_secs(10), inflight)
        .await
        .expect("in-flight request did not finish within 10s")
        .expect("in-flight task panicked");
    assert_eq!(
        status, 200,
        "in-flight /slow completes 200 under the OLD route table across a reload"
    );
    assert_eq!(body, b"from-slow");

    // Sanity: the NEW table is live — /probe now routes (200), and /slow is gone.
    let (probe_status, _) = http1_oneshot(h.hcm, "/probe").await;
    assert_eq!(
        probe_status, 200,
        "post-reload /probe → 200 (new table live)"
    );
}
