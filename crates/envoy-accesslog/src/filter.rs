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
    /// Phase 71: emit a record iff its response-flag token ∈ `flags`. An EMPTY
    /// `flags` matches any record that HAS a flag set (ADR-0145 PV-6).
    ResponseFlag {
        flags: Vec<String>,
    },
}

impl LogFilter {
    /// Returns `true` iff a record with the given final response `status` and
    /// `response_flags` token should be emitted. The `StatusCode` arm ignores
    /// `response_flags`; the `ResponseFlag` arm ignores `status`. The status
    /// comparison is widened to `u32` (lossless; status is always in `u16` range).
    pub fn should_log(&self, status: u16, response_flags: &str) -> bool {
        match self {
            LogFilter::StatusCode(c) => {
                let s = status as u32;
                match c.op {
                    FilterOp::Eq => s == c.threshold,
                    FilterOp::Ge => s >= c.threshold,
                    FilterOp::Le => s <= c.threshold,
                }
            }
            LogFilter::ResponseFlag { flags } => {
                if flags.is_empty() {
                    // MEASURED (ADR-0145 PV-6): an empty `flags` matches any
                    // record that HAS a response flag set. "-" is the no-flag
                    // sentinel; envoy-rust renders a single token otherwise.
                    response_flags != "-"
                } else {
                    flags.iter().any(|f| f == response_flags)
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

    fn rf(flags: &[&str]) -> LogFilter {
        LogFilter::ResponseFlag {
            flags: flags.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn ge_500_boundary() {
        assert!(!ge(500).should_log(499, "-"));
        assert!(ge(500).should_log(500, "-"));
        assert!(ge(500).should_log(503, "-"));
    }

    #[test]
    fn eq_404_boundary() {
        assert!(!eq(404).should_log(403, "-"));
        assert!(eq(404).should_log(404, "NR"));
        assert!(!eq(404).should_log(405, "-"));
    }

    #[test]
    fn le_200_boundary() {
        assert!(le(200).should_log(200, "-"));
        assert!(!le(200).should_log(201, "-"));
        assert!(le(200).should_log(100, "-"));
    }

    #[test]
    fn response_flag_membership() {
        // The ResponseFlag arm ignores `status`; pass any value.
        assert!(rf(&["NR"]).should_log(404, "NR"));
        assert!(rf(&["UH", "NR"]).should_log(404, "NR"));
        assert!(!rf(&["UH"]).should_log(404, "NR"));
    }

    #[test]
    fn response_flag_dash_sentinel_never_matches_nonempty() {
        // "-" ∉ the 29-token set, so a non-empty `flags` never matches it.
        assert!(!rf(&["NR"]).should_log(503, "-"));
        assert!(!rf(&["UH", "UF"]).should_log(503, "-"));
    }

    #[test]
    fn response_flag_empty_matches_any_flag_set() {
        // MEASURED (ADR-0145 PV-6): empty `flags` keeps records WITH a flag,
        // drops the "-" no-flag sentinel.
        assert!(rf(&[]).should_log(404, "NR"));
        assert!(rf(&[]).should_log(503, "UF"));
        assert!(!rf(&[]).should_log(503, "-"));
    }

    #[test]
    fn response_flag_inert_token_never_matches_produced() {
        // A config may carry an inert token (`DI`); envoy-rust never renders it.
        assert!(!rf(&["DI"]).should_log(404, "NR"));
        assert!(!rf(&["DI"]).should_log(503, "-"));
    }

    #[test]
    fn status_code_arm_ignores_response_flags() {
        let f = LogFilter::StatusCode(StatusCodeComparison {
            op: FilterOp::Ge,
            threshold: 500,
        });
        assert!(f.should_log(503, "-"));
        assert!(f.should_log(503, "NR"));
        assert!(!f.should_log(200, "NR"));
    }
}
