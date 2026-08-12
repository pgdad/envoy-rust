//! 26 Task 3: `RdsWatcher` — the 5th periodic-background primitive. 27 Task 3
//! (ADR-0067): now a thin RDS-domain adapter over the domain-free
//! [`envoy_cluster::XdsFileWatcher`] poll/mtime/cancel core.
//!
//! The poll loop, mtime-detect, `JoinHandle` holding, and `shutdown`/drain all
//! live in the generic core ([`envoy_cluster::xds_watch`]); this module keeps
//! only the RDS-DOMAIN pieces: the [`WatchTarget`] context (path +
//! route_config_name + the swappable [`HCMConfig`] store + the `rds.*`
//! counters), the [`RdsCounters`] taxonomy handles, and the [`reload`] §6.2
//! pipeline (reparse → revalidate → atomic `store_route_config` swap with a
//! WARM-REJECT on any failure). `RdsWatcher::spawn` builds, for each RDS
//! target, a `Box<dyn FnMut() + Send>` reload closure that runs `reload()` and
//! logs a warm-reject (with the RDS path + route_config_name context) on
//! failure, then constructs an inner `XdsFileWatcher` and delegates
//! `shutdown`/`task_count` to it.
//!
//! The envoy-bin call site is UNCHANGED (`RdsWatcher::spawn(targets, token)` +
//! `.shutdown().await`): `RdsWatcher`/`WatchTarget`/`RdsCounters` stay in this
//! crate, only their loop machinery now lives in `envoy-cluster`. This is the
//! "second instance reveals the abstraction" refactor — EDS (phase 27, a later
//! task) consumes the SAME `XdsFileWatcher` core.
//!
//! §5.2 inertness: the target list is built by walking the listeners for HCMs
//! with `rds` configured. A bootstrap with no rds HCM yields an empty target
//! list → zero watch tasks (the watcher is constructed but inert), exactly
//! mirroring how the health scheduler / outlier manager spawn nothing when
//! their feature is unconfigured.
//!
//! 26 Task 4: the per-target `reload` runs the real §6.2 pipeline (ADR-0066)
//! — reparse → revalidate → atomic `store_route_config` swap, with a WARM-REJECT
//! on any failure (the last-good table is kept; the proxy never crashes). The
//! `rds.*` counters live on each [`WatchTarget`] and are ticked per the locked
//! taxonomy (the envoy-bin target-walk re-resolves the same registered handles
//! by name).

use std::path::PathBuf;
use std::sync::Arc;

use envoy_cluster::{WatchTarget as XdsWatchTarget, XdsFileWatcher};
use envoy_stats::Counter;
use tokio_util::sync::CancellationToken;

use crate::hcm::HCMConfig;

/// 26 Task 4: the 5 `rds.*` counter handles a watch target ticks on each reload,
/// per the §6.2-LOCKED taxonomy (ADR-0066). Every attempt ticks `update_attempt`;
/// a success additionally ticks `update_success` + `config_reload`; an
/// {IO, parse} failure ticks `update_failure`; a {name-not-found, unknown-cluster}
/// rejection ticks `update_rejected`. These are the SAME handles
/// `envoy_listener::register_rds_stats` registers (by hierarchical name) at
/// initial load — re-registering a name returns the same underlying counter, so
/// the watcher continues the same series the initial load seeded at `1/1/0/0/1`.
#[derive(Debug, Clone)]
pub struct RdsCounters {
    pub update_attempt: Arc<Counter>,
    pub update_success: Arc<Counter>,
    pub update_failure: Arc<Counter>,
    pub update_rejected: Arc<Counter>,
    pub config_reload: Arc<Counter>,
}

