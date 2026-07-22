//! Phase 70: the access-log FILTER predicate — the per-record emission gate
//! compiled from `envoy_config::AccessLogFilter`. Phase 70 added
//! `status_code_filter`; phase 71 added `response_flag_filter`; phase 72 adds
//! `header_filter` (the `LogFilter::Header` arm).

use std::sync::Arc;

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

/// Phase 72 (ADR-0150): the runtime seam for the `header_filter` arm. This crate
/// CANNOT depend on `envoy-config` — `envoy-config` already depends on THIS crate
/// (the compiled-config posture, e.g. `envoy_accesslog::parse_format`), so the
/// reverse edge is a dependency CYCLE. The header-match engine is therefore
/// injected as a trait object: `envoy-config` impls `HeaderMatch` for its
/// `HeaderMatcher` (reusing the phase-04.2 7-mode engine VERBATIM), and the HCM
/// compile step in `envoy-http1` (which depends on both crates) boxes it into
/// `LogFilter::Header`. `Send + Sync` because sinks cross async await points.
pub trait HeaderMatch: std::fmt::Debug + Send + Sync {
    /// Returns `true` iff this matcher matches the given request-header set.
    fn matches(&self, headers: &[(String, String)]) -> bool;
}

/// The compiled access-log filter. `None`-carrying sinks skip this type
/// entirely (they log every record); a `Some(LogFilter)` gates emission.
///
/// `Eq`/`PartialEq` were dropped in phase 72: the `Header` arm carries an
/// `Arc<dyn HeaderMatch>` (not comparable), and nothing compares `LogFilter`
/// values (grep-confirmed — `FileSink`, the sole container, derives only `Debug`).
#[derive(Debug, Clone)]
pub enum LogFilter {
    StatusCode(StatusCodeComparison),
    /// Phase 71: emit a record iff its response-flag token ∈ `flags`. An EMPTY
    /// `flags` matches any record that HAS a flag set (ADR-0145 PV-6).
    ResponseFlag {
        flags: Vec<String>,
    },
    /// Phase 72: emit a record iff a named request header matches `matcher`. The
    /// matcher is an injected `HeaderMatch` trait object (ADR-0150 cycle seam);
    /// `envoy-config` provides the impl over its `HeaderMatcher`.
    Header {
        matcher: Arc<dyn HeaderMatch>,
    },
    /// Phase 73: emit iff ALL nested child predicates match (`and_filter`).
    /// Recurses through `Vec<LogFilter>` (NO `Box`). Introduces no `Eq`/`PartialEq`
    /// and no `envoy-config` dep (ADR-0150 holds).
    And(Vec<LogFilter>),
    /// Phase 73: emit iff ANY nested child predicate matches (`or_filter`).
    Or(Vec<LogFilter>),
}

impl LogFilter {
    /// Returns `true` iff a record with the given final response `status`,
    /// `response_flags` token, and request `headers` should be emitted. The
    /// `StatusCode` arm reads only `status`; the `ResponseFlag` arm only
    /// `response_flags`; the `Header` arm only `headers`. The status comparison
    /// is widened to `u32` (lossless; status is always in `u16` range).
    pub fn should_log(
        &self,
        status: u16,
        response_flags: &str,
        headers: &[(String, String)],
    ) -> bool {
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
            // Phase 72: gate on whether the named request header matches. Present-
            // mismatch AND absent both drop (the engine's own semantics); PV-4's
            // `mode_result ^ invert_match` is preserved because the injected impl
            // calls `HeaderMatcher::matches` verbatim.
            LogFilter::Header { matcher } => matcher.matches(headers),
            // Phase 73: boolean composition over the nested predicates. The
            // config validator's `min_items = 2` makes the empty-vec edge
            // (all→true, any→false) unreachable at runtime; the semantics are
            // pinned in-process regardless.
            LogFilter::And(filters) => filters
                .iter()
                .all(|f| f.should_log(status, response_flags, headers)),
            LogFilter::Or(filters) => filters
                .iter()
                .any(|f| f.should_log(status, response_flags, headers)),
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
        assert!(!ge(500).should_log(499, "-", &[]));
        assert!(ge(500).should_log(500, "-", &[]));
        assert!(ge(500).should_log(503, "-", &[]));
    }

