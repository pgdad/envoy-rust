//! Cluster data model + round-robin LB. See SPEC §D1.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A configured upstream cluster. Owns the static endpoint list and the
/// round-robin `AtomicUsize` cursor. Constructed by `from_bootstrap` only;
/// external code works through `ClusterHandle`.
#[derive(Debug)]
pub struct Cluster {
    #[allow(dead_code)] // used in Task 7 (ClusterManager)
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

/// Placeholder — see Task 7 for the real implementation.
#[allow(dead_code)]
pub struct ClusterManager {
    pub(crate) clusters: std::collections::HashMap<String, Arc<Cluster>>,
}

/// Placeholder — see Task 7 for the real implementation.
#[derive(Debug, thiserror::Error)]
pub enum ClusterError {
    #[error("placeholder")]
    Placeholder,
}

/// Placeholder — see Task 7 for the real implementation.
pub fn from_bootstrap(
    _bootstrap: &envoy_config::Bootstrap,
) -> Result<ClusterManager, ClusterError> {
    Err(ClusterError::Placeholder)
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
}
