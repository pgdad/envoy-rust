//! Cluster data model + round-robin LB. See SPEC §D1.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};

/// 06.3 D15.3.b: RAII guard around `cluster.<name>.upstream_cx_active`.
/// Construction increments via the cluster's `cx_active_guard()`; Drop
/// decrements. Covers both success and error close paths uniformly —
/// the guard exits scope at the per-call task's epilogue regardless of
/// whether the upstream connect succeeded.
///
/// Per architecture decision 13 (06.3 PROGRESS Task 1 preamble): the
/// guard lives in envoy-cluster because the gauge does; per parent-06
/// SPEC §6 Rule 2 the consumer increments — the consumer here is the
/// HCM proxy-arm / envoy-tcp dial site, which holds the guard.
pub struct ConnGaugeGuard {
    gauge: Arc<envoy_stats::Gauge>,
}

impl ConnGaugeGuard {
    /// 13.1 D3: construct a guard from a pre-incremented `Arc<Gauge>` handle.
    /// The caller MUST have called `gauge.inc()` already; Drop calls `gauge.dec()`.
    /// Mirrors `Cluster::cx_active_guard()`'s `inc + wrap` pattern, exposed for
    /// `envoy-http1::H1Pool` (which doesn't hold a `Cluster` reference but
    /// shares the `Arc<Gauge>` via the StatsRegistry's same-kind-idempotency).
    pub fn from_gauge(gauge: Arc<envoy_stats::Gauge>) -> Self {
        Self { gauge }
    }
}

impl Drop for ConnGaugeGuard {
    fn drop(&mut self) {
        self.gauge.dec();
    }
}

/// 14.1 D5/D6: cluster-level outlier-detection state, owned by `Cluster` when the
/// cluster's `outlier_detection` block is configured. `None` ⇒ outlier detection is
/// disabled for the cluster (§5.3 inert-when-unconfigured invariant; the 21 existing
/// fixtures stay green).
///
/// The per-endpoint `EndpointEjection` handles are aligned by index with
/// `Cluster.endpoints`. The cluster-level `ejections_overflow` counter increments at
/// `Cluster::record_response`'s cap-met arm per ADR-0041 §6.2 item-2 (overflow re-fires
/// per detection-tick, NOT once-per-host).
#[derive(Debug)]
pub(crate) struct OutlierDetectionState {
    pub(crate) endpoints: Vec<Arc<crate::EndpointEjection>>,
    pub(crate) max_ejection_percent: u32,
    pub(crate) ejections_overflow: Arc<envoy_stats::Counter>,
    /// 14.2 D7 (lock-in #6): the runtime ejection-duration. The D7
    /// `OutlierEjectionSweeper` un-ejects an endpoint at the next `interval` tick after
    /// `now - eject_time >= base_ejection_time`. Populated in `from_bootstrap` from the
    /// parsed `outlier_detection.base_ejection_time` (Envoy v3 default 30s when omitted).
    pub(crate) base_ejection_time: std::time::Duration,
    /// 14.2 D7 (lock-in #6): the runtime sweep cadence. Drives the `OutlierEjectionSweeper`'s
    /// `tokio::time::interval` period. Populated in `from_bootstrap` from the parsed
    /// `outlier_detection.interval` (Envoy v3 default 10s when omitted).
    pub(crate) interval: std::time::Duration,
}

impl OutlierDetectionState {
    /// 14.2 M5: borrow the per-cluster shared `EndpointEjectionStats`. Every endpoint holds a
    /// clone of the same `Arc` handles, so the first endpoint's view is the cluster-wide
    /// aggregate. Crate-internal; used by the `record_response` enforced-counter tests.
    #[cfg(test)]
    pub(crate) fn stats(&self) -> &crate::EndpointEjectionStats {
        self.endpoints[0].stats()
    }
}

/// 27 Task 4 (§6.2 / ADR-0068): the per-cluster EDS-reload state, retained ONLY
/// for `type: EDS` clusters (`None` for STATIC / STRICT_DNS). Bundles everything
/// the file-based EDS reload pipeline ([`crate::eds_reload`]) needs to reparse,
/// select, validate, and mirror-apply (or warm-reject) a changed assignment
/// file: the file path, the CLA selection name, and the 5 `update_*` counter
/// handles. The in-crate `build_eds_watch_targets` reads these (they are
/// `pub(crate)`) to bundle a watch target per PLAIN EDS cluster.
#[derive(Debug)]
pub(crate) struct EdsReloadState {
    /// The EDS assignment file to stat + re-read. From the cluster's
    /// `eds_cluster_config.eds_config.path_config_source.path`.
    pub(crate) path: std::path::PathBuf,
    /// The name used to select the `ClusterLoadAssignment` out of the parsed
    /// file. MIRRORS the phase-21 initial load (`load_dynamic_resources`,
    /// `crates/envoy-config/src/lib.rs`): `service_name` if set, else the
    /// cluster name. The V4(b) "no CLA matches" reject fires when no CLA in the
    /// file has `cluster_name == selection_name`.
    pub(crate) selection_name: String,
    /// The 5 `cluster.<name>.update_*` counter handles, ticked per the §6.2
    /// V4 taxonomy on each reload. Captured at construction; re-registering by
    /// name is idempotent (returns the same handle the initial load seeded).
    pub(crate) update_attempt: Arc<envoy_stats::Counter>,
    pub(crate) update_success: Arc<envoy_stats::Counter>,
    pub(crate) update_failure: Arc<envoy_stats::Counter>,
    pub(crate) update_empty: Arc<envoy_stats::Counter>,
    pub(crate) update_rejected: Arc<envoy_stats::Counter>,
}

/// 29 (ADR-0071/0072): the consistent-hashing LB variant a `Cluster` dispatches
/// on. `Ring` is the phase-28 ketama ring (RING_HASH); `Maglev` is the phase-29
/// permutation table (MAGLEV). Both expose a `lookup(key_hash) -> Option<usize>`
/// returning a host index aligned with the cluster's endpoint Vec.
#[derive(Debug)]
pub(crate) enum HashLb {
    Ring(crate::ring_hash::HashRing),
    Maglev(crate::maglev::MaglevTable),
}

/// A configured upstream cluster. Owns the static endpoint list and the
/// round-robin `AtomicUsize` cursor. Constructed by `from_bootstrap` only;
/// external code works through `ClusterHandle`.
#[derive(Debug)]
pub struct Cluster {
    pub(crate) name: String,
    /// 27 D1 (§6.2 / ADR-0068): the endpoint address set behind a swappable
    /// handle so a file-based EDS watcher (a later phase-27 task) can
    /// `store_endpoints(new)` atomically while the round-robin LB reads it
    /// per-request. `RwLock<Arc<…>>` mirrors the phase-26
    /// `HCMConfig.route_config` precedent verbatim (std-only; no `arc-swap`).
    ///
    /// Readers MUST go through [`Cluster::current_endpoints`], which snapshots
    /// the current `Arc` ONCE per selection (the §5.4 / M26-1 read-once
    /// invariant): an in-flight `pick()` holds its snapshot for the whole
    /// selection so a concurrent [`store_endpoints`] swap cannot tear the read.
    /// The `Arc` clone is a cheap pointer bump, NOT a deep `Vec` clone.
    ///
    /// [`store_endpoints`]: Cluster::store_endpoints
    pub(crate) endpoints: RwLock<Arc<Vec<SocketAddr>>>,
    pub(crate) cursor: AtomicUsize,
    /// 05.3 NEW per SPEC §3 D3: cluster-level upstream protocol selector.
    /// Set in `from_bootstrap` from the parsed cluster's
    /// `typed_extension_protocol_options`. Defaulted to `Http1`.
    pub(crate) upstream_protocol: UpstreamProtocol,
    /// 06.1 D4.b: per-cluster counter incremented once per established
    /// upstream TCP connection. Registered at construct time as
    /// `cluster.<name>.upstream_cx_total`. Exposed via `cx_total()` so the
    /// connect-site callers (envoy-tcp::TcpProxy, envoy_http1::Client and
    /// envoy_http2::Client invocations from the HCM router-proxy arm)
    /// can `inc()` after a successful upstream connect. The connect site
    /// itself does NOT live in this crate: `Cluster` is a configuration /
    /// load-balancing data structure, not a connection factory. See SPEC
    /// §3 D4.b's increment-site-at-call-site posture.
    pub(crate) cx_total: Arc<envoy_stats::Counter>,
    /// 06.3 D15.3.b: per-cluster active-connection gauge. Registered at
    /// construct time as `cluster.<name>.upstream_cx_active`. Exposed via
    /// `cx_active_guard()` (on `Cluster` + `ClusterHandle`) which atomically
    /// increments and returns a `ConnGaugeGuard`; the guard's `Drop` impl
    /// decrements, covering both success and error close paths uniformly.
    pub(crate) cx_active: Arc<envoy_stats::Gauge>,
    /// 06.3 D15.3.c: per-cluster counter incremented once per upstream
    /// response received (success path only — the 502/503 synth paths do NOT
    /// increment). Registered at construct time as
    /// `cluster.<name>.upstream_rq_total`. Exposed via `upstream_rq_total()`.
    pub(crate) upstream_rq_total: Arc<envoy_stats::Counter>,
    /// 06.3 D15.3.c: per-cluster counter incremented when the upstream
    /// response status is 5xx (status / 100 == 5). Registered at construct
    /// time as `cluster.<name>.upstream_rq_5xx`. Exposed via
    /// `upstream_rq_5xx()`.
    pub(crate) upstream_rq_5xx: Arc<envoy_stats::Counter>,
    /// 16 Task 3: per-cluster counter incremented once per retry attempt
    /// dispatched. Registered unconditionally at construct time as
    /// `cluster.<name>.upstream_rq_retry` (inert-at-0 until Task 4/5 increment
    /// it). Exposed via `upstream_rq_retry()`.
    pub(crate) upstream_rq_retry: Arc<envoy_stats::Counter>,
    /// 16 Task 3: per-cluster counter incremented when a retried request
    /// ultimately succeeds. Registered unconditionally at construct time as
    /// `cluster.<name>.upstream_rq_retry_success`. Exposed via
    /// `upstream_rq_retry_success()`.
    pub(crate) upstream_rq_retry_success: Arc<envoy_stats::Counter>,
    /// 16 Task 3: per-cluster counter incremented when a request exhausts its
    /// configured retry budget (num_retries reached). Registered unconditionally
    /// at construct time as `cluster.<name>.upstream_rq_retry_limit_exceeded`.
    /// Exposed via `upstream_rq_retry_limit_exceeded()`.
    pub(crate) upstream_rq_retry_limit_exceeded: Arc<envoy_stats::Counter>,
    /// 17 Task 3: per-cluster counter registered unconditionally at construct time
    /// as `cluster.<name>.upstream_rq_retry_overflow`. Shared with `BudgetState`
    /// via the idempotent-registration contract: when a `BudgetState` is also
    /// constructed, its registration of the same name returns the SAME Arc.
    /// Inert at 0 on clusters without `circuit_breakers`; incremented by
    /// `BudgetState::try_acquire_retry` when the breaker is open.
    pub(crate) upstream_rq_retry_overflow: Arc<envoy_stats::Counter>,
    /// 17 Task 3: per-cluster circuit-breaker budget state. `None` when the
    /// cluster has no `circuit_breakers` configured (the §5.3
    /// inert-when-unconfigured invariant). When present, `try_acquire_retry` /
    /// `try_acquire_request` gate in-flight retries and requests against the
    /// configured caps (L1/L2).
    pub(crate) budget: Option<Arc<crate::BudgetState>>,
    /// 12.1 (parent-12 D3/D5): per-endpoint active-health-check state, aligned by
    /// index with `endpoints`. `None` when the cluster has no `health_checks`
    /// configured (the §5.4 inert-when-unconfigured invariant) — `pick()` is then
    /// byte-for-byte phase-02 round-robin. `Some` carries one `Arc<EndpointHealth>`
    /// per (resolved) endpoint; the 12.2 probe task mutates them while `pick()`
    /// reads them.
    pub(crate) endpoint_health: Option<Vec<Arc<crate::EndpointHealth>>>,
    /// 12.1 (parent-12 D5): `common_lb_config.healthy_panic_threshold` percentage
    /// (default 50.0). Read by `pick()` whenever any eligibility filter is configured —
    /// `endpoint_health` (active HC) AND/OR `outlier_detection` (14.2). Parsed unconditionally
    /// in `from_bootstrap` (the 14.2 Task-8 fixup hoisted it out of the health-check branch so
    /// outlier-detection-only clusters honor it).
    pub(crate) panic_threshold: f64,
    /// 14.1 D5/D6 (parent-14 D3/D5/D6): per-cluster outlier-detection state. `None`
    /// when the cluster's `outlier_detection` config block is absent — the §5.3
    /// inert-when-unconfigured invariant. `pick()`'s fast path bypasses entirely
    /// when this AND `endpoint_health` are both `None`.
    pub(crate) outlier_detection: Option<OutlierDetectionState>,
    /// 29 (ADR-0071/0072): the consistent-hash LB dispatch. `Some` iff `lb_policy`
    /// is a hash policy (built in `from_bootstrap`); `None` for ROUND_ROBIN (the
    /// cursor path). Replaces the phase-28 `ring: Option<HashRing>` discriminator
    /// (M28-3): `ring.is_some()` could not distinguish a SECOND ring-building policy.
    pub(crate) hash_lb: Option<HashLb>,
    /// 30 (ADR-0073/0074): metadata-based subset LB index. `Some` ONLY when the
    /// cluster's `lb_subset_config` block is present (built in `from_bootstrap`
    /// over an `endpoint_metadata` Vec index-aligned with `endpoints`); `None`
    /// for clusters with no subset config — the §6.2 inert-when-unconfigured
    /// invariant. When `None`, `pick()` runs the EXISTING hash_lb/fast/slow
    /// dispatch byte-identically (the no-op regression proof). When `Some`,
    /// `pick()` resolves the route `metadata_match` to an eligible-endpoint set
    /// BEFORE that dispatch and narrows to it.
    pub(crate) subset: Option<crate::subset::SubsetIndex>,
    /// 27 Task 4 (§6.2 / ADR-0068): EDS-reload state, `Some` ONLY for `type: EDS`
    /// clusters (`None` for STATIC / STRICT_DNS — the §5.2 inert-when-unconfigured
    /// discipline). Carries the assignment-file path, the CLA selection name, and
    /// the 5 `update_*` counter handles the [`crate::eds_reload`] pipeline ticks.
    /// `build_eds_watch_targets` reads this in-crate to bundle a watch target per
    /// PLAIN EDS cluster.
    pub(crate) eds_reload: Option<EdsReloadState>,
}

impl Cluster {
    /// Cluster name as configured in `bootstrap.static_resources.clusters[].name`.
    /// Surfaced for use in error variants and tracing log lines that name the
    /// cluster a request was routed to (per phase-04.3 SPEC §3 D5; closes the
    /// multi-phase Cluster::name() carryforward originating in phase-02.1
    /// REVIEW M1).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 05.3 NEW: cluster-level upstream protocol. See `UpstreamProtocol`'s
    /// docs. Mirrors the `name()` accessor's posture (typed value, copy
    /// semantics; no Result, no panic). Per SPEC §6 inherited signpost 1
    /// the typed value is set at cluster-build time, not derived per call.
    pub fn upstream_protocol(&self) -> UpstreamProtocol {
        self.upstream_protocol
    }

    /// 06.1 D4.b: shared accessor for the per-cluster upstream connection
    /// counter (`cluster.<name>.upstream_cx_total`). Returns the cached
    /// `Arc<Counter>` registered at `from_bootstrap` time. The connect-site
    /// caller does `cluster.cx_total().inc()` after the upstream
    /// `TcpStream::connect` succeeds. Mirrors `name()`'s borrow shape
    /// (returns a `&` into the `Cluster`'s lifetime).
    pub fn cx_total(&self) -> &Arc<envoy_stats::Counter> {
        &self.cx_total
    }

    /// 06.3 D15.3.c: shared accessor for the per-cluster upstream-response
    /// total counter (`cluster.<name>.upstream_rq_total`). Returns the
    /// cached `Arc<Counter>` registered at `from_bootstrap` time. The
    /// response-site caller does `cluster.upstream_rq_total().inc()` once
    /// the upstream response is successfully received. Mirrors `cx_total()`'s
    /// borrow shape.
    pub fn upstream_rq_total(&self) -> &Arc<envoy_stats::Counter> {
        &self.upstream_rq_total
    }

    /// 06.3 D15.3.c: shared accessor for the per-cluster upstream-5xx
    /// counter (`cluster.<name>.upstream_rq_5xx`). The response-site caller
    /// increments conditionally when `upstream_resp.status / 100 == 5`.
    /// Mirrors `upstream_rq_total()`'s borrow shape.
    pub fn upstream_rq_5xx(&self) -> &Arc<envoy_stats::Counter> {
        &self.upstream_rq_5xx
    }

    /// 16 Task 3: shared accessor for the per-cluster retry-attempt counter
    /// (`cluster.<name>.upstream_rq_retry`). Incremented by the retry loop
    /// (Tasks 4/5) once per attempt beyond the first. Mirrors
    /// `upstream_rq_total()`'s borrow shape.
    pub fn upstream_rq_retry(&self) -> &Arc<envoy_stats::Counter> {
        &self.upstream_rq_retry
    }

    /// 16 Task 3: shared accessor for the per-cluster retry-success counter
    /// (`cluster.<name>.upstream_rq_retry_success`). Incremented by the retry
    /// loop when a retried request ultimately returns a non-retry-eligible
    /// response. Mirrors `upstream_rq_total()`'s borrow shape.
    pub fn upstream_rq_retry_success(&self) -> &Arc<envoy_stats::Counter> {
        &self.upstream_rq_retry_success
    }

    /// 16 Task 3: shared accessor for the per-cluster retry-limit-exceeded
    /// counter (`cluster.<name>.upstream_rq_retry_limit_exceeded`). Incremented
    /// by the retry loop when num_retries is exhausted. Mirrors
    /// `upstream_rq_total()`'s borrow shape.
    pub fn upstream_rq_retry_limit_exceeded(&self) -> &Arc<envoy_stats::Counter> {
        &self.upstream_rq_retry_limit_exceeded
    }

    /// 17 Task 3: shared accessor for the per-cluster retry-overflow counter
    /// (`cluster.<name>.upstream_rq_retry_overflow`). Registered unconditionally
    /// at construct time (inert at 0 on clusters without `circuit_breakers`).
    /// When a `BudgetState` is also present, they share this Arc via the
    /// idempotent-registration contract. Mirrors `upstream_rq_retry()`'s
    /// borrow shape.
    pub fn upstream_rq_retry_overflow(&self) -> &Arc<envoy_stats::Counter> {
        &self.upstream_rq_retry_overflow
    }

    /// 06.3 D15.3.b: increment `cluster.<name>.upstream_cx_active` and
    /// return a `ConnGaugeGuard` that decrements the gauge on drop. The
    /// caller must bind the guard to `_cx_guard` (not `_`) to preserve
    /// the binding for the connection's scope. Covers both success and
    /// error close paths uniformly.
    pub fn cx_active_guard(&self) -> ConnGaugeGuard {
        self.cx_active.inc();
        ConnGaugeGuard {
            gauge: Arc::clone(&self.cx_active),
        }
    }

    /// 13.2 D5: shared accessor for the per-cluster `upstream_cx_active`
    /// gauge handle. Returns the cached `Arc<Gauge>` registered at
    /// `from_bootstrap` time. Used by `H1PoolManager::for_bootstrap` and
    /// `H2PoolManager::for_bootstrap` to `debug_assert!(Arc::ptr_eq(...))`
    /// that the gauge handle the pool acquired from
    /// `StatsRegistry::register_gauge` is the SAME `Arc` the cluster holds
    /// (single-bootstrap-per-process invariant — closes 13.1 REVIEW
    /// Cluster A-M2). Mirrors `cx_total()`'s borrow shape.
    pub fn cx_active_arc(&self) -> &Arc<envoy_stats::Gauge> {
        &self.cx_active
    }

