#![forbid(unsafe_code)]

//! envoy-rust h2spec conformance runner.
//!
//! The crate is a test-only workspace member; the lib is empty (the runner
//! lives in `tests/h2spec_runner.rs`). Per phase 05.2 SPEC §3 D7.
