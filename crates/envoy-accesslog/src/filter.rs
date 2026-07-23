//! Phase 70: the access-log FILTER predicate — the per-record emission gate
//! compiled from `envoy_config::AccessLogFilter`. Phase 70 added
//! `status_code_filter`; phase 71 added `response_flag_filter`; phase 72 adds
//! `header_filter` (the `LogFilter::Header` arm).

use std::collections::BTreeMap;
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

/// Phase 74 (ADR-0150/ADR-0155): the runtime seam for the `metadata_filter` arm.
/// Same cycle constraint as `HeaderMatch` — this crate CANNOT depend on
/// `envoy-config`, so the resolution engine is injected as a trait object:
/// `envoy-config` impls `MetadataMatch` for its `MetadataMatcher` (resolving
/// `filter` → `path[0].key` and delegating to `ValueMatcher::matches` VERBATIM),
/// and the HCM compile step in `envoy-http1` boxes it into
/// `LogFilter::Metadata`. `Send + Sync` because sinks cross async await points.
///
/// **Returns `Option<bool>`, NOT `bool`.** `None` iff the metadata path did NOT
/// resolve, so the `match_if_key_not_found` policy — which lives on the FILTER,
/// not the matcher — stays in `LogFilter`, expressing the MEASURED rule (SPEC §0
/// R-0.4) exactly once. Collapsing `None` into `false` (as
/// `ValueMatcher::matches_resolved` does for the RBAC path) would DROP every
/// key-absent record, the opposite of the measured upstream default (`true`).
pub trait MetadataMatch: std::fmt::Debug + Send + Sync {
    /// `None` iff the configured metadata path did not resolve; otherwise
    /// `Some(value_matcher_verdict)`.
    fn matches(
        &self,
        dynamic_metadata: &BTreeMap<String, BTreeMap<String, String>>,
    ) -> Option<bool>;
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
    /// Phase 74: emit a record iff its DYNAMIC METADATA satisfies the matcher —
    /// or, when the metadata path does not resolve, iff
    /// `match_if_key_not_found`. `matcher` is `Option` because upstream ACCEPTS
    /// a matcher-less `metadata_filter: {}` (MEASURED R-0.2), in which case
    /// every record takes the not-found policy. `match_if_key_not_found` is
    /// already resolved to a concrete `bool` by the compile step
    /// (`Option<bool>::unwrap_or(true)` — the MEASURED wrapper default, R-0.4).
    /// Introduces no `Eq`/`PartialEq` and no `envoy-config` dep (ADR-0150 holds).
    Metadata {
        matcher: Option<Arc<dyn MetadataMatch>>,
        match_if_key_not_found: bool,
    },
}

impl LogFilter {
    /// Returns `true` iff a record with the given final response `status`,
    /// `response_flags` token, request `headers`, and per-request
    /// `dynamic_metadata` should be emitted. The `StatusCode` arm reads only
    /// `status`; the `ResponseFlag` arm only `response_flags`; the `Header` arm
    /// only `headers`; the phase-74 `Metadata` arm only `dynamic_metadata`. The
    /// status comparison is widened to `u32` (lossless; status is always in
    /// `u16` range).
    pub fn should_log(
        &self,
        status: u16,
        response_flags: &str,
        headers: &[(String, String)],
        dynamic_metadata: &BTreeMap<String, BTreeMap<String, String>>,
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
                .all(|f| f.should_log(status, response_flags, headers, dynamic_metadata)),
            LogFilter::Or(filters) => filters
                .iter()
                .any(|f| f.should_log(status, response_flags, headers, dynamic_metadata)),
            // Phase 74: the MEASURED decision rule (SPEC §0 R-0.3/R-0.4) —
            // resolve `dynamic_metadata[filter][path[0].key]`; unresolved (or no
            // matcher at all) → `match_if_key_not_found`; resolved →
            // `value.matches(v)`. A missing NAMESPACE behaves identically to a
            // missing KEY (the trait impl returns `None` for both).
            LogFilter::Metadata {
                matcher,
                match_if_key_not_found,
            } => match matcher {
                None => *match_if_key_not_found,
                Some(m) => m
                    .matches(dynamic_metadata)
                    .unwrap_or(*match_if_key_not_found),
            },
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
        assert!(!ge(500).should_log(499, "-", &[], &Default::default()));
        assert!(ge(500).should_log(500, "-", &[], &Default::default()));
        assert!(ge(500).should_log(503, "-", &[], &Default::default()));
    }

