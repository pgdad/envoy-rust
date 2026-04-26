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
    #[allow(dead_code)]
    pub(crate) name: String,
    pub(crate) endpoints: Vec<SocketAddr>,
    pub(crate) cursor: AtomicUsize,
}

impl Cluster {
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
/// rejects a malformed address.
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
}

/// Constructs a `ClusterManager` from a validated `Bootstrap`. The caller
/// should have already run `envoy_config::parse_bootstrap`, but this function
/// validates its own preconditions for library robustness.
pub fn from_bootstrap(bootstrap: &envoy_config::Bootstrap) -> Result<ClusterManager, ClusterError> {
    let mut clusters: HashMap<String, Arc<Cluster>> = HashMap::new();
    for cfg in &bootstrap.static_resources.clusters {
        // envoy-config enforces cluster_type == Static, lb_policy == RoundRobin,
        // load_assignment.cluster_name == cfg.name, and total endpoints ≥ 1 at
        // parse time. We don't re-check those here; we do re-check emptiness
        // and duplicate names as defense-in-depth, and we parse each address
        // (which envoy-config does NOT do).
        let mut endpoints: Vec<SocketAddr> = Vec::new();
        for locality in &cfg.load_assignment.endpoints {
            for lbe in &locality.lb_endpoints {
                let sa = &lbe.endpoint.address.socket_address;
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
        }
        if endpoints.is_empty() {
            return Err(ClusterError::EmptyCluster {
                name: cfg.name.clone(),
            });
        }
        let cluster = Arc::new(Cluster {
            name: cfg.name.clone(),
            endpoints,
            cursor: AtomicUsize::new(0),
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

    #[test]
    fn from_bootstrap_builds_single_endpoint_cluster() {
        let bootstrap = envoy_config::parse_bootstrap(SINGLE_ENDPOINT_YAML).expect("valid");
        let mgr = crate::from_bootstrap(&bootstrap).expect("construct");
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

    #[test]
    fn from_bootstrap_builds_three_endpoint_cluster() {
        let bootstrap = envoy_config::parse_bootstrap(THREE_ENDPOINT_YAML).expect("valid");
        let mgr = crate::from_bootstrap(&bootstrap).expect("construct");
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

    #[test]
    fn from_bootstrap_rejects_empty_cluster() {
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
                }],
            },
        };
        let err = crate::from_bootstrap(&bootstrap).expect_err("must reject");
        assert!(
            matches!(err, ClusterError::EmptyCluster { ref name } if name == "backend"),
            "got {err:?}",
        );
    }

    #[test]
    fn from_bootstrap_rejects_duplicate_cluster_name() {
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
            }),
            static_resources: StaticResources {
                listeners: vec![],
                clusters: vec![mk_cluster(), mk_cluster()],
            },
        };
        let err = crate::from_bootstrap(&bootstrap).expect_err("must reject");
        assert!(
            matches!(err, ClusterError::DuplicateClusterName { ref name } if name == "backend"),
            "got {err:?}",
        );
    }

    #[test]
    fn from_bootstrap_rejects_malformed_endpoint_address() {
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
        let err = crate::from_bootstrap(&bootstrap).expect_err("must reject");
        assert!(
            matches!(
                err,
                ClusterError::EndpointParse { ref cluster, ref addr, .. }
                    if cluster == "backend" && addr == "not-a-host:10001"
            ),
            "got {err:?}",
        );
    }
}
