//! 13.2 D5: per-cluster H2 connection pool. Holds TCP connections each
//! multiplexing many concurrent H2 streams; `acquire()` returns a guard
//! to a stream slot on an existing connection with remaining capacity
//! (subject to peer's SETTINGS_MAX_CONCURRENT_STREAMS and the cluster's
//! `circuit_breakers.max_connections` cap); otherwise creates a new H2
//! connection.
//!
//! Architectural sibling of `envoy_http1::pool` (13.1 Task 3) — the
//! external-manager + RAII-guard + idle-sweeper patterns carry over
//! verbatim; the H2-specific differences are: (1) one connection
//! multiplexes many streams (per-entry `active_streams: AtomicU32`); (2)
//! `ClientStream` is `Clone` so the per-stream `H2PoolGuard` holds a
//! fresh `SendRequest` clone, not a borrow; (3) Drop is synchronous (the
//! H2 pool's mutexes are `parking_lot::Mutex` — no `tokio::spawn` in
//! Drop).
//!
//! The synchronous-Drop design is the joint H1+H2 close-out of the 13.1
//! REVIEW Cluster A-I3 deferred-Important (spurious-overflow race under
//! concurrent acquire/release). The H1 pool migrates to the same shape
//! at this task — see `crates/envoy-http1/src/pool.rs` for the parallel
//! H1 changes.

#![allow(clippy::type_complexity)]

use crate::client::{Client, ClientStream};
use crate::error::Http2Error;
use envoy_cluster::ConnGaugeGuard;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// Phase-13 hardcoded H2 pool defaults (parent-13 SPEC §6.2 item-ii +
/// 13.1 default parity).
const DEFAULT_MAX_CONNECTIONS: u32 = 1024;
/// 15 D3: default `max_pending_requests` for clusters without circuit-breakers
/// config. Matches Envoy's default + the as-today behavior (the reject gate
/// never fires unless explicitly set to 0). See lock-in #4. Mirrors the H1
/// pool's `DEFAULT_MAX_PENDING_REQUESTS`.
const DEFAULT_MAX_PENDING_REQUESTS: u32 = 1024;
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(60);
/// Sweeper tick interval: `idle_timeout / 4` (15s at the default 60s timeout).
const SWEEPER_DIVISOR: u32 = 4;
/// RFC 7540 §6.5.2 default for `SETTINGS_MAX_CONCURRENT_STREAMS` when the
/// peer has not sent a SETTINGS frame. Upstream Envoy v1.33 uses the same
/// default per parent-13 SPEC §6.2 item-vi.
const DEFAULT_MAX_CONCURRENT_STREAMS: u32 = 100;

/// Errors returned by `H2Pool::acquire`.
#[derive(Debug, thiserror::Error)]
pub enum PoolError {
    /// Pool is at `max_connections` AND every existing connection is at its
    /// peer-advertised `SETTINGS_MAX_CONCURRENT_STREAMS` cap.
    #[error("upstream H2 pool overflow: cluster='{cluster}', max_connections={max}")]
    Overflow { cluster: String, max: u32 },
    /// Pool's `max_pending_requests` is 0 and a new connection must be established
    /// (no idle stream slot to reuse). Envoy reject-on-establish parity
    /// (ADR-0043 §6.2 finding 1). H2 sibling of H1's `PoolError::PendingOverflow`.
    #[error("upstream pending-request overflow: cluster='{cluster}' (max_pending_requests=0)")]
    PendingOverflow { cluster: String },
    /// `Client::connect()` failed on the connect-on-miss branch.
    #[error(transparent)]
    Connect(#[from] Http2Error),
}

/// One H2 multiplexing connection. `active_streams` counts in-flight
/// streams; `last_idle` records when the count last hit zero (sweeper input).
struct H2PoolEntry {
    client_stream: ClientStream,
    max_streams: u32,
    active_streams: AtomicU32,
    last_idle: parking_lot::Mutex<Option<Instant>>,
}

/// Per-acquire RAII handle. Owns one `ConnGaugeGuard` (gauge decrements on
/// drop) + a `Clone`d `ClientStream` (sharing the underlying H2 connection
/// with the rest of the streams routed through the same `H2PoolEntry`).
/// Drop decrements `active_streams`; on `invalidate()` it also removes the
/// entry from the pool + decrements `established` + increments
/// `cx_destroy`. Mutexes are `parking_lot::Mutex` (synchronous) so Drop
/// has NO `.await` / `tokio::spawn` path — closes 13.1 REVIEW Cluster A-I3.
pub struct H2PoolGuard {
    pool: Arc<H2Pool>,
    endpoint: SocketAddr,
    entry: Arc<H2PoolEntry>,
    client_stream: ClientStream,
    _cx_active_guard: ConnGaugeGuard,
    invalidated: bool,
}

// Hand-rolled `Debug` (rather than `#[derive]`) — `ConnGaugeGuard` and
// `H2PoolEntry` don't impl Debug; mirrors `envoy_http1::pool::PoolGuard`'s
// posture.
impl std::fmt::Debug for H2PoolGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("H2PoolGuard")
            .field("cluster", &self.pool.cluster_name)
            .field("endpoint", &self.endpoint)
            .field("invalidated", &self.invalidated)
            .finish_non_exhaustive()
    }
}

