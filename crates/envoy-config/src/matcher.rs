//! HeaderMatcher / StringMatcher runtime. Per-matcher truth predicate
//! consumed by HCM's route walker (in crates/envoy-http1/src/hcm.rs).
//!
//! AND-combination across multiple HeaderMatchers on the same Route lives
//! in the route walker, not here — `HeaderMatcher::matches` is per-matcher.
//!
//! Phase 04.2.

use crate::bootstrap::{
    HeaderMatcher, HeaderMatcherMode, MetadataMatcher, StringMatcher, StringMatcherMode,
    ValueMatcher,
};

impl HeaderMatcher {
    /// Returns true iff this matcher matches the given header set.
    ///
    /// Header NAME matching is case-insensitive per HTTP/1.1 RFC 7230 §3.2.
    /// Header VALUE matching is case-sensitive by default; the StringMatcher
    /// variant's `ignore_case` flips it for the value (Exact/Prefix/Suffix/
    /// Contains only — SafeRegex callers express case insensitivity via the
    /// `(?i)` inline flag; SPEC §6 signpost 15).
    pub fn matches(&self, headers: &[(String, String)]) -> bool {
        let value = headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(&self.name))
            .map(|(_, v)| v.as_str());

        let mode_result = match (&self.mode, value) {
            // present_match is the ONLY mode that evaluates with the header
            // ABSENT, and the only one an absent header carries into
            // `invert_match`. present_match: true → must be PRESENT;
            // present_match: false → must be ABSENT.
            (HeaderMatcherMode::PresentMatch(want_present), v) => v.is_some() == *want_present,
            // Every VALUE mode short-circuits on an absent header WITHOUT
            // reaching the XOR below. Order matters: this arm must sit after
            // the present_match arm and before every value arm.
            (_, None) => return false,
            (HeaderMatcherMode::ExactMatch(lit), Some(v)) => v == lit.as_str(),
            (HeaderMatcherMode::PrefixMatch(lit), Some(v)) => v.starts_with(lit.as_str()),
            (HeaderMatcherMode::SuffixMatch(lit), Some(v)) => v.ends_with(lit.as_str()),
            (HeaderMatcherMode::SafeRegexMatch(sr), Some(v)) => sr
                .compiled
                .as_ref()
                .expect("validator ensured HeaderMatcher SafeRegex compiled")
                .is_match(v),
            (HeaderMatcherMode::RangeMatch(r), Some(v)) => {
                v.parse::<i64>().is_ok_and(|n| n >= r.start && n < r.end)
            }
            (HeaderMatcherMode::StringMatch(sm), Some(v)) => sm.matches(v),
        };

        mode_result ^ self.invert_match
    }
}

/// Phase 72 (ADR-0150): expose the phase-04.2 `HeaderMatcher` engine to the
/// access-log crate as an injected trait object. `envoy-accesslog` cannot
/// depend on `envoy-config` (the reverse edge already exists → cycle), so it
/// defines the `HeaderMatch` seam and `envoy-config` — which DOES depend on
/// `envoy-accesslog` — provides the impl here, reusing `HeaderMatcher::matches`
/// VERBATIM. This keeps PV-4 (`mode_result ^ invert_match`, incl. absent+invert
/// = keep) identical between route matching and access-log filtering with zero
/// duplication.
impl envoy_accesslog::HeaderMatch for HeaderMatcher {
    fn matches(&self, headers: &[(String, String)]) -> bool {
        // Method-call syntax gives the inherent `HeaderMatcher::matches` (the
        // engine, above) priority over this trait method, so this delegates —
        // it does NOT recurse.
        self.matches(headers)
    }
}

/// Phase 74 (ADR-0150/ADR-0155): the sole `MetadataMatch` impl — the access-log
/// `metadata_filter` resolution engine. `envoy-accesslog` cannot see
/// `MetadataMatcher`'s `filter`/`path` fields (it must not depend on
/// `envoy-config` — cycle), so resolution happens HERE and only the verdict
/// crosses the seam.
///
/// The MEASURED rule (SPEC §0 R-0.3/R-0.4, `envoyproxy/envoy:v1.33.0`):
/// `resolved = dynamic_metadata[filter][path[0].key]`; unresolved → `None` (the
/// caller applies `match_if_key_not_found`, whose measured default is `true`);
/// resolved → `Some(value.matches(v))`, reusing the phase-35/36
/// `ValueMatcher::matches` engine VERBATIM.
///
/// NB `ValueMatcher::matches_resolved` — the RBAC-path sibling — is deliberately
/// NOT used: it maps an unresolved path to `false`, which would drop every
/// key-absent record instead of deferring to `match_if_key_not_found`.
///
/// `path.first()?` (rather than `path[0]`) keeps this total: the T2 validator
/// guarantees `path.len() == 1` for every matcher that can reach here, so an
/// empty path is unreachable in a booted proxy, and degrading to "unresolved"
/// beats a panic.
impl envoy_accesslog::MetadataMatch for MetadataMatcher {
    fn matches(
        &self,
        dynamic_metadata: &std::collections::BTreeMap<
            String,
            std::collections::BTreeMap<String, String>,
        >,
    ) -> Option<bool> {
        let key = &self.path.first()?.key;
        let resolved = dynamic_metadata.get(&self.filter)?.get(key)?;
        Some(self.value.matches(resolved))
    }
}

