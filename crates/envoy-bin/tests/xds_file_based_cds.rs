//! Phase 18 Task 8 (ADR-0049): in-process backstop for file-based CDS. Boots the
//! real `envoy-bin` binary as a subprocess and exercises the paths the
//! differential fixture (0026) CANNOT — the negative paths and the static/dynamic
//! collision.
//!
//! Per ADR-0049 these tests ARE the recorded-divergence proof: envoy-rust treats
//! a missing OR malformed CDS file as a FATAL startup error (L4), deliberately
//! diverging from Envoy's warn-and-serve. A deliberately-broken Envoy-side fixture
//! is not a thing this project does, so the divergence is recorded HERE.
//!
//! Five paths (each boots its own envoy-bin instance):
//!
//!   (i)   happy path — bootstrap with `dynamic_resources.cds_config` → a CDS file
//!         defining a STRICT_DNS `dynamic_backend` → an in-process backend. Zero
//!         static clusters; an HCM listener routes `/` to `dynamic_backend`
//!         (`validate_clusters: false`). Boot succeeds → data-plane GET / → 200 +
//!         backend body; `/stats` shows the six conditional `cluster_manager.*`
//!         names (L3); `/config_dump` `configs[1]` is the ClustersConfigDump whose
//!         first `dynamic_active_clusters` entry is `dynamic_backend`.
//!
//!   (ii)  missing CDS file (L4a) — bootstrap points at a nonexistent path → the
//!         process EXITS non-zero; the process diagnostic carries the CdsFileError text
//!         ("reading CDS file"); the listener port NEVER accepts connections.
//!
//!   (iii) malformed CDS file (L4b — the recorded-divergence proof) — the CDS file
//!         is `resources: [unclosed` → same fatal-exit triple; the process diagnostic carries the
//!         CdsParseError text ("parsing CDS file").
//!
//!   (iv)  static/dynamic collision (L9 — static wins) — bootstrap defines
//!         `dynamic_backend` STATICALLY (→ backend A) AND the CDS file defines
//!         `dynamic_backend` (→ backend B, a different port). Boot SUCCEEDS; the
//!         data-plane GET returns backend A's response (static wins, proven on the
//!         DATA PLANE); `/config_dump`'s ClustersConfigDump shows `static_clusters`
//!         and OMITS `dynamic_active_clusters` (empty → key omitted per L5).
//!
//!   (v)   inertness (§5.2) — a bootstrap WITHOUT `dynamic_resources` (static
//!         cluster + listener) → `/stats` carries NO `cluster_manager.` names;
//!         `/config_dump` has exactly ONE entry (the fixture-0014 regression shape).
//!
//! Boot/harness discipline copied verbatim from the established backstops
//! (`upstream_circuit_breaker_budgets.rs`, `upstream_active_health_check.rs`):
//! `tokio::process::Command` + `.kill_on_drop(true)` + `stdout: Stdio::null()` +
//! `stderr: Stdio::piped()` + `wait_ready` polling. Readiness budgets follow the
//! `upstream_active_health_check.rs` precedent: 30s backend, 10s envoy-bin.

#![forbid(unsafe_code)]

use std::io::Write;
use std::net::SocketAddr;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;

mod common;

use common::{
    admin_get_body, assert_stat, http1_oneshot, reserve_port, scrape_admin_stats, spawn_backend,
    spawn_envoy_bin, wait_ready, write_file,
};

// ── bootstrap builders ────────────────────────────────────────────────────────

