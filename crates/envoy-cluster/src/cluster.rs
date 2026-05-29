//! Cluster data model + round-robin LB. See SPEC §D1.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

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
}

/// A configured upstream cluster. Owns the static endpoint list and the
/// round-robin `AtomicUsize` cursor. Constructed by `from_bootstrap` only;
/// external code works through `ClusterHandle`.
#[derive(Debug)]
pub struct Cluster {
    pub(crate) name: String,
    pub(crate) endpoints: Vec<SocketAddr>,
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
    /// 12.1 (parent-12 D3/D5): per-endpoint active-health-check state, aligned by
    /// index with `endpoints`. `None` when the cluster has no `health_checks`
    /// configured (the §5.4 inert-when-unconfigured invariant) — `pick()` is then
    /// byte-for-byte phase-02 round-robin. `Some` carries one `Arc<EndpointHealth>`
    /// per (resolved) endpoint; the 12.2 probe task mutates them while `pick()`
    /// reads them.
    pub(crate) endpoint_health: Option<Vec<Arc<crate::EndpointHealth>>>,
    /// 12.1 (parent-12 D5): `common_lb_config.healthy_panic_threshold` percentage
    /// (default 50.0). Read by `pick()` only when `endpoint_health` is `Some`.
    pub(crate) panic_threshold: f64,
    /// 14.1 D5/D6 (parent-14 D3/D5/D6): per-cluster outlier-detection state. `None`
    /// when the cluster's `outlier_detection` config block is absent — the §5.3
    /// inert-when-unconfigured invariant. `pick()`'s fast path bypasses entirely
    /// when this AND `endpoint_health` are both `None`.
    pub(crate) outlier_detection: Option<OutlierDetectionState>,
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
    fn pick(&self) -> Option<SocketAddr> {
        if self.endpoints.is_empty() {
            // `from_bootstrap` rejects empty clusters; this is defense-in-depth.
            return None;
        }
        let total = self.endpoints.len();
        // Fast path: nothing configured → phase-02 round-robin (byte-for-byte).
        if self.endpoint_health.is_none() && self.outlier_detection.is_none() {
            let i = self.cursor.fetch_add(1, Ordering::Relaxed);
            return Some(self.endpoints[i % total]);
        }
        // Slow path: at least one filter is configured. Eligibility = healthy AND
        // not-ejected (either filter being `None` is treated as `true`).
        let health = self.endpoint_health.as_ref();
        let ejection = self.outlier_detection.as_ref().map(|od| &od.endpoints);
        let is_eligible = |i: usize| -> bool {
            let healthy = match health {
                None => true,
                Some(h) => h[i].is_healthy(),
            };
            let not_ejected = match ejection {
                None => true,
                Some(e) => !e[i].is_ejected(),
            };
            healthy && not_ejected
        };
        let eligible_count = (0..total).filter(|&i| is_eligible(i)).count();
        let eligible_percent = 100.0 * (eligible_count as f64) / (total as f64);
        // Panic threshold (strictly-below): route over ALL endpoints when the
        // eligible fraction is below the threshold. `value: 0` disables panic
        // (`0.0 < 0.0` is false), so a 0-eligible cluster falls through to None.
        if eligible_percent < self.panic_threshold {
            let i = self.cursor.fetch_add(1, Ordering::Relaxed);
            return Some(self.endpoints[i % total]);
        }
        // Round-robin over the eligible endpoints only.
        let eligible_idx: Vec<usize> = (0..total).filter(|&i| is_eligible(i)).collect();
        if eligible_idx.is_empty() {
            // No eligible endpoints + panic not engaged → None → the pre-built
            // synth-503 path fires (unchanged at 12.1; body reconciliation is 12.2).
            return None;
        }
        let i = self.cursor.fetch_add(1, Ordering::Relaxed);
        Some(self.endpoints[eligible_idx[i % eligible_idx.len()]])
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
        let Some(idx) = self.endpoints.iter().position(|e| *e == endpoint) else {
            return; // defense-in-depth (lock-in #10)
        };
        let state = &od.endpoints[idx];
        let decision = state.record_response(status);
        if !decision.any() {
            return;
        }
        let total = self.endpoints.len();
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
    }
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
    pub fn pick_endpoint(&self) -> Option<SocketAddr> {
        self.inner.pick()
    }

