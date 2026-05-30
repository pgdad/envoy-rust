//! 14.1 D3 (parent-14 D3): per-endpoint outlier-detection state machine.
//!
//! The STATE lives in `envoy-cluster` (not a new crate) so `Cluster::pick()` reads it
//! cycle-free (parent SPEC §5.1). The TASK that *mutates* it via `record_response` lands
//! in **14.2 D4** (the H1+H2 router-arm response-receipt hooks). Initial state is
//! never-ejected (§6.2 item-3 confirmed: an outlier-detection endpoint is implicitly
//! healthy until threshold-crossing causes ejection; no warmup window).
//!
//! See 14.1 PLAN lock-in #11 for the `Relaxed`-ordering rationale (single-writer per
//! endpoint at 14.2; matches the 12.1 `EndpointHealth` precedent + the `cluster.rs`
//! `pick()` cursor).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// The per-endpoint outlier-detection stat handles. Grouped into a single struct (PLAN
/// lock-in #20) so `EndpointEjection::new` stays legible. All 6 handles are
/// **cluster-level shared** — each endpoint in the same cluster holds a clone of the
/// same `Arc<...>`, so transitions on any endpoint increment the cluster-wide
/// aggregate counter / gauge.
#[derive(Clone, Debug)]
pub struct EndpointEjectionStats {
    /// `cluster.<name>.outlier_detection.ejections_active` — gauge of currently-ejected
    /// endpoints in this cluster. `inc()` on `eject()`'s edge; `dec()` on `try_un_eject`'s
    /// edge. Single source of truth — NOT polled.
    pub ejections_active: Arc<envoy_stats::Gauge>,
    /// `cluster.<name>.outlier_detection.ejections_enforced_total` — counter of total
    /// ejections enforced (after cap check; per-detector sum modulo overflow).
    pub ejections_enforced_total: Arc<envoy_stats::Counter>,
    /// `cluster.<name>.outlier_detection.ejections_detected_consecutive_5xx` — counter
    /// of threshold-crossings on the consecutive_5xx detector, regardless of whether
    /// the cap permits enforcement (per ADR-0041 §6.2 item-2).
    pub ejections_detected_consecutive_5xx: Arc<envoy_stats::Counter>,
    /// Sibling of `ejections_detected_consecutive_5xx` — increments only when the
    /// threshold-crossing actually drives an ejection (cap honored).
    pub ejections_enforced_consecutive_5xx: Arc<envoy_stats::Counter>,
    /// `cluster.<name>.outlier_detection.ejections_detected_consecutive_gateway_failure`
    /// — counter of threshold-crossings on the consecutive_gateway_failure detector,
    /// regardless of cap.
    pub ejections_detected_consecutive_gateway_failure: Arc<envoy_stats::Counter>,
    /// Sibling of `ejections_detected_consecutive_gateway_failure` — increments only on
    /// cap-enforced ejection.
    pub ejections_enforced_consecutive_gateway_failure: Arc<envoy_stats::Counter>,
}

/// Which detector type caused a threshold crossing. Used by `Cluster::record_response`
/// to pick the `_enforced_*` counter to tick at ejection time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetectorType {
    Consecutive5xx,
    ConsecutiveGatewayFailure,
}

/// Result of `EndpointEjection::record_response`. Tracks which detectors crossed their
/// thresholds on this call. `Cluster::record_response` consumes the decision and
/// enforces the cluster-level `max_ejection_percent` cap.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EjectionDecision {
    pub crossed_5xx: bool,
    pub crossed_gateway_failure: bool,
}

impl EjectionDecision {
    /// True iff any detector crossed (caller proceeds to cap-check + eject).
    pub fn any(&self) -> bool {
        self.crossed_5xx || self.crossed_gateway_failure
    }
}

