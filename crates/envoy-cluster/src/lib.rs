#![forbid(unsafe_code)]

//! Phase 02.1 static-cluster surface for envoy-rust. Owns the `ClusterManager`
//! entrypoint plus the `Cluster` / `ClusterHandle` data model and the
//! round-robin load-balancer cursor.
//!
//! `envoy-cluster` is synchronous: no `async fn`, no `Future`, no
//! `tokio::spawn`. The cluster layer stays at the data-model seam; async I/O
//! lives downstream in `envoy-tcp` (sub-phase 02.2) and later phases. See
//! `docs/envoy-rust/phases/02.1-config-cluster/SPEC.md` §§D1, §6 signpost 10.

mod cluster;

pub use cluster::{
    Cluster, ClusterError, ClusterHandle, ClusterManager, UpstreamProtocol, from_bootstrap,
};