impl H2PoolGuard {
    /// Borrow the underlying (cloned) `ClientStream` mutably for
    /// `send_request`. Mirrors `envoy_http1::pool::PoolGuard::stream_mut`'s
    /// posture, but no panic-on-invalidate guard — invalidated H2 guards
    /// remain usable until Drop (the underlying conn may still be alive;
    /// invalidate just flags the entry for destruction).
    pub fn client_stream_mut(&mut self) -> &mut ClientStream {
        &mut self.client_stream
    }

    /// Mark the underlying entry for destruction. Drop will remove the
    /// `H2PoolEntry` from the pool's connections list, decrement
    /// `established`, and increment `cx_destroy`. Call on any
    /// protocol-level error that suggests the connection is broken.
    pub fn invalidate(&mut self) {
        self.invalidated = true;
    }
}

impl Drop for H2PoolGuard {
    fn drop(&mut self) {
        // Two distinct paths — see the comments below. The split is
        // load-bearing: the INVALIDATE path must serialize the
        // `active_streams.fetch_sub` AND the list-eviction under the
        // SAME `connections` lock that `acquire()`'s Phase-1 walker
        // takes, otherwise a TOCTOU race lets a concurrent acquire
        // claim a slot on an entry we're about to evict (Task 1
        // code-quality review CRITICAL fix).
        if self.invalidated {
            // INVALIDATE PATH (race-critical):
            //
            // Take the `connections` lock BEFORE decrementing
            // `active_streams`. A Phase-1 walker in `acquire()` holds
            // this same lock while iterating the per-endpoint list and
            // CAS'ing on each entry's `active_streams` — so while we
            // hold the lock here, no walker can reach this entry's CAS
            // site. Under the lock we (1) decrement `active_streams`,
            // (2) retain the entry out of the per-endpoint list. Once
            // the entry is gone from the list, a future walker can no
            // longer see it. Walkers already inside the lock either
            // already passed this entry (their CAS happened pre-evict;
            // benign — see the analysis below) or hadn't reached it
            // yet (post-retain they'll skip the now-absent entry).
            //
            // Concretely, the pre-fix race was:
            //   T_B `fetch_sub` (no lock) → potentially 0
            //   T_A grabs `connections` lock, CAS'es this entry 0→1
            //       (claims a slot), releases lock
            //   T_B grabs `connections` lock, retains entry out of
            //       list, decrements `established` — but T_A still
            //       holds an `H2PoolGuard` against the orphaned entry,
            //       with `cx_destroy` falsely fired and
            //       `max_connections` accounting off-by-one.
            // Post-fix: T_B holds the lock for the entire decrement +
            // retain, so T_A's CAS cannot land between them.
            let mut conns = self.pool.connections.lock();
            let prev = self.entry.active_streams.fetch_sub(1, Ordering::AcqRel);
            debug_assert!(prev >= 1, "H2PoolGuard::drop with active_streams == 0");
            if let Some(list) = conns.get_mut(&self.endpoint) {
                list.retain(|e| !Arc::ptr_eq(e, &self.entry));
            }
            drop(conns);
            // `established` + `cx_destroy` are pool-level book-keeping;
            // they don't gate slot-claim, so we can release `connections`
            // before taking `established`.
            {
                let mut est = self.pool.established.lock();
                if let Some(n) = est.get_mut(&self.endpoint) {
                    *n = n.saturating_sub(1);
                    // 15 D4 (lock-in #6): clear cx_open when this invalidate
                    // eviction drops the per-endpoint count below max_connections.
                    if *n < self.pool.max_connections
                        && let Some(g) = &self.pool.cx_open
                    {
                        g.set(0);
                    }
                }
            }
            self.pool.cx_destroy.inc();
        } else {
            // RETURN-TO-POOL PATH (no eviction):
            //
            // No `connections` lock needed — we're not evicting the
            // entry, just releasing one stream slot back to its
            // multiplex pool. `active_streams.fetch_sub` outside any
            // lock is benign:
            //   * If a concurrent Phase-1 walker CAS'es this entry's
            //     `active_streams` from N to N+1 before our fetch_sub
            //     lands, the final value is the same and no eviction
            //     happens — fine.
            //   * The `last_idle` write (only when `prev == 1`, i.e. we
            //     transitioned to 0) is benign by the sweeper's
            //     early-return: `sweep_once` checks
            //     `active_streams.load() > 0` BEFORE consulting
            //     `last_idle`, so a stale `Some(now)` written after a
            //     concurrent claim already bumped `active_streams`
            //     back to ≥1 never causes spurious eviction.
            //   * `acquire()`'s `try_claim_stream_slot` writes
            //     `last_idle = None` AFTER a successful 0→1 CAS, but
            //     even without that the sweeper's `active_streams != 0`
            //     check is the load-bearing guard.
            let prev = self.entry.active_streams.fetch_sub(1, Ordering::AcqRel);
            debug_assert!(prev >= 1, "H2PoolGuard::drop with active_streams == 0");
            if prev == 1 {
                *self.entry.last_idle.lock() = Some(Instant::now());
            }
        }
        // _cx_active_guard's Drop fires here → upstream_cx_active.dec().
    }
}