impl StringMatcher {
    /// Returns true iff this matcher matches the given value. Case sensitivity
    /// of value comparison follows `self.ignore_case` for Exact / Prefix /
    /// Suffix / Contains; SafeRegex ignores `ignore_case` (regex callers use
    /// `(?i)` inline flag instead) per Envoy proto.
    pub fn matches(&self, value: &str) -> bool {
        match &self.mode {
            StringMatcherMode::Exact(lit) => {
                if self.ignore_case {
                    value.eq_ignore_ascii_case(lit)
                } else {
                    value == lit.as_str()
                }
            }
            StringMatcherMode::Prefix(lit) => {
                if self.ignore_case {
                    value
                        .get(..lit.len())
                        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(lit))
                } else {
                    value.starts_with(lit.as_str())
                }
            }
            StringMatcherMode::Suffix(lit) => {
                if self.ignore_case {
                    value
                        .get(value.len().saturating_sub(lit.len())..)
                        .is_some_and(|suffix| suffix.eq_ignore_ascii_case(lit))
                } else {
                    value.ends_with(lit.as_str())
                }
            }
            StringMatcherMode::SafeRegex(sr) => sr
                .compiled
                .as_ref()
                .expect("validator ensured StringMatcher SafeRegex compiled")
                .is_match(value),
            StringMatcherMode::Contains(lit) => {
                if self.ignore_case {
                    value
                        .to_ascii_lowercase()
                        .contains(&lit.to_ascii_lowercase())
                } else {
                    value.contains(lit.as_str())
                }
            }
        }
    }
}

impl ValueMatcher {
    /// Match against a PRESENT value. `present_match` returns its bool (the value is present,
    /// so `present && want == want`). Kept for the value-present call sites.
    pub fn matches(&self, value: &str) -> bool {
        match self {
            ValueMatcher::StringMatch(sm) => sm.matches(value),
            ValueMatcher::PresentMatch(want) => *want,
        }
    }

