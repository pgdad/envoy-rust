#![forbid(unsafe_code)]

//! envoy-stats — counter / gauge primitives + hierarchical stats registry +
//! Prometheus text-exposition emitter.
//!
//! Owns no workspace dep on any stats-specific crate (no `prometheus`, no
//! `metrics`, etc.); primitives are hand-rolled atop `std` atomics. Other
//! workspace crates (envoy-listener, envoy-cluster, envoy-http1, envoy-http2,
//! envoy-admin) consume `envoy_stats::*` via `Arc<StatsRegistry>` injection.
//! See parent-phase-06 SPEC §6 architectural rule 1 + ADR-0029.

pub mod counter;
pub mod gauge;
pub mod registry;
pub mod prometheus;
mod error;

pub use counter::Counter;
pub use gauge::Gauge;
pub use registry::{StatHandle, StatsRegistry};
pub use error::StatsError;