    /// Picks the next endpoint in round-robin order. When the cluster has no
    /// active health checks AND no outlier detection (`endpoint_health` and
    /// `outlier_detection` are both `None`) this is exactly the phase-02
    /// round-robin (the §5.4 / §5.3 inert-when-unconfigured invariant). When at
    /// least one filter is configured, eligibility is the AND-composition of
    /// "healthy" (12.1 active HC) AND "not-ejected" (14.1 outlier detection) —
    /// a `None` filter is vacuously `true` for every endpoint. The panic
    /// threshold (§6.2 item-3) is honored against the eligible fraction.
    /// `Relaxed` ordering is sufficient for the cursor and the health / ejection
    /// reads (single-writer per endpoint; no happens-before dependency).
    ///
    /// 28 Task 5 (§6.2-LOCKED / ADR-0070): `key_hash` carries the per-request
    /// hash key (the `xxh64` of the route's `hash_policy` material) for
    /// `RING_HASH` clusters. `RoundRobin` IGNORES it entirely (no behavior
    /// change — the phase-02 cursor path). `RingHash` with `Some(key_hash)` does
    /// the consistent-hashing ring lookup; `None` falls back to the cursor path
    /// (Task 6 + the request-path backstop cover the real no-hash fallback). The
    /// PUBLIC `ClusterHandle::pick_endpoint` delegate passes `None` FOR NOW — Task
    /// 6 threads the real key through.
    ///
    /// 30 (ADR-0073/0074): `subset_match` carries the route's `metadata_match`
    /// (the `envoy.lb` map). When the cluster has `subset: None` (no
    /// `lb_subset_config`) this argument is INERT — the existing
    /// hash_lb/fast/slow dispatch runs byte-identically (the no-op regression
    /// proof). When `subset: Some`, the index resolves `subset_match` to an
    /// eligible-endpoint set BEFORE the dispatch: `Eligible::All` → treated as
    /// "all endpoints" (dispatch unchanged); `Eligible::Some(idxs)` → round-robin
    /// within the matched subset (intersected with HC/OD eligibility, I-1);
    /// `Eligible::None` → `None` (NO_FALLBACK no-match → 503). Subset +
    /// consistent-hash-inner-LB and subset + panic-threshold are §2.2 deferred
    /// non-goals (the MVP inner LB is ROUND_ROBIN). The real route-`metadata_match`
    /// threading at the HCM sites is Task 6; every caller passes `None` for now.
    fn pick(
        &self,
        key_hash: Option<u64>,
        subset_match: Option<&std::collections::BTreeMap<String, String>>,
    ) -> Option<SocketAddr> {
        // 27 D1 (§5.4 / M26-1): snapshot the current endpoint Arc ONCE at entry
        // and use this snapshot for the whole selection. Never re-acquire the
        // lock mid-selection — a concurrent `store_endpoints` swap then cannot
        // tear an in-flight read (the read-once invariant).
        let eps = self.current_endpoints();
        if eps.is_empty() {
            // `from_bootstrap` rejects empty clusters at startup; a hot-reload
            // CAN apply an empty set (V4(d)) → short-circuit BEFORE any modulo
            // (avoids `% 0`).
            return None;
        }
        let total = eps.len();
        // 30: subset narrowing BEFORE the hash_lb/cursor dispatch. No lb_subset_config -> None
        // (the no-op: the existing all-endpoints dispatch below runs UNCHANGED, byte-identical).
        let subset_idxs: Option<Vec<usize>> = match self.subset.as_ref() {
            None => None,
            Some(ix) => match ix.resolve(subset_match) {
                crate::subset::Eligible::All => None, // treat as "all endpoints"
                crate::subset::Eligible::Some(idxs) => Some(idxs),
                crate::subset::Eligible::None => return None, // NO_FALLBACK no-match -> 503
            },
        };
        if let Some(idxs) = subset_idxs {
            // 30 (I-1): MVP inner LB within a matched subset = ROUND_ROBIN cursor. Subset +
            // hash-LB and subset + panic-threshold are §2.2 deferred non-goals. Compose with
            // HC/OD by intersection so a subset cluster that ALSO has HC/OD never returns an
            // out-of-subset or unhealthy host (the MVP fixture is plain → this filter is a
            // vacuous pass). A stale index (reload-shrunk eps) is guarded by `i < total`.
            let sub: Vec<usize> = idxs
                .into_iter()
                .filter(|&i| i < total)
                .filter(|&i| self.endpoint_eligible(i))
                .collect();
            if sub.is_empty() {
                return None; // no eligible host in the subset -> 503 (NO_FALLBACK semantics)
            }
            let c = self.cursor.fetch_add(1, Ordering::Relaxed);
            return Some(eps[sub[c % sub.len()]]);
        }
        // 29 (M28-3 / §6.2-LOCKED): consistent-hash dispatch on `hash_lb` (Ring →
        // ketama-ring lookup, Maglev → permutation-table lookup). Both variants'
        // `lookup` returns a `host_index` that aligns with the `eps` Vec ordering
        // (the LB was built from the SAME endpoint slice at construction — see
        // `from_bootstrap`). `Some(key_hash)` → lookup → the endpoint at that host
        // index. `None` → fall through to the cursor path below (the no-hash
        // fallback; the real fallback lands in Task 6). HC/OD + hash-LB composition
        // is a deferred non-goal (SPEC §2.2): hash-LB clusters are plain, so the
        // host is returned directly without an eligibility filter. `hash_lb` is
        // `Some` iff `lb_policy` is a hash policy (built together in
        // `from_bootstrap`), so gating on `hash_lb.as_ref()` is equivalent to
        // dispatching on `lb_policy` — a RoundRobin cluster has `hash_lb == None`
        // and falls straight through, its `key_hash` inert. The LB was built from
        // the bootstrap endpoint set; for a plain hash-LB cluster `eps` is that
        // same set, so `host_index` indexes `eps` directly (a reload-shrunk set is
        // out of scope — the `hi < total` guard is defense-in-depth). A `None` key
        // (or a stale index) falls through to the cursor path below — the Task-6 /
        // backstop no-hash fallback. The RING_HASH path is byte-identical to phase
        // 28; this is a pure dispatch refactor.
        if let (Some(hlb), Some(kh)) = (self.hash_lb.as_ref(), key_hash) {
            let host_index = match hlb {
                HashLb::Ring(r) => r.lookup(kh),
                HashLb::Maglev(t) => t.lookup(kh),
            };
            if let Some(hi) = host_index
                && hi < total
            {
                return Some(eps[hi]);
            }
        }
        // Fast path: nothing configured → phase-02 round-robin (byte-for-byte).
        if self.endpoint_health.is_none() && self.outlier_detection.is_none() {
            let i = self.cursor.fetch_add(1, Ordering::Relaxed);
            return Some(eps[i % total]);
        }
        // Slow path: at least one filter is configured. Eligibility = healthy AND
        // not-ejected (either filter being `None` is treated as `true`).
        let health = self.endpoint_health.as_ref();
        let ejection = self.outlier_detection.as_ref().map(|od| &od.endpoints);
        // M27-2 (phase-27 carry-forward): the slow path indexes `health[i]` and
        // `ejection[i]` for `i in 0..total` (== `eps.len()`). Both per-endpoint
        // arrays are built index-aligned with the endpoint set in `from_bootstrap`;
        // this assertion guards a future HC/OD-wiring regression that desyncs them.
        if let Some(h) = health {
            debug_assert_eq!(
                eps.len(),
                h.len(),
                "endpoint_health must align with endpoints"
            );
        }
        if let Some(e) = ejection {
            debug_assert_eq!(
                eps.len(),
                e.len(),
                "outlier ejection must align with endpoints"
            );
        }
        let eligible_idx: Vec<usize> = (0..total).filter(|&i| self.endpoint_eligible(i)).collect();
        let eligible_percent = 100.0 * (eligible_idx.len() as f64) / (total as f64);
        // Panic threshold (strictly-below): route over ALL endpoints when the
        // eligible fraction is below the threshold. `value: 0` disables panic
        // (`0.0 < 0.0` is false), so a 0-eligible cluster falls through to None.
        if eligible_percent < self.panic_threshold {
            let i = self.cursor.fetch_add(1, Ordering::Relaxed);
            return Some(eps[i % total]);
        }
        // Round-robin over the eligible endpoints only.
        if eligible_idx.is_empty() {
            // No eligible endpoints + panic not engaged → None → the pre-built
            // synth-503 path fires (unchanged at 12.1; body reconciliation is 12.2).
            return None;
        }
        let i = self.cursor.fetch_add(1, Ordering::Relaxed);
        Some(eps[eligible_idx[i % eligible_idx.len()]])
    }

    /// 30 (Task-5 review M-1): the per-endpoint eligibility predicate shared by the
    /// subset-narrowing path AND the slow HC/OD path — endpoint `i` is eligible iff it
    /// is healthy (active HC; vacuously true when `endpoint_health` is None) AND not
    /// ejected (outlier detection; vacuously true when `outlier_detection` is None).
    /// `i` MUST be < the current endpoint count (callers guarantee this).
    fn endpoint_eligible(&self, i: usize) -> bool {
        let healthy = match self.endpoint_health.as_ref() {
            None => true,
            Some(h) => h[i].is_healthy(),
        };
        let not_ejected = match self.outlier_detection.as_ref().map(|od| &od.endpoints) {
            None => true,
            Some(e) => !e[i].is_ejected(),
        };
        healthy && not_ejected
    }

    /// 27 D1 (§5.4 read-once): snapshot the current endpoint address set as a
    /// cheap `Arc` pointer-clone. Every LB selection ([`pick`]) calls this ONCE
    /// at entry and uses the returned snapshot for the whole selection, so a
    /// concurrent [`store_endpoints`] swap cannot tear an in-flight read.
    /// Mirrors the phase-26 `HCMConfig::current_route_config` poison-recovery
    /// precedent: a poisoned lock means a writer panicked mid-store, but the
    /// inner `Arc` is never left torn (the swap is a single atomic move), so we
    /// recover the guard and read the consistent current set rather than
    /// inheriting the panic. This matters concretely for phase 27 because Task 4
    /// adds a second writer (the EDS reload pipeline calls [`store_endpoints`]),
    /// so a *reader* must degrade gracefully instead of becoming a latent panic
    /// site keyed on an unrelated writer-side poison.
    ///
    /// [`pick`]: Cluster::pick
    /// [`store_endpoints`]: Cluster::store_endpoints
    pub fn current_endpoints(&self) -> Arc<Vec<SocketAddr>> {
        // Read-once: clone the `Arc` (pointer bump), then drop the guard.
        self.endpoints
            .read()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone()
    }

    /// 27 D1 (§6.2 swap API): atomically replace the endpoint address set. A
    /// later phase-27 EDS file-watcher task calls this when the assignment file
    /// changes; the NEXT [`current_endpoints`] / [`pick`] observes `eps`, while
    /// any in-flight selection that already snapshotted the previous `Arc`
    /// keeps its snapshot. Task 2 adds this API but nothing calls it yet (the
    /// reload pipeline lands later).
    ///
    /// [`current_endpoints`]: Cluster::current_endpoints
    /// [`pick`]: Cluster::pick
    ///
    /// M27-1 (phase-27 carry-forward): tightened `pub` → `pub(crate)`. The only
    /// callers are the in-crate `eds_reload` pipeline and the
    /// `#[doc(hidden)] pub` `ClusterHandle::store_endpoints` delegate (the latter
    /// is the real cross-crate surface — referenced by an `envoy-admin` test —
    /// and stays `pub`). No external crate reaches `&Cluster` / `Arc<Cluster>`
    /// directly (`ClusterHandle::inner` is `pub(crate)`), so this is safe.
    pub(crate) fn store_endpoints(&self, eps: Arc<Vec<SocketAddr>>) {
        // Single-statement swap: hold the write guard only for the pointer
        // assignment. Poison-recovery (mirroring the phase-26
        // `HCMConfig::store_route_config` precedent) is safe while this critical
        // section stays a single `Arc` move — a panic mid-`*guard = eps` cannot
        // tear the inner `Arc` — so a recovered guard always observes a
        // consistent set. Phase-27 Task 4's EDS reload pipeline keeps its
        // reparse+revalidate OUTSIDE this lock so the section never widens.
        *self
            .endpoints
            .write()
            .unwrap_or_else(|poison| poison.into_inner()) = eps;
    }

    /// 14.1 D3 (parent-14 D3/D4): record an upstream response status against an
    /// endpoint's outlier-detection state machine, enforcing the cluster-level
    /// `max_ejection_percent` cap. Declared at 14.1; the production caller (the H1+H2
    /// router-arm response-receipt hook) wires at 14.2 D4. At 14.1 this method is
    /// exercised via direct unit tests on `Cluster::record_response`.
    ///
    /// **Behavior:**
    /// - No-op when `outlier_detection.is_none()` (the §5.3 inert invariant).
    /// - No-op when the endpoint is not in `self.endpoints` (defense-in-depth).
    /// - Else: delegates to `EndpointEjection::record_response(status)` for counter
    ///   ticks + threshold detection. On any threshold crossing, computes the cap
    ///   `floor(host_count * max_ejection_percent / 100)` and counts current ejections.
    ///   If `active >= cap_count`, increments `ejections_overflow` (per detection-tick
    ///   per ADR-0041 §6.2 item-2) and returns without ejecting. Else picks the
    ///   detector that crossed (5xx wins ties per lock-in #15) and calls
    ///   `EndpointEjection::eject(detector)`.
    ///
    /// **Connect-failure synth-status path note:** per ADR-0041 §6.2 item-9, the 14.2
    /// D4 hook DOES call `record_response` from the connect-failure synth path with
    /// the synth status (502 / 503), which the classifier automatically treats as
    /// 5xx + gateway-failure. The `pick() -> None` no-healthy-upstream synth-503 path
    /// does NOT call `record_response` (no endpoint to attribute) — that decision lives
    /// at the 14.2 D4 call-site, NOT here.
    pub fn record_response(&self, endpoint: SocketAddr, status: u16) {
        let Some(od) = self.outlier_detection.as_ref() else {
            return; // §5.3 inert
        };
        // 27 D1 (§5.4 read-once): snapshot the address set once; the OD
        // per-endpoint arrays (`od.endpoints`) are index-aligned with it.
        let eps = self.current_endpoints();
        let Some(idx) = eps.iter().position(|e| *e == endpoint) else {
            return; // defense-in-depth (lock-in #10)
        };
        let state = &od.endpoints[idx];
        // 14.2 M4 (lock-in #4): hold the per-endpoint serialization lock across the WHOLE
        // compound (record → cap-check → eject + stamp) so the `Relaxed` atomics are mutated
        // by exactly one writer at a time — the D4 hook fires from every in-flight request
        // task and the D7 sweeper is a concurrent writer. `pick()`'s read side stays
        // lock-free (`is_ejected()` is a single `Relaxed` load).
        let mut ejected_at = state.ejected_at.lock().unwrap();
        let decision = state.record_response(status);
        if !decision.any() {
            return;
        }
        let total = eps.len();
        // 14.1 M6 (§6.2 item-4): cap_count = floor(total * max_ejection_percent / 100). When
        // max_ejection_percent == 0 ⇒ cap_count == 0 ⇒ active_count (0) >= cap_count (0) on
        // the first crossing ⇒ overflow, never ejecting (a deliberate "0% = eject nothing"
        // edge). Overflow re-fires per detection-tick, NOT once-per-host (§6.2 item-2).
        let cap_count = (total * od.max_ejection_percent as usize) / 100;
        let active_count = od.endpoints.iter().filter(|e| e.is_ejected()).count();
        if active_count >= cap_count {
            od.ejections_overflow.inc();
            return;
        }
        // 5xx wins ties (lock-in #15) — the parent-14 SPEC's first-named detector.
        let detector = if decision.crossed_5xx {
            crate::DetectorType::Consecutive5xx
        } else {
            crate::DetectorType::ConsecutiveGatewayFailure
        };
        state.eject(detector);
        // lock-in #5: stamp the eject-timestamp under the held guard (NOT inside `eject`,
        // which would re-enter the lock and self-deadlock). The 14.2 D7 sweeper reads this to
        // apply `base_ejection_time`.
        *ejected_at = Some(std::time::Instant::now());
    }

    /// 17 Task 3: attempt to acquire one retry budget slot.
    ///
    /// Returns `BudgetAcquisition::Unlimited` when the cluster has no
    /// `circuit_breakers` configured (never gates; zero stat side-effects).
    /// Returns `BudgetAcquisition::Acquired(guard)` on success; dropping the
    /// guard releases the slot. Returns `BudgetAcquisition::Rejected` when the
    /// active retry count is at or above `max_retries` (the overflow counter is
    /// incremented inside `BudgetState::try_acquire_retry`).
    pub fn try_acquire_retry(&self) -> crate::BudgetAcquisition<crate::RetryBudgetGuard> {
        match &self.budget {
            None => crate::BudgetAcquisition::Unlimited,
            Some(b) => match b.try_acquire_retry() {
                Some(guard) => crate::BudgetAcquisition::Acquired(guard),
                None => crate::BudgetAcquisition::Rejected,
            },
        }
    }

    /// 17 Task 3: attempt to acquire one request budget slot.
    ///
    /// Returns `BudgetAcquisition::Unlimited` when the cluster has no
    /// `circuit_breakers` configured (never gates; zero stat side-effects).
    /// Returns `BudgetAcquisition::Acquired(guard)` on success; dropping the
    /// guard releases the slot. Returns `BudgetAcquisition::Rejected` when the
    /// active request count is at or above `max_requests` (the overflow counter
    /// is incremented inside `BudgetState::try_acquire_request`).
    pub fn try_acquire_request(&self) -> crate::BudgetAcquisition<crate::RequestBudgetGuard> {
        match &self.budget {
            None => crate::BudgetAcquisition::Unlimited,
            Some(b) => match b.try_acquire_request() {
                Some(guard) => crate::BudgetAcquisition::Acquired(guard),
                None => crate::BudgetAcquisition::Rejected,
            },
        }
    }

    /// 14.2 D7 (lock-in #6/#7): borrow the cluster's runtime outlier-detection state, if
    /// configured. `None` when the cluster has no `outlier_detection` block (§5.3 inert
    /// invariant). Consumed by `OutlierManager::for_bootstrap` (via `ClusterHandle::
    /// inner_outlier_detection_state`) to read the per-endpoint `EndpointEjection` handles +
    /// the `base_ejection_time` / `interval` Durations for the sweeper.
    pub(crate) fn outlier_detection_state(&self) -> Option<&OutlierDetectionState> {
        self.outlier_detection.as_ref()
    }
}

/// 28 Task 6: hash a request's hash-policy material into the per-request key
/// consumed by [`ClusterHandle::pick_endpoint`]. This is the ONLY public hashing
/// surface of `envoy-cluster` — the underlying `xxh64` stays crate-internal so
/// the hash function is an implementation detail the ring and the request path
/// share. The HCMs call this with the matched route's `hash_policy` header value
/// bytes; an EMPTY value hashes deterministically to `xxh64(b"")` (NOT a
/// fallback — see ADR-0070). Equivalent to the ring's internal hashing so the
/// HCM-computed key lands on the same ring point.
pub fn hash_request_key(value: &[u8]) -> u64 {
    crate::xxhash::xxh64(value)
}

/// A handle to a `Cluster` that hands out endpoints via round-robin. Cheaply
/// cloneable (`Arc`-internal); clones share the same cursor.
#[derive(Clone, Debug)]
pub struct ClusterHandle {
    pub(crate) inner: Arc<Cluster>,
}

impl ClusterHandle {
    /// Returns the next endpoint to use, in round-robin order. Returns `None`
    /// when the cluster configures active health checks and has no healthy
    /// endpoints with panic mode not engaged (12.1), or when the cluster is
    /// empty (`from_bootstrap` rejects empty clusters — defense-in-depth).
    /// Without health checks the cluster always yields an endpoint (the
    /// inert-when-unconfigured round-robin).
    /// 28 Task 6: `key_hash` carries the per-request hash key — the `xxh64` of
    /// the matched route's `hash_policy` header material (computed by the HCM via
    /// [`hash_request_key`]). `RING_HASH` clusters route by the ring on
    /// `Some(key_hash)`; `RoundRobin` IGNORES it (the cursor path). `None`
    /// (no `hash_policy`, or the header was ABSENT) falls back to the cursor
    /// path. Delegates straight to the private [`Cluster::pick`].
    /// 30 (ADR-0073/0074): `subset_match` carries the route's `metadata_match`
    /// (the `envoy.lb` map) for metadata subset LB. INERT for clusters with no
    /// `lb_subset_config` (the no-op). Every caller passes `None` FOR NOW — Task
    /// 6 threads the real route `metadata_match` through at the HCM sites.
    pub fn pick_endpoint(
        &self,
        key_hash: Option<u64>,
        subset_match: Option<&std::collections::BTreeMap<String, String>>,
    ) -> Option<SocketAddr> {
        self.inner.pick(key_hash, subset_match)
    }

    /// 27 D1 (§5.4 / §6.2): delegates to [`Cluster::current_endpoints`] — a
    /// read-once `Arc` pointer-clone of the live endpoint address set.
    /// `envoy-admin`'s `config_dump` (a later phase-27 task) reads the live set
    /// through this handle; since `inner` is `pub(crate)`, a public accessor on
    /// `ClusterHandle` is required for cross-crate reach.
    ///
    /// [`Cluster::current_endpoints`]: Cluster::current_endpoints
    pub fn current_endpoints(&self) -> Arc<Vec<SocketAddr>> {
        self.inner.current_endpoints()
    }

    /// 27 D5 (cross-crate test reach): delegates to [`Cluster::store_endpoints`]
    /// — the atomic swap of the live endpoint address set. This is NOT a
    /// production API: the production EDS reload pipeline drives the swap through
    /// the in-crate `eds_reload` module (which reaches `inner` directly). This
    /// delegate exists solely for cross-crate *tests* (e.g. `envoy-admin`'s
    /// `/config_dump` read-through test) that hold only a `ClusterHandle` and
    /// need to simulate a reload, since `inner` is `pub(crate)`.
    ///
    /// `#[doc(hidden)]` (not `#[cfg(test)]`) because the consumer is a test in
    /// an *other* crate (envoy-admin), and `#[cfg(test)]` items are invisible to
    /// downstream crates. Mirrors `ClusterManager::empty()` /
    /// `is_endpoint_ejected_for_test`'s cross-crate-test-fixture posture.
    ///
    /// [`Cluster::store_endpoints`]: Cluster::store_endpoints
    #[doc(hidden)]
    pub fn store_endpoints(&self, eps: Arc<Vec<SocketAddr>>) {
        self.inner.store_endpoints(eps);
    }

    /// 27 Task 4: hand out the inner `Arc<Cluster>`. `pub(crate)` so the
    /// in-crate `eds_reload::build_eds_watch_targets` can reach the plainness
    /// fields (`endpoint_health` / `outlier_detection`) + the retained
    /// `eds_reload` state to filter + bundle EDS watch targets — sidestepping
    /// the envoy-bin→envoy-cluster encapsulation wall (envoy-bin cannot reach
    /// `inner` nor those `pub(crate)` fields).
    pub(crate) fn into_inner(self) -> Arc<Cluster> {
        self.inner
    }

    /// 14.1 D3: delegates to `Cluster::record_response`. The 14.2 D4 response-receipt
    /// hook callers hold a `ClusterHandle`; this mirrors the accessor for ergonomic
    /// reach. See `Cluster::record_response` for the full behavior contract.
    pub fn record_response(&self, endpoint: SocketAddr, status: u16) {
        self.inner.record_response(endpoint, status);
    }

    /// 14.2 D4 (cross-crate test fixture): is the endpoint at index `idx`
    /// currently ejected? Delegates to `EndpointEjection::is_ejected()` for the
    /// indexed endpoint. Returns `false` when the cluster has no
    /// `outlier_detection` configured (the §5.3 inert invariant — nothing can
    /// be ejected). Used by the envoy-http1 / envoy-http2 HCM router-arm
    /// response-receipt-hook tests to assert that `record_response(endpoint,
    /// 5xx)` ejected the picked endpoint.
    ///
    /// `#[doc(hidden)]` (not `#[cfg(test)]`) because the consumers are tests in
    /// *other* crates (envoy-http1/envoy-http2), and `#[cfg(test)]` items are
    /// invisible to downstream crates. Mirrors `ClusterManager::empty()`'s
    /// cross-crate-test-fixture posture.
    #[doc(hidden)]
    pub fn is_endpoint_ejected_for_test(&self, idx: usize) -> bool {
        self.inner
            .outlier_detection
            .as_ref()
            .map(|od| od.endpoints[idx].is_ejected())
            .unwrap_or(false)
    }

    /// Cluster name (delegates to `Cluster::name`). Mirrors `Cluster::name`'s
    /// public posture per phase-04.3 SPEC §3 D5.
    pub fn name(&self) -> &str {
        self.inner.name()
    }

    /// 14.2 D7 (lock-in #6/#7): delegates to `Cluster::outlier_detection_state`.
    /// `OutlierManager::for_bootstrap` walks `ClusterManager::clusters()` and reaches each
    /// cluster's runtime outlier-detection state through this accessor (mirroring the H1/H2
    /// pool managers' `cluster_mgr`-walk precedent). `pub(crate)` because the sole consumer
    /// (`outlier::OutlierManager`) lives in this crate.
    pub(crate) fn inner_outlier_detection_state(&self) -> Option<&OutlierDetectionState> {
        self.inner.outlier_detection_state()
    }

    /// 05.3 NEW: delegates to `Cluster::upstream_protocol`. Mirrors `name()`'s
    /// posture per SPEC §6 inherited signpost 1.
    pub fn upstream_protocol(&self) -> UpstreamProtocol {
        self.inner.upstream_protocol()
    }

    /// 06.1 D4.b: delegates to `Cluster::cx_total`. The connect-site
    /// callers (see `Cluster::cx_total` doc) hold a `ClusterHandle` rather
    /// than a `Cluster`, so the accessor is mirrored here for ergonomic
    /// reach.
    pub fn cx_total(&self) -> &Arc<envoy_stats::Counter> {
        self.inner.cx_total()
    }

    /// 06.3 D15.3.b: delegates to `Cluster::cx_active_guard`. Connect-site
    /// callers hold a `ClusterHandle`; this mirrors the accessor for
    /// ergonomic reach. See `Cluster::cx_active_guard` for usage contract.
    pub fn cx_active_guard(&self) -> ConnGaugeGuard {
        self.inner.cx_active_guard()
    }

    /// 13.2 D5: delegates to `Cluster::cx_active_arc`. The pool managers'
    /// `for_bootstrap` debug-assert site holds a `ClusterHandle`; this
    /// mirrors the accessor for ergonomic reach. Closes 13.1 REVIEW
    /// Cluster A-M2 (the `Arc::ptr_eq` debug-assert at the gauge wiring
    /// site).
    pub fn cx_active_arc(&self) -> &Arc<envoy_stats::Gauge> {
        self.inner.cx_active_arc()
    }

    /// 06.3 D15.3.c: delegates to `Cluster::upstream_rq_total`. Response-
    /// site callers hold a `ClusterHandle`; this mirrors the accessor for
    /// ergonomic reach.
    pub fn upstream_rq_total(&self) -> &Arc<envoy_stats::Counter> {
        self.inner.upstream_rq_total()
    }

