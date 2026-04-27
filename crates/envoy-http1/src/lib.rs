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

pub mod codec;
pub mod date;
mod error;
pub mod hcm;
pub mod headers;
pub mod response;

pub use error::Http1Error;
// Tasks 8–10 will add re-exports for codec/response/hcm public types.
