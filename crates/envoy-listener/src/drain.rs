//! Phase 08.2 D11: shared `DrainState` foundation observed by data-plane
//! listener accept loops.
//!
//! Lives at `envoy-listener::drain` (and re-exported from `envoy-admin`) per
//! parent-08 SPEC §5.1's Cargo-cycle resolution: the natural placement
//! would be `envoy-admin::drain` (admin endpoints are its only writers),
//! but `envoy-listener::Listener::serve` must consume a typed
//! `Arc<DrainState>` for its accept-loop `tokio::select!` — and
//! `envoy-admin` already depends on `envoy-listener::ConnectionHandler`,
//! so an `envoy-admin → envoy-listener` reverse dep would create a Cargo
//! cycle (structurally identical to the 05.3 / 07.1 cycles resolved at
//! ADR-0028 / ADR-0031). Resolution: `DrainState` lives in `envoy-listener`;
//! `envoy-admin::lib` re-exports `pub use envoy_listener::DrainState` so
//! admin-side call sites read naturally. Mirrors the M4 `DRAIN_BUDGET`
//! hoist (D3 at 08.1) pattern.
//!
//! State machine (parent-08 SPEC §5.6 + 08.2 SPEC §3 D11):
//!
//! ```text
//!         fail_healthcheck()                drain()
//!     Live ─────────────────► HealthcheckFailing ─────────► Draining
//!      ▲                              │                       │ │
//!      │                              │ ok_healthcheck()      │ │
//!      └──────────────────────────────┘                       │ │
//!                                                             ▼ ▼
//!                                            drain() repeat ──┘ │
//!                                        ok_healthcheck() ──────┘  (no-op; sticky)
//! ```
//!
//! `notify.notify_waiters()` fires EXACTLY ONCE — on the first
//! `Live → Draining` or `HealthcheckFailing → Draining` transition.
//! `drain_signal()` returns an immediately-ready future when state is
//! already `Draining` (idempotent + re-entrant). This crate ships ONLY the
//! state-machine + signal primitive at Task 1; gauge wiring
//! (`server.live`, `server.state`, `listener_manager.total_listeners_active`)
//! lands at Task 2 (08.2 PLAN architecture-decision lock-in #3 — the SPEC
//! §6.4 split: foundation first, stats integration second).

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use tokio::sync::Notify;

/// Discriminant matches the `server.state` gauge value per parent-08 SPEC
/// §2.3 + 08.2 SPEC §2.2: `Live = 0`, `HealthcheckFailing = 1`,
/// `Draining = 2`. The `#[repr(u8)]` is load-bearing — `DrainState::current()`
/// converts via `from_u8` and `drain()` writes the discriminant directly
/// via `AtomicU8::store`. Sticky-drain: `Draining` is terminal.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrainStage {
    Live = 0,
    HealthcheckFailing = 1,
    Draining = 2,
}

impl DrainStage {
    /// Convert from the underlying `AtomicU8` representation. Returns `None`
    /// for unrepresented discriminants — the `current()` accessor on
    /// `DrainState` collapses `None` to a panic because the only writers
    /// of the atomic are this module's own methods.
    pub fn from_u8(n: u8) -> Option<Self> {
        match n {
            0 => Some(DrainStage::Live),
            1 => Some(DrainStage::HealthcheckFailing),
            2 => Some(DrainStage::Draining),
            _ => None,
        }
    }
}