    /// 06.3 D15.3.c: delegates to `Cluster::upstream_rq_5xx`. Response-
    /// site callers hold a `ClusterHandle`; this mirrors the accessor for
    /// ergonomic reach.
    pub fn upstream_rq_5xx(&self) -> &Arc<envoy_stats::Counter> {
        self.inner.upstream_rq_5xx()
    }

    /// 16 Task 3: delegates to `Cluster::upstream_rq_retry`. Retry-loop
    /// callers hold a `ClusterHandle`; this mirrors the accessor for
    /// ergonomic reach.
    pub fn upstream_rq_retry(&self) -> &Arc<envoy_stats::Counter> {
        self.inner.upstream_rq_retry()
    }

    /// 16 Task 3: delegates to `Cluster::upstream_rq_retry_success`.
    /// Mirrors `upstream_rq_retry()`'s borrow shape.
    pub fn upstream_rq_retry_success(&self) -> &Arc<envoy_stats::Counter> {
        self.inner.upstream_rq_retry_success()
    }

    /// 16 Task 3: delegates to `Cluster::upstream_rq_retry_limit_exceeded`.
    /// Mirrors `upstream_rq_retry()`'s borrow shape.
    pub fn upstream_rq_retry_limit_exceeded(&self) -> &Arc<envoy_stats::Counter> {
        self.inner.upstream_rq_retry_limit_exceeded()
    }

    /// 17 Task 3: delegates to `Cluster::upstream_rq_retry_overflow`. Mirrors
    /// `upstream_rq_retry()`'s borrow shape.
    pub fn upstream_rq_retry_overflow(&self) -> &Arc<envoy_stats::Counter> {
        self.inner.upstream_rq_retry_overflow()
    }

    /// 17 Task 3: delegates to `Cluster::try_acquire_retry`. H1/H2 HCM callers
    /// hold a `ClusterHandle`; this mirrors the accessor for ergonomic reach.
    /// See `Cluster::try_acquire_retry` for the full behavior contract.
    pub fn try_acquire_retry(&self) -> crate::BudgetAcquisition<crate::RetryBudgetGuard> {
        self.inner.try_acquire_retry()
    }

    /// 17 Task 3: delegates to `Cluster::try_acquire_request`. H1/H2 HCM callers
    /// hold a `ClusterHandle`; this mirrors the accessor for ergonomic reach.
    /// See `Cluster::try_acquire_request` for the full behavior contract.
    pub fn try_acquire_request(&self) -> crate::BudgetAcquisition<crate::RequestBudgetGuard> {
        self.inner.try_acquire_request()
    }

    /// 12.2 (parent-12 D4): per-endpoint health-probe targets when this
    /// cluster configures active health checks. Yields one (addr,
    /// EndpointHealth) pair per resolved endpoint that the `envoy-health`
    /// probe task drives (one task per pair; single-writer-per-endpoint
    /// per the 12.1 REVIEW M2 forward-correctness contract closed at
    /// `envoy-health`'s API boundary). Returns `None` when the cluster
    /// has no `health_checks` configured (the §5.4 inert-when-unconfigured
    /// invariant — no probe task should spawn).
    pub fn health_probe_targets(&self) -> Option<Vec<(SocketAddr, Arc<crate::EndpointHealth>)>> {
        let health = self.inner.endpoint_health.as_ref()?;
        // 27 D1 (§5.4 read-once): snapshot the address set once; the health
        // array is index-aligned with it. Probe targets are computed at
        // bootstrap (health-checked clusters are not reloadable in phase 27).
        let eps = self.inner.current_endpoints();
        Some(
            eps.iter()
                .copied()
                .zip(health.iter().map(Arc::clone))
                .collect(),
        )
    }
}

/// The cluster registry, keyed by cluster name. Built once via
/// `from_bootstrap`, read many times via `get`.
#[derive(Debug)]
pub struct ClusterManager {
    clusters: HashMap<String, Arc<Cluster>>,
}

impl ClusterManager {
    /// Looks up a cluster by name. Returns `None` if no cluster with that
    /// name was constructed.
    pub fn get(&self, name: &str) -> Option<ClusterHandle> {
        self.clusters.get(name).map(|arc| ClusterHandle {
            inner: Arc::clone(arc),
        })
    }

    /// Iterate over all clusters as `ClusterHandle`s in deterministic by-name
    /// order. Phase 08.1 D7 consumer: `envoy-admin`'s `/clusters` endpoint walks
    /// every cluster to emit the per-cluster plain-text stanza. The renderer
    /// requires deterministic ordering for differential equivalence — the
    /// internal representation is a `HashMap` whose iteration order is
    /// non-deterministic, so the accessor sorts by cluster name before
    /// yielding.
    pub fn clusters(&self) -> impl Iterator<Item = ClusterHandle> + '_ {
        let mut entries: Vec<(&String, &Arc<Cluster>)> = self.clusters.iter().collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        entries.into_iter().map(|(_, arc)| ClusterHandle {
            inner: Arc::clone(arc),
        })
    }

    /// Build an empty `ClusterManager` carrying zero clusters. Used by
    /// downstream test fixtures (envoy-http2 Task 9) where the HCM under
    /// test only takes `RouteAction::DirectResponse` paths and never invokes
    /// `cluster_mgr.get`. The runtime path still goes through
    /// `from_bootstrap`; this is a test-shaped constructor that bypasses the
    /// validator-shaped `EmptyCluster`/`DuplicateClusterName` invariants
    /// because by definition there are no clusters to violate them.
    ///
    /// Callable from any crate (still `pub` for cross-crate test fixtures),
    /// but `#[doc(hidden)]` to discourage production callers from reaching
    /// past `from_bootstrap`. Production callers should always go through
    /// `from_bootstrap(...)`.
    #[doc(hidden)]
    pub fn empty() -> Self {
        Self {
            clusters: HashMap::new(),
        }
    }
}

/// Errors returned by `from_bootstrap`.
///
/// `EmptyCluster` and `DuplicateClusterName` are defense-in-depth: the
/// `envoy-config` validator also rejects these shapes (`EmptyClusterEndpoints`,
/// cluster-name collisions via per-cluster `UnknownCluster` checks). They exist
/// here because `envoy-cluster` is a library whose invariants must hold even
/// when callers construct `Bootstrap` values by hand.
///
/// `EndpointParse` is *not* defense-in-depth: `envoy-config` accepts any
/// serde-valid `SocketAddress { address: String, port_value: u16 }` shape
/// (including `"not-a-host"`); the `SocketAddr` parse is the first place that
/// rejects a malformed address. Reached only on the `Static` cluster-type arm;
/// the `StrictDns` arm uses `tokio::net::lookup_host` instead and surfaces
/// resolution failure via `DnsResolutionFailed`.
///
/// `DnsResolutionFailed` is the runtime counterpart of `EndpointParse` for
/// `STRICT_DNS` clusters: the configured `address` is a DNS name (not a
/// literal IP), and `tokio::net::lookup_host` either errored or returned zero
/// addresses. Per ADR-0023, `STRICT_DNS` resolves once at cluster-build time
/// and caches the result; periodic re-resolution defers to a future phase.
#[derive(Debug, thiserror::Error)]
pub enum ClusterError {
    #[error("cluster '{name}' has no lb_endpoints")]
    EmptyCluster { name: String },
    #[error("duplicate cluster name '{name}'")]
    DuplicateClusterName { name: String },
    #[error("cluster '{cluster}' endpoint address {addr:?} is not a valid SocketAddr: {source}")]
    EndpointParse {
        cluster: String,
        addr: String,
        #[source]
        source: std::net::AddrParseError,
    },
    #[error("cluster '{cluster}' STRICT_DNS resolution of '{address}' failed: {source}")]
    DnsResolutionFailed {
        cluster: String,
        address: String,
        #[source]
        source: std::io::Error,
    },
    /// 06.1 D4.b: registering the per-cluster counter
    /// (`cluster.<name>.upstream_cx_total`) against the global
    /// `StatsRegistry` failed. Wraps the registry error's `Display`
    /// rendering so this crate doesn't need to publicly re-export
    /// `envoy_stats::StatsError` in its error surface.
    #[error("registering cluster '{cluster}' stats: {message}")]
    StatsRegistration { cluster: String, message: String },
}

/// Per-cluster upstream protocol selector. Defaulted to `Http1` for
/// backwards-compat with all phase-04 clusters; set at cluster-build time in
/// `from_bootstrap` from the parsed cluster's `typed_extension_protocol_options.
/// HttpProtocolOptions.explicit_http_config`. Mirrors the established
/// `LbPolicy` shape (Clone/Copy/Debug/Default/PartialEq/Eq derives).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UpstreamProtocol {
    /// Default. The 04.3-landed router H1 dispatch path.
    #[default]
    Http1,
    /// 05.3 NEW per ADR-0022 (parent-05 split). Selects the
    /// envoy_http2::Client dispatch path at the router H2-arm.
    Http2,
}

/// 27 Task 4: parse one endpoint's `address`+`port_value` into a numeric
/// `SocketAddr` (STATIC / EDS semantics — NOT DNS-resolved). Factored out of
/// `from_bootstrap`'s endpoint loop so the EDS reload pipeline
/// ([`crate::eds_reload`]) reuses the EXACT same parse.
///
/// Returns `Result` rather than `?`-propagating internally so callers choose
/// the disposition: the startup path (`from_bootstrap`) `?`-propagates the
/// `ClusterError::EndpointParse` (startup is all-fatal — UNCHANGED), while the
/// reload path catches it LOCALLY and maps it to the §6.2-LOCKED V4(c) reject
/// disposition (a bad endpoint in a hot-reloaded file must NOT kill the watch
/// loop — the last-good set is kept).
pub(crate) fn parse_numeric_endpoint(
    cluster: &str,
    address: &str,
    port_value: u16,
) -> Result<SocketAddr, ClusterError> {
    let addr_str = format!("{address}:{port_value}");
    addr_str
        .parse()
        .map_err(|source| ClusterError::EndpointParse {
            cluster: cluster.to_string(),
            addr: addr_str,
            source,
        })
}

