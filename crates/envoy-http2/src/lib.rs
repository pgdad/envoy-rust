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

mod error;

pub use error::Http2Error;
