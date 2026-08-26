#![forbid(unsafe_code)]

//! envoy-http2 — HTTP/2 cleartext (H2C prior-knowledge) codec wrapper.
//!
//! Owns the workspace's only direct dependency on the `h2` crate. All other
//! workspace crates import `envoy_http2::*` types instead of `h2::*` types.
//! See parent-phase-05 SPEC §3 cross-sub-phase architectural rule 1 + ADR-0022
//! (parent-05 split decision).
//!
//! Module decomposition (lands across phase 05.2 Tasks 5–9):
//!   - `error`    — typed-error enum (Task 5).
//!   - `request`  — H2-RecvStream → envoy-Request value translator (Task 6).
//!   - `response` — envoy-Response → H2-SendStream emitter (Task 7).
//!   - `codec`    — Http2ProtocolOptions → h2::server::Builder configurer (Task 8).
//!   - `hcm`      — ConnectionHandler impl for downstream H2C listeners (Task 9).
//!
//! 05.3-projected (NOT in 05.2):
//!   - `client`   — upstream H2C origination (envoy_http2::Client + ClientStream).

pub mod client;
pub mod codec;
mod error;
pub mod grpc;
pub mod hcm;
pub mod pool;
pub mod request;
pub mod response;

/// H2-forbidden hop-by-hop headers per RFC 7540 §8.1.2.2 + RFC 9113 §8.2.2.
/// Consolidated per Task 2 review I2: was duplicated across `client.rs` and
/// `response.rs`; now a single canonical crate-level constant. Both modules
/// import this via `crate::H2_FORBIDDEN_HOP_BY_HOP`.
///
/// Stripped defensively at codec edges (the h2 crate also rejects these names,
/// but our posture per parent SPEC §3 architectural rule 4 is to strip first).
pub(crate) const H2_FORBIDDEN_HOP_BY_HOP: &[&str] = &[
    "connection",
    "transfer-encoding",
    "keep-alive",
    "upgrade",
    "proxy-connection",
];

/// Phase 111: an upstream HTTP/2 response's TRAILER block, in wire order —
/// `None` when the upstream sent none, which is the overwhelmingly common case.
/// `Some(vec![])` is never produced.
///
/// The block rides ALONGSIDE `envoy_http1::Response` rather than as a field on
/// it (D-PLAN-2): that type is shared across four crates with 42 struct-literal
/// sites and derives `PartialEq`/`Eq`, so a fifth field would both fan out an
/// `E0063` across all of them and silently redefine every whole-`Response`
/// equality assertion — for a value only the HTTP/2 path can ever populate.
///
/// `Vec<(String, String)>` rather than `http::HeaderMap` so it matches the
/// shape `Response.headers` already uses, preserving duplicate names and wire
/// order for free.
///
/// This alias exists because the nested form
/// `Result<(Response, Option<Vec<(String, String)>>), String>` trips
/// `clippy::type_complexity` at the retry loop's `AcquireOutcome::Sent`.
pub type TrailerBlock = Option<Vec<(String, String)>>;

pub use client::{Client, ClientStream};
pub use codec::build_h2_server;
pub use error::Http2Error;
pub use hcm::{HCM, HCMConfig};
pub use pool::H2PoolManager;
pub use request::http_to_envoy_request;
pub use response::{build_http_response, send_envoy_response};
