//! 27 Task 7 (D8 / ADR-0067/0068): in-process EDS endpoint-reload BACKSTOP — the
//! deterministic LOCAL complement to the `0035-xds-eds-hot-reload` differential
//! fixture.
//!
//! The differential proves EDS endpoint hot-reload bilaterally, but ONLY on
//! native-Linux CI (under Docker Desktop virtiofs the upstream Envoy can't
//! observe a bind-mount reload) and via probe responses only. THIS backstop boots
//! the real `envoy-bin` binary as a NATIVE subprocess (NOT a container — the §6.2
//! watcher is POLL-based mtime at ~1s cadence, so a native subprocess DOES
//! observe a host-side atomic-rename), performs reloads by atomic-renaming the
//! EDS file, and scrapes admin to assert the things the differential can't cleanly
//! drive:
//!
//!   1. happy reload — flips the live endpoint, ticks the §6.2-LOCKED counter
//!      taxonomy `cluster.eds_backend.update_{attempt,success,failure,rejected,
//!      empty}` `1/1/0/0/0` → `2/2/0/0/0`, and the new endpoint is reflected in
//!      `/config_dump?include_eds` (the EndpointsConfigDump).
//!   2. V4(a) IO/malformed reload — WARM-REJECTS (keeps last-good), ticks
//!      attempt + `update_failure`.
//!   3. V4(b) no-matching-CLA reload — WARM-REJECTS, ticks attempt +
//!      `update_rejected`, last-good kept.
//!   4. V4(c) unparseable-endpoint reload — WARM-REJECTS, ticks attempt +
//!      `update_rejected`, last-good kept.
//!   5. V4(d) matched-CLA-`endpoints: []` reload — `update_success` (an empty
//!      assignment is a SUCCESS, MIRRORING Envoy), APPLY the empty set → the next
//!      `/probe` is a synth-503 `no healthy upstream` (19 bytes).
//!   6. V4(e) `resources: []` (empty envelope) reload — ticks attempt +
//!      `update_empty`, last-good kept.
//!   7. in-flight isolation (V6 / §5.4) — a request that picked an endpoint (a
//!      SLOW backend) completes against it across a concurrent reload; the next
//!      pick sees the new set.
//!   8. cursor-bounds on a SHRINKING set — start with TWO endpoints, reload down
//!      to ONE; the round-robin cursor (`i % total`) stays in-bounds, every pick
//!      lands on the surviving endpoint, no panic.
//!
//! The §6.2 EDS counters are `cluster.<name>.update_<event>` for the 5 events
//! `attempt / success / failure / rejected / empty` (vs the per-HCM
//! `http.<prefix>.rds.<route>.*` of the RDS backstop). `update_attempt` ALWAYS
//! ticks (even on a rejected reload), so "wait until `update_attempt == N`" is the
//! bounded convergence signal for a rejected reload (whose `update_success` never
//! advances).
//!
//! The helper block (`reserve_port`/`wait_ready`/`http1_oneshot`/`admin_get_body`/
//! `scrape_admin_stats`/`assert_stat`/`spawn_backend`/`spawn_slow_backend`/
//! `serve_backend_conn`/`write_file`/`write_bootstrap`/`spawn_envoy_bin` +
//! `bootstrap`/`inline_listener_block`/`eds_cluster_block`/`atomic_rename_eds`/
//! `wait_for_stat`/`eds_file*` config builders) is COPIED from the phase-26 RDS
//! hot-reload backstop (`xds_rds_hot_reload.rs`) and the phase-21 EDS-load
//! backstop (`xds_file_based_eds.rs`), which themselves copied from the LDS/CDS
//! backstops.
//!
//! ── M18-9 / M26-6 deferred shared-test-support-crate extraction pressure ──
//! This backstop is now the THIRD atomic-rename-helper user (after the phase-26
//! RDS hot-reload backstop and the differential harness's `atomic_rename_over`),
//! and at least the SEVENTH copy of the `reserve_port`/`wait_ready`/
//! `http1_oneshot`/`scrape_admin_stats`/`spawn_backend` block (CDS/LDS/RDS-load +
//! EDS-load + RDS-reload + EDS-reload). The M18-9 "extract a shared test-support
//! crate" item remains DELIBERATELY DEFERRED (the established, tracked pattern is
//! to copy); extraction stays a FUTURE HARDENING TASK, recorded here per the
//! Task-7 directive.
//!
//! Boot/harness discipline: `tokio::process::Command` + `.kill_on_drop(true)` +
//! `wait_ready` polling. The spawned echo backends are IN-PROCESS tokio listeners
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

