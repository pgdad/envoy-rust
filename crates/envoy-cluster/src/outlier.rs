//! 14.2 D7: passive outlier-detection ejection sweeper — the FOURTH periodic-background
//! primitive (after the 12.2 active-HC scheduler + the 13.1 H1 pool idle sweeper + the 13.2
//! H2 pool idle sweeper). Reverses ejections: the D4 response-receipt hooks
//! (`Cluster::record_response`) eject endpoints on consecutive 5xx; this sweeper un-ejects
//! them once `base_ejection_time` has elapsed (per §6.2 item-5).
//!
//! Mirrors the established CancellationToken cancellation discipline verbatim: one shared
//! `tokio_util::sync::CancellationToken`, a `tokio::time::interval` loop, and a
//! `pub async fn shutdown(self)` that cancels + joins with no leaked tasks. `OutlierManager`
//! is an EXTERNAL sibling registry (lock-in #7) — it does NOT live on `ClusterManager`,
//! mirroring `H1PoolManager` / `H2PoolManager` / `envoy_health::Scheduler`.

use crate::EndpointEjection;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// 14.2 D7: the per-cluster ejection sweeper. Spawns one background task that, at each
/// `interval` tick, un-ejects every endpoint whose ejection has aged past
/// `base_ejection_time`. The struct is just `{ cancel, join }` (lock-in #7) — identical
/// shape to the H1/H2 idle sweepers + the `envoy-health` scheduler.
pub struct OutlierEjectionSweeper {
    cancel: CancellationToken,
    join: tokio::task::JoinHandle<()>,
}

impl OutlierEjectionSweeper {
    /// Spawn the sweep task. `endpoints` are the per-endpoint `EndpointEjection` handles
    /// (aligned by index with the cluster's endpoints). The task runs until `cancel`
    /// fires; `shutdown` is the clean-exit path.
    pub fn spawn(
        cluster_name: String,
        endpoints: Vec<Arc<EndpointEjection>>,
        base_ejection_time: Duration,
        interval: Duration,
        cancel: CancellationToken,
    ) -> Self {
        let task_cancel = cancel.clone();
        let join = tokio::spawn(async move {
            // Clamp to >=1ms: `tokio::time::interval(Duration::ZERO)` panics. Mirrors the
            // H1/H2 idle sweepers' defensive clamp (13.1 REVIEW Cluster A I2). The
            // validator rejects a zero `interval`, but `spawn` accepts any `Duration`.
            let period = interval.max(Duration::from_millis(1));
            let mut tick = tokio::time::interval(period);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = task_cancel.cancelled() => return,
                    _ = tick.tick() => sweep_once(&cluster_name, &endpoints, base_ejection_time),
                }
            }
        });
        Self { cancel, join }
    }

    /// Cancel the sweep task + await its join. Consumes `self`. Mirrors
    /// `envoy_health::Scheduler::shutdown` / the H1/H2 pool managers' `shutdown`.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.join.await;
    }
}

/// One sweep pass: un-eject every endpoint aged past `base`. Holds `ep.ejected_at.lock()`
/// across the check-and-un-eject (lock-in #4/#5) so it cannot interleave with a concurrent
/// `Cluster::record_response` compound. `try_un_eject` does the full §6.2 item-5 un-eject
/// (clears the `ejected` bool, resets both consecutive counters, decrements the
/// `ejections_active` gauge); the sweeper composes by clearing the `ejected_at` timestamp
/// AFTER it, under the SAME held guard.
fn sweep_once(cluster_name: &str, endpoints: &[Arc<EndpointEjection>], base: Duration) {
    for (i, ep) in endpoints.iter().enumerate() {
        let mut at = ep.ejected_at.lock().unwrap();
        if let Some(t) = *at
            && t.elapsed() >= base
        {
            // `try_un_eject` already resets counters + decrements the gauge (14.1
            // semantics) — do NOT duplicate those here (would double-decrement).
            ep.try_un_eject();
            // lock-in #5: clear the timestamp under the held guard, NOT inside
            // `try_un_eject` (which would self-deadlock with this externally-held guard).
            *at = None;
            tracing::debug!(
                cluster = %cluster_name,
                endpoint_idx = i,
                "outlier: un-ejected endpoint after base_ejection_time"
            );
        }
    }
}

