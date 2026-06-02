//! 17 D3: cluster-scoped circuit-breaker budget primitives (ADR-0046 §5.4).
//! Budget state lives on the CLUSTER (not the per-protocol pools): max_retries /
//! max_requests are cluster-wide concepts spanning both protocol pools.
//! Stat side-effects (overflow counters + gauges) live INSIDE the acquire/release
//! paths — single source of truth (§5.3); H1/H2 callers never touch them directly.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use envoy_stats::StatsRegistry;

// ── BudgetState ──────────────────────────────────────────────────────────────

/// Cluster-scoped circuit-breaker budget (ADR-0046 §5.4). Holds the concurrent-
/// active counters for in-flight retries and requests, the overflow counters that
/// tick on failed acquisition, the momentary open-state gauges, and (optionally)
/// the `remaining_*` gauges.
///
/// Callers acquire a RAII guard via [`BudgetState::try_acquire_retry`] /
/// [`BudgetState::try_acquire_request`]. Dropping the guard releases the slot and
/// updates all gauges — the single-source-of-truth discipline (§5.3).
///
/// `BudgetState::new` takes already-resolved cap values; the L5 default resolution
/// (3 / 1024) is the caller's responsibility (Task 3 `from_bootstrap` wiring).
#[derive(Debug)]
pub struct BudgetState {
    max_retries: u32,
    max_requests: u32,
    active_retries: AtomicI64,
    active_requests: AtomicI64,
    /// Overflow counter — ticks inside failed `try_acquire_retry` (§5.3 / L3 / L7).
    rq_retry_overflow: Arc<envoy_stats::Counter>,
    /// Overflow counter — ticks inside failed `try_acquire_request` (§5.3 / L3).
    rq_pending_overflow: Arc<envoy_stats::Counter>,
    /// Momentary breaker gauge (L4 / ADR-0047): 1 iff active > 0 AND active >= max.
    rq_retry_open: Arc<envoy_stats::Gauge>,
    /// Momentary breaker gauge (L4 / ADR-0047): 1 iff active > 0 AND active >= max.
    rq_open: Arc<envoy_stats::Gauge>,
    /// `remaining_retries` gauge — registered ONLY when `track_remaining: true` (L8).
    remaining_retries: Option<Arc<envoy_stats::Gauge>>,
    /// `remaining_rq` gauge — registered ONLY when `track_remaining: true` (L8).
    remaining_rq: Option<Arc<envoy_stats::Gauge>>,
}

impl BudgetState {
    /// Construct a new `BudgetState`, registering all stats against `registry`.
    ///
    /// - `max_retries` / `max_requests`: already-resolved cap values (L5 — the
    ///   caller resolves defaults 3 / 1024 from the config).
    /// - `track_remaining`: when `true`, registers `remaining_retries` and
    ///   `remaining_rq` gauges initialised to their respective cap values (L8).
    ///   When `false`, those names are absent from the registry snapshot.
    /// - `cluster_name`: used to build Envoy-shaped stat names
    ///   `cluster.<name>.*`.
    ///
    /// Returns `Err` if any registry registration fails (e.g., name conflict).
    pub fn new(
        max_retries: u32,
        max_requests: u32,
        track_remaining: bool,
        registry: &StatsRegistry,
        cluster_name: &str,
    ) -> Result<Arc<Self>, envoy_stats::StatsError> {
        let prefix = format!("cluster.{cluster_name}");
        let cb_prefix = format!("cluster.{cluster_name}.circuit_breakers.default");

        let rq_retry_overflow =
            registry.register_counter(&format!("{prefix}.upstream_rq_retry_overflow"))?;
        let rq_pending_overflow =
            registry.register_counter(&format!("{prefix}.upstream_rq_pending_overflow"))?;
        let rq_retry_open = registry.register_gauge(&format!("{cb_prefix}.rq_retry_open"))?;
        let rq_open = registry.register_gauge(&format!("{cb_prefix}.rq_open"))?;

        let remaining_retries = if track_remaining {
            let g = registry.register_gauge(&format!("{cb_prefix}.remaining_retries"))?;
            g.set(max_retries as i64);
            Some(g)
        } else {
            None
        };
        let remaining_rq = if track_remaining {
            let g = registry.register_gauge(&format!("{cb_prefix}.remaining_rq"))?;
            g.set(max_requests as i64);
            Some(g)
        } else {
            None
        };

        Ok(Arc::new(Self {
            max_retries,
            max_requests,
            active_retries: AtomicI64::new(0),
            active_requests: AtomicI64::new(0),
            rq_retry_overflow,
            rq_pending_overflow,
            rq_retry_open,
            rq_open,
            remaining_retries,
            remaining_rq,
        }))
    }