/// 26 Task 4: one rds watch target — the reload pipeline's per-HCM context.
///
/// On a detected file change the watcher runs the §6.2 pipeline against this
/// target: reparse+revalidate `path`, then on success atomically
/// `store.store_route_config(...)` (a warm-reject keeps the live table on any
/// failure); the `counters` are ticked per the locked taxonomy. The envoy-bin
/// target-walk builds one of these per rds-configured HCM; `RdsWatcher::spawn`
/// adapts each into a generic `envoy_cluster::WatchTarget` reload closure that
/// the inner `XdsFileWatcher` polls.
///
/// For an H2 listener whose `envoy_http2::HCMConfig` wraps an inner
/// `Arc<envoy_http1::HCMConfig>`, `store` MUST be that INNER h1 config — it
/// owns the swappable `RwLock<Arc<RouteConfiguration>>` cell (the H2 wrapper
/// only holds `.inner` + the H2 pool manager). envoy-bin threads
/// `Arc::clone(&hcm_config)` (the h1 handle it built BEFORE wrapping) so both
/// dispatch paths observe the same swappable cell.
#[derive(Debug, Clone)]
pub struct WatchTarget {
    /// The rds file to stat (and, in Task 4, re-read on change). Comes from
    /// the HCM's `rds.config_source.path_config_source.path`.
    pub path: PathBuf,
    /// The rds resource name to select from the file. Comes from the HCM's
    /// `rds.route_config_name`; [`reload`] uses it to pick the matching
    /// `RouteConfiguration` out of the parsed rds file.
    pub route_config_name: String,
    /// The Task-2 swappable-handle owner. `reload` calls
    /// `store.store_route_config(new)` to atomically swap the live route table
    /// after a successful reparse+revalidate. `store.cluster_mgr` is the
    /// immutable live cluster set the revalidation checks route references
    /// against.
    pub store: Arc<HCMConfig>,
    /// 26 Task 4: the registered `rds.*` counter handles this target ticks per
    /// the §6.2-LOCKED taxonomy (ADR-0066). Wired from envoy-bin's registry —
    /// re-resolving the names `register_rds_stats` registered returns the same
    /// underlying counters, so the watcher continues the initial-load series.
    pub counters: RdsCounters,
}

/// 26 Task 3: the rds watcher. 27 Task 3 (ADR-0067): a thin RDS-domain adapter
/// that delegates its poll/mtime/cancel/drain machinery to an inner
/// [`XdsFileWatcher`]. Drop without `shutdown()` is safe — the inner tasks
/// observe the runtime shutdown via the token — but `shutdown().await` is
/// preferred for a clean drain (mirrors the 12.2 `Scheduler`).
#[derive(Debug)]
pub struct RdsWatcher {
    inner: XdsFileWatcher,
}

impl RdsWatcher {
    /// 26 Task 3: build the rds watcher. For each RDS [`WatchTarget`] this
    /// constructs a domain-free `envoy_cluster::WatchTarget` whose reload
    /// closure runs the §6.2 [`reload`] pipeline and logs a warm-reject (with
    /// the RDS path + route_config_name context) on failure, then hands the
    /// targets to an inner [`XdsFileWatcher`]. `cancel` is the shared shutdown
    /// token — a caller cancelling it (via the envoy-bin signal token) or
    /// calling `shutdown()` terminates every loop at its next `tokio::select!`
    /// boundary.
    ///
    /// Spawn is INFALLIBLE (`-> Self`): the generic core does no fallible work
    /// at spawn time and the RDS reload closure handles its own errors (warm
    /// reject) — unlike the 12.2 `Scheduler::spawn`. The envoy-bin call site is
    /// UNCHANGED by the 27 Task 3 refactor.
    pub fn spawn(targets: Vec<WatchTarget>, cancel: CancellationToken) -> Self {
        let xds_targets = targets
            .into_iter()
            .map(|target| XdsWatchTarget {
                path: target.path.clone(),
                reload: Box::new(move || {
                    // 26 Task 4: run the real reparse+revalidate+atomic-swap
                    // pipeline. A failed reload is WARM-REJECTED (the live table
                    // is kept and the failure-class counter ticked inside
                    // `reload`); we log it here and keep watching for the next
                    // edit. The closure never propagates a reload error — a bad
                    // RDS file must NOT take the proxy down.
                    if let Err(err) = reload(&target) {
                        tracing::warn!(
                            path = %target.path.display(),
                            route_config_name = %target.route_config_name,
                            error = ?err,
                            "rds reload warm-rejected; keeping last-good route table",
                        );
                    }
                }),
            })
            .collect();
        RdsWatcher {
            inner: XdsFileWatcher::spawn(xds_targets, cancel),
        }
    }