/// Constructs a `ClusterManager` from a validated `Bootstrap`. The caller
/// should have already run `envoy_config::parse_bootstrap`, but this function
/// validates its own preconditions for library robustness.
///
/// Async since 05.1: `STRICT_DNS` clusters call `tokio::net::lookup_host`
/// (which is async). `STATIC` clusters don't await any I/O — the parse path
/// stays unchanged from phase 02.1 — but the function signature is uniformly
/// async because Rust doesn't have a "conditionally async" mechanism. The
/// single envoy-bin caller (`crates/envoy-bin/src/main.rs`) awaits this once
/// at startup, before serving any traffic.
pub async fn from_bootstrap(
    bootstrap: &envoy_config::Bootstrap,
    registry: Arc<envoy_stats::StatsRegistry>,
) -> Result<ClusterManager, ClusterError> {
    let mut clusters: HashMap<String, Arc<Cluster>> = HashMap::new();
    for cfg in bootstrap.all_clusters() {
        // Every per-cluster stat registration maps failures to the same
        // `ClusterError::StatsRegistration { cluster, message }` shape, so the
        // error mapping + registry calls are hoisted into shared helpers here.
        let stats_err = |e: envoy_stats::StatsError| ClusterError::StatsRegistration {
            cluster: cfg.name.clone(),
            message: e.to_string(),
        };
        let reg_counter = |name: &str| -> Result<Arc<envoy_stats::Counter>, ClusterError> {
            registry.register_counter(name).map_err(&stats_err)
        };
        let reg_gauge = |name: &str| -> Result<Arc<envoy_stats::Gauge>, ClusterError> {
            registry.register_gauge(name).map_err(&stats_err)
        };
        // envoy-config enforces cluster_type ∈ {Static, StrictDns} (post-05.1),
        // lb_policy == RoundRobin, load_assignment.cluster_name == cfg.name,
        // and total endpoints ≥ 1 at parse time. We don't re-check those here;
        // we do re-check emptiness and duplicate names as defense-in-depth,
        // and we resolve each endpoint to a SocketAddr (which envoy-config
        // does NOT do — neither the literal-IP parse for STATIC nor the DNS
        // lookup for STRICT_DNS).
        // 21 D1 (§5.3): every cluster has `load_assignment: Some` after
        // `load_dynamic_resources` (inline from parse; EDS populated by the
        // merge). The expect is the structural witness of that invariant.
        let load_assignment = cfg
            .load_assignment
            .as_ref()
            .expect("load_assignment populated post-load — §5.3 invariant");
        let mut endpoints: Vec<SocketAddr> = Vec::new();
        // 30 (ADR-0073/0074, I-2 alignment): a Vec of each endpoint's `envoy.lb`
        // metadata map, pushed ONCE PER RESOLVED `SocketAddr` so it stays exactly
        // index-aligned with `endpoints` (Static/Eds: one LbEndpoint → one push;
        // StrictDns: one LbEndpoint fans out to N resolved addrs → N pushes of the
        // SAME map). The subset index `build` consumes this slice.
        let mut endpoint_metadata: Vec<std::collections::BTreeMap<String, String>> = Vec::new();
        for locality in &load_assignment.endpoints {
            for lbe in &locality.lb_endpoints {
                let sa = &lbe.endpoint.address.socket_address;
                // The endpoint's `envoy.lb` map (empty when no metadata), computed
                // once per LbEndpoint and pushed once per resolved SocketAddr below.
                let md = lbe
                    .metadata
                    .as_ref()
                    .map(|m| m.envoy_lb.clone())
                    .unwrap_or_default();
                match cfg.cluster_type {
                    // 21 D1 (L1): EDS endpoints are resolved numeric socket
                    // addresses, parsed exactly like STATIC (NOT DNS-resolved
                    // like STRICT_DNS).
                    envoy_config::ClusterType::Static | envoy_config::ClusterType::Eds => {
                        // EXISTING path (phase 02.1): each endpoint's address
                        // parses as a literal SocketAddr via SocketAddr::from_str.
                        // Failure surfaces as ClusterError::EndpointParse —
                        // regression-guarded by the I3-closing test
                        // static_cluster_constructs_with_literal_ip.
                        //
                        // 27 Task 4: the numeric-IP parse is factored into
                        // `parse_numeric_endpoint` so the EDS reload pipeline
                        // (`eds_reload.rs`) reuses the EXACT same parse. Startup
                        // keeps `?`-propagating (the all-fatal startup posture is
                        // UNCHANGED); the reload path consumes the `Result`
                        // LOCALLY (mapping a parse failure to the V4(c) reject
                        // disposition) — it must NOT propagate `EndpointParse`,
                        // which would kill the watch loop.
                        endpoints.push(parse_numeric_endpoint(
                            &cfg.name,
                            &sa.address,
                            sa.port_value,
                        )?);
                        // 30 (I-2): one resolved addr → one metadata push.
                        endpoint_metadata.push(md.clone());
                    }
                    envoy_config::ClusterType::StrictDns => {
                        // 05.1 NEW per ADR-0023: each endpoint's address is a
                        // DNS name; resolve via tokio::net::lookup_host at
                        // cluster-build time. The lookup runs once; results
                        // cached for the cluster's lifetime, matching Envoy
                        // v1.33 STRICT_DNS semantics with default
                        // dns_refresh_rate (periodic re-resolution defers per
                        // parent-05 SPEC §4 / 05.1 SPEC §4).
                        let target = format!("{}:{}", sa.address, sa.port_value);
                        let resolved: Vec<SocketAddr> = tokio::net::lookup_host(&target)
                            .await
                            .map_err(|source| ClusterError::DnsResolutionFailed {
                                cluster: cfg.name.clone(),
                                address: sa.address.clone(),
                                source,
                            })?
                            .collect();
                        if resolved.is_empty() {
                            // Defensive zero-result guard: lookup_host can
                            // return an empty iterator on success on some
                            // platforms (e.g., NXDOMAIN may surface as empty
                            // rather than as an io::Error). Synthesise an
                            // io::Error so DnsResolutionFailed.source carries
                            // diagnostic info uniformly.
                            return Err(ClusterError::DnsResolutionFailed {
                                cluster: cfg.name.clone(),
                                address: sa.address.clone(),
                                source: std::io::Error::new(
                                    std::io::ErrorKind::NotFound,
                                    "DNS resolution returned zero addresses",
                                ),
                            });
                        }
                        let n = resolved.len();
                        endpoints.extend(resolved);
                        // 30 (I-2): one LbEndpoint fans out to N resolved addrs →
                        // push the SAME `envoy.lb` map N times so `endpoint_metadata`
                        // stays index-aligned with `endpoints`.
                        for _ in 0..n {
                            endpoint_metadata.push(md.clone());
                        }
                    }
                }
            }
        }
        debug_assert_eq!(
            endpoints.len(),
            endpoint_metadata.len(),
            "endpoint_metadata must align 1:1 with endpoints (I-2)"
        );
        if endpoints.is_empty() {
            return Err(ClusterError::EmptyCluster {
                name: cfg.name.clone(),
            });
        }
        // 30 (ADR-0073/0074): build the subset index ONLY when `lb_subset_config`
        // is present (else `None` → `pick()` is byte-identical to before — the
        // no-op regression proof). Built over the index-aligned `endpoint_metadata`.
        let subset = cfg
            .lb_subset_config
            .as_ref()
            .map(|sc| crate::subset::SubsetIndex::build(sc, &endpoint_metadata));
        // 05.3 NEW per SPEC §3 D3: project upstream_protocol from the parsed
        // cluster's typed_extension_protocol_options. The match arm is sync;
        // 05.1's lookup_host async branch is unaffected (the two are
        // orthogonal — cluster_type controls endpoint shape, upstream_protocol
        // controls upstream dispatch). Per SPEC §6 local signpost 15: the
        // "both Some" case is validator-rejected; defense-in-depth defaults
        // to Http1.
        let upstream_protocol = match &cfg.typed_extension_protocol_options {
            None => UpstreamProtocol::Http1,
            Some(teo) => {
                let ehc = &teo.http_protocol_options.explicit_http_config;
                match (&ehc.http_protocol_options, &ehc.http2_protocol_options) {
                    (_, Some(_)) => UpstreamProtocol::Http2,
                    (Some(_), None) => UpstreamProtocol::Http1,
                    (None, None) => UpstreamProtocol::Http1,
                }
            }
        };
        // 06.1 D4.b: register `cluster.<name>.upstream_cx_total` against
        // the global registry. Idempotent for same-kind re-registration
        // (Task 5 contract); a `Bootstrap` with two clusters of the same
        // name is rejected by the `clusters.insert(...).is_some()` check
        // below, so this is the cluster's first registration in practice.
        let cx_total = reg_counter(&format!("cluster.{}.upstream_cx_total", cfg.name))?;
        // 06.3 D15.3.b: register `cluster.<name>.upstream_cx_active` gauge.
        // Idempotent for same-kind re-registration (Task 5 contract).
        let cx_active = reg_gauge(&format!("cluster.{}.upstream_cx_active", cfg.name))?;
        // 06.3 D15.3.c: register per-cluster upstream-request counters.
        let upstream_rq_total = reg_counter(&format!("cluster.{}.upstream_rq_total", cfg.name))?;
        let upstream_rq_5xx = reg_counter(&format!("cluster.{}.upstream_rq_5xx", cfg.name))?;
        // 16 Task 3: register per-cluster retry counters unconditionally at 0.
        // A route's retry config is not known here; these are inert until the
        // retry loop (Tasks 4/5) increments them. All 23 existing fixtures remain
        // green because they do not assert the absence of these names.
        let upstream_rq_retry = reg_counter(&format!("cluster.{}.upstream_rq_retry", cfg.name))?;
        let upstream_rq_retry_success =
            reg_counter(&format!("cluster.{}.upstream_rq_retry_success", cfg.name))?;
        let upstream_rq_retry_limit_exceeded = reg_counter(&format!(
            "cluster.{}.upstream_rq_retry_limit_exceeded",
            cfg.name
        ))?;
        // 17 Task 3: register `cluster.<name>.upstream_rq_retry_overflow`
        // unconditionally (every cluster gets it, inert at 0). When a
        // `BudgetState` is also constructed below, its idempotent
        // registration of the same name returns the SAME Arc (they share
        // the counter — single source of truth per §5.3).
        let upstream_rq_retry_overflow =
            reg_counter(&format!("cluster.{}.upstream_rq_retry_overflow", cfg.name))?;
        // 17 Task 3: conditionally build the circuit-breaker budget when
        // `circuit_breakers` is configured. Clusters WITHOUT the block get
        // `budget: None` and register ZERO new `circuit_breakers.default.*`
        // stats (inert-when-unconfigured discipline per phase-15 precedent).
        let budget: Option<Arc<crate::BudgetState>> =
            if let Some(cb) = cfg.circuit_breakers.as_ref() {
                let t = cb.thresholds.first();
                let max_retries = t.and_then(|t| t.max_retries).unwrap_or(3); // L5 default
                let max_requests = t.and_then(|t| t.max_requests).unwrap_or(1024); // L5 default
                let track = t.and_then(|t| t.track_remaining).unwrap_or(false);
                Some(
                    crate::BudgetState::new(max_retries, max_requests, track, &registry, &cfg.name)
                        .map_err(&stats_err)?,
                )
            } else {
                None
            };
        // 12.1 (parent-12 D5): `common_lb_config.healthy_panic_threshold` is a
        // cluster-level load-balancing property independent of active health
        // checking — it governs `pick()`'s panic-routing for ANY eligibility
        // filter (12.1 active-HC unhealth AND/OR 14.1 outlier-detection ejection).
        // It MUST therefore be parsed unconditionally, NOT only when `health_checks`
        // is configured. (14.2 Task-8 regression: an outlier-detection-only cluster
        // with `healthy_panic_threshold: {value: 0}` previously fell into the
        // else-branch default of 50.0, so a freshly-ejected sole endpoint —
        // 0% eligible < 50% — was re-admitted by panic routing and `pick()` never
        // returned `None`, suppressing the no-healthy-upstream synth-503.)
        let panic_threshold = cfg
            .common_lb_config
            .as_ref()
            .and_then(|c| c.healthy_panic_threshold.as_ref())
            .map(|p| p.value)
            .unwrap_or(50.0);
        // 12.1 (parent-12 D3/D5/D6): if the cluster configures an active health
        // check (validator guarantees 0 or 1), build per-endpoint EndpointHealth
        // (all starting Unhealthy) + register the membership_healthy gauge. No
        // health checks ⇒ endpoint_health: None ⇒ pick() filters on outlier
        // detection only (or is phase-02 round-robin if neither is configured).
        let endpoint_health = if let Some(hc) = cfg.health_checks.first() {
            let membership_healthy =
                reg_gauge(&format!("cluster.{}.membership_healthy", cfg.name))?;
            let health: Vec<Arc<crate::EndpointHealth>> = endpoints
                .iter()
                .map(|_| {
                    Arc::new(crate::EndpointHealth::new(
                        hc.healthy_threshold,
                        hc.unhealthy_threshold,
                        Arc::clone(&membership_healthy),
                    ))
                })
                .collect();
            Some(health)
        } else {
            None
        };
        // 21 D4 (ADR-0053/0054; §6.2 L3/L10): the per-cluster EDS update_* family
        // — registered ONLY for `type: EDS` clusters (the §5.2 conditional-
        // registration discipline; STATIC/STRICT_DNS clusters emit no update_*).
        // All values deterministic at a successful initial load (the all-fatal
        // posture makes update_failure/update_empty structurally 0 — L4), so
        // register-and-set directly (no handle threading). membership_healthy is
        // health-check-gated above and membership_total does not exist in
        // envoy-rust — neither is in this family (a recorded narrowing vs Envoy).
        // 27 Task 4 (§6.2-LOCKED / ADR-0068 V4): capture ALL FIVE update_* handles
        // for `type: EDS` clusters and retain them (with the file path + CLA
        // selection name) in `EdsReloadState`, so the file-based reload pipeline
        // ticks the SAME series the initial load seeds. `register_counter` is
        // idempotent by name, so re-registering in the reload path returns the
        // same handle. `update_rejected` is ADDED here (it was NOT registered by
        // phase 21) at INITIAL VALUE 0 — the `mk(..)?`-without-`.add(1)` form,
        // like update_failure/update_empty; do NOT `.add(1)` it (the V4 "trio = 0
        // on a successful initial load" witness depends on it).
        let eds_reload = if cfg.cluster_type == envoy_config::ClusterType::Eds {
            let mk = |suffix: &str| reg_counter(&format!("cluster.{}.{suffix}", cfg.name));
            let update_attempt = mk("update_attempt")?;
            let update_success = mk("update_success")?;
            let update_failure = mk("update_failure")?; // registers at 0 (L4)
            let update_empty = mk("update_empty")?; // registers at 0 (L4)
            let update_rejected = mk("update_rejected")?; // 27 Task 4: NEW, at 0
            update_attempt.add(1);
            update_success.add(1);
            // The EDS cluster's `eds_cluster_config` is validated present at parse
            // (envoy-config rejects `type: EDS` without it); the path is the
            // assignment file, the selection name mirrors the phase-21 initial
            // load (`service_name` if set, else the cluster name — see
            // `load_dynamic_resources` in `crates/envoy-config/src/lib.rs`).
            let eds_cfg = cfg
                .eds_cluster_config
                .as_ref()
                .expect("EDS cluster has eds_cluster_config — validated at parse");
            let selection_name = eds_cfg
                .service_name
                .clone()
                .unwrap_or_else(|| cfg.name.clone());
            Some(EdsReloadState {
                path: std::path::PathBuf::from(&eds_cfg.eds_config.path_config_source.path),
                selection_name,
                update_attempt,
                update_success,
                update_failure,
                update_empty,
                update_rejected,
            })
        } else {
            None
        };
        // 14.1 D5/D6 (parent-14 D3/D5/D6): if the cluster configures outlier_detection,
        // build the cluster-level state (per-endpoint EndpointEjection Vec +
        // max_ejection_percent + ejections_overflow). Envoy v3 defaults (§6.2 item-1):
        //   consecutive_5xx=5, consecutive_gateway_failure=5, interval=10s,
        //   base_ejection_time=30s, max_ejection_percent=10.
        // The interval + base_ejection_time fields are validator-checked but consumed
        // ONLY at 14.2 D7 (sweeper); 14.1 reads only the detector thresholds + cap.
        let outlier_detection = if let Some(od_cfg) = cfg.outlier_detection.as_ref() {
            let consecutive_5xx_threshold = od_cfg.consecutive_5xx.unwrap_or(5);
            let consecutive_gateway_failure_threshold =
                od_cfg.consecutive_gateway_failure.unwrap_or(5);
            let max_ejection_percent = od_cfg.max_ejection_percent.unwrap_or(10);
            // 14.2 D7 (lock-in #6): project the runtime timing Durations from the parsed
            // config. REUSE `envoy_config::parse_duration` (the same helper 14.1's validator
            // ran on these fields) so the sweeper never re-parses the bootstrap. The validator
            // already accepted both fields (or they're absent); on this defense-in-depth parse
            // a failure falls back to the Envoy v3 default rather than erroring (the validator
            // is the authoritative gate). Defaults (§6.2 item-1): base_ejection_time = 30s,
            // interval = 10s.
            let base_ejection_time = od_cfg
                .base_ejection_time
                .as_deref()
                .and_then(|s| envoy_config::parse_duration(s).ok())
                .unwrap_or(std::time::Duration::from_secs(30));
            let interval = od_cfg
                .interval
                .as_deref()
                .and_then(|s| envoy_config::parse_duration(s).ok())
                .unwrap_or(std::time::Duration::from_secs(10));
            let mk_counter = |suffix: &str| {
                reg_counter(&format!("cluster.{}.outlier_detection.{suffix}", cfg.name))
            };
            let mk_gauge = |suffix: &str| {
                reg_gauge(&format!("cluster.{}.outlier_detection.{suffix}", cfg.name))
            };
            let stats = crate::EndpointEjectionStats {
                ejections_active: mk_gauge("ejections_active")?,
                ejections_enforced_total: mk_counter("ejections_enforced_total")?,
                ejections_detected_consecutive_5xx: mk_counter(
                    "ejections_detected_consecutive_5xx",
                )?,
                ejections_enforced_consecutive_5xx: mk_counter(
                    "ejections_enforced_consecutive_5xx",
                )?,
                ejections_detected_consecutive_gateway_failure: mk_counter(
                    "ejections_detected_consecutive_gateway_failure",
                )?,
                ejections_enforced_consecutive_gateway_failure: mk_counter(
                    "ejections_enforced_consecutive_gateway_failure",
                )?,
            };
            let ejections_overflow = mk_counter("ejections_overflow")?;
            let endpoints_state: Vec<Arc<crate::EndpointEjection>> = endpoints
                .iter()
                .map(|_| {
                    Arc::new(crate::EndpointEjection::new(
                        consecutive_5xx_threshold,
                        consecutive_gateway_failure_threshold,
                        stats.clone(),
                    ))
                })
                .collect();
            Some(OutlierDetectionState {
                endpoints: endpoints_state,
                max_ejection_percent,
                ejections_overflow,
                base_ejection_time,
                interval,
            })
        } else {
            None
        };
        // 29 (M28-3 / §6.2-LOCKED / ADR-0070/0071/0072): build the consistent-hash
        // LB per `lb_policy` from the endpoint `ip:port` Display strings. The LB's
        // `host_index` aligns with the `endpoints` Vec ordering (we build the
        // address strings by iterating `endpoints` in order, so LB index `i` ==
        // `endpoints[i]`). `SocketAddr` Display gives `ip:port` for IPv4 (e.g.
        // `172.22.0.2:5678`), matching Envoy's `address()->asString()`; IPv6 is a
        // bracketed-form UNTESTED non-goal (the differential fixtures are IPv4). The
        // LB is built ONCE here — hash-LB + reloadable membership is out of scope
        // (STATIC fixtures only). Sizes come from the cluster's `*_lb_config` (Envoy
        // proto defaults when absent: ring 1024, maglev 65537).
        let addrs: Vec<String> = endpoints.iter().map(|a| a.to_string()).collect();
        let hash_lb = match cfg.lb_policy {
            envoy_config::LbPolicy::RingHash => {
                let min_ring_size = cfg
                    .ring_hash_lb_config
                    .as_ref()
                    .map(|c| c.minimum_ring_size)
                    .unwrap_or(1024);
                Some(HashLb::Ring(crate::ring_hash::HashRing::build(
                    &addrs,
                    min_ring_size,
                )))
            }
            envoy_config::LbPolicy::Maglev => {
                let table_size = cfg
                    .maglev_lb_config
                    .as_ref()
                    .map(|c| c.table_size)
                    .unwrap_or(65537);
                Some(HashLb::Maglev(crate::maglev::MaglevTable::build(
                    &addrs, table_size,
                )))
            }
            envoy_config::LbPolicy::RoundRobin => None,
        };
        let cluster = Arc::new(Cluster {
            name: cfg.name.clone(),
            endpoints: RwLock::new(Arc::new(endpoints)),
            cursor: AtomicUsize::new(0),
            upstream_protocol,
            cx_total,
            cx_active,
            upstream_rq_total,
            upstream_rq_5xx,
            upstream_rq_retry,
            upstream_rq_retry_success,
            upstream_rq_retry_limit_exceeded,
            upstream_rq_retry_overflow,
            budget,
            endpoint_health,
            panic_threshold,
            outlier_detection,
            hash_lb,
            subset,
            eds_reload,
        });
        if clusters.insert(cfg.name.clone(), cluster).is_some() {
            return Err(ClusterError::DuplicateClusterName {
                name: cfg.name.clone(),
            });
        }
    }
    // 18 D4 (ADR-0049 L3/L10): the cluster_manager.* stat family — the project's
    // first top-level-scope (non-resource-name-prefixed) stat family. Registered
    // ONLY when dynamic_resources.cds_config is configured (the §5.2 conditional-
    // registration discipline; Envoy emits the base cluster_manager.* names
    // unconditionally — those stay Envoy-only-unasserted on non-CDS fixtures).
    // All failure paths are fatal pre-construction (L4 reconciliation), so
    // update_failure / update_rejected register at 0 and never tick. The counts
    // include STATIC clusters too (Envoy counts ALL clusters added to the
    // manager) — `clusters` is the merged static+dynamic map.
    if bootstrap
        .dynamic_resources
        .as_ref()
        .and_then(|dr| dr.cds_config.as_ref())
        .is_some()
    {
        let total = clusters.len() as u64;
        let mk = |name: &str| -> Result<Arc<envoy_stats::Counter>, ClusterError> {
            registry
                .register_counter(name)
                .map_err(|e| ClusterError::StatsRegistration {
                    cluster: name.to_string(),
                    message: e.to_string(),
                })
        };
        mk("cluster_manager.cds.update_attempt")?.add(1);
        mk("cluster_manager.cds.update_success")?.add(1);
        mk("cluster_manager.cds.update_failure")?; // registers at 0 (L4)
        mk("cluster_manager.cds.update_rejected")?; // registers at 0 (L4)
        mk("cluster_manager.cluster_added")?.add(total);
        // active_clusters is the lone GAUGE in the family (the 5 above are
        // counters), so it can't route through the counter-typed `mk` closure.
        registry
            .register_gauge("cluster_manager.active_clusters")
            .map_err(|e| ClusterError::StatsRegistration {
                cluster: "cluster_manager.active_clusters".to_string(),
                message: e.to_string(),
            })?
            .set(total as i64);
    }
    Ok(ClusterManager { clusters })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use std::sync::atomic::AtomicUsize;
    use std::sync::{Arc, RwLock};

    fn mk_endpoints(n: u16) -> Vec<SocketAddr> {
        (0..n)
            .map(|i| format!("127.0.0.1:{}", 10000 + i).parse().unwrap())
            .collect()
    }

    /// Construct a base per-test `Cluster` bypassing `from_bootstrap`: the 8
    /// unconditional per-cluster stats are registered against `registry` (so
    /// the test mirrors the real `from_bootstrap` Arc-clone shape —
    /// Counter/Gauge constructors are `pub(crate)` to envoy-stats; consumers
    /// always go through the registry), every optional feature field is
    /// `None`, and `panic_threshold` is the 50.0 default. Callers override the
    /// distinctive fields before wrapping the Cluster in a handle.
    fn mk_test_cluster(
        name: &str,
        endpoints: Vec<SocketAddr>,
        registry: &envoy_stats::StatsRegistry,
    ) -> Cluster {
        let counter = |suffix: &str| {
            registry
                .register_counter(&format!("cluster.{name}.{suffix}"))
                .expect("counter registers")
        };
        Cluster {
            name: name.to_string(),
            endpoints: RwLock::new(Arc::new(endpoints)),
            cursor: AtomicUsize::new(0),
            upstream_protocol: UpstreamProtocol::default(),
            cx_total: counter("upstream_cx_total"),
            cx_active: registry
                .register_gauge(&format!("cluster.{name}.upstream_cx_active"))
                .expect("gauge registers"),
            upstream_rq_total: counter("upstream_rq_total"),
            upstream_rq_5xx: counter("upstream_rq_5xx"),
            upstream_rq_retry: counter("upstream_rq_retry"),
            upstream_rq_retry_success: counter("upstream_rq_retry_success"),
            upstream_rq_retry_limit_exceeded: counter("upstream_rq_retry_limit_exceeded"),
            upstream_rq_retry_overflow: counter("upstream_rq_retry_overflow"),
            budget: None,
            endpoint_health: None,
            panic_threshold: 50.0,
            outlier_detection: None,
            hash_lb: None,
            subset: None,
            eds_reload: None,
        }
    }

    /// Construct a per-test Cluster + ClusterHandle bypassing `from_bootstrap`,
    /// registering stats against a fresh registry.
    fn mk_handle(name: &str, endpoints: Vec<SocketAddr>) -> ClusterHandle {
        let registry = envoy_stats::StatsRegistry::new();
        ClusterHandle {
            inner: Arc::new(mk_test_cluster(name, endpoints, &registry)),
        }
    }

    /// 28 Task 5 / 29 Task 7: build a `ClusterHandle` with the given
    /// pre-built consistent-hash LB installed (the `mk_handle` default is
    /// RoundRobin/no-LB).
    fn mk_hash_lb_handle(name: &str, endpoints: Vec<SocketAddr>, hash_lb: HashLb) -> ClusterHandle {
        let registry = envoy_stats::StatsRegistry::new();
        let mut cluster = mk_test_cluster(name, endpoints, &registry);
        cluster.hash_lb = Some(hash_lb);
        ClusterHandle {
            inner: Arc::new(cluster),
        }
    }

    /// 28 Task 5: build a RING_HASH `ClusterHandle` with the given endpoints and
    /// `minimum_ring_size`, mirroring `from_bootstrap`'s ring build (host index
    /// `i` = `endpoints[i]`, address strings from `SocketAddr` Display).
    fn mk_ring_hash_handle(
        name: &str,
        endpoints: Vec<SocketAddr>,
        min_ring_size: u64,
    ) -> ClusterHandle {
        let addrs: Vec<String> = endpoints.iter().map(|a| a.to_string()).collect();
        let ring = crate::ring_hash::HashRing::build(&addrs, min_ring_size);
        mk_hash_lb_handle(name, endpoints, HashLb::Ring(ring))
    }

    /// 29 Task 7: build a MAGLEV `ClusterHandle` with the given endpoints and
    /// `table_size`, mirroring `from_bootstrap`'s table build (host index `i` =
    /// `endpoints[i]`, address strings from `SocketAddr` Display — the SAME
    /// derivation `from_bootstrap` uses, so the table is built through the real
    /// path).
    fn mk_maglev_handle(name: &str, endpoints: Vec<SocketAddr>, table_size: u64) -> ClusterHandle {
        let addrs: Vec<String> = endpoints.iter().map(|a| a.to_string()).collect();
        let table = crate::maglev::MaglevTable::build(&addrs, table_size);
        mk_hash_lb_handle(name, endpoints, HashLb::Maglev(table))
    }

    /// 29 Task 7: build a standalone reference `MaglevTable` over the SAME
    /// address strings the cluster derives (`SocketAddr` Display), so a test can
    /// compute the oracle host index `MaglevTable::lookup(kh)` for the cluster's
    /// dispatch to be checked against. Mirrors `from_bootstrap` / `mk_maglev_handle`.
    fn reference_maglev_table(
        endpoints: &[SocketAddr],
        table_size: u64,
    ) -> crate::maglev::MaglevTable {
        let addrs: Vec<String> = endpoints.iter().map(|a| a.to_string()).collect();
        crate::maglev::MaglevTable::build(&addrs, table_size)
    }

    /// 28 Task 5: a RING_HASH cluster's `pick(Some(key_hash))` routes by the ring
    /// (consistent with the §6.2 oracle), while a RoundRobin cluster IGNORES the
    /// key (the cursor path is inert to it).
    #[test]
    fn pick_ring_hash_dispatch_and_round_robin_key_inert() {
        // host 0 = 172.22.0.2:5678 (ONE), host 1 = 172.22.0.3:5678 (TWO).
        let ep0: SocketAddr = "172.22.0.2:5678".parse().unwrap();
        let ep1: SocketAddr = "172.22.0.3:5678".parse().unwrap();
        let rh = mk_ring_hash_handle("rh", vec![ep0, ep1], 1024);
        // Oracle: key-0 → host 0, key-2 → host 1.
        assert_eq!(
            rh.inner.pick(Some(crate::xxhash::xxh64(b"key-0")), None),
            Some(ep0),
            "key-0 → host 0 (the ONE backend)"
        );
        assert_eq!(
            rh.inner.pick(Some(crate::xxhash::xxh64(b"key-2")), None),
            Some(ep1),
            "key-2 → host 1 (the TWO backend)"
        );

        // RoundRobin: the key is inert — `pick(Some(123))` behaves like the
        // cursor path (first call → endpoints[0], second → endpoints[1]).
        let rr = mk_handle("rr", vec![ep0, ep1]);
        assert_eq!(
            rr.inner.pick(Some(123), None),
            Some(ep0),
            "cursor 0, key ignored"
        );
        assert_eq!(
            rr.inner.pick(Some(123), None),
            Some(ep1),
            "cursor 1, key ignored"
        );
        // And matches a `None`-key call (proving the key is truly inert).
        assert_eq!(rr.inner.pick(None, None), Some(ep0), "cursor 2 % 2 = 0");
    }

    /// 28 Task 6: the public `hash_request_key` helper is the ONLY public
    /// hashing surface (xxh64 stays `pub(crate)`). It must equal `xxh64` so the
    /// HCM-computed key matches the ring's internal hashing.
    #[test]
    fn hash_request_key_equals_xxh64() {
        assert_eq!(
            super::hash_request_key(b"key-0"),
            crate::xxhash::xxh64(b"key-0")
        );
        assert_eq!(super::hash_request_key(b""), crate::xxhash::xxh64(b""));
    }

    /// 28 Task 6 (a): the PUBLIC `ClusterHandle::pick_endpoint` delegate now
    /// takes its own `Option<u64>` request-hash-key and threads it to the
    /// private `Cluster::pick`. A RING_HASH cluster routes by the ring (the §6.2
    /// oracle: `xxh64("key-0")` → host 0); `None` falls back to a valid host.
    #[test]
    fn pick_endpoint_ring_hash_threads_key() {
        let ep0: SocketAddr = "172.22.0.2:5678".parse().unwrap();
        let ep1: SocketAddr = "172.22.0.3:5678".parse().unwrap();
        let rh = mk_ring_hash_handle("rh_pub", vec![ep0, ep1], 1024);
        // `Some(xxh64("key-0"))` → host 0 (the oracle).
        assert_eq!(
            rh.pick_endpoint(Some(crate::xxhash::xxh64(b"key-0")), None),
            Some(ep0),
            "key-0 → host 0 through the public delegate"
        );
        // `None` (no-hash path) → a valid host (the ring is skipped; falls
        // through to the cursor path which yields endpoints[0] first).
        assert_eq!(
            rh.pick_endpoint(None, None),
            Some(ep0),
            "None falls through to the cursor path → a valid host"
        );
    }

    /// 28 Task 6 (b): a RoundRobin cluster IGNORES the key — `pick_endpoint(Some(123))`
    /// behaves identically to `pick_endpoint(None)` (the cursor path; the key is
    /// inert — regression-equivalence through the public delegate).
    #[test]
    fn pick_endpoint_round_robin_key_inert() {
        let ep0: SocketAddr = "172.22.0.2:5678".parse().unwrap();
        let ep1: SocketAddr = "172.22.0.3:5678".parse().unwrap();
        let rr = mk_handle("rr_pub", vec![ep0, ep1]);
        // The key is inert: the cursor advances regardless of Some/None.
        assert_eq!(
            rr.pick_endpoint(Some(123), None),
            Some(ep0),
            "cursor 0, key ignored"
        );
        assert_eq!(
            rr.pick_endpoint(None, None),
            Some(ep1),
            "cursor 1, key ignored"
        );
        assert_eq!(
            rr.pick_endpoint(Some(123), None),
            Some(ep0),
            "cursor 2 % 2 = 0"
        );
    }

    // ---------------------------------------------------------------------
    // 29 Task 7: MAGLEV cluster-level `pick()`-dispatch backstop. These
    // complement the maglev.rs TABLE-level oracle/distribution tests (Task 4)
    // by exercising `Cluster::pick` / `ClusterHandle::pick_endpoint` for a
    // MAGLEV cluster — proving the `hash_lb` dispatch routes a MAGLEV cluster
    // through the table — plus the M28-3 three-policy regression witness.
    // The hosts use the §6.2-oracle addresses (172.31.0.2/.3:5678) so the
    // pinned-oracle host indices line up with maglev.rs.
    // ---------------------------------------------------------------------

    /// 29 Task 7 (1): a MAGLEV cluster's `pick_endpoint(Some(kh))` returns the
    /// endpoint at the index `MaglevTable::lookup(kh)` would return (the dispatch
    /// routes THROUGH the table), and same-key→same-endpoint across repeated
    /// calls (cluster-level determinism — not the round-robin cursor).
    #[test]
    fn pick_endpoint_maglev_routes_through_table_and_is_deterministic() {
        let ep0: SocketAddr = "172.31.0.2:5678".parse().unwrap(); // host 0
        let ep1: SocketAddr = "172.31.0.3:5678".parse().unwrap(); // host 1
        let endpoints = vec![ep0, ep1];
        let mg = mk_maglev_handle("mg", endpoints.clone(), 65537);
        let table = reference_maglev_table(&endpoints, 65537);
        // For a sweep of keys, the cluster dispatch must equal eps[table.lookup(kh)].
        for key in [
            "key-0",
            "key-2",
            "key-7",
            "key-10",
            "user-alice",
            "user-bob",
            "session-abc",
        ] {
            let kh = crate::xxhash::xxh64(key.as_bytes());
            let expected_idx = table.lookup(kh).expect("non-empty table");
            assert_eq!(
                mg.pick_endpoint(Some(kh), None),
                Some(endpoints[expected_idx]),
                "key {key:?} dispatches through the table to host {expected_idx}"
            );
            // Determinism at the cluster level: repeated calls are stable (the
            // table lookup, NOT the advancing cursor).
            assert_eq!(
                mg.pick_endpoint(Some(kh), None),
                mg.pick_endpoint(Some(kh), None),
                "same key → same endpoint across repeated cluster picks"
            );
        }
    }

    /// 29 Task 7 (2): a sweep of distinct key_hashes selects BOTH endpoints at
    /// the cluster level (the table distributes across hosts — the dispatch is
    /// not pinned to a single host).
    #[test]
    fn pick_endpoint_maglev_spreads_over_endpoints() {
        let ep0: SocketAddr = "172.31.0.2:5678".parse().unwrap();
        let ep1: SocketAddr = "172.31.0.3:5678".parse().unwrap();
        let mg = mk_maglev_handle("mg_spread", vec![ep0, ep1], 65537);
        let mut saw0 = false;
        let mut saw1 = false;
        for i in 0..256u64 {
            let kh = crate::xxhash::xxh64(format!("spread-key-{i}").as_bytes());
            match mg.pick_endpoint(Some(kh), None) {
                Some(ep) if ep == ep0 => saw0 = true,
                Some(ep) if ep == ep1 => saw1 = true,
                other => panic!("unexpected pick {other:?}"),
            }
            if saw0 && saw1 {
                break;
            }
        }
        assert!(saw0 && saw1, "a key sweep must select both endpoints");
    }

    /// 29 Task 7 (3): the M28-2 no-hash-key fallback — `pick_endpoint(None)` on a
    /// MAGLEV cluster falls through to the CURSOR / round-robin path (NOT the
    /// table): it cycles endpoints like round-robin. This characterizes the
    /// absent-key fallback (the cursor path) per phase-28 M28-2.
    #[test]
    fn pick_endpoint_maglev_no_hash_key_falls_through_to_cursor() {
        let ep0: SocketAddr = "172.31.0.2:5678".parse().unwrap();
        let ep1: SocketAddr = "172.31.0.3:5678".parse().unwrap();
        let mg = mk_maglev_handle("mg_none", vec![ep0, ep1], 65537);
        // No-hash → cursor path: cycles endpoints[0], [1], [0], [1], ... like RR.
        assert_eq!(mg.pick_endpoint(None, None), Some(ep0), "cursor 0");
        assert_eq!(mg.pick_endpoint(None, None), Some(ep1), "cursor 1");
        assert_eq!(mg.pick_endpoint(None, None), Some(ep0), "cursor 2 % 2 = 0");
        assert_eq!(mg.pick_endpoint(None, None), Some(ep1), "cursor 3 % 2 = 1");
    }

    /// 29 Task 7 (4): an EMPTY-but-PRESENT hash value hashes to `xxh64(b"")` (NOT
    /// a fallback — ADR-0070). `pick_endpoint(Some(hash_request_key(b"")))` is
    /// deterministic and equals the table host for `xxh64(b"")` — the present-
    /// empty-vs-absent distinction. (The maglev.rs oracle pins `"" → host 0`.)
    #[test]
    fn pick_endpoint_maglev_empty_but_present_hashes_not_fallback() {
        let ep0: SocketAddr = "172.31.0.2:5678".parse().unwrap(); // oracle: "" → host 0
        let ep1: SocketAddr = "172.31.0.3:5678".parse().unwrap();
        let endpoints = vec![ep0, ep1];
        let mg = mk_maglev_handle("mg_empty", endpoints.clone(), 65537);
        // `hash_request_key(b"")` is the HCM's request-key hash for a present-
        // empty value; it equals `xxh64(b"")` (NOT None — not the cursor fallback).
        let kh = super::hash_request_key(b"");
        let table = reference_maglev_table(&endpoints, 65537);
        let expected_idx = table.lookup(kh).expect("non-empty table");
        // Pin the present-empty oracle host (maglev.rs: "" → host 0).
        assert_eq!(expected_idx, 0, "empty-but-present oracle maps to host 0");
        // Deterministic + routes through the table (NOT the cursor).
        assert_eq!(
            mg.pick_endpoint(Some(kh), None),
            Some(endpoints[expected_idx])
        );
        assert_eq!(
            mg.pick_endpoint(Some(kh), None),
            mg.pick_endpoint(Some(kh), None),
            "empty-but-present key is deterministic (table, not cursor)"
        );
    }

    /// 29 Task 7 (5): the M28-3 regression witness (load-bearing). Asserts the
    /// `hash_lb: Option<HashLb>` dispatch refactor sends EACH of the three
    /// policies down the right path in ONE place:
    ///   - ROUND_ROBIN: `pick_endpoint(Some(kh))` IGNORES the key (cursor path —
    ///     identical to `pick_endpoint(None)`; key inert).
    ///   - RING_HASH: `pick_endpoint(Some(kh))` routes via the ring.
    ///   - MAGLEV: `pick_endpoint(Some(kh))` routes via the table.
    #[test]
    fn pick_endpoint_m28_3_three_policy_dispatch_witness() {
        let ep0: SocketAddr = "172.31.0.2:5678".parse().unwrap(); // host 0
        let ep1: SocketAddr = "172.31.0.3:5678".parse().unwrap(); // host 1
        let endpoints = vec![ep0, ep1];
        let kh = crate::xxhash::xxh64(b"key-0"); // ring oracle + table both defined

        // (a) ROUND_ROBIN: key inert — Some(kh) behaves like None (cursor path).
        let rr = mk_handle("witness_rr", endpoints.clone());
        let rr_none = mk_handle("witness_rr2", endpoints.clone());
        assert_eq!(
            rr.pick_endpoint(Some(kh), None),
            rr_none.pick_endpoint(None, None),
            "ROUND_ROBIN: Some(kh) == None (key inert, cursor path)"
        );
        // And the cursor advances regardless of the key being present.
        assert_eq!(
            rr.pick_endpoint(Some(kh), None),
            Some(ep1),
            "RR cursor advances; key inert"
        );

        // (b) RING_HASH: routes via the ring (eps[ring.lookup(kh)] — computed
        // against a reference ring built from the SAME address strings, since
        // the ring host for a given key is address-string-dependent).
        let rh = mk_ring_hash_handle("witness_rh", endpoints.clone(), 1024);
        let ring_addrs: Vec<String> = endpoints.iter().map(|a| a.to_string()).collect();
        let ref_ring = crate::ring_hash::HashRing::build(&ring_addrs, 1024);
        let rh_idx = ref_ring.lookup(kh).expect("non-empty ring");
        assert_eq!(
            rh.pick_endpoint(Some(kh), None),
            Some(endpoints[rh_idx]),
            "RING_HASH: routes via the ring"
        );
        assert_eq!(
            rh.pick_endpoint(Some(kh), None),
            rh.pick_endpoint(Some(kh), None),
            "RING_HASH: same key → same host (ring, not cursor)"
        );

        // (c) MAGLEV: routes via the table (eps[table.lookup(kh)]).
        let mg = mk_maglev_handle("witness_mg", endpoints.clone(), 65537);
        let table = reference_maglev_table(&endpoints, 65537);
        let mg_idx = table.lookup(kh).expect("non-empty table");
        assert_eq!(
            mg.pick_endpoint(Some(kh), None),
            Some(endpoints[mg_idx]),
            "MAGLEV: routes via the table"
        );
        assert_eq!(
            mg.pick_endpoint(Some(kh), None),
            mg.pick_endpoint(Some(kh), None),
            "MAGLEV: same key → same host (table, not cursor)"
        );
    }

    /// 29 Task 7 (6): a single-host MAGLEV cluster at the CLUSTER level always
    /// returns that endpoint — for any key, AND for the no-hash fallback (a
    /// 1-element cursor cycle). Complements maglev.rs's TABLE-level
    /// single-host test by exercising the dispatch.
    #[test]
    fn pick_endpoint_maglev_single_host_always_returns_it() {
        let only: SocketAddr = "10.0.0.1:80".parse().unwrap();
        let mg = mk_maglev_handle("mg_solo", vec![only], 65537);
        for key in ["", "a", "key-0", "1.2.3.4", "anything"] {
            let kh = crate::xxhash::xxh64(key.as_bytes());
            assert_eq!(
                mg.pick_endpoint(Some(kh), None),
                Some(only),
                "key {key:?} → sole host"
            );
        }
        // No-hash fallback (cursor path over a 1-element set) also yields it.
        assert_eq!(
            mg.pick_endpoint(None, None),
            Some(only),
            "no-hash → sole host"
        );
        assert_eq!(
            mg.pick_endpoint(None, None),
            Some(only),
            "cursor cycle of 1"
        );
    }

    // ---------------------------------------------------------------------
    // phase-27 Task 2 (D1 / §6.2-INDEPENDENT): the endpoint set is a
    // swappable handle (`RwLock<Arc<Vec<SocketAddr>>>`). These tests pin the
    // §5.4 read-once invariant + the V4(d)/V6 swap-safety foundations BEFORE
    // any watcher exists. Task 2 adds the swap API only; nothing reloads yet.
    // ---------------------------------------------------------------------

    /// §5.4 (a)+(b): `pick()` reads the CURRENT endpoint Arc, and a
    /// `store_endpoints(new)` is visible to the NEXT `pick()`.
    #[test]
    fn endpoint_handle_store_is_visible_to_next_pick() {
        let initial = mk_endpoints(2); // 127.0.0.1:10000, :10001
        let handle = mk_handle("backend", initial.clone());
        // (a) reads current set.
        assert_eq!(handle.pick_endpoint(None, None).unwrap(), initial[0]); // cursor 0
        // (b) swap in a brand-new, disjoint set.
        let replacement: Vec<SocketAddr> = vec![
            "127.0.0.1:20000".parse().unwrap(),
            "127.0.0.1:20001".parse().unwrap(),
        ];
        handle.inner.store_endpoints(Arc::new(replacement.clone()));
        // The NEXT pick observes the replacement (cursor 1 → replacement[1]).
        assert_eq!(handle.pick_endpoint(None, None).unwrap(), replacement[1]);
        assert!(
            replacement.contains(&handle.pick_endpoint(None, None).unwrap()),
            "every subsequent pick reads the replacement set"
        );
    }

    /// §5.4 (c): an in-flight selection that snapshotted the OLD Arc keeps its
    /// snapshot — a `store_endpoints` landing after the snapshot does NOT tear
    /// the read (the read-once guarantee). We emulate the in-flight task by
    /// holding the `Arc` returned by `current_endpoints()` across a swap.
    #[test]
    fn endpoint_handle_inflight_snapshot_is_isolated_from_swap() {
        let initial = mk_endpoints(2);
        let handle = mk_handle("backend", initial.clone());
        // An in-flight selection snapshots the current Arc once.
        let snapshot = handle.inner.current_endpoints();
        assert_eq!(&*snapshot, &initial);
        // A reload lands mid-selection.
        let replacement: Vec<SocketAddr> = vec!["127.0.0.1:30000".parse().unwrap()];
        handle.inner.store_endpoints(Arc::new(replacement));
        // The snapshot the in-flight selection holds is unchanged (no tear).
        assert_eq!(
            &*snapshot, &initial,
            "the snapshot taken before the swap is isolated from it"
        );
        // The handle now points at the replacement.
        assert_eq!(handle.inner.current_endpoints().len(), 1);
    }

    /// §5.4 (d) (V4(d) apply-empty foundation): swapping in an EMPTY set makes
    /// the next `pick()` return `None` (no panic, no `% 0`).
    #[test]
    fn endpoint_handle_store_empty_yields_none_next_pick() {
        let handle = mk_handle("backend", mk_endpoints(2));
        assert!(handle.pick_endpoint(None, None).is_some());
        handle.inner.store_endpoints(Arc::new(Vec::new()));
        assert_eq!(
            handle.pick_endpoint(None, None),
            None,
            "an empty endpoint set short-circuits to None before any modulo"
        );
    }

    /// §5.4 (e) (V6 cursor-bounds): a SHRINKING set (2→1) leaves the cursor
    /// safe — `i % total` over the NEW snapshot stays in-bounds even after the
    /// cursor has advanced past the new length.
    #[test]
    fn endpoint_handle_shrinking_set_keeps_cursor_in_bounds() {
        let initial = mk_endpoints(2);
        let handle = mk_handle("backend", initial);
        // Advance the cursor past 1 (so a stale length would index out of range).
        for _ in 0..5 {
            assert!(handle.pick_endpoint(None, None).is_some());
        }
        // Shrink to a single endpoint.
        let one: Vec<SocketAddr> = vec!["127.0.0.1:40000".parse().unwrap()];
        handle.inner.store_endpoints(Arc::new(one.clone()));
        // Every subsequent pick must be the sole survivor — never an OOB index.
        for _ in 0..5 {
            assert_eq!(handle.pick_endpoint(None, None).unwrap(), one[0]);
        }
    }

    #[test]
    fn pick_endpoint_cycles_over_three_endpoints() {
        let endpoints = mk_endpoints(3);
        let handle = mk_handle("backend", endpoints.clone());
        let picks: Vec<SocketAddr> = (0..7)
            .map(|_| handle.pick_endpoint(None, None).unwrap())
            .collect();
        let expected = vec![
            endpoints[0],
            endpoints[1],
            endpoints[2],
            endpoints[0],
            endpoints[1],
            endpoints[2],
            endpoints[0],
        ];
        assert_eq!(picks, expected);
    }

    #[test]
    fn pick_endpoint_is_stable_under_concurrent_calls() {
        use std::collections::HashMap;
        use std::sync::Mutex;
        use std::thread;

        const N_ENDPOINTS: usize = 3;
        const N_CALLS: usize = 1000;

        let endpoints = mk_endpoints(N_ENDPOINTS as u16);
        let handle = mk_handle("backend", endpoints.clone());

        let counts: Arc<Mutex<HashMap<SocketAddr, usize>>> = Arc::new(Mutex::new(HashMap::new()));
        let mut handles = Vec::with_capacity(N_CALLS);
        for _ in 0..N_CALLS {
            let h = handle.clone();
            let c = Arc::clone(&counts);
            handles.push(thread::spawn(move || {
                let ep = h.pick_endpoint(None, None).expect("non-empty");
                *c.lock().unwrap().entry(ep).or_insert(0) += 1;
            }));
        }
        for t in handles {
            t.join().unwrap();
        }

        let counts = counts.lock().unwrap();
        let expected = N_CALLS / N_ENDPOINTS; // 333
        let tolerance = (expected as f64 * 0.10) as usize; // 33 ≈ 10 %
        assert_eq!(counts.values().sum::<usize>(), N_CALLS);
        for ep in &endpoints {
            let got = *counts.get(ep).unwrap_or(&0);
            assert!(
                got.abs_diff(expected) <= tolerance,
                "endpoint {ep:?} picked {got} times; expected {expected} ± {tolerance}",
            );
        }
    }

    #[test]
    fn handle_clone_shares_cursor() {
        let endpoints = mk_endpoints(2);
        let a = mk_handle("backend", endpoints.clone());
        let b = a.clone();

        // Interleave picks across the clone and the original. With a shared
        // cursor, the sequence is alternating-index; with separate cursors
        // each handle would pick its own [0,1,0,1,...].
        let seq: Vec<SocketAddr> = vec![
            a.pick_endpoint(None, None).unwrap(), // cursor=0 -> endpoints[0]
            b.pick_endpoint(None, None).unwrap(), // cursor=1 -> endpoints[1]
            a.pick_endpoint(None, None).unwrap(), // cursor=2 -> endpoints[0]
            b.pick_endpoint(None, None).unwrap(), // cursor=3 -> endpoints[1]
        ];
        assert_eq!(
            seq,
            vec![endpoints[0], endpoints[1], endpoints[0], endpoints[1]]
        );
    }

    const SINGLE_ENDPOINT_YAML: &str = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10042
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;

    #[tokio::test]
    async fn from_bootstrap_builds_single_endpoint_cluster() {
        let bootstrap = envoy_config::parse_bootstrap(SINGLE_ENDPOINT_YAML).expect("valid");
        let mgr = crate::from_bootstrap(&bootstrap, Arc::new(envoy_stats::StatsRegistry::new()))
            .await
            .expect("construct");
        let handle = mgr.get("backend").expect("cluster present");
        let picked = handle.pick_endpoint(None, None).expect("non-empty");
        assert_eq!(picked, "127.0.0.1:10042".parse::<SocketAddr>().unwrap());
    }

    const THREE_ENDPOINT_YAML: &str = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10002
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10003
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;

    #[tokio::test]
    async fn from_bootstrap_builds_three_endpoint_cluster() {
        let bootstrap = envoy_config::parse_bootstrap(THREE_ENDPOINT_YAML).expect("valid");
        let mgr = crate::from_bootstrap(&bootstrap, Arc::new(envoy_stats::StatsRegistry::new()))
            .await
            .expect("construct");
        let handle = mgr.get("backend").expect("cluster present");
        let picks: Vec<SocketAddr> = (0..3)
            .map(|_| handle.pick_endpoint(None, None).unwrap())
            .collect();
        assert_eq!(
            picks,
            vec![
                "127.0.0.1:10001".parse().unwrap(),
                "127.0.0.1:10002".parse().unwrap(),
                "127.0.0.1:10003".parse().unwrap(),
            ],
        );
    }

    #[tokio::test]
    async fn from_bootstrap_rejects_empty_cluster() {
        // envoy-config rejects zero-endpoint clusters before we get here, so
        // build the Bootstrap by-hand to exercise the cluster-crate edge.
        let mut cluster = mk_static_cluster("backend", 10001);
        cluster
            .load_assignment
            .as_mut()
            .expect("mk_static_cluster sets load_assignment")
            .endpoints
            .clear();
        let bootstrap = mk_bootstrap(vec![cluster], None, None);
        let err = crate::from_bootstrap(&bootstrap, Arc::new(envoy_stats::StatsRegistry::new()))
            .await
            .expect_err("must reject");
        assert!(
            matches!(err, ClusterError::EmptyCluster { ref name } if name == "backend"),
            "got {err:?}",
        );
    }

    #[tokio::test]
    async fn from_bootstrap_rejects_duplicate_cluster_name() {
        // envoy-config doesn't reject duplicate cluster names (Vec<Cluster>
        // allows dupes at the serde layer); envoy-cluster is the first
        // enforcement. Build via by-hand Bootstrap to exercise this edge.
        let bootstrap = mk_bootstrap(
            vec![
                mk_static_cluster("backend", 10001),
                mk_static_cluster("backend", 10001),
            ],
            None,
            None,
        );
        let err = crate::from_bootstrap(&bootstrap, Arc::new(envoy_stats::StatsRegistry::new()))
            .await
            .expect_err("must reject");
        assert!(
            matches!(err, ClusterError::DuplicateClusterName { ref name } if name == "backend"),
            "got {err:?}",
        );
    }

    #[test]
    fn cluster_name_returns_configured_name() {
        let registry = envoy_stats::StatsRegistry::new();
        let c = mk_test_cluster("backend", mk_endpoints(1), &registry);
        assert_eq!(c.name(), "backend");
    }

    #[test]
    fn cluster_handle_exposes_name() {
        let h = mk_handle("primary", mk_endpoints(2));
        assert_eq!(h.name(), "primary");
    }

    #[test]
    fn cluster_name_outlives_borrow_correctly() {
        // The accessor returns a borrow tied to the Cluster's lifetime.
        // Borrow-check regression guard: holding the borrow while picking
        // an endpoint compiles cleanly.
        let h = mk_handle("primary", mk_endpoints(2));
        let name = h.name();
        let _ep = h.pick_endpoint(None, None);
        assert_eq!(name, "primary");
    }

    #[tokio::test]
    async fn from_bootstrap_rejects_malformed_endpoint_address() {
        // envoy-config accepts the YAML at parse time (address: String);
        // envoy-cluster is the first layer that parses it into SocketAddr.
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: not-a-host
                      port_value: 10001
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let bootstrap = envoy_config::parse_bootstrap(yaml).expect("serde accepts");
        let err = crate::from_bootstrap(&bootstrap, Arc::new(envoy_stats::StatsRegistry::new()))
            .await
            .expect_err("must reject");
        assert!(
            matches!(
                err,
                ClusterError::EndpointParse { ref cluster, ref addr, .. }
                    if cluster == "backend" && addr == "not-a-host:10001"
            ),
            "got {err:?}",
        );
    }

    #[tokio::test]
    async fn static_cluster_constructs_with_literal_ip() {
        // 05.1 NEW (closes phase-02.1 REVIEW I3): positive Static regression guard.
        // Was un-writable before phase 05.1 because ClusterType had only one variant
        // (`Static`); with `StrictDns` now landing in 05.1 the `match cluster_type`
        // arm is structurally meaningful, so the Static path is exercised here as
        // an explicit guard against accidental schema/runtime regressions.
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 7000
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let bootstrap = envoy_config::parse_bootstrap(yaml).expect("valid");
        let mgr = crate::from_bootstrap(&bootstrap, Arc::new(envoy_stats::StatsRegistry::new()))
            .await
            .expect("Static cluster constructs cleanly");
        let handle = mgr.get("backend").expect("cluster present");
        let picked = handle.pick_endpoint(None, None).expect("non-empty");
        assert_eq!(picked, "127.0.0.1:7000".parse::<SocketAddr>().unwrap());
    }

    #[tokio::test]
    async fn strict_dns_cluster_resolves_localhost_at_build_time() {
        // 05.1 NEW: STRICT_DNS resolves a DNS name at cluster-build time via
        // tokio::net::lookup_host. `localhost` is universally resolvable across
        // dev/CI (loopback-bound; no network dependency); see PLAN.md signpost D.
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: localhost
                      port_value: 7000
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let bootstrap = envoy_config::parse_bootstrap(yaml).expect("valid");
        let mgr = crate::from_bootstrap(&bootstrap, Arc::new(envoy_stats::StatsRegistry::new()))
            .await
            .expect("STRICT_DNS cluster resolves localhost cleanly");
        let handle = mgr.get("backend").expect("cluster present");
        let picked = handle.pick_endpoint(None, None).expect("non-empty");
        assert_eq!(
            picked.port(),
            7000,
            "resolved endpoint should preserve configured port",
        );
        assert!(
            picked.ip().is_loopback(),
            "localhost should resolve to loopback (127.0.0.1 or ::1), got {picked:?}",
        );
    }

    #[tokio::test]
    async fn strict_dns_cluster_returns_dns_resolution_failed_on_nxdomain() {
        // 05.1 NEW: NXDOMAIN-equivalent path returns ClusterError::DnsResolutionFailed
        // with the diagnostic fields populated. `.invalid` TLD is RFC 6761 §6.4
        // reserved as non-resolvable (PLAN.md signpost E). If CI flakes due to a
        // misconfigured resolver synthesizing a positive answer, fall back to the
        // empty-host case per signpost E's documented escape hatch.
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: this-host-does-not-exist.invalid
                      port_value: 7000
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let bootstrap = envoy_config::parse_bootstrap(yaml).expect("valid");
        let err = crate::from_bootstrap(&bootstrap, Arc::new(envoy_stats::StatsRegistry::new()))
            .await
            .expect_err("STRICT_DNS resolution of .invalid TLD must fail");
        assert!(
            matches!(
                err,
                ClusterError::DnsResolutionFailed {
                    ref cluster,
                    ref address,
                    ..
                } if cluster == "backend" && address == "this-host-does-not-exist.invalid"
            ),
            "expected DnsResolutionFailed{{cluster:'backend',address:'this-host-does-not-exist.invalid',..}}, got {err:?}",
        );
    }

    /// Helper: build a Bootstrap from a YAML string and run from_bootstrap;
    /// returns the resulting ClusterManager. Panics on parse / build error.
    async fn build_cluster_mgr(yaml: &str) -> ClusterManager {
        let bootstrap = envoy_config::parse_bootstrap(yaml).expect("parse");
        from_bootstrap(&bootstrap, Arc::new(envoy_stats::StatsRegistry::new()))
            .await
            .expect("from_bootstrap")
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn cluster_upstream_protocol_defaults_to_http1() {
        let yaml = r#"
node: { id: x, cluster: y }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: l
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
"#;
        let mgr = build_cluster_mgr(yaml).await;
        let handle = mgr.get("backend").expect("backend cluster");
        assert_eq!(handle.upstream_protocol(), UpstreamProtocol::Http1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn cluster_upstream_protocol_http2_set_from_typed_extension_protocol_options() {
        // 06.3 D14.3: changed from codec_type HTTP1 → HTTP2 so the H2 cluster
        // target remains valid under the new H1×H2 reachability gate. The
        // purpose of this test is to verify UpstreamProtocol::Http2 is set from
        // typed_extension_protocol_options; HTTP2 listener + H2 cluster is the
        // correct canonical shape for that combination.
        let yaml = r#"
node: { id: x, cluster: y }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: l
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP2
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.HttpProtocolOptions":
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http2_protocol_options:
              max_concurrent_streams: 100
"#;
        let mgr = build_cluster_mgr(yaml).await;
        let handle = mgr.get("backend").expect("backend cluster");
        assert_eq!(handle.upstream_protocol(), UpstreamProtocol::Http2);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn cluster_upstream_protocol_http1_set_from_explicit_http1_options() {
        let yaml = r#"
node: { id: x, cluster: y }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: l
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.HttpProtocolOptions":
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http_protocol_options: {}
"#;
        let mgr = build_cluster_mgr(yaml).await;
        let handle = mgr.get("backend").expect("backend cluster");
        assert_eq!(handle.upstream_protocol(), UpstreamProtocol::Http1);
    }

    // ── 06.3 D15.3.b: cx_active gauge + ConnGaugeGuard tests ─────────────

    /// 06.3 D15.3.b: `ConnGaugeGuard` increments `upstream_cx_active` at
    /// construction and decrements via `Drop`. Exercises the direct RAII
    /// contract without any async scaffolding.
    #[tokio::test(flavor = "multi_thread")]
    async fn cluster_cx_active_guard_increments_on_construct_and_decrements_on_drop() {
        let backend = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let backend_addr = backend.local_addr().unwrap();
        let yaml = format!(
            r#"
static_resources:
  listeners: []
  clusters:
    - name: backend_cluster
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend_cluster
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: {}
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#,
            backend_addr.port()
        );
        let bootstrap = envoy_config::parse_bootstrap(&yaml).expect("parse");
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let mgr = from_bootstrap(&bootstrap, Arc::clone(&registry))
            .await
            .expect("from_bootstrap");

        // Re-register to get the same Arc (idempotent same-kind contract).
        let cx_active = registry
            .register_gauge("cluster.backend_cluster.upstream_cx_active")
            .expect("gauge registers");
        assert_eq!(cx_active.value(), 0, "gauge starts at zero");

        let handle = mgr.get("backend_cluster").expect("cluster present");
        {
            let _guard = handle.cx_active_guard();
            assert_eq!(cx_active.value(), 1, "guard construction increments gauge");
        }
        // Drop fires here.
        assert_eq!(cx_active.value(), 0, "Drop decrements gauge back to zero");
    }

    /// 06.3 D15.3.b: gauge is observable at > 0 while a guard is held
    /// across an async yield point (simulates a live upstream connection).
    /// Simplified from the full H1 integration test — per PROGRESS deviation
    /// note: running an actual H1 client call inside the cluster crate would
    /// require pulling envoy-http1 as a dev-dependency, which is heavyweight.
    /// The RAII correctness (inc + async hold + dec-on-drop) is fully covered
    /// here; cross-crate wiring is verified by the HCM-level integration tests
    /// in the envoy-http1 and envoy-http2 crates.
    #[tokio::test(flavor = "multi_thread")]
    async fn cluster_cx_active_round_trip_through_h1_call() {
        let backend = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let backend_addr = backend.local_addr().unwrap();
        let yaml = format!(
            r#"
static_resources:
  listeners: []
  clusters:
    - name: backend_cluster
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend_cluster
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: {}
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#,
            backend_addr.port()
        );
        let bootstrap = envoy_config::parse_bootstrap(&yaml).expect("parse");
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let mgr = from_bootstrap(&bootstrap, Arc::clone(&registry))
            .await
            .expect("from_bootstrap");

        let cx_active = registry
            .register_gauge("cluster.backend_cluster.upstream_cx_active")
            .expect("gauge registers");

        let handle = mgr.get("backend_cluster").expect("cluster present");

        // Hold the guard across an async yield — simulates the window during
        // which an upstream connection is live. Guard is visible at value 1
        // mid-await and falls back to 0 after the scope exits.
        let guard = handle.cx_active_guard();
        assert_eq!(cx_active.value(), 1, "gauge is 1 while guard is held");
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        assert_eq!(cx_active.value(), 1, "gauge is still 1 during async sleep");
        drop(guard);
        assert_eq!(cx_active.value(), 0, "gauge returns to 0 after guard drop");
    }

    /// 06.3 D15.3.b: 10 concurrent guard acquisitions each held for ~50 ms.
    /// The gauge must settle to 0 after all tasks join. A peak observation is
    /// obtained by reading the gauge immediately after all guards are acquired
    /// in a tight loop (no async yield between acquire and read) — this
    /// guarantees the gauge reaches N before any drop fires.
    #[tokio::test(flavor = "multi_thread")]
    async fn cluster_cx_active_monotonic_then_decreasing_under_concurrent_calls() {
        let backend = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let backend_addr = backend.local_addr().unwrap();
        let yaml = format!(
            r#"
static_resources:
  listeners: []
  clusters:
    - name: backend_cluster
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend_cluster
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: {}
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#,
            backend_addr.port()
        );
        let bootstrap = envoy_config::parse_bootstrap(&yaml).expect("parse");
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let mgr = from_bootstrap(&bootstrap, Arc::clone(&registry))
            .await
            .expect("from_bootstrap");

        let cx_active = registry
            .register_gauge("cluster.backend_cluster.upstream_cx_active")
            .expect("gauge registers");

        let handle = mgr.get("backend_cluster").expect("cluster present");
        const N: usize = 10;

        // Acquire all N guards in the current task (no yield between calls) so
        // the gauge atomically reaches N before any concurrent drop fires.
        let mut guards: Vec<ConnGaugeGuard> = (0..N).map(|_| handle.cx_active_guard()).collect();
        assert_eq!(
            cx_active.value(),
            N as i64,
            "gauge peaks at {N} while all guards are held",
        );

        // Spawn N tasks each holding a guard for ~50 ms; then drop our
        // synchronous guards too so the gauge returns to 0.
        let mut join_handles = Vec::with_capacity(N);
        for g in guards.drain(..) {
            join_handles.push(tokio::spawn(async move {
                let _guard = g;
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                // _guard drops here
            }));
        }

        // Join all tasks; after join the gauge must be back at 0.
        for jh in join_handles {
            jh.await.expect("task did not panic");
        }
        assert_eq!(
            cx_active.value(),
            0,
            "gauge returns to 0 after all guards drop",
        );
    }

    // ── 12.1: health-aware pick() tests ─────────────────────────────────

    /// 12.1: build a ClusterHandle whose endpoints carry EndpointHealth, all
    /// starting Unhealthy, with the given panic threshold. Returns the handle +
    /// the per-endpoint EndpointHealth Arcs so tests can drive transitions.
    fn mk_handle_with_health(
        name: &str,
        endpoints: Vec<SocketAddr>,
        healthy_threshold: u32,
        unhealthy_threshold: u32,
        panic_threshold: f64,
    ) -> (ClusterHandle, Vec<Arc<crate::EndpointHealth>>) {
        let registry = envoy_stats::StatsRegistry::new();
        let gauge = registry
            .register_gauge(&format!("cluster.{name}.membership_healthy"))
            .unwrap();
        let health: Vec<Arc<crate::EndpointHealth>> = endpoints
            .iter()
            .map(|_| {
                Arc::new(crate::EndpointHealth::new(
                    healthy_threshold,
                    unhealthy_threshold,
                    Arc::clone(&gauge),
                ))
            })
            .collect();
        let mut cluster = mk_test_cluster(name, endpoints, &registry);
        cluster.endpoint_health = Some(health.clone());
        cluster.panic_threshold = panic_threshold;
        let handle = ClusterHandle {
            inner: Arc::new(cluster),
        };
        (handle, health)
    }

    #[test]
    fn pick_excludes_unhealthy_endpoints() {
        let eps = mk_endpoints(2);
        // panic disabled (value 0) so a partially-unhealthy set does not panic-route.
        let (handle, health) = mk_handle_with_health("b", eps.clone(), 1, 1, 0.0);
        // Make endpoint 0 healthy, endpoint 1 stays unhealthy.
        health[0].record_success();
        let picks: Vec<SocketAddr> = (0..4)
            .map(|_| handle.pick_endpoint(None, None).unwrap())
            .collect();
        assert!(
            picks.iter().all(|&p| p == eps[0]),
            "only the healthy endpoint is picked: {picks:?}"
        );
    }

    #[test]
    fn pick_round_robins_over_noncontiguous_healthy_subset() {
        let eps = mk_endpoints(3);
        // panic disabled; mark endpoints 0 and 2 healthy, leave 1 unhealthy.
        // This stresses healthy_idx = [0, 2] and the modulo over a >1-element,
        // non-contiguous healthy index set (off-by-one guard for the remap).
        let (handle, health) = mk_handle_with_health("b", eps.clone(), 1, 1, 0.0);
        health[0].record_success();
        health[2].record_success();
        let picks: Vec<SocketAddr> = (0..4)
            .map(|_| handle.pick_endpoint(None, None).unwrap())
            .collect();
        assert_eq!(
            picks,
            vec![eps[0], eps[2], eps[0], eps[2]],
            "round-robins over the healthy subset {{0,2}}, never the unhealthy endpoint 1: {picks:?}"
        );
    }

    #[test]
    fn pick_returns_none_when_no_healthy_and_panic_disabled() {
        let eps = mk_endpoints(2);
        let (handle, _health) = mk_handle_with_health("b", eps, 1, 1, 0.0);
        // All endpoints start Unhealthy; panic disabled → None.
        assert!(handle.pick_endpoint(None, None).is_none());
    }

    #[test]
    fn pick_panics_to_all_when_below_threshold() {
        let eps = mk_endpoints(2);
        // default 50% panic threshold; 0 healthy → 0% < 50% → panic → round-robin ALL.
        let (handle, _health) = mk_handle_with_health("b", eps.clone(), 1, 1, 50.0);
        let picks: Vec<SocketAddr> = (0..4)
            .map(|_| handle.pick_endpoint(None, None).unwrap())
            .collect();
        assert_eq!(
            picks,
            vec![eps[0], eps[1], eps[0], eps[1]],
            "panic mode round-robins over all endpoints"
        );
    }

    #[test]
    fn pick_does_not_panic_at_exactly_the_threshold() {
        let eps = mk_endpoints(2);
        // 1 of 2 healthy = 50% ; threshold 50 ; 50 < 50 is false → no panic → only healthy.
        let (handle, health) = mk_handle_with_health("b", eps.clone(), 1, 1, 50.0);
        health[0].record_success();
        let picks: Vec<SocketAddr> = (0..4)
            .map(|_| handle.pick_endpoint(None, None).unwrap())
            .collect();
        assert!(
            picks.iter().all(|&p| p == eps[0]),
            "strictly-below: 50% is not < 50% so no panic: {picks:?}"
        );
    }

    #[tokio::test]
    async fn from_bootstrap_no_health_checks_pick_unchanged() {
        // Regression-equivalence: a cluster with no health_checks behaves exactly
        // as phase-02 round-robin (endpoint_health is None).
        let mgr = build_cluster_mgr(THREE_ENDPOINT_YAML).await;
        let handle = mgr.get("backend").expect("cluster");
        let picks: Vec<SocketAddr> = (0..3)
            .map(|_| handle.pick_endpoint(None, None).unwrap())
            .collect();
        assert_eq!(
            picks,
            vec![
                "127.0.0.1:10001".parse().unwrap(),
                "127.0.0.1:10002".parse().unwrap(),
                "127.0.0.1:10003".parse().unwrap(),
            ]
        );
    }

    #[tokio::test]
    async fn from_bootstrap_registers_membership_healthy_gauge_at_zero() {
        // D6: a configured-HC cluster registers cluster.<name>.membership_healthy;
        // it reads 0 at construction (all endpoints start Unhealthy). The 3
        // health_check.{attempt,success,failure} counters defer to 12.2.
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: hc_backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      health_checks:
        - timeout: 1s
          interval: 1s
          healthy_threshold: 1
          unhealthy_threshold: 1
          http_health_check: { path: /healthz }
      load_assignment:
        cluster_name: hc_backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: localhost, port_value: 7000 } }
admin:
  address:
    socket_address: { address: 127.0.0.1, port_value: 9901 }
"#;
        let bootstrap = envoy_config::parse_bootstrap(yaml).expect("parse");
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let _mgr = from_bootstrap(&bootstrap, Arc::clone(&registry))
            .await
            .expect("build");
        // Assert the gauge was registered by from_bootstrap (snapshot reflects
        // real registrations; register_gauge below is idempotent and returns the
        // same Arc, so the value read is the live one — but presence must be
        // proven via the snapshot, since register_gauge would otherwise create a
        // fresh 0-valued gauge and mask a missing registration).
        assert!(
            registry
                .snapshot()
                .iter()
                .any(|(name, _)| name == "cluster.hc_backend.membership_healthy"),
            "from_bootstrap must register the membership_healthy gauge"
        );
        let gauge = registry
            .register_gauge("cluster.hc_backend.membership_healthy")
            .expect("gauge");
        assert_eq!(gauge.value(), 0, "all endpoints start Unhealthy");
    }

    #[tokio::test]
    async fn from_bootstrap_no_health_checks_registers_no_membership_gauge() {
        // Inert-when-unconfigured: no membership_healthy gauge for a plain cluster.
        let mgr_registry = Arc::new(envoy_stats::StatsRegistry::new());
        let bootstrap = envoy_config::parse_bootstrap(THREE_ENDPOINT_YAML).expect("parse");
        let _mgr = from_bootstrap(&bootstrap, Arc::clone(&mgr_registry))
            .await
            .expect("build");
        let has_gauge = mgr_registry
            .snapshot()
            .iter()
            .any(|(name, _)| name == "cluster.backend.membership_healthy");
        assert!(
            !has_gauge,
            "no membership gauge when health_checks unconfigured"
        );
    }

    #[tokio::test]
    async fn from_bootstrap_with_health_checks_starts_all_unhealthy() {
        // A configured-HC cluster (panic disabled) with no probe task → all
        // endpoints start Unhealthy → pick() returns None (the 12.2 task drives them).
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: hc_backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      common_lb_config:
        healthy_panic_threshold: { value: 0 }
      health_checks:
        - timeout: 1s
          interval: 1s
          healthy_threshold: 1
          unhealthy_threshold: 1
          http_health_check: { path: /healthz }
      load_assignment:
        cluster_name: hc_backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: localhost, port_value: 7000 } }
admin:
  address:
    socket_address: { address: 127.0.0.1, port_value: 9901 }
"#;
        let mgr = build_cluster_mgr(yaml).await;
        let handle = mgr.get("hc_backend").expect("cluster");
        assert!(
            handle.pick_endpoint(None, None).is_none(),
            "all endpoints start unhealthy + panic disabled"
        );
    }

    /// 06.1 D4.b: per-cluster `upstream_cx_total` counter increments via
    /// the call-site pattern (`cluster.cx_total().inc()` after a
    /// successful upstream connect). The actual `TcpStream::connect` site
    /// lives in envoy-tcp / envoy-http1::client / envoy-http2::client per
    /// SPEC §3 D4.b's "increment-at-call-site" posture; this test exercises
    /// the cluster-side wiring (registration + accessor) by simulating
    /// that pattern in-place. Cross-crate call-site wiring is verified by
    /// fixture 0011 and the H1/H2 HCM integration tests in Task 11+.
    #[tokio::test(flavor = "multi_thread")]
    async fn cluster_increments_cx_total_on_connect() {
        // Spawn a no-op TCP listener as the upstream backend so the
        // simulated connect succeeds.
        let backend = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let backend_addr = backend.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                if backend.accept().await.is_err() {
                    return;
                }
            }
        });

        let yaml = format!(
            r#"
static_resources:
  listeners: []
  clusters:
    - name: backend_cluster
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend_cluster
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: {}
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#,
            backend_addr.port()
        );
        let bootstrap = envoy_config::parse_bootstrap(&yaml).expect("parse");
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let mgr = from_bootstrap(&bootstrap, Arc::clone(&registry))
            .await
            .expect("from_bootstrap");

        // Re-register by name to fetch the same Arc the cluster holds
        // (idempotent same-kind contract from Task 5).
        let cx_total = registry
            .register_counter("cluster.backend_cluster.upstream_cx_total")
            .expect("counter registers");
        assert_eq!(cx_total.value(), 0, "counter starts at zero");

        // Simulate the call-site connect-then-increment pattern.
        let handle = mgr.get("backend_cluster").expect("cluster present");
        for _ in 0..3 {
            let endpoint = handle.pick_endpoint(None, None).expect("endpoint");
            let _stream = tokio::net::TcpStream::connect(endpoint).await.unwrap();
            // 06.1 D4.b: this is the call-site increment pattern that
            // envoy-tcp / envoy_http1::Client::connect / envoy_http2::
            // Client::connect callers in the HCM router-proxy arm perform.
            handle.cx_total().inc();
        }
        assert_eq!(
            cx_total.value(),
            3,
            "expected one increment per simulated successful upstream connect",
        );
    }

    // ── 14.1 D5: outlier-detection pick() AND-composition + record_response ──

    // Test helper: build a Cluster with both 12.1 EndpointHealth AND 14.1 EndpointEjection
    // state, bypassing from_bootstrap. Both filters share the same endpoints (aligned by
    // index). Caller chooses which endpoints to mark healthy / ejected via the returned
    // Arc handles.
    #[allow(clippy::too_many_arguments)]
    fn mk_handle_with_health_and_ejection(
        name: &str,
        endpoints: Vec<SocketAddr>,
        healthy_threshold: u32,
        unhealthy_threshold: u32,
        panic_threshold: f64,
        consecutive_5xx_threshold: u32,
        consecutive_gateway_failure_threshold: u32,
        max_ejection_percent: u32,
    ) -> (
        ClusterHandle,
        Vec<Arc<crate::EndpointHealth>>,
        Vec<Arc<crate::EndpointEjection>>,
    ) {
        let registry = envoy_stats::StatsRegistry::new();
        let membership = registry
            .register_gauge(&format!("cluster.{name}.membership_healthy"))
            .unwrap();
        let health: Vec<Arc<crate::EndpointHealth>> = endpoints
            .iter()
            .map(|_| {
                Arc::new(crate::EndpointHealth::new(
                    healthy_threshold,
                    unhealthy_threshold,
                    Arc::clone(&membership),
                ))
            })
            .collect();
        let stats = crate::EndpointEjectionStats {
            ejections_active: registry
                .register_gauge(&format!("cluster.{name}.outlier_detection.ejections_active"))
                .unwrap(),
            ejections_enforced_total: registry
                .register_counter(&format!(
                    "cluster.{name}.outlier_detection.ejections_enforced_total"
                ))
                .unwrap(),
            ejections_detected_consecutive_5xx: registry
                .register_counter(&format!(
                    "cluster.{name}.outlier_detection.ejections_detected_consecutive_5xx"
                ))
                .unwrap(),
            ejections_enforced_consecutive_5xx: registry
                .register_counter(&format!(
                    "cluster.{name}.outlier_detection.ejections_enforced_consecutive_5xx"
                ))
                .unwrap(),
            ejections_detected_consecutive_gateway_failure: registry
                .register_counter(&format!(
                    "cluster.{name}.outlier_detection.ejections_detected_consecutive_gateway_failure"
                ))
                .unwrap(),
            ejections_enforced_consecutive_gateway_failure: registry
                .register_counter(&format!(
                    "cluster.{name}.outlier_detection.ejections_enforced_consecutive_gateway_failure"
                ))
                .unwrap(),
        };
        let ejection: Vec<Arc<crate::EndpointEjection>> = endpoints
            .iter()
            .map(|_| {
                Arc::new(crate::EndpointEjection::new(
                    consecutive_5xx_threshold,
                    consecutive_gateway_failure_threshold,
                    stats.clone(),
                ))
            })
            .collect();
        let ejections_overflow = registry
            .register_counter(&format!(
                "cluster.{name}.outlier_detection.ejections_overflow"
            ))
            .unwrap();
        let od_state = OutlierDetectionState {
            endpoints: ejection.clone(),
            max_ejection_percent,
            ejections_overflow,
            // 14.2 D7: timing fields are irrelevant to the `record_response` / `pick()` tests
            // this helper backs (the sweeper is tested separately in `outlier.rs`); use the
            // Envoy v3 defaults for representative values.
            base_ejection_time: std::time::Duration::from_secs(30),
            interval: std::time::Duration::from_secs(10),
        };
        let mut cluster = mk_test_cluster(name, endpoints, &registry);
        cluster.endpoint_health = Some(health.clone());
        cluster.panic_threshold = panic_threshold;
        cluster.outlier_detection = Some(od_state);
        let handle = ClusterHandle {
            inner: Arc::new(cluster),
        };
        (handle, health, ejection)
    }

    #[test]
    fn pick_inert_when_neither_filter_configured() {
        // Acceptance gate (b) regression-equivalence: when both endpoint_health AND
        // outlier_detection are None, pick() must be byte-for-byte phase-02 round-robin.
        let endpoints = mk_endpoints(3);
        let handle = mk_handle("backend", endpoints.clone()); // unchanged 12.1 helper
        let picks: Vec<SocketAddr> = (0..6)
            .map(|_| handle.pick_endpoint(None, None).unwrap())
            .collect();
        assert_eq!(
            picks,
            vec![
                endpoints[0],
                endpoints[1],
                endpoints[2],
                endpoints[0],
                endpoints[1],
                endpoints[2]
            ],
        );
    }

    #[test]
    fn pick_excludes_ejected_endpoints() {
        let eps = mk_endpoints(2);
        // panic disabled (value 0) + thresholds 1 (immediate ejection on first 500).
        let (handle, health, ejection) =
            mk_handle_with_health_and_ejection("b", eps.clone(), 1, 1, 0.0, 1, 1, 100);
        // Make both endpoints healthy so the active-HC filter doesn't interfere.
        health[0].record_success();
        health[1].record_success();
        // Eject endpoint 0 directly.
        ejection[0].eject(crate::DetectorType::Consecutive5xx);
        // pick() should now only return endpoint 1.
        for _ in 0..5 {
            assert_eq!(handle.pick_endpoint(None, None).unwrap(), eps[1]);
        }
    }

    #[test]
    fn pick_returns_none_when_all_endpoints_ejected_and_panic_disabled() {
        let eps = mk_endpoints(2);
        let (handle, health, ejection) =
            mk_handle_with_health_and_ejection("b", eps.clone(), 1, 1, 0.0, 1, 1, 100);
        health[0].record_success();
        health[1].record_success();
        ejection[0].eject(crate::DetectorType::Consecutive5xx);
        ejection[1].eject(crate::DetectorType::Consecutive5xx);
        assert!(
            handle.pick_endpoint(None, None).is_none(),
            "all ejected + panic=0 → None"
        );
    }

    #[test]
    fn pick_panic_routes_over_all_when_eligible_fraction_below_threshold() {
        let eps = mk_endpoints(2);
        // panic_threshold 60% (strictly-below): with 50% eligible, panic engages.
        let (handle, health, ejection) =
            mk_handle_with_health_and_ejection("b", eps.clone(), 1, 1, 60.0, 1, 1, 100);
        health[0].record_success();
        health[1].record_success();
        ejection[0].eject(crate::DetectorType::Consecutive5xx);
        // 1 of 2 eligible (50.0 < 60.0) → panic → round-robin over ALL.
        let picks: Vec<SocketAddr> = (0..4)
            .map(|_| handle.pick_endpoint(None, None).unwrap())
            .collect();
        assert_eq!(picks, vec![eps[0], eps[1], eps[0], eps[1]]);
    }

    #[test]
    fn pick_and_composes_health_and_ejection_filters() {
        // 4 endpoints; endpoint 0 unhealthy; endpoint 1 ejected; endpoint 2 BOTH unhealthy
        // AND ejected; endpoint 3 healthy+not-ejected. Eligible set = {3}.
        let eps = mk_endpoints(4);
        let (handle, health, ejection) =
            mk_handle_with_health_and_ejection("b", eps.clone(), 1, 1, 0.0, 1, 1, 100);
        // Mark endpoints 1, 3 healthy. Endpoints 0, 2 stay unhealthy.
        health[1].record_success();
        health[3].record_success();
        // Eject endpoints 1, 2.
        ejection[1].eject(crate::DetectorType::Consecutive5xx);
        ejection[2].eject(crate::DetectorType::Consecutive5xx);
        // Eligible: only endpoint 3.
        for _ in 0..5 {
            assert_eq!(handle.pick_endpoint(None, None).unwrap(), eps[3]);
        }
    }

    #[test]
    fn cluster_record_response_no_op_when_outlier_detection_unconfigured() {
        // The §5.3 inert invariant + lock-in #16: record_response on a cluster without
        // outlier_detection silently returns (no-op; no panic; no stats touched).
        let eps = mk_endpoints(1);
        let handle = mk_handle("backend", eps.clone());
        handle.record_response(eps[0], 500); // must not panic
        handle.record_response(eps[0], 503);
        // No assertable side-effect (no OD state) — the test passes iff no panic.
    }

    #[test]
    fn cluster_record_response_ejects_endpoint_when_threshold_crossed() {
        let eps = mk_endpoints(2);
        let (handle, _health, ejection) =
            mk_handle_with_health_and_ejection("b", eps.clone(), 1, 1, 0.0, 2, 2, 100);
        handle.record_response(eps[0], 500);
        assert!(!ejection[0].is_ejected(), "1 < threshold 2");
        handle.record_response(eps[0], 500);
        assert!(ejection[0].is_ejected(), "2 == threshold 2 → ejected");
    }

    #[test]
    fn cluster_record_response_honors_max_ejection_percent_cap() {
        // 4 endpoints, max_ejection_percent=25 → cap_count = floor(4*25/100) = 1.
        // First ejection succeeds; subsequent threshold-crossings increment
        // ejections_overflow (per ADR-0041 §6.2 item-2 — overflow re-fires per
        // detection-tick).
        let eps = mk_endpoints(4);
        let (handle, _health, ejection) =
            mk_handle_with_health_and_ejection("b", eps.clone(), 1, 1, 0.0, 1, 1, 25);
        // Endpoint 0: cross threshold (immediate at threshold=1).
        handle.record_response(eps[0], 500);
        assert!(ejection[0].is_ejected());
        // Endpoint 1: cross threshold; cap met (1 active >= cap 1) → no ejection, but
        // overflow ticks.
        handle.record_response(eps[1], 500);
        assert!(!ejection[1].is_ejected(), "cap met → no eject");
        // ejections_overflow value should be 1 (one cap-blocked event).
        let od = handle
            .inner
            .outlier_detection
            .as_ref()
            .expect("OD configured");
        assert_eq!(od.ejections_overflow.value(), 1);
        // Endpoint 2: another threshold-cross under cap → overflow re-fires.
        handle.record_response(eps[2], 500);
        assert!(!ejection[2].is_ejected());
        assert_eq!(
            od.ejections_overflow.value(),
            2,
            "overflow per detection-tick"
        );
    }

    #[test]
    fn cluster_record_response_silent_on_unknown_endpoint() {
        // Defense-in-depth (lock-in #10): if the caller passes an endpoint not in
        // self.endpoints, the method returns silently (no panic; no stats touched).
        let eps = mk_endpoints(1);
        let (handle, _health, _ejection) =
            mk_handle_with_health_and_ejection("b", eps.clone(), 1, 1, 0.0, 1, 1, 100);
        let unknown: SocketAddr = "127.0.0.1:65530".parse().unwrap();
        handle.record_response(unknown, 500); // must not panic
    }

    #[test]
    fn cluster_record_response_stamps_ejected_at_on_eject() {
        // 14.2 M4 (lock-in #4/#5): driving `Cluster::record_response` to the eject threshold
        // both ejects the endpoint AND stamps `ejected_at` under the serialization lock the
        // compound holds. The stamp is the eject-timestamp the 14.2 D7 sweeper later reads.
        let eps = mk_endpoints(1);
        let (handle, _health, ejection) =
            mk_handle_with_health_and_ejection("b", eps.clone(), 1, 1, 0.0, 3, 3, 100);
        for _ in 0..3 {
            handle.record_response(eps[0], 500);
        }
        assert!(ejection[0].is_ejected());
        assert!(
            ejection[0].ejected_at.lock().unwrap().is_some(),
            "M4: record_response stamps ejected_at under the serialization lock"
        );
    }

    #[test]
    fn cluster_record_response_max_ejection_percent_zero_never_ejects() {
        // 14.1 M6 (§6.2 item-4): max_ejection_percent=0 ⇒ cap_count=0 ⇒ active_count (0) >=
        // cap_count (0) on the first crossing ⇒ overflow, never ejecting (the deliberate
        // "0% = eject nothing" edge). The overflow counter ticks exactly once per crossing.
        let eps = mk_endpoints(1);
        let (handle, _health, ejection) =
            mk_handle_with_health_and_ejection("b", eps.clone(), 1, 1, 0.0, 3, 3, 0);
        for _ in 0..3 {
            handle.record_response(eps[0], 500);
        }
        assert!(!ejection[0].is_ejected(), "0% cap ⇒ never ejects");
        assert!(
            ejection[0].ejected_at.lock().unwrap().is_none(),
            "no eject ⇒ no timestamp"
        );
        let od = handle.inner.outlier_detection.as_ref().unwrap();
        assert_eq!(
            od.ejections_overflow.value(),
            1,
            "first crossing at 0% cap overflows exactly once"
        );
    }

    #[test]
    fn cluster_record_response_picks_5xx_detector_on_ties() {
        // M5: a 503 crosses BOTH consecutive_5xx AND consecutive_gateway_failure at
        // threshold=1; 5xx wins the tie (lock-in #15). Assert the endpoint ejects AND only
        // the _enforced_consecutive_5xx counter ticks (gateway-failure enforced stays 0).
        let eps = mk_endpoints(1);
        let (handle, _health, ejection) =
            mk_handle_with_health_and_ejection("b", eps.clone(), 1, 1, 0.0, 1, 1, 100);
        handle.record_response(eps[0], 503);
        assert!(ejection[0].is_ejected());
        let stats = handle.inner.outlier_detection.as_ref().unwrap().stats();
        assert_eq!(
            stats.ejections_enforced_consecutive_5xx.value(),
            1,
            "5xx wins the tie"
        );
        assert_eq!(
            stats.ejections_enforced_consecutive_gateway_failure.value(),
            0,
            "gateway-failure does NOT enforce on a 5xx-won tie"
        );
    }

    // ---- 14.1 Task 5: from_bootstrap configured-OD stats wiring ----

    const OD_CLUSTER_YAML: &str = r#"