/// 14.2 D7 (lock-in #7): per-bootstrap registry of `OutlierEjectionSweeper`s, one per
/// outlier-detection-configured cluster. An EXTERNAL sibling registry — mirrors
/// `H1PoolManager` / `H2PoolManager` / `envoy_health::Scheduler` verbatim. Wired into
/// `envoy-bin` at 14.2 Task 5 (not here). `Debug` is hand-rolled because
/// `OutlierEjectionSweeper` carries a `JoinHandle` (no `Debug`).
pub struct OutlierManager {
    sweepers: Vec<OutlierEjectionSweeper>,
}

impl std::fmt::Debug for OutlierManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OutlierManager")
            .field("sweepers", &self.sweepers.len())
            .finish()
    }
}

impl OutlierManager {
    /// Build the registry from the constructed `ClusterManager`. Walks every cluster
    /// (`ClusterManager::clusters()` — the same iterator the `envoy-health` scheduler
    /// walks); for each cluster with outlier-detection configured, spawns one sweeper
    /// reading the runtime `base_ejection_time` / `interval` (lock-in #6 — NOT re-parsing
    /// the bootstrap). Clusters without outlier detection spawn no task (§5.3 inert).
    pub fn for_bootstrap(cluster_mgr: &crate::ClusterManager, cancel: CancellationToken) -> Self {
        let mut sweepers = Vec::new();
        for handle in cluster_mgr.clusters() {
            if let Some(od) = handle.inner_outlier_detection_state() {
                sweepers.push(OutlierEjectionSweeper::spawn(
                    handle.name().to_string(),
                    od.endpoints.clone(),
                    od.base_ejection_time,
                    od.interval,
                    cancel.clone(),
                ));
            }
        }
        Self { sweepers }
    }