    /// 26 Task 3: cancel every watch task and await its drain. Delegates to the
    /// inner [`XdsFileWatcher`] (mirrors the 12.2 `Scheduler::shutdown`).
    pub async fn shutdown(self) {
        self.inner.shutdown().await;
    }

    /// 26 Task 3: test helper — count of spawned watch tasks (mirrors the 12.2
    /// `Scheduler::task_count`). Zero when no rds HCM is configured (the §5.2
    /// inertness witness). Delegates to the inner [`XdsFileWatcher`].
    pub fn task_count(&self) -> usize {
        self.inner.task_count()
    }
}

/// 26 Task 4 (ADR-0066): the rds reload pipeline.
///
/// Every call ticks `update_attempt`. Steps 1-4 (read+parse+select+revalidate)
/// run via `envoy_config::reparse_and_select_route_config` — pure work producing
/// a candidate `RouteConfiguration` OUTSIDE any lock (CARRY-FORWARD (a): the
/// write critical section must stay a single Arc move). On success the candidate
/// is atomically installed via `store_route_config` and `update_success` +
/// `config_reload` tick. On ANY failure the live table is KEPT (warm-reject) and
/// the failure-class counter ticks: `update_failure` for {IO, parse},
/// `update_rejected` for {route_config_name-not-found, unknown-cluster}. The
/// unknown-cluster rejection is the recorded divergence vs Envoy — see the
/// module + `reparse_and_select_route_config` docs.
fn reload(target: &WatchTarget) -> Result<(), envoy_config::ConfigError> {
    target.counters.update_attempt.add(1);
    // Steps 1-4, OUTSIDE the write lock. The cluster predicate resolves names
    // through the live (immutable) ClusterManager; `get(..).is_some()` is the
    // existence check the request path relies on.
    let cluster_mgr = Arc::clone(&target.store.cluster_mgr);
    let result = envoy_config::reparse_and_select_route_config(
        &target.path,
        &target.route_config_name,
        &move |name| cluster_mgr.get(name).is_some(),
    );
    match result {
        Ok(new_rc) => {
            // Step 5: atomic swap — the ONLY part touching the write lock.
            target.store.store_route_config(Arc::new(new_rc));
            target.counters.update_success.add(1);
            target.counters.config_reload.add(1);
            Ok(())
        }
        Err(err) => {
            // Warm-reject: classify the error and tick the matching counter; the
            // live route table is left untouched (we never reached step 5).
            match &err {
                // {IO error, malformed YAML/parse error} → update_failure.
                envoy_config::ConfigError::RdsFileError { .. }
                | envoy_config::ConfigError::RdsParseError { .. } => {
                    target.counters.update_failure.add(1);
                }
                // {route_config_name absent, route→unknown-cluster,
                //  mutually-exclusive redirect oneof} → update_rejected.
                //
                // 76.2 §5.2 re-entry (REVIEW.md I-1): the two Redirect*Conflict
                // variants were added to `reparse_and_select_route_config`'s
                // returnable set by Task 8 (which closed CF-76-2 by having the
                // warm path re-validate the redirect oneofs) but NOT to this
                // classifier, so a hot reload of exactly the config CF-76-2 was
                // filed about fell to `other =>` and PANICKED — `panic =
                // "abort"` in release, i.e. the whole proxy process died on a
                // routine config edit. They belong here: they are VALIDATION
                // REJECTIONS, the same class as `UnknownCluster` — the config
                // was read and parsed fine, it is the CONTENT that is refused.
                envoy_config::ConfigError::RdsRouteConfigNotFound { .. }
                | envoy_config::ConfigError::UnknownCluster(_)
                | envoy_config::ConfigError::RedirectPathRewriteConflict { .. }
                | envoy_config::ConfigError::RedirectSchemeRewriteConflict { .. } => {
                    target.counters.update_rejected.add(1);
                }
                // `reparse_and_select_route_config` can return ONLY the six
                // variants matched above. Any other `ConfigError` here means the
                // producer grew a new returnable variant without updating this
                // classifier — fail loud rather than silently misbucket it. If a
                // new variant is added, this match must be extended explicitly.
                // NOTE (I-1): "fail loud" here means ABORT in release. Widening
                // the producer's returnable set is a CROSS-CRATE change — when
                // you add an `Err` arm to `reparse_and_select_route_config`,
                // grep its callers, because the compiler will NOT tell you:
                // `unreachable!()` compiles clean.
                other => {
                    unreachable!(
                        "reparse_and_select_route_config returned an unexpected \
                         ConfigError variant not handled by the reload classifier: \
                         {other:?}"
                    )
                }
            }
            Err(err)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hcm::HCMStats;
    use envoy_cluster::xds_watch::POLL_INTERVAL;
    use envoy_config::RouteConfiguration;
    use envoy_config::runtime::RuntimeSnapshot;
    use std::sync::RwLock;
    use std::time::Duration;

    // 26 Task 4: build the 5 rds.* counters from a fresh test registry. Mirrors
    // the names `register_rds_stats` (envoy-listener) emits — re-registering by
    // name returns the same underlying Arc<Counter>, so a test that seeds them
    // at `1/1/0/0/1` (the post-initial-load state) and the watcher's reload tick
    // share one set of counters.
    fn test_counters(registry: &envoy_stats::StatsRegistry, base: &str) -> RdsCounters {
        let mk = |suffix: &str| {
            registry
                .register_counter(&format!("{base}.{suffix}"))
                .expect("register rds counter")
        };
        RdsCounters {
            update_attempt: mk("update_attempt"),
            update_success: mk("update_success"),
            update_failure: mk("update_failure"),
            update_rejected: mk("update_rejected"),
            config_reload: mk("config_reload"),
        }
    }

    // An RDS file naming `local_route` whose single route forwards to `cluster`.
    fn rds_file_body(cluster: &str) -> String {
        format!(
            r#"
resources:
- "@type": type.googleapis.com/envoy.config.route.v3.RouteConfiguration
  name: local_route
  virtual_hosts:
  - name: backend
    domains: ["*"]
    routes:
    - match: {{ prefix: "/" }}
      route: {{ cluster: {cluster} }}
"#
        )
    }

    // Build an `Arc<HCMConfig>` whose cluster_mgr contains exactly one cluster
    // named `cluster_name`, plus the seeded `1/1/0/0/1` rds counters. Returns
    // the store, the counters, and the registry (kept alive by the caller).
    async fn store_with_cluster(
        cluster_name: &str,
    ) -> (Arc<HCMConfig>, RdsCounters, Arc<envoy_stats::StatsRegistry>) {
        let yaml = format!(
            r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
static_resources:
  listeners: []
  clusters:
  - name: {cluster_name}
    type: STATIC
    lb_policy: ROUND_ROBIN
    load_assignment:
      cluster_name: {cluster_name}
      endpoints:
      - lb_endpoints:
        - endpoint:
            address:
              socket_address:
                address: 127.0.0.1
                port_value: 8080
"#
        );
        let bootstrap = envoy_config::parse_bootstrap(&yaml).expect("bootstrap parses");
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let cluster_mgr = Arc::new(
            envoy_cluster::from_bootstrap(&bootstrap, Arc::clone(&registry))
                .await
                .expect("cluster mgr"),
        );
        let stats = Arc::new(HCMStats::register(&registry, "ingress_http").expect("stats"));
        let filter_pipeline = Arc::new(
            envoy_filter::FilterPipeline::build_from_config(
                &[envoy_config::HttpFilter {
                    name: "envoy.filters.http.router".to_string(),
                    typed_config: envoy_config::HttpFilterTypedConfig::Router(
                        envoy_config::RouterConfig {},
                    ),
                }],
                &registry,
                "ingress_http",
            )
            .expect("router pipeline"),
        );
        let store = Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
            cluster_mgr,
            http2_protocol_options: None,
            stats,
            access_log: vec![],
            filter_pipeline,
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "local_route".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
        });
        // Seed the rds.* counters at the post-initial-load `1/1/0/0/1` state, so
        // a successful reload moves them to `2/2/0/0/2` (the §6.2 expectation).
        let counters = test_counters(
            &registry,
            &envoy_listener::rds_counter_base("ingress_http", "local_route"),
        );
        counters.update_attempt.add(1);
        counters.update_success.add(1);
        counters.config_reload.add(1);
        (store, counters, registry)
    }

    fn write_rds(path: &std::path::Path, body: &str) {
        std::fs::write(path, body).expect("write rds file");
    }

    /// §6.2 happy reload: a valid edited RDS file (route → a KNOWN cluster) is
    /// reparsed, revalidated, and ATOMICALLY swapped into the live table.
    /// Counters move `1/1/0/0/1` → `2/2/0/0/2`.
    #[tokio::test]
    async fn reload_happy_swaps_table_and_ticks() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("rds.yaml");
        write_rds(&path, &rds_file_body("known_cluster"));
        let (store, counters, _reg) = store_with_cluster("known_cluster").await;

        // Before: the seeded last-good table has zero virtual hosts.
        assert!(store.current_route_config().virtual_hosts.is_empty());

        let target = WatchTarget {
            path,
            route_config_name: "local_route".to_string(),
            store: Arc::clone(&store),
            counters: counters.clone(),
        };
        reload(&target).expect("happy reload returns Ok");

        // After: the live table is the freshly-parsed one (one virtual host).
        let live = store.current_route_config();
        assert_eq!(live.name, "local_route");
        assert_eq!(live.virtual_hosts.len(), 1, "table was swapped");

        assert_eq!(counters.update_attempt.value(), 2);
        assert_eq!(counters.update_success.value(), 2);
        assert_eq!(counters.config_reload.value(), 2);
        assert_eq!(counters.update_failure.value(), 0);
        assert_eq!(counters.update_rejected.value(), 0);
    }

    /// §6.2 warm-reject (update_failure): malformed YAML keeps the last-good
    /// table byte-unchanged and ticks attempt + failure only.
    #[tokio::test]
    async fn reload_malformed_keeps_last_good_and_ticks_failure() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("rds.yaml");
        write_rds(&path, "resources: [unclosed");
        let (store, counters, _reg) = store_with_cluster("known_cluster").await;
        let before = store.current_route_config();

        let target = WatchTarget {
            path,
            route_config_name: "local_route".to_string(),
            store: Arc::clone(&store),
            counters: counters.clone(),
        };
        reload(&target).expect_err("malformed reload returns Err");

        // The live Arc is byte-unchanged (same pointer, same content).
        let after = store.current_route_config();
        assert!(Arc::ptr_eq(&before, &after), "last-good table kept");

        assert_eq!(counters.update_attempt.value(), 2);
        assert_eq!(counters.update_failure.value(), 1);
        assert_eq!(counters.update_success.value(), 1, "no success tick");
        assert_eq!(counters.update_rejected.value(), 0);
        assert_eq!(counters.config_reload.value(), 1, "no reload tick");
    }

    /// §6.2 warm-reject (update_rejected): the requested route_config_name is
    /// absent from the edited file → keep last-good, tick attempt + rejected.
    #[tokio::test]
    async fn reload_missing_route_config_name_keeps_last_good_and_ticks_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("rds.yaml");
        // File names a DIFFERENT route config than the target.
        write_rds(
            &path,
            &rds_file_body("known_cluster").replace("local_route", "other_route"),
        );
        let (store, counters, _reg) = store_with_cluster("known_cluster").await;
        let before = store.current_route_config();

        let target = WatchTarget {
            path,
            route_config_name: "local_route".to_string(),
            store: Arc::clone(&store),
            counters: counters.clone(),
        };
        reload(&target).expect_err("name-not-found reload returns Err");

        let after = store.current_route_config();
        assert!(Arc::ptr_eq(&before, &after), "last-good table kept");

        assert_eq!(counters.update_attempt.value(), 2);
        assert_eq!(counters.update_rejected.value(), 1);
        assert_eq!(counters.update_failure.value(), 0);
        assert_eq!(counters.update_success.value(), 1);
        assert_eq!(counters.config_reload.value(), 1);
    }

    /// §6.2 warm-reject (update_rejected) — THE RECORDED DIVERGENCE (ADR-0066):
    /// an edited file whose route references a cluster NOT in the live manager
    /// is REJECTED (envoy-rust diverges from Envoy's accept-and-503 because the
    /// request path `.expect()`s cluster existence). Keep last-good; tick
    /// attempt + rejected.
    #[tokio::test]
    async fn reload_unknown_cluster_keeps_last_good_and_ticks_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("rds.yaml");
        // Route forwards to a cluster the manager does NOT contain.
        write_rds(&path, &rds_file_body("ghost_cluster"));
        let (store, counters, _reg) = store_with_cluster("known_cluster").await;
        let before = store.current_route_config();

        let target = WatchTarget {
            path,
            route_config_name: "local_route".to_string(),
            store: Arc::clone(&store),
            counters: counters.clone(),
        };
        reload(&target).expect_err("unknown-cluster reload returns Err");

        let after = store.current_route_config();
        assert!(Arc::ptr_eq(&before, &after), "last-good table kept");

        assert_eq!(counters.update_attempt.value(), 2);
        assert_eq!(counters.update_rejected.value(), 1);
        assert_eq!(counters.update_failure.value(), 0);
        assert_eq!(counters.update_success.value(), 1);
        assert_eq!(counters.config_reload.value(), 1);
    }

    // 76.2 §5.2 re-entry: an RDS file naming `local_route` whose single route
    // is a `redirect:` carrying the given oneof body. Used by the two
    // conflicting-oneof reload tests below; `body` is spliced verbatim so each
    // test names the exact pair it exercises.
    fn rds_redirect_file_body(redirect_body: &str) -> String {
        format!(
            r#"
resources:
- "@type": type.googleapis.com/envoy.config.route.v3.RouteConfiguration
  name: local_route
  virtual_hosts:
  - name: backend
    domains: ["*"]
    routes:
    - match: {{ prefix: "/" }}
      redirect: {{ {redirect_body} }}
"#
        )
    }

    /// 76.2 §5.2 re-entry — `REVIEW.md` ISSUE I-1, the `path_redirect` +
    /// `prefix_rewrite` half.
    ///
    /// Task 8 closed carry-forward CF-76-2 by making
    /// `reparse_and_select_route_config` re-validate the `redirect:` oneofs on
    /// the warm path, which WIDENED the set of `ConfigError` variants it can
    /// return to include `RedirectPathRewriteConflict` and
    /// `RedirectSchemeRewriteConflict`. Its sole production caller — the
    /// classifier in [`reload`] — still matched only four variants and ended
    /// `other => unreachable!(…)`, so the reload PANICKED on exactly the config
    /// CF-76-2 was filed about. `Cargo.toml` sets `panic = "abort"` under
    /// `[profile.release]`, so that panic kills the whole proxy process on a
    /// routine config edit — converting CF-76-2's failure mode from "installs a
    /// bad config" into "aborts the proxy", which is worse than the gap it
    /// closed.
    ///
    /// The intended outcome is an ordinary WARM REJECT, in the same class as
    /// `UnknownCluster`: last-good table retained, `update_rejected` ticked,
    /// process alive. `unreachable!()` compiles cleanly, so gate (e) is
    /// structurally blind to this whole class — only a `reload()`-LEVEL test
    /// can catch it, which is precisely why the gap shipped: the four existing
    /// `reload()` tests cover happy / malformed / name-not-found /
    /// unknown-cluster only, and the two Task-8 tests
    /// (`crates/envoy-config/src/rds.rs`) call the producer directly and never
    /// reach `reload()`.
    #[tokio::test]
    async fn reload_conflicting_redirect_path_oneof_keeps_last_good_and_ticks_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("rds.yaml");
        write_rds(
            &path,
            &rds_redirect_file_body(r#"path_redirect: "/p", prefix_rewrite: "/q""#),
        );
        let (store, counters, _reg) = store_with_cluster("known_cluster").await;
        let before = store.current_route_config();

        let target = WatchTarget {
            path,
            route_config_name: "local_route".to_string(),
            store: Arc::clone(&store),
            counters: counters.clone(),
        };
        // Before the I-1 fix this line did not merely return Err — it PANICKED
        // inside the classifier.
        let err = reload(&target).expect_err("conflicting redirect oneof reload returns Err");
        assert!(
            matches!(
                err,
                envoy_config::ConfigError::RedirectPathRewriteConflict { .. }
            ),
            "expected RedirectPathRewriteConflict, got {err:?}"
        );

        let after = store.current_route_config();
        assert!(Arc::ptr_eq(&before, &after), "last-good table kept");

        assert_eq!(counters.update_attempt.value(), 2);
        assert_eq!(
            counters.update_rejected.value(),
            1,
            "classified as rejected"
        );
        assert_eq!(counters.update_failure.value(), 0, "not a failure");
        assert_eq!(counters.update_success.value(), 1, "no success tick");
        assert_eq!(counters.config_reload.value(), 1, "no reload tick");
    }

    /// 76.2 §5.2 re-entry — `REVIEW.md` ISSUE I-1, the `https_redirect` +
    /// `scheme_redirect` half.
    ///
    /// The I-1 fix adds TWO variants to the `update_rejected` arm. Pinning only
    /// one of them would leave the other arm unexercised — which is the exact
    /// shape of the gap this re-entry exists to close. See the sibling test
    /// above for the full chain.
    #[tokio::test]
    async fn reload_conflicting_redirect_scheme_oneof_keeps_last_good_and_ticks_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("rds.yaml");
        write_rds(
            &path,
            &rds_redirect_file_body(r#"https_redirect: true, scheme_redirect: "ftp""#),
        );
        let (store, counters, _reg) = store_with_cluster("known_cluster").await;
        let before = store.current_route_config();

        let target = WatchTarget {
            path,
            route_config_name: "local_route".to_string(),
            store: Arc::clone(&store),
            counters: counters.clone(),
        };
        let err = reload(&target).expect_err("conflicting redirect oneof reload returns Err");
        assert!(
            matches!(
                err,
                envoy_config::ConfigError::RedirectSchemeRewriteConflict { .. }
            ),
            "expected RedirectSchemeRewriteConflict, got {err:?}"
        );

        let after = store.current_route_config();
        assert!(Arc::ptr_eq(&before, &after), "last-good table kept");

        assert_eq!(counters.update_attempt.value(), 2);
        assert_eq!(
            counters.update_rejected.value(),
            1,
            "classified as rejected"
        );
        assert_eq!(counters.update_failure.value(), 0, "not a failure");
        assert_eq!(counters.update_success.value(), 1, "no success tick");
        assert_eq!(counters.config_reload.value(), 1, "no reload tick");
    }

    /// 76.2 §5.2 re-entry (I-1) — the ACCEPT-direction control. Without it the
    /// two reject tests above would pass just as well if `reload` rejected
    /// EVERY redirect route, and the warm path's redirect support would be
    /// silently dead. A valid `redirect:` route must still swap the table and
    /// tick success, exactly like the happy `route:` case.
    #[tokio::test]
    async fn reload_accepts_a_valid_redirect_route_and_swaps_table() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("rds.yaml");
        write_rds(
            &path,
            &rds_redirect_file_body(r#"host_redirect: "example.com""#),
        );
        let (store, counters, _reg) = store_with_cluster("known_cluster").await;

        assert!(store.current_route_config().virtual_hosts.is_empty());

        let target = WatchTarget {
            path,
            route_config_name: "local_route".to_string(),
            store: Arc::clone(&store),
            counters: counters.clone(),
        };
        reload(&target).expect("a valid redirect route reloads warm");

        let live = store.current_route_config();
        assert_eq!(live.virtual_hosts.len(), 1, "table was swapped");
        assert!(matches!(
            live.virtual_hosts[0].routes[0].action,
            envoy_config::RouteAction::Redirect(_)
        ));

        assert_eq!(counters.update_attempt.value(), 2);
        assert_eq!(counters.update_success.value(), 2);
        assert_eq!(counters.config_reload.value(), 2);
        assert_eq!(counters.update_failure.value(), 0);
        assert_eq!(counters.update_rejected.value(), 0);
    }

    // A minimal `Arc<HCMConfig>` for the WatchTarget's `store` field. The
    // lifecycle tests below never edit the watched file (the mtime stays
    // stable, so `reload` never runs), so this only needs to be a
    // structurally-valid handle (the swappable route-table cell is present).
    async fn minimal_store() -> Arc<HCMConfig> {
        let yaml = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
static_resources:
  listeners: []
  clusters: []
"#;
        let bootstrap = envoy_config::parse_bootstrap(yaml).expect("bootstrap parses");
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let cluster_mgr = Arc::new(
            envoy_cluster::from_bootstrap(&bootstrap, Arc::clone(&registry))
                .await
                .expect("cluster mgr"),
        );
        let stats = Arc::new(HCMStats::register(&registry, "ingress_http").expect("stats"));
        let filter_pipeline = Arc::new(
            envoy_filter::FilterPipeline::build_from_config(
                &[envoy_config::HttpFilter {
                    name: "envoy.filters.http.router".to_string(),
                    typed_config: envoy_config::HttpFilterTypedConfig::Router(
                        envoy_config::RouterConfig {},
                    ),
                }],
                &registry,
                "ingress_http",
            )
            .expect("router pipeline"),
        );
        Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
            cluster_mgr,
            http2_protocol_options: None,
            stats,
            access_log: vec![],
            filter_pipeline,
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "local_route".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![],
            })),
            runtime: Arc::new(RuntimeSnapshot::default()),
        })
    }

    /// Lifecycle: spawn over a target whose file mtime never changes → the
    /// watcher idles (the §5.2 / 0028 inertness witness) and `shutdown()`
    /// terminates the loop promptly. Driven on a paused clock so the poll
    /// interval advances deterministically without real sleeps.
    #[tokio::test(start_paused = true)]
    async fn idles_when_mtime_stable_and_shuts_down() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("rds.yaml");
        std::fs::write(&path, "resources: []\n").expect("write rds file");

        let reg = envoy_stats::StatsRegistry::new();
        let target = WatchTarget {
            path: path.clone(),
            route_config_name: "local_route".to_string(),
            store: minimal_store().await,
            counters: test_counters(
                &reg,
                &envoy_listener::rds_counter_base("ingress_http", "local_route"),
            ),
        };
        let cancel = CancellationToken::new();
        let watcher = RdsWatcher::spawn(vec![target], cancel.clone());
        assert_eq!(watcher.task_count(), 1, "one watch task per target");

        // Advance several poll intervals WITHOUT touching the file. No reload
        // fires (the §6.2 pipeline runs only on an mtime CHANGE), and the loop
        // keeps running — i.e. it idles. This asserts the `RdsWatcher` adapter
        // wiring over the generic `XdsFileWatcher` core stays inert on a stable
        // mtime (lifecycle, not reload semantics — §5.2).
        for _ in 0..5 {
            tokio::time::advance(POLL_INTERVAL).await;
        }

        // Cancel must terminate the loop promptly (it exits the cancel branch,
        // not after the next tick). `shutdown()` joins the handle.
        let drained = tokio::time::timeout(Duration::from_secs(3), watcher.shutdown()).await;
        assert!(drained.is_ok(), "shutdown returned promptly on cancel");
    }

    /// Lifecycle: an empty target list spawns ZERO tasks (the §5.2 inertness
    /// invariant — a non-rds bootstrap yields an empty target list) and
    /// `shutdown()` is a clean no-op.
    #[tokio::test(start_paused = true)]
    async fn empty_targets_spawn_no_tasks() {
        let cancel = CancellationToken::new();
        let watcher = RdsWatcher::spawn(vec![], cancel);
        assert_eq!(watcher.task_count(), 0, "no watch task when no rds target");
        watcher.shutdown().await;
    }

    /// Lifecycle: `cancel.cancel()` (rather than `shutdown()`) also terminates
    /// the loop — the watcher observes the shared token, mirroring the
    /// envoy-bin signal-token wiring.
    #[tokio::test(start_paused = true)]
    async fn cancel_token_terminates_loop() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("rds.yaml");
        std::fs::write(&path, "resources: []\n").expect("write rds file");

        let reg = envoy_stats::StatsRegistry::new();
        let target = WatchTarget {
            path,
            route_config_name: "local_route".to_string(),
            store: minimal_store().await,
            counters: test_counters(
                &reg,
                &envoy_listener::rds_counter_base("ingress_http", "local_route"),
            ),
        };
        let cancel = CancellationToken::new();
        let watcher = RdsWatcher::spawn(vec![target], cancel.clone());
        tokio::time::advance(POLL_INTERVAL).await;

        cancel.cancel();
        let drained = tokio::time::timeout(Duration::from_secs(3), watcher.shutdown()).await;
        assert!(drained.is_ok(), "loop exits on external cancel");
    }
}
