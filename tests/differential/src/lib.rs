#![forbid(unsafe_code)]

//! Differential test harness for envoy-rust. See
//! `docs/envoy-rust/phases/00-bootstrap/SPEC.md` §4 (D4) and
//! `docs/envoy-rust/BEHAVIOR_CONTRACT.md` for the contract this harness enforces.

// Public surface is populated by later tasks. This crate compiles on its own
// so the workspace-level green-build gate (D-3.6) holds after Task 4.
