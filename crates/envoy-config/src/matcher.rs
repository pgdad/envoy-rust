//! HeaderMatcher / StringMatcher runtime. Per-matcher truth predicate
//! consumed by HCM's route walker (in crates/envoy-http1/src/hcm.rs).
//!
//! AND-combination across multiple HeaderMatchers on the same Route lives
//! in the route walker, not here — `HeaderMatcher::matches` is per-matcher.
//!
//! Phase 04.2.

use crate::bootstrap::{
    HeaderMatcher, HeaderMatcherMode, StringMatcher, StringMatcherMode, ValueMatcher,
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

        let mode_result = match &self.mode {
            HeaderMatcherMode::ExactMatch(lit) => value == Some(lit.as_str()),
            HeaderMatcherMode::PrefixMatch(lit) => {
                value.is_some_and(|v| v.starts_with(lit.as_str()))
            }
            HeaderMatcherMode::SuffixMatch(lit) => value.is_some_and(|v| v.ends_with(lit.as_str())),
            HeaderMatcherMode::SafeRegexMatch(sr) => value.is_some_and(|v| {
                sr.compiled
                    .as_ref()
                    .expect("validator ensured HeaderMatcher SafeRegex compiled")
                    .is_match(v)
            }),
            HeaderMatcherMode::RangeMatch(r) => value
                .and_then(|v| v.parse::<i64>().ok())
                .is_some_and(|n| n >= r.start && n < r.end),
            HeaderMatcherMode::PresentMatch(want_present) => {
                // present_match: true  → header must be present
                // present_match: false → no presence requirement (always true)
                // SPEC §6 signpost 7.
                if *want_present { value.is_some() } else { true }
            }
            HeaderMatcherMode::StringMatch(sm) => value.is_some_and(|v| sm.matches(v)),
        };

        mode_result ^ self.invert_match
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
    /// Returns true iff the resolved metadata value matches. String-only MVP: delegates to the
    /// inner `StringMatcher::matches` (phase 35).
    pub fn matches(&self, value: &str) -> bool {
        match self {
            ValueMatcher::StringMatch(sm) => sm.matches(value),
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
    fn present_match_false_returns_true_when_present() {
        // Subtle: present_match: false is "no presence requirement", always true.
        let m = hm("authorization", HeaderMatcherMode::PresentMatch(false));
        assert!(m.matches(&[h("authorization", "Bearer x")]));
    }
    #[test]
    fn present_match_false_returns_true_when_absent() {
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
        let m = hm_inverted("authorization", HeaderMatcherMode::PresentMatch(true));
        assert!(m.matches(&[]));
        assert!(!m.matches(&[h("authorization", "x")]));
    }
}
