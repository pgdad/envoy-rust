//! `HealthError` — surfaced by `Scheduler::spawn` when stats registration or
//! duration parsing fails. Per-cluster context (`cluster: String`) matches
//! the envoy-cluster / envoy-config error discipline.

use thiserror::Error;

/// Phase-12.2 health-scheduler error surface.
#[derive(Debug, Error)]
pub enum HealthError {
    /// Stats registration failed for one of the 3 per-cluster counters.
    #[error("registering health_check stats for cluster '{cluster}': {message}")]
    StatsRegistration { cluster: String, message: String },
    /// `parse_duration` rejected `interval` or `timeout` (the 12.1 D2
    /// validator already rejects these at parse, so this is defense-in-depth).
    #[error("parsing {field} for cluster '{cluster}': {message}")]
    InvalidDuration {
        cluster: String,
        field: &'static str,
        message: String,
    },
}