/// A STRICT_DNS cluster `dynamic_backend` pointing at `backend_port`, rendered as
/// the CDS-file `resources:` envelope OR (when `cds=false`) as a bare cluster body
/// for inclusion under `static_resources.clusters`.
fn dynamic_backend_cluster_body(backend_port: u16) -> String {
    format!(
        r#"    name: dynamic_backend
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

/// The CDS file body (the `resources:` envelope with one `@type`-tagged Cluster).
fn cds_file(backend_port: u16) -> String {
    format!(
        "resources:\n  - \"@type\": type.googleapis.com/envoy.config.cluster.v3.Cluster\n{}",
        dynamic_backend_cluster_body(backend_port)
    )
}

/// The `dynamic_backend` cluster rendered as one `static_resources.clusters` list
/// item: a `- ` lead on the first key and a consistent 6-space indent on the
/// remaining mapping keys (authored directly at the correct list-item depth).
fn static_cluster_block(backend_port: u16) -> String {
    format!(
        r#"    - name: dynamic_backend
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

/// A bootstrap with an HCM listener routing `/` to `dynamic_backend`. The
/// `clusters` block is supplied by the caller (empty for the happy/dynamic case,
/// or a static `dynamic_backend` for the collision case). When `cds_path` is
/// `Some`, a `dynamic_resources.cds_config` block is emitted.
fn bootstrap_with_route(
    hcm_port: u16,
    admin_port: u16,
    clusters_block: &str,
    cds_path: Option<&str>,
) -> String {
    let dynamic_resources = match cds_path {
        Some(p) => format!(
            "dynamic_resources:\n  cds_config:\n    resource_api_version: V3\n    path_config_source:\n      path: {p}\n"
        ),
        None => String::new(),
    };
    format!(
        r#"admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: {admin_port}
{dynamic_resources}static_resources:
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
                  validate_clusters: false
                  virtual_hosts:
                    - name: local
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          route: {{ cluster: dynamic_backend }}
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
{clusters_block}"#
    )
}

// ── (i) happy path ──────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn happy_path_dynamic_cluster_serves_and_reports() {
    let hcm_port = reserve_port();
    let admin_port = reserve_port();
    let hcm_addr: SocketAddr = format!("127.0.0.1:{hcm_port}").parse().unwrap();
    let admin_addr: SocketAddr = format!("127.0.0.1:{admin_port}").parse().unwrap();
    let backend_addr_port = spawn_backend("from-dynamic-backend").await;

    let dir = tempfile::tempdir().unwrap();
    let cds_path = write_file(dir.path(), "cds.yaml", &cds_file(backend_addr_port));
    let bootstrap = bootstrap_with_route(hcm_port, admin_port, "", Some(&cds_path));
    let cfg = dir.path().join("envoy-rust.yaml");
    std::fs::File::create(&cfg)
        .unwrap()
        .write_all(bootstrap.as_bytes())
        .unwrap();

    let _envoy = spawn_envoy_bin(&cfg);
    wait_ready(hcm_addr, Duration::from_secs(10))
        .await
        .expect("envoy-bin HCM ready");
    wait_ready(admin_addr, Duration::from_secs(10))
        .await
        .expect("envoy-bin admin ready");

    // Data plane: GET / routed through the CDS-supplied cluster → 200 + body.
    let (status, _, body) = http1_oneshot(hcm_addr, "/", "dynamic_backend").await;
    assert_eq!(
        status, 200,
        "(i) happy path: expected 200 via dynamic cluster"
    );
    assert_eq!(
        body, b"from-dynamic-backend",
        "(i) happy path: backend body served through dynamic_backend"
    );

    // /stats: the six conditional cluster_manager.* names (L3).
    let s = scrape_admin_stats(admin_addr).await;
    assert_stat(&s, "cluster_manager.cds.update_attempt", 1);
    assert_stat(&s, "cluster_manager.cds.update_success", 1);
    assert_stat(&s, "cluster_manager.cds.update_failure", 0);
    assert_stat(&s, "cluster_manager.cds.update_rejected", 0);
    assert_stat(&s, "cluster_manager.cluster_added", 1);
    assert_stat(&s, "cluster_manager.active_clusters", 1);

    // /config_dump: configs[1] is the ClustersConfigDump; its first
    // dynamic_active_clusters entry is `dynamic_backend`.
    let dump = admin_get_body(admin_addr, "/config_dump").await;
    let json: serde_json::Value = serde_json::from_slice(&dump).expect("config_dump json");
    assert_eq!(
        json.pointer("/configs/1/@type").and_then(|v| v.as_str()),
        Some("type.googleapis.com/envoy.admin.v3.ClustersConfigDump"),
        "(i) config_dump configs[1] must be the ClustersConfigDump"
    );
    assert_eq!(
        json.pointer("/configs/1/dynamic_active_clusters/0/cluster/name")
            .and_then(|v| v.as_str()),
        Some("dynamic_backend"),
        "(i) config_dump dynamic_active_clusters[0].cluster.name must be dynamic_backend"
    );
}

// ── (ii) missing CDS file (L4a) ───────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn missing_cds_file_is_fatal() {
    let hcm_port = reserve_port();
    let admin_port = reserve_port();
    let hcm_addr: SocketAddr = format!("127.0.0.1:{hcm_port}").parse().unwrap();

    let dir = tempfile::tempdir().unwrap();
    // Point at a path that does NOT exist (never written).
    let missing = dir.path().join("does-not-exist.yaml");
    let bootstrap = bootstrap_with_route(hcm_port, admin_port, "", Some(missing.to_str().unwrap()));
    let cfg = dir.path().join("envoy-rust.yaml");
    std::fs::File::create(&cfg)
        .unwrap()
        .write_all(bootstrap.as_bytes())
        .unwrap();

    assert_fatal_startup(&cfg, hcm_addr, "reading CDS file", "(ii) missing CDS file").await;
}

// ── (iii) malformed CDS file (L4b — recorded-divergence proof) ────────────────

#[tokio::test(flavor = "multi_thread")]
async fn malformed_cds_file_is_fatal() {
    let hcm_port = reserve_port();
    let admin_port = reserve_port();
    let hcm_addr: SocketAddr = format!("127.0.0.1:{hcm_port}").parse().unwrap();

    let dir = tempfile::tempdir().unwrap();
    // A syntactically-broken CDS file (unclosed flow sequence).
    let cds_path = write_file(dir.path(), "cds.yaml", "resources: [unclosed");
    let bootstrap = bootstrap_with_route(hcm_port, admin_port, "", Some(&cds_path));
    let cfg = dir.path().join("envoy-rust.yaml");
    std::fs::File::create(&cfg)
        .unwrap()
        .write_all(bootstrap.as_bytes())
        .unwrap();

    assert_fatal_startup(
        &cfg,
        hcm_addr,
        "parsing CDS file",
        "(iii) malformed CDS file",
    )
    .await;
}

/// Shared negative-path assertion: boot envoy-bin against `cfg`, expect a non-zero
/// exit within a budget, expect stderr to contain `needle`, and expect the
/// listener `hcm_addr` to NEVER accept a connection.
async fn assert_fatal_startup(
    cfg: &std::path::Path,
    hcm_addr: SocketAddr,
    needle: &str,
    ctx: &str,
) {
    // Pipe BOTH stdout and stderr: the fatal `run()` error is surfaced via the
    // process's `tracing` ERROR diagnostic. The `tracing_subscriber::fmt`
    // subscriber writes to STDOUT (not stderr), so the CdsFileError /
    // CdsParseError message text lands on stdout; we scan the combined streams
    // for the needle so the assertion does not depend on which fd the diagnostic
    // happens to use (and stays robust if the writer is later retargeted).
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_envoy-bin"))
        .arg("-c")
        .arg(cfg)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn envoy-bin");

    // Drain both pipes to EOF concurrently with the wait: each read-to-end
    // completes when the process closes that fd at exit. Reading BEFORE the
    // process is reaped guarantees the diagnostic is captured even if a pipe
    // buffer were near full.
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
        "{ctx}: envoy-bin must exit NON-ZERO on a CDS load error, got {exit:?}"
    );

    // The listener never accepted: a connect attempt fails now (process is gone).
    let connect = TcpStream::connect(hcm_addr).await;
    assert!(
        connect.is_err(),
        "{ctx}: listener port {hcm_addr} must NEVER accept a connection on fatal startup"
    );

    // The diagnostic carries the specific CDS error text.
    let combined = format!("{out}{err}");
    assert!(
        combined.contains(needle),
        "{ctx}: process diagnostic must contain {needle:?}\nstdout+stderr was:\n{combined}"
    );
}

// ── (iv) static/dynamic collision (L9 — static wins) ──────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn static_dynamic_collision_static_wins() {
    let hcm_port = reserve_port();
    let admin_port = reserve_port();
    let hcm_addr: SocketAddr = format!("127.0.0.1:{hcm_port}").parse().unwrap();
    let admin_addr: SocketAddr = format!("127.0.0.1:{admin_port}").parse().unwrap();

    // Two distinct backends: A served by the STATIC dynamic_backend, B by the CDS
    // duplicate. Static must win → the data plane returns A's body.
    let backend_a = spawn_backend("from-STATIC-backend-A").await;
    let backend_b = spawn_backend("from-DYNAMIC-backend-B").await;

    let dir = tempfile::tempdir().unwrap();
    // CDS file defines `dynamic_backend` → backend B.
    let cds_path = write_file(dir.path(), "cds.yaml", &cds_file(backend_b));
    // Bootstrap ALSO defines `dynamic_backend` statically → backend A.
    let static_block = static_cluster_block(backend_a);
    let bootstrap = bootstrap_with_route(hcm_port, admin_port, &static_block, Some(&cds_path));
    let cfg = dir.path().join("envoy-rust.yaml");
    std::fs::File::create(&cfg)
        .unwrap()
        .write_all(bootstrap.as_bytes())
        .unwrap();

    let _envoy = spawn_envoy_bin(&cfg);
    wait_ready(hcm_addr, Duration::from_secs(10))
        .await
        .expect("envoy-bin HCM ready");
    wait_ready(admin_addr, Duration::from_secs(10))
        .await
        .expect("envoy-bin admin ready");

    // DATA PLANE proof: static wins → backend A's body.
    let (status, _, body) = http1_oneshot(hcm_addr, "/", "dynamic_backend").await;
    assert_eq!(
        status, 200,
        "(iv) collision: expected 200 (static cluster serves)"
    );
    assert_eq!(
        body, b"from-STATIC-backend-A",
        "(iv) collision: static cluster must WIN (backend A served, not B)"
    );

    // config_dump: ClustersConfigDump has static_clusters containing dynamic_backend
    // and OMITS dynamic_active_clusters (the dynamic duplicate was skipped; empty
    // list ⇒ key omitted per L5).
    let dump = admin_get_body(admin_addr, "/config_dump").await;
    let json: serde_json::Value = serde_json::from_slice(&dump).expect("config_dump json");
    assert_eq!(
        json.pointer("/configs/1/@type").and_then(|v| v.as_str()),
        Some("type.googleapis.com/envoy.admin.v3.ClustersConfigDump"),
        "(iv) config_dump configs[1] must be the ClustersConfigDump"
    );
    assert_eq!(
        json.pointer("/configs/1/static_clusters/0/cluster/name")
            .and_then(|v| v.as_str()),
        Some("dynamic_backend"),
        "(iv) config_dump static_clusters[0] must be dynamic_backend"
    );
    assert!(
        json.pointer("/configs/1/dynamic_active_clusters").is_none(),
        "(iv) config_dump dynamic_active_clusters must be OMITTED (dynamic duplicate skipped)"
    );
}

