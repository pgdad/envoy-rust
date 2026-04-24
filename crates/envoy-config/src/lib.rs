#![forbid(unsafe_code)]

//! Phase 01 config surface for envoy-rust. Owns the `Bootstrap` type tree and
//! the `parse_bootstrap` entrypoint (fleshed out in Task 4). Phase 00's inline
//! parser in `crates/envoy-bin/src/config.rs` is superseded by this crate.
//!
//! See `docs/envoy-rust/DECISIONS.md` ADR-0008 for the extraction rationale.

pub mod bootstrap;
