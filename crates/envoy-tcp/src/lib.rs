#![forbid(unsafe_code)]

//! Phase 02.2 TCP proxy filter for envoy-rust. Implements
//! `envoy_listener::ConnectionHandler` for `TcpProxy`. Public surface is
//! populated by Task 8 of `docs/envoy-rust/phases/02.2-listener-tcp-proxy/PLAN.md`.
//!
//! Half-close posture follows ADR-0016 (Envoy v1.33.0 default
//! `enable_half_close: false`): `tokio::io::copy` runs in both directions
//! and EOF on either side propagates via drop of the write half.