const C_ATTEMPT: &str = "cluster.eds_backend.update_attempt";
const C_SUCCESS: &str = "cluster.eds_backend.update_success";
const C_FAILURE: &str = "cluster.eds_backend.update_failure";
const C_REJECTED: &str = "cluster.eds_backend.update_rejected";
const C_EMPTY: &str = "cluster.eds_backend.update_empty";

/// Assert the full 5-name EDS counter family in one shot.
fn assert_eds_counters(
    stats: &HashMap<String, u64>,
    attempt: u64,
    success: u64,
    failure: u64,
    rejected: u64,
    empty: u64,
) {
    assert_stat(stats, C_ATTEMPT, attempt);
    assert_stat(stats, C_SUCCESS, success);
    assert_stat(stats, C_FAILURE, failure);
    assert_stat(stats, C_REJECTED, rejected);
    assert_stat(stats, C_EMPTY, empty);
}

// ── in-process backend ──────────────────────────────────────────────────────

/// Spawn an in-process H1 backend that replies to every request with a 200 whose
/// body is the fixed `body` string. Returns the bound port.
async fn spawn_backend(body: &'static str) -> u16 {
    spawn_slow_backend(body, Duration::ZERO).await
}

/// Like `spawn_backend` but sleeps `delay` before responding to each request (the
/// in-flight-isolation test routes to a SLOW backend so a request is reliably
/// mid-flight when the concurrent reload lands).
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

// ── eds file builders ─────────────────────────────────────────────────────────

/// An EDS file body: a bare `resources:` envelope carrying one `@type`-tagged
/// `ClusterLoadAssignment` named `cla_name`, with one numeric-IP (127.0.0.1)
/// endpoint per `backend_port`. NO Envoy-only fields (the shared-template shape).
fn eds_file(cla_name: &str, backend_ports: &[u16]) -> String {
    let mut s = format!(
        "resources:\n  - \"@type\": type.googleapis.com/envoy.config.endpoint.v3.ClusterLoadAssignment\n    cluster_name: {cla_name}\n    endpoints:\n      - lb_endpoints:\n"
    );
    for port in backend_ports {
        s.push_str(&format!(
            "          - endpoint:\n              address:\n                socket_address: {{ address: 127.0.0.1, port_value: {port} }}\n"
        ));
    }
    s
}

/// An EDS file whose matched CLA carries `endpoints: []` (V4(d) apply-empty).
fn eds_file_empty_endpoints(cla_name: &str) -> String {
    format!(
        "resources:\n  - \"@type\": type.googleapis.com/envoy.config.endpoint.v3.ClusterLoadAssignment\n    cluster_name: {cla_name}\n    endpoints: []\n"
    )
}

/// An EDS file whose envelope carries ZERO CLAs (V4(e) empty envelope).
fn eds_file_empty_envelope() -> String {
    "resources: []\n".to_string()
}

/// An EDS file whose matched CLA carries an endpoint with a NON-numeric address
/// (V4(c) unparseable endpoint → update_rejected).
fn eds_file_bad_endpoint(cla_name: &str) -> String {
    format!(
        "resources:\n  - \"@type\": type.googleapis.com/envoy.config.endpoint.v3.ClusterLoadAssignment\n    cluster_name: {cla_name}\n    endpoints:\n      - lb_endpoints:\n          - endpoint:\n              address:\n                socket_address: {{ address: not-a-numeric-ip, port_value: 8080 }}\n"
    )
}

/// A `type: EDS` cluster `eds_backend` whose endpoints arrive from the EDS file at
/// `eds_path` (NO inline `load_assignment`, NO HC / OD — a PLAIN EDS cluster).
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

/// A STATIC listener `http1_listener` binding `127.0.0.1:<listener_port>` whose
/// HCM INLINE-routes `/probe` → `eds_backend`.
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
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: backend_vh
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/probe" }}
                          route: {{ cluster: eds_backend }}
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#
    )
}