/// One pool per cluster. Holds the per-endpoint connection list (each
/// entry is one H2 multiplexing connection) + the established-count
/// counter (idle + in-flight) for `max_connections` enforcement.
pub struct H2Pool {
    cluster_name: String,
    max_connections: u32,
    /// 15 D3: `max_pending_requests` cap. Only `0` (no-queue) is meaningful at
    /// phase-15 scope (the validator rejects `> 0`); `0` rejects every
    /// connect-on-miss with `PendingOverflow`. Defaults to
    /// `DEFAULT_MAX_PENDING_REQUESTS` (1024) for unconfigured clusters → the
    /// gate never fires (lock-in #4). Mirrors the H1 pool field.
    max_pending_requests: u32,
    idle_timeout: Duration,
    /// Per-endpoint multiplexing-connection list. `parking_lot::Mutex` —
    /// synchronous — so Drop can release locks without a runtime (joint
    /// A-I3 close with the H1 pool).
    connections: parking_lot::Mutex<HashMap<SocketAddr, Vec<Arc<H2PoolEntry>>>>,
    /// Per-endpoint total established conn count.
    established: parking_lot::Mutex<HashMap<SocketAddr, u32>>,
    /// Per-cluster `upstream_cx_total` — shared Arc with `Cluster.cx_total`
    /// (the H2 pool's connect-on-miss is the sole incrementer at 13.2 per
    /// the parallel of 13.1 lock-in #6).
    cx_total: Arc<envoy_stats::Counter>,
    /// Per-cluster `upstream_cx_destroy` — incremented at every entry eviction.
    cx_destroy: Arc<envoy_stats::Counter>,
    /// Per-cluster `upstream_cx_http2_total` — incremented at every H2 connect-on-miss.
    cx_http2_total: Arc<envoy_stats::Counter>,
    /// Per-cluster `upstream_cx_active` gauge handle — shared Arc with `Cluster.cx_active`.
    /// Each `H2PoolGuard` owns a `ConnGaugeGuard` created via this handle.
    cx_active: Arc<envoy_stats::Gauge>,
    /// 15 D3: per-cluster `upstream_rq_pending_overflow` counter, registered
    /// ONLY for clusters whose `circuit_breakers` is configured (lock-in #4 —
    /// inert-when-unconfigured). `None` for unconfigured clusters; the reject
    /// gate short-circuits on `max_pending_requests != 0` (default 1024) before
    /// this is ever touched. (`envoy_stats::Counter::new()` is `pub(crate)`, so
    /// a throwaway unregistered handle cannot be built cross-crate — the
    /// `Option` is the documented fallback, mirroring the H1 pool.)
    rq_pending_overflow: Option<Arc<envoy_stats::Counter>>,
    /// 15 D4: per-cluster `upstream_cx_overflow` counter (lock-in #5),
    /// incremented at the SOLE cap-check branch in `acquire()` (Phase 2) when
    /// the per-endpoint `established` count is already at `max_connections`.
    /// Registered ONLY for circuit-breakers-configured clusters
    /// (inert-when-unconfigured); `None` otherwise. Guarded with
    /// `if let Some(h) = &self.cx_overflow`. Mirrors the H1 pool.
    cx_overflow: Option<Arc<envoy_stats::Counter>>,
    /// 15 D4: per-cluster `circuit_breakers.default.cx_open` gauge (lock-in #6),
    /// edge-driven (NOT polled): `set(1)` when an `established` increment makes
    /// the per-endpoint count reach `max_connections` (at-cap inclusive);
    /// `set(0)` at each decrement edge that drops below the cap (the
    /// `H2PoolGuard::Drop` invalidate path, the connect-failure rollback, the
    /// idle-sweeper eviction). All edge updates run UNDER the held `established`
    /// lock. Registered ONLY for circuit-breakers-configured clusters; `None`
    /// otherwise — guarded with `if let Some(g) = &self.cx_open`. Terminal-0
    /// (returns to 0 after drain) so a post-settle scrape is deterministic.
    /// NOTE: per-cluster gauge but `established` is per-endpoint; for the
    /// single-endpoint fixtures they coincide (multi-endpoint reconciliation
    /// defers — lock-in #6). Mirrors the H1 pool.
    cx_open: Option<Arc<envoy_stats::Gauge>>,
}

// Hand-rolled `Debug` — see `H1PoolManager`'s rationale; mirrors that
// posture so `HCMConfig`-style parent `#[derive(Debug)]` keeps compiling.
impl std::fmt::Debug for H2Pool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("H2Pool")
            .field("cluster_name", &self.cluster_name)
            .field("max_connections", &self.max_connections)
            .field("idle_timeout", &self.idle_timeout)
            .finish_non_exhaustive()
    }
}