// ── (v) inertness (§5.2) ──────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn no_dynamic_resources_is_inert() {
    let hcm_port = reserve_port();
    let admin_port = reserve_port();
    let hcm_addr: SocketAddr = format!("127.0.0.1:{hcm_port}").parse().unwrap();
    let admin_addr: SocketAddr = format!("127.0.0.1:{admin_port}").parse().unwrap();
    let backend_port = spawn_backend("from-static").await;

    let dir = tempfile::tempdir().unwrap();
    // Static cluster + listener, NO dynamic_resources block.
    let static_block = static_cluster_block(backend_port);
    let bootstrap = bootstrap_with_route(hcm_port, admin_port, &static_block, None);
    let cfg = dir.path().join("envoy-rust.yaml");
    std::fs::File::create(&cfg)
        .unwrap()
        .write_all(bootstrap.as_bytes())
        .unwrap();

    let _envoy = spawn_envoy_bin(&cfg);
    wait_ready(hcm_addr, Duration::from_secs(10))
        .await
        .expect("envoy-bin HCM ready");
    wait_ready(admin_addr, Duration::from_secs(10))
        .await
        .expect("envoy-bin admin ready");

    // /stats carries NO cluster_manager.* names.
    let s = scrape_admin_stats(admin_addr).await;
    let cm: Vec<&String> = s
        .keys()
        .filter(|k| k.starts_with("cluster_manager."))
        .collect();
    assert!(
        cm.is_empty(),
        "(v) inertness: NO cluster_manager.* stats expected, found {cm:?}"
    );

    // /config_dump has exactly ONE entry (the fixture-0014 regression shape).
    let dump = admin_get_body(admin_addr, "/config_dump").await;
    let json: serde_json::Value = serde_json::from_slice(&dump).expect("config_dump json");
    let configs = json
        .get("configs")
        .and_then(|c| c.as_array())
        .expect("config_dump configs array");
    assert_eq!(
        configs.len(),
        1,
        "(v) inertness: config_dump must have exactly ONE entry (no ClustersConfigDump)"
    );
}
