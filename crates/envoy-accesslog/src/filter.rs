//! Phase 70: the access-log FILTER predicate — the per-record emission gate
//! compiled from `envoy_config::AccessLogFilter`. This phase implements the
//! single `status_code_filter` variant.

/// The comparison operator (`ComparisonFilter.Op`): exactly `{EQ, GE, LE}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterOp {
    Eq,
    Ge,
    Le,
}

/// A `status_code_filter` comparison: `op(status, threshold)`. `threshold` is
/// `RuntimeUInt32.default_value` (the `runtime_key` override is RTDS-inert).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusCodeComparison {
    pub op: FilterOp,
    pub threshold: u32,
}

/// The compiled access-log filter. `None`-carrying sinks skip this type
/// entirely (they log every record); a `Some(LogFilter)` gates emission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogFilter {
    StatusCode(StatusCodeComparison),
}

impl LogFilter {
    /// Returns `true` iff a record with the given final response `status`
    /// should be emitted. Comparison is widened to `u32` (lossless; status is
    /// always in `u16` range).
    pub fn should_log(&self, status: u16) -> bool {
        match self {
            LogFilter::StatusCode(c) => {
                let s = status as u32;
                match c.op {
                    FilterOp::Eq => s == c.threshold,
                    FilterOp::Ge => s >= c.threshold,
                    FilterOp::Le => s <= c.threshold,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ge(t: u32) -> LogFilter {
        LogFilter::StatusCode(StatusCodeComparison {
            op: FilterOp::Ge,
            threshold: t,
        })
    }
    fn eq(t: u32) -> LogFilter {
        LogFilter::StatusCode(StatusCodeComparison {
            op: FilterOp::Eq,
            threshold: t,
        })
    }
    fn le(t: u32) -> LogFilter {
        LogFilter::StatusCode(StatusCodeComparison {
            op: FilterOp::Le,
            threshold: t,
        })
    }

    #[test]
    fn ge_500_boundary() {
        assert!(!ge(500).should_log(499));
        assert!(ge(500).should_log(500));
        assert!(ge(500).should_log(503));
    }

    #[test]
    fn eq_404_boundary() {
        assert!(!eq(404).should_log(403));
        assert!(eq(404).should_log(404));
        assert!(!eq(404).should_log(405));
    }

    #[test]
    fn le_200_boundary() {
        assert!(le(200).should_log(200));
        assert!(!le(200).should_log(201));
        assert!(le(200).should_log(100));
    }
}