/// Assemble a bootstrap: admin + `static_resources` whose `listeners:` /
/// `clusters:` blocks are supplied by the caller. NO `dynamic_resources` — the
/// EDS cluster is STATIC-but-EDS.
fn bootstrap(admin_port: u16, listeners_block: &str, clusters_block: &str) -> String {
    format!(
        r#"node: {{ id: envoy-rust-phase-27-backstop, cluster: envoy-rust-phase-27 }}
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

/// ATOMICALLY replace the EDS file's contents. Writes a SAME-DIR sibling temp
/// then `std::fs::rename`s it over `target`. The §6.2 watcher detects the change
/// by the file's mtime stepping forward; an atomic rename guarantees the watcher
/// only ever stats a COMPLETE file.
fn atomic_rename_eds(target: &Path, new_contents: &str) {
    let tmp = target.with_extension("reload-tmp");
    std::fs::File::create(&tmp)
        .unwrap()
        .write_all(new_contents.as_bytes())
        .unwrap();
    std::fs::rename(&tmp, target).expect("atomic rename eds over target");
}

/// Poll `/stats` at ~150ms until `stats[name] == expected` or `budget` elapses.
/// The watcher poll cadence is ~1s, so a `budget` of ~8s gives several poll
/// windows of slack. Panics (with the last observed value) on timeout.
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

/// Shared setup: a bootstrap with a STATIC H1 listener INLINE-routing /probe →
/// `eds_backend`, plus a PLAIN `type: EDS` cluster whose endpoints arrive from
/// the watched `eds.yaml` (`initial_eds`). Returns the live HCM/admin addrs, the
/// eds file path, the spawned child, and the tempdir (both kept alive).
struct Harness {
    hcm: SocketAddr,
    admin: SocketAddr,
    eds_path: std::path::PathBuf,
    _child: tokio::process::Child,
    _dir: tempfile::TempDir,
}

async fn boot_harness(initial_eds: &str) -> Harness {
    let listener_port = reserve_port();
    let admin_port = reserve_port();
    let hcm: SocketAddr = format!("127.0.0.1:{listener_port}").parse().unwrap();
    let admin: SocketAddr = format!("127.0.0.1:{admin_port}").parse().unwrap();

    let dir = tempfile::tempdir().unwrap();
    // The eds file lives in the host's real fs tempdir so the watcher polls its
    // mtime. The `.yaml` extension matters (PathConfigSource infers the format).
    let eds_path_str = write_file(dir.path(), "eds.yaml", initial_eds);
    let eds_path = std::path::PathBuf::from(&eds_path_str);

    let listeners = inline_listener_block(listener_port);
    let clusters = eds_cluster_block(&eds_path_str);
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
        eds_path,
        _child: child,
        _dir: dir,
    }
}

// ── 1: happy reload flips endpoint + ticks counters + reflects in config_dump ──

#[tokio::test(flavor = "multi_thread")]
async fn happy_reload_flips_endpoint_and_ticks_counters() {
    let backend_1 = spawn_backend("from-backend-1").await;
    let backend_2 = spawn_backend("from-backend-2").await;
    let h = boot_harness(&eds_file("eds_backend", &[backend_1])).await;

    // Initial endpoint set routes /probe → backend_1.
    let (status, body) = http1_oneshot(h.hcm, "/probe").await;
    assert_eq!(status, 200, "initial /probe → 200 via backend_1");
    assert_eq!(body, b"from-backend-1");

    let s0 = scrape_admin_stats(h.admin).await;
    // Initial-load counter values per the §6.2-LOCKED taxonomy.
    assert_eds_counters(&s0, 1, 1, 0, 0, 0);

    // Atomic-rename the eds file → endpoint backend_2. The watcher (poll cadence
    // ~1s) observes the mtime step and runs the §6.2 reload pipeline.
    atomic_rename_eds(&h.eds_path, &eds_file("eds_backend", &[backend_2]));
    wait_for_stat(h.admin, C_SUCCESS, 2, Duration::from_secs(8)).await;

    // The live endpoint set now routes /probe → backend_2.
    let (status, body) = http1_oneshot(h.hcm, "/probe").await;
    assert_eq!(status, 200, "post-reload /probe → 200 via backend_2");
    assert_eq!(body, b"from-backend-2");

    let s1 = scrape_admin_stats(h.admin).await;
    // One successful reload ticks attempt + success by 1; the rest stay 0.
    assert_eds_counters(&s1, 2, 2, 0, 0, 0);

    // config_dump?include_eds reflects the live reloaded endpoint — the
    // EndpointsConfigDump's first static_endpoint_configs entry carries the new
    // endpoint address (backend_2's port). Walk to the EndpointsConfigDump entry
    // (scoped — `last_updated` legitimately appears on OTHER dump sections like a
    // dynamic ClustersConfigDump; the §6.2-LOCKED fact is that the EDS section's
    // entries carry NEITHER last_updated NOR version_info).
    let dump = admin_get_body(h.admin, "/config_dump?include_eds").await;
    let dump_text = std::str::from_utf8(&dump).expect("config_dump utf8");
    assert!(
        dump_text.contains("EndpointsConfigDump"),
        "config_dump?include_eds must contain EndpointsConfigDump"
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
        .expect("config_dump must carry an EndpointsConfigDump entry");
    // Scoped: the EDS section itself carries neither last_updated nor version_info.
    let eds_text = serde_json::to_string(endpoints).unwrap();
    assert!(
        !eds_text.contains("last_updated"),
        "EndpointsConfigDump must NOT carry last_updated; was:\n{eds_text}"
    );
    assert!(
        !eds_text.contains("version_info"),
        "EndpointsConfigDump must NOT carry version_info; was:\n{eds_text}"
    );
    // The first static_endpoint_configs entry is the eds_backend CLA, and its
    // reloaded endpoint address carries backend_2's port (the swapped endpoint).
    assert_eq!(
        endpoints
            .pointer("/static_endpoint_configs/0/endpoint_config/cluster_name")
            .and_then(|v| v.as_str()),
        Some("eds_backend"),
        "EndpointsConfigDump first entry must be the eds_backend CLA"
    );
    assert!(
        eds_text.contains(&backend_2.to_string()),
        "EndpointsConfigDump must reflect the reloaded endpoint port {backend_2}; was:\n{eds_text}"
    );
}

// ── 2: V4(a) malformed reload warm-rejects (update_failure), keeps last-good ───

#[tokio::test(flavor = "multi_thread")]
async fn v4a_malformed_reload_warm_rejects_and_keeps_last_good() {
    let backend_1 = spawn_backend("from-backend-1").await;
    let h = boot_harness(&eds_file("eds_backend", &[backend_1])).await;

    let (status, body) = http1_oneshot(h.hcm, "/probe").await;
    assert_eq!(status, 200, "initial /probe → 200 via backend_1");
    assert_eq!(body, b"from-backend-1");

    // Atomic-rename → a syntactically-broken EDS file. The reparse fails
    // (IO/parse class) → WARM-REJECT: keep last-good, tick attempt +
    // update_failure. update_attempt ALWAYS ticks, so == 2 is the convergence
    // signal for this rejected reload.
    atomic_rename_eds(&h.eds_path, "resources: [unclosed");
    wait_for_stat(h.admin, C_ATTEMPT, 2, Duration::from_secs(8)).await;

    let s = scrape_admin_stats(h.admin).await;
    assert_eds_counters(&s, 2, 1, 1, 0, 0);

    // Last-good kept: /probe still routes to backend_1.
    let (status, body) = http1_oneshot(h.hcm, "/probe").await;
    assert_eq!(
        status, 200,
        "post-warm-reject /probe → 200 (last-good kept)"
    );
    assert_eq!(body, b"from-backend-1");
}

// ── 3: V4(b) no-matching-CLA reload warm-rejects (update_rejected) ─────────────

#[tokio::test(flavor = "multi_thread")]
async fn v4b_no_matching_cla_warm_rejects_and_keeps_last_good() {
    let backend_1 = spawn_backend("from-backend-1").await;
    let backend_2 = spawn_backend("from-backend-2").await;
    let h = boot_harness(&eds_file("eds_backend", &[backend_1])).await;

    let (status, _) = http1_oneshot(h.hcm, "/probe").await;
    assert_eq!(status, 200, "initial /probe → 200 via backend_1");

    // Atomic-rename → a (non-empty) envelope whose only CLA is named `other_cla`
    // (the cluster's selection name is `eds_backend`). No match → WARM-REJECT:
    // keep last-good, tick attempt + update_rejected.
    atomic_rename_eds(&h.eds_path, &eds_file("other_cla", &[backend_2]));
    wait_for_stat(h.admin, C_ATTEMPT, 2, Duration::from_secs(8)).await;

    let s = scrape_admin_stats(h.admin).await;
    assert_eds_counters(&s, 2, 1, 0, 1, 0);

    // Last-good kept: /probe still routes to backend_1.
    let (status, body) = http1_oneshot(h.hcm, "/probe").await;
    assert_eq!(
        status, 200,
        "post-warm-reject /probe → 200 (last-good kept)"
    );
    assert_eq!(body, b"from-backend-1");
}

// ── 4: V4(c) unparseable-endpoint reload warm-rejects (update_rejected) ────────

#[tokio::test(flavor = "multi_thread")]
async fn v4c_unparseable_endpoint_warm_rejects_and_keeps_last_good() {
    let backend_1 = spawn_backend("from-backend-1").await;
    let h = boot_harness(&eds_file("eds_backend", &[backend_1])).await;

    let (status, _) = http1_oneshot(h.hcm, "/probe").await;
    assert_eq!(status, 200, "initial /probe → 200 via backend_1");

    // Atomic-rename → the matched CLA carries a non-numeric endpoint address.
    // EDS rejects hostnames (numeric-IP-only) → revalidation fails → WARM-REJECT:
    // keep last-good, tick attempt + update_rejected.
    atomic_rename_eds(&h.eds_path, &eds_file_bad_endpoint("eds_backend"));
    wait_for_stat(h.admin, C_ATTEMPT, 2, Duration::from_secs(8)).await;

    let s = scrape_admin_stats(h.admin).await;
    assert_eds_counters(&s, 2, 1, 0, 1, 0);

    // Last-good kept: /probe still routes to backend_1.
    let (status, body) = http1_oneshot(h.hcm, "/probe").await;
    assert_eq!(
        status, 200,
        "post-warm-reject /probe → 200 (last-good kept)"
    );
    assert_eq!(body, b"from-backend-1");
}

// ── 5: V4(d) apply-empty reload → update_success + 503 "no healthy upstream" ───

#[tokio::test(flavor = "multi_thread")]
async fn v4d_apply_empty_reload_succeeds_and_serves_503() {
    let backend_1 = spawn_backend("from-backend-1").await;
    let h = boot_harness(&eds_file("eds_backend", &[backend_1])).await;

    let (status, _) = http1_oneshot(h.hcm, "/probe").await;
    assert_eq!(status, 200, "initial /probe → 200 via backend_1");

    // Atomic-rename → the matched CLA carries `endpoints: []`. An empty assignment
    // is a SUCCESSFUL update (MIRRORING Envoy), NOT a reject: APPLY the empty set
    // → update_success ticks. update_success == 2 is the convergence signal.
    atomic_rename_eds(&h.eds_path, &eds_file_empty_endpoints("eds_backend"));
    wait_for_stat(h.admin, C_SUCCESS, 2, Duration::from_secs(8)).await;

    let s = scrape_admin_stats(h.admin).await;
    // attempt + success ticked; failure/rejected/empty unchanged.
    assert_eds_counters(&s, 2, 2, 0, 0, 0);

    // The empty endpoint set → pick() returns None → the synth-503
    // "no healthy upstream" (19 bytes).
    let (status, body) = http1_oneshot(h.hcm, "/probe").await;
    assert_eq!(status, 503, "apply-empty → 503 (no endpoints to pick)");
    assert_eq!(
        body, b"no healthy upstream",
        "apply-empty synth-503 body is the 19-byte 'no healthy upstream'"
    );
}

// ── 6: V4(e) empty-envelope reload → update_empty, keeps last-good ─────────────

#[tokio::test(flavor = "multi_thread")]
async fn v4e_empty_envelope_reload_ticks_update_empty_and_keeps_last_good() {
    let backend_1 = spawn_backend("from-backend-1").await;
    let h = boot_harness(&eds_file("eds_backend", &[backend_1])).await;

    let (status, _) = http1_oneshot(h.hcm, "/probe").await;
    assert_eq!(status, 200, "initial /probe → 200 via backend_1");

    // Atomic-rename → `resources: []` (zero CLAs). Distinct from V4(d): KEEP
    // last-good, tick attempt + update_empty.
    atomic_rename_eds(&h.eds_path, &eds_file_empty_envelope());
    wait_for_stat(h.admin, C_ATTEMPT, 2, Duration::from_secs(8)).await;

    let s = scrape_admin_stats(h.admin).await;
    assert_eds_counters(&s, 2, 1, 0, 0, 1);

    // Last-good kept: /probe still routes to backend_1 (200, not 503).
    let (status, body) = http1_oneshot(h.hcm, "/probe").await;
    assert_eq!(
        status, 200,
        "post-empty-envelope /probe → 200 (last-good kept)"
    );
    assert_eq!(body, b"from-backend-1");
}

// ── 7: in-flight request completes under the OLD endpoint set (V6 / §5.4) ───────

#[tokio::test(flavor = "multi_thread")]
async fn in_flight_request_completes_under_old_endpoint_set() {
    // A SLOW backend_1 (2s response delay); the initial set routes /probe → it. A
    // generous delay keeps the request reliably mid-flight across the reload.
    let slow_1 = spawn_slow_backend("from-slow-1", Duration::from_secs(2)).await;
    let backend_2 = spawn_backend("from-backend-2").await;
    let h = boot_harness(&eds_file("eds_backend", &[slow_1])).await;

    // Warm the route (and seed update_attempt/success at 1/1).
    let (warm_status, warm_body) = http1_oneshot(h.hcm, "/probe").await;
    assert_eq!(warm_status, 200, "warm /probe → 200 via slow backend_1");
    assert_eq!(warm_body, b"from-slow-1");

    // Start an in-flight /probe request WITHOUT awaiting it.
    let hcm = h.hcm;
    let inflight = tokio::spawn(async move { http1_oneshot(hcm, "/probe").await });

    // Give it a moment to connect + send headers + reach the slow backend, so it
    // has surely PICKED the (old) endpoint before the reload.
    tokio::time::sleep(Duration::from_millis(400)).await;

    // Atomic-rename → endpoint backend_2. A request entering AFTER this swap would
    // pick backend_2; the in-flight one, having read the endpoint snapshot once at
    // pick, must complete against backend_1 (the OLD set).
    atomic_rename_eds(&h.eds_path, &eds_file("eds_backend", &[backend_2]));
    wait_for_stat(h.admin, C_SUCCESS, 2, Duration::from_secs(8)).await;

    // The in-flight request completes 200 against backend_1 — no panic.
    let (status, body) = tokio::time::timeout(Duration::from_secs(10), inflight)
        .await
        .expect("in-flight request did not finish within 10s")
        .expect("in-flight task panicked");
    assert_eq!(
        status, 200,
        "in-flight /probe completes 200 under the OLD endpoint set across a reload"
    );
    assert_eq!(body, b"from-slow-1");

    // Sanity: the NEW set is live — the next pick lands on backend_2.
    let (status, body) = http1_oneshot(h.hcm, "/probe").await;
    assert_eq!(status, 200, "post-reload /probe → 200 (new set live)");
    assert_eq!(body, b"from-backend-2");
}

// ── 8: cursor-bounds on a SHRINKING (2 → 1) endpoint set stays in-bounds ───────

#[tokio::test(flavor = "multi_thread")]
async fn cursor_bounds_on_shrinking_endpoint_set() {
    // TWO endpoints initially (round-robin alternates). Both serve the SAME body
    // so the data-plane result is body-agnostic — what matters is no panic and a
    // 200 on every pick after the set shrinks to ONE (`i % total` re-bounds the
    // cursor that may have advanced past index 0).
    let backend_a = spawn_backend("from-backend").await;
    let backend_b = spawn_backend("from-backend").await;
    let h = boot_harness(&eds_file("eds_backend", &[backend_a, backend_b])).await;

    // Advance the round-robin cursor over the 2-endpoint set (several picks so the
    // cursor lands at an index that WOULD be out-of-bounds for a 1-endpoint set).
    for _ in 0..5 {
        let (status, _) = http1_oneshot(h.hcm, "/probe").await;
        assert_eq!(
            status, 200,
            "pre-shrink /probe → 200 over the 2-endpoint set"
        );
    }

    // Atomic-rename → ONE endpoint (drop backend_b). The cursor (an AtomicUsize
    // that has advanced past 1) must re-bound via `i % total` against the new
    // total of 1 — every subsequent pick lands on the surviving endpoint.
    atomic_rename_eds(&h.eds_path, &eds_file("eds_backend", &[backend_a]));
    wait_for_stat(h.admin, C_SUCCESS, 2, Duration::from_secs(8)).await;

    // Several more picks: all 200, none panic, none index out of bounds.
    for _ in 0..5 {
        let (status, _) = http1_oneshot(h.hcm, "/probe").await;
        assert_eq!(
            status, 200,
            "post-shrink /probe → 200 (cursor re-bounded to the 1-endpoint set)"
        );
    }

    let s = scrape_admin_stats(h.admin).await;
    assert_eds_counters(&s, 2, 2, 0, 0, 0);
}
