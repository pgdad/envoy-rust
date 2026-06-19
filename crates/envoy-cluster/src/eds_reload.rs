//! 27 Task 4 (D3 + D4, §6.2-LOCKED / ADR-0068 V4): the EDS endpoint-reload
//! pipeline plus the in-crate watch-target builder.
//!
//! This is envoy-cluster's OWN domain code (parallel to how RDS domain code
//! lives in envoy-http1's `rds_watcher.rs`); it produces generic
//! [`crate::xds_watch::WatchTarget`]s whose `Box<dyn FnMut>` closure runs the
//! EDS reload pipeline + logs the warm-reject with EDS context (path +
//! selection name). It deliberately lives HERE, not in `xds_watch.rs`: Task 3
//! made `xds_watch.rs` a domain-free generic poll/mtime/cancel core (praised by
//! the code-quality review), and that purity is preserved — the PLAN's
//! "in xds_watch.rs" wording predates Task 3's clean split, and this honors the
//! plan's intent (EDS code in envoy-cluster) while keeping the generic core
//! pristine.
//!
//! ## §6.2-LOCKED bad-reload taxonomy (ADR-0068 V4 — class → counter, MIRROR Envoy)
//!
//! - (a) IO / parse / malformed → `update_attempt` + `update_failure`, KEEP.
//! - (b) no CLA matches `selection_name` (envelope non-empty) →
//!   `update_attempt` + `update_rejected`, KEEP.
//! - (c) matched CLA has an unparseable endpoint → `update_attempt` +
//!   `update_rejected`, KEEP.
//! - (d) matched CLA has `endpoints: []` → `update_attempt` + `update_success`,
//!   **APPLY the empty set** (→ 503 via `pick()` returning `None`). This MIRRORS
//!   Envoy (an empty assignment is a successful update, not a reject).
//! - (e) `resources: []` (zero CLAs) → `update_attempt` + `update_empty`, KEEP.
//!
//! Happy path (V3): `update_attempt` + `update_success` each +1;
//! `store_endpoints(Arc::new(new))`.
//!
//! ## Lock discipline
//!
//! Reparse + select + validate produce a candidate `Vec<SocketAddr>` OUTSIDE any
//! lock; on success/apply-empty the ONLY lock touch is the single
//! `store_endpoints(Arc::new(candidate))`. This mirrors the phase-26 RDS
//! `reload()` discipline — the write critical section never widens.

use std::path::PathBuf;
use std::sync::Arc;

use crate::cluster::{Cluster, ClusterManager, parse_numeric_endpoint};
use crate::xds_watch::WatchTarget;

/// 27 Task 4: the per-target EDS reload context. Carries the swap-owner cluster
/// handle (`Arc<Cluster>`, which owns the swappable endpoint cell + the retained
/// `EdsReloadState`). The `path` is duplicated onto the generic
/// [`WatchTarget`] for the watcher to stat; the reload closure re-reads it.
struct EdsReloadTarget {
    /// The swap-owner cluster. `reload` calls `cluster.store_endpoints(new)` on
    /// the success / apply-empty dispositions, and reads `cluster.eds_reload`
    /// (the path, selection name, and 5 counter handles) for the pipeline.
    cluster: Arc<Cluster>,
}

/// 27 Task 4: classify a reload outcome for the warm-reject log line. The
/// pipeline ticks the counters itself (per the V4 taxonomy); this is returned
/// so the watch closure can log a context-rich warning on the KEEP dispositions
/// (a / b / c / e). Success and apply-empty return `Ok(())`.
#[derive(Debug)]
enum EdsReloadError {
    /// (a) IO read or YAML/`@type` parse failure.
    IoOrParse(envoy_config::ConfigError),
    /// (b) no CLA in the (non-empty) envelope matches the selection name.
    NoMatchingCla,
    /// (c) the matched CLA carries an endpoint whose address does not parse as
    /// a numeric `SocketAddr`.
    EndpointParse(crate::ClusterError),
    /// (e) the envelope carried zero CLAs (`resources: []`).
    EmptyEnvelope,
}

impl std::fmt::Display for EdsReloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EdsReloadError::IoOrParse(e) => write!(f, "io/parse failure: {e}"),
            EdsReloadError::NoMatchingCla => {
                write!(f, "no ClusterLoadAssignment matched the selection name")
            }
            EdsReloadError::EndpointParse(e) => write!(f, "endpoint parse failure: {e}"),
            EdsReloadError::EmptyEnvelope => write!(f, "envelope carried zero resources"),
        }
    }
}