    #[test]
    fn and_or_should_log_all_any_and_empty_boundary() {
        // AND = all children match; OR = any child matches. Uses status-code
        // children (ge/le) so the test needs no header stub.
        let and = LogFilter::And(vec![ge(200), le(299)]); // 2xx band
        assert!(and.should_log(200, "-", &[], &Default::default())); // both true
        assert!(and.should_log(299, "-", &[], &Default::default()));
        assert!(!and.should_log(500, "-", &[], &Default::default())); // le(299) false → AND false

        let or = LogFilter::Or(vec![le(199), ge(500)]); // 1xx OR 5xx
        assert!(or.should_log(100, "-", &[], &Default::default())); // le(199) true
        assert!(or.should_log(503, "-", &[], &Default::default())); // ge(500) true
        assert!(!or.should_log(200, "-", &[], &Default::default())); // neither → OR false

        // Nested composition recurses.
        let nested = LogFilter::Or(vec![LogFilter::And(vec![ge(200), le(299)]), ge(500)]);
        assert!(nested.should_log(204, "-", &[], &Default::default())); // AND-child true
        assert!(nested.should_log(500, "-", &[], &Default::default())); // leaf true
        assert!(!nested.should_log(404, "-", &[], &Default::default())); // AND-child false, leaf false

        // Empty-vec boundary (unreachable via config's min_items=2, pinned as a
        // semantic invariant): all([]) = true, any([]) = false.
        assert!(LogFilter::And(vec![]).should_log(200, "-", &[], &Default::default()));
        assert!(!LogFilter::Or(vec![]).should_log(200, "-", &[], &Default::default()));
    }

    #[test]
    fn eq_404_boundary() {
        assert!(!eq(404).should_log(403, "-", &[], &Default::default()));
        assert!(eq(404).should_log(404, "NR", &[], &Default::default()));
        assert!(!eq(404).should_log(405, "-", &[], &Default::default()));
    }

    #[test]
    fn le_200_boundary() {
        assert!(le(200).should_log(200, "-", &[], &Default::default()));
        assert!(!le(200).should_log(201, "-", &[], &Default::default()));
        assert!(le(200).should_log(100, "-", &[], &Default::default()));
    }

    #[test]
    fn response_flag_membership() {
        // The ResponseFlag arm ignores `status`; pass any value.
        assert!(rf(&["NR"]).should_log(404, "NR", &[], &Default::default()));
        assert!(rf(&["UH", "NR"]).should_log(404, "NR", &[], &Default::default()));
        assert!(!rf(&["UH"]).should_log(404, "NR", &[], &Default::default()));
    }

    #[test]
    fn response_flag_dash_sentinel_never_matches_nonempty() {
        // "-" ∉ the 29-token set, so a non-empty `flags` never matches it.
        assert!(!rf(&["NR"]).should_log(503, "-", &[], &Default::default()));
        assert!(!rf(&["UH", "UF"]).should_log(503, "-", &[], &Default::default()));
    }

    #[test]
    fn response_flag_empty_matches_any_flag_set() {
        // MEASURED (ADR-0145 PV-6): empty `flags` keeps records WITH a flag,
        // drops the "-" no-flag sentinel.
        assert!(rf(&[]).should_log(404, "NR", &[], &Default::default()));
        assert!(rf(&[]).should_log(503, "UF", &[], &Default::default()));
        assert!(!rf(&[]).should_log(503, "-", &[], &Default::default()));
    }