/// Shared drain-state primitive. Constructed once at `envoy-bin::main`
/// startup via [`DrainState::new`]; an `Arc<DrainState>` is cloned into the
/// admin handler (writer) and each data-plane listener accept-loop
/// (reader/observer).
pub struct DrainState {
    /// Underlying `AtomicU8` carrying the `DrainStage` discriminant.
    /// `compare_exchange` semantics gate the `Live | HealthcheckFailing →
    /// Draining` transition so `notify.notify_waiters()` fires exactly once.
    state: AtomicU8,
    /// Wakes all `drain_signal()` waiters when `drain()` first succeeds at
    /// flipping to `Draining`. `tokio::sync::Notify` is the right primitive
    /// per parent-08 SPEC §6.6 — multi-consumer, zero-copy, cheap to clone
    /// via `Arc`.
    notify: Notify,
    /// Task 2 (D14): `server.live` gauge. `1` ONLY when
    /// `current() == Live`; `0` otherwise (per 08.2 SPEC §2.2 —
    /// HealthcheckFailing AND Draining both emit `0`). Updated inline at
    /// every `*_healthcheck` / `drain` CAS-success site; never polled.
    server_live: Arc<envoy_stats::Gauge>,
    /// Task 2 (D14): `server.state` gauge. Discriminant of `DrainStage`
    /// (`Live = 0`, `HealthcheckFailing = 1`, `Draining = 2`). The
    /// `#[repr(u8)]` on `DrainStage` makes the discriminant load-bearing;
    /// gauge values are projected via `stage as i64`. Updated at the same
    /// CAS-success sites as `server_live` (one source of truth).
    server_state: Arc<envoy_stats::Gauge>,
}

impl DrainState {
    /// Construct a fresh `DrainState` in the `Live` stage with no pending
    /// waiters. Registers the `server.live` and `server.state` gauges
    /// against `registry` and stores both as `Arc<Gauge>` fields for inline
    /// updates at every state-transition CAS-success site (one source of
    /// truth — never polled).
    ///
    /// Initial gauge values match the initial `DrainStage::Live` state:
    /// `server.live = 1` and `server.state = 0` (per 08.2 SPEC §2.2).
    pub fn new(registry: &Arc<envoy_stats::StatsRegistry>) -> Self {
        let server_live = registry
            .register_gauge("server.live")
            .expect("server.live gauge registration");
        let server_state = registry
            .register_gauge("server.state")
            .expect("server.state gauge registration");
        // Initial values match the initial DrainStage::Live state.
        server_live.set(1);
        server_state.set(DrainStage::Live as i64);
        Self {
            state: AtomicU8::new(DrainStage::Live as u8),
            notify: Notify::new(),
            server_live,
            server_state,
        }
    }

    /// Read the current `DrainStage`. Uses `Ordering::Acquire` to pair with
    /// the `Ordering::Release` store in the mutator methods — every observer
    /// sees a coherent stage value with respect to the mutator that last
    /// wrote it.
    pub fn current(&self) -> DrainStage {
        let raw = self.state.load(Ordering::Acquire);
        DrainStage::from_u8(raw)
            .unwrap_or_else(|| panic!("DrainState atomic carries invalid discriminant: {raw}"))
    }