/// 27 Task 4 (§6.2-LOCKED V4): the EDS reload pipeline.
///
/// Every call ticks `update_attempt`. Steps (re-read → `parse_eds_file` →
/// select the CLA whose `cluster_name == selection_name` → revalidate each
/// endpoint via [`parse_numeric_endpoint`]) run as PURE work producing a
/// candidate `Vec<SocketAddr>` OUTSIDE any lock. On success / apply-empty the
/// candidate is atomically installed via the single `store_endpoints` write
/// (the ONLY lock touch) and `update_success` ticks. On the KEEP dispositions
/// the live set is untouched and the class counter ticks per the V4 table.
///
/// Returns `Ok(())` on success / apply-empty (both APPLY); `Err` on the KEEP
/// dispositions so the watch closure can log a warm-reject (the error is NEVER
/// propagated past the closure — a bad file must not take the proxy down).
fn reload(target: &EdsReloadTarget) -> Result<(), EdsReloadError> {
    let eds = target
        .cluster
        .eds_reload
        .as_ref()
        .expect("EdsReloadTarget always wraps an EDS cluster with eds_reload state");
    eds.update_attempt.add(1);

    // ── PURE work, OUTSIDE any lock ───────────────────────────────────────
    let path_str = eds.path.to_string_lossy();

    // (a) IO read.
    let contents = match std::fs::read_to_string(&eds.path) {
        Ok(c) => c,
        Err(source) => {
            eds.update_failure.add(1);
            return Err(EdsReloadError::IoOrParse(
                envoy_config::ConfigError::EdsFileError {
                    path: path_str.into_owned(),
                    source,
                },
            ));
        }
    };

    // (a) parse / `@type` / malformed.
    let clas = match envoy_config::parse_eds_file(&path_str, &contents) {
        Ok(clas) => clas,
        Err(e) => {
            eds.update_failure.add(1);
            return Err(EdsReloadError::IoOrParse(e));
        }
    };

    // (e) `resources: []` — zero CLAs. Distinct from (d): KEEP last-good.
    if clas.is_empty() {
        eds.update_empty.add(1);
        return Err(EdsReloadError::EmptyEnvelope);
    }

    // (b) select the CLA whose cluster_name == selection_name. No match (in a
    // non-empty envelope) → reject, KEEP.
    let cla = match clas
        .into_iter()
        .find(|la| la.cluster_name == eds.selection_name)
    {
        Some(cla) => cla,
        None => {
            eds.update_rejected.add(1);
            return Err(EdsReloadError::NoMatchingCla);
        }
    };

    // (c) revalidate each endpoint as a numeric SocketAddr — via the SAME
    // helper the startup path uses. A parse failure is caught LOCALLY (NOT
    // `?`-propagated — that is the startup-fatal path) and mapped to the V4(c)
    // reject disposition.
    let mut candidate: Vec<std::net::SocketAddr> = Vec::new();
    for locality in &cla.endpoints {
        for lbe in &locality.lb_endpoints {
            let sa = &lbe.endpoint.address.socket_address;
            match parse_numeric_endpoint(&target.cluster.name, &sa.address, sa.port_value) {
                Ok(addr) => candidate.push(addr),
                Err(e) => {
                    eds.update_rejected.add(1);
                    return Err(EdsReloadError::EndpointParse(e));
                }
            }
        }
    }

    // ── (d) apply-empty + (V3) happy path: BOTH are update_success + APPLY ──
    // An empty candidate (the matched CLA carried `endpoints: []`) is a SUCCESS
    // that stores the empty set (→ pick() returns None → the 503 path),
    // MIRRORING Envoy — NOT a reject. The single `store_endpoints` is the only
    // lock touch; the reparse/revalidate above stayed outside the lock.
    target.cluster.store_endpoints(Arc::new(candidate));
    eds.update_success.add(1);
    Ok(())
}