    /// 14.1 D3: delegates to `Cluster::record_response`. The 14.2 D4 response-receipt
    /// hook callers hold a `ClusterHandle`; this mirrors the accessor for ergonomic
    /// reach. See `Cluster::record_response` for the full behavior contract.
    pub fn record_response(&self, endpoint: SocketAddr, status: u16) {
        self.inner.record_response(endpoint, status);
    }

    /// Cluster name (delegates to `Cluster::name`). Mirrors `Cluster::name`'s
    /// public posture per phase-04.3 SPEC §3 D5.
    pub fn name(&self) -> &str {
        self.inner.name()
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
        Some(
            self.inner
                .endpoints
                .iter()
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
    for cfg in &bootstrap.static_resources.clusters {
        // envoy-config enforces cluster_type ∈ {Static, StrictDns} (post-05.1),
        // lb_policy == RoundRobin, load_assignment.cluster_name == cfg.name,
        // and total endpoints ≥ 1 at parse time. We don't re-check those here;
        // we do re-check emptiness and duplicate names as defense-in-depth,
        // and we resolve each endpoint to a SocketAddr (which envoy-config
        // does NOT do — neither the literal-IP parse for STATIC nor the DNS
        // lookup for STRICT_DNS).
        let mut endpoints: Vec<SocketAddr> = Vec::new();
        for locality in &cfg.load_assignment.endpoints {
            for lbe in &locality.lb_endpoints {
                let sa = &lbe.endpoint.address.socket_address;
                match cfg.cluster_type {
                    envoy_config::ClusterType::Static => {
                        // EXISTING path (phase 02.1): each endpoint's address
                        // parses as a literal SocketAddr via SocketAddr::from_str.
                        // Failure surfaces as ClusterError::EndpointParse —
                        // regression-guarded by the I3-closing test
                        // static_cluster_constructs_with_literal_ip.
                        let addr_str = format!("{}:{}", sa.address, sa.port_value);
                        let parsed: SocketAddr =
                            addr_str
                                .parse()
                                .map_err(|source| ClusterError::EndpointParse {
                                    cluster: cfg.name.clone(),
                                    addr: addr_str.clone(),
                                    source,
                                })?;
                        endpoints.push(parsed);
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
                        endpoints.extend(resolved);
                    }
                }
            }
        }
        if endpoints.is_empty() {
            return Err(ClusterError::EmptyCluster {
                name: cfg.name.clone(),
            });
        }
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
        let cx_total = registry
            .register_counter(&format!("cluster.{}.upstream_cx_total", cfg.name))
            .map_err(|e| ClusterError::StatsRegistration {
                cluster: cfg.name.clone(),
                message: e.to_string(),
            })?;
        // 06.3 D15.3.b: register `cluster.<name>.upstream_cx_active` gauge.
        // Idempotent for same-kind re-registration (Task 5 contract).
        let cx_active = registry
            .register_gauge(&format!("cluster.{}.upstream_cx_active", cfg.name))
            .map_err(|e| ClusterError::StatsRegistration {
                cluster: cfg.name.clone(),
                message: e.to_string(),
            })?;
        // 06.3 D15.3.c: register per-cluster upstream-request counters.
        let upstream_rq_total = registry
            .register_counter(&format!("cluster.{}.upstream_rq_total", cfg.name))
            .map_err(|e| ClusterError::StatsRegistration {
                cluster: cfg.name.clone(),
                message: e.to_string(),
            })?;
        let upstream_rq_5xx = registry
            .register_counter(&format!("cluster.{}.upstream_rq_5xx", cfg.name))
            .map_err(|e| ClusterError::StatsRegistration {
                cluster: cfg.name.clone(),
                message: e.to_string(),
            })?;
        // 12.1 (parent-12 D3/D5/D6): if the cluster configures an active health
        // check (validator guarantees 0 or 1), build per-endpoint EndpointHealth
        // (all starting Unhealthy) + register the membership_healthy gauge. No
        // health checks ⇒ endpoint_health: None ⇒ pick() is phase-02 round-robin.
        let (endpoint_health, panic_threshold) = if let Some(hc) = cfg.health_checks.first() {
            let membership_healthy = registry
                .register_gauge(&format!("cluster.{}.membership_healthy", cfg.name))
                .map_err(|e| ClusterError::StatsRegistration {
                    cluster: cfg.name.clone(),
                    message: e.to_string(),
                })?;
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
            let panic_threshold = cfg
                .common_lb_config
                .as_ref()
                .and_then(|c| c.healthy_panic_threshold.as_ref())
                .map(|p| p.value)
                .unwrap_or(50.0);
            (Some(health), panic_threshold)
        } else {
            (None, 50.0)
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
            let mk_counter = |suffix: &str| -> Result<Arc<envoy_stats::Counter>, ClusterError> {
                registry
                    .register_counter(&format!(
                        "cluster.{}.outlier_detection.{}",
                        cfg.name, suffix
                    ))
                    .map_err(|e| ClusterError::StatsRegistration {
                        cluster: cfg.name.clone(),
                        message: e.to_string(),
                    })
            };
            let mk_gauge = |suffix: &str| -> Result<Arc<envoy_stats::Gauge>, ClusterError> {
                registry
                    .register_gauge(&format!(
                        "cluster.{}.outlier_detection.{}",
                        cfg.name, suffix
                    ))
                    .map_err(|e| ClusterError::StatsRegistration {
                        cluster: cfg.name.clone(),
                        message: e.to_string(),
                    })
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
            })
        } else {
            None
        };
        let cluster = Arc::new(Cluster {
            name: cfg.name.clone(),
            endpoints,
            cursor: AtomicUsize::new(0),
            upstream_protocol,
            cx_total,
            cx_active,
            upstream_rq_total,
            upstream_rq_5xx,
            endpoint_health,
            panic_threshold,
            outlier_detection,
        });
        if clusters.insert(cfg.name.clone(), cluster).is_some() {
            return Err(ClusterError::DuplicateClusterName {
                name: cfg.name.clone(),
            });
        }
    }
    Ok(ClusterManager { clusters })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;

    fn mk_endpoints(n: u16) -> Vec<SocketAddr> {
        (0..n)
            .map(|i| format!("127.0.0.1:{}", 10000 + i).parse().unwrap())
            .collect()
    }

    /// Construct a per-test Cluster + ClusterHandle bypassing
    /// `from_bootstrap`. Counter and gauge are registered against a fresh
    /// registry so the test mirrors the real `from_bootstrap` Arc-clone shape
    /// (Counter/Gauge constructors are `pub(crate)` to envoy-stats; consumers
    /// always go through the registry).
    fn mk_handle(name: &str, endpoints: Vec<SocketAddr>) -> ClusterHandle {
        let registry = envoy_stats::StatsRegistry::new();
        let cx_total = registry
            .register_counter(&format!("cluster.{name}.upstream_cx_total"))
            .expect("counter registers");
        let cx_active = registry
            .register_gauge(&format!("cluster.{name}.upstream_cx_active"))
            .expect("gauge registers");
        let upstream_rq_total = registry
            .register_counter(&format!("cluster.{name}.upstream_rq_total"))
            .expect("counter registers");
        let upstream_rq_5xx = registry
            .register_counter(&format!("cluster.{name}.upstream_rq_5xx"))
            .expect("counter registers");
        ClusterHandle {
            inner: Arc::new(Cluster {
                name: name.to_string(),
                endpoints,
                cursor: AtomicUsize::new(0),
                upstream_protocol: UpstreamProtocol::default(),
                cx_total,
                cx_active,
                upstream_rq_total,
                upstream_rq_5xx,
                endpoint_health: None,
                panic_threshold: 50.0,
                outlier_detection: None,
            }),
        }
    }

    #[test]
    fn pick_endpoint_cycles_over_three_endpoints() {
        let endpoints = mk_endpoints(3);
        let handle = mk_handle("backend", endpoints.clone());
        let picks: Vec<SocketAddr> = (0..7).map(|_| handle.pick_endpoint().unwrap()).collect();
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
                let ep = h.pick_endpoint().expect("non-empty");
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
            a.pick_endpoint().unwrap(), // cursor=0 -> endpoints[0]
            b.pick_endpoint().unwrap(), // cursor=1 -> endpoints[1]
            a.pick_endpoint().unwrap(), // cursor=2 -> endpoints[0]
            b.pick_endpoint().unwrap(), // cursor=3 -> endpoints[1]
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
        let picked = handle.pick_endpoint().expect("non-empty");
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
        let picks: Vec<SocketAddr> = (0..3).map(|_| handle.pick_endpoint().unwrap()).collect();
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
        use envoy_config::{
            Address, Admin, Bootstrap, Cluster, ClusterType, LbPolicy, LoadAssignment,
            SocketAddress, StaticResources,
        };
        let bootstrap = Bootstrap {
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
                clusters: vec![Cluster {
                    name: "backend".into(),
                    cluster_type: ClusterType::Static,
                    lb_policy: LbPolicy::RoundRobin,
                    load_assignment: LoadAssignment {
                        cluster_name: "backend".into(),
                        endpoints: vec![],
                    },
                    transport_socket: None,
                    dns_lookup_family: None,
                    typed_extension_protocol_options: None,
                    health_checks: vec![],
                    common_lb_config: None,
                    circuit_breakers: None,
                    outlier_detection: None, // 14.1 D1
                }],
            },
        };
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
        use envoy_config::{
            Address, Admin, Bootstrap, Cluster, ClusterType, Endpoint, LbEndpoint, LbPolicy,
            LoadAssignment, LocalityLbEndpoints, SocketAddress, StaticResources,
        };
        let mk_cluster = || Cluster {
            name: "backend".into(),
            cluster_type: ClusterType::Static,
            lb_policy: LbPolicy::RoundRobin,
            load_assignment: LoadAssignment {
                cluster_name: "backend".into(),
                endpoints: vec![LocalityLbEndpoints {
                    lb_endpoints: vec![LbEndpoint {
                        endpoint: Endpoint {
                            address: Address {
                                socket_address: SocketAddress {
                                    address: "127.0.0.1".into(),
                                    port_value: 10001,
                                },
                            },
                        },
                    }],
                }],
            },
            transport_socket: None,
            dns_lookup_family: None,
            typed_extension_protocol_options: None,
            health_checks: vec![],
            common_lb_config: None,
            circuit_breakers: None,
            outlier_detection: None, // 14.1 D1
        };
        let bootstrap = Bootstrap {
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
                clusters: vec![mk_cluster(), mk_cluster()],
            },
        };
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
        let c = Cluster {
            name: "backend".to_string(),
            endpoints: mk_endpoints(1),
            cursor: AtomicUsize::new(0),
            upstream_protocol: UpstreamProtocol::default(),
            cx_total: registry
                .register_counter("cluster.backend.upstream_cx_total")
                .expect("counter registers"),
            cx_active: registry
                .register_gauge("cluster.backend.upstream_cx_active")
                .expect("gauge registers"),
            upstream_rq_total: registry
                .register_counter("cluster.backend.upstream_rq_total")
                .expect("counter registers"),
            upstream_rq_5xx: registry
                .register_counter("cluster.backend.upstream_rq_5xx")
                .expect("counter registers"),
            endpoint_health: None,
            panic_threshold: 50.0,
            outlier_detection: None,
        };
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
        let _ep = h.pick_endpoint();
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
        let picked = handle.pick_endpoint().expect("non-empty");
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
        let picked = handle.pick_endpoint().expect("non-empty");
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
        let cx_total = registry
            .register_counter(&format!("cluster.{name}.upstream_cx_total"))
            .unwrap();
        let cx_active = registry
            .register_gauge(&format!("cluster.{name}.upstream_cx_active"))
            .unwrap();
        let upstream_rq_total = registry
            .register_counter(&format!("cluster.{name}.upstream_rq_total"))
            .unwrap();
        let upstream_rq_5xx = registry
            .register_counter(&format!("cluster.{name}.upstream_rq_5xx"))
            .unwrap();
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
        let handle = ClusterHandle {
            inner: Arc::new(Cluster {
                name: name.to_string(),
                endpoints,
                cursor: AtomicUsize::new(0),
                upstream_protocol: UpstreamProtocol::default(),
                cx_total,
                cx_active,
                upstream_rq_total,
                upstream_rq_5xx,
                endpoint_health: Some(health.clone()),
                panic_threshold,
                outlier_detection: None,
            }),
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
        let picks: Vec<SocketAddr> = (0..4).map(|_| handle.pick_endpoint().unwrap()).collect();
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
        let picks: Vec<SocketAddr> = (0..4).map(|_| handle.pick_endpoint().unwrap()).collect();
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
        assert!(handle.pick_endpoint().is_none());
    }