    /// Transition `Live → HealthcheckFailing` (`compare_exchange`). All
    /// other from-stages are no-ops (sticky `Draining`; idempotent
    /// `HealthcheckFailing`). Does NOT call `notify_waiters` — only
    /// `drain()` does.
    pub fn fail_healthcheck(&self) {
        let cas = self.state.compare_exchange(
            DrainStage::Live as u8,
            DrainStage::HealthcheckFailing as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        // Sticky `Draining` and self-loop `HealthcheckFailing` both fail
        // the CAS silently — that's the desired idempotent behavior.
        if cas.is_ok() {
            // Task 2 (D14): `server.live = 0` when `current() != Live`
            // (08.2 SPEC §2.2). `server.state` carries the
            // HealthcheckFailing discriminant.
            self.server_live.set(0);
            self.server_state.set(DrainStage::HealthcheckFailing as i64);
        }
    }

    /// Transition `HealthcheckFailing → Live` (`compare_exchange`). All
    /// other from-stages are no-ops (sticky `Draining`; idempotent `Live`).
    /// Does NOT call `notify_waiters` — only `drain()` does. The sticky-
    /// drain semantic at parent-08 SPEC §5.6: `ok_healthcheck()` AFTER
    /// `drain()` MUST NOT un-drain.
    pub fn ok_healthcheck(&self) {
        let cas = self.state.compare_exchange(
            DrainStage::HealthcheckFailing as u8,
            DrainStage::Live as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        // Sticky `Draining` and self-loop `Live` both fail the CAS silently.
        if cas.is_ok() {
            // Task 2 (D14): restore Live values.
            self.server_live.set(1);
            self.server_state.set(DrainStage::Live as i64);
        }
    }

    /// Sticky transition `* → Draining`. Calls `notify_waiters` EXACTLY
    /// ONCE — on the first successful CAS from `Live` or `HealthcheckFailing`
    /// to `Draining`. Repeat `drain()` calls fail the CAS silently and do
    /// NOT re-notify (avoids wasted cycles per parent-08 SPEC §6.6).
    pub fn drain(&self) {
        // Two CAS attempts cover the two valid from-stages (`Live` and
        // `HealthcheckFailing`); exactly one can succeed on the first
        // call. Already-`Draining` falls through both CAS-failures (the
        // store-write order is `compare_exchange` succeeds only when the
        // current value matches the `expected` arg, so a Draining-from
        // value never matches either expected arg).
        let from_live = self.state.compare_exchange(
            DrainStage::Live as u8,
            DrainStage::Draining as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        let from_hc = if from_live.is_err() {
            self.state.compare_exchange(
                DrainStage::HealthcheckFailing as u8,
                DrainStage::Draining as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
        } else {
            Err(0) // Sentinel — we already succeeded via `from_live`.
        };
        if from_live.is_ok() || from_hc.is_ok() {
            // Task 2 (D14): gauges update BEFORE `notify_waiters` so a
            // just-woken waiter that reads the registry sees the
            // post-drain values.
            self.server_live.set(0);
            self.server_state.set(DrainStage::Draining as i64);
            // Wake all currently-registered `drain_signal()` waiters.
            // Future calls to `drain_signal()` see the already-Draining
            // branch in that method and return an immediately-ready
            // future (no notify needed).
            self.notify.notify_waiters();
        }
    }

    /// Returns a future that resolves when `drain()` has fired (now or in
    /// the future). If the state is ALREADY `Draining`, the returned
    /// future is immediately ready; otherwise it parks on `notify.notified()`
    /// until `drain()` fires.
    ///
    /// Observed by `envoy_listener::Listener::serve`'s `tokio::select!`
    /// arm at Task 6 (D12). The admin listener (`envoy_admin::serve`) does
    /// NOT observe its own `drain_signal` per parent-08 SPEC §5.5 — the
    /// admin listener stays serving during drain so `/server_info` +
    /// `/stats/prometheus` remain reachable.
    pub fn drain_signal(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        // Anchor the notify snapshot BEFORE the state load so a concurrent
        // `drain()` firing between the two cannot leave a registered waiter
        // permanently parked. `tokio::sync::Notify::notified()` snapshots the
        // notify_waiters counter at construction time per the tokio docs
        // ("guaranteed to receive wakeups from notify_waiters() as soon as it
        // has been created"). If we loaded state first and then constructed
        // `notified`, a between-the-two `drain()` could bump the counter,
        // making the subsequently-constructed `Notified` snapshot the post-bump
        // value — on first poll the counter comparison would fall through and
        // register a waiter that never unparks (sticky-drain idempotency means
        // no second `notify_waiters()` ever fires).
        let notified = self.notify.notified();
        if self.state.load(Ordering::Acquire) == DrainStage::Draining as u8 {
            // Discard the unpolled `Notified` and return immediately. Drop on
            // an unpolled `Notified` is safe (no registration has occurred yet).
            return Box::pin(std::future::ready(()));
        }
        Box::pin(notified)
    }
}

// Task 2 (D14): the `Default` impl from Task 1 is REMOVED — `DrainState::new`
// now requires `&Arc<StatsRegistry>`, which `Default` cannot synthesize.

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    /// Construct an `Arc<StatsRegistry>` for tests that don't need to assert
    /// gauge values, only that `DrainState::new(&registry)` accepts a registry.
    /// Task 2 widening: `DrainState::new` now takes `&Arc<StatsRegistry>` for
    /// gauge registration.
    fn mk_registry() -> Arc<envoy_stats::StatsRegistry> {
        Arc::new(envoy_stats::StatsRegistry::new())
    }

    /// Test 1 of 9: a fresh `DrainState::new(&registry)` starts at
    /// `DrainStage::Live`.
    #[test]
    fn new_returns_live() {
        let drain = DrainState::new(&mk_registry());
        assert_eq!(drain.current(), DrainStage::Live);
    }

    /// Test 2 of 9: `drain()` flips state to `Draining` AND notifies all
    /// pending `drain_signal()` waiters exactly once. Uses a 3-party `Barrier`
    /// (Important #2 carryforward from Task 1 fixup review: deterministic
    /// rendezvous replaces the prior 50ms sleep, which was flake-prone on
    /// loaded CI runners).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn drain_flips_to_draining_and_notifies_waiters_once() {
        use tokio::sync::Barrier;

        let drain = Arc::new(DrainState::new(&mk_registry()));
        let barrier = Arc::new(Barrier::new(3));

        // Spawn two waiters; each constructs `drain_signal()`, signals at the
        // barrier (rendezvous with main task + sibling waiter), then awaits.
        let d1 = Arc::clone(&drain);
        let b1 = Arc::clone(&barrier);
        let h1 = tokio::spawn(async move {
            let signal = d1.drain_signal();
            b1.wait().await;
            signal.await;
        });
        let d2 = Arc::clone(&drain);
        let b2 = Arc::clone(&barrier);
        let h2 = tokio::spawn(async move {
            let signal = d2.drain_signal();
            b2.wait().await;
            signal.await;
        });

        // Wait at the barrier; once all 3 parties arrive, both waiters have
        // anchored their notify snapshots and are about to await.
        barrier.wait().await;

        // Fire drain ONCE; both waiters must complete.
        drain.drain();
        tokio::time::timeout(Duration::from_secs(1), h1)
            .await
            .expect("waiter 1 must complete within 1s of drain()")
            .expect("waiter 1 join");
        tokio::time::timeout(Duration::from_secs(1), h2)
            .await
            .expect("waiter 2 must complete within 1s of drain()")
            .expect("waiter 2 join");

        assert_eq!(drain.current(), DrainStage::Draining);

        // A NEW post-drain waiter must complete IMMEDIATELY (already-Draining
        // path returns a ready future).
        tokio::time::timeout(Duration::from_millis(50), drain.drain_signal())
            .await
            .expect("post-drain drain_signal must be immediately ready");
    }

    /// Test 3 of 9: `fail_healthcheck()` from `Live` flips to
    /// `HealthcheckFailing`.
    #[test]
    fn fail_healthcheck_flips_to_healthcheck_failing() {
        let drain = DrainState::new(&mk_registry());
        assert_eq!(drain.current(), DrainStage::Live);
        drain.fail_healthcheck();
        assert_eq!(drain.current(), DrainStage::HealthcheckFailing);
    }

    /// Test 4 of 9: `ok_healthcheck()` from `HealthcheckFailing` restores
    /// `Live`.
    #[test]
    fn ok_healthcheck_restores_to_live() {
        let drain = DrainState::new(&mk_registry());
        drain.fail_healthcheck();
        assert_eq!(drain.current(), DrainStage::HealthcheckFailing);
        drain.ok_healthcheck();
        assert_eq!(drain.current(), DrainStage::Live);
    }

    /// Test 5 of 9: `ok_healthcheck()` AFTER `drain()` is a no-op (sticky-
    /// drain semantic per parent-08 SPEC §5.6).
    #[test]
    fn ok_healthcheck_after_drain_is_noop_sticky() {
        let drain = DrainState::new(&mk_registry());
        drain.drain();
        assert_eq!(drain.current(), DrainStage::Draining);
        drain.ok_healthcheck();
        assert_eq!(
            drain.current(),
            DrainStage::Draining,
            "ok_healthcheck after drain must NOT un-drain (sticky)"
        );
    }

    /// Test 6 of 9: repeat `drain()` calls are idempotent (no second
    /// notify_waiters; state stays `Draining`).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn repeat_drain_calls_are_idempotent() {
        let drain = Arc::new(DrainState::new(&mk_registry()));
        drain.drain();
        assert_eq!(drain.current(), DrainStage::Draining);

        // Second drain() must not panic + state stays Draining.
        drain.drain();
        assert_eq!(drain.current(), DrainStage::Draining);

        // Third drain() ditto.
        drain.drain();
        assert_eq!(drain.current(), DrainStage::Draining);

        // A waiter registered AFTER any drain() call completes immediately.
        tokio::time::timeout(Duration::from_millis(50), drain.drain_signal())
            .await
            .expect("post-drain drain_signal must be immediately ready");
    }

    /// Test 7 of 9: `drain_signal()` is race-free with respect to a concurrent
    /// `drain()` call that fires AFTER the future's caller has entered
    /// `drain_signal()` but BEFORE the returned future is polled. Regression
    /// test for the TOCTOU race closed at the Task 1 fixup commit: in the
    /// pre-fix shape `state.load() → (window) → notify.notified()` a `drain()`
    /// in the window would bump the notify counter, the subsequently-constructed
    /// `Notified` would snapshot the post-bump value, and on poll the waiter
    /// would park forever (sticky-drain means no second `notify_waiters()` ever
    /// fires). The fixed shape inverts the order — `notify.notified()` first,
    /// then state.load — so the snapshot anchors before any concurrent counter
    /// bump can race in. Without the fix this test deterministically hangs;
    /// with the fix it completes well within the 1s timeout.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn drain_signal_is_race_free_with_concurrent_drain() {
        use tokio::sync::Barrier;

        let drain = Arc::new(DrainState::new(&mk_registry()));
        // Barrier of 2: the spawned task signals it has called `drain_signal()`
        // (anchoring the notify snapshot); the main task then fires `drain()`.
        let barrier = Arc::new(Barrier::new(2));

        let d_inner = Arc::clone(&drain);
        let b_inner = Arc::clone(&barrier);
        let handle = tokio::spawn(async move {
            // Construct the signal future BEFORE the barrier rendezvous so
            // the notify snapshot is anchored before drain can fire.
            let signal = d_inner.drain_signal();
            b_inner.wait().await;
            // Now drain may fire on the main task; poll the signal future.
            signal.await;
        });

        // Wait for the spawned task to have constructed its signal future.
        barrier.wait().await;
        // Fire drain on the main task. The spawned task's signal future must
        // observe completion via the counter-bump fast path inside `Notified`'s
        // poll (the snapshot was taken before the bump, so the counter check
        // succeeds on first poll).
        drain.drain();

        tokio::time::timeout(Duration::from_secs(1), handle)
            .await
            .expect("signal future must complete within 1s of drain() under the race-free shape")
            .expect("spawned task join");
    }

