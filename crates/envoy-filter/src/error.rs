//! Typed errors emitted by the filter framework.
//!
//! Most parse-time validation lives in `envoy_config::validate_http_filters`
//! (Task 4). `FilterError` exists for the residual cases where the
//! framework's `build_from_config` arm asserts an invariant the
//! validator would also catch (defense-in-depth) plus future runtime
//! errors (e.g., `StopAndSend` invariants).

use std::sync::Arc;

use envoy_stats::{Counter, StatsRegistry};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FilterError {
    #[error("filter chain is empty (must contain at least Router)")]
    EmptyChain,

    #[error("filter chain references unsupported filter type at position {position}: {name}")]
    UnsupportedFilterType { position: usize, name: String },

    /// 09: filter config rejected at `build_from_config` time (defense-in-depth
    /// — the envoy-config validator at `validate_local_rate_limit_config` is
    /// the primary gate).
    #[error("Filter config invalid: {message}")]
    InvalidConfig { message: String },
}

/// Register a stat counter, mapping a registry rejection to the canonical
/// `FilterError::InvalidConfig { message: "StatsRegistry: {e}" }` (byte-exact
/// across every stats-bearing filter's `build_from_config`).
pub(crate) fn register_counter(
    registry: &StatsRegistry,
    name: &str,
) -> Result<Arc<Counter>, FilterError> {
    registry
        .register_counter(name)
        .map_err(|e| FilterError::InvalidConfig {
            message: format!("StatsRegistry: {e}"),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_empty_chain_is_human_readable() {
        let s = format!("{}", FilterError::EmptyChain);
        assert_eq!(s, "filter chain is empty (must contain at least Router)");
    }

    #[test]
    fn display_unsupported_filter_type_includes_position_and_name() {
        let e = FilterError::UnsupportedFilterType {
            position: 1,
            name: "envoy.filters.http.cors".to_string(),
        };
        let s = format!("{e}");
        assert!(s.contains("position 1"));
        assert!(s.contains("envoy.filters.http.cors"));
    }

    /// Static assertion that `FilterError` is `Send + Sync + 'static`.
    ///
    /// Required so the error can flow through tokio task boundaries.
    #[test]
    fn filter_error_is_send_sync_static() {
        fn _assert_send_sync<T: Send + Sync + 'static>() {}
        _assert_send_sync::<FilterError>();
    }
}