impl H2Pool {
    /// Build a new pool. `cx_total`/`cx_active` come from the existing
    /// cluster stat handles (shared `Arc`); `cx_destroy`/`cx_http2_total`
    /// are registered by the caller (see `H2PoolManager::for_bootstrap`).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cluster_name: String,
        max_connections: u32,
        max_pending_requests: u32,
        idle_timeout: Duration,
        cx_total: Arc<envoy_stats::Counter>,
        cx_destroy: Arc<envoy_stats::Counter>,
        cx_http2_total: Arc<envoy_stats::Counter>,
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
            connections: parking_lot::Mutex::new(HashMap::new()),
            established: parking_lot::Mutex::new(HashMap::new()),
            cx_total,
            cx_destroy,
            cx_http2_total,
            cx_active,
            rq_pending_overflow,
            cx_overflow,
            cx_open,
        })
    }

    /// Acquire a stream slot on a connection to `endpoint`. Reuses an
    /// existing connection with remaining stream capacity if any; otherwise
    /// creates a new H2 connection (subject to `max_connections`). On
    /// overflow + all connections at their stream cap, returns
    /// `PoolError::Overflow`.
    pub async fn acquire(
        self: &Arc<Self>,
        endpoint: SocketAddr,
        host: &str,
    ) -> Result<H2PoolGuard, PoolError> {
        // Phase 1: try to claim a stream slot on an existing entry. Walk
        // the per-endpoint list under the connections lock; for each entry
        // attempt a compare_exchange_weak on active_streams against
        // max_streams.
        {
            let conns = self.connections.lock();
            if let Some(list) = conns.get(&endpoint) {
                for entry in list.iter() {
                    if Self::try_claim_stream_slot(entry) {
                        // Successful claim. Clone the SendRequest handle +
                        // bind cx_active_guard. Drop the lock by letting
                        // the scope exit.
                        let entry = Arc::clone(entry);
                        let client_stream = entry.client_stream.clone();
                        let _cx_active_guard = self.acquire_cx_active_guard();
                        drop(conns);
                        return Ok(H2PoolGuard {
                            pool: Arc::clone(self),
                            endpoint,
                            entry,
                            client_stream,
                            _cx_active_guard,
                            invalidated: false,
                        });
                    }
                }
            }
        }
        // 15 D3 (lock-in #7): max_pending_requests:0 reject-on-establish. No
        // existing entry had a free stream slot → a new connection must be
        // established; under max_pending_requests:0 Envoy rejects before any
        // connect (ADR-0043 §6.2 finding 1). Fires BEFORE the Phase-2 cap-check
        // so upstream_cx_overflow stays 0 (no connection demand reaches the
        // cap). For unconfigured clusters max_pending_requests defaults to
        // 1024, so this branch is dead and the `rq_pending_overflow` Option is
        // never touched. Mirrors the H1 pool gate.
        if self.max_pending_requests == 0 {
            if let Some(counter) = &self.rq_pending_overflow {
                counter.inc();
            }
            return Err(PoolError::PendingOverflow {
                cluster: self.cluster_name.clone(),
            });
        }
        // Phase 2: at-cap on all existing entries → check + reserve a new
        // connection slot under the established lock.
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
        // Phase 3: release locks; H2 handshake is the slow path.
        let client_stream = match Client::connect(endpoint, host).await {
            Ok(s) => s,
            Err(e) => {
                // Roll back the established increment.
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
        // Phase 4: register the new entry. cx_total + cx_http2_total fire
        // here (lock-in #6 mirror — sole incrementer is connect-on-miss).
        self.cx_total.inc();
        self.cx_http2_total.inc();
        let entry = Arc::new(H2PoolEntry {
            client_stream: client_stream.clone(),
            max_streams: DEFAULT_MAX_CONCURRENT_STREAMS,
            active_streams: AtomicU32::new(1),
            last_idle: parking_lot::Mutex::new(None),
        });
        {
            let mut conns = self.connections.lock();
            conns.entry(endpoint).or_default().push(Arc::clone(&entry));
        }
        let _cx_active_guard = self.acquire_cx_active_guard();
        Ok(H2PoolGuard {
            pool: Arc::clone(self),
            endpoint,
            entry,
            client_stream,
            _cx_active_guard,
            invalidated: false,
        })
    }

    /// Try to atomically increment `active_streams` if it's currently
    /// below `max_streams`. Returns true on success (caller has claimed
    /// one stream slot). Uses `compare_exchange_weak` per RFC-style
    /// spurious-failure loop semantics.
    fn try_claim_stream_slot(entry: &H2PoolEntry) -> bool {
        let mut cur = entry.active_streams.load(Ordering::Acquire);
        loop {
            if cur >= entry.max_streams {
                return false;
            }
            match entry.active_streams.compare_exchange_weak(
                cur,
                cur + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    // If we transitioned 0 → 1 the entry is no longer idle.
                    if cur == 0 {
                        *entry.last_idle.lock() = None;
                    }
                    return true;
                }
                Err(observed) => {
                    cur = observed;
                }
            }
        }
    }

    /// Internal: build a `ConnGaugeGuard` for `cx_active` via inc+wrap.
    /// Mirrors `envoy_http1::pool::H1Pool::acquire_cx_active_guard` — the
    /// pool doesn't hold a `Cluster` reference; the inc+wrap pattern is
    /// duplicated here against the shared `Arc<Gauge>`.
    fn acquire_cx_active_guard(&self) -> ConnGaugeGuard {
        self.cx_active.inc();
        ConnGaugeGuard::from_gauge(Arc::clone(&self.cx_active))
    }

    /// Spawn the idle-timeout sweeper task. The returned `JoinHandle` is
    /// owned by the caller (typically `H2PoolManager` -> envoy-bin).
    /// Aborts cleanly when `token` cancels.
    pub fn spawn_idle_sweeper(
        self: &Arc<Self>,
        token: CancellationToken,
    ) -> tokio::task::JoinHandle<()> {
        let pool = Arc::clone(self);
        // 13.2 mirrors 13.1's state-5 fold-in interval clamp:
        // `tokio::time::interval(Duration::ZERO)` panics; defensive clamp
        // so a zero/sub-4ns `idle_timeout` cannot crash the sweeper.
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

    /// One sweep pass. Walks per-endpoint entries; evicts any entry whose
    /// `active_streams == 0` AND `last_idle` is older than `idle_timeout`.
    /// Synchronous — no `.await` / lock-across-await — joint A-I3 close.
    fn sweep_once(self: &Arc<Self>) {
        let now = Instant::now();
        let evictions: Vec<(SocketAddr, u32)> = {
            let mut conns = self.connections.lock();
            let mut evictions: Vec<(SocketAddr, u32)> = Vec::new();
            for (endpoint, list) in conns.iter_mut() {
                let before = list.len();
                list.retain(|entry| {
                    if entry.active_streams.load(Ordering::Acquire) > 0 {
                        return true;
                    }
                    let last_idle = *entry.last_idle.lock();
                    match last_idle {
                        Some(t) => now.duration_since(t) < self.idle_timeout,
                        // No `last_idle` recorded yet (never returned to
                        // idle) — keep.
                        None => true,
                    }
                });
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

/// Per-bootstrap registry of `Arc<H2Pool>` keyed by cluster name.
/// Constructed bin-side after `from_bootstrap`. The H2 HCM proxy arm
/// looks up its pool via `manager.get(cluster_name)`.
pub struct H2PoolManager {
    pools: HashMap<String, Arc<H2Pool>>,
    /// Idle-sweeper JoinHandles, one per pool. Owned for lifetime parity
    /// with envoy-bin's `health_scheduler.shutdown().await`; aborted on
    /// token cancel OR explicit `shutdown()`. 13.2 A-M1 closure: field
    /// rename `_sweepers → sweepers` + `pub async fn shutdown(self)` —
    /// mirrors `envoy_health::Scheduler::shutdown`.
    sweepers: Vec<tokio::task::JoinHandle<()>>,
}

// Hand-rolled `Debug` — surface only the per-cluster pool names so that
// any parent struct holding `Arc<H2PoolManager>` with `#[derive(Debug)]`
// keeps compiling.
impl std::fmt::Debug for H2PoolManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("H2PoolManager")
            .field("clusters", &self.pools.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl H2PoolManager {
    /// Build the H2 pool registry from the parsed bootstrap + the
    /// constructed `ClusterManager`. Mirrors `H1PoolManager::for_bootstrap`
    /// modulo the protocol filter (`UpstreamProtocol::Http2`).
    ///
    /// 13.2 A-M2 closure: `Arc::ptr_eq` debug-assert at the gauge wiring
    /// site. 13.2 A-M4 closure: the `.expect` message names the
    /// single-bootstrap-per-process invariant explicitly.
    pub fn for_bootstrap(
        bootstrap: &envoy_config::Bootstrap,
        cluster_mgr: &envoy_cluster::ClusterManager,
        registry: Arc<envoy_stats::StatsRegistry>,
        token: CancellationToken,
    ) -> Result<Arc<Self>, envoy_stats::StatsError> {
        let mut pools: HashMap<String, Arc<H2Pool>> = HashMap::new();
        let mut sweepers: Vec<tokio::task::JoinHandle<()>> = Vec::new();
        for cfg in bootstrap.all_clusters() {
            let handle = cluster_mgr.get(&cfg.name).expect(
                "H2PoolManager::for_bootstrap requires cluster_mgr built from the same \
                 bootstrap (single-bootstrap-per-process invariant)",
            );
            if handle.upstream_protocol() != envoy_cluster::UpstreamProtocol::Http2 {
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
            // 15 D3/D4 (lock-in #4): register the three circuit-breaker stats
            // ONLY when circuit_breakers is configured (inert-when-unconfigured).
            // Unconfigured clusters get `None` — and never reach the gate
            // (max_pending_requests defaults to 1024) / the guarded increment
            // sites. `Counter::new()`/`Gauge::new()` are pub(crate), so a
            // throwaway unregistered handle can't be built here; `Option` is the
            // documented fallback (mirrors the H1 pool).
            let rq_pending_overflow = if cfg.circuit_breakers.is_some() {
                Some(registry.register_counter(&format!(
                    "cluster.{}.upstream_rq_pending_overflow",
                    cfg.name
                ))?)
            } else {
                None
            };
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
            let cx_http2_total = registry
                .register_counter(&format!("cluster.{}.upstream_cx_http2_total", cfg.name))?;
            // Re-register cx_total + cx_active for the shared Arc
            // (idempotent same-kind contract — envoy-stats returns the
            // same Arc on second register).
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
                "H2PoolManager: cx_active Arc mismatch for cluster '{}' — \
                 single-bootstrap-per-process invariant violated",
                cfg.name
            );
            let pool = H2Pool::new(
                cfg.name.clone(),
                max_connections,
                max_pending_requests,
                DEFAULT_IDLE_TIMEOUT,
                cx_total,
                cx_destroy,
                cx_http2_total,
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

    /// Look up the pool for `cluster_name`. Returns `None` if no H2
    /// cluster with that name exists.
    pub fn get(&self, cluster_name: &str) -> Option<&Arc<H2Pool>> {
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
    use bytes::Bytes;
    use tokio::net::TcpListener;

    /// Per-test counter/gauge registration via a fresh registry. Returns
    /// the pool + all 4 shared stat handles. Mirrors the H1 `mk_pool`
    /// helper.
    fn mk_pool(
        cluster: &str,
        max_connections: u32,
        idle_timeout: Duration,
    ) -> (
        Arc<H2Pool>,
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
        let cx_http2_total = registry
            .register_counter(&format!("cluster.{cluster}.upstream_cx_http2_total"))
            .unwrap();
        let cx_active = registry
            .register_gauge(&format!("cluster.{cluster}.upstream_cx_active"))
            .unwrap();
        let pool = H2Pool::new(
            cluster.to_string(),
            max_connections,
            DEFAULT_MAX_PENDING_REQUESTS,
            idle_timeout,
            Arc::clone(&cx_total),
            Arc::clone(&cx_destroy),
            Arc::clone(&cx_http2_total),
            Arc::clone(&cx_active),
            None,
            None,
            None,
        );
        (pool, cx_total, cx_destroy, cx_http2_total, cx_active)
    }

    /// 15 D3: build a pool with a configured `max_pending_requests` + a
    /// registered `upstream_rq_pending_overflow` counter handle (the
    /// circuit-breakers-configured shape). Returns the pool + the counter
    /// handle so tests can assert the overflow count. Mirrors the H1
    /// `mk_pool_pending` helper.
    fn mk_pool_pending(
        cluster: &str,
        max_connections: u32,
        max_pending_requests: u32,
    ) -> (Arc<H2Pool>, Arc<envoy_stats::Counter>) {
        let registry = envoy_stats::StatsRegistry::new();
        let cx_total = registry
            .register_counter(&format!("cluster.{cluster}.upstream_cx_total"))
            .unwrap();
        let cx_destroy = registry
            .register_counter(&format!("cluster.{cluster}.upstream_cx_destroy"))
            .unwrap();
        let cx_http2_total = registry
            .register_counter(&format!("cluster.{cluster}.upstream_cx_http2_total"))
            .unwrap();
        let cx_active = registry
            .register_gauge(&format!("cluster.{cluster}.upstream_cx_active"))
            .unwrap();
        let rq_pending_overflow = registry
            .register_counter(&format!("cluster.{cluster}.upstream_rq_pending_overflow"))
            .unwrap();
        let pool = H2Pool::new(
            cluster.to_string(),
            max_connections,
            max_pending_requests,
            Duration::from_secs(60),
            cx_total,
            cx_destroy,
            cx_http2_total,
            cx_active,
            Some(Arc::clone(&rq_pending_overflow)),
            None,
            None,
        );
        (pool, rq_pending_overflow)
    }

    /// 15 D4: build a pool with a registered `upstream_cx_overflow` counter +
    /// `circuit_breakers.default.cx_open` gauge (the circuit-breakers-configured
    /// shape). Returns the pool + both handles so tests can assert the
    /// cap-overflow + edge-driven gauge semantics. Mirrors the H1 `mk_pool_cb`
    /// helper.
    #[allow(clippy::type_complexity)]
    fn mk_pool_cb(
        cluster: &str,
        max_connections: u32,
    ) -> (
        Arc<H2Pool>,
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
        let cx_http2_total = registry
            .register_counter(&format!("cluster.{cluster}.upstream_cx_http2_total"))
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
        let pool = H2Pool::new(
            cluster.to_string(),
            max_connections,
            DEFAULT_MAX_PENDING_REQUESTS,
            Duration::from_secs(60),
            cx_total,
            cx_destroy,
            cx_http2_total,
            cx_active,
            None,
            Some(Arc::clone(&cx_overflow)),
            Some(Arc::clone(&cx_open)),
        );
        (pool, cx_overflow, cx_open)
    }

    /// Spawn an in-process h2 server on a 127.0.0.1 ephemeral port that
    /// echoes any request with a 200 OK + empty body. Mirrors the
    /// `crate::client::tests::spawn_h2_server` shape but simpler — the
    /// pool tests don't need to capture the request. Returns the bound
    /// addr; the server runs until the process exits (the listener+task
    /// are leaked deliberately, matching the H1 `echo_backend` pattern).
    async fn spawn_h2_server() -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (tcp, _peer) = match listener.accept().await {
                    Ok(p) => p,
                    Err(_) => return,
                };
                tokio::spawn(async move {
                    let mut conn = match h2::server::handshake(tcp).await {
                        Ok(c) => c,
                        Err(_) => return,
                    };
                    while let Some(result) = conn.accept().await {
                        let (req, mut send_response) = match result {
                            Ok(p) => p,
                            Err(_) => return,
                        };
                        // Drain request body (small-body assumption).
                        let (_parts, mut body) = req.into_parts();
                        while let Some(chunk_result) = body.data().await {
                            let chunk = match chunk_result {
                                Ok(c) => c,
                                Err(_) => return,
                            };
                            let _ = body.flow_control().release_capacity(chunk.len());
                        }
                        let resp = http::Response::builder().status(200).body(()).unwrap();
                        let mut send_stream = match send_response.send_response(resp, false) {
                            Ok(s) => s,
                            Err(_) => return,
                        };
                        let _ = send_stream.send_data(Bytes::new(), true);
                    }
                });
            }
        });
        addr
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn acquire_from_empty_pool_creates_connection_and_fires_counters() {
        let addr = spawn_h2_server().await;
        let (pool, cx_total, _cx_destroy, cx_http2_total, cx_active) =
            mk_pool("c", 4, Duration::from_secs(60));
        let guard = pool.acquire(addr, "host.example").await.expect("acquire");
        assert_eq!(cx_total.value(), 1, "cx_total fires on connect-on-miss");
        assert_eq!(
            cx_http2_total.value(),
            1,
            "cx_http2_total fires on connect-on-miss"
        );
        assert_eq!(cx_active.value(), 1, "cx_active increments via guard");
        drop(guard);
        // Sync Drop — no spawn needed; the gauge decrement is observable
        // immediately.
        assert_eq!(cx_active.value(), 0, "cx_active decrements on guard drop");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn acquire_after_release_reuses_existing_connection_without_incrementing_cx_total() {
        let addr = spawn_h2_server().await;
        let (pool, cx_total, _cx_destroy, _cx_http2_total, _cx_active) =
            mk_pool("c", 4, Duration::from_secs(60));
        let g1 = pool.acquire(addr, "h").await.expect("acquire 1");
        drop(g1);
        let _g2 = pool.acquire(addr, "h").await.expect("acquire 2");
        assert_eq!(cx_total.value(), 1, "reuse must not re-fire cx_total");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn acquire_with_concurrent_streams_shares_one_connection() {
        let addr = spawn_h2_server().await;
        let (pool, cx_total, _cx_destroy, _cx_http2_total, cx_active) =
            mk_pool("c", 4, Duration::from_secs(60));
        let g1 = pool.acquire(addr, "h").await.expect("acquire 1");
        let g2 = pool.acquire(addr, "h").await.expect("acquire 2");
        let g3 = pool.acquire(addr, "h").await.expect("acquire 3");
        assert_eq!(
            cx_total.value(),
            1,
            "3 concurrent streams must share one connection"
        );
        // Per PLAN lock-in #6 (per-guard / 'active streams' semantic):
        // each guard contributes 1 to cx_active.
        assert_eq!(
            cx_active.value(),
            3,
            "cx_active reads N concurrent guards = N streams"
        );
        drop(g1);
        drop(g2);
        drop(g3);
        assert_eq!(cx_active.value(), 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn acquire_returns_overflow_when_at_max_connections() {
        // Construct a pool with `max_connections=1`, manually saturate the
        // entry's `active_streams` atomic to its `max_streams` via a
        // test-only path: insert an entry directly into the connections
        // map with active_streams == max_streams (simulating the at-cap
        // multiplex state). The second acquire — barred from claiming a
        // slot on the existing entry AND barred from creating a new conn
        // (max_connections=1) — must surface PoolError::Overflow.
        //
        // This deterministic test-shape replaces the 101-concurrent-stream
        // alternative (per PLAN Task 1 test #4 simpler-shape guidance).
        let addr = spawn_h2_server().await;
        let (pool, _cx_total, _cx_destroy, _cx_http2_total, _cx_active) =
            mk_pool("c", 1, Duration::from_secs(60));
        // Acquire a real connection (drives established → 1).
        let _g1 = pool.acquire(addr, "h").await.expect("first acquire");
        // Saturate the entry's active_streams to its max_streams cap so
        // Phase-1 slot-claim fails on every retry.
        {
            let conns = pool.connections.lock();
            let list = conns.get(&addr).expect("entry present after first acquire");
            assert_eq!(list.len(), 1);
            let entry = &list[0];
            entry
                .active_streams
                .store(entry.max_streams, Ordering::Release);
        }
        // The second acquire: phase 1 finds no slot; phase 2 sees
        // established == max_connections (1) → PoolError::Overflow.
        let err = pool
            .acquire(addr, "h")
            .await
            .expect_err("second acquire must overflow");
        assert!(
            matches!(err, PoolError::Overflow { ref cluster, max: 1 } if cluster == "c"),
            "expected Overflow{{cluster:'c',max:1}}, got {err:?}"
        );
        // Restore the entry's stream count so g1's Drop bookkeeping is
        // consistent (active_streams decrements from 1, not from
        // max_streams).
        {
            let conns = pool.connections.lock();
            let list = conns.get(&addr).expect("entry still present");
            list[0].active_streams.store(1, Ordering::Release);
        }
    }

    /// 15 D3 (lock-in #7): under `max_pending_requests:0` the first
    /// connect-on-miss is rejected with `PoolError::PendingOverflow` BEFORE
    /// any connect (the backend is never dialed) and the
    /// `upstream_rq_pending_overflow` counter ticks to 1. Mirrors the H1
    /// sibling test.
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
    /// Drives the `H2PoolGuard::Drop` invalidate path to confirm the gauge
    /// returns to terminal-0. Mirrors the H1 sibling test, adapted to the H2
    /// pool's stream-slot saturation shape (Phase-1 slot-claim must fail so
    /// the second acquire reaches the Phase-2 cap-check).
    #[tokio::test(flavor = "multi_thread")]
    async fn cx_overflow_increments_and_cx_open_tracks_cap_edges() {
        let addr = spawn_h2_server().await;
        let (pool, cx_overflow, cx_open) = mk_pool_cb("c", 1);
        // First acquire connects → established reaches the cap (1) → cx_open
        // set to 1.
        let mut g1 = pool
            .acquire(addr, "h")
            .await
            .expect("first acquire connects");
        assert_eq!(cx_open.value(), 1, "cx_open set at cap after first connect");
        assert_eq!(cx_overflow.value(), 0, "no overflow yet");
        // Saturate the entry's active_streams to its max_streams cap so the
        // second acquire's Phase-1 slot-claim fails on every retry and it
        // reaches the Phase-2 cap-check.
        {
            let conns = pool.connections.lock();
            let list = conns.get(&addr).expect("entry present after first acquire");
            assert_eq!(list.len(), 1);
            list[0]
                .active_streams
                .store(list[0].max_streams, Ordering::Release);
        }
        // Second acquire overflows (no slot, established == cap) → cx_overflow
        // increments; cx_open unchanged (still at cap).
        let err = pool
            .acquire(addr, "h")
            .await
            .expect_err("second acquire must overflow");
        assert!(
            matches!(err, PoolError::Overflow { ref cluster, max: 1 } if cluster == "c"),
            "expected Overflow, got {err:?}"
        );
        assert_eq!(cx_overflow.value(), 1, "cx_overflow ticks on cap-hit");
        assert_eq!(cx_open.value(), 1, "cx_open still at cap after overflow");
        // Restore the entry's stream count so g1's Drop bookkeeping is
        // consistent (active_streams decrements from 1, not from max_streams).
        {
            let conns = pool.connections.lock();
            let list = conns.get(&addr).expect("entry still present");
            list[0].active_streams.store(1, Ordering::Release);
        }
        // Drive the invalidate Drop path: established decrements below the cap
        // → cx_open returns to terminal-0.
        g1.invalidate();
        drop(g1);
        assert_eq!(
            cx_open.value(),
            0,
            "cx_open returns to 0 at the invalidate decrement edge"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn invalidate_evicts_entry_and_increments_cx_destroy() {
        let addr = spawn_h2_server().await;
        let (pool, _cx_total, cx_destroy, _cx_http2_total, _cx_active) =
            mk_pool("c", 4, Duration::from_secs(60));
        let mut g = pool.acquire(addr, "h").await.expect("acquire");
        g.invalidate();
        drop(g);
        assert_eq!(cx_destroy.value(), 1, "invalidate path fires cx_destroy");
        // Connections list should now be empty for this endpoint.
        let conns = pool.connections.lock();
        assert!(
            conns.get(&addr).is_none_or(|list| list.is_empty()),
            "invalidate must remove the entry from the connections list"
        );
        let est = pool.established.lock();
        assert_eq!(
            est.get(&addr).copied().unwrap_or(0),
            0,
            "invalidate must decrement established"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn idle_sweeper_evicts_past_deadline_entries() {
        let addr = spawn_h2_server().await;
        let (pool, _cx_total, cx_destroy, _cx_http2_total, _cx_active) =
            mk_pool("c", 4, Duration::from_millis(40));
        let token = CancellationToken::new();
        let sweeper = pool.spawn_idle_sweeper(token.clone());
        let g = pool.acquire(addr, "h").await.expect("acquire");
        drop(g);
        // Give the sweeper time to tick past the idle deadline. Interval
        // is idle_timeout/4 = 10ms; sleep ~120ms to allow several ticks.
        tokio::time::sleep(Duration::from_millis(120)).await;
        assert!(
            cx_destroy.value() >= 1,
            "sweeper must evict idle entry past deadline; cx_destroy={}",
            cx_destroy.value()
        );
        token.cancel();
        let _ = sweeper.await;
    }

    /// 13.2 mirrors 13.1's state-5 fold-in (REVIEW Cluster A I2): a zero
    /// or sub-4ns `idle_timeout` must not panic the sweeper. The interval
    /// is clamped to ≥1ms.
    #[tokio::test(flavor = "multi_thread")]
    async fn spawn_idle_sweeper_with_zero_idle_timeout_does_not_panic() {
        let (pool, _cx_total, _cx_destroy, _cx_http2_total, _cx_active) =
            mk_pool("c", 4, Duration::ZERO);
        let token = CancellationToken::new();
        let sweeper = pool.spawn_idle_sweeper(token.clone());
        tokio::time::sleep(Duration::from_millis(10)).await;
        token.cancel();
        sweeper.await.expect("sweeper must exit cleanly, not panic");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn h2_pool_manager_registers_cx_destroy_and_cx_http2_total_per_h2_cluster() {
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
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.HttpProtocolOptions":
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http2_protocol_options: {}
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
            H2PoolManager::for_bootstrap(&bootstrap, &mgr, Arc::clone(&registry), token)
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
                .any(|(n, _)| n == "cluster.c1.upstream_cx_http2_total"),
            "expected cluster.c1.upstream_cx_http2_total in registry; got: {:?}",
            snapshot.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>()
        );
    }

    /// 13.2 A-I3 race regression: under the synchronous parking_lot
    /// Mutex, an `invalidate()`-flagged guard whose `drop_task` has
    /// joined MUST have run its destroy-path bookkeeping
    /// (connections-list-evict + established-decrement + cx_destroy.inc)
    /// by the time the join returns. The follow-up acquire therefore
    /// either claims a freshly-evicted connection slot (via a new
    /// Client::connect) OR succeeds without spurious Overflow.
    ///
    /// Mirrors the H1 sibling test shape — the joint A-I3 close requires
    /// both pools' Drop paths be structurally synchronous so `await`-ing
    /// a drop task is a happens-before boundary for the bookkeeping
    /// effects.
    #[tokio::test(flavor = "multi_thread")]
    async fn pool_acquire_after_concurrent_release_does_not_yield_spurious_overflow() {
        let addr = spawn_h2_server().await;
        let (pool, _cx_total, _cx_destroy, _cx_http2_total, _cx_active) =
            mk_pool("c", 1, Duration::from_secs(60));
        for i in 0..16 {
            let pool_a = Arc::clone(&pool);
            let pool_b = Arc::clone(&pool);
            // Pre-acquire a guard; mark it invalidated so its Drop runs
            // the destroy-path bookkeeping (frees the established slot).
            let mut g1 = pool_a.acquire(addr, "h").await.expect("pre-acquire");
            g1.invalidate();
            // Drop the guard on a separate task and AWAIT its
            // completion. Under sync Drop, the connections-list evict +
            // established-decrement land synchronously inside `drop(g1)`.
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
}
