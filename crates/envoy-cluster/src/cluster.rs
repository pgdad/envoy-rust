//! Cluster data model + round-robin LB — populated in Tasks 6 and 7.
//!
//! The placeholder items below let `lib.rs` re-export a stable set of names
//! while the fleshed-out types land. Each placeholder is replaced wholesale
//! by the named task.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

/// Placeholder — see Task 6 for the real implementation.
#[allow(dead_code)]
pub struct Cluster {
    pub(crate) name: String,
    pub(crate) endpoints: Vec<SocketAddr>,
    pub(crate) cursor: AtomicUsize,
}

/// Placeholder — see Task 6 for the real implementation.
#[derive(Clone)]
#[allow(dead_code)]
pub struct ClusterHandle {
    pub(crate) inner: Arc<Cluster>,
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