    /// Presence-aware entry (§A1). `resolved` is `Some(v)` iff the metadata key resolved.
    /// `present_match`: `match = resolved.is_some() && want`. `string_match`: value present AND matches.
    pub fn matches_resolved(&self, resolved: Option<&str>) -> bool {
        match self {
            ValueMatcher::StringMatch(sm) => resolved.is_some_and(|v| sm.matches(v)),
            ValueMatcher::PresentMatch(want) => resolved.is_some() && *want,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bootstrap::{Int64Range, SafeRegex};

    fn h(name: &str, value: &str) -> (String, String) {
        (name.to_string(), value.to_string())
    }

    fn compile(pattern: &str) -> SafeRegex {
        SafeRegex {
            regex: pattern.to_string(),
            compiled: Some(std::sync::Arc::new(regex::Regex::new(pattern).unwrap())),
        }
    }

    fn hm(name: &str, mode: HeaderMatcherMode) -> HeaderMatcher {
        HeaderMatcher {
            name: name.to_string(),
            mode,
            invert_match: false,
        }
    }

    fn hm_inverted(name: &str, mode: HeaderMatcherMode) -> HeaderMatcher {
        HeaderMatcher {
            name: name.to_string(),
            mode,
            invert_match: true,
        }
    }

    // ExactMatch: 3 cells.
    #[test]
    fn exact_match_matches_value() {
        let m = hm("x-foo", HeaderMatcherMode::ExactMatch("bar".into()));
        assert!(m.matches(&[h("x-foo", "bar")]));
    }
    #[test]
    fn exact_match_rejects_value() {
        let m = hm("x-foo", HeaderMatcherMode::ExactMatch("bar".into()));
        assert!(!m.matches(&[h("x-foo", "baz")]));
    }
    #[test]
    fn exact_match_absent_returns_false() {
        let m = hm("x-foo", HeaderMatcherMode::ExactMatch("bar".into()));
        assert!(!m.matches(&[h("x-other", "bar")]));
    }

    // PrefixMatch: 3 cells.
    #[test]
    fn prefix_match_matches_value() {
        let m = hm("x-foo", HeaderMatcherMode::PrefixMatch("ba".into()));
        assert!(m.matches(&[h("x-foo", "bar")]));
    }
    #[test]
    fn prefix_match_rejects_value() {
        let m = hm("x-foo", HeaderMatcherMode::PrefixMatch("ba".into()));
        assert!(!m.matches(&[h("x-foo", "qux")]));
    }
    #[test]
    fn prefix_match_absent_returns_false() {
        let m = hm("x-foo", HeaderMatcherMode::PrefixMatch("ba".into()));
        assert!(!m.matches(&[]));
    }

    // SuffixMatch: 3 cells.
    #[test]
    fn suffix_match_matches_value() {
        let m = hm("x-foo", HeaderMatcherMode::SuffixMatch("ar".into()));
        assert!(m.matches(&[h("x-foo", "bar")]));
    }
    #[test]
    fn suffix_match_rejects_value() {
        let m = hm("x-foo", HeaderMatcherMode::SuffixMatch("ar".into()));
        assert!(!m.matches(&[h("x-foo", "qux")]));
    }
    #[test]
    fn suffix_match_absent_returns_false() {
        let m = hm("x-foo", HeaderMatcherMode::SuffixMatch("ar".into()));
        assert!(!m.matches(&[]));
    }

    // SafeRegexMatch: 3 cells.
    #[test]
    fn safe_regex_match_matches_value() {
        let m = hm(
            "x-version",
            HeaderMatcherMode::SafeRegexMatch(compile("^v[0-9]+$")),
        );
        assert!(m.matches(&[h("x-version", "v42")]));
    }
    #[test]
    fn safe_regex_match_rejects_value() {
        let m = hm(
            "x-version",
            HeaderMatcherMode::SafeRegexMatch(compile("^v[0-9]+$")),
        );
        assert!(!m.matches(&[h("x-version", "vBETA")]));
    }
    #[test]
    fn safe_regex_match_absent_returns_false() {
        let m = hm(
            "x-version",
            HeaderMatcherMode::SafeRegexMatch(compile("^v[0-9]+$")),
        );
        assert!(!m.matches(&[]));
    }

    // RangeMatch: 5 cells (boundary checks per SPEC §6 signpost 6).
    #[test]
    fn range_match_value_in_range_returns_true() {
        let m = hm(
            "x-version",
            HeaderMatcherMode::RangeMatch(Int64Range { start: 1, end: 100 }),
        );
        assert!(m.matches(&[h("x-version", "42")]));
    }
    #[test]
    fn range_match_value_at_start_returns_true() {
        let m = hm(
            "x-version",
            HeaderMatcherMode::RangeMatch(Int64Range { start: 1, end: 100 }),
        );
        assert!(m.matches(&[h("x-version", "1")]));
    }
    #[test]
    fn range_match_value_at_end_returns_false() {
        // Half-open: end is exclusive.
        let m = hm(
            "x-version",
            HeaderMatcherMode::RangeMatch(Int64Range { start: 1, end: 100 }),
        );
        assert!(!m.matches(&[h("x-version", "100")]));
    }
    #[test]
    fn range_match_value_below_start_returns_false() {
        let m = hm(
            "x-version",
            HeaderMatcherMode::RangeMatch(Int64Range { start: 1, end: 100 }),
        );
        assert!(!m.matches(&[h("x-version", "0")]));
    }
    #[test]
    fn range_match_non_parseable_value_returns_false() {
        // Non-parseable values fail the match (NOT an error). SPEC §6 signpost 6.
        let m = hm(
            "x-version",
            HeaderMatcherMode::RangeMatch(Int64Range { start: 1, end: 100 }),
        );
        assert!(!m.matches(&[h("x-version", "vBETA")]));
    }

    // PresentMatch: 4 cells (true × present, true × absent, false × present, false × absent).
    #[test]
    fn present_match_true_returns_true_when_present() {
        let m = hm("authorization", HeaderMatcherMode::PresentMatch(true));
        assert!(m.matches(&[h("authorization", "Bearer x")]));
    }
    #[test]
    fn present_match_true_returns_false_when_absent() {
        let m = hm("authorization", HeaderMatcherMode::PresentMatch(true));
        assert!(!m.matches(&[]));
    }
    #[test]
    fn present_match_false_requires_the_header_to_be_absent() {
        // MEASURED (SPEC §2.3 probe p12, both proxies): upstream
        // `present_match: false` means the header must be ABSENT. Before phase
        // 75.1 this test asserted the opposite ("no presence requirement,
        // always true") and was the in-tree test that PINNED divergence D2.
        let m = hm("authorization", HeaderMatcherMode::PresentMatch(false));
        assert!(!m.matches(&[h("authorization", "Bearer x")]));
    }
    #[test]
    fn present_match_false_matches_when_absent() {
        // Right answer, and after phase 75.1 for the right reason: the rule is
        // `(present == want)`, so absent + want=false is `(false == false)` =
        // true. Before 75.1 this passed only because the mode arm returned an
        // UNCONDITIONAL `true` — the same wrong rule that made
        // `present_match_false_requires_the_header_to_be_absent` fail.
        let m = hm("authorization", HeaderMatcherMode::PresentMatch(false));
        assert!(m.matches(&[]));
    }

    // StringMatch: 3 representative cells.
    #[test]
    fn string_match_contains_returns_true() {
        let sm = StringMatcher {
            mode: StringMatcherMode::Contains("beta".into()),
            ignore_case: false,
        };
        let m = hm("x-tag", HeaderMatcherMode::StringMatch(sm));
        assert!(m.matches(&[h("x-tag", "release-beta-1")]));
    }
    #[test]
    fn string_match_contains_with_ignore_case_returns_true_on_uppercase() {
        let sm = StringMatcher {
            mode: StringMatcherMode::Contains("beta".into()),
            ignore_case: true,
        };
        let m = hm("x-tag", HeaderMatcherMode::StringMatch(sm));
        assert!(m.matches(&[h("x-tag", "RELEASE-BETA-1")]));
    }
    #[test]
    fn string_match_safe_regex_ignore_case_no_effect() {
        // ignore_case: true does NOT affect the SafeRegex variant per Envoy proto.
        let sm = StringMatcher {
            mode: StringMatcherMode::SafeRegex(compile("^beta$")),
            ignore_case: true,
        };
        let m = hm("x-tag", HeaderMatcherMode::StringMatch(sm));
        // Pattern is case-sensitive; "BETA" should not match despite ignore_case.
        assert!(!m.matches(&[h("x-tag", "BETA")]));
        assert!(m.matches(&[h("x-tag", "beta")]));
    }

    #[test]
    fn string_match_prefix_with_ignore_case_matches() {
        let sm = StringMatcher {
            mode: StringMatcherMode::Prefix("BA".into()),
            ignore_case: true,
        };
        let m = hm("x-foo", HeaderMatcherMode::StringMatch(sm));
        assert!(m.matches(&[h("x-foo", "bar")]));
        assert!(!m.matches(&[h("x-foo", "qux")]));
    }

    #[test]
    fn string_match_suffix_with_ignore_case_matches() {
        let sm = StringMatcher {
            mode: StringMatcherMode::Suffix("AR".into()),
            ignore_case: true,
        };
        let m = hm("x-foo", HeaderMatcherMode::StringMatch(sm));
        assert!(m.matches(&[h("x-foo", "bar")]));
        assert!(!m.matches(&[h("x-foo", "baz")]));
    }

    // Cross-cutting tests.
    #[test]
    fn header_name_match_is_case_insensitive() {
        let m = hm("X-Foo", HeaderMatcherMode::ExactMatch("bar".into()));
        assert!(m.matches(&[h("x-foo", "bar")]));
    }
    #[test]
    fn header_value_match_is_case_sensitive_by_default() {
        let m = hm("x-foo", HeaderMatcherMode::ExactMatch("bar".into()));
        assert!(!m.matches(&[h("x-foo", "BAR")]));
    }
    #[test]
    fn invert_match_inverts_exact_match_result() {
        let m = hm_inverted("x-foo", HeaderMatcherMode::ExactMatch("bar".into()));
        assert!(m.matches(&[h("x-foo", "baz")]));
        assert!(!m.matches(&[h("x-foo", "bar")]));
    }
    #[test]
    fn invert_match_inverts_present_match_result() {
        // GUARD (phase 75.1): `present_match` is the ONLY mode whose ABSENT
        // cell still reaches `invert_match`. A uniform "absent => DROP" fix of
        // the shared engine would flip the first assertion below and mint a NEW
        // divergence. MEASURED PARITY on both proxies (SPEC §2.3 probe p07).
        let m = hm_inverted("authorization", HeaderMatcherMode::PresentMatch(true));
        assert!(m.matches(&[]));
        assert!(!m.matches(&[h("authorization", "x")]));
    }

    #[test]
    fn pv4_value_matcher_absent_plus_invert_dropped_is_parity_with_upstream() {
        // MEASURED (ADR-0151, re-measured at the phase-75 state-2 PLAN-write on
        // BOTH proxies): a VALUE-based matcher
        // (exact/prefix/suffix/regex/range/string_match) with `invert_match` +
        // an ABSENT header DROPS on upstream `envoyproxy/envoy:v1.33.0` — a
        // missing header is an unconditional value no-match that `invert_match`
        // does NOT resurrect. Until phase 75.1 the shared engine
        // (matcher.rs:52) applied `mode_result ^ invert_match` UNIFORMLY and
        // KEPT it, which was carry-forward CF-72-1; phase 75.1 CLOSED it by
        // short-circuiting every value mode to `false` on an absent header
        // BEFORE the XOR. Contrast the PARITY companion
        // `pv4_present_match_absent_plus_invert_kept_is_parity_with_upstream`,
        // which must keep the OPPOSITE verdict — that asymmetry is the whole
        // point of the mode scoping.
        let hm = hm_inverted("x-log", HeaderMatcherMode::ExactMatch("yes".into()));
        // Direct engine (route path):
        assert!(
            !hm.matches(&[]),
            "value-matcher absent+invert DROPS, matching upstream (CF-72-1 CLOSED)"
        );
        // Same verdict through the access-log `HeaderMatch` seam:
        let via_trait: std::sync::Arc<dyn envoy_accesslog::HeaderMatch> = std::sync::Arc::new(
            hm_inverted("x-log", HeaderMatcherMode::ExactMatch("yes".into())),
        );
        assert!(
            !via_trait.matches(&[]),
            "access-log path drops value-matcher absent+invert too (CF-72-1 CLOSED)"
        );
    }

    #[test]
    fn pv4_present_match_absent_plus_invert_kept_is_parity_with_upstream() {
        // MEASURED (ADR-0151; phase-72 §5 state-5 LIVE-PROBE both proxies):
        // `present_match` (the PRESENCE mode, NOT a value matcher) with
        // `invert_match` + an ABSENT header is PARITY — envoy-rust AND upstream
        // BOTH KEEP. Upstream's present-check is `false` for a missing header and
        // `invert_match` DOES flip it (→ KEEP); since phase 75.1 the in-tree
        // engine computes `(present == want)` = `(false == true)` = false and
        // then XORs the invert, which also KEEPs. This mode does NOT diverge.
        // The phase-75.1 fixer PRESERVED this KEEP; any future refactor MUST
        // continue to — a naive uniform-DROP "fix" of the shared engine would
        // BREAK this parity case and introduce a NEW divergence. Contrast the
        // value-matcher companion
        // `pv4_value_matcher_absent_plus_invert_dropped_is_parity_with_upstream`.
        let hm = hm_inverted("x-log", HeaderMatcherMode::PresentMatch(true));
        assert!(
            hm.matches(&[]),
            "present_match absent+invert = KEEP on BOTH proxies (PARITY, not a divergence)"
        );
        // Same result through the access-log `HeaderMatch` seam.
        let via_trait: std::sync::Arc<dyn envoy_accesslog::HeaderMatch> =
            std::sync::Arc::new(hm_inverted("x-log", HeaderMatcherMode::PresentMatch(true)));
        assert!(
            via_trait.matches(&[]),
            "access-log present_match absent+invert = KEEP (PARITY)"
        );
    }

    #[test]
    fn header_match_trait_delegates_to_inherent_engine() {
        // Phase 72 (ADR-0150): the injected `HeaderMatch` trait impl must call
        // the inherent engine (NOT recurse). Exercise it through the trait object.
        let m: std::sync::Arc<dyn envoy_accesslog::HeaderMatch> =
            std::sync::Arc::new(hm("x-log", HeaderMatcherMode::ExactMatch("yes".into())));
        assert!(m.matches(&[h("x-log", "yes")]));
        assert!(!m.matches(&[h("x-log", "no")]));
        assert!(!m.matches(&[])); // absent → drop
        // invert now DROPS through the seam too: a VALUE matcher + invert +
        // absent = DROP, matching upstream (phase 75.1; CF-72-1 CLOSED). See
        // `pv4_value_matcher_absent_plus_invert_dropped_is_parity_with_upstream`.
        let inv: std::sync::Arc<dyn envoy_accesslog::HeaderMatch> = std::sync::Arc::new(
            hm_inverted("x-log", HeaderMatcherMode::ExactMatch("yes".into())),
        );
        assert!(!inv.matches(&[])); // value-matcher absent + invert = drop (parity)
    }

    #[test]
    fn value_matcher_present_match_resolved_semantics() {
        // §A1: match = present && want.  present_match:false NEVER matches.
        let t = ValueMatcher::PresentMatch(true);
        assert!(t.matches_resolved(Some("anything"))); // present && true
        assert!(!t.matches_resolved(None)); // absent
        let f = ValueMatcher::PresentMatch(false);
        assert!(!f.matches_resolved(Some("anything"))); // present && false → false
        assert!(!f.matches_resolved(None)); // absent
        // StringMatch via matches_resolved:
        let sm = ValueMatcher::StringMatch(StringMatcher {
            mode: StringMatcherMode::Exact("prod".into()),
            ignore_case: false,
        });
        assert!(sm.matches_resolved(Some("prod")));
        assert!(!sm.matches_resolved(Some("dev")));
        assert!(!sm.matches_resolved(None));
    }

    /// Phase 75.1 (ADR-0159): the full ABSENCE-SEMANTICS matrix for the shared
    /// engine — seven modes × {absent, present-matching, present-non-matching}
    /// × {invert, no-invert}, plus the empty-header-VALUE control.
    ///
    /// Every expectation below is the MEASURED upstream
    /// `envoyproxy/envoy:v1.33.0` verdict (`SPEC.md` §2.3, a 13-probe × 5-variant
    /// backend-free route matrix driven live against BOTH proxies). The rule:
    ///
    /// * `present_match(want)` is the ONLY mode evaluated with the header
    ///   ABSENT: `(present == want) ^ invert_match`.
    /// * every VALUE mode returns `false` when the header is absent —
    ///   `invert_match` is NOT applied to a missing header.
    /// * an EMPTY header VALUE counts as PRESENT.
    ///
    /// This matrix is the coverage whose absence let the `present_match: false`
    /// divergence (D2) survive from phase 04.2 to phase 75.1.
    #[test]
    fn absence_semantics_matrix_matches_measured_upstream() {
        let string_exact = |lit: &str| {
            HeaderMatcherMode::StringMatch(StringMatcher {
                mode: StringMatcherMode::Exact(lit.into()),
                ignore_case: false,
            })
        };

        // (label, mode, a value that MATCHES the mode, a value that does NOT)
        let value_modes: Vec<(&str, HeaderMatcherMode, &str, &str)> = vec![
            (
                "exact_match",
                HeaderMatcherMode::ExactMatch("v".into()),
                "v",
                "zzz",
            ),
            (
                "prefix_match",
                HeaderMatcherMode::PrefixMatch("v".into()),
                "v1",
                "zzz",
            ),
            (
                "suffix_match",
                HeaderMatcherMode::SuffixMatch("v".into()),
                "1v",
                "zzz",
            ),
            (
                "safe_regex_match",
                HeaderMatcherMode::SafeRegexMatch(compile("^v$")),
                "v",
                "zzz",
            ),
            (
                "range_match",
                HeaderMatcherMode::RangeMatch(Int64Range { start: 1, end: 10 }),
                "5",
                "zzz",
            ),
            ("string_match", string_exact("v"), "v", "zzz"),
        ];

        for (label, mode, hit, miss) in value_modes {
            // --- no invert ---
            let m = hm("x-a", mode.clone());
            assert!(m.matches(&[h("x-a", hit)]), "{label}: present+matching");
            assert!(
                !m.matches(&[h("x-a", miss)]),
                "{label}: present+non-matching"
            );
            assert!(!m.matches(&[]), "{label}: absent");

            // --- invert ---
            let mi = hm_inverted("x-a", mode.clone());
            assert!(
                !mi.matches(&[h("x-a", hit)]),
                "{label}+invert: present+matching"
            );
            assert!(
                mi.matches(&[h("x-a", miss)]),
                "{label}+invert: present+non-matching"
            );
            // THE D1 CELL. Upstream DROPS: a missing header is an unconditional
            // value no-match that `invert_match` does NOT resurrect.
            assert!(
                !mi.matches(&[]),
                "{label}+invert: ABSENT must be false — invert_match is NOT \
                 applied to a missing header (D1 / CF-72-1)"
            );

            // --- empty VALUE counts as PRESENT, so it takes the value path ---
            assert!(
                !m.matches(&[h("x-a", "")]),
                "{label}: empty value is PRESENT and fails the value match"
            );
            assert!(
                mi.matches(&[h("x-a", "")]),
                "{label}+invert: empty value is PRESENT, so invert DOES apply"
            );
        }

        // --- present_match: the ONLY mode evaluated on an absent header ---
        let pm_true = hm("x-a", HeaderMatcherMode::PresentMatch(true));
        assert!(pm_true.matches(&[h("x-a", "v")]), "present(true): present");
        assert!(
            pm_true.matches(&[h("x-a", "")]),
            "present(true): EMPTY VALUE counts as PRESENT"
        );
        assert!(!pm_true.matches(&[]), "present(true): absent");

        let pm_true_inv = hm_inverted("x-a", HeaderMatcherMode::PresentMatch(true));
        assert!(
            !pm_true_inv.matches(&[h("x-a", "v")]),
            "present(true)+invert: present"
        );
        // THE P1 GUARD CELL — MEASURED PARITY on both proxies. A uniform
        // "absent => DROP" fix breaks this and mints a NEW divergence.
        assert!(
            pm_true_inv.matches(&[]),
            "present(true)+invert: ABSENT must stay KEEP (P1 — MEASURED PARITY)"
        );

        // D2: upstream `present_match: false` means the header must be ABSENT.
        let pm_false = hm("x-a", HeaderMatcherMode::PresentMatch(false));
        assert!(
            !pm_false.matches(&[h("x-a", "v")]),
            "present(false): a PRESENT header must NOT match (D2)"
        );
        assert!(
            !pm_false.matches(&[h("x-a", "")]),
            "present(false): an EMPTY VALUE is PRESENT, so it must NOT match (D2)"
        );
        assert!(pm_false.matches(&[]), "present(false): absent matches");

        let pm_false_inv = hm_inverted("x-a", HeaderMatcherMode::PresentMatch(false));
        assert!(
            pm_false_inv.matches(&[h("x-a", "v")]),
            "present(false)+invert: present matches (D2, inverted)"
        );
        assert!(
            !pm_false_inv.matches(&[]),
            "present(false)+invert: absent does not match"
        );

        // The name match stays case-insensitive under the restructure.
        assert!(
            hm("X-A", HeaderMatcherMode::PresentMatch(true)).matches(&[h("x-a", "v")]),
            "header NAME matching stays case-insensitive"
        );
    }
}

#[cfg(test)]
mod metadata_match_tests {
    use crate::bootstrap::{
        MetadataMatcher, MetadataPathSegment, StringMatcher, StringMatcherMode, ValueMatcher,
    };
    use envoy_accesslog::MetadataMatch;
    use std::collections::BTreeMap;

    fn store(pairs: &[(&str, &str, &str)]) -> BTreeMap<String, BTreeMap<String, String>> {
        let mut m: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        for (ns, k, v) in pairs {
            m.entry((*ns).to_string())
                .or_default()
                .insert((*k).to_string(), (*v).to_string());
        }
        m
    }

    fn matcher(value: ValueMatcher) -> MetadataMatcher {
        MetadataMatcher {
            filter: "com.example".into(),
            path: vec![MetadataPathSegment { key: "k".into() }],
            value,
        }
    }

    fn exact(v: &str) -> ValueMatcher {
        ValueMatcher::StringMatch(StringMatcher {
            mode: StringMatcherMode::Exact(v.into()),
            ignore_case: false,
        })
    }

    #[test]
    fn resolution_contract_is_option_bool() {
        let m = matcher(exact("1"));
        // Resolved + matches → Some(true).
        assert_eq!(m.matches(&store(&[("com.example", "k", "1")])), Some(true));
        // Resolved + mismatch → Some(false) — NOT None. The caller must be able
        // to distinguish "value said no" from "path did not resolve", because
        // only the latter falls back to `match_if_key_not_found` (R-0.4).
        assert_eq!(m.matches(&store(&[("com.example", "k", "2")])), Some(false));
        // Missing KEY → None.
        assert_eq!(m.matches(&store(&[("com.example", "other", "1")])), None);
        // Missing NAMESPACE → None (MEASURED R-0.4: identical to a missing key).
        assert_eq!(m.matches(&store(&[("com.other", "k", "1")])), None);
        // Empty store → None.
        assert_eq!(m.matches(&store(&[])), None);
    }

    #[test]
    fn reuses_the_value_matcher_engine_verbatim() {
        // All FIVE modelled `StringMatcherMode` variants — Exact, Prefix,
        // Suffix, Contains and SafeRegex — route through
        // `ValueMatcher::matches` on the metadata path, plus `present_match`.
        //
        // Phase 74 §5.2 state-3 re-entry (`REVIEW.md` I-4): SafeRegex was the
        // one mode this test SKIPPED, while the comment here claimed full
        // coverage. That mattered more than the other four, because
        // `StringMatcher::matches` reaches SafeRegex through
        // `.expect("validator ensured StringMatcher SafeRegex compiled")` — a
        // REQUEST-TIME panic path, not a wrong-verdict path. Nothing anywhere
        // in the workspace evaluated a SafeRegex metadata value, so clearing
        // `compiled` failed no test. (The state-5 code-review live-probed it
        // clean cross-proxy, including at DEPTH 2 inside a composition, so this
        // pins CORRECT behavior — see the `safe_regex` block below.)
        let md = store(&[("com.example", "k", "prod-1")]);
        let case = |mode: StringMatcherMode, ignore_case: bool| {
            matcher(ValueMatcher::StringMatch(StringMatcher {
                mode,
                ignore_case,
            }))
            .matches(&md)
        };
        assert_eq!(
            case(StringMatcherMode::Exact("prod-1".into()), false),
            Some(true)
        );
        assert_eq!(
            case(StringMatcherMode::Prefix("prod".into()), false),
            Some(true)
        );
        assert_eq!(
            case(StringMatcherMode::Suffix("-1".into()), false),
            Some(true)
        );
        assert_eq!(
            case(StringMatcherMode::Contains("od-".into()), false),
            Some(true)
        );
        assert_eq!(
            case(StringMatcherMode::Exact("PROD-1".into()), true),
            Some(true)
        );
        assert_eq!(
            case(StringMatcherMode::Exact("PROD-1".into()), false),
            Some(false)
        );

        // SafeRegex — the fifth mode, and the only one whose evaluation can
        // PANIC rather than merely return the wrong verdict. `compiled` is
        // filled IN PLACE by the access-log validator
        // (`validate_access_log_metadata_matcher` → `compile_safe_regexes`), so
        // this helper compiles exactly as the validator does before matching.
        let safe_regex = |pattern: &str| {
            let mut v = ValueMatcher::StringMatch(StringMatcher {
                mode: StringMatcherMode::SafeRegex(crate::bootstrap::SafeRegex {
                    regex: pattern.to_string(),
                    compiled: None,
                }),
                ignore_case: false,
            });
            v.compile_safe_regexes().expect("pattern compiles");
            matcher(v).matches(&md)
        };
        assert_eq!(safe_regex("^prod-[0-9]+$"), Some(true));
        // A non-matching pattern must return Some(false) — NOT None. Only an
        // unresolved PATH yields None; a resolved value that the regex rejects
        // is a definite "no", so it must NOT fall back to
        // `match_if_key_not_found` (the same tri-state distinction
        // `resolution_contract_is_option_bool` pins for Exact).
        assert_eq!(safe_regex("^stage-[0-9]+$"), Some(false));
        // NB the anchoring/full-match semantics of `SafeRegex` itself are
        // phase-35/36 `StringMatcher` surface, NOT the metadata route, and are
        // deliberately not asserted here — this test's job is that SafeRegex
        // ROUTES through the engine, yields the tri-state correctly, and does
        // not panic on the metadata path.
        //
        // An ABSENT key short-circuits before the value matcher runs, so the
        // `.expect()` is never reached with a missing key.
        let mut unresolved = ValueMatcher::StringMatch(StringMatcher {
            mode: StringMatcherMode::SafeRegex(crate::bootstrap::SafeRegex {
                regex: "^prod-[0-9]+$".to_string(),
                compiled: None,
            }),
            ignore_case: false,
        });
        unresolved.compile_safe_regexes().expect("pattern compiles");
        assert_eq!(matcher(unresolved).matches(&store(&[])), None);

        // present_match (phase-36 §A1 semantics `match = present && want`): the
        // path RESOLVED here, so it reduces to `want`. An ABSENT key returns
        // None and takes `match_if_key_not_found` instead. **MEASURED
        // cross-proxy** at the phase-74 state-5 code-review (probe group 1,
        // sinks S4/S5 — exact complements, both proxies agreeing on every
        // cell), which CLOSED CF-74-5; `BEHAVIOR_CONTRACT.md` §G carries the
        // table.
        assert_eq!(
            matcher(ValueMatcher::PresentMatch(true)).matches(&md),
            Some(true)
        );
        assert_eq!(
            matcher(ValueMatcher::PresentMatch(false)).matches(&md),
            Some(false)
        );
        assert_eq!(
            matcher(ValueMatcher::PresentMatch(true)).matches(&store(&[])),
            None
        );
    }

    #[test]
    fn empty_path_resolves_to_none_rather_than_panicking() {
        // The validator guarantees `path.len() == 1` for every matcher that can
        // reach this impl (T2), so this is unreachable in a booted proxy — but
        // the impl uses `path.first()?` so a mis-wired caller degrades to
        // "unresolved" rather than panicking.
        let m = MetadataMatcher {
            filter: "com.example".into(),
            path: vec![],
            value: exact("1"),
        };
        assert_eq!(m.matches(&store(&[("com.example", "k", "1")])), None);
    }
}
