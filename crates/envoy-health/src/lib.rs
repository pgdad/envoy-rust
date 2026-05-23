#![forbid(unsafe_code)]

//! Phase 12.2 (parent-12 D4): active HTTP health-check probe tasks.
//!
//! The first periodic-background primitive in the project. `Scheduler::spawn`
//! walks every cluster carrying `health_checks` and spawns one
//! `tokio::spawn`ed `probe_loop` per (cluster, endpoint) pair. Each loop
//! ticks every `interval`, issues a `GET <path>` via `envoy_http1::Client`
//! to the endpoint, evaluates the response status against `expected_statuses`,
//! and calls `EndpointHealth::record_success` / `record_failure` (the 12.1
//! state machine) — driving the `membership_healthy` gauge + the 3
//! `cluster.<n>.health_check.{attempt,success,failure}` counters this crate
//! registers.
//!
//! Single-writer contract per (cluster, endpoint): `Scheduler::spawn`
//! produces EXACTLY ONE `probe_loop` task per pair, and the `Arc<EndpointHealth>`
//! is MOVED into that task. No other code path in envoy-rust calls `record_*`
//! on it. The 12.1 `EndpointHealth` Relaxed-ordering soundness rests on this
//! contract (12.1 REVIEW M2; closed at this crate's API boundary).
//!
//! Cycle-free dependency graph: `envoy-health → envoy-http1 → envoy-cluster`
//! plus `envoy-health → envoy-cluster` (clean DAG; verified at PLAN-write).
//! `envoy-cluster` stays a leaf for `pick()`; `envoy-http1` stays a router-side
//! consumer; `envoy-health` sits above both as the active-HC driver.

pub mod error;
mod probe;
mod scheduler;

pub use error::HealthError;
pub use scheduler::Scheduler;