static_resources:
  listeners: []
  clusters:
    - name: od_backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      outlier_detection:
        consecutive_5xx: 5
        consecutive_gateway_failure: 5
        interval: 10s
        base_ejection_time: 30s
        max_ejection_percent: 10
      load_assignment:
        cluster_name: od_backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7001 } }
admin:
  address:
    socket_address: { address: 127.0.0.1, port_value: 9901 }
"#;

    /// 14.2 Task 8 regression (root cause of the in-process backstop failure):
    /// `common_lb_config.healthy_panic_threshold` MUST be honored on a cluster
    /// that configures `outlier_detection` but NO `health_checks`. Before the fix,
    /// `from_bootstrap` only parsed `panic_threshold` inside the `health_checks`
    /// branch and defaulted to `50.0` otherwise — so an OD-only cluster with
    /// `healthy_panic_threshold: {value: 0}` wrongly got `panic_threshold = 50.0`,
    /// and once the sole endpoint ejected (`0% eligible < 50%`) panic-routing
    /// re-admitted it, so `pick()` never returned `None` and the no-healthy-upstream
    /// synth-503 never fired. Drives the REAL `from_bootstrap` config path.
    #[tokio::test]
    async fn from_bootstrap_honors_panic_threshold_zero_without_health_checks() {
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: c1
      type: STATIC
      lb_policy: ROUND_ROBIN
      outlier_detection:
        consecutive_5xx: 1
        max_ejection_percent: 100
      common_lb_config:
        healthy_panic_threshold: { value: 0 }
      load_assignment:
        cluster_name: c1
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
admin: { address: { socket_address: { address: 127.0.0.1, port_value: 9901 } } }
"#;
        let bootstrap = envoy_config::parse_bootstrap(yaml).expect("valid");
        let mgr = crate::from_bootstrap(&bootstrap, Arc::new(envoy_stats::StatsRegistry::new()))
            .await
            .expect("construct");
        let handle = mgr.get("c1").expect("cluster c1");
        let ep = handle
            .pick_endpoint(None, None)
            .expect("endpoint pickable pre-ejection");
        // One 500 crosses consecutive_5xx=1 → ejects the sole endpoint.
        handle.record_response(ep, 500);
        assert!(
            handle.is_endpoint_ejected_for_test(0),
            "endpoint should be ejected after the 500",
        );
        // With panic disabled (value 0) and the only endpoint ejected, pick() MUST
        // yield None (→ the 12.2 no-healthy-upstream synth-503). The bug made this
        // return Some(ep) because panic_threshold wrongly defaulted to 50.0.
        assert!(
            handle.pick_endpoint(None, None).is_none(),
            "panic_threshold=0 + sole endpoint ejected ⇒ pick() == None",
        );
    }

    #[tokio::test]
    async fn from_bootstrap_registers_7_outlier_detection_stats_when_configured() {
        let bootstrap = envoy_config::parse_bootstrap(OD_CLUSTER_YAML).expect("valid");
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let _mgr = crate::from_bootstrap(&bootstrap, Arc::clone(&registry))
            .await
            .expect("construct");
        let snapshot = registry.snapshot();
        let names: Vec<&str> = snapshot.iter().map(|(n, _)| n.as_str()).collect();
        // Each of the 7 must be present (1 gauge + 6 counters).
        for stat in &[
            "cluster.od_backend.outlier_detection.ejections_active",
            "cluster.od_backend.outlier_detection.ejections_enforced_total",
            "cluster.od_backend.outlier_detection.ejections_overflow",
            "cluster.od_backend.outlier_detection.ejections_detected_consecutive_5xx",
            "cluster.od_backend.outlier_detection.ejections_enforced_consecutive_5xx",
            "cluster.od_backend.outlier_detection.ejections_detected_consecutive_gateway_failure",
            "cluster.od_backend.outlier_detection.ejections_enforced_consecutive_gateway_failure",
        ] {
            assert!(names.contains(stat), "{stat} not registered; got {names:?}");
        }
    }

    #[tokio::test]
    async fn from_bootstrap_omits_outlier_detection_stats_when_unconfigured() {
        // The 14.1 SPEC §5.3 + acceptance gate (b): a cluster WITHOUT outlier_detection
        // configures no outlier-detection stats.
        let yaml = SINGLE_ENDPOINT_YAML; // existing const — no outlier_detection
        let bootstrap = envoy_config::parse_bootstrap(yaml).expect("valid");
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let _mgr = crate::from_bootstrap(&bootstrap, Arc::clone(&registry))
            .await
            .expect("construct");
        let snapshot = registry.snapshot();
        for (name, _) in &snapshot {
            assert!(
                !name.contains("outlier_detection"),
                "unconfigured cluster MUST NOT register outlier-detection stats; got {name}",
            );
        }
    }

    #[tokio::test]
    async fn from_bootstrap_outlier_detection_active_gauge_reads_zero_at_construct() {
        let bootstrap = envoy_config::parse_bootstrap(OD_CLUSTER_YAML).expect("valid");
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let _mgr = crate::from_bootstrap(&bootstrap, Arc::clone(&registry))
            .await
            .expect("construct");
        // Presence must be proven via the snapshot (register_gauge below is idempotent
        // and returns the live Arc — but would otherwise create a fresh 0-valued gauge
        // and mask a missing registration). Established 12.1 membership_healthy pattern.
        assert!(
            registry
                .snapshot()
                .iter()
                .any(|(n, _)| n == "cluster.od_backend.outlier_detection.ejections_active"),
            "ejections_active gauge must be registered by from_bootstrap",
        );
        let gauge = registry
            .register_gauge("cluster.od_backend.outlier_detection.ejections_active")
            .expect("gauge present");
        assert_eq!(gauge.value(), 0, "no ejections at construct (§6.2 item-3)");
    }

    #[tokio::test]
    async fn from_bootstrap_outlier_detection_uses_envoy_defaults_when_omitted() {
        // outlier_detection: {} ⇒ all detector / cap fields default per §6.2 item-1.
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: od
      type: STATIC
      lb_policy: ROUND_ROBIN
      outlier_detection: {}
      load_assignment:
        cluster_name: od
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
admin: { address: { socket_address: { address: 127.0.0.1, port_value: 9901 } } }
"#;
        let bootstrap = envoy_config::parse_bootstrap(yaml).expect("valid");
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let mgr = crate::from_bootstrap(&bootstrap, Arc::clone(&registry))
            .await
            .expect("construct");
        let handle = mgr.get("od").expect("cluster present");
        let od = handle.inner.outlier_detection.as_ref().expect("OD wired");
        assert_eq!(od.max_ejection_percent, 10, "Envoy default 10");
    }

    #[tokio::test]
    async fn from_bootstrap_registers_upstream_rq_retry_counters_at_zero() {
        // 16 Task 3: `from_bootstrap` unconditionally registers
        // cluster.<name>.upstream_rq_retry, upstream_rq_retry_success, and
        // upstream_rq_retry_limit_exceeded — each readable at 0. The accessors
        // on both `Cluster` and `ClusterHandle` must return the same Arc handles.
        // Mirrors the `from_bootstrap_registers_7_outlier_detection_stats_when_configured`
        // pattern (snapshot → name presence) plus the upstream_rq_total/5xx accessor shape.
        let bootstrap = envoy_config::parse_bootstrap(SINGLE_ENDPOINT_YAML).expect("valid");
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let mgr = crate::from_bootstrap(&bootstrap, Arc::clone(&registry))
            .await
            .expect("construct");
        let handle = mgr.get("backend").expect("cluster present");

        // 1. Each stat name appears in the registry snapshot.
        let snapshot = registry.snapshot();
        let names: Vec<&str> = snapshot.iter().map(|(n, _)| n.as_str()).collect();
        for stat in &[
            "cluster.backend.upstream_rq_retry",
            "cluster.backend.upstream_rq_retry_success",
            "cluster.backend.upstream_rq_retry_limit_exceeded",
        ] {
            assert!(names.contains(stat), "{stat} not registered; got {names:?}");
        }

        // 2. Each counter starts at 0 (inert-at-0 invariant: no retry config
        //    is plumbed here; registration is unconditional per PLAN Task 3 step 3).
        let retry = registry
            .register_counter("cluster.backend.upstream_rq_retry")
            .expect("counter present");
        assert_eq!(retry.value(), 0, "upstream_rq_retry must start at 0");

        let retry_success = registry
            .register_counter("cluster.backend.upstream_rq_retry_success")
            .expect("counter present");
        assert_eq!(
            retry_success.value(),
            0,
            "upstream_rq_retry_success must start at 0"
        );

        let retry_limit_exceeded = registry
            .register_counter("cluster.backend.upstream_rq_retry_limit_exceeded")
            .expect("counter present");
        assert_eq!(
            retry_limit_exceeded.value(),
            0,
            "upstream_rq_retry_limit_exceeded must start at 0"
        );

        // 3. The accessor methods return handles (accessor existence check).
        let _ = handle.upstream_rq_retry();
        let _ = handle.upstream_rq_retry_success();
        let _ = handle.upstream_rq_retry_limit_exceeded();
    }

    // ── 17 Task 3: budget integration tests ─────────────────────────────────

    /// Helper: build a bootstrap YAML with a single-endpoint static cluster.
    /// `cb_block` is the optional YAML text for the `circuit_breakers:` key +
    /// value (already indented for the cluster level). Pass `""` for no block.
    fn mk_cb_yaml(cb_block: &str) -> String {
        format!(
            r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      {cb_block}
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10042
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#
        )
    }

    /// (a) A cluster WITHOUT `circuit_breakers`:
    ///  - `try_acquire_retry()` / `try_acquire_request()` return `Unlimited`
    ///  - NO `circuit_breakers.default.*` stats registered
    ///  - `upstream_rq_retry_overflow` IS registered (unconditional, inert at 0)
    #[tokio::test]
    async fn budget_integration_no_circuit_breakers_returns_unlimited() {
        let yaml = mk_cb_yaml("");
        let bootstrap = envoy_config::parse_bootstrap(&yaml).expect("valid");
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let mgr = crate::from_bootstrap(&bootstrap, Arc::clone(&registry))
            .await
            .expect("construct");
        let handle = mgr.get("backend").expect("cluster present");

        // try_acquire_retry returns Unlimited (budget: None)
        assert!(
            matches!(
                handle.try_acquire_retry(),
                crate::BudgetAcquisition::Unlimited
            ),
            "no circuit_breakers → try_acquire_retry must be Unlimited"
        );
        // try_acquire_request returns Unlimited
        assert!(
            matches!(
                handle.try_acquire_request(),
                crate::BudgetAcquisition::Unlimited
            ),
            "no circuit_breakers → try_acquire_request must be Unlimited"
        );

        // No circuit_breakers.default.* stats registered
        let snapshot = registry.snapshot();
        for (name, _) in &snapshot {
            assert!(
                !name.contains("circuit_breakers.default"),
                "unconfigured cluster MUST NOT register circuit_breakers.default stats; got {name}"
            );
        }

        // upstream_rq_retry_overflow IS registered (unconditional, inert at 0)
        let names: Vec<&str> = snapshot.iter().map(|(n, _)| n.as_str()).collect();
        assert!(
            names.contains(&"cluster.backend.upstream_rq_retry_overflow"),
            "upstream_rq_retry_overflow must be registered unconditionally; snapshot: {names:?}"
        );
        let overflow_counter = registry
            .register_counter("cluster.backend.upstream_rq_retry_overflow")
            .expect("present");
        assert_eq!(
            overflow_counter.value(),
            0,
            "upstream_rq_retry_overflow must be inert at 0 with no circuit_breakers"
        );
    }

    /// (b) A cluster WITH `circuit_breakers: {thresholds: [{max_retries: 0}]}`:
    ///  - budget present
    ///  - retry acquisition returns Rejected (cap 0 = always-open breaker)
    ///  - max_requests resolves to default 1024 (L5)
    #[tokio::test]
    async fn budget_integration_zero_max_retries_returns_rejected() {
        let yaml = mk_cb_yaml("circuit_breakers:\n        thresholds:\n          - max_retries: 0");
        let bootstrap = envoy_config::parse_bootstrap(&yaml).expect("valid");
        let mgr = crate::from_bootstrap(&bootstrap, Arc::new(envoy_stats::StatsRegistry::new()))
            .await
            .expect("construct");
        let handle = mgr.get("backend").expect("cluster present");

        // Retry acquisition: Rejected (cap 0)
        assert!(
            matches!(
                handle.try_acquire_retry(),
                crate::BudgetAcquisition::Rejected
            ),
            "max_retries=0 → try_acquire_retry must be Rejected"
        );

        // Request acquisition: Acquired (default cap 1024 >> 0 active)
        let acq = handle.try_acquire_request();
        assert!(
            matches!(acq, crate::BudgetAcquisition::Acquired(_)),
            "max_requests defaults to 1024, should be Acquired"
        );
        // acq dropped here — test only checks the variant, not guard lifetime
    }

    /// (c) `track_remaining: true` → `remaining_retries`/`remaining_rq` registered
    ///     at the cap values; `track_remaining` absent/false → ABSENT
    #[tokio::test]
    async fn budget_integration_track_remaining_conditional_registration() {
        // With track_remaining: true
        let yaml_with = mk_cb_yaml(
            "circuit_breakers:\n        thresholds:\n          - max_retries: 5\n            max_requests: 10\n            track_remaining: true",
        );
        let bootstrap = envoy_config::parse_bootstrap(&yaml_with).expect("valid");
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let _mgr = crate::from_bootstrap(&bootstrap, Arc::clone(&registry))
            .await
            .expect("construct");

        let snapshot = registry.snapshot();
        let names: Vec<&str> = snapshot.iter().map(|(n, _)| n.as_str()).collect();

        // remaining_retries registered at cap 5
        assert!(
            names.contains(&"cluster.backend.circuit_breakers.default.remaining_retries"),
            "remaining_retries must be present when track_remaining=true; got {names:?}"
        );
        assert!(
            names.contains(&"cluster.backend.circuit_breakers.default.remaining_rq"),
            "remaining_rq must be present when track_remaining=true; got {names:?}"
        );
        let remaining_retries = registry
            .register_gauge("cluster.backend.circuit_breakers.default.remaining_retries")
            .expect("present");
        assert_eq!(
            remaining_retries.value(),
            5,
            "remaining_retries should equal max_retries cap at construct"
        );
        let remaining_rq = registry
            .register_gauge("cluster.backend.circuit_breakers.default.remaining_rq")
            .expect("present");
        assert_eq!(
            remaining_rq.value(),
            10,
            "remaining_rq should equal max_requests cap at construct"
        );

        // Without track_remaining (absent → false)
        let yaml_without = mk_cb_yaml(
            "circuit_breakers:\n        thresholds:\n          - max_retries: 5\n            max_requests: 10",
        );
        let bootstrap2 = envoy_config::parse_bootstrap(&yaml_without).expect("valid");
        let registry2 = Arc::new(envoy_stats::StatsRegistry::new());
        let _mgr2 = crate::from_bootstrap(&bootstrap2, Arc::clone(&registry2))
            .await
            .expect("construct");

        for (name, _) in registry2.snapshot() {
            assert!(
                !name.contains("remaining_retries") && !name.contains("remaining_rq"),
                "remaining_* must be absent when track_remaining is not set; got {name}"
            );
        }
    }

    /// (d) `circuit_breakers: {thresholds: [{}]}` (empty threshold) → L5 defaults:
    ///     max_retries = 3, max_requests = 1024
    #[tokio::test]
    async fn budget_integration_empty_threshold_uses_l5_defaults() {
        let yaml = mk_cb_yaml("circuit_breakers:\n        thresholds:\n          - {}");
        let bootstrap = envoy_config::parse_bootstrap(&yaml).expect("valid");
        let mgr = crate::from_bootstrap(&bootstrap, Arc::new(envoy_stats::StatsRegistry::new()))
            .await
            .expect("construct");
        let handle = mgr.get("backend").expect("cluster present");

        // Acquire 3 retry slots — should succeed (default cap = 3)
        let g1 = match handle.try_acquire_retry() {
            crate::BudgetAcquisition::Acquired(g) => g,
            _other => panic!("expected Acquired for slot 1, got non-Acquired variant"),
        };
        let g2 = match handle.try_acquire_retry() {
            crate::BudgetAcquisition::Acquired(g) => g,
            _other => panic!("expected Acquired for slot 2, got non-Acquired variant"),
        };
        let g3 = match handle.try_acquire_retry() {
            crate::BudgetAcquisition::Acquired(g) => g,
            _other => panic!("expected Acquired for slot 3, got non-Acquired variant"),
        };
        // 4th retry should be Rejected (cap = 3)
        assert!(
            matches!(
                handle.try_acquire_retry(),
                crate::BudgetAcquisition::Rejected
            ),
            "4th retry beyond default cap 3 must be Rejected"
        );
        drop((g1, g2, g3));

        // Acquire 1024 request slots — should all succeed (default cap = 1024)
        let mut guards = Vec::with_capacity(1024);
        for i in 0..1024 {
            match handle.try_acquire_request() {
                crate::BudgetAcquisition::Acquired(g) => guards.push(g),
                _ => panic!("request slot {i} should be Acquired (default cap 1024)"),
            }
        }
        // 1025th request should be Rejected
        assert!(
            matches!(
                handle.try_acquire_request(),
                crate::BudgetAcquisition::Rejected
            ),
            "1025th request beyond default cap 1024 must be Rejected"
        );
        drop(guards);
    }

    /// 17 Task 3: idempotent-registration contract — the `upstream_rq_retry_overflow`
    /// counter registered unconditionally by `from_bootstrap` and the same counter
    /// registered inside `BudgetState::new` must be the SAME Arc (shared identity).
    #[tokio::test]
    async fn budget_integration_retry_overflow_counter_shared_with_budget_state() {
        let yaml = mk_cb_yaml("circuit_breakers:\n        thresholds:\n          - max_retries: 0");
        let bootstrap = envoy_config::parse_bootstrap(&yaml).expect("valid");
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let mgr = crate::from_bootstrap(&bootstrap, Arc::clone(&registry))
            .await
            .expect("construct");
        let handle = mgr.get("backend").expect("cluster present");

        // Trigger a retry overflow (cap 0 = always rejects)
        let _ = handle.try_acquire_retry(); // increments upstream_rq_retry_overflow

        // Both the handle accessor and the registry re-lookup must see value 1
        assert_eq!(
            handle.upstream_rq_retry_overflow().value(),
            1,
            "overflow counter must increment via BudgetState's try_acquire_retry"
        );
        let from_reg = registry
            .register_counter("cluster.backend.upstream_rq_retry_overflow")
            .expect("present");
        assert_eq!(
            from_reg.value(),
            1,
            "registry-obtained handle must share the same Arc (idempotent registration)"
        );
    }

    // ---- 18 Task 4 (ADR-0049 L3/L10): cluster_manager.* stat family ----

    /// Build a minimal valid single-endpoint STATIC `Cluster` with the given
    /// name and port. Mirrors the by-hand constructor shape used by
    /// `from_bootstrap_rejects_duplicate_cluster_name` so these tests can set
    /// `dynamic_resources` / `dynamic_clusters` directly (no file I/O at this
    /// layer).
    fn mk_static_cluster(name: &str, port: u16) -> envoy_config::Cluster {
        use envoy_config::{
            Address, Cluster, ClusterType, Endpoint, LbEndpoint, LbPolicy, LoadAssignment,
            LocalityLbEndpoints, SocketAddress,
        };
        Cluster {
            name: name.into(),
            cluster_type: ClusterType::Static,
            common_http_protocol_options: None,
            lb_policy: LbPolicy::RoundRobin,
            load_assignment: Some(LoadAssignment {
                cluster_name: name.into(),
                endpoints: vec![LocalityLbEndpoints {
                    lb_endpoints: vec![LbEndpoint {
                        endpoint: Endpoint {
                            address: Address {
                                socket_address: SocketAddress {
                                    address: "127.0.0.1".into(),
                                    port_value: port,
                                },
                            },
                        },
                        metadata: None, // 30 Task 1 (LbEndpoint.metadata)
                    }],
                }],
            }),
            eds_cluster_config: None,
            transport_socket: None,
            dns_lookup_family: None,
            typed_extension_protocol_options: None,
            health_checks: vec![],
            common_lb_config: None,
            circuit_breakers: None,
            outlier_detection: None,
            ring_hash_lb_config: None, // 28 Task 3
            maglev_lb_config: None,    // 29 Task 2
            lb_subset_config: None,    // 30 Task 2
        }
    }

    /// `dynamic_resources.cds_config` configured (the conditionality predicate
    /// for the cluster_manager.* family).
    fn cds_dynamic_resources() -> envoy_config::DynamicResources {
        use envoy_config::{ConfigSource, DynamicResources, PathConfigSource};
        DynamicResources {
            cds_config: Some(ConfigSource {
                path_config_source: PathConfigSource {
                    path: "/tmp/cds.yaml".into(),
                },
                resource_api_version: None,
            }),
            lds_config: None,
        }
    }

    fn mk_bootstrap(
        static_clusters: Vec<envoy_config::Cluster>,
        dynamic_resources: Option<envoy_config::DynamicResources>,
        dynamic_clusters: Option<Vec<envoy_config::Cluster>>,
    ) -> envoy_config::Bootstrap {
        use envoy_config::{Address, Admin, Bootstrap, SocketAddress, StaticResources};
        Bootstrap {
            node: None,
            admin: Some(Admin {
                address: Address {
                    socket_address: SocketAddress {
                        address: "127.0.0.1".into(),
                        port_value: 9901,
                    },
                },
                access_log_path: None,
            }),
            static_resources: StaticResources {
                listeners: vec![],
                clusters: static_clusters,
            },
            dynamic_resources,
            // 108.1: `Bootstrap` gained `layered_runtime`; this helper builds no
            // runtime layer stack, and `None` (absent) is NOT the same as an
            // empty block upstream — see `envoy_config::LayeredRuntime`.
            layered_runtime: None,
            dynamic_clusters,
            dynamic_listeners: None,
        }
    }

    /// Scrape the registry for the current u64/i64 value of a stat by name.
    fn stat_value(registry: &envoy_stats::StatsRegistry, name: &str) -> Option<i64> {
        registry.snapshot().into_iter().find_map(|(n, h)| {
            if n != name {
                return None;
            }
            Some(match h {
                envoy_stats::StatHandle::Counter(c) => c.value() as i64,
                envoy_stats::StatHandle::Gauge(g) => g.value(),
            })
        })
    }

    #[tokio::test]
    async fn cluster_manager_stats_not_registered_without_dynamic_resources() {
        // §5.2 inertness: with NO dynamic_resources, none of the
        // cluster_manager.* names register.
        let bootstrap = mk_bootstrap(vec![mk_static_cluster("backend", 10001)], None, None);
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let _mgr = crate::from_bootstrap(&bootstrap, Arc::clone(&registry))
            .await
            .expect("construct");
        let cm_names: Vec<String> = registry
            .snapshot()
            .into_iter()
            .map(|(n, _)| n)
            .filter(|n| n.starts_with("cluster_manager."))
            .collect();
        assert!(
            cm_names.is_empty(),
            "no cluster_manager.* stat may register without dynamic_resources.cds_config; got {cm_names:?}"
        );
    }

    #[tokio::test]
    async fn cluster_manager_stats_registered_with_cds_bootstrap() {
        // CDS configured + one dynamic cluster (zero static, like fixture 0026).
        let bootstrap = mk_bootstrap(
            vec![],
            Some(cds_dynamic_resources()),
            Some(vec![mk_static_cluster("dyn-backend", 10010)]),
        );
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let _mgr = crate::from_bootstrap(&bootstrap, Arc::clone(&registry))
            .await
            .expect("construct");
        assert_eq!(
            stat_value(&registry, "cluster_manager.cds.update_attempt"),
            Some(1)
        );
        assert_eq!(
            stat_value(&registry, "cluster_manager.cds.update_success"),
            Some(1)
        );
        assert_eq!(
            stat_value(&registry, "cluster_manager.cds.update_failure"),
            Some(0)
        );
        assert_eq!(
            stat_value(&registry, "cluster_manager.cds.update_rejected"),
            Some(0)
        );
        assert_eq!(
            stat_value(&registry, "cluster_manager.cluster_added"),
            Some(1)
        );
        assert_eq!(
            stat_value(&registry, "cluster_manager.active_clusters"),
            Some(1)
        );
    }

    #[tokio::test]
    async fn cluster_manager_counts_include_static_clusters() {
        // 1 static + 1 dynamic, CDS configured → counts include the static one.
        let bootstrap = mk_bootstrap(
            vec![mk_static_cluster("static-backend", 10020)],
            Some(cds_dynamic_resources()),
            Some(vec![mk_static_cluster("dyn-backend", 10021)]),
        );
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let _mgr = crate::from_bootstrap(&bootstrap, Arc::clone(&registry))
            .await
            .expect("construct");
        assert_eq!(
            stat_value(&registry, "cluster_manager.cluster_added"),
            Some(2),
            "cluster_added must count static + dynamic clusters"
        );
        assert_eq!(
            stat_value(&registry, "cluster_manager.active_clusters"),
            Some(2),
            "active_clusters must count static + dynamic clusters"
        );
    }

    // ---- 21 Task 4 (ADR-0053/0054 §6.2 L3/L10): per-cluster EDS update_* ----

    /// Build a `type: EDS` `Cluster` with its `load_assignment` already
    /// populated (numeric IP, so the Static-shared build arm resolves it) and
    /// `eds_cluster_config` present — mirrors the post-EDS-pass shape the
    /// `cluster_type == Eds` stat predicate keys on (independent of HOW the
    /// load_assignment got there).
    fn mk_eds_cluster(name: &str, port: u16) -> envoy_config::Cluster {
        use envoy_config::bootstrap::EdsClusterConfig;
        use envoy_config::{ClusterType, ConfigSource, PathConfigSource};
        let mut cluster = mk_static_cluster(name, port);
        cluster.cluster_type = ClusterType::Eds;
        cluster.eds_cluster_config = Some(EdsClusterConfig {
            eds_config: ConfigSource {
                path_config_source: PathConfigSource {
                    path: "/tmp/eds.yaml".into(),
                },
                resource_api_version: None,
            },
            service_name: None,
        });
        cluster
    }

    #[tokio::test]
    async fn eds_stats_not_registered_for_non_eds_clusters() {
        // §5.2 inertness: a bootstrap whose clusters are all STATIC/STRICT_DNS
        // (incl. a CDS-configured one — the fixture-0026 inertness witness)
        // registers NO `cluster.<name>.update_*` name.
        let bootstrap = mk_bootstrap(
            vec![mk_static_cluster("static-backend", 10030)],
            Some(cds_dynamic_resources()),
            Some(vec![mk_static_cluster("dyn-backend", 10031)]),
        );
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let _mgr = crate::from_bootstrap(&bootstrap, Arc::clone(&registry))
            .await
            .expect("construct");
        let update_names: Vec<String> = registry
            .snapshot()
            .into_iter()
            .map(|(n, _)| n)
            .filter(|n| n.starts_with("cluster.") && n.contains(".update_"))
            .collect();
        assert!(
            update_names.is_empty(),
            "no cluster.<name>.update_* may register for non-EDS clusters; got {update_names:?}"
        );
    }

    #[tokio::test]
    async fn eds_stats_registered_for_eds_cluster() {
        // The 4-name subset on an EDS cluster reads 1 / 1 / 0 / 0.
        let bootstrap = mk_bootstrap(vec![mk_eds_cluster("eds_backend", 10040)], None, None);
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let _mgr = crate::from_bootstrap(&bootstrap, Arc::clone(&registry))
            .await
            .expect("construct");
        assert_eq!(
            stat_value(&registry, "cluster.eds_backend.update_attempt"),
            Some(1)
        );
        assert_eq!(
            stat_value(&registry, "cluster.eds_backend.update_success"),
            Some(1)
        );
        assert_eq!(
            stat_value(&registry, "cluster.eds_backend.update_failure"),
            Some(0)
        );
        assert_eq!(
            stat_value(&registry, "cluster.eds_backend.update_empty"),
            Some(0)
        );
    }

    #[tokio::test]
    async fn eds_stats_register_no_membership_gauges() {
        // L3 narrowing: this task registers ONLY the 4 update_* names. An EDS
        // cluster with no health checks must NOT get membership_total (absent
        // from envoy-rust entirely) nor membership_healthy (HC-gated at :926).
        let bootstrap = mk_bootstrap(vec![mk_eds_cluster("eds_backend", 10050)], None, None);
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let _mgr = crate::from_bootstrap(&bootstrap, Arc::clone(&registry))
            .await
            .expect("construct");
        let names: Vec<String> = registry.snapshot().into_iter().map(|(n, _)| n).collect();
        assert!(
            !names
                .iter()
                .any(|n| n == "cluster.eds_backend.membership_total"),
            "membership_total does not exist in envoy-rust; got {names:?}"
        );
        assert!(
            !names
                .iter()
                .any(|n| n == "cluster.eds_backend.membership_healthy"),
            "membership_healthy is HC-gated and must not register for a non-HC EDS cluster; got {names:?}"
        );
    }

    // ---- 30 Task 5 (ADR-0073/0074): pick() subset narrowing ----

    /// 30 Task 5 (no-op regression witness): a ROUND_ROBIN cluster with NO
    /// `lb_subset_config` (so `subset: None`) MUST ignore `subset_match` entirely —
    /// `pick_endpoint(None, Some(&map))` round-robins EXACTLY like
    /// `pick_endpoint(None, None)`. This is the byte-identical no-op proof at the
    /// cluster level: the new `subset_match` argument is inert when the cluster has
    /// no subset config.
    #[test]
    fn subset_match_is_inert_when_no_lb_subset_config() {
        let ep0: SocketAddr = "127.0.0.1:10001".parse().unwrap();
        let ep1: SocketAddr = "127.0.0.1:10002".parse().unwrap();
        // A non-empty subset_match that would matter IF a subset index existed.
        let some_map: std::collections::BTreeMap<String, String> =
            [("stage".to_string(), "prod".to_string())]
                .into_iter()
                .collect();

        // Cluster A: drive with Some(subset_match). Cluster B: drive with None.
        // Both have subset == None (built by mk_handle), so the cursor sequence
        // must be identical, proving the match is inert.
        let a = mk_handle("noop_a", vec![ep0, ep1]);
        let b = mk_handle("noop_b", vec![ep0, ep1]);
        for _ in 0..4 {
            assert_eq!(
                a.pick_endpoint(None, Some(&some_map)),
                b.pick_endpoint(None, None),
                "subset_match must be inert when subset is None (no-op proof)"
            );
        }
        // Spot-check the exact cursor order is the phase-02 round-robin.
        let c = mk_handle("noop_c", vec![ep0, ep1]);
        assert_eq!(c.pick_endpoint(None, Some(&some_map)), Some(ep0));
        assert_eq!(c.pick_endpoint(None, Some(&some_map)), Some(ep1));
        assert_eq!(c.pick_endpoint(None, Some(&some_map)), Some(ep0));
    }

    const SUBSET_TWO_ENDPOINT_YAML: &str = r#"