/// Per-endpoint outlier-detection state. Shared (`Arc`) so the 14.2 D4 response-receipt
/// hook can mutate it while `pick()` (D5) reads it.
#[derive(Debug)]
pub struct EndpointEjection {
    /// `true` when the endpoint is currently ejected (excluded from `pick()`). Initial
    /// `false` per §6.2 item-3 (no warmup window).
    ejected: AtomicBool,
    /// Count of consecutive 5xx responses since last reset (2xx/3xx/4xx OR un-eject).
    consecutive_5xx: AtomicU32,
    /// Count of consecutive 502/503/504 responses since last reset. Sibling counter
    /// to `consecutive_5xx`; both reset together.
    consecutive_gateway_failure: AtomicU32,
    /// The consecutive_5xx threshold (from the config). `0` disables the detector
    /// (defensive — the validator rejects 0, but `EndpointEjection` is robust).
    consecutive_5xx_threshold: u32,
    /// Sibling threshold for consecutive_gateway_failure.
    consecutive_gateway_failure_threshold: u32,
    /// 14.2 M4 discharge (lock-in #4): the per-endpoint serialization lock. The
    /// `Cluster::record_response` compound (record → cap-check → eject) and the 14.2 D7
    /// sweeper's per-endpoint un-eject each hold this guard for their full duration, so the
    /// `Relaxed` atomics above are mutated by exactly one writer at a time (the D4 hook fires
    /// from every in-flight request task and the D7 sweeper is a concurrent writer). The
    /// `Option<Instant>` payload doubles as the eject-timestamp the sweeper reads to apply
    /// `base_ejection_time` (§6.2 item-5). `pick()`'s read side stays lock-free
    /// (`is_ejected()` is a single `Relaxed` `AtomicBool` load). Set by
    /// `Cluster::record_response` right after `eject`; cleared by the sweeper right after
    /// `try_un_eject` — NOT inside `eject`/`try_un_eject` (which would self-deadlock with the
    /// externally-held guard), lock-in #5.
    pub(crate) ejected_at: std::sync::Mutex<Option<std::time::Instant>>,
    /// Per-cluster shared stat handles (see `EndpointEjectionStats`).
    stats: EndpointEjectionStats,
}

impl EndpointEjection {
    /// Construct an endpoint that starts never-ejected (§6.2 item-3). Both consecutive
    /// counters start at 0; the `ejections_active` gauge contributes 0 (no edge to
    /// trigger an `inc()`).
    pub fn new(
        consecutive_5xx_threshold: u32,
        consecutive_gateway_failure_threshold: u32,
        stats: EndpointEjectionStats,
    ) -> Self {
        Self {
            ejected: AtomicBool::new(false),
            consecutive_5xx: AtomicU32::new(0),
            consecutive_gateway_failure: AtomicU32::new(0),
            consecutive_5xx_threshold,
            consecutive_gateway_failure_threshold,
            ejected_at: std::sync::Mutex::new(None),
            stats,
        }
    }

    /// Whether the endpoint is currently ejected. Read by `Cluster::pick()` at every
    /// candidate-build pass (`Relaxed`-load; matches the cursor's ordering).
    pub fn is_ejected(&self) -> bool {
        self.ejected.load(Ordering::Relaxed)
    }

    /// 14.2 M5: borrow the per-cluster shared stat handles. Crate-internal + test-only so
    /// `OutlierDetectionState::stats()` (and the `cluster.rs` tie/enforced-counter tests)
    /// can read the `_enforced_*` counters without threading the `EndpointEjectionStats`
    /// through every test-helper return tuple. Production code reads these via the registry.
    #[cfg(test)]
    pub(crate) fn stats(&self) -> &EndpointEjectionStats {
        &self.stats
    }

