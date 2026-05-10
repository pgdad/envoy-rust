//! envoy-stats typed-error enum (lands at Task 4).

// Placeholder; Task 4 ships the real surface. The `pub use error::StatsError;`
// re-export in `lib.rs` is satisfied by Task 4's contents.
#[derive(Debug, thiserror::Error)]
pub enum StatsError {}
