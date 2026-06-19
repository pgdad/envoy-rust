#![forbid(unsafe_code)]

//! Phase 02.1 static-cluster surface for envoy-rust. Owns the `ClusterManager`
//! entrypoint plus the `Cluster` / `ClusterHandle` data model and the
//! round-robin load-balancer cursor.
//!
//! The cluster data model + LB cursor stay at the synchronous data-model seam; async I/O
//! lives downstream in `envoy-tcp` (sub-phase 02.2) and later phases. See
//! `docs/envoy-rust/phases/02.1-config-cluster/SPEC.md` §§D1, §6 signpost 10. The 14.2 D7
//! `outlier` module is the lone exception: it owns a periodic-background sweeper task
//! (`tokio::spawn` + `CancellationToken`), mirroring the H1/H2 pool idle sweepers + the
//! `envoy-health` scheduler — the FOURTH periodic-background primitive.

pub mod budget;
mod cluster;
mod eds_reload;
mod ejection;
mod health;
mod outlier;
pub mod xds_watch;

pub use budget::{BudgetAcquisition, BudgetState, RequestBudgetGuard, RetryBudgetGuard};
pub use cluster::{
    Cluster, ClusterError, ClusterHandle, ClusterManager, ConnGaugeGuard, UpstreamProtocol,
    from_bootstrap,
};
pub use eds_reload::build_eds_watch_targets;
pub use ejection::{DetectorType, EjectionDecision, EndpointEjection, EndpointEjectionStats};
pub use health::EndpointHealth;
pub use outlier::{OutlierEjectionSweeper, OutlierManager};
pub use xds_watch::{WatchTarget, XdsFileWatcher};
