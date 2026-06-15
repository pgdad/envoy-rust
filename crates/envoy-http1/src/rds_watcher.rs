//! 26 Task 3: `RdsWatcher` — the 5th periodic-background primitive.
//!
//! Mirrors the 12.2 `envoy_health::Scheduler` topology: holds the
//! `JoinHandle`s of every spawned watch task plus the shared
//! `CancellationToken`; `shutdown(self)` cancels the token and awaits the
//! handles for a clean drain. envoy-bin constructs it AFTER the HCMConfigs
//! exist (it needs their swappable `Arc<HCMConfig>` route-table handles plus
//! the rds file paths), passes `token.clone()`, and drains it via
//! `shutdown().await` on the runtime drain path.
//!
//! §5.2 inertness: the target list is built by walking the listeners for HCMs
//! with `rds` configured. A bootstrap with no rds HCM yields an empty target
//! list → zero watch tasks (the watcher is constructed but inert), exactly
//! mirroring how the health scheduler / outlier manager spawn nothing when
//! their feature is unconfigured.
//!
//! SCOPING (26 Task 3 is a SKELETON): the per-target `reload` is a no-op
//! `Ok(())` stub this task. The real reparse → revalidate → atomic
//! `store_route_config` swap is Task 4 (BLOCKED on a Linux §6.2 verification);
//! the `rds.*` counter ticks are Task 5. The struct fields and the loop's
//! reload call site are shaped so Tasks 4 & 5 fill them in WITHOUT re-plumbing
//! the envoy-bin wiring.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::hcm::HCMConfig;

/// 26 Task 3: the rds file poll cadence. A `tokio::time::interval` ticks every
/// `POLL_INTERVAL`; on each tick the loop stats the file and compares its
/// mtime against the last-seen value. Task 1's §6.2 settle/poll-bound output
/// may TUNE this constant (it is a placeholder default here — 1s matches the
/// other periodic primitives' "sensible default" cadence).
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

/// 26 Task 3: one rds watch target — the seam Tasks 4 & 5 fill in.
///
/// Task 4 calls `store.store_route_config(...)` after a successful
/// reparse+revalidate of `path` (a warm-reject per §5.5 on failure). Task 5
/// adds the registered `rds.*` `Arc<Counter>` handles. The envoy-bin
/// target-walk builds one of these per rds-configured HCM; the watcher spawns
/// one `watch_loop` per target.
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
    /// `rds.route_config_name`. Unused by the skeleton's no-op reload; Task 4
    /// uses it to pick the matching `RouteConfiguration` out of the parsed
    /// rds file.
    pub route_config_name: String,
    /// The Task-2 swappable-handle owner. Task 4 calls
    /// `store.store_route_config(new)` to atomically swap the live route table
    /// after a successful reparse+revalidate.
    pub store: Arc<HCMConfig>,
    // Task 5 will add: the registered `rds.*` Arc<Counter> handles
    // (attempt/success/failure/version) so the loop can tick them.
}

/// 26 Task 3: the rds watcher. Holds the JoinHandles of every spawned
/// `watch_loop` and the shared `CancellationToken`. Drop without `shutdown()`
/// is safe — the tasks observe the runtime shutdown via the token — but
/// `shutdown().await` is preferred for a clean drain (mirrors the 12.2
/// `Scheduler`).
#[derive(Debug)]
pub struct RdsWatcher {
    handles: Vec<JoinHandle<()>>,
    cancel: CancellationToken,
}

impl RdsWatcher {
    /// 26 Task 3: spawn one `watch_loop` per target. Returns an `RdsWatcher`
    /// holding the task handles. `cancel` is the shared shutdown token — a
    /// caller cancelling it (via the envoy-bin signal token) or calling
    /// `shutdown()` terminates every loop at its next `tokio::select!`
    /// boundary.
    ///
    /// Spawn is INFALLIBLE (`-> Self`): the skeleton registers no counters
    /// (that is Task 5) and the `reload` stub cannot fail, so there is no
    /// fallible work at spawn time — unlike the 12.2 `Scheduler::spawn`, which
    /// is `Result<_>` because it registers per-cluster counters and re-parses
    /// durations. When Task 5 adds counter registration, spawn may need to
    /// become fallible; the envoy-bin call site already `?`-threads the
    /// neighbouring primitives, so that change is local.
    pub fn spawn(targets: Vec<WatchTarget>, cancel: CancellationToken) -> Self {
        let mut handles = Vec::with_capacity(targets.len());
        for target in targets {
            let cancel = cancel.clone();
            let handle = tokio::spawn(async move {
                watch_loop(target, cancel).await;
            });
            handles.push(handle);
        }
        RdsWatcher { handles, cancel }
    }

    /// 26 Task 3: cancel every watch task and await their JoinHandles. Returns
    /// once every loop has exited at its next `tokio::select!` boundary
    /// (mirrors the 12.2 `Scheduler::shutdown`).
    pub async fn shutdown(self) {
        self.cancel.cancel();
        for handle in self.handles {
            let _ = handle.await;
        }
    }