    /// Record a response status. Ticks the per-detector counters per the classifier
    /// (per ADR-0041 §6.2 item-9 — purely status-driven, no `source` flag):
    ///   - 5xx (500-599): tick consecutive_5xx
    ///   - 502/503/504 specifically: ALSO tick consecutive_gateway_failure
    ///   - 2xx/3xx/4xx: reset both counters (§6.2 item-5)
    ///
    /// Increments the `ejections_detected_*` counters inline on threshold-crossings (per
    /// ADR-0041 §6.2 item-2: detected-ticks fire regardless of cap; the cluster-level
    /// caller decides whether the cap permits enforcement). Returns an
    /// `EjectionDecision` describing which detectors crossed; the caller (`Cluster::
    /// record_response`) enforces the cap and decides whether to call `eject()`.
    ///
    /// **Already-ejected endpoints skip ALL counter mutation** (Envoy semantic — an
    /// ejected endpoint doesn't accumulate state until `try_un_eject` resets it).
    pub fn record_response(&self, status: u16) -> EjectionDecision {
        if self.ejected.load(Ordering::Relaxed) {
            return EjectionDecision::default();
        }
        match status / 100 {
            5 => {
                let n5 = self.consecutive_5xx.fetch_add(1, Ordering::Relaxed) + 1;
                let crossed_5xx =
                    self.consecutive_5xx_threshold > 0 && n5 >= self.consecutive_5xx_threshold;
                let is_gateway_failure = matches!(status, 502..=504);
                let crossed_gf = if is_gateway_failure {
                    let ngf = self
                        .consecutive_gateway_failure
                        .fetch_add(1, Ordering::Relaxed)
                        + 1;
                    self.consecutive_gateway_failure_threshold > 0
                        && ngf >= self.consecutive_gateway_failure_threshold
                } else {
                    false
                };
                if crossed_5xx {
                    self.stats.ejections_detected_consecutive_5xx.inc();
                }
                if crossed_gf {
                    self.stats
                        .ejections_detected_consecutive_gateway_failure
                        .inc();
                }
                EjectionDecision {
                    crossed_5xx,
                    crossed_gateway_failure: crossed_gf,
                }
            }
            _ => {
                // 2xx/3xx/4xx: reset both counters per §6.2 item-5.
                self.consecutive_5xx.store(0, Ordering::Relaxed);
                self.consecutive_gateway_failure.store(0, Ordering::Relaxed);
                EjectionDecision::default()
            }
        }
    }

    /// Eject the endpoint. Called by `Cluster::record_response` when the
    /// `max_ejection_percent` cap permits. Idempotent — re-ejection of an already-
    /// ejected endpoint is a no-op (the state-machine's atomic swap ensures the gauge /
    /// counters tick exactly once per ejection edge).
    pub fn eject(&self, detector: DetectorType) {
        let was = self.ejected.swap(true, Ordering::Relaxed);
        if !was {
            self.stats.ejections_active.inc();
            self.stats.ejections_enforced_total.inc();
            match detector {
                DetectorType::Consecutive5xx => {
                    self.stats.ejections_enforced_consecutive_5xx.inc();
                }
                DetectorType::ConsecutiveGatewayFailure => {
                    self.stats
                        .ejections_enforced_consecutive_gateway_failure
                        .inc();
                }
            }
        }
    }