    #[test]
    fn response_flag_inert_token_never_matches_produced() {
        // A config may carry an inert token (`DI`); envoy-rust never renders it.
        assert!(!rf(&["DI"]).should_log(404, "NR", &[], &Default::default()));
        assert!(!rf(&["DI"]).should_log(503, "-", &[], &Default::default()));
    }

    #[test]
    fn status_code_arm_ignores_response_flags() {
        let f = LogFilter::StatusCode(StatusCodeComparison {
            op: FilterOp::Ge,
            threshold: 500,
        });
        assert!(f.should_log(503, "-", &[], &Default::default()));
        assert!(f.should_log(503, "NR", &[], &Default::default()));
        assert!(!f.should_log(200, "NR", &[], &Default::default()));
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
        assert!(f.should_log(
            200,
            "-",
            &[("x-log".to_string(), "yes".to_string())],
            &Default::default()
        ));
        assert!(!f.should_log(
            200,
            "-",
            &[("x-log".to_string(), "no".to_string())],
            &Default::default()
        ));
        assert!(!f.should_log(200, "-", &[], &Default::default()));
    }

    /// Phase 74 T3: `should_log` carries the per-request dynamic-metadata store
    /// as a 4th argument. Every PRE-74 arm ignores it — this pins that the
    /// widening is behavior-neutral.
    #[test]
    fn existing_arms_ignore_the_dynamic_metadata_argument() {
        use std::collections::BTreeMap;
        let mut md: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        md.entry("com.example".into())
            .or_default()
            .insert("k".into(), "1".into());
        let empty: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();

        // StatusCode arm: identical verdict with and without metadata.
        assert!(ge(500).should_log(503, "-", &[], &md));
        assert!(ge(500).should_log(503, "-", &[], &empty));
        assert!(!ge(500).should_log(499, "-", &[], &md));

        // ResponseFlag arm.
        assert!(rf(&["NR"]).should_log(404, "NR", &[], &md));
        assert!(!rf(&["UH"]).should_log(404, "NR", &[], &md));

        // Header arm (via the local stub).
        let h = LogFilter::Header {
            matcher: std::sync::Arc::new(HasHeaderValue("x-log", "yes")),
        };
        assert!(h.should_log(200, "-", &[("x-log".to_string(), "yes".to_string())], &md));
        assert!(!h.should_log(200, "-", &[], &md));

        // Composition arms thread the new argument through the recursion.
        let and = LogFilter::And(vec![ge(200), le(299)]);
        assert!(and.should_log(204, "-", &[], &md));
        assert!(!and.should_log(500, "-", &[], &md));
        let or = LogFilter::Or(vec![le(199), ge(500)]);
        assert!(or.should_log(503, "-", &[], &md));
        assert!(!or.should_log(200, "-", &[], &md));
    }

    // --- phase 74: LogFilter::Metadata + the injected MetadataMatch seam ---

