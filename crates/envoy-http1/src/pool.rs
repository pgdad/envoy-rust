//! 13.1 D3: per-cluster, per-endpoint H1 connection pool. Holds an idle
//! keep-alive list of `ClientStream`s; `acquire()` reuses an idle stream
//! or connects a new one (subject to `max_connections` cap). `PoolGuard`
//! is the per-acquire RAII handle; Drop returns the stream to the pool's
//! idle list (success) or destroys it (on `invalidate()` flag, e.g. on
//! protocol error). One `H1Pool` per cluster lives inside `H1PoolManager`,
//! keyed by cluster name; the manager is constructed bin-side at startup
//! and looked up by the H1 HCM proxy arm via `manager.get(cluster_name)`.

#![allow(clippy::type_complexity)]

use crate::client::{Client, ClientStream};
use crate::error::Http1Error;
use envoy_cluster::ConnGaugeGuard;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// Phase-13 hardcoded H1 pool defaults (§5.4 + §2 item-iii deferral).
const DEFAULT_MAX_CONNECTIONS: u32 = 1024;
/// 15 D3: default `max_pending_requests` for clusters without circuit-breakers
/// config. Matches Envoy's default + the as-today behavior (the reject gate
/// never fires unless explicitly set to 0). See lock-in #4.
const DEFAULT_MAX_PENDING_REQUESTS: u32 = 1024;
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(60);
/// Sweeper tick interval: `idle_timeout / 4` (15s at the default 60s timeout).
const SWEEPER_DIVISOR: u32 = 4;

/// Errors returned by `H1Pool::acquire`.
#[derive(Debug, thiserror::Error)]
pub enum PoolError {
    /// Pool is at `max_connections` AND no idle stream available.
    #[error("upstream pool overflow: cluster='{cluster}', max_connections={max}")]
    Overflow { cluster: String, max: u32 },
    /// Pool's `max_pending_requests` is 0 and a new connection must be established
    /// (no idle stream to reuse). Envoy reject-on-establish parity (ADR-0043 §6.2 finding 1).
    #[error("upstream pending-request overflow: cluster='{cluster}' (max_pending_requests=0)")]
    PendingOverflow { cluster: String },
    /// `Client::connect()` failed on the connect-on-miss branch.
    #[error(transparent)]
    Connect(#[from] Http1Error),
}

struct IdleEntry {
    stream: ClientStream,
    last_returned: Instant,
}

/// One pool per cluster. Holds idle keep-alive streams per endpoint, plus
/// the established-count counter (idle + in-flight) for max_connections.
pub struct H1Pool {
    cluster_name: String,
    max_connections: u32,
    /// 15 D3: `max_pending_requests` cap. Only `0` (no-queue) is meaningful at
    /// phase-15 scope (the validator rejects `> 0`); `0` rejects every
    /// connect-on-miss with `PendingOverflow`. Defaults to
    /// `DEFAULT_MAX_PENDING_REQUESTS` (1024) for unconfigured clusters → the
    /// gate never fires (lock-in #4).
    max_pending_requests: u32,
    idle_timeout: Duration,
    /// Per-endpoint idle list. 13.2 A-I3 closure: switched from
    /// `tokio::sync::Mutex` to `parking_lot::Mutex` so the per-acquire
    /// `acquire()` and the per-release `Drop` paths are both synchronous
    /// — the spurious-overflow race between concurrent acquire/release
    /// (originally diagnosed under the async-Mutex `tokio::spawn`-in-Drop
    /// shape) is eliminated structurally. `acquire()` no longer holds the
    /// lock across an `.await`; the connect step happens after the lock
    /// is released.
    idle: parking_lot::Mutex<HashMap<SocketAddr, Vec<IdleEntry>>>,
    /// Per-endpoint total established conn count (idle + in-flight).
    established: parking_lot::Mutex<HashMap<SocketAddr, u32>>,
    /// Per-cluster `upstream_cx_total` — shared Arc with `Cluster.cx_total`
    /// (the same `envoy_stats::Counter` handle; pool's `acquire()` connect-on-miss
    /// is the SOLE incrementer at 13.1 per lock-in #6).
    cx_total: Arc<envoy_stats::Counter>,
    /// Per-cluster `upstream_cx_destroy` — incremented at every pool eviction.
    cx_destroy: Arc<envoy_stats::Counter>,
    /// Per-cluster `upstream_cx_http1_total` — incremented at every H1 connect-on-miss.
    cx_http1_total: Arc<envoy_stats::Counter>,
    /// Per-cluster `upstream_cx_active` gauge handle — shared Arc with `Cluster.cx_active`.
    /// Each `PoolGuard` owns a `ConnGaugeGuard` created via this handle.
    cx_active: Arc<envoy_stats::Gauge>,
    /// 15 D3: per-cluster `upstream_rq_pending_overflow` counter, registered
    /// ONLY for clusters whose `circuit_breakers` is configured (lock-in #4 —
    /// inert-when-unconfigured). `None` for unconfigured clusters; the
    /// reject gate short-circuits on `max_pending_requests != 0` (default
    /// 1024) before this is ever touched, so an unconfigured cluster never
    /// reaches an `unwrap`. (`envoy_stats::Counter::new()` is `pub(crate)`,
    /// so a throwaway unregistered handle cannot be built here — the `Option`
    /// is the documented fallback per the PLAN Step 6 caveat.)
    rq_pending_overflow: Option<Arc<envoy_stats::Counter>>,
    /// 15 D4: per-cluster `upstream_cx_overflow` counter (lock-in #5),
    /// incremented at the SOLE cap-check branch in `acquire()` when the
    /// per-endpoint `established` count is already at `max_connections`.
    /// Registered ONLY for clusters whose `circuit_breakers` is configured
    /// (lock-in #4 — inert-when-unconfigured); `None` otherwise. The increment
    /// site guards with `if let Some(h) = &self.cx_overflow` so an unconfigured
    /// cluster never touches it. (`Counter::new()` is `pub(crate)` — the
    /// `Option` is the documented fallback, mirroring `rq_pending_overflow`.)
    cx_overflow: Option<Arc<envoy_stats::Counter>>,
    /// 15 D4: per-cluster `circuit_breakers.default.cx_open` gauge (lock-in #6),
    /// edge-driven (NOT polled): `set(1)` when an `established` increment makes
    /// the per-endpoint count reach `max_connections` (at-cap inclusive);
    /// `set(0)` at each decrement edge that drops below the cap (the
    /// `PoolGuard::Drop` destroy path, the connect-failure rollback, the
    /// idle-sweeper eviction). All edge updates run UNDER the held `established`
    /// lock. Registered ONLY for circuit-breakers-configured clusters
    /// (inert-when-unconfigured); `None` otherwise — guarded with
    /// `if let Some(g) = &self.cx_open`. (`Gauge::new()` is `pub(crate)` — the
    /// `Option` is the documented fallback.) Terminal-0 (returns to 0 after
    /// drain) so a post-settle scrape is deterministic. NOTE: `cx_open` is a
    /// per-cluster gauge but `established` is per-endpoint; for the
    /// single-endpoint fixtures they coincide (multi-endpoint reconciliation
    /// defers — lock-in #6).
    cx_open: Option<Arc<envoy_stats::Gauge>>,
}

/// Per-acquire RAII handle. Owns one `ConnGaugeGuard` (gauge decrements on
/// drop) + holds the borrowed `ClientStream` until Drop returns it to the
/// pool's idle list (success) or destroys it (`invalidate()`-flagged path).
pub struct PoolGuard {
    pool: Arc<H1Pool>,
    endpoint: SocketAddr,
    stream: Option<ClientStream>,
    _cx_active_guard: ConnGaugeGuard,
}

// Hand-rolled `Debug` (rather than `#[derive]`) because `ConnGaugeGuard`
// doesn't impl Debug. Surfaces just the loadbearing identifiers — the
// per-acquire endpoint + the parent cluster name — which is what
// `Result::expect_err`'s formatting needs.
impl std::fmt::Debug for PoolGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PoolGuard")
            .field("cluster", &self.pool.cluster_name)
            .field("endpoint", &self.endpoint)
            .field("has_stream", &self.stream.is_some())
            .finish()
    }
}