/// 27 Task 4 (ADR-0068 Decision-5): build the EDS watch targets by walking the
/// cluster manager IN-CRATE. Returns one generic [`WatchTarget`] per cluster
/// that is EDS-with-a-file-path AND PLAIN, where PLAIN means
/// `endpoint_health.is_none() && outlier_detection.is_none()` — an EDS cluster
/// WITH active health checking or outlier detection gets NO watcher (its
/// endpoints stay frozen at initial load; the deferred non-goal — it is NOT
/// rejected, just skipped).
///
/// The filtering + handle-bundling MUST happen in-crate: envoy-bin cannot reach
/// `ClusterHandle.inner` (`pub(crate)`) nor the plainness fields, so this
/// sidesteps the envoy-bin→envoy-cluster encapsulation wall. Each target's
/// reload closure runs [`reload`] and logs a warm-reject (with the EDS path +
/// selection name) on the KEEP dispositions.
///
/// §5.2 inertness: a bootstrap with no plain EDS cluster yields an empty target
/// list → the generic `XdsFileWatcher` spawns zero watch tasks.
pub fn build_eds_watch_targets(cluster_mgr: &ClusterManager) -> Vec<WatchTarget> {
    let mut targets = Vec::new();
    for handle in cluster_mgr.clusters() {
        let cluster = handle.into_inner();
        // PLAIN EDS filter (Decision-5): EDS state present AND no active HC /
        // outlier detection.
        let is_plain_eds = cluster.eds_reload.is_some()
            && cluster.endpoint_health.is_none()
            && cluster.outlier_detection.is_none();
        if !is_plain_eds {
            continue;
        }
        // SAFETY of expect: `eds_reload.is_some()` checked above.
        let eds = cluster
            .eds_reload
            .as_ref()
            .expect("plain-EDS filter guarantees eds_reload is Some");
        let path: PathBuf = eds.path.clone();
        let selection_name = eds.selection_name.clone();
        let target = EdsReloadTarget {
            cluster: Arc::clone(&cluster),
        };
        targets.push(WatchTarget {
            path: path.clone(),
            reload: Box::new(move || {
                // Run the §6.2-LOCKED V4 pipeline. A KEEP disposition is
                // WARM-REJECTED (the live set is kept; the class counter ticked
                // inside `reload`); log it with EDS context and keep watching.
                // The closure NEVER propagates a reload error — a bad EDS file
                // must NOT take the proxy down.
                if let Err(err) = reload(&target) {
                    tracing::warn!(
                        path = %path.display(),
                        selection_name = %selection_name,
                        error = %err,
                        "eds reload warm-rejected; keeping last-good endpoint set",
                    );
                }
            }),
        });
    }
    targets
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    // Build a single-EDS-cluster bootstrap whose EDS file lives at `eds_path`.
    // `service_name` selects the CLA (else the cluster name). `hc`/`od` toggle
    // the active-HC / outlier-detection blocks (to exercise the plainness
    // filter). Returns the bootstrap YAML.
    fn bootstrap_yaml(eds_path: &str, service_name: Option<&str>, hc: bool, od: bool) -> String {
        let sn = service_name
            .map(|s| format!("\n    eds_cluster_config:\n      service_name: {s}\n      eds_config:\n        path_config_source:\n          path: {eds_path}"))
            .unwrap_or_else(|| format!("\n    eds_cluster_config:\n      eds_config:\n        path_config_source:\n          path: {eds_path}"));
        let hc_block = if hc {
            "\n    health_checks:\n    - timeout: 1s\n      interval: 1s\n      unhealthy_threshold: 2\n      healthy_threshold: 2\n      http_health_check:\n        path: /healthz"
        } else {
            ""
        };
        let od_block = if od {
            "\n    outlier_detection:\n      consecutive_5xx: 5"
        } else {
            ""
        };
        format!(
            r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
static_resources:
  listeners: []
  clusters:
  - name: eds_cluster
    type: EDS
    lb_policy: ROUND_ROBIN{sn}{hc_block}{od_block}
"#
        )
    }

    // An EDS file with one CLA named `cla_name` carrying the listed endpoints
    // (address:port pairs).
    fn eds_file(cla_name: &str, endpoints: &[(&str, u16)]) -> String {
        let mut s = format!(
            "resources:\n- \"@type\": type.googleapis.com/envoy.config.endpoint.v3.ClusterLoadAssignment\n  cluster_name: {cla_name}\n  endpoints:\n  - lb_endpoints:\n"
        );
        for (addr, port) in endpoints {
            s.push_str(&format!(
                "    - endpoint:\n        address:\n          socket_address: {{ address: {addr}, port_value: {port} }}\n"
            ));
        }
        if endpoints.is_empty() {
            // An empty `endpoints: []` CLA (the apply-empty case).
            s = format!(
                "resources:\n- \"@type\": type.googleapis.com/envoy.config.endpoint.v3.ClusterLoadAssignment\n  cluster_name: {cla_name}\n  endpoints: []\n"
            );
        }
        s
    }

    // Build a ClusterManager from `bootstrap_yaml`, given an existing EDS file
    // on disk (so the initial load succeeds). Returns the manager + registry.
    async fn build_mgr(yaml: &str) -> (Arc<ClusterManager>, Arc<envoy_stats::StatsRegistry>) {
        let mut bootstrap = envoy_config::parse_bootstrap(yaml).expect("bootstrap parses");
        // Run the EDS load pass so `load_assignment` is populated (mirrors the
        // envoy-bin startup sequence: parse_bootstrap → load_dynamic_resources).
        envoy_config::load_dynamic_resources(&mut bootstrap).expect("eds initial load");
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let mgr = Arc::new(
            crate::from_bootstrap(&bootstrap, Arc::clone(&registry))
                .await
                .expect("cluster mgr builds"),
        );
        (mgr, registry)
    }

    // Snapshot the 5 update_* counters of the (sole) EDS cluster as a tuple
    // (attempt, success, failure, empty, rejected).
    fn counters(cluster: &Cluster) -> (u64, u64, u64, u64, u64) {
        let e = cluster.eds_reload.as_ref().expect("eds_reload");
        (
            e.update_attempt.value(),
            e.update_success.value(),
            e.update_failure.value(),
            e.update_empty.value(),
            e.update_rejected.value(),
        )
    }

    fn target_for(mgr: &ClusterManager) -> (EdsReloadTarget, Arc<Cluster>) {
        let cluster = mgr.get("eds_cluster").expect("cluster").into_inner();
        (
            EdsReloadTarget {
                cluster: Arc::clone(&cluster),
            },
            cluster,
        )
    }

    fn write_atomic(dir: &std::path::Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        let tmp = dir.join(format!("{name}.tmp"));
        std::fs::write(&tmp, body).expect("write tmp");
        std::fs::rename(&tmp, &path).expect("atomic rename");
        path
    }

    fn current(cluster: &Cluster) -> Vec<SocketAddr> {
        cluster.current_endpoints().as_ref().clone()
    }

    /// HAPPY (V3): a valid file change lands the new set + ticks attempt/success.
    /// After one reload the trio is 2/2 with failure/empty/rejected all 0.
    #[tokio::test]
    async fn reload_happy_swaps_endpoints_and_ticks() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_atomic(
            dir.path(),
            "eds.yaml",
            &eds_file("eds_cluster", &[("127.0.0.1", 8124)]),
        );
        let yaml = bootstrap_yaml(path.to_str().unwrap(), None, false, false);
        let (mgr, _reg) = build_mgr(&yaml).await;
        let (target, cluster) = target_for(&mgr);

        // Initial load seeded 1/1/0/0/0 and one endpoint.
        assert_eq!(counters(&cluster), (1, 1, 0, 0, 0));
        assert_eq!(current(&cluster), vec!["127.0.0.1:8124".parse().unwrap()]);

        // Change the file to a NEW endpoint set, then reload.
        write_atomic(
            dir.path(),
            "eds.yaml",
            &eds_file("eds_cluster", &[("127.0.0.2", 9000), ("127.0.0.3", 9001)]),
        );
        reload(&target).expect("happy reload returns Ok");

        assert_eq!(
            current(&cluster),
            vec![
                "127.0.0.2:9000".parse().unwrap(),
                "127.0.0.3:9001".parse().unwrap()
            ],
            "new endpoint set applied"
        );
        assert_eq!(
            counters(&cluster),
            (2, 2, 0, 0, 0),
            "attempt/success +1 each"
        );
    }

    /// HAPPY selects via `service_name` (mirrors the phase-21 initial load).
    #[tokio::test]
    async fn reload_selects_by_service_name() {
        let dir = tempfile::tempdir().expect("tempdir");
        // CLA is named by the SERVICE name, not the cluster name.
        let path = write_atomic(
            dir.path(),
            "eds.yaml",
            &eds_file("svc_name", &[("127.0.0.1", 8124)]),
        );
        let yaml = bootstrap_yaml(path.to_str().unwrap(), Some("svc_name"), false, false);
        let (mgr, _reg) = build_mgr(&yaml).await;
        let (target, cluster) = target_for(&mgr);

        write_atomic(
            dir.path(),
            "eds.yaml",
            &eds_file("svc_name", &[("127.0.0.9", 9999)]),
        );
        reload(&target).expect("service_name selection reloads Ok");
        assert_eq!(current(&cluster), vec!["127.0.0.9:9999".parse().unwrap()]);
        assert_eq!(counters(&cluster), (2, 2, 0, 0, 0));
    }

    /// (a) IO failure (file deleted) → attempt+failure, KEEP last-good.
    #[tokio::test]
    async fn reload_io_failure_keeps_last_good_ticks_failure() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_atomic(
            dir.path(),
            "eds.yaml",
            &eds_file("eds_cluster", &[("127.0.0.1", 8124)]),
        );
        let yaml = bootstrap_yaml(path.to_str().unwrap(), None, false, false);
        let (mgr, _reg) = build_mgr(&yaml).await;
        let (target, cluster) = target_for(&mgr);
        let before = current(&cluster);

        std::fs::remove_file(&path).expect("delete eds file");
        reload(&target).expect_err("io failure returns Err");

        assert_eq!(current(&cluster), before, "last-good kept on IO failure");
        assert_eq!(counters(&cluster), (2, 1, 1, 0, 0), "attempt+failure");
    }

    /// (a) malformed YAML → attempt+failure, KEEP last-good.
    #[tokio::test]
    async fn reload_malformed_keeps_last_good_ticks_failure() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_atomic(
            dir.path(),
            "eds.yaml",
            &eds_file("eds_cluster", &[("127.0.0.1", 8124)]),
        );
        let yaml = bootstrap_yaml(path.to_str().unwrap(), None, false, false);
        let (mgr, _reg) = build_mgr(&yaml).await;
        let (target, cluster) = target_for(&mgr);
        let before = current(&cluster);

        write_atomic(dir.path(), "eds.yaml", "resources: [unclosed");
        reload(&target).expect_err("malformed returns Err");

        assert_eq!(current(&cluster), before, "last-good kept on parse failure");
        assert_eq!(counters(&cluster), (2, 1, 1, 0, 0), "attempt+failure");
    }

    /// (b) no CLA matches the selection name (envelope non-empty) →
    /// attempt+rejected, KEEP.
    #[tokio::test]
    async fn reload_no_matching_cla_keeps_last_good_ticks_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_atomic(
            dir.path(),
            "eds.yaml",
            &eds_file("eds_cluster", &[("127.0.0.1", 8124)]),
        );
        let yaml = bootstrap_yaml(path.to_str().unwrap(), None, false, false);
        let (mgr, _reg) = build_mgr(&yaml).await;
        let (target, cluster) = target_for(&mgr);
        let before = current(&cluster);

        // The file now carries a CLA for a DIFFERENT cluster name.
        write_atomic(
            dir.path(),
            "eds.yaml",
            &eds_file("some_other_cluster", &[("127.0.0.2", 9000)]),
        );
        reload(&target).expect_err("no-match returns Err");

        assert_eq!(current(&cluster), before, "last-good kept on no-match");
        assert_eq!(counters(&cluster), (2, 1, 0, 0, 1), "attempt+rejected");
    }

    /// (c) matched CLA has an unparseable endpoint → attempt+rejected, KEEP.
    /// The parse failure is caught LOCALLY (NOT propagated, which would kill the
    /// watch loop).
    #[tokio::test]
    async fn reload_unparseable_endpoint_keeps_last_good_ticks_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_atomic(
            dir.path(),
            "eds.yaml",
            &eds_file("eds_cluster", &[("127.0.0.1", 8124)]),
        );
        let yaml = bootstrap_yaml(path.to_str().unwrap(), None, false, false);
        let (mgr, _reg) = build_mgr(&yaml).await;
        let (target, cluster) = target_for(&mgr);
        let before = current(&cluster);

        // A non-numeric (DNS-name) address — invalid for EDS numeric semantics.
        write_atomic(
            dir.path(),
            "eds.yaml",
            &eds_file("eds_cluster", &[("not-a-numeric-host", 9000)]),
        );
        reload(&target).expect_err("unparseable endpoint returns Err");

        assert_eq!(
            current(&cluster),
            before,
            "last-good kept on endpoint parse failure"
        );
        assert_eq!(counters(&cluster), (2, 1, 0, 0, 1), "attempt+rejected");
    }

    /// (d) APPLY-EMPTY: matched CLA has `endpoints: []` → attempt+success,
    /// store the empty set → `pick()` returns None.
    #[tokio::test]
    async fn reload_apply_empty_stores_empty_and_ticks_success() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_atomic(
            dir.path(),
            "eds.yaml",
            &eds_file("eds_cluster", &[("127.0.0.1", 8124)]),
        );
        let yaml = bootstrap_yaml(path.to_str().unwrap(), None, false, false);
        let (mgr, _reg) = build_mgr(&yaml).await;
        let (target, cluster) = target_for(&mgr);

        // The matched CLA now carries an EMPTY endpoint list.
        write_atomic(dir.path(), "eds.yaml", &eds_file("eds_cluster", &[]));
        reload(&target).expect("apply-empty is a SUCCESS (Ok)");

        assert!(current(&cluster).is_empty(), "empty set applied");
        assert!(
            mgr.get("eds_cluster").unwrap().pick_endpoint().is_none(),
            "pick() returns None on the empty set (503 path)"
        );
        assert_eq!(
            counters(&cluster),
            (2, 2, 0, 0, 0),
            "attempt+success (NOT reject)"
        );
    }

    /// (e) empty envelope `resources: []` (zero CLAs) → attempt+empty, KEEP.
    /// Distinct from (d) apply-empty.
    #[tokio::test]
    async fn reload_empty_envelope_keeps_last_good_ticks_empty() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_atomic(
            dir.path(),
            "eds.yaml",
            &eds_file("eds_cluster", &[("127.0.0.1", 8124)]),
        );
        let yaml = bootstrap_yaml(path.to_str().unwrap(), None, false, false);
        let (mgr, _reg) = build_mgr(&yaml).await;
        let (target, cluster) = target_for(&mgr);
        let before = current(&cluster);

        write_atomic(dir.path(), "eds.yaml", "resources: []\n");
        reload(&target).expect_err("empty envelope returns Err");

        assert_eq!(
            current(&cluster),
            before,
            "last-good kept on empty envelope"
        );
        assert_eq!(counters(&cluster), (2, 1, 0, 1, 0), "attempt+empty");
    }

    /// Builder: a PLAIN EDS cluster yields exactly one watch target.
    #[tokio::test]
    async fn builder_plain_eds_yields_one_target() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_atomic(
            dir.path(),
            "eds.yaml",
            &eds_file("eds_cluster", &[("127.0.0.1", 8124)]),
        );
        let yaml = bootstrap_yaml(path.to_str().unwrap(), None, false, false);
        let (mgr, _reg) = build_mgr(&yaml).await;

        let targets = build_eds_watch_targets(&mgr);
        assert_eq!(targets.len(), 1, "plain EDS cluster yields one target");
        assert_eq!(targets[0].path, path);
    }

    /// Builder: an EDS cluster WITH active health checking is SKIPPED (frozen at
    /// initial load — the deferred non-goal; not rejected).
    #[tokio::test]
    async fn builder_eds_with_health_check_yields_no_target() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_atomic(
            dir.path(),
            "eds.yaml",
            &eds_file("eds_cluster", &[("127.0.0.1", 8124)]),
        );
        let yaml = bootstrap_yaml(path.to_str().unwrap(), None, true, false);
        let (mgr, _reg) = build_mgr(&yaml).await;

        let targets = build_eds_watch_targets(&mgr);
        assert!(targets.is_empty(), "EDS+HC cluster gets no watcher target");
    }

    /// Builder: an EDS cluster WITH outlier detection is SKIPPED.
    #[tokio::test]
    async fn builder_eds_with_outlier_detection_yields_no_target() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_atomic(
            dir.path(),
            "eds.yaml",
            &eds_file("eds_cluster", &[("127.0.0.1", 8124)]),
        );
        let yaml = bootstrap_yaml(path.to_str().unwrap(), None, false, true);
        let (mgr, _reg) = build_mgr(&yaml).await;

        let targets = build_eds_watch_targets(&mgr);
        assert!(
            targets.is_empty(),
            "EDS+outlier-detection cluster gets no watcher target"
        );
    }

    /// Builder: a non-EDS (STATIC) bootstrap yields zero targets (the §5.2
    /// inertness witness).
    #[tokio::test]
    async fn builder_no_eds_cluster_yields_no_target() {
        let yaml = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
static_resources:
  listeners: []
  clusters:
  - name: static_cluster
    type: STATIC
    lb_policy: ROUND_ROBIN
    load_assignment:
      cluster_name: static_cluster
      endpoints:
      - lb_endpoints:
        - endpoint:
            address:
              socket_address: { address: 127.0.0.1, port_value: 8080 }
"#;
        let (mgr, _reg) = build_mgr(yaml).await;
        assert!(build_eds_watch_targets(&mgr).is_empty());
    }
}