    /// Attempt to acquire one retry budget slot.
    ///
    /// Returns a [`RetryBudgetGuard`] on success; the guard's `Drop` releases
    /// the slot and re-syncs all gauges. Returns `None` when the active retry
    /// count is at or above `max_retries`, incrementing
    /// `upstream_rq_retry_overflow` (L3/L7). A `max_retries == 0` cap is the
    /// always-open breaker — acquisition always fails (L1).
    pub fn try_acquire_retry(self: &Arc<Self>) -> Option<RetryBudgetGuard> {
        loop {
            let cur = self.active_retries.load(Ordering::Acquire);
            if cur >= self.max_retries as i64 {
                self.rq_retry_overflow.inc();
                self.update_retry_gauges(cur);
                return None;
            }
            match self.active_retries.compare_exchange(
                cur,
                cur + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    self.update_retry_gauges(cur + 1);
                    return Some(RetryBudgetGuard {
                        budget: Arc::clone(self),
                    });
                }
                Err(_) => {
                    // CAS contended; retry the loop.
                    continue;
                }
            }
        }
    }

    /// Attempt to acquire one request budget slot.
    ///
    /// Returns a [`RequestBudgetGuard`] on success; the guard's `Drop` releases
    /// the slot and re-syncs all gauges. Returns `None` when the active request
    /// count is at or above `max_requests`, incrementing
    /// `upstream_rq_pending_overflow` (L3). A `max_requests == 0` cap is the
    /// always-open breaker — acquisition always fails (L2).
    pub fn try_acquire_request(self: &Arc<Self>) -> Option<RequestBudgetGuard> {
        loop {
            let cur = self.active_requests.load(Ordering::Acquire);
            if cur >= self.max_requests as i64 {
                self.rq_pending_overflow.inc();
                self.update_request_gauges(cur);
                return None;
            }
            match self.active_requests.compare_exchange(
                cur,
                cur + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    self.update_request_gauges(cur + 1);
                    return Some(RequestBudgetGuard {
                        budget: Arc::clone(self),
                    });
                }
                Err(_) => {
                    continue;
                }
            }
        }
    }

    // ── private gauge helpers ────────────────────────────────────────────────

    /// Update `rq_retry_open` and (if present) `remaining_retries` to reflect
    /// `active` in-flight retries.
    ///
    /// L4 momentary semantic: open iff `active > 0 AND active >= max`.
    ///
    /// Note: the `set()` here is unordered with respect to the CAS/fetch_sub of
    /// concurrent acquires/releases, so the gauge can be transiently stale during
    /// concurrent operations; every subsequent acquire or release corrects it
    /// (ADR-0047 L4 "momentary" semantic — never persistently stale).
    fn update_retry_gauges(&self, active: i64) {
        let open = if active > 0 && active >= self.max_retries as i64 {
            1
        } else {
            0
        };
        self.rq_retry_open.set(open);
        if let Some(g) = &self.remaining_retries {
            g.set((self.max_retries as i64 - active).max(0));
        }
    }

    /// Update `rq_open` and (if present) `remaining_rq` to reflect `active`
    /// in-flight requests.
    ///
    /// L4 momentary semantic: open iff `active > 0 AND active >= max`.
    ///
    /// Note: the `set()` here is unordered with respect to the CAS/fetch_sub of
    /// concurrent acquires/releases, so the gauge can be transiently stale during
    /// concurrent operations; every subsequent acquire or release corrects it
    /// (ADR-0047 L4 "momentary" semantic — never persistently stale).
    fn update_request_gauges(&self, active: i64) {
        let open = if active > 0 && active >= self.max_requests as i64 {
            1
        } else {
            0
        };
        self.rq_open.set(open);
        if let Some(g) = &self.remaining_rq {
            g.set((self.max_requests as i64 - active).max(0));
        }
    }
}

// ── RetryBudgetGuard ─────────────────────────────────────────────────────────