impl PoolGuard {
    /// Borrow the underlying `ClientStream` mutably for `send_request`.
    /// Panics if called after `invalidate()` — invalidated guards are intended
    /// to drop immediately, not to send more requests.
    pub fn stream_mut(&mut self) -> &mut ClientStream {
        self.stream.as_mut().expect("stream_mut after invalidate")
    }

    /// Mark the stream as un-returnable. Drop will destroy + increment
    /// `cx_destroy` instead of returning to the pool's idle list. Call on
    /// any protocol-level error that may have left the stream in a
    /// half-broken state.
    pub fn invalidate(&mut self) {
        // Take + immediately drop the stream (TCP close). Drop below sees
        // `self.stream == None` and runs the destroy-bookkeeping branch.
        drop(self.stream.take());
    }
}

impl Drop for PoolGuard {
    fn drop(&mut self) {
        // 13.2 A-I3 closure: Drop is synchronous. The pool's mutexes are
        // `parking_lot::Mutex` so the return-to-pool + destroy paths run
        // in-place without spawning. Eliminates the spurious-overflow
        // race that the original `tokio::spawn`-in-Drop shape produced
        // under concurrent acquire/release (the async return-to-pool
        // could race the next acquire's connect-on-miss cap-check).
        // `_cx_active_guard`'s Drop fires at field-drop time →
        // upstream_cx_active.dec().
        match self.stream.take() {
            Some(stream) => {
                // Return-to-pool: synchronous push into the idle list.
                let mut idle = self.pool.idle.lock();
                idle.entry(self.endpoint).or_default().push(IdleEntry {
                    stream,
                    last_returned: Instant::now(),
                });
            }
            None => {
                // Destroy path (invalidated): increment cx_destroy + decrement
                // established. Both are synchronous (counter inc + sync lock).
                self.pool.cx_destroy.inc();
                let mut est = self.pool.established.lock();
                if let Some(n) = est.get_mut(&self.endpoint) {
                    *n = n.saturating_sub(1);
                    // 15 D4 (lock-in #6): clear cx_open when this decrement
                    // drops the per-endpoint count below max_connections.
                    if *n < self.pool.max_connections
                        && let Some(g) = &self.pool.cx_open
                    {
                        g.set(0);
                    }
                }
            }
        }
    }
}

