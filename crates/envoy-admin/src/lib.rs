#![forbid(unsafe_code)]

//! envoy-admin — HCM-style HTTP/1.1 admin listener serving the project's
//! built-in admin endpoints (`/ready`, `/stats`, `/stats/prometheus` in
//! 06.1; extended in later phases). Sole-dep-owner of admin-listener wiring;
//! depends on envoy-http1 for request/response value types and HCM-style
//! request handling. HTTP/1.1 only — H2 admin defers indefinitely (parent-06
//! SPEC §6 rule 3 + §4 deferred non-goal).

pub mod config;
pub mod endpoint;
mod error;
pub mod handler;

pub use config::AdminConfig;
pub use endpoint::AdminEndpoint;
pub use envoy_listener::{DrainStage, DrainState};
pub use error::AdminError;
pub use handler::{AdminHandler, serve};
