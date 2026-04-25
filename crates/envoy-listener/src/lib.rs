#![forbid(unsafe_code)]

//! Phase 02.2 listener surface for envoy-rust. Owns TCP listener binding,
//! the accept loop, the `ConnectionHandler` trait that filters implement, and
//! a shutdown-gated graceful drain. Public surface is populated by Tasks 5 and
//! 6 of `docs/envoy-rust/phases/02.2-listener-tcp-proxy/PLAN.md`.
//!
//! `BoxFuture` and `ConnectionHandler` are defined in-crate to avoid pulling
//! `futures` or `async-trait` (neither on the D-3.2 permitted-foundations
//! list); see SPEC §6 signposts 2 and 3.