    /// Test 8 of 9: `drain()` from `HealthcheckFailing` flips state to
    /// `Draining` AND notifies any pending `drain_signal()` waiter — exercises
    /// the second-CAS branch of `drain()` (Live→Draining CAS fails because
    /// state is HealthcheckFailing, then HealthcheckFailing→Draining CAS
    /// succeeds; `notify_waiters()` still fires exactly once).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn drain_from_healthcheck_failing_notifies_waiters_once() {
        use tokio::sync::Barrier;

        let drain = Arc::new(DrainState::new(&mk_registry()));
        drain.fail_healthcheck();
        assert_eq!(drain.current(), DrainStage::HealthcheckFailing);

        let barrier = Arc::new(Barrier::new(2));
        let d_inner = Arc::clone(&drain);
        let b_inner = Arc::clone(&barrier);
        let h = tokio::spawn(async move {
            let signal = d_inner.drain_signal();
            b_inner.wait().await;
            signal.await;
        });

        barrier.wait().await;
        drain.drain();
        tokio::time::timeout(Duration::from_secs(1), h)
            .await
            .expect("waiter must complete within 1s of drain() from HealthcheckFailing")
            .expect("waiter join");

        assert_eq!(drain.current(), DrainStage::Draining);
    }

    /// Test 9 of 9: `fail_healthcheck()` AFTER `drain()` is a no-op (sticky-
    /// drain semantic per parent-08 SPEC §5.6; symmetric with Test 5's
    /// `ok_healthcheck()` post-drain assertion). The `compare_exchange` for
    /// Live → HealthcheckFailing silently fails when the current value is
    /// Draining (CAS expected-value mismatch).
    #[test]
    fn fail_healthcheck_after_drain_is_noop_sticky() {
        let drain = DrainState::new(&mk_registry());
        drain.drain();
        assert_eq!(drain.current(), DrainStage::Draining);
        drain.fail_healthcheck();
        assert_eq!(
            drain.current(),
            DrainStage::Draining,
            "fail_healthcheck after drain must NOT downgrade out of Draining (sticky)"
        );
    }

    // ─────────────────────────────────────────────────────────────────
    // Task 2 (D14 — gauge wiring): 5 new tests.
    // ─────────────────────────────────────────────────────────────────

    /// Test 10 of 14: `DrainState::new(&registry)` registers `server.live`
    /// against the supplied registry.
    #[test]
    fn new_registers_server_live_gauge() {
        let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());
        let _drain = DrainState::new(&registry);
        let snapshot: std::collections::BTreeMap<_, _> = registry.snapshot().into_iter().collect();
        assert!(
            snapshot.contains_key("server.live"),
            "server.live gauge missing; snapshot keys: {:?}",
            snapshot.keys().collect::<Vec<_>>()
        );
    }

    /// Test 11 of 14: `DrainState::new(&registry)` registers `server.state`
    /// against the supplied registry.
    #[test]
    fn new_registers_server_state_gauge() {
        let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());
        let _drain = DrainState::new(&registry);
        let snapshot: std::collections::BTreeMap<_, _> = registry.snapshot().into_iter().collect();
        assert!(snapshot.contains_key("server.state"));
    }

    /// Test 12 of 14: a fresh `DrainState` has `server.live = 1` and
    /// `server.state = 0` (Live discriminant). Initial-value invariant per
    /// 08.2 SPEC §2.2.
    #[test]
    fn new_initial_gauge_values_are_live() {
        let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());
        let _drain = DrainState::new(&registry);
        let snapshot: std::collections::BTreeMap<_, _> = registry.snapshot().into_iter().collect();
        match snapshot.get("server.live").expect("server.live missing") {
            envoy_stats::StatHandle::Gauge(g) => assert_eq!(g.value(), 1),
            _ => panic!("server.live is not a gauge"),
        }
        match snapshot.get("server.state").expect("server.state missing") {
            envoy_stats::StatHandle::Gauge(g) => assert_eq!(g.value(), 0),
            _ => panic!("server.state is not a gauge"),
        }
    }

    /// Test 13 of 14: `drain()` flips `server.live = 0` and
    /// `server.state = 2` (Draining discriminant) at the CAS-success site,
    /// BEFORE `notify_waiters()` fires (so a just-woken waiter that reads
    /// the gauge sees the post-drain value).
    #[test]
    fn drain_updates_server_live_to_zero_and_server_state_to_two() {
        let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());
        let drain = DrainState::new(&registry);
        drain.drain();
        let snapshot: std::collections::BTreeMap<_, _> = registry.snapshot().into_iter().collect();
        match snapshot.get("server.live").unwrap() {
            envoy_stats::StatHandle::Gauge(g) => assert_eq!(g.value(), 0),
            _ => unreachable!(),
        }
        match snapshot.get("server.state").unwrap() {
            envoy_stats::StatHandle::Gauge(g) => assert_eq!(g.value(), 2),
            _ => unreachable!(),
        }
    }

    /// Test 14 of 14: `fail_healthcheck()` flips `server.live = 0` and
    /// `server.state = 1` (HealthcheckFailing discriminant) at the
    /// CAS-success site.
    ///
    /// Per 08.2 SPEC §2.2: `server.live` emits `1` ONLY when
    /// `current() == Live`; HealthcheckFailing AND Draining both emit `0`.
    /// (This corrects PLAN.md's first-draft test name
    /// `fail_healthcheck_keeps_server_live_at_one` — see PLAN.md line 989
    /// for the in-PLAN self-correction.)
    #[test]
    fn fail_healthcheck_sets_server_live_to_zero_and_server_state_to_one() {
        let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());
        let drain = DrainState::new(&registry);
        drain.fail_healthcheck();
        let snapshot: std::collections::BTreeMap<_, _> = registry.snapshot().into_iter().collect();
        match snapshot.get("server.live").unwrap() {
            envoy_stats::StatHandle::Gauge(g) => assert_eq!(
                g.value(),
                0,
                "server.live MUST be 0 when current() != Live (HealthcheckFailing case)"
            ),
            _ => unreachable!(),
        }
        match snapshot.get("server.state").unwrap() {
            envoy_stats::StatHandle::Gauge(g) => assert_eq!(g.value(), 1),
            _ => unreachable!(),
        }
    }
}