    /// Cancel + join every sweeper. Consumes `self`. Mirrors the sibling managers'
    /// `shutdown`.
    pub async fn shutdown(self) {
        for s in self.sweepers {
            s.shutdown().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DetectorType, EndpointEjectionStats};
    use std::time::Duration;

    /// Build an `Arc<EndpointEjection>` backed by a fresh per-test `StatsRegistry`,
    /// mirroring the 14.1 `ejection.rs` / Task-1 `cluster.rs` test-fixture shape (every
    /// stat handle registered against the registry). Returns the ejection handle plus the
    /// stats bundle so the test can assert the `ejections_active` gauge.
    fn test_ejection_handle() -> (Arc<EndpointEjection>, EndpointEjectionStats) {
        let registry = envoy_stats::StatsRegistry::new();
        let stats = EndpointEjectionStats {
            ejections_active: registry
                .register_gauge("cluster.t.outlier_detection.ejections_active")
                .unwrap(),
            ejections_enforced_total: registry
                .register_counter("cluster.t.outlier_detection.ejections_enforced_total")
                .unwrap(),
            ejections_detected_consecutive_5xx: registry
                .register_counter("cluster.t.outlier_detection.ejections_detected_consecutive_5xx")
                .unwrap(),
            ejections_enforced_consecutive_5xx: registry
                .register_counter("cluster.t.outlier_detection.ejections_enforced_consecutive_5xx")
                .unwrap(),
            ejections_detected_consecutive_gateway_failure: registry
                .register_counter(
                    "cluster.t.outlier_detection.ejections_detected_consecutive_gateway_failure",
                )
                .unwrap(),
            ejections_enforced_consecutive_gateway_failure: registry
                .register_counter(
                    "cluster.t.outlier_detection.ejections_enforced_consecutive_gateway_failure",
                )
                .unwrap(),
        };
        let ep = Arc::new(EndpointEjection::new(5, 5, stats.clone()));
        (ep, stats)
    }

    /// 14.2 D7 core behavior: after `>= base_ejection_time` of elapsed WALL time AND at
    /// least one interval tick, the ejected endpoint is un-ejected, `ejected_at` is
    /// cleared, and the `ejections_active` gauge is decremented.
    ///
    /// Virtual-clock note: `std::time::Instant` is WALL-clock and does NOT advance with
    /// `tokio::time::advance`, so a `start_paused` runtime + `advance` would never make
    /// `Instant::elapsed()` cross `base_ejection_time`. This test therefore uses a real
    /// (multi_thread) runtime with small real Durations so wall-clock elapsed genuinely
    /// crosses the threshold while staying fast + deterministic.
    #[tokio::test(flavor = "multi_thread")]
    async fn sweeper_un_ejects_after_base_ejection_time() {
        let (ep, stats) = test_ejection_handle();
        ep.eject(DetectorType::Consecutive5xx);
        *ep.ejected_at.lock().unwrap() = Some(std::time::Instant::now());
        assert!(ep.is_ejected());
        assert_eq!(stats.ejections_active.value(), 1);

        let cancel = CancellationToken::new();
        // base_ejection_time = 20ms gives an unambiguous wall-clock threshold; interval = 5ms
        // guarantees several ticks fire after the threshold within the poll budget below.
        let sweeper = OutlierEjectionSweeper::spawn(
            "c1".to_string(),
            vec![ep.clone()],
            Duration::from_millis(20), // base_ejection_time
            Duration::from_millis(5),  // interval
            cancel.clone(),
        );

        // Poll until the sweeper un-ejects (real wall-clock elapsed must cross 20ms AND a
        // 5ms tick must fire afterwards). Generous budget (200 x 10ms = 2s) so the assertion
        // is robust under CI load while a genuine regression still fails reasonably fast.
        let mut un_ejected = false;
        for _ in 0..200 {
            tokio::time::sleep(Duration::from_millis(10)).await;
            if !ep.is_ejected() {
                un_ejected = true;
                break;
            }
        }
        assert!(un_ejected, "endpoint un-ejected after base_ejection_time");
        assert!(
            ep.ejected_at.lock().unwrap().is_none(),
            "timestamp cleared on un-eject"
        );
        assert_eq!(
            stats.ejections_active.value(),
            0,
            "ejections_active gauge decremented on un-eject"
        );
        sweeper.shutdown().await;
    }

    /// 14.2 D7 cancellation discipline: `shutdown` cancels + joins cleanly with no leaked
    /// task, even with an empty endpoint list (the spawn-and-immediately-shutdown path).
    #[tokio::test(flavor = "multi_thread")]
    async fn sweeper_shutdown_joins_cleanly() {
        let cancel = CancellationToken::new();
        let sweeper = OutlierEjectionSweeper::spawn(
            "c1".to_string(),
            vec![],
            Duration::from_secs(5),
            Duration::from_secs(1),
            cancel.clone(),
        );
        sweeper.shutdown().await;
    }

    /// An endpoint ejected for LESS than `base_ejection_time` must NOT be un-ejected by a
    /// sweep tick — the negative of the core behavior, proving the elapsed-check gates the
    /// un-eject rather than un-ejecting on every tick unconditionally.
    #[tokio::test(flavor = "multi_thread")]
    async fn sweeper_does_not_un_eject_before_base_ejection_time() {
        let (ep, stats) = test_ejection_handle();
        ep.eject(DetectorType::Consecutive5xx);
        *ep.ejected_at.lock().unwrap() = Some(std::time::Instant::now());

        let cancel = CancellationToken::new();
        // base_ejection_time far in the future; interval tiny so ticks DO fire — but every
        // tick must see `elapsed < base` and leave the endpoint ejected.
        let sweeper = OutlierEjectionSweeper::spawn(
            "c1".to_string(),
            vec![ep.clone()],
            Duration::from_secs(3600),
            Duration::from_millis(1),
            cancel.clone(),
        );
        // Let several sweep ticks fire.
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(
            ep.is_ejected(),
            "endpoint must stay ejected before base_ejection_time elapses"
        );
        assert!(
            ep.ejected_at.lock().unwrap().is_some(),
            "timestamp retained"
        );
        assert_eq!(stats.ejections_active.value(), 1);
        sweeper.shutdown().await;
    }
}