/// RAII guard for a retry budget slot (mirrors the 13.x `PoolGuard` / `ConnGaugeGuard`
/// discipline). Dropping the guard decrements `active_retries` and re-syncs all
/// retry-related gauges. Single source of truth: callers never touch gauges directly.
pub struct RetryBudgetGuard {
    budget: Arc<BudgetState>,
}

impl std::fmt::Debug for RetryBudgetGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RetryBudgetGuard").finish_non_exhaustive()
    }
}

impl Drop for RetryBudgetGuard {
    fn drop(&mut self) {
        let prev = self.budget.active_retries.fetch_sub(1, Ordering::AcqRel);
        // `prev` was the value BEFORE the decrement; active after = prev - 1.
        self.budget.update_retry_gauges(prev - 1);
    }
}

// ── RequestBudgetGuard ───────────────────────────────────────────────────────

/// RAII guard for a request budget slot. Dropping the guard decrements
/// `active_requests` and re-syncs all request-related gauges.
pub struct RequestBudgetGuard {
    budget: Arc<BudgetState>,
}

impl std::fmt::Debug for RequestBudgetGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RequestBudgetGuard").finish_non_exhaustive()
    }
}

impl Drop for RequestBudgetGuard {
    fn drop(&mut self) {
        let prev = self.budget.active_requests.fetch_sub(1, Ordering::AcqRel);
        self.budget.update_request_gauges(prev - 1);
    }
}

// ── BudgetAcquisition ────────────────────────────────────────────────────────

/// Three-state result of a `try_acquire_retry` / `try_acquire_request` call on
/// a `Cluster` (Task 3 public API).
///
/// Keeping three distinct variants (rather than `Option<Option<G>>`) gives the
/// H1/H2 HCM call sites a single `match` with no nesting, which clippy prefers
/// and ADR-0047 §5.2 mandates.
///
/// - `Unlimited`: the cluster has **no** `circuit_breakers` configured; callers
///   must proceed without gating (zero stat side-effects).
/// - `Acquired(guard)`: a budget slot was acquired; hold the guard for the
///   operation's duration — dropping it releases the slot and re-syncs all
///   gauges.
/// - `Rejected`: the budget is exhausted (active ≥ cap, or cap == 0); the
///   overflow counter has already been incremented inside `BudgetState`.
#[must_use = "dropping BudgetAcquisition::Acquired immediately releases the budget slot"]
#[derive(Debug)]
pub enum BudgetAcquisition<G> {
    /// No `circuit_breakers` configured — never gate; zero stat side-effects.
    Unlimited,
    /// A budget slot was acquired; hold the guard for the operation's duration.
    Acquired(G),
    /// The budget is exhausted (or the cap is 0); the overflow counter has
    /// already ticked.
    Rejected,
}

// ── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use envoy_stats::{StatHandle, StatsRegistry};

    fn test_registry() -> StatsRegistry {
        StatsRegistry::new()
    }

    fn counter_value(reg: &StatsRegistry, name: &str) -> u64 {
        for (n, handle) in reg.snapshot() {
            if n == name
                && let StatHandle::Counter(c) = handle
            {
                return c.value();
            }
        }
        panic!("counter '{name}' not found in registry snapshot");
    }

    fn gauge_value(reg: &StatsRegistry, name: &str) -> i64 {
        for (n, handle) in reg.snapshot() {
            if n == name
                && let StatHandle::Gauge(g) = handle
            {
                return g.value();
            }
        }
        panic!("gauge '{name}' not found in registry snapshot");
    }

    fn gauge_absent(reg: &StatsRegistry, name: &str) -> bool {
        reg.snapshot().iter().all(|(n, _)| n != name)
    }

    // ── mandatory TDD tests (from PLAN.md) ───────────────────────────────────

    #[test]
    fn zero_cap_always_fails_acquisition() {
        // L1/L2: a 0 cap is an always-open breaker — acquisition fails from construction.
        let b = BudgetState::new(0, 0, false, &test_registry(), "c").unwrap();
        assert!(b.try_acquire_retry().is_none());
        assert!(b.try_acquire_request().is_none());
    }

    #[test]
    fn guard_release_frees_the_slot() {
        let b = BudgetState::new(3, 1024, false, &test_registry(), "c").unwrap();
        let g1 = b.try_acquire_retry().expect("slot 1");
        let g2 = b.try_acquire_retry().expect("slot 2");
        let g3 = b.try_acquire_retry().expect("slot 3");
        assert!(b.try_acquire_retry().is_none()); // cap 3 reached
        drop(g1);
        assert!(b.try_acquire_retry().is_some()); // slot freed
        drop((g2, g3));
    }

    #[test]
    fn overflow_counter_ticks_inside_failed_acquire() {
        // §5.3 single source of truth: the failed try_acquire_retry ticks
        // upstream_rq_retry_overflow; the failed try_acquire_request ticks
        // upstream_rq_pending_overflow (L3). Callers never tick these directly.
        let reg = test_registry();
        let b = BudgetState::new(0, 0, false, &reg, "c").unwrap();
        assert!(b.try_acquire_retry().is_none());
        assert_eq!(
            counter_value(&reg, "cluster.c.upstream_rq_retry_overflow"),
            1
        );
        assert!(b.try_acquire_request().is_none());
        assert_eq!(
            counter_value(&reg, "cluster.c.upstream_rq_pending_overflow"),
            1
        );
    }

    // ── additional coverage ───────────────────────────────────────────────────

    /// L4 momentary open-gauge semantic: gauge reads 1 iff active > 0 AND active >= max.
    #[test]
    fn momentary_open_gauge_semantic() {
        let reg = test_registry();
        let b = BudgetState::new(2, 2, false, &reg, "g").unwrap();

        // Initially 0 active → gauge 0.
        assert_eq!(
            gauge_value(&reg, "cluster.g.circuit_breakers.default.rq_retry_open"),
            0
        );
        assert_eq!(
            gauge_value(&reg, "cluster.g.circuit_breakers.default.rq_open"),
            0
        );

        // Acquire 1 → active=1, max=2 → NOT open (active < max).
        let _g1 = b.try_acquire_retry().expect("slot 1");
        assert_eq!(
            gauge_value(&reg, "cluster.g.circuit_breakers.default.rq_retry_open"),
            0
        );

        // Acquire 2nd → active=2, max=2 → open (active > 0 AND active >= max).
        let _g2 = b.try_acquire_retry().expect("slot 2");
        assert_eq!(
            gauge_value(&reg, "cluster.g.circuit_breakers.default.rq_retry_open"),
            1
        );

        // Drop 1 → active=1 → gauge back to 0.
        drop(_g1);
        assert_eq!(
            gauge_value(&reg, "cluster.g.circuit_breakers.default.rq_retry_open"),
            0
        );

        // Drop last → active=0 → gauge 0.
        drop(_g2);
        assert_eq!(
            gauge_value(&reg, "cluster.g.circuit_breakers.default.rq_retry_open"),
            0
        );
    }

    /// L4: zero-cap breaker never latches open (active never rises above 0).
    #[test]
    fn zero_cap_open_gauge_stays_zero() {
        let reg = test_registry();
        let b = BudgetState::new(0, 0, false, &reg, "z").unwrap();
        // Failed acquisition still updates gauges — open must stay 0.
        assert!(b.try_acquire_retry().is_none());
        assert_eq!(
            gauge_value(&reg, "cluster.z.circuit_breakers.default.rq_retry_open"),
            0
        );
        assert!(b.try_acquire_request().is_none());
        assert_eq!(
            gauge_value(&reg, "cluster.z.circuit_breakers.default.rq_open"),
            0
        );
    }

    /// L8: `remaining_*` gauges registered only when `track_remaining: true`,
    /// initialised to cap values, updated on acquire/release, floored at 0.
    #[test]
    fn remaining_gauges_registered_and_updated_when_track_remaining_true() {
        let reg = test_registry();
        let b = BudgetState::new(3, 5, true, &reg, "r").unwrap();

        // Initialised to cap values.
        assert_eq!(
            gauge_value(&reg, "cluster.r.circuit_breakers.default.remaining_retries"),
            3
        );
        assert_eq!(
            gauge_value(&reg, "cluster.r.circuit_breakers.default.remaining_rq"),
            5
        );

        // Acquire one retry slot → remaining_retries goes to 2.
        let g1 = b.try_acquire_retry().expect("slot");
        assert_eq!(
            gauge_value(&reg, "cluster.r.circuit_breakers.default.remaining_retries"),
            2
        );

        // Release → back to 3.
        drop(g1);
        assert_eq!(
            gauge_value(&reg, "cluster.r.circuit_breakers.default.remaining_retries"),
            3
        );

        // Exhaust all 3 retry slots → remaining_retries = 0 (floored, not negative).
        let _a = b.try_acquire_retry().unwrap();
        let _b = b.try_acquire_retry().unwrap();
        let _c = b.try_acquire_retry().unwrap();
        assert_eq!(
            gauge_value(&reg, "cluster.r.circuit_breakers.default.remaining_retries"),
            0
        );
        // A failed acquire on an exhausted budget → remaining_retries stays 0 (not -1).
        assert!(b.try_acquire_retry().is_none());
        assert_eq!(
            gauge_value(&reg, "cluster.r.circuit_breakers.default.remaining_retries"),
            0
        );

        // ── remaining_rq: acquire/release cycle ──────────────────────────────
        // Initial value is the cap (5).
        assert_eq!(
            gauge_value(&reg, "cluster.r.circuit_breakers.default.remaining_rq"),
            5
        );

        // Acquire one request slot → remaining_rq goes to 4.
        let rq1 = b.try_acquire_request().expect("rq slot 1");
        assert_eq!(
            gauge_value(&reg, "cluster.r.circuit_breakers.default.remaining_rq"),
            4
        );

        // Release → back to 5.
        drop(rq1);
        assert_eq!(
            gauge_value(&reg, "cluster.r.circuit_breakers.default.remaining_rq"),
            5
        );

        // Exhaust all 5 request slots → remaining_rq = 0 (floored, not negative).
        let _rq_a = b.try_acquire_request().unwrap();
        let _rq_b = b.try_acquire_request().unwrap();
        let _rq_c = b.try_acquire_request().unwrap();
        let _rq_d = b.try_acquire_request().unwrap();
        let _rq_e = b.try_acquire_request().unwrap();
        assert_eq!(
            gauge_value(&reg, "cluster.r.circuit_breakers.default.remaining_rq"),
            0
        );

        // A failed acquire on an exhausted budget → remaining_rq stays 0 (not -1).
        assert!(b.try_acquire_request().is_none());
        assert_eq!(
            gauge_value(&reg, "cluster.r.circuit_breakers.default.remaining_rq"),
            0
        );
    }

    /// L8: `remaining_*` gauges ABSENT when `track_remaining: false`.
    #[test]
    fn remaining_gauges_absent_when_track_remaining_false() {
        let reg = test_registry();
        let _b = BudgetState::new(3, 5, false, &reg, "nr").unwrap();
        assert!(gauge_absent(
            &reg,
            "cluster.nr.circuit_breakers.default.remaining_retries"
        ));
        assert!(gauge_absent(
            &reg,
            "cluster.nr.circuit_breakers.default.remaining_rq"
        ));
    }

    /// Shared-Arc idempotency: `register_counter` on an already-registered name
    /// returns the same Arc — verify the budget stats share identity with a
    /// separately-obtained handle (simulates the phase-15 pool overlap for
    /// `upstream_rq_pending_overflow`).
    #[test]
    fn idempotent_counter_registration_shares_arc() {
        let reg = test_registry();
        let b = BudgetState::new(1, 1, false, &reg, "idem").unwrap();
        // Obtain a second handle for the overflow counter via the registry directly.
        let second = reg
            .register_counter("cluster.idem.upstream_rq_pending_overflow")
            .unwrap();
        // Trigger one overflow from the BudgetState path.
        let _hold = b.try_acquire_request(); // fills the single slot; guard held
        let _ = b.try_acquire_request(); // overflows (slot still held)
        // Both handles must read the same value because they are the same Arc.
        assert_eq!(second.value(), 1);
    }

    /// request-budget RAII: acquired slots are counted and released correctly.
    #[test]
    fn request_guard_release_frees_the_slot() {
        let b = BudgetState::new(1024, 2, false, &test_registry(), "rq").unwrap();
        let r1 = b.try_acquire_request().expect("slot 1");
        let r2 = b.try_acquire_request().expect("slot 2");
        assert!(b.try_acquire_request().is_none()); // cap 2 reached
        drop(r1);
        let r3 = b.try_acquire_request().expect("slot freed");
        drop((r2, r3));
    }
}