    /// A local `MetadataMatch` stub. The accesslog crate cannot build a real
    /// `envoy_config::MetadataMatcher` (it must not depend on `envoy-config` —
    /// ADR-0150 cycle), so this proves the `should_log` PLUMBING and the
    /// `Option<bool>` contract: `None` iff the path did not resolve, so
    /// `LogFilter` applies `match_if_key_not_found`. The real resolution +
    /// value-matcher coverage lives in `envoy-config` (T5) and `envoy-http1`
    /// (T6) over the actual engine.
    #[derive(Debug)]
    struct NsKeyEquals(&'static str, &'static str, &'static str);
    impl MetadataMatch for NsKeyEquals {
        fn matches(
            &self,
            dynamic_metadata: &BTreeMap<String, BTreeMap<String, String>>,
        ) -> Option<bool> {
            let v = dynamic_metadata.get(self.0)?.get(self.1)?;
            Some(v == self.2)
        }
    }

    fn md(pairs: &[(&str, &str, &str)]) -> BTreeMap<String, BTreeMap<String, String>> {
        let mut m: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        for (ns, k, v) in pairs {
            m.entry((*ns).to_string())
                .or_default()
                .insert((*k).to_string(), (*v).to_string());
        }
        m
    }

    #[test]
    fn metadata_arm_implements_the_measured_decision_rule() {
        // MEASURED (SPEC §0 R-0.3/R-0.4, `envoyproxy/envoy:v1.33.0`):
        //   resolved = dynamic_metadata[filter][path[0].key]
        //   None    => match_if_key_not_found     (DEFAULT true)
        //   Some(v) => value.matches(v)
        let keep_default = LogFilter::Metadata {
            matcher: Some(std::sync::Arc::new(NsKeyEquals("com.example", "k", "1"))),
            match_if_key_not_found: true,
        };
        let drop_default = LogFilter::Metadata {
            matcher: Some(std::sync::Arc::new(NsKeyEquals("com.example", "k", "1"))),
            match_if_key_not_found: false,
        };

        // Value MATCHES → KEEP, regardless of the not-found policy.
        let hit = md(&[("com.example", "k", "1")]);
        assert!(keep_default.should_log(200, "-", &[], &hit));
        assert!(drop_default.should_log(200, "-", &[], &hit));

        // Value MISMATCH → DROP, regardless of the not-found policy (the value
        // matcher is only consulted when the path RESOLVES).
        let miss = md(&[("com.example", "k", "2")]);
        assert!(!keep_default.should_log(200, "-", &[], &miss));
        assert!(!drop_default.should_log(200, "-", &[], &miss));

        // KEY absent inside a PRESENT namespace → the not-found policy decides.
        let other_key = md(&[("com.example", "other", "1")]);
        assert!(keep_default.should_log(200, "-", &[], &other_key));
        assert!(!drop_default.should_log(200, "-", &[], &other_key));

        // NAMESPACE absent behaves IDENTICALLY to a missing key (MEASURED R-0.4).
        let other_ns = md(&[("com.other", "k", "1")]);
        assert!(keep_default.should_log(200, "-", &[], &other_ns));
        assert!(!drop_default.should_log(200, "-", &[], &other_ns));

        // Wholly empty store → same not-found path.
        let empty = md(&[]);
        assert!(keep_default.should_log(200, "-", &[], &empty));
        assert!(!drop_default.should_log(200, "-", &[], &empty));

        // MATCHER-LESS filter (upstream accepts `metadata_filter: {}`, R-0.2):
        // every record takes the not-found policy.
        let no_matcher_keep = LogFilter::Metadata {
            matcher: None,
            match_if_key_not_found: true,
        };
        let no_matcher_drop = LogFilter::Metadata {
            matcher: None,
            match_if_key_not_found: false,
        };
        assert!(no_matcher_keep.should_log(200, "-", &[], &hit));
        assert!(!no_matcher_drop.should_log(200, "-", &[], &hit));

        // The arm ignores status / response_flags / headers.
        assert!(keep_default.should_log(503, "UF", &[("x-a".into(), "1".into())], &hit));
    }

    #[test]
    fn metadata_arm_composes_under_and_or() {
        // The phase-73 composition arms thread the store through the recursion.
        let meta = LogFilter::Metadata {
            matcher: Some(std::sync::Arc::new(NsKeyEquals("com.example", "k", "1"))),
            match_if_key_not_found: false,
        };
        let and = LogFilter::And(vec![meta.clone(), ge(500)]);
        let hit = md(&[("com.example", "k", "1")]);
        assert!(and.should_log(503, "-", &[], &hit)); // both true
        assert!(!and.should_log(200, "-", &[], &hit)); // status false
        assert!(!and.should_log(503, "-", &[], &md(&[]))); // metadata false

        let or = LogFilter::Or(vec![meta, ge(500)]);
        assert!(or.should_log(200, "-", &[], &hit)); // metadata true
        assert!(or.should_log(503, "-", &[], &md(&[]))); // status true
        assert!(!or.should_log(200, "-", &[], &md(&[]))); // neither
    }
}