    #[test]
    fn and_or_should_log_all_any_and_empty_boundary() {
        // AND = all children match; OR = any child matches. Uses status-code
        // children (ge/le) so the test needs no header stub.
        let and = LogFilter::And(vec![ge(200), le(299)]); // 2xx band
        assert!(and.should_log(200, "-", &[])); // both true
        assert!(and.should_log(299, "-", &[]));
        assert!(!and.should_log(500, "-", &[])); // le(299) false → AND false

        let or = LogFilter::Or(vec![le(199), ge(500)]); // 1xx OR 5xx
        assert!(or.should_log(100, "-", &[])); // le(199) true
        assert!(or.should_log(503, "-", &[])); // ge(500) true
        assert!(!or.should_log(200, "-", &[])); // neither → OR false

        // Nested composition recurses.
        let nested = LogFilter::Or(vec![LogFilter::And(vec![ge(200), le(299)]), ge(500)]);
        assert!(nested.should_log(204, "-", &[])); // AND-child true
        assert!(nested.should_log(500, "-", &[])); // leaf true
        assert!(!nested.should_log(404, "-", &[])); // AND-child false, leaf false

        // Empty-vec boundary (unreachable via config's min_items=2, pinned as a
        // semantic invariant): all([]) = true, any([]) = false.
        assert!(LogFilter::And(vec![]).should_log(200, "-", &[]));
        assert!(!LogFilter::Or(vec![]).should_log(200, "-", &[]));
    }

    #[test]
    fn eq_404_boundary() {
        assert!(!eq(404).should_log(403, "-", &[]));
        assert!(eq(404).should_log(404, "NR", &[]));
        assert!(!eq(404).should_log(405, "-", &[]));
    }

    #[test]
    fn le_200_boundary() {
        assert!(le(200).should_log(200, "-", &[]));
        assert!(!le(200).should_log(201, "-", &[]));
        assert!(le(200).should_log(100, "-", &[]));
    }

    #[test]
    fn response_flag_membership() {
        // The ResponseFlag arm ignores `status`; pass any value.
        assert!(rf(&["NR"]).should_log(404, "NR", &[]));
        assert!(rf(&["UH", "NR"]).should_log(404, "NR", &[]));
        assert!(!rf(&["UH"]).should_log(404, "NR", &[]));
    }

    #[test]
    fn response_flag_dash_sentinel_never_matches_nonempty() {
        // "-" ∉ the 29-token set, so a non-empty `flags` never matches it.
        assert!(!rf(&["NR"]).should_log(503, "-", &[]));
        assert!(!rf(&["UH", "UF"]).should_log(503, "-", &[]));
    }

    #[test]
    fn response_flag_empty_matches_any_flag_set() {
        // MEASURED (ADR-0145 PV-6): empty `flags` keeps records WITH a flag,
        // drops the "-" no-flag sentinel.
        assert!(rf(&[]).should_log(404, "NR", &[]));
        assert!(rf(&[]).should_log(503, "UF", &[]));
        assert!(!rf(&[]).should_log(503, "-", &[]));
    }

    #[test]
    fn response_flag_inert_token_never_matches_produced() {
        // A config may carry an inert token (`DI`); envoy-rust never renders it.
        assert!(!rf(&["DI"]).should_log(404, "NR", &[]));
        assert!(!rf(&["DI"]).should_log(503, "-", &[]));
    }

    #[test]
    fn status_code_arm_ignores_response_flags() {
        let f = LogFilter::StatusCode(StatusCodeComparison {
            op: FilterOp::Ge,
            threshold: 500,
        });
        assert!(f.should_log(503, "-", &[]));
        assert!(f.should_log(503, "NR", &[]));
        assert!(!f.should_log(200, "NR", &[]));
    }

    // --- phase 72: LogFilter::Header delegates to the injected HeaderMatch ---

    /// A local `HeaderMatch` stub. The accesslog crate cannot build a real
    /// `envoy_config::HeaderMatcher` (it must not depend on `envoy-config` —
    /// ADR-0150 cycle), so this proves the `should_log` PLUMBING: the `Header`
    /// arm routes to `matcher.matches(headers)`, keeping on match and dropping
    /// on mismatch/absent. Real per-mode membership is covered end-to-end in
    /// `envoy-http1` (phase 72 T9) over the actual engine.
    #[derive(Debug)]
    struct HasHeaderValue(&'static str, &'static str);
    impl HeaderMatch for HasHeaderValue {
        fn matches(&self, headers: &[(String, String)]) -> bool {
            headers.iter().any(|(n, v)| n == self.0 && v == self.1)
        }
    }

    #[test]
    fn header_filter_should_log_delegates_to_matcher() {
        let f = LogFilter::Header {
            matcher: std::sync::Arc::new(HasHeaderValue("x-log", "yes")),
        };
        // The `Header` arm ignores `status`/`response_flags`; it gates on headers.
        assert!(f.should_log(200, "-", &[("x-log".to_string(), "yes".to_string())]));
        assert!(!f.should_log(200, "-", &[("x-log".to_string(), "no".to_string())]));
        assert!(!f.should_log(200, "-", &[]));
    }
}