impl H1Pool {
    /// Build a new pool. `cx_total`/`cx_active` come from the existing cluster
    /// stat handles (shared `Arc`); `cx_destroy`/`cx_http1_total` are
    /// registered by the caller (see `H1PoolManager::for_bootstrap`).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cluster_name: String,
        max_connections: u32,
        max_pending_requests: u32,
        idle_timeout: Duration,
        cx_total: Arc<envoy_stats::Counter>,
        cx_destroy: Arc<envoy_stats::Counter>,
        cx_http1_total: Arc<envoy_stats::Counter>,
        cx_active: Arc<envoy_stats::Gauge>,
        rq_pending_overflow: Option<Arc<envoy_stats::Counter>>,
        cx_overflow: Option<Arc<envoy_stats::Counter>>,
        cx_open: Option<Arc<envoy_stats::Gauge>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            cluster_name,
            max_connections,
            max_pending_requests,
            idle_timeout,
            idle: parking_lot::Mutex::new(HashMap::new()),
            established: parking_lot::Mutex::new(HashMap::new()),
            cx_total,
            cx_destroy,
            cx_http1_total,
            cx_active,
            rq_pending_overflow,
            cx_overflow,
            cx_open,
        })
    }

    /// Acquire a stream to `endpoint`. Reuses an idle stream if any; otherwise
    /// creates a new TCP connection (subject to `max_connections`). On
    /// overflow + no idle, returns `PoolError::Overflow`.
    pub async fn acquire(
        self: &Arc<Self>,
        endpoint: SocketAddr,
        host: &str,
    ) -> Result<PoolGuard, PoolError> {
        // Try idle reuse first (synchronous pop under lock). 13.2 A-I3
        // closure: `parking_lot::Mutex` — no `.await` at lock acquisition.
        {
            let mut idle = self.idle.lock();
            if let Some(list) = idle.get_mut(&endpoint)
                && let Some(entry) = list.pop()
            {
                // Reuse: established count unchanged (was already counted at
                // original connect). Bind cx_active_guard via a fresh per-PoolGuard.
                let _cx_active_guard = self.acquire_cx_active_guard();
                return Ok(PoolGuard {
                    pool: Arc::clone(self),
                    endpoint,
                    stream: Some(entry.stream),
                    _cx_active_guard,
                });
            }
        }
        // 15 D3 (lock-in #7): max_pending_requests:0 reject-on-establish. A new
        // connection must be established (no idle stream); under
        // max_pending_requests:0 Envoy rejects before any connect (ADR-0043 §6.2
        // finding 1). Fires BEFORE the cap-check so upstream_cx_overflow stays 0
        // (no connection demand reaches the cap). For unconfigured clusters
        // max_pending_requests defaults to 1024, so this branch is dead and the
        // `rq_pending_overflow` Option is never touched.
        if self.max_pending_requests == 0 {
            if let Some(counter) = &self.rq_pending_overflow {
                counter.inc();
            }
            return Err(PoolError::PendingOverflow {
                cluster: self.cluster_name.clone(),
            });
        }
        // Connect-on-miss: enforce cap.
        {
            let mut est = self.established.lock();
            let n = est.entry(endpoint).or_insert(0);
            if *n >= self.max_connections {
                // 15 D4 (lock-in #5): cap-hit count — the SOLE cx_overflow site.
                if let Some(h) = &self.cx_overflow {
                    h.inc();
                }
                return Err(PoolError::Overflow {
                    cluster: self.cluster_name.clone(),
                    max: self.max_connections,
                });
            }
            *n += 1;
            // 15 D4 (lock-in #6): at-cap inclusive — set cx_open=1 when this
            // increment makes the per-endpoint count reach max_connections.
            if *n >= self.max_connections
                && let Some(g) = &self.cx_open
            {
                g.set(1);
            }
        }
        // Connect (lock released — connect is the slow path).
        let stream = match Client::connect(endpoint, host).await {
            Ok(s) => s,
            Err(e) => {
                // Roll back the established count.
                let mut est = self.established.lock();
                if let Some(n) = est.get_mut(&endpoint) {
                    *n = n.saturating_sub(1);
                    // 15 D4 (lock-in #6): clear cx_open if the rollback drops
                    // the per-endpoint count below max_connections.
                    if *n < self.max_connections
                        && let Some(g) = &self.cx_open
                    {
                        g.set(0);
                    }
                }
                return Err(PoolError::Connect(e));
            }
        };
        // Fire the two connect-on-miss counters (lock-in #6 + lock-in #3 namespacing).
        self.cx_total.inc();
        self.cx_http1_total.inc();
        let _cx_active_guard = self.acquire_cx_active_guard();
        Ok(PoolGuard {
            pool: Arc::clone(self),
            endpoint,
            stream: Some(stream),
            _cx_active_guard,
        })
    }

    /// Internal: build a `ConnGaugeGuard` for the `cx_active` gauge via inc+wrap.
    /// 13.1 deviates from the existing `Cluster::cx_active_guard` path (which
    /// requires a `Cluster` reference): pool callers don't hold a `Cluster`;
    /// the inc+wrap pattern is duplicated here against the shared `Arc<Gauge>`
    /// (load-bearing: the gauge handle is the SAME Arc the cluster holds).
    fn acquire_cx_active_guard(&self) -> ConnGaugeGuard {
        self.cx_active.inc();
        ConnGaugeGuard::from_gauge(Arc::clone(&self.cx_active))
    }

    /// Spawn the idle-timeout sweeper task. The returned `JoinHandle` is owned
    /// by the caller (typically `H1PoolManager` -> envoy-bin). Aborts cleanly
    /// when `token` cancels.
    pub fn spawn_idle_sweeper(
        self: &Arc<Self>,
        token: CancellationToken,
    ) -> tokio::task::JoinHandle<()> {
        let pool = Arc::clone(self);
        // 13.1 state-5 fold-in (REVIEW Cluster A I2): clamp interval to ≥1ms.
        // `tokio::time::interval(Duration::ZERO)` panics; today
        // `DEFAULT_IDLE_TIMEOUT = 60s` makes `idle_timeout / SWEEPER_DIVISOR =
        // 15s` (safe), but `H1Pool::new` accepts `idle_timeout: Duration`
        // publicly and SPEC §2 item-iii defers a future config-driven knob —
        // defensive clamp so a zero/sub-4ns idle_timeout cannot crash the
        // sweeper.
        let interval_period = (pool.idle_timeout / SWEEPER_DIVISOR).max(Duration::from_millis(1));
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(interval_period);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = token.cancelled() => return,
                    _ = tick.tick() => pool.sweep_once(),
                }
            }
        })
    }

    fn sweep_once(self: &Arc<Self>) {
        let now = Instant::now();
        // Collect evictions under `idle` lock first, then take `est` lock.
        // Separating the two locks avoids any chance of re-entrant ordering
        // issues with `acquire()`'s `idle`-then-`est` sequence.
        let evictions: Vec<(SocketAddr, u32)> = {
            let mut idle = self.idle.lock();
            let mut evictions: Vec<(SocketAddr, u32)> = Vec::new();
            for (endpoint, list) in idle.iter_mut() {
                let before = list.len();
                list.retain(|entry| now.duration_since(entry.last_returned) < self.idle_timeout);
                let evicted = before - list.len();
                if evicted > 0 {
                    evictions.push((*endpoint, evicted as u32));
                }
            }
            evictions
        };
        if evictions.is_empty() {
            return;
        }
        let mut est = self.established.lock();
        for (endpoint, evicted) in evictions {
            if let Some(n) = est.get_mut(&endpoint) {
                *n = n.saturating_sub(evicted);
                // 15 D4 (lock-in #6): clear cx_open when eviction drops the
                // per-endpoint count below max_connections.
                if *n < self.max_connections
                    && let Some(g) = &self.cx_open
                {
                    g.set(0);
                }
            }
            for _ in 0..evicted {
                self.cx_destroy.inc();
            }
        }
    }
}

