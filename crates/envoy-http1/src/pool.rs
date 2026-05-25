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
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(60);
/// Sweeper tick interval: `idle_timeout / 4` (15s at the default 60s timeout).
const SWEEPER_DIVISOR: u32 = 4;

/// Errors returned by `H1Pool::acquire`.
#[derive(Debug, thiserror::Error)]
pub enum PoolError {
    /// Pool is at `max_connections` AND no idle stream available.
    #[error("upstream pool overflow: cluster='{cluster}', max_connections={max}")]
    Overflow { cluster: String, max: u32 },
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
                }
            }
        }
    }
}

impl H1Pool {
    /// Build a new pool. `cx_total`/`cx_active` come from the existing cluster
    /// stat handles (shared `Arc`); `cx_destroy`/`cx_http1_total` are
    /// registered by the caller (see `H1PoolManager::for_bootstrap`).
    pub fn new(
        cluster_name: String,
        max_connections: u32,
        idle_timeout: Duration,
        cx_total: Arc<envoy_stats::Counter>,
        cx_destroy: Arc<envoy_stats::Counter>,
        cx_http1_total: Arc<envoy_stats::Counter>,
        cx_active: Arc<envoy_stats::Gauge>,
    ) -> Arc<Self> {
        Arc::new(Self {
            cluster_name,
            max_connections,
            idle_timeout,
            idle: parking_lot::Mutex::new(HashMap::new()),
            established: parking_lot::Mutex::new(HashMap::new()),
            cx_total,
            cx_destroy,
            cx_http1_total,
            cx_active,
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
        // Connect-on-miss: enforce cap.
        {
            let mut est = self.established.lock();
            let n = est.entry(endpoint).or_insert(0);
            if *n >= self.max_connections {
                return Err(PoolError::Overflow {
                    cluster: self.cluster_name.clone(),
                    max: self.max_connections,
                });
            }
            *n += 1;
        }
        // Connect (lock released — connect is the slow path).
        let stream = match Client::connect(endpoint, host).await {
            Ok(s) => s,
            Err(e) => {
                // Roll back the established count.
                let mut est = self.established.lock();
                if let Some(n) = est.get_mut(&endpoint) {
                    *n = n.saturating_sub(1);
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
// `tokio::sync::Mutex` + per-pool `Counter`/`Gauge` Arcs aren't reflected
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
                DEFAULT_IDLE_TIMEOUT,
                cx_total,
                cx_destroy,
                cx_http1_total,
                cx_active,
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
            idle_timeout,
            Arc::clone(&cx_total),
            Arc::clone(&cx_destroy),
            Arc::clone(&cx_http1_total),
            Arc::clone(&cx_active),
        );
        (pool, cx_total, cx_destroy, cx_http1_total, cx_active)
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