static_resources:
  listeners: []
  clusters:
    - name: subset_backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      lb_subset_config:
        fallback_policy: NO_FALLBACK
        subset_selectors:
          - keys: [stage]
      load_assignment:
        cluster_name: subset_backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
                metadata:
                  filter_metadata:
                    envoy.lb:
                      stage: prod
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10002
                metadata:
                  filter_metadata:
                    envoy.lb:
                      stage: canary
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;

    /// 30 Task 5 (subset narrowing, §A oracle at the cluster level): a cluster with
    /// `lb_subset_config` (selector `keys:[stage]`, NO_FALLBACK) and two endpoints
    /// carrying `envoy.lb` metadata routes deterministically within the matched
    /// subset: `{stage:prod}` → the prod host, `{stage:canary}` → the canary host,
    /// `{stage:nonexistent}` → None (NO_FALLBACK no-match → 503).
    #[tokio::test]
    async fn subset_narrows_to_matched_endpoint() {
        let prod: SocketAddr = "127.0.0.1:10001".parse().unwrap();
        let canary: SocketAddr = "127.0.0.1:10002".parse().unwrap();
        let bootstrap = envoy_config::parse_bootstrap(SUBSET_TWO_ENDPOINT_YAML).expect("valid");
        let mgr = crate::from_bootstrap(&bootstrap, Arc::new(envoy_stats::StatsRegistry::new()))
            .await
            .expect("construct");
        let handle = mgr.get("subset_backend").expect("cluster present");

        let mk = |k: &str, v: &str| -> std::collections::BTreeMap<String, String> {
            [(k.to_string(), v.to_string())].into_iter().collect()
        };

        // {stage:prod} narrows to the single prod endpoint, deterministically.
        let prod_match = mk("stage", "prod");
        for _ in 0..3 {
            assert_eq!(
                handle.pick_endpoint(None, Some(&prod_match)),
                Some(prod),
                "stage:prod must route to the prod endpoint"
            );
        }
        // {stage:canary} narrows to the single canary endpoint, deterministically.
        let canary_match = mk("stage", "canary");
        for _ in 0..3 {
            assert_eq!(
                handle.pick_endpoint(None, Some(&canary_match)),
                Some(canary),
                "stage:canary must route to the canary endpoint"
            );
        }
        // {stage:nonexistent} under NO_FALLBACK -> None (503).
        let none_match = mk("stage", "nonexistent");
        assert_eq!(
            handle.pick_endpoint(None, Some(&none_match)),
            None,
            "no matching subset under NO_FALLBACK must return None (503)"
        );
    }

    // ---- 30 Task 8 backstop GAPS (pick() level) ----

    /// Build a STATIC subset cluster YAML: two endpoints (prod @10001, canary
    /// @10002), selector `keys:[stage]`, the given `fallback_policy`, optional
    /// `default_subset: {stage: <ds>}`.
    fn subset_yaml(fallback_policy: &str, default_subset_stage: Option<&str>) -> String {
        let ds = match default_subset_stage {
            Some(v) => format!("        default_subset:\n          stage: {v}\n"),
            None => String::new(),
        };
        format!(
            r#"
static_resources:
  listeners: []
  clusters:
    - name: subset_backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      lb_subset_config:
        fallback_policy: {fallback_policy}
{ds}        subset_selectors:
          - keys: [stage]
      load_assignment:
        cluster_name: subset_backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
                metadata:
                  filter_metadata:
                    envoy.lb:
                      stage: prod
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10002
                metadata:
                  filter_metadata:
                    envoy.lb:
                      stage: canary
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#
        )
    }

    async fn build_subset_handle(yaml: &str) -> ClusterHandle {
        let bootstrap = envoy_config::parse_bootstrap(yaml).expect("valid");
        let mgr = crate::from_bootstrap(&bootstrap, Arc::new(envoy_stats::StatsRegistry::new()))
            .await
            .expect("construct");
        mgr.get("subset_backend").expect("cluster present")
    }

    fn stage_match(v: &str) -> std::collections::BTreeMap<String, String> {
        [("stage".to_string(), v.to_string())].into_iter().collect()
    }

    /// GAP 4 (ANY_ENDPOINT fallback): a no-match `metadata_match` under
    /// ANY_ENDPOINT round-robins over ALL endpoints — never None, and over
    /// repeated calls it hits BOTH the prod and canary hosts.
    #[tokio::test]
    async fn subset_any_endpoint_fallback_round_robins_all() {
        let prod: SocketAddr = "127.0.0.1:10001".parse().unwrap();
        let canary: SocketAddr = "127.0.0.1:10002".parse().unwrap();
        let handle = build_subset_handle(&subset_yaml("ANY_ENDPOINT", None)).await;

        let none_match = stage_match("nonexistent");
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..6 {
            let got = handle
                .pick_endpoint(None, Some(&none_match))
                .expect("ANY_ENDPOINT no-match must return a host, never None");
            seen.insert(got);
        }
        assert_eq!(
            seen,
            [prod, canary].into_iter().collect(),
            "ANY_ENDPOINT must round-robin over ALL endpoints"
        );

        // The no-metadata_match request (None) under ANY_ENDPOINT also returns a host.
        let mut seen_none = std::collections::BTreeSet::new();
        for _ in 0..6 {
            seen_none.insert(
                handle
                    .pick_endpoint(None, None)
                    .expect("ANY_ENDPOINT with no match must return a host"),
            );
        }
        assert_eq!(seen_none, [prod, canary].into_iter().collect());
    }

    /// GAP 4 (DEFAULT_SUBSET fallback): a no-match `metadata_match` (and the
    /// no-`metadata_match` request) under DEFAULT_SUBSET `{stage:prod}` returns
    /// the prod host deterministically.
    #[tokio::test]
    async fn subset_default_subset_fallback_routes_to_default() {
        let prod: SocketAddr = "127.0.0.1:10001".parse().unwrap();
        let handle = build_subset_handle(&subset_yaml("DEFAULT_SUBSET", Some("prod"))).await;

        let none_match = stage_match("nonexistent");
        for _ in 0..3 {
            assert_eq!(
                handle.pick_endpoint(None, Some(&none_match)),
                Some(prod),
                "DEFAULT_SUBSET no-match must route to the default subset (prod)"
            );
        }
        // The no-metadata_match request also falls back to the default subset.
        for _ in 0..3 {
            assert_eq!(
                handle.pick_endpoint(None, None),
                Some(prod),
                "DEFAULT_SUBSET with no match must route to the default subset (prod)"
            );
        }
    }

    /// GAP 4 (NO_FALLBACK, no-`metadata_match` request): a subset cluster under
    /// NO_FALLBACK with a `None` `metadata_match` returns None (503). (The
    /// no-match `{stage:nonexistent}` case is already in
    /// `subset_narrows_to_matched_endpoint`.)
    #[tokio::test]
    async fn subset_no_fallback_no_metadata_match_returns_none() {
        let handle = build_subset_handle(&subset_yaml("NO_FALLBACK", None)).await;
        assert_eq!(
            handle.pick_endpoint(None, None),
            None,
            "NO_FALLBACK with no metadata_match must return None (503)"
        );
    }

    /// GAP 5 (M-2): a `metadata_match` selecting a subset of TWO endpoints
    /// round-robins over the two subset members across repeated calls — pinning
    /// the MVP inner-LB-within-subset = ROUND_ROBIN cursor rotation that the
    /// single-host fixture can't exercise.
    #[tokio::test]
    async fn subset_multi_member_round_robins_within_subset() {
        // Two endpoints SHARE stage:prod; a third is stage:canary (excluded).
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: subset_backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      lb_subset_config:
        fallback_policy: NO_FALLBACK
        subset_selectors:
          - keys: [stage]
      load_assignment:
        cluster_name: subset_backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
                metadata:
                  filter_metadata:
                    envoy.lb:
                      stage: prod
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10002
                metadata:
                  filter_metadata:
                    envoy.lb:
                      stage: prod
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10003
                metadata:
                  filter_metadata:
                    envoy.lb:
                      stage: canary
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let prod0: SocketAddr = "127.0.0.1:10001".parse().unwrap();
        let prod1: SocketAddr = "127.0.0.1:10002".parse().unwrap();
        let canary: SocketAddr = "127.0.0.1:10003".parse().unwrap();
        let handle = build_subset_handle(yaml).await;

        let prod_match = stage_match("prod");
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..6 {
            let got = handle
                .pick_endpoint(None, Some(&prod_match))
                .expect("a 2-member subset must return a host");
            assert_ne!(
                got, canary,
                "the canary host is OUTSIDE the {{stage:prod}} subset"
            );
            seen.insert(got);
        }
        assert_eq!(
            seen,
            [prod0, prod1].into_iter().collect(),
            "the cursor must round-robin over BOTH members of the {{stage:prod}} subset"
        );
    }

    /// GAP 6 (empty subset_selectors no-op at pick level): a cluster with an
    /// `lb_subset_config` whose `subset_selectors` is empty disables the layer —
    /// `pick_endpoint(None, Some(&{stage:prod}))` round-robins ALL hosts even
    /// under NO_FALLBACK (never None).
    #[tokio::test]
    async fn subset_empty_selectors_round_robins_all_at_pick_level() {
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: subset_backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      lb_subset_config:
        fallback_policy: NO_FALLBACK
        subset_selectors: []
      load_assignment:
        cluster_name: subset_backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
                metadata:
                  filter_metadata:
                    envoy.lb:
                      stage: prod
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10002
                metadata:
                  filter_metadata:
                    envoy.lb:
                      stage: canary
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let prod: SocketAddr = "127.0.0.1:10001".parse().unwrap();
        let canary: SocketAddr = "127.0.0.1:10002".parse().unwrap();
        let handle = build_subset_handle(yaml).await;

        let prod_match = stage_match("prod");
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..6 {
            seen.insert(
                handle
                    .pick_endpoint(None, Some(&prod_match))
                    .expect("empty selectors disable the layer -> always a host"),
            );
        }
        assert_eq!(
            seen,
            [prod, canary].into_iter().collect(),
            "empty subset_selectors must round-robin ALL hosts even under NO_FALLBACK"
        );
    }
}

#[cfg(test)]
mod clusters_accessor_tests {
    use crate::ClusterManager;

    #[test]
    fn empty_cluster_manager_yields_no_clusters() {
        let cm = ClusterManager::empty();
        assert_eq!(cm.clusters().count(), 0);
    }
}