    /// 26 Task 3: test helper — count of spawned watch tasks (mirrors the 12.2
    /// `Scheduler::task_count`). Zero when no rds HCM is configured (the §5.2
    /// inertness witness).
    pub fn task_count(&self) -> usize {
        self.handles.len()
    }
}

/// 26 Task 3: the per-target poll loop. `tokio::select!`s between
/// `cancel.cancelled()` and a `tokio::time::interval` tick. On each tick it
/// stats `target.path` and compares the mtime against the last-seen value; on
/// a change it calls `reload(&target)`. The cancel branch exits the loop
/// promptly (it does not wait for the next tick — the §5.x clean-drain
/// discipline shared with the 12.2 `probe_loop`).
async fn watch_loop(target: WatchTarget, cancel: CancellationToken) {
    let mut interval = tokio::time::interval(POLL_INTERVAL);
    // Burn the immediate first tick that `tokio::time::interval` fires at t=0
    // so the first MTIME poll happens one `POLL_INTERVAL` after spawn (parity
    // with the probe_loop / sweeper cadence — they observe a real interval
    // before their first action).
    interval.tick().await;

    // Seed the last-seen mtime from the file as it stands at spawn time. A
    // missing/unreadable file seeds `None`; the first successful stat that
    // yields a different value (including: file appears) counts as a change.
    let mut last_mtime: Option<SystemTime> = read_mtime(&target.path);

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                // Clean exit on shutdown — do not wait for the next tick.
                break;
            }
            _ = interval.tick() => {
                let current = read_mtime(&target.path);
                // Only a CHANGED, present mtime triggers a reload. A stable
                // mtime (the 0028 idle witness) or a vanished file is a no-op.
                if let Some(now) = current
                    && last_mtime != Some(now)
                {
                    last_mtime = Some(now);
                    // Task 4: real reparse+revalidate+store_route_config;
                    // warm-reject per §5.5. STUBBED to a no-op Ok(()) this task.
                    if let Err(err) = reload(&target) {
                        tracing::warn!(
                            path = %target.path.display(),
                            route_config_name = %target.route_config_name,
                            error = ?err,
                            "rds reload failed (skeleton no-op stub should never fail)",
                        );
                    }
                }
            }
        }
    }
}

/// 26 Task 3: stat `path` and return its mtime, or `None` if the file is
/// missing/unreadable/exposes no mtime. The loop compares this against the
/// last-seen value to detect a change.
fn read_mtime(path: &std::path::Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok().and_then(|m| m.modified().ok())
}

/// 26 Task 3 SKELETON STUB: the rds reload pipeline.
///
/// Task 4: real reparse+revalidate+store_route_config; warm-reject per §5.5.
/// This task leaves it a no-op `Ok(())` so the watch loop's call site, error
/// handling, and envoy-bin wiring are all in place for Task 4 to fill in
/// WITHOUT touching the spawn/target-walk/drain plumbing.
fn reload(_target: &WatchTarget) -> Result<(), std::io::Error> {
    // Task 4: read `target.path`, `parse_rds_file`, select
    // `target.route_config_name`, revalidate, then
    // `target.store.store_route_config(Arc::new(new))` on success (warm-reject
    // — keep the live table — on any failure, per §5.5).
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hcm::HCMStats;
    use envoy_config::RouteConfiguration;
    use std::sync::RwLock;
    use std::time::Duration;

    // A minimal `Arc<HCMConfig>` for the WatchTarget's `store` field. The
    // skeleton's no-op `reload` never touches it, so this only needs to be a
    // structurally-valid handle (the swappable route-table cell is present so
    // Task 4 can swap into it).
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

        let target = WatchTarget {
            path: path.clone(),
            route_config_name: "local_route".to_string(),
            store: minimal_store().await,
        };
        let cancel = CancellationToken::new();
        let watcher = RdsWatcher::spawn(vec![target], cancel.clone());
        assert_eq!(watcher.task_count(), 1, "one watch task per target");

        // Advance several poll intervals WITHOUT touching the file. No reload
        // fires (the no-op stub is reached only on an mtime CHANGE), and the
        // loop keeps running — i.e. it idles. We can only observe "no panic /
        // task still alive"; the stub has no side effect to assert on, which
        // is the point (lifecycle, not reload semantics — §5.2).
        for _ in 0..5 {
            tokio::time::advance(POLL_INTERVAL).await;
        }

        // Cancel must terminate the loop promptly (it exits the cancel branch,
        // not after the next tick). `shutdown()` joins the handle.
        let drained =
            tokio::time::timeout(Duration::from_secs(3), watcher.shutdown()).await;
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

        let target = WatchTarget {
            path,
            route_config_name: "local_route".to_string(),
            store: minimal_store().await,
        };
        let cancel = CancellationToken::new();
        let watcher = RdsWatcher::spawn(vec![target], cancel.clone());
        tokio::time::advance(POLL_INTERVAL).await;

        cancel.cancel();
        let drained =
            tokio::time::timeout(Duration::from_secs(3), watcher.shutdown()).await;
        assert!(drained.is_ok(), "loop exits on external cancel");
    }
}
