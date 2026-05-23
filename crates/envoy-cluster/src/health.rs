//! 12.1 (parent-12 D3): per-endpoint active-health-check state machine.
//!
//! The STATE lives in `envoy-cluster` (not a new crate) so `Cluster::pick()`
//! reads it cycle-free (parent SPEC §5.1). The TASK that *mutates* it via
//! `record_success`/`record_failure` lands in 12.2's `envoy-health` crate.
//! Initial state is Unhealthy (§6.2 item-1): an active-HC endpoint is not
//! healthy until the first `healthy_threshold` consecutive successes.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};

const UNHEALTHY: u8 = 0;
const HEALTHY: u8 = 1;

/// Per-endpoint active-health-check state. Shared (`Arc`) so the 12.2 probe
/// task can mutate it while `pick()` (D5) reads it. Single-writer per endpoint
/// (one probe task per (cluster, endpoint)), so `record_*` never race each
/// other for a given endpoint; `pick()` reads `is_healthy()` concurrently with
/// `Relaxed` loads (no happens-before dependency — the `cluster.rs` `pick()`
/// cursor `Relaxed` precedent).
///
/// **API-boundary contract (12.1 REVIEW M2; closed at 12.2):** the live
/// production writer of every `EndpointHealth` is the `envoy-health::Scheduler`
/// probe task spawned per (cluster, endpoint); callers obtaining an
/// `Arc<EndpointHealth>` from `ClusterHandle::health_probe_targets()` (12.2)
/// MUST NOT call `record_success`/`record_failure` themselves and MUST NOT
/// hand the `Arc` to additional writer tasks. Violating this contract makes
/// the `Relaxed`-ordering soundness assumption invalid (concurrent
/// load-modify-store races on `state` may double-increment/decrement the
/// membership gauge). Tests + the 12.2 review verify the contract at the
/// scheduler boundary.
#[derive(Debug)]
pub struct EndpointHealth {
    state: AtomicU8,
    consecutive_success: AtomicU32,
    consecutive_failure: AtomicU32,
    healthy_threshold: u32,
    unhealthy_threshold: u32,
    /// Shared `cluster.<name>.membership_healthy` gauge; `inc()` on a flip to
    /// Healthy, `dec()` on a flip to Unhealthy (the single source of truth for
    /// the healthy-endpoint count — NOT polled, the 08.2 inline pattern).
    membership_healthy: Arc<envoy_stats::Gauge>,
}

impl EndpointHealth {
    /// Construct an endpoint that starts Unhealthy (gauge contributes 0).
    pub fn new(
        healthy_threshold: u32,
        unhealthy_threshold: u32,
        membership_healthy: Arc<envoy_stats::Gauge>,
    ) -> Self {
        Self {
            state: AtomicU8::new(UNHEALTHY),
            consecutive_success: AtomicU32::new(0),
            consecutive_failure: AtomicU32::new(0),
            healthy_threshold,
            unhealthy_threshold,
            membership_healthy,
        }
    }

    /// Record a probe success. Resets the failure counter; transitions
    /// Unhealthy → Healthy after `healthy_threshold` consecutive successes
    /// (incrementing the membership gauge on that edge).
    pub fn record_success(&self) {
        self.consecutive_failure.store(0, Ordering::Relaxed);
        let n = self.consecutive_success.fetch_add(1, Ordering::Relaxed) + 1;
        if self.state.load(Ordering::Relaxed) == UNHEALTHY && n >= self.healthy_threshold {
            self.state.store(HEALTHY, Ordering::Relaxed);
            self.membership_healthy.inc();
        }
    }

    /// Record a probe failure. Resets the success counter; transitions
    /// Healthy → Unhealthy after `unhealthy_threshold` consecutive failures
    /// (decrementing the membership gauge on that edge).
    pub fn record_failure(&self) {
        self.consecutive_success.store(0, Ordering::Relaxed);
        let n = self.consecutive_failure.fetch_add(1, Ordering::Relaxed) + 1;
        if self.state.load(Ordering::Relaxed) == HEALTHY && n >= self.unhealthy_threshold {
            self.state.store(UNHEALTHY, Ordering::Relaxed);
            self.membership_healthy.dec();
        }
    }

    /// Whether the endpoint is currently Healthy (read by `pick()`).
    pub fn is_healthy(&self) -> bool {
        self.state.load(Ordering::Relaxed) == HEALTHY
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn mk(
        healthy_threshold: u32,
        unhealthy_threshold: u32,
    ) -> (EndpointHealth, Arc<envoy_stats::Gauge>) {
        let reg = envoy_stats::StatsRegistry::new();
        let gauge = reg
            .register_gauge("cluster.t.membership_healthy")
            .expect("gauge");
        let eh = EndpointHealth::new(healthy_threshold, unhealthy_threshold, Arc::clone(&gauge));
        (eh, gauge)
    }

    #[test]
    fn starts_unhealthy_with_gauge_zero() {
        let (eh, gauge) = mk(1, 1);
        assert!(!eh.is_healthy());
        assert_eq!(gauge.value(), 0);
    }

    #[test]
    fn flips_healthy_after_healthy_threshold_successes() {
        let (eh, gauge) = mk(2, 1);
        eh.record_success();
        assert!(!eh.is_healthy(), "1 < threshold 2");
        assert_eq!(gauge.value(), 0);
        eh.record_success();
        assert!(eh.is_healthy(), "2 == threshold 2");
        assert_eq!(gauge.value(), 1, "gauge inc on flip to healthy");
    }

    #[test]
    fn flips_unhealthy_after_unhealthy_threshold_failures() {
        let (eh, gauge) = mk(1, 2);
        eh.record_success(); // -> Healthy
        assert!(eh.is_healthy());
        assert_eq!(gauge.value(), 1);
        eh.record_failure();
        assert!(eh.is_healthy(), "1 failure < threshold 2");
        assert_eq!(gauge.value(), 1);
        eh.record_failure();
        assert!(!eh.is_healthy(), "2 failures == threshold 2");
        assert_eq!(gauge.value(), 0, "gauge dec on flip to unhealthy");
    }

    #[test]
    fn opposite_result_resets_the_consecutive_counter() {
        let (eh, _gauge) = mk(3, 3);
        eh.record_success();
        eh.record_success(); // 2 successes
        eh.record_failure(); // resets success counter to 0
        eh.record_success();
        eh.record_success(); // only 2 again
        assert!(
            !eh.is_healthy(),
            "counter was reset by the intervening failure"
        );
    }

    #[test]
    fn repeated_success_while_healthy_does_not_double_increment_gauge() {
        let (eh, gauge) = mk(1, 1);
        eh.record_success(); // flip -> Healthy, gauge 1
        eh.record_success(); // already healthy; no further inc
        eh.record_success();
        assert_eq!(gauge.value(), 1, "gauge increments only on the edge");
    }
}