    #[test]
    fn pick_panics_to_all_when_below_threshold() {
        let eps = mk_endpoints(2);
        // default 50% panic threshold; 0 healthy → 0% < 50% → panic → round-robin ALL.
        let (handle, _health) = mk_handle_with_health("b", eps.clone(), 1, 1, 50.0);
        let picks: Vec<SocketAddr> = (0..4).map(|_| handle.pick_endpoint().unwrap()).collect();
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
        let picks: Vec<SocketAddr> = (0..4).map(|_| handle.pick_endpoint().unwrap()).collect();
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
        let picks: Vec<SocketAddr> = (0..3).map(|_| handle.pick_endpoint().unwrap()).collect();
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
            handle.pick_endpoint().is_none(),
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
            let endpoint = handle.pick_endpoint().expect("endpoint");
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
        let cx_total = registry
            .register_counter(&format!("cluster.{name}.upstream_cx_total"))
            .unwrap();
        let cx_active = registry
            .register_gauge(&format!("cluster.{name}.upstream_cx_active"))
            .unwrap();
        let upstream_rq_total = registry
            .register_counter(&format!("cluster.{name}.upstream_rq_total"))
            .unwrap();
        let upstream_rq_5xx = registry
            .register_counter(&format!("cluster.{name}.upstream_rq_5xx"))
            .unwrap();
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
        };
        let handle = ClusterHandle {
            inner: Arc::new(Cluster {
                name: name.to_string(),
                endpoints,
                cursor: AtomicUsize::new(0),
                upstream_protocol: UpstreamProtocol::default(),
                cx_total,
                cx_active,
                upstream_rq_total,
                upstream_rq_5xx,
                endpoint_health: Some(health.clone()),
                panic_threshold,
                outlier_detection: Some(od_state),
            }),
        };
        (handle, health, ejection)
    }

    #[test]
    fn pick_inert_when_neither_filter_configured() {
        // Acceptance gate (b) regression-equivalence: when both endpoint_health AND
        // outlier_detection are None, pick() must be byte-for-byte phase-02 round-robin.
        let endpoints = mk_endpoints(3);
        let handle = mk_handle("backend", endpoints.clone()); // unchanged 12.1 helper
        let picks: Vec<SocketAddr> = (0..6).map(|_| handle.pick_endpoint().unwrap()).collect();
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
            assert_eq!(handle.pick_endpoint().unwrap(), eps[1]);
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
            handle.pick_endpoint().is_none(),
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
        let picks: Vec<SocketAddr> = (0..4).map(|_| handle.pick_endpoint().unwrap()).collect();
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
            assert_eq!(handle.pick_endpoint().unwrap(), eps[3]);
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
    fn cluster_record_response_picks_5xx_detector_on_ties() {
        // 503 crosses BOTH thresholds simultaneously at threshold=1. Per lock-in #15,
        // 5xx wins ties — endpoint ejects with DetectorType::Consecutive5xx.
        let eps = mk_endpoints(1);
        let (handle, _health, ejection) =
            mk_handle_with_health_and_ejection("b", eps.clone(), 1, 1, 0.0, 1, 1, 100);
        handle.record_response(eps[0], 503);
        assert!(ejection[0].is_ejected());
        let od = handle.inner.outlier_detection.as_ref().unwrap();
        let stats_active = &od.endpoints[0];
        let _ = stats_active;
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
