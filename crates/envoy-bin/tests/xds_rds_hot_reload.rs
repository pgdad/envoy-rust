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
use std::net::SocketAddr;
use std::time::Duration;

mod common;

use common::{
    admin_get_body, assert_stat, atomic_rename_over, http1_oneshot, rds_listener_block,
    reserve_port, scrape_admin_stats, spawn_backend, spawn_envoy_bin, spawn_slow_backend,
    wait_for_stat, wait_ready, write_bootstrap, write_file,
};

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

// ── hot-reload-specific helpers ───────────────────────────────────────────────

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
    let (status, _, body) = http1_oneshot(h.hcm, "/probe", "backend").await;
    assert_eq!(status, 200, "initial /probe → 200 via backend_a");
    assert_eq!(body, b"from-backend");

    let s0 = scrape_admin_stats(h.admin).await;
    assert_stat(&s0, "cluster.backend_a.upstream_rq_total", 1);
    // Initial-load counter values per the §6.2-LOCKED taxonomy.
    assert_rds_counters(&s0, 1, 1, 0, 0, 1);

    // Atomic-rename the rds file → routing /probe → backend_b. The watcher (poll
    // cadence ~1s) observes the mtime step and runs the §6.2 reload pipeline.
    atomic_rename_over(&h.rds_path, &rds_routing_to("backend_b"));
    wait_for_stat(h.admin, C_SUCCESS, 2, Duration::from_secs(8)).await;

    // The live table now routes /probe → backend_b.
    let (status, _, body) = http1_oneshot(h.hcm, "/probe", "backend").await;
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

    let (status, _, _) = http1_oneshot(h.hcm, "/probe", "backend").await;
    assert_eq!(status, 200, "initial /probe → 200 via backend_a");

    // Atomic-rename → a syntactically-broken RDS file (unclosed flow sequence).
    // The reload reparse fails (IO/parse class) → WARM-REJECT: keep last-good,
    // tick attempt + update_failure. update_attempt ALWAYS ticks, so == 2 is the
    // convergence signal for this rejected reload.
    atomic_rename_over(&h.rds_path, "resources: [unclosed");
    wait_for_stat(h.admin, C_ATTEMPT, 2, Duration::from_secs(8)).await;

    let s = scrape_admin_stats(h.admin).await;
    // attempt + failure ticked; success/rejected/config_reload unchanged.
    assert_rds_counters(&s, 2, 1, 1, 0, 1);

    // Last-good kept: /probe still routes to backend_a (NOT backend_b).
    let (status, _, _) = http1_oneshot(h.hcm, "/probe", "backend").await;
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

    let (status, _, _) = http1_oneshot(h.hcm, "/probe", "backend").await;
    assert_eq!(status, 200, "initial /probe → 200 via backend_a");

    // Atomic-rename → a RouteConfiguration named `other_route` (the HCM's rds wants
    // `local_route`). The name is absent from the reloaded file → WARM-REJECT:
    // keep last-good, tick attempt + update_rejected.
    atomic_rename_over(
        &h.rds_path,
        &rds_named_route("backend_b", "other_route", "/probe"),
    );
    wait_for_stat(h.admin, C_ATTEMPT, 2, Duration::from_secs(8)).await;

    let s = scrape_admin_stats(h.admin).await;
    // attempt + rejected ticked; success/failure/config_reload unchanged.
    assert_rds_counters(&s, 2, 1, 0, 1, 1);

    // Last-good kept: /probe still routes to backend_a.
    let (status, _, _) = http1_oneshot(h.hcm, "/probe", "backend").await;
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

    let (status, _, _) = http1_oneshot(h.hcm, "/probe", "backend").await;
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
    atomic_rename_over(&h.rds_path, &rds_routing_to("nope"));
    wait_for_stat(h.admin, C_ATTEMPT, 2, Duration::from_secs(8)).await;

    let s = scrape_admin_stats(h.admin).await;
    // attempt + rejected ticked (NOT failure — unknown-cluster is a rejection).
    assert_rds_counters(&s, 2, 1, 0, 1, 1);

    // Last-good kept: /probe still routes to backend_a (and never panics).
    let (status, _, _) = http1_oneshot(h.hcm, "/probe", "backend").await;
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
    let (warm_status, _, warm_body) = http1_oneshot(h.hcm, "/slow", "backend").await;
    assert_eq!(warm_status, 200, "warm /slow → 200 via backend_slow");
    assert_eq!(warm_body, b"from-slow");

    // Start an in-flight /slow request WITHOUT awaiting it.
    let hcm = h.hcm;
    let inflight = tokio::spawn(async move { http1_oneshot(hcm, "/slow", "backend").await });

    // Give it a moment to connect + send headers + reach the slow backend, so it
    // has surely SNAPSHOTTED the (old) route table at entry before the reload.
    tokio::time::sleep(Duration::from_millis(400)).await;

    // Atomic-rename → a new table that routes /probe → backend_a and DROPS /slow.
    // A request entering AFTER this swap would 404 /slow; the in-flight one,
    // having read the route handle once at entry, must complete under the OLD
    // table.
    atomic_rename_over(&h.rds_path, &rds_routing_to("backend_a"));
    // Convergence: the reload succeeded (table is valid) → update_success == 2.
    wait_for_stat(h.admin, C_SUCCESS, 2, Duration::from_secs(8)).await;

    // The in-flight request completes 200 under the OLD table — no panic, no
    // disruption. End-to-end confirmation of the Task-2 read-once snapshot.
    let (status, _, body) = tokio::time::timeout(Duration::from_secs(10), inflight)
        .await
        .expect("in-flight request did not finish within 10s")
        .expect("in-flight task panicked");
    assert_eq!(
        status, 200,
        "in-flight /slow completes 200 under the OLD route table across a reload"
    );
    assert_eq!(body, b"from-slow");

    // Sanity: the NEW table is live — /probe now routes (200), and /slow is gone.
    let (probe_status, _, _) = http1_oneshot(h.hcm, "/probe", "backend").await;
    assert_eq!(
        probe_status, 200,
        "post-reload /probe → 200 (new table live)"
    );
}