    /// Un-eject the endpoint. Called by the 14.2 D7 OutlierEjectionSweeper at sweep
    /// time (when `now - eject_time >= base_ejection_time`). At 14.1 this method has
    /// no production caller — tests exercise it directly. Returns `true` if the
    /// endpoint was actually ejected (and is now un-ejected); `false` if it was already
    /// not ejected (the sweeper's idempotent no-op case).
    ///
    /// Resets BOTH consecutive counters per §6.2 item-5 (a freshly un-ejected endpoint
    /// gets a fresh streak window).
    pub fn try_un_eject(&self) -> bool {
        let was = self.ejected.swap(false, Ordering::Relaxed);
        if was {
            self.consecutive_5xx.store(0, Ordering::Relaxed);
            self.consecutive_gateway_failure.store(0, Ordering::Relaxed);
            self.stats.ejections_active.dec();
        }
        was
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// Build a fresh EndpointEjection backed by a per-test StatsRegistry. Returns the
    /// EndpointEjection plus handles to inspect counter / gauge values from the test.
    fn mk(
        consecutive_5xx_threshold: u32,
        consecutive_gateway_failure_threshold: u32,
    ) -> (EndpointEjection, EndpointEjectionStats) {
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
        let ee = EndpointEjection::new(
            consecutive_5xx_threshold,
            consecutive_gateway_failure_threshold,
            EndpointEjectionStats {
                ejections_active: Arc::clone(&stats.ejections_active),
                ejections_enforced_total: Arc::clone(&stats.ejections_enforced_total),
                ejections_detected_consecutive_5xx: Arc::clone(
                    &stats.ejections_detected_consecutive_5xx,
                ),
                ejections_enforced_consecutive_5xx: Arc::clone(
                    &stats.ejections_enforced_consecutive_5xx,
                ),
                ejections_detected_consecutive_gateway_failure: Arc::clone(
                    &stats.ejections_detected_consecutive_gateway_failure,
                ),
                ejections_enforced_consecutive_gateway_failure: Arc::clone(
                    &stats.ejections_enforced_consecutive_gateway_failure,
                ),
            },
        );
        (ee, stats)
    }

    #[test]
    fn starts_never_ejected_with_zero_active_gauge() {
        let (ee, stats) = mk(5, 5);
        assert!(
            !ee.is_ejected(),
            "§6.2 item-3: initial state is never-ejected"
        );
        assert_eq!(stats.ejections_active.value(), 0);
    }

    #[test]
    fn record_response_5xx_ticks_consecutive_5xx_only_on_500() {
        let (ee, stats) = mk(3, 3);
        // Status 500 is 5xx but NOT 502/503/504 → only consecutive_5xx ticks.
        let d1 = ee.record_response(500);
        assert!(!d1.crossed_5xx);
        let d2 = ee.record_response(500);
        assert!(!d2.crossed_5xx);
        let d3 = ee.record_response(500);
        // Threshold met on the third 500.
        assert!(d3.crossed_5xx);
        assert!(!d3.crossed_gateway_failure);
        assert_eq!(stats.ejections_detected_consecutive_5xx.value(), 1);
        assert_eq!(
            stats.ejections_detected_consecutive_gateway_failure.value(),
            0
        );
    }

    #[test]
    fn record_response_503_ticks_both_detectors_per_adr_0041_item_9() {
        let (ee, stats) = mk(2, 2);
        // 503 is BOTH 5xx and gateway-failure — both counters tick.
        let d1 = ee.record_response(503);
        assert!(!d1.crossed_5xx);
        assert!(!d1.crossed_gateway_failure);
        let d2 = ee.record_response(503);
        assert!(d2.crossed_5xx);
        assert!(d2.crossed_gateway_failure);
        assert_eq!(stats.ejections_detected_consecutive_5xx.value(), 1);
        assert_eq!(
            stats.ejections_detected_consecutive_gateway_failure.value(),
            1
        );
    }

    #[test]
    fn record_response_502_ticks_both_detectors() {
        let (ee, _stats) = mk(1, 1);
        let d = ee.record_response(502);
        assert!(d.crossed_5xx);
        assert!(d.crossed_gateway_failure);
    }

    #[test]
    fn record_response_504_ticks_both_detectors() {
        let (ee, _stats) = mk(1, 1);
        let d = ee.record_response(504);
        assert!(d.crossed_5xx);
        assert!(d.crossed_gateway_failure);
    }

    #[test]
    fn record_response_2xx_3xx_4xx_resets_both_counters() {
        let (ee, _stats) = mk(3, 3);
        ee.record_response(500); // consecutive_5xx = 1
        ee.record_response(503); // consecutive_5xx = 2, consecutive_gateway_failure = 1
        // 200 resets BOTH counters (§6.2 item-5).
        let d = ee.record_response(200);
        assert!(!d.crossed_5xx);
        assert!(!d.crossed_gateway_failure);
        // After reset, two more 500s alone shouldn't cross (threshold 3):
        ee.record_response(500);
        ee.record_response(500);
        let d2 = ee.record_response(404); // 4xx also resets
        assert!(!d2.crossed_5xx);
    }

    #[test]
    fn record_response_skips_when_already_ejected() {
        let (ee, stats) = mk(1, 1);
        let d = ee.record_response(500);
        assert!(d.crossed_5xx);
        ee.eject(DetectorType::Consecutive5xx);
        // Already ejected — subsequent calls return NoChange (Envoy semantic: ejected
        // endpoints don't accumulate counters until un-ejected).
        let d2 = ee.record_response(500);
        assert!(!d2.crossed_5xx);
        // ejections_detected_consecutive_5xx didn't tick again.
        assert_eq!(stats.ejections_detected_consecutive_5xx.value(), 1);
    }

    #[test]
    fn eject_increments_active_and_enforced_counters() {
        let (ee, stats) = mk(1, 1);
        ee.record_response(500);
        ee.eject(DetectorType::Consecutive5xx);
        assert!(ee.is_ejected());
        assert_eq!(stats.ejections_active.value(), 1);
        assert_eq!(stats.ejections_enforced_total.value(), 1);
        assert_eq!(stats.ejections_enforced_consecutive_5xx.value(), 1);
        assert_eq!(
            stats.ejections_enforced_consecutive_gateway_failure.value(),
            0
        );
    }

    #[test]
    fn eject_for_gateway_failure_increments_the_gateway_counter() {
        let (ee, stats) = mk(1, 1);
        ee.record_response(503);
        ee.eject(DetectorType::ConsecutiveGatewayFailure);
        assert_eq!(stats.ejections_active.value(), 1);
        assert_eq!(stats.ejections_enforced_total.value(), 1);
        assert_eq!(stats.ejections_enforced_consecutive_5xx.value(), 0);
        assert_eq!(
            stats.ejections_enforced_consecutive_gateway_failure.value(),
            1
        );
    }

    #[test]
    fn eject_is_idempotent_no_double_increment() {
        let (ee, stats) = mk(1, 1);
        ee.record_response(500);
        ee.eject(DetectorType::Consecutive5xx);
        ee.eject(DetectorType::Consecutive5xx);
        ee.eject(DetectorType::Consecutive5xx);
        assert_eq!(stats.ejections_active.value(), 1);
        assert_eq!(stats.ejections_enforced_total.value(), 1);
    }

    #[test]
    fn try_un_eject_decrements_active_and_resets_counters() {
        let (ee, stats) = mk(3, 3);
        ee.record_response(500);
        ee.record_response(500);
        ee.record_response(500);
        ee.eject(DetectorType::Consecutive5xx);
        assert_eq!(stats.ejections_active.value(), 1);
        let did = ee.try_un_eject();
        assert!(did);
        assert!(!ee.is_ejected());
        assert_eq!(stats.ejections_active.value(), 0);
        // §6.2 item-5: counters reset on un-eject. Next 2 500s alone don't re-cross
        // (the counter was reset to 0, so threshold 3 requires 3 more):
        ee.record_response(500);
        ee.record_response(500);
        let d = ee.record_response(500);
        // 3 fresh 500s after un-eject → threshold crossed again.
        assert!(d.crossed_5xx);
    }

    #[test]
    fn try_un_eject_when_not_ejected_returns_false() {
        let (ee, stats) = mk(1, 1);
        let did = ee.try_un_eject();
        assert!(!did);
        assert_eq!(stats.ejections_active.value(), 0);
    }

    #[test]
    fn threshold_zero_means_disabled_detector() {
        // Per the validator (Task 2): the schema rejects `0` thresholds, but the
        // state machine has its own defense — threshold 0 should NOT spuriously
        // trigger ejection on the first response.
        let (ee, _stats) = mk(0, 0);
        let d = ee.record_response(500);
        assert!(!d.crossed_5xx, "threshold 0 must NOT trigger");
        assert!(!d.crossed_gateway_failure);
    }

    #[test]
    fn ejected_at_is_none_until_eject_and_set_after() {
        // 14.2 M4 / lock-in #5: the `ejected_at` payload is `None` for a never-ejected
        // endpoint and becomes `Some(Instant)` once the lock-SITE (here, the test standing in
        // for `Cluster::record_response`) stamps it after `eject`. `eject` itself does NOT
        // touch `ejected_at` (it would self-deadlock with the externally-held guard).
        let (ee, _stats) = mk(3, 3);
        assert!(
            ee.ejected_at.lock().unwrap().is_none(),
            "never-ejected ⇒ no timestamp"
        );
        for _ in 0..3 {
            let _ = ee.record_response(500);
        }
        ee.eject(DetectorType::Consecutive5xx);
        *ee.ejected_at.lock().unwrap() = Some(std::time::Instant::now());
        assert!(
            ee.ejected_at.lock().unwrap().is_some(),
            "ejected ⇒ timestamp set"
        );
        assert!(ee.is_ejected());
    }
}
