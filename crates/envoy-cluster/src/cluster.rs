//! Cluster data model + round-robin LB. See SPEC §D1.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

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

    /// Picks the next endpoint in round-robin order. `Relaxed` ordering is
    /// sufficient because no other observation depends on a happens-before
    /// relationship with the cursor update (SPEC §6 signpost 3).
    fn pick(&self) -> Option<SocketAddr> {
        if self.endpoints.is_empty() {
            // `from_bootstrap` rejects empty clusters; this is defense-in-depth.
            return None;
        }
        let i = self.cursor.fetch_add(1, Ordering::Relaxed);
        Some(self.endpoints[i % self.endpoints.len()])
    }
}

/// A handle to a `Cluster` that hands out endpoints via round-robin. Cheaply
/// cloneable (`Arc`-internal); clones share the same cursor.
#[derive(Clone, Debug)]
pub struct ClusterHandle {
    pub(crate) inner: Arc<Cluster>,
}

impl ClusterHandle {
    /// Returns the next endpoint in round-robin order.
    ///
    /// Returns `None` only when the cluster is empty — which `from_bootstrap`
    /// rejects at construction time, so this is effectively infallible in
    /// phase 02. `Option<_>` is preserved for phase-06+ health checking.
    pub fn pick_endpoint(&self) -> Option<SocketAddr> {
        self.inner.pick()
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
        let cluster = Arc::new(Cluster {
            name: cfg.name.clone(),
            endpoints,
            cursor: AtomicUsize::new(0),
            upstream_protocol,
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

    fn mk_handle(name: &str, endpoints: Vec<SocketAddr>) -> ClusterHandle {
        ClusterHandle {
            inner: Arc::new(Cluster {
                name: name.to_string(),
                endpoints,
                cursor: AtomicUsize::new(0),
                upstream_protocol: UpstreamProtocol::default(),
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
        let mgr = crate::from_bootstrap(&bootstrap).await.expect("construct");
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
        let mgr = crate::from_bootstrap(&bootstrap).await.expect("construct");
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
                }],
            },
        };
        let err = crate::from_bootstrap(&bootstrap)
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
        let err = crate::from_bootstrap(&bootstrap)
            .await
            .expect_err("must reject");
        assert!(
            matches!(err, ClusterError::DuplicateClusterName { ref name } if name == "backend"),
            "got {err:?}",
        );
    }

    #[test]
    fn cluster_name_returns_configured_name() {
        let c = Cluster {
            name: "backend".to_string(),
            endpoints: mk_endpoints(1),
            cursor: AtomicUsize::new(0),
            upstream_protocol: UpstreamProtocol::default(),
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
        let err = crate::from_bootstrap(&bootstrap)
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
        let mgr = crate::from_bootstrap(&bootstrap)
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
        let mgr = crate::from_bootstrap(&bootstrap)
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
        let err = crate::from_bootstrap(&bootstrap)
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
        from_bootstrap(&bootstrap).await.expect("from_bootstrap")
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
}