/// Per-bootstrap registry of `Arc<H1Pool>` keyed by cluster name. Constructed
/// bin-side after `from_bootstrap`. The H1 HCM proxy arm looks up its pool via
/// `manager.get(cluster_name)`.
pub struct H1PoolManager {
    pools: HashMap<String, Arc<H1Pool>>,
    /// Idle-sweeper JoinHandles, one per pool. Owned for lifetime parity with
    /// envoy-bin's `health_scheduler.shutdown().await`; aborted on token
    /// cancel OR explicit `shutdown()`. 13.2 A-M1 closure: field renamed
    /// `_sweepers → sweepers` (the underscore prefix is no longer correct
    /// — the field is read by `shutdown()`); paired with the new
    /// `pub async fn shutdown(self)` method below.
    sweepers: Vec<tokio::task::JoinHandle<()>>,
}

// Hand-rolled `Debug` (rather than `#[derive]`): `H1Pool`'s internal
// `parking_lot::Mutex` + per-pool `Counter`/`Gauge` Arcs aren't reflected
// here — surface only the per-cluster pool names so that the parent
// `HCMConfig` `#[derive(Debug)]` (which carries `pool_mgr` as an
// `Option<Arc<H1PoolManager>>` at 13.1 Task 4) keeps compiling.
impl std::fmt::Debug for H1PoolManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("H1PoolManager")
            .field("clusters", &self.pools.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl H1PoolManager {
    /// Build the pool registry from the parsed bootstrap + the constructed
    /// `ClusterManager` (the latter is the source of the existing `Arc<Counter>`
    /// for `upstream_cx_total` + `Arc<Gauge>` for `upstream_cx_active`, both
    /// already registered at `from_bootstrap` time). One pool per cluster
    /// (default-enabled per §5.4 lock-in #2); H2 clusters' pools defer to 13.2.
    pub fn for_bootstrap(
        bootstrap: &envoy_config::Bootstrap,
        cluster_mgr: &envoy_cluster::ClusterManager,
        registry: Arc<envoy_stats::StatsRegistry>,
        token: CancellationToken,
    ) -> Result<Arc<Self>, envoy_stats::StatsError> {
        let mut pools: HashMap<String, Arc<H1Pool>> = HashMap::new();
        let mut sweepers: Vec<tokio::task::JoinHandle<()>> = Vec::new();
        for cfg in &bootstrap.static_resources.clusters {
            // 13.2 A-M4 closure: improved `.expect` message naming the
            // single-bootstrap-per-process invariant explicitly.
            let handle = cluster_mgr.get(&cfg.name).expect(
                "H1PoolManager::for_bootstrap requires cluster_mgr built from the same \
                 bootstrap (single-bootstrap-per-process invariant)",
            );
            if handle.upstream_protocol() != envoy_cluster::UpstreamProtocol::Http1 {
                continue;
            }
            let max_connections = cfg
                .circuit_breakers
                .as_ref()
                .and_then(|cb| cb.thresholds.first())
                .and_then(|t| t.max_connections)
                .unwrap_or(DEFAULT_MAX_CONNECTIONS);
            let max_pending_requests = cfg
                .circuit_breakers
                .as_ref()
                .and_then(|cb| cb.thresholds.first())
                .and_then(|t| t.max_pending_requests)
                .unwrap_or(DEFAULT_MAX_PENDING_REQUESTS);
            // 15 D3 (lock-in #4): register upstream_rq_pending_overflow ONLY when
            // circuit_breakers is configured (inert-when-unconfigured). Unconfigured
            // clusters get `None` — and never reach the gate (max_pending_requests
            // defaults to 1024). `Counter::new()` is pub(crate), so a throwaway
            // unregistered handle can't be built here; `Option` is the documented
            // fallback per the PLAN Step 6 caveat.
            let rq_pending_overflow = if cfg.circuit_breakers.is_some() {
                Some(registry.register_counter(&format!(
                    "cluster.{}.upstream_rq_pending_overflow",
                    cfg.name
                ))?)
            } else {
                None
            };
            // 15 D4 (lock-in #4): register upstream_cx_overflow +
            // circuit_breakers.default.cx_open ONLY when circuit_breakers is
            // configured (inert-when-unconfigured). `None` otherwise — the
            // increment/edge sites guard on `Some`. `Counter::new()`/`Gauge::new()`
            // are pub(crate), so a throwaway unregistered handle can't be built
            // here; `Option` is the documented fallback (mirrors
            // rq_pending_overflow).
            let cx_overflow = if cfg.circuit_breakers.is_some() {
                Some(
                    registry
                        .register_counter(&format!("cluster.{}.upstream_cx_overflow", cfg.name))?,
                )
            } else {
                None
            };
            let cx_open = if cfg.circuit_breakers.is_some() {
                Some(registry.register_gauge(&format!(
                    "cluster.{}.circuit_breakers.default.cx_open",
                    cfg.name
                ))?)
            } else {
                None
            };
            let cx_destroy =
                registry.register_counter(&format!("cluster.{}.upstream_cx_destroy", cfg.name))?;
            let cx_http1_total = registry
                .register_counter(&format!("cluster.{}.upstream_cx_http1_total", cfg.name))?;
            // Re-register cx_total + cx_active for the shared Arc (idempotent
            // same-kind contract — envoy-stats returns the same Arc on second register).
            let cx_total =
                registry.register_counter(&format!("cluster.{}.upstream_cx_total", cfg.name))?;
            let cx_active =
                registry.register_gauge(&format!("cluster.{}.upstream_cx_active", cfg.name))?;
            // 13.2 A-M2 closure: assert the gauge handle the pool just
            // got from the registry is the SAME Arc the cluster holds.
            // Holds under the single-bootstrap-per-process invariant
            // (the same `registry` was passed to both `from_bootstrap`
            // and `for_bootstrap`).
            debug_assert!(
                Arc::ptr_eq(&cx_active, handle.cx_active_arc()),
                "H1PoolManager: cx_active Arc mismatch for cluster '{}' — \
                 single-bootstrap-per-process invariant violated",
                cfg.name
            );
            let pool = H1Pool::new(
                cfg.name.clone(),
                max_connections,
                max_pending_requests,
                DEFAULT_IDLE_TIMEOUT,
                cx_total,
                cx_destroy,
                cx_http1_total,
                cx_active,
                rq_pending_overflow,
                cx_overflow,
                cx_open,
            );
            sweepers.push(pool.spawn_idle_sweeper(token.clone()));
            pools.insert(cfg.name.clone(), pool);
        }
        Ok(Arc::new(Self { pools, sweepers }))
    }

    /// Look up the pool for `cluster_name`. Returns `None` if no H1 cluster
    /// with that name exists.
    pub fn get(&self, cluster_name: &str) -> Option<&Arc<H1Pool>> {
        self.pools.get(cluster_name)
    }

    /// 13.2 A-M1 closure: explicit shutdown path. Aborts every sweeper
    /// handle + awaits each. Mirrors `envoy_health::Scheduler::shutdown`'s
    /// posture. Consumes `self`.
    pub async fn shutdown(mut self) {
        for handle in self.sweepers.drain(..) {
            handle.abort();
            let _ = handle.await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Per-test counter/gauge registration via a fresh registry.
    fn mk_pool(
        cluster: &str,
        max_connections: u32,
        idle_timeout: Duration,
    ) -> (
        Arc<H1Pool>,
        Arc<envoy_stats::Counter>,
        Arc<envoy_stats::Counter>,
        Arc<envoy_stats::Counter>,
        Arc<envoy_stats::Gauge>,
    ) {
        let registry = envoy_stats::StatsRegistry::new();
        let cx_total = registry
            .register_counter(&format!("cluster.{cluster}.upstream_cx_total"))
            .unwrap();
        let cx_destroy = registry
            .register_counter(&format!("cluster.{cluster}.upstream_cx_destroy"))
            .unwrap();
        let cx_http1_total = registry
            .register_counter(&format!("cluster.{cluster}.upstream_cx_http1_total"))
            .unwrap();
        let cx_active = registry
            .register_gauge(&format!("cluster.{cluster}.upstream_cx_active"))
            .unwrap();
        let pool = H1Pool::new(
            cluster.to_string(),
            max_connections,
            DEFAULT_MAX_PENDING_REQUESTS,
            idle_timeout,
            Arc::clone(&cx_total),
            Arc::clone(&cx_destroy),
            Arc::clone(&cx_http1_total),
            Arc::clone(&cx_active),
            None,
            None,
            None,
        );
        (pool, cx_total, cx_destroy, cx_http1_total, cx_active)
    }

    /// 15 D3: build a pool with a configured `max_pending_requests` + a
    /// registered `upstream_rq_pending_overflow` counter handle (the
    /// circuit-breakers-configured shape). Returns the pool + the counter
    /// handle so tests can assert the overflow count.
    fn mk_pool_pending(
        cluster: &str,
        max_connections: u32,
        max_pending_requests: u32,
    ) -> (Arc<H1Pool>, Arc<envoy_stats::Counter>) {
        let registry = envoy_stats::StatsRegistry::new();
        let cx_total = registry
            .register_counter(&format!("cluster.{cluster}.upstream_cx_total"))
            .unwrap();
        let cx_destroy = registry
            .register_counter(&format!("cluster.{cluster}.upstream_cx_destroy"))
            .unwrap();
        let cx_http1_total = registry
            .register_counter(&format!("cluster.{cluster}.upstream_cx_http1_total"))
            .unwrap();
        let cx_active = registry
            .register_gauge(&format!("cluster.{cluster}.upstream_cx_active"))
            .unwrap();
        let rq_pending_overflow = registry
            .register_counter(&format!("cluster.{cluster}.upstream_rq_pending_overflow"))
            .unwrap();
        let pool = H1Pool::new(
            cluster.to_string(),
            max_connections,
            max_pending_requests,
            Duration::from_secs(60),
            cx_total,
            cx_destroy,
            cx_http1_total,
            cx_active,
            Some(Arc::clone(&rq_pending_overflow)),
            None,
            None,
        );
        (pool, rq_pending_overflow)
    }

    /// 15 D4: hold-capable in-test backend. Accepts connections and holds
    /// them open WITHOUT ever responding (so the acquired connection stays
    /// established + in-flight, keeping the per-endpoint count at the cap).
    /// Returns the bound address + the `TcpListener`-owning `JoinHandle`
    /// (the caller binds it to a `_srv` so the accept loop lives for the
    /// test's duration). Used to drive the `cx_open` at-cap edge.
    async fn spawn_holding_backend() -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let mut held = Vec::new();
            loop {
                match listener.accept().await {
                    Ok((sock, _)) => held.push(sock), // hold open, never respond
                    Err(_) => return,
                }
            }
        });
        (addr, handle)
    }

    /// 15 D4: build a pool with a registered `upstream_cx_overflow` counter +
    /// `circuit_breakers.default.cx_open` gauge (the circuit-breakers-configured
    /// shape). Returns the pool + both handles so tests can assert the
    /// cap-overflow + edge-driven gauge semantics.
    #[allow(clippy::type_complexity)]
    fn mk_pool_cb(
        cluster: &str,
        max_connections: u32,
    ) -> (
        Arc<H1Pool>,
        Arc<envoy_stats::Counter>,
        Arc<envoy_stats::Gauge>,
    ) {
        let registry = envoy_stats::StatsRegistry::new();
        let cx_total = registry
            .register_counter(&format!("cluster.{cluster}.upstream_cx_total"))
            .unwrap();
        let cx_destroy = registry
            .register_counter(&format!("cluster.{cluster}.upstream_cx_destroy"))
            .unwrap();
        let cx_http1_total = registry
            .register_counter(&format!("cluster.{cluster}.upstream_cx_http1_total"))
            .unwrap();
        let cx_active = registry
            .register_gauge(&format!("cluster.{cluster}.upstream_cx_active"))
            .unwrap();
        let cx_overflow = registry
            .register_counter(&format!("cluster.{cluster}.upstream_cx_overflow"))
            .unwrap();
        let cx_open = registry
            .register_gauge(&format!(
                "cluster.{cluster}.circuit_breakers.default.cx_open"
            ))
            .unwrap();
        let pool = H1Pool::new(
            cluster.to_string(),
            max_connections,
            DEFAULT_MAX_PENDING_REQUESTS,
            Duration::from_secs(60),
            cx_total,
            cx_destroy,
            cx_http1_total,
            cx_active,
            None,
            Some(Arc::clone(&cx_overflow)),
            Some(Arc::clone(&cx_open)),
        );
        (pool, cx_overflow, cx_open)
    }

    /// In-process echo backend that responds to each request with a minimal
    /// 200 OK. Returns the bound address; accepts forever until dropped.
    async fn echo_backend() -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut sock, _) = listener.accept().await.unwrap();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    loop {
                        let n = sock.read(&mut buf).await.unwrap_or(0);
                        if n == 0 {
                            return;
                        }
                        if buf[..n].windows(4).any(|w| w == b"\r\n\r\n") {
                            let _ = sock
                                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: keep-alive\r\n\r\n")
                                .await;
                        }
                    }
                });
            }
        });
        addr
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn acquire_from_empty_pool_creates_connection_and_fires_counters() {
        let addr = echo_backend().await;
        let (pool, cx_total, _cx_destroy, cx_http1_total, cx_active) =
            mk_pool("c", 4, Duration::from_secs(60));
        let guard = pool.acquire(addr, "host.example").await.expect("acquire");
        assert_eq!(cx_total.value(), 1, "cx_total fires on connect-on-miss");
        assert_eq!(
            cx_http1_total.value(),
            1,
            "cx_http1_total fires on connect-on-miss"
        );
        assert_eq!(cx_active.value(), 1, "cx_active increments via guard");
        drop(guard);
        tokio::task::yield_now().await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(cx_active.value(), 0, "cx_active decrements on guard drop");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn acquire_after_return_reuses_idle_stream_without_incrementing_cx_total() {
        let addr = echo_backend().await;
        let (pool, cx_total, _cx_destroy, _cx_http1_total, _cx_active) =
            mk_pool("c", 4, Duration::from_secs(60));
        let g1 = pool.acquire(addr, "h").await.expect("acquire 1");
        drop(g1);
        tokio::time::sleep(Duration::from_millis(50)).await;
        let _g2 = pool.acquire(addr, "h").await.expect("acquire 2");
        assert_eq!(cx_total.value(), 1, "reuse must not re-fire cx_total");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn acquire_returns_overflow_when_at_cap() {
        let addr = echo_backend().await;
        let (pool, _, _, _, _) = mk_pool("c", 1, Duration::from_secs(60));
        let _g1 = pool.acquire(addr, "h").await.expect("first acquire");
        let err = pool
            .acquire(addr, "h")
            .await
            .expect_err("second acquire must overflow");
        assert!(matches!(err, PoolError::Overflow { ref cluster, max: 1 } if cluster == "c"));
    }

    /// 15 D3 (lock-in #7): under `max_pending_requests:0` the first
    /// connect-on-miss is rejected with `PoolError::PendingOverflow` BEFORE
    /// any connect (the backend is never dialed) and the
    /// `upstream_rq_pending_overflow` counter ticks to 1.
    #[tokio::test(flavor = "multi_thread")]
    async fn acquire_rejects_with_pending_overflow_when_max_pending_requests_zero() {
        let (pool, rq_pending_overflow) = mk_pool_pending("c", 1, 0);
        // Unroutable endpoint: must never be dialed. If the gate fails to
        // fire, the connect to 127.0.0.1:1 would error with Connect, not
        // PendingOverflow — so the assertion below catches a missing gate.
        let endpoint: SocketAddr = "127.0.0.1:1".parse().unwrap();
        let err = pool
            .acquire(endpoint, "c")
            .await
            .expect_err("max_pending_requests:0 must reject");
        assert!(
            matches!(err, PoolError::PendingOverflow { ref cluster } if cluster == "c"),
            "expected PendingOverflow, got {err:?}"
        );
        assert_eq!(
            rq_pending_overflow.value(),
            1,
            "upstream_rq_pending_overflow must read 1 after the reject"
        );
    }

    /// 15 D4 (lock-ins #5 + #6): `upstream_cx_overflow` increments on a cap-hit
    /// and `circuit_breakers.default.cx_open` is an edge-driven gauge — `set(1)`
    /// when an `established` increment reaches `max_connections` (at-cap
    /// inclusive), `set(0)` at the decrement edges that drop below the cap.
    /// Drives the `PoolGuard::Drop` destroy path (via `invalidate()`) to confirm
    /// the gauge returns to terminal-0.
    #[tokio::test(flavor = "multi_thread")]
    async fn cx_overflow_increments_and_cx_open_tracks_cap_edges() {
        let (backend_addr, _srv) = spawn_holding_backend().await;
        let (pool, cx_overflow, cx_open) = mk_pool_cb("c", 1);
        // First acquire connects (the holding backend accepts but never
        // responds) → established reaches the cap (1) → cx_open set to 1.
        let mut g1 = pool
            .acquire(backend_addr, "h")
            .await
            .expect("first acquire connects");
        assert_eq!(cx_open.value(), 1, "cx_open set at cap after first connect");
        assert_eq!(cx_overflow.value(), 0, "no overflow yet");
        // Second acquire overflows (no idle stream, established == cap) →
        // cx_overflow increments; cx_open unchanged (still at cap).
        let err = pool
            .acquire(backend_addr, "h")
            .await
            .expect_err("second acquire must overflow");
        assert!(
            matches!(err, PoolError::Overflow { ref cluster, max: 1 } if cluster == "c"),
            "expected Overflow, got {err:?}"
        );
        assert_eq!(cx_overflow.value(), 1, "cx_overflow ticks on cap-hit");
        assert_eq!(cx_open.value(), 1, "cx_open still at cap after overflow");
        // Drive the Drop destroy path: invalidate g1 so its Drop decrements
        // established below the cap → cx_open returns to terminal-0.
        g1.invalidate();
        drop(g1);
        assert_eq!(
            cx_open.value(),
            0,
            "cx_open returns to 0 at the destroy decrement edge"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn invalidate_destroys_stream_and_increments_cx_destroy() {
        let addr = echo_backend().await;
        let (pool, _cx_total, cx_destroy, _, _) = mk_pool("c", 4, Duration::from_secs(60));
        let mut g = pool.acquire(addr, "h").await.expect("acquire");
        g.invalidate();
        drop(g);
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(cx_destroy.value(), 1, "invalidate path fires cx_destroy");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn idle_sweeper_evicts_past_deadline_entries() {
        let addr = echo_backend().await;
        let (pool, _cx_total, cx_destroy, _, _) = mk_pool("c", 4, Duration::from_millis(100));
        let token = CancellationToken::new();
        let sweeper = pool.spawn_idle_sweeper(token.clone());
        let g = pool.acquire(addr, "h").await.expect("acquire");
        drop(g);
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(cx_destroy.value(), 0);
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(
            cx_destroy.value() >= 1,
            "sweeper must evict idle entry past deadline"
        );
        token.cancel();
        let _ = sweeper.await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn h1_pool_manager_registers_cx_destroy_and_cx_http1_total_per_h1_cluster() {
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: c1
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: c1
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 8080 } }
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let bootstrap = envoy_config::parse_bootstrap(yaml).expect("parse");
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let mgr = envoy_cluster::from_bootstrap(&bootstrap, Arc::clone(&registry))
            .await
            .expect("cluster mgr");
        let token = CancellationToken::new();
        let _pool_mgr =
            H1PoolManager::for_bootstrap(&bootstrap, &mgr, Arc::clone(&registry), token)
                .expect("pool mgr");
        let snapshot = registry.snapshot();
        assert!(
            snapshot
                .iter()
                .any(|(n, _)| n == "cluster.c1.upstream_cx_destroy"),
            "expected cluster.c1.upstream_cx_destroy in registry; got: {:?}",
            snapshot.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>()
        );
        assert!(
            snapshot
                .iter()
                .any(|(n, _)| n == "cluster.c1.upstream_cx_http1_total"),
            "expected cluster.c1.upstream_cx_http1_total in registry; got: {:?}",
            snapshot.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>()
        );
    }

    /// 13.2 A-I3 closure: post-mutex-switch, Drop is synchronous — an
    /// acquired-then-dropped stream is back in the idle list immediately
    /// (no `tokio::spawn` round-trip), so the very next `acquire()` on
    /// the same endpoint reuses it without re-firing `cx_total`. The
    /// pre-fix `tokio::spawn`-in-Drop shape required a `tokio::time::sleep`
    /// between drop and re-acquire to observe reuse; the post-fix shape
    /// does NOT.
    ///
    /// REPLACES the pre-13.2 `pool_guard_drop_outside_runtime_does_not_panic`
    /// test — that scenario (drop after runtime exit) is now structurally
    /// unreachable: sync Drop never spawns, so there's no "no reactor
    /// running" panic to guard against.
    #[tokio::test(flavor = "multi_thread")]
    async fn pool_guard_drop_is_synchronous_and_returns_to_pool_immediately() {
        let addr = echo_backend().await;
        let (pool, cx_total, _cx_destroy, _cx_http1_total, _cx_active) =
            mk_pool("c", 4, Duration::from_secs(60));
        let g1 = pool.acquire(addr, "h").await.expect("first acquire");
        assert_eq!(cx_total.value(), 1, "cx_total fires on first connect");
        drop(g1);
        // No yield_now, no sleep — sync Drop already returned the stream
        // to idle. The next acquire MUST reuse without bumping cx_total.
        let _g2 = pool.acquire(addr, "h").await.expect("immediate re-acquire");
        assert_eq!(
            cx_total.value(),
            1,
            "immediate re-acquire after sync Drop must reuse idle stream (cx_total unchanged)"
        );
    }

    /// 13.2 A-I3 race regression: under the synchronous parking_lot
    /// Mutex, an `invalidate()`-flagged guard whose `drop_task` has
    /// joined MUST have run its destroy-path bookkeeping
    /// (established-decrement + cx_destroy.inc) by the time the join
    /// returns. The follow-up acquire therefore sees
    /// `established < max_connections` and succeeds.
    ///
    /// Pre-fix, the `tokio::spawn` in `Drop` deferred the
    /// established-decrement: even after `drop_task.await` returned, the
    /// spawned async closure had not yet run, so the next acquire could
    /// see `established == max_connections` and return spurious Overflow.
    /// Post-fix, Drop is fully synchronous — the decrement lands BEFORE
    /// Drop returns; `drop_task.await` is a structural happens-before
    /// boundary for the established-decrement, not just for the guard's
    /// scope exit.
    #[tokio::test(flavor = "multi_thread")]
    async fn pool_acquire_after_concurrent_release_does_not_yield_spurious_overflow() {
        let addr = echo_backend().await;
        let (pool, _cx_total, _cx_destroy, _cx_http1_total, _cx_active) =
            mk_pool("c", 1, Duration::from_secs(60));
        for i in 0..32 {
            let pool_a = Arc::clone(&pool);
            let pool_b = Arc::clone(&pool);
            // First guard — mark invalidate so its Drop drives the
            // destroy-path (decrement established) rather than the
            // return-to-idle path. This is the path that was previously
            // deferred via `tokio::spawn` and that the A-I3 spurious
            // Overflow originated from.
            let mut g1 = pool_a.acquire(addr, "h").await.expect("pre-acquire");
            g1.invalidate();
            // Drop the guard on a separate task and AWAIT its
            // completion. Under sync Drop, established is decremented
            // synchronously inside `drop(g1)` — so by the time the join
            // returns, the slot is structurally free.
            let drop_task = tokio::spawn(async move {
                drop(g1);
            });
            let _ = drop_task.await;
            // Follow-up acquire on the same endpoint must succeed.
            let result = pool_b.acquire(addr, "h").await;
            assert!(
                result.is_ok(),
                "iter {i}: expected acquire after sync Drop to succeed, got {result:?}"
            );
        }
    }

    /// 13.2 Task 1 fold-in (code-quality review IMPORTANT): the
    /// existing structural test
    /// (`pool_acquire_after_concurrent_release_does_not_yield_spurious_overflow`)
    /// runs 32 iterations and asserts the happens-before invariant
    /// after `drop_task.await`. The reviewer correctly observed that
    /// invariant alone would also hold under async Drop AS LONG AS the
    /// spawned drop task completes its full body before its `await`
    /// returns. Pre-fix, `drop(g1)` inside the spawned closure
    /// INTERNALLY spawned a SECOND task (via the
    /// `Handle::try_current()` branch) to do the established-decrement
    /// — so the outer `drop_task.await` could return BEFORE the
    /// established-decrement landed; with low iteration counts that
    /// race window was easy to miss. THIS test runs the SAME shape at
    /// 1000 iterations to make the pre-fix race window probabilistically
    /// detectable. Post-fix Drop is fully synchronous (no inner spawn)
    /// so the 1000-iter loop must produce 0 spurious Overflows.
    ///
    /// Runtime: well under 10s on a modern dev box (the echo_backend
    /// returns each response in microseconds; the pool's parking_lot
    /// locks are uncontended).
    #[tokio::test(flavor = "multi_thread")]
    async fn pool_acquire_after_concurrent_release_1000_iterations_zero_spurious_overflows() {
        let addr = echo_backend().await;
        let (pool, _cx_total, _cx_destroy, _cx_http1_total, _cx_active) =
            mk_pool("stress", 1, Duration::from_secs(60));
        let mut spurious_overflows = 0;
        for _ in 0..1000 {
            let pool_a = Arc::clone(&pool);
            let pool_b = Arc::clone(&pool);
            // Pre-acquire + invalidate so the drop runs the destroy-path
            // (decrement established) — the path the A-I3 race lived on.
            let mut g1 = pool_a.acquire(addr, "h").await.expect("pre-acquire");
            g1.invalidate();
            let drop_task = tokio::spawn(async move {
                drop(g1);
            });
            let _ = drop_task.await;
            match pool_b.acquire(addr, "h").await {
                Ok(_g2) => { /* expected post-fix */ }
                Err(PoolError::Overflow { .. }) => spurious_overflows += 1,
                Err(other) => panic!("unexpected error: {other:?}"),
            }
        }
        assert_eq!(
            spurious_overflows, 0,
            "post-fix sync Drop must produce 0 spurious Overflows over 1000 iterations \
             (saw {spurious_overflows} — race window not fully closed)"
        );
    }

    /// 13.1 state-5 fold-in (REVIEW Cluster A I2): a zero or sub-4ns
    /// `idle_timeout` must not panic the sweeper. Pre-fix,
    /// `tokio::time::interval(Duration::ZERO)` panicked at sweeper spawn;
    /// post-fix the interval is clamped to ≥1ms.
    #[tokio::test(flavor = "multi_thread")]
    async fn spawn_idle_sweeper_with_zero_idle_timeout_does_not_panic() {
        let (pool, _cx_total, _cx_destroy, _cx_http1_total, _cx_active) =
            mk_pool("c", 4, Duration::ZERO);
        let token = CancellationToken::new();
        let sweeper = pool.spawn_idle_sweeper(token.clone());
        // Let the sweeper tick at least once with the clamped 1ms interval.
        tokio::time::sleep(Duration::from_millis(10)).await;
        token.cancel();
        sweeper.await.expect("sweeper must exit cleanly, not panic");
    }
}
