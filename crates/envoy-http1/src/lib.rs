#![forbid(unsafe_code)]
//! HTTP/1.1 codec + connection manager (HCM) for envoy-rust.
//!
//! This crate is the workspace's sole runtime owner of the `httparse`
//! dependency (per phase-04 parent SPEC §3 cross-sub-phase rule 1). All
//! HTTP/1.1 request parsing in runtime code goes through `Http1Codec`;
//! response wire-format generation goes through `Http1Response`.
//!
//! envoy-bin's admin endpoint historically imported `httparse` directly
//! (introduced in phase 01). The architectural posture from 04.1 onwards
//! is that admin code routes through this crate's public types when admin
//! is next touched; 04.1 does not perform an in-flight refactor of admin.

pub mod client;
pub mod codec;
pub mod date;
// 110.1: gRPC-aware local replies. DELIBERATELY `pub(crate)` — see the module
// doc. Nothing outside this crate may reach it, because `envoy-http2` shares
// this crate's `build_response` and must stay untransformed (CF-110-1).
mod error;
pub(crate) mod grpc;
pub mod hcm;
pub mod headers;
pub mod pool; // 13.1 NEW (Task 3): per-cluster H1 connection pool.
pub mod rds_watcher; // 26 NEW (Task 3): the 5th periodic-background primitive.
pub mod response;
pub mod router; // 04.3 NEW (Task 8)
#[cfg(all(feature = "uring", target_os = "linux"))]
pub mod uring; // EXPERIMENTAL io_uring data-plane worker (perf prototype).

pub use client::{Client, ClientStream};
pub use codec::{Http1Codec, HttpVersion, Request};
pub use error::Http1Error;
pub use hcm::{BuildOutcome, HCM, HCMConfig, HCMStats, build_response};
pub use pool::{H1Pool, H1PoolManager, PoolError, PoolGuard}; // 13.1 NEW (Task 3)
pub use rds_watcher::{RdsCounters, RdsWatcher, WatchTarget}; // 26 NEW (Task 3/4)
pub use response::{Http1Response, Response};
pub use router::RouterError; // 04.3 NEW (Task 8)
