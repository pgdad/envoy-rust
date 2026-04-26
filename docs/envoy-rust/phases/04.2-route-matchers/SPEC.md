# Phase 04.2 — HTTP route header matcher fan-out + ADR-0021 (regex permitted)

- **Phase id:** `04.2`
- **Parent phase:** `04-http1` (split per ADR-0020; `docs/envoy-rust/phases/04-http1/SPEC.md`, committed at SHA `805433e`)
- **Slug:** `04.2-route-matchers`
- **Title:** All 7 of Envoy's `HeaderMatcher` modes + `StringMatcher` tagged union + `invert_match: bool` on `Route.match.headers`; ADR-0021 lands `regex` as a narrowly-scoped permitted foundation for header / route matching
- **Depends on:** `04.1` (sub-phase `04.1-hcm-direct-response`). 04.1 lands the `RouteMatch` schema (with `prefix` + `path` axes) plus fixture `0007-http1-direct-response`; 04.2 extends `RouteMatch` additively with a `headers: Vec<HeaderMatcher>` axis and amends fixture 0007 to exercise a non-trivial matcher.
- **Differential surface when done:** **no new fixture.** The differential property — "envoy and envoy-rust select the same route given the same matchers" — is exercised implicitly by amending fixture 0007 (landed in 04.1) to add a second route under the same virtual host whose `match` carries a non-trivial `headers:` matcher. Two new request probes (one that hits the matcher route, one that falls through to the default route) drive the amended fixture; both proxies must select the same route on each probe. Pre-existing fixtures `0001-tcp-echo`, `0002-static-admin-ready`, `0003-tcp-proxy`, `0004-tls-downstream`, `0005-tls-upstream`, `0006-tls-sni`, and `0007-http1-direct-response` (in its 04.1-landed shape, plus the 04.2 amendment) all remain green.
- **Seeded by:** parent SPEC `docs/envoy-rust/phases/04-http1/SPEC.md` §3 D6.2 + D7.2 (the 04.2 deliverables: HeaderMatcher schema additions + ADR-0021); §4 (non-goals, a subset of which 04.2 inherits); §5 (the 3-way split rationale that placed the matcher fan-out in its own sub-phase); §6 signpost 8 (matcher impl shape); §7 (ADR-0020 lands at parent-04 state-2 = the commit landing this SPEC; ADR-0021 lands at 04.2 Task 1).

This SPEC is the design contract for sub-phase 04.2. The next session converts it into `PLAN.md` per the phase lifecycle (§5 of `BOOTSTRAP_PROMPT.md` / `SKILL_ROUTING.md`). It is self-contained per doctrine D-3.4; a stranger reading only this file plus the stable doctrine documents (`MISSION.md`, `BEHAVIOR_CONTRACT.md`, `DECISIONS.md`, `BOOTSTRAP_PROMPT.md`) and the landed phase-04.1 surface (via `git log` and the in-tree `envoy-config` / `envoy-http1` shape at the 04.1 phase-done commit) must be able to execute it without consulting the parent `04-http1/SPEC.md`.

---

## 1. Goal and acceptance signal

**Goal.** Land all 7 of Envoy's `HeaderMatcher` modes + the `StringMatcher` tagged union + `invert_match: bool`, exposed on `Route.match.headers: Vec<HeaderMatcher>` (additively extending the 04.1-landed `RouteMatch` schema). Add a runtime `HeaderMatcher::matches(headers) -> bool` method consumed by the route-walker in HCM (which already lives in `crates/envoy-http1/` per 04.1's D3.1 placement decision). Land **ADR-0021** at 04.2 Task 1 narrowly permitting `regex = "1"` as a runtime dependency on `envoy-config` for `HeaderMatcher::SafeRegex` + `StringMatcher::SafeRegex` compilation at config-load time (mirrors phase 03.1's task-1 ADR-0018 + ADR-0019 inline-landing pattern).

The 7 modes in scope (Envoy v1.33.0 `envoy.config.route.v3.HeaderMatcher` proto):

1. `exact_match: String` — value equals literal (case-sensitive on the value).
2. `prefix_match: String` — value starts with literal (case-sensitive).
3. `suffix_match: String` — value ends with literal (case-sensitive).
4. `safe_regex_match: SafeRegex` — value matches a `regex::Regex` compiled at config-load time.
5. `range_match: Int64Range` — value parses as `i64` (decimal) and falls in the half-open interval `[start, end)`.
6. `present_match: bool` — see §6 signpost 7 for the subtle `false` semantics.
7. `string_match: StringMatcher` — Envoy's modern generic tagged-union (5 variants: `Exact`, `Prefix`, `Suffix`, `SafeRegex`, `Contains`; each carries `ignore_case: bool` semantics on the matched value).

`invert_match: bool` (default `false`) sits on the outer `HeaderMatcher` struct and inverts the entire mode-specific match result.

The 04.1-landed fixture `0007-http1-direct-response` is amended in 04.2 to add a second route under the existing virtual host: `match: { prefix: "/api/", headers: [{ name: "x-foo", exact_match: "bar" }] }` returning `direct_response: { status: 418, body: { inline_string: "teapot\n" } }`. The first route stays as it was (prefix `/`, direct_response 200 `"ok\n"`). A new request probe that includes `X-Foo: bar` is added to inputs/ and exercises the matcher route; the existing `GET /healthz` probe still falls through to the first route. Both proxies must select the same route on each probe — that is the differential property 04.2 newly exercises.

There is no new fixture in 04.2. Matchers are purely config-side: they introduce no new response headers, no new wire format, and no new route action. The fixture-0007 amendment is the minimum viable production-use demonstration; per the parent SPEC §3 D6.2 the rest of the matcher coverage lives in ~25 schema/runtime unit tests + 1 fuzz-corpus seed extension.

**Acceptance signal** — the phase-done gate from §7.5 of `BOOTSTRAP_PROMPT.md`, scoped to 04.2's feature surface:

- (a) the 04.1-landed fixture `tests/fixtures/0007-http1-direct-response/` remains green **after** the matcher-bearing route is added (proves the matcher works in production on both proxies);
- (b) the pre-existing differential fixtures `tests/fixtures/0001-tcp-echo/`, `tests/fixtures/0002-static-admin-ready/`, `tests/fixtures/0003-tcp-proxy/`, `tests/fixtures/0004-tls-downstream/`, `tests/fixtures/0005-tls-upstream/`, `tests/fixtures/0006-tls-sni/` remain green;
- (c) no conformance suites run this sub-phase (first one — `h2spec` — attaches in phase 05);
- (d) the existing fuzz target `parse_bootstrap` runs clean for its short-budget CI run (`cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30`) against the corpus extended in 04.2 with one new HeaderMatcher-shaped seed (`route_with_header_matchers.yaml`); no new fuzz target ships;
- (e) `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, and **`cargo deny check`** are all clean on the stable-toolchain CI job. The `cargo deny check` clearance is load-bearing: ADR-0021 introduces `regex = "1"` plus its transitive surface (`regex-syntax`, `memchr`, `aho-corasick`); per ADR-0021's consequences section the licenses (MIT/Apache-2.0/Unlicense) are already on the deny.toml allow-list, but the plan-writer cross-checks at 04.2 Task 1 alongside the ADR landing;
- (f) `REVIEW.md` for this sub-phase is approved.

The 04.2 phase-done commit flips ROADMAP row `04.2` from `in-progress` to `done` (parent row `04` stays `in-progress` until 04.3's phase-done commit, per the ROADMAP-schema invariant "parent flips to `done` only after all sub-phases are `done`"). STATE.md advances to phase `04.3` lifecycle state 3 (04.3's SPEC was already landed at parent-04 state-2 alongside 04.1's and 04.2's SPECs in this same commit; the next session runs `superpowers:writing-plans` scoped to sub-phase 04.3).

---

## 2. Behavior-contract scope for sub-phase 04.2

**No `BEHAVIOR_CONTRACT.md` edits in 04.2.** The `Header allow-list` section (populated in 04.1 with `server` + `date`; extended in 04.3 with `x-envoy-upstream-service-time` per parent SPEC §2's full-table projection) is not edited in 04.2. Matchers are **config-side**: they do not introduce new response headers, do not change response status semantics for routes that already match, and do not change response body framing. The matcher just selects which route the HCM dispatches; the chosen route's `direct_response` already produces a response shape covered by 04.1's allow-list.

Equivalence-matrix dimensions touched by the fixture-0007 amendment (per `BEHAVIOR_CONTRACT.md` §7.2):

- Row 1 (Response status). The matcher route returns `418 I'm a teapot`; the default route returns `200 OK`. Both proxies must return the same status on each probe — the `equivalence.response_status` opt-in already enabled by fixture 0007 in 04.1 covers this.
- Row 2 (Response body). The matcher route returns `"teapot\n"` byte-exact; the default route returns `"ok\n"` byte-exact. Both static `direct_response` bodies; covered by 04.1's `byte_exact` body equivalence.
- Row 3 (Response headers). The matcher route's response carries the same allow-listed header set as the default route's response (`server`, `date`, `content-length`, `content-type`, `connection`); no new header introduced. The 04.1-landed `HEADER_ALLOW_LIST` constant in `tests/differential/src/lib.rs` is unchanged.

The currently-empty `Stat-name mapping`, `Access log field mapping`, `xDS wire state machine`, `Timing tolerances` sections of `BEHAVIOR_CONTRACT.md` remain empty.

---

## 3. Deliverables

### D1 — `envoy-config` schema additions for `HeaderMatcher` + `StringMatcher`

`crates/envoy-config/src/bootstrap.rs` gains the full `HeaderMatcher` type tree, the `StringMatcher` tagged union, the `Int64Range` value type, and an additive `headers: Vec<HeaderMatcher>` field on the existing `RouteMatch` struct (which 04.1 lands with `prefix` + `path` axes only).

The full schema:

```rust
/// One header-matching predicate. AND-combined with sibling HeaderMatchers
/// in `RouteMatch.headers` (per Envoy v1.33.0 default `headers_match_options:
/// ALL`; see §6 signpost 3).
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HeaderMatcher {
    /// Header name. Matched case-insensitively against the request's header
    /// names per HTTP/1.1 RFC 7230 §3.2 (see §6 signpost 4). Empty string is
    /// rejected by the validator with ConfigError::EmptyHeaderName.
    pub name: String,

    /// The mode discriminator. The Envoy proto uses field-name oneof shape
    /// (the discriminator is *which* of the seven mode fields is present);
    /// serde tagged-enum doesn't directly model this, so the parsed form goes
    /// through a hand-rolled Deserialize impl that inspects the YAML mapping
    /// keys and dispatches to the matching variant. See §6 signpost 1 for
    /// the implementation shape and a worked example.
    pub mode: HeaderMatcherMode,

    /// If true, the entire mode-specific match result is inverted (NOT after
    /// the mode match). See §6 signpost 5.
    #[serde(default)]
    pub invert_match: bool,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub enum HeaderMatcherMode {
    /// `exact_match: <string>` — value equals literal (case-sensitive on the
    /// value; the header name match is always case-insensitive per HTTP/1.1).
    ExactMatch(String),

    /// `prefix_match: <string>` — value starts with literal.
    PrefixMatch(String),

    /// `suffix_match: <string>` — value ends with literal.
    SuffixMatch(String),

    /// `safe_regex_match: { regex: "<pattern>" }` — value matches the regex.
    /// Compiled at config-load time into Arc<regex::Regex>; the validator
    /// rejects unparseable patterns with ConfigError::InvalidRegex.
    SafeRegexMatch(SafeRegex),

    /// `range_match: { start: <i64>, end: <i64> }` — value parses as i64
    /// (decimal) and falls in [start, end). Non-parseable values fail the
    /// match (NOT an error — see §6 signpost 6).
    RangeMatch(Int64Range),

    /// `present_match: <bool>` — header presence (true) or "no presence
    /// requirement" (false; see §6 signpost 7 for the subtle false semantics).
    PresentMatch(bool),

    /// `string_match: <StringMatcher>` — Envoy's modern generic tagged-union
    /// (the only path to Contains; §6 signpost 8).
    StringMatch(StringMatcher),
}

/// Reference to a regex pattern. Held both as the original String (for
/// re-serialization / equality / debugging) and the compiled Arc<regex::Regex>
/// (for cheap clone + zero-cost matching). The compiled form is *not* a serde
/// field; it's filled in by the envoy-config validator after deserialization.
#[derive(Debug, Clone)]
pub struct SafeRegex {
    pub regex: String,
    /// Filled in by the validator (envoy_config::bootstrap::validate). At
    /// deserialization time this is None; after a successful validate() call
    /// it's Some(Arc<regex::Regex>). Consumers (the route walker in HCM) take
    /// the .as_ref().expect("validator ensured compiled") shape, mirroring
    /// phase 02.1's "validator ensured cluster present" precedent.
    pub compiled: Option<std::sync::Arc<regex::Regex>>,
}

/// Custom Deserialize: only reads `regex: String`; sets `compiled: None`.
/// Validator extension fills the compiled form.
impl<'de> serde::Deserialize<'de> for SafeRegex { /* ... */ }

/// Custom PartialEq: compares only the `regex` String. Compiled regex has no
/// stable equality (regex::Regex doesn't impl PartialEq).
impl PartialEq for SafeRegex {
    fn eq(&self, other: &Self) -> bool { self.regex == other.regex }
}

/// Half-open i64 range. Validator rejects start >= end with
/// ConfigError::InvalidInt64Range.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Int64Range {
    pub start: i64,
    pub end: i64,
}

/// Envoy's modern generic StringMatcher (proto:
/// envoy.type.matcher.v3.StringMatcher). Field-name oneof shape mirrors
/// HeaderMatcher; same hand-rolled Deserialize approach (§6 signpost 1).
/// `ignore_case` is an outer field (per Envoy proto: it's a peer of the mode
/// discriminator, not a per-variant field) that controls case sensitivity of
/// the value match. Defaults to `false`. Has no effect on the regex variant
/// (regex callers express case insensitivity via the `(?i)` inline flag).
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StringMatcher {
    #[serde(flatten)]
    pub mode: StringMatcherMode,
    #[serde(default)]
    pub ignore_case: bool,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub enum StringMatcherMode {
    /// `exact: <string>`.
    Exact(String),
    /// `prefix: <string>`.
    Prefix(String),
    /// `suffix: <string>`.
    Suffix(String),
    /// `safe_regex: { regex: "<pattern>" }`.
    SafeRegex(SafeRegex),
    /// `contains: <string>` — substring match. Only reachable through
    /// HeaderMatcherMode::StringMatch(StringMatcher::Contains(...)); there is
    /// no top-level HeaderMatcherMode::ContainsMatch (Envoy v1.33.0 only
    /// supports Contains via the modern string_match field; see §6 signpost 8).
    Contains(String),
}

// On RouteMatch — additive 04.2 extension to the 04.1-landed struct:
pub struct RouteMatch {
    // … existing 04.1 fields (oneof prefix / path) …
    /// Empty Vec means "no header constraints" (the route matches as long as
    /// the path-side oneof matches). Non-empty Vec means ALL HeaderMatchers
    /// must match (AND semantics; §6 signpost 3).
    #[serde(default)]
    pub headers: Vec<HeaderMatcher>,
}
```

**Worked YAML → Rust example.** Per parent SPEC §3 D6.2 and §6 signpost 1 the field-name oneof shape decision warrants a worked example so the planner has the wire shape pinned at SPEC writeup:

```yaml
match:
  prefix: "/api/"
  headers:
    - name: "x-foo"
      exact_match: "bar"
      invert_match: false
    - name: "x-version"
      range_match: { start: 1, end: 100 }
    - name: "x-tag"
      string_match:
        contains: "beta"
        ignore_case: true
    - name: "authorization"
      present_match: true
```

deserializes to:

```rust
RouteMatch {
    prefix: Some("/api/".into()),
    path: None,
    headers: vec![
        HeaderMatcher {
            name: "x-foo".into(),
            mode: HeaderMatcherMode::ExactMatch("bar".into()),
            invert_match: false,
        },
        HeaderMatcher {
            name: "x-version".into(),
            mode: HeaderMatcherMode::RangeMatch(Int64Range { start: 1, end: 100 }),
            invert_match: false,
        },
        HeaderMatcher {
            name: "x-tag".into(),
            mode: HeaderMatcherMode::StringMatch(StringMatcher {
                mode: StringMatcherMode::Contains("beta".into()),
                ignore_case: true,
            }),
            invert_match: false,
        },
        HeaderMatcher {
            name: "authorization".into(),
            mode: HeaderMatcherMode::PresentMatch(true),
            invert_match: false,
        },
    ],
}
```

**Validator extensions** in `envoy-config::bootstrap::validate` — new `ConfigError` variants:

- `EmptyHeaderName` — `HeaderMatcher.name` must be non-empty.
- `InvalidRegex { source: regex::Error }` — `SafeRegex.regex` failed `regex::Regex::new`. Carries the underlying `regex::Error` for diagnostic context. Compiled `Arc<regex::Regex>` is stored back on the `SafeRegex.compiled` field on success.
- `InvalidInt64Range { start: i64, end: i64 }` — `Int64Range.start >= Int64Range.end` (the half-open interval would be empty).

The validator runs at config-load time (`envoy_config::load_bootstrap`); compiled regexes are stored on the parsed `SafeRegex`'s `compiled: Option<Arc<regex::Regex>>` field per §6 signpost 9. Validator extensions per matcher mode are mechanical (most modes carry no further validation beyond the mode-specific Deserialize bounds; the regex + range modes are the two with substantive validate-arms).

`deny_unknown_fields` regression guards land on every new struct (`HeaderMatcher`, `Int64Range`, `StringMatcher`) — ensures Envoy-side config keys outside the 04.2-supported set are rejected at parse time rather than silently ignored. The seven `HeaderMatcherMode` discriminator keys (`exact_match`, `prefix_match`, `suffix_match`, `safe_regex_match`, `range_match`, `present_match`, `string_match`) are owned by the hand-rolled Deserialize impl — anything else as the discriminator key produces a `ConfigError::UnknownHeaderMatcherMode { got: String }` (added alongside the three above).

**Validator unit tests appended to `crates/envoy-config/src/bootstrap.rs::tests` (~20 tests):**

Per-mode happy path (one each):

- `parses_route_with_exact_match_header` — `exact_match: "bar"` + `invert_match: false` (default) round-trips.
- `parses_route_with_prefix_match_header`
- `parses_route_with_suffix_match_header`
- `parses_route_with_safe_regex_match_header` — `safe_regex_match: { regex: "^v[0-9]+$" }` parses, validator compiles, `compiled.is_some()`.
- `parses_route_with_range_match_header` — `range_match: { start: 1, end: 100 }` with `start < end`.
- `parses_route_with_present_match_true_header` and `..._false_header` (two tests; `false` is parseable per Envoy proto).
- `parses_route_with_string_match_exact_header` — `string_match: { exact: "foo", ignore_case: true }`.
- `parses_route_with_string_match_contains_header` — `contains: "beta"`.
- `parses_route_with_string_match_safe_regex_header` — `safe_regex: { regex: "..." }`.

Validator-error paths:

- `rejects_empty_header_name` — `name: ""` → `EmptyHeaderName`.
- `rejects_invalid_regex_in_safe_regex_match` — `regex: "[unclosed"` → `InvalidRegex { source: regex::Error }`.
- `rejects_invalid_regex_in_string_match_safe_regex` — same pattern through the StringMatcher path.
- `rejects_invalid_int64_range_start_eq_end` — `start: 100, end: 100` → `InvalidInt64Range { start: 100, end: 100 }`.
- `rejects_invalid_int64_range_start_gt_end` — `start: 200, end: 100` → same.
- `rejects_unknown_header_matcher_mode` — `name: "x-foo", weird_match: "bar"` → `UnknownHeaderMatcherMode { got: "weird_match" }`.
- `rejects_unknown_field_in_header_matcher` — `name: "x-foo", exact_match: "bar", future_field: 1` → serde deny_unknown_fields error.
- `rejects_unknown_field_in_int64_range` — `start: 1, end: 100, step: 5` → serde deny_unknown_fields error.
- `rejects_unknown_field_in_string_matcher` — `exact: "foo", future_knob: true` → serde deny_unknown_fields error (note: `ignore_case` IS allowed; this rejects unknown peers).

Multi-matcher AND semantics (one round-trip test for the parsed shape; the runtime behavior is in D2's matcher-runtime tests):

- `parses_route_with_multiple_header_matchers` — `headers: [matcher1, matcher2]` round-trips into a 2-element Vec preserving order.

Total: ~20 envoy-config validator tests. ~50 LoC schema + ~80 LoC validator (compile loop + per-mode arms) + ~20 unit tests.

**Re-exports in `crates/envoy-config/src/lib.rs`** — add `HeaderMatcher`, `HeaderMatcherMode`, `SafeRegex`, `Int64Range`, `StringMatcher`, `StringMatcherMode` to the crate's public surface so HCM's route walker can name them. Extend `ConfigError` enum with the four new variants (`EmptyHeaderName`, `InvalidRegex`, `InvalidInt64Range`, `UnknownHeaderMatcherMode`).

**Fuzz corpus extension.** `crates/envoy-config/fuzz/corpus/parse_bootstrap/` gains 1 new seed:

- `route_with_header_matchers.yaml` — full bootstrap with a listener → HCM filter chain → route_config carrying a single VirtualHost with a single Route whose `match.headers` exercises **5 of the 7 modes simultaneously** (exact_match, safe_regex_match, range_match, present_match, string_match-with-contains-and-ignore_case). The seventh mode (suffix_match) and the prefix_match mode show up via the existing happy-path 04.1 seeds being mutated by the fuzzer; the goal of this seed is to exercise serde's hand-rolled Deserialize on the field-name oneof against all the substantive modes simultaneously. Plausible-but-irrelevant regex pattern (`^v[0-9]+$`); the fuzzer never executes it (the parse_bootstrap target only exercises serde + the validator's `regex::Regex::new` compile, not header matching).

**Matcher runtime — `HeaderMatcher::matches`.** Added as an inherent method on the parsed `HeaderMatcher` struct (lives in the same `bootstrap.rs` file as the schema; or a sibling `matcher.rs` module if the planner prefers — module decomposition decided at PLAN.md writeup, but the visibility is `pub`):

```rust
impl HeaderMatcher {
    /// Returns true iff this matcher matches the given header set. The
    /// `headers` slice is the request's headers as the HCM has them post-parse
    /// (Vec<(String, String)> ordered by emission order; case-preserving).
    /// Header NAME matching is case-insensitive per HTTP/1.1 §3.2; header
    /// VALUE matching is case-sensitive by default (StringMatcher.ignore_case
    /// flips it for the value).
    ///
    /// AND semantics across multiple HeaderMatchers on a Route is implemented
    /// by the route walker (HCM), not here — this method is per-matcher.
    pub fn matches(&self, headers: &[(String, String)]) -> bool {
        let value = headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(&self.name))
            .map(|(_, v)| v.as_str());

        let mode_result = match &self.mode {
            HeaderMatcherMode::ExactMatch(lit) => value == Some(lit.as_str()),
            HeaderMatcherMode::PrefixMatch(lit) => value.is_some_and(|v| v.starts_with(lit.as_str())),
            HeaderMatcherMode::SuffixMatch(lit) => value.is_some_and(|v| v.ends_with(lit.as_str())),
            HeaderMatcherMode::SafeRegexMatch(re) => value.is_some_and(|v| {
                re.compiled.as_ref().expect("validator ensured compiled").is_match(v)
            }),
            HeaderMatcherMode::RangeMatch(r) => value
                .and_then(|v| v.parse::<i64>().ok())
                .is_some_and(|n| n >= r.start && n < r.end),
            HeaderMatcherMode::PresentMatch(want_present) => {
                // present_match: true  → header must be present
                // present_match: false → no presence requirement (always true)
                //                        per Envoy proto semantics; §6 signpost 7
                if *want_present { value.is_some() } else { true }
            }
            HeaderMatcherMode::StringMatch(sm) => value.is_some_and(|v| sm.matches(v)),
        };

        mode_result ^ self.invert_match
    }
}

impl StringMatcher {
    pub fn matches(&self, value: &str) -> bool {
        let (haystack, needle_or_pattern): (std::borrow::Cow<'_, str>, std::borrow::Cow<'_, str>) =
            if self.ignore_case {
                (value.to_ascii_lowercase().into(), /* lowercase needle below per variant */)
            } else {
                (value.into(), /* needle as-is */)
            };
        // ... per-variant match (Exact / Prefix / Suffix / SafeRegex / Contains)
        // SafeRegex ignores `ignore_case` (use `(?i)` inline flag instead).
        unimplemented!() // illustrative; full impl in PLAN.md
    }
}
```

Matcher-runtime unit tests appended near the schema tests (~25 tests; each test constructs a `HeaderMatcher` value directly, calls `.matches(&[(name, val), ...])`, asserts the boolean):

Per-mode boolean truth tables (~3 tests each: present-and-matches, present-and-doesn't-match, absent):

- `exact_match_matches_value`, `exact_match_rejects_value`, `exact_match_absent_returns_false`.
- Same triple for prefix, suffix.
- `safe_regex_match_matches_value` (compiled at the test setup time via `validate(&mut HeaderMatcher)`), `safe_regex_match_rejects_value`, `safe_regex_match_absent_returns_false`.
- `range_match_value_in_range_returns_true`, `range_match_value_below_start_returns_false`, `range_match_value_at_end_returns_false` (half-open: end is exclusive), `range_match_value_above_end_returns_false`, `range_match_non_parseable_value_returns_false` (matcher fails, not an error).
- `present_match_true_returns_true_when_present`, `present_match_true_returns_false_when_absent`, `present_match_false_returns_true_when_present`, `present_match_false_returns_true_when_absent` (the subtle `false` case — see §6 signpost 7).
- `string_match_contains_returns_true`, `string_match_contains_with_ignore_case_returns_true_on_uppercase`, `string_match_safe_regex_ignore_case_no_effect` (regex callers use `(?i)`; `ignore_case: true` doesn't change SafeRegex matching — Envoy-compat behavior).

Cross-cutting:

- `header_name_match_is_case_insensitive` — matcher `name: "X-Foo"` matches request header `("x-foo", "bar")`.
- `header_value_match_is_case_sensitive_by_default` — matcher `exact_match: "bar"` does NOT match request header `("x-foo", "BAR")`.
- `invert_match_inverts_exact_match_result` — `exact_match: "bar", invert_match: true`; matches when value is `"baz"`, doesn't match when value is `"bar"`.
- `invert_match_inverts_present_match_result` — `present_match: true, invert_match: true`; matches when header is absent, doesn't match when header is present.

Total D1: ~150 LoC schema + ~80 LoC matcher runtime + ~80 LoC validator + ~20 envoy-config validator tests + ~25 matcher-runtime unit tests + 1 fuzz seed. ~600 LoC.

### D2 — ADR-0021 (`regex` permitted as a foundation for header / route matching)

**Lands at 04.2 Task 1**, alongside the runtime-dep addition to `crates/envoy-config/Cargo.toml`. Mirrors phase 03.1 Task 1's ADR-0018 + ADR-0019 inline-landing pattern (cf. ADR-0018's provenance footer in `docs/envoy-rust/DECISIONS.md`).

Provenance: this ADR was projected as the next-sequential available ADR number in parent-04 SPEC §7 (`docs/envoy-rust/phases/04-http1/SPEC.md`, committed at SHA `805433e`); ADR-0020 (the parent-04 split decision) lands at parent-04 state-2 = the commit landing this SPEC; ADR-0021 lands at 04.2 Task 1.

Scope (verbatim from the parent SPEC §7 D2):

- Narrowly permits `regex = "1"` as a runtime dep on `envoy-config` for `HeaderMatcher::SafeRegex` + `StringMatcher::SafeRegex` compilation at config-load time.
- **NOT** permitted for general-purpose use elsewhere. Future filter-framework regex needs (e.g., URL path templates, Lua filter `string.find`, header-rewrite patterns) require an explicit scope-extension ADR.

Cargo dep added: `regex = "1"` to `crates/envoy-config/Cargo.toml`'s runtime `[dependencies]` section (sibling to `serde`, `serde_yaml`, `thiserror`). No envoy-config dev-dep changes.

Consequences:

- `cargo deny check` remains clean. `regex` is dual-licensed MIT/Apache-2.0 (already on the deny.toml allow-list since phase 00). Its main transitive deps — `regex-syntax` (MIT/Apache-2.0), `aho-corasick` (MIT/Unlicense), `memchr` (MIT/Unlicense) — are all on the allow-list. Plan-writer cross-checks at 04.2 Task 1 and updates `deny.toml` only if a fresh transitive license surfaces (not anticipated).
- `Cargo.lock` gains the `regex` + transitive surface as a dedicated commit at the 04.2 state-4 phase-done gate (mirrors the established phase precedent: phase-01 `4955252`, phase-02.1 `dea4d16`, phase-02.2 `2146014`, phase-03.1 `eb039e6`, phase-03.2 `85633e7`; cf. parent-04 SPEC §8 "Cargo.lock — synced as a dedicated commit at each sub-phase's state-4 phase-done gate per the established phase-precedent").
- envoy-config's binary size grows; not measured (the project has no bin-size budget).
- Future filter-framework phases that want regex (e.g., phase 07's HTTP filter chain framework if it admits a regex-driven rewrite filter) land their own scope-extension ADR explicitly citing ADR-0021's narrow scope.

Full ADR text (~25 lines following the ADR-0018/ADR-0019 template) lands in `docs/envoy-rust/DECISIONS.md` at 04.2 Task 1; see §7 of this SPEC for the projected text.

### D3 — Fixture 0007 amendment

The 04.1-landed fixture `tests/fixtures/0007-http1-direct-response/` is amended in 04.2 to add a second route with a non-trivial `headers:` matcher, demonstrating production matcher use. Edits both `envoy.yaml` and `envoy-rust.yaml` (the two YAMLs are identical except for the per-side divergences from 04.1: bind address, admin block, etc.).

The amended `route_config` (illustrative for `envoy.yaml`; `envoy-rust.yaml` carries the same `routes:` shape):

```yaml
route_config:
  name: local_route
  virtual_hosts:
    - name: local_service
      domains: ["*"]
      routes:
        # 04.2 NEW route — placed first so first-match-wins reaches it before
        # the catch-all. The matcher selects this route only when the request
        # path starts with "/api/" AND the X-Foo header equals "bar".
        - match:
            prefix: "/api/"
            headers:
              - name: "x-foo"
                exact_match: "bar"
          direct_response:
            status: 418
            body:
              inline_string: "teapot\n"
        # 04.1 PRE-EXISTING route — unchanged. Catch-all default.
        - match:
            prefix: "/"
          direct_response:
            status: 200
            body:
              inline_string: "ok\n"
```

The route ordering matters: per parent SPEC §3 D3.1 the route walker is single-pass first-match-wins. The matcher route must come first so that requests carrying `X-Foo: bar` to `/api/...` reach it; otherwise the catch-all `prefix: "/"` would match first and the matcher route would be unreachable.

`tests/fixtures/0007-http1-direct-response/inputs/` gains a second probe input:

- `inputs/payload.bin` — unchanged (the existing 04.1 `GET /healthz HTTP/1.1\r\nHost: envoy-rust.test\r\nContent-Length: 0\r\n\r\n` request bytes; falls through to the default route).
- `inputs/payload-matcher.bin` — new; serialized `GET /api/widgets HTTP/1.1\r\nHost: envoy-rust.test\r\nX-Foo: bar\r\nContent-Length: 0\r\n\r\n` request bytes; hits the matcher route.

`tests/fixtures/0007-http1-direct-response/expectations.yaml` is restructured from a single-probe shape to a two-probe shape:

```yaml
probes:
  - name: default-route
    input: inputs/payload.bin
    driver:
      kind: http1
      method: GET
      path: "/healthz"
      host: "envoy-rust.test"
    equivalence:
      response_status: { expected: 200 }
      response_body: { byte_exact: "ok\n" }
      response_headers: { rule: set_equal_modulo_allow_list }
  - name: matcher-route
    input: inputs/payload-matcher.bin
    driver:
      kind: http1
      method: GET
      path: "/api/widgets"
      host: "envoy-rust.test"
      extra_headers: [["X-Foo", "bar"]]
    equivalence:
      response_status: { expected: 418 }
      response_body: { byte_exact: "teapot\n" }
      response_headers: { rule: set_equal_modulo_allow_list }
```

The exact `expectations.yaml` schema is whatever 04.1 lands; this SPEC names the shape conceptually. If 04.1's `expectations.yaml` schema is single-probe-only, the harness extension to support `probes: [...]` lists is also part of D3 (~30 LoC in `tests/differential/src/lib.rs`'s `run_fixture` to iterate over a Vec of probes). If 04.1 already lands a probe-list shape (per parent SPEC §3 D5.1's projection — the `Driver::Http1` variant carries `expected_status: Option<u16>` etc., which suggests single-probe; the planner cross-checks at 04.2 SPEC writeup time and either uses 04.1's existing shape or extends it).

`tests/fixtures/0007-http1-direct-response/README.md` is amended with one paragraph noting the 04.2-added matcher route + its property (the matcher demonstrates production matcher use; both proxies must select the same route given the same request); ADR-0021 is added to the README's ADR-references list.

Total D3: ~50 LoC fixture diff (envoy.yaml + envoy-rust.yaml route addition; new inputs/payload-matcher.bin; expectations.yaml restructure; README.md paragraph) + up to ~30 LoC harness probe-list extension if needed. Total: ~80 LoC.

### D4 — Phase-04.1 REVIEW carryforwards (status check; expected no action in 04.2)

Per BOOTSTRAP_PROMPT.md §7.5 each sub-phase REVIEW evaluates the previous sub-phase's open carryforwards. 04.1's REVIEW.md will likely surface its own M-tier observations at the time it lands; this SPEC anticipates the standard set:

- **HCM placement decision (parent SPEC §6 signpost 17)** — recommended at parent-04 state-2 to place HCM in `envoy-http1`. 04.2 does not touch HCM placement.
- **Hand-rolled IMF-fixdate writer vs. `httpdate` (parent SPEC §6 signpost 4)** — 04.2 does not touch the date writer.
- **Header allow-list (parent SPEC §2)** — 04.2 adds nothing new (matchers are config-side; no new response headers).
- **`Cluster::name()` accessor M1 carryforward (parent SPEC §6 signpost 19)** — evaluated in 04.3, not 04.2.

If 04.1's REVIEW lands with concrete M-tier or higher observations beyond this anticipated set, 04.2's PLAN.md adds a `task 0` carryforward step per the established phase pattern.

### D5 — CI workflow

`.github/workflows/ci.yml` changes: **none** in 04.2. The existing `build` job runs `cargo test --workspace`, which picks up the new envoy-config matcher tests automatically. The existing `fuzz` job exercises the extended `parse_bootstrap` corpus via the same `cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30` invocation (now covering 1 new HeaderMatcher-shaped seed). The Docker-gated integration test for fixture 0007 (landed in 04.1) keeps its existing `#[ignore]`-unless-`DOCKER=1` gate; the amended fixture exercises through the same gate.

---

## 4. Non-goals (deferred to 04.3 or later phases)

Deferred explicitly to sub-phase 04.3:

- **Upstream HTTP/1.1 origination (`envoy-http1::Client`)** — sub-phase 04.3's D8.3.
- **Router filter's `Route(RouteAction_Route)` arm** — sub-phase 04.3's D9.3. 04.2 does NOT touch the router filter beyond the matcher integration in HCM's route walker (matcher-runtime hookup).
- **Helper crate `tests/helpers/http1-echo-server/`** — sub-phase 04.3's D10.3.
- **Fixture `0008-http1-router-upstream`** — sub-phase 04.3's D11.3.
- **Header allow-list extension for `x-envoy-upstream-service-time`** — sub-phase 04.3 (per parent SPEC §2 full-table projection: this header lands when 04.3's router proxy arm starts emitting it).
- **`Cluster::name()` opportunistic close-out (M1 carryforward)** — sub-phase 04.3's D12.3.
- **Phase-04 parent ROADMAP row flip to `done`** — happens at sub-phase 04.3's final commit, not 04.2's.

Deferred to later phases (a subset inherited from parent SPEC §4; only the items relevant to the matcher surface are reproduced here):

- **HTTP/2 and HTTP/3.** Phase 05 (HTTP/2) and the QUIC family. 04.2 still rejects `codec_type: HTTP2` / `HTTP3` via the validator path landed in 04.1.
- **HTTP filter chain framework** (per-route config; iteration protocol with `Continue` / `StopIteration` etc.; extension registry). Phase 07. 04.2's matcher integration into HCM's hardcoded router invocation (per parent SPEC architectural rule 3 in §3) is unchanged from 04.1's hardcoded shape.
- **Per-virtual-host `typed_per_filter_config` / per-route `typed_per_filter_config`.** Phase 07 (filter chain framework).
- **Wildcard `domains: ["*.example.com"]` matching** on virtual hosts. Phase 04 (parent) supports `["*"]` (catch-all) or exact-string matching only. Wildcard-prefix DOMAIN matching is unrelated to HeaderMatcher; deferred to whichever phase first needs it. (Note: `prefix_match` on a HeaderMatcher value is a HeaderMatcher mode and IS in scope for 04.2; the deferred item is wildcard prefixes on `VirtualHost.domains`.)
- **Matcher framework generalization.** Envoy v1.33.0 has both the legacy `HeaderMatcher` (the 7 modes in 04.2) and a newer `Matcher` framework (the `envoy.matcher.v3.Matcher` / `MatcherTree` proto family used by listener filter matchers, RBAC, etc.). 04.2 implements only the legacy HeaderMatcher (which is the one HCM's RouteMatch uses); the modern Matcher framework is deferred to whichever phase first needs it (likely the RBAC family or a future listener-filter-matcher phase).
- **`headers_match_options` knob.** Envoy's RouteMatch grew an explicit `headers_match_options` enum field (default `ALL`; alternative `ANY` for OR semantics). 04.2 hardcodes ALL semantics per Envoy default; the field is not parsed (validator rejects with the existing serde `deny_unknown_fields` if present in YAML). Deferred to whichever phase first needs OR-shaped header matching.
- **`query_parameters` matcher on RouteMatch.** Envoy's `RouteMatch.query_parameters: Vec<QueryParameterMatcher>` (a separate matcher type, sibling of HeaderMatcher). Deferred; no 04.x fixture exercises it.
- **`grpc` matcher on RouteMatch.** Envoy's `RouteMatch.grpc: GrpcRouteMatchOptions` for gRPC-specific routing. Deferred; gRPC routing landings happen in the HTTP/2 + xDS family.
- **`tls_context` matcher on RouteMatch.** Envoy's `RouteMatch.tls_context: TlsContextMatchOptions` for TLS-property-based routing. Deferred.
- **Case-sensitive RouteMatch path matching toggle (`case_sensitive`).** Envoy's `RouteMatch.case_sensitive: BoolValue` (default true) on the path/prefix oneof. 04.2 hardcodes case-sensitive prefix/path matching per Envoy default; the field is not parsed. Deferred to whichever phase first needs it.
- **`safe_regex_match.google_re2` proto sub-field.** Envoy's `SafeRegexMatcher.google_re2: GoogleRE2` carries RE2-engine-specific tuning knobs (max-program-size etc.); ignored by 04.2. The Rust `regex` crate uses an RE2-compatible NFA engine by default; no tuning surface is exposed.

The non-goals list is otherwise inherited from parent SPEC §4 verbatim.

---

## 5. Splitting guidance for the planner

Estimated scope:

| Surface | Net LoC (impl + tests) |
|---|---|
| envoy-config schema (HeaderMatcher + HeaderMatcherMode + SafeRegex + Int64Range + StringMatcher + StringMatcherMode + RouteMatch.headers field; hand-rolled Deserialize for the field-name oneof) | ~150 + ~50 |
| envoy-config validator extensions (regex compile + Int64Range bounds check + ConfigError variants + EmptyHeaderName + UnknownHeaderMatcherMode) + ~20 envoy-config validator tests | ~80 + ~80 |
| Matcher runtime (`HeaderMatcher::matches` + `StringMatcher::matches`) + ~25 matcher-runtime unit tests | ~80 + ~120 |
| envoy-config Cargo.toml `regex = "1"` runtime dep + envoy-config lib.rs re-exports + ConfigError extension | ~10 |
| ADR-0021 (DECISIONS.md) | ~25 lines markdown (no Rust) |
| Fixture 0007 amendment (envoy.yaml + envoy-rust.yaml route addition + inputs/payload-matcher.bin + expectations.yaml restructure + README.md paragraph) + harness probe-list extension if needed | ~50 + ~30 |
| Fuzz corpus (1 new HeaderMatcher-shaped seed) | ~30 |
| Cargo.lock sync (state-4 dedicated commit; no source change) | ~0 |
| **Total** | **~700 LoC impl + ~400 LoC tests + ~30 LoC fuzz seed + ~25 lines ADR ≈ 1300 LoC; ~14 tasks** |

Both `BOOTSTRAP_PROMPT.md` §6.1 gates (> ~25 tasks OR > ~1500 LoC) hold comfortably at ~14 tasks / ~1300 LoC. **Do not split 04.2 further.** Per BOOTSTRAP_PROMPT.md §6.1 and parent SPEC §5, nested splits of an already-split sub-phase warrant `superpowers:systematic-debugging` first — and per the parent-04 brainstorm's express avoidance of nested splits (the 3-way flat split was chosen specifically to avoid nesting), a 04.2.1 / 04.2.2 split would be a strong scope-creep signal.

If the plan as actually written crosses either gate mid-write, invoke `superpowers:systematic-debugging` before attempting a nested split. The most plausible scope-creep vector is the matcher-runtime unit-test count (~25 tests is an estimate; per-mode coverage may justify more). If the test count alone pushes the LoC gate, the planner may opt for a more terse table-driven test shape (one test fn that iterates a (mode, value, expected) triple list); this is a coverage-preserving refactor, not a split trigger.

---

## 6. Implementation signposts for the planner

Notes flagging predictable planner questions so the planner resolves them in-plan rather than mid-execution. Inherits parent SPEC §6 signposts where relevant (esp. signpost 8 — matcher impl shape) and extends with 04.2-specific signposts.

1. **Field-name oneof shape — hand-rolled Deserialize.** Envoy's `HeaderMatcher` proto uses field-name-discriminated oneof shape (the discriminator is *which* of `exact_match` / `prefix_match` / `suffix_match` / `safe_regex_match` / `range_match` / `present_match` / `string_match` is the present key). serde's tagged-enum with `#[serde(tag = "...")]` doesn't model this directly (it expects a discriminator field with a fixed name), and `#[serde(untagged)]` would silently pick the first variant that parses (fragile + hard to diagnose). Phase 03.1's `TransportSocketTypedConfig` used `@type`-tagged-union (a **value**-tagged shape) — that's a different shape and not applicable here.

   Decision: **hand-rolled `impl<'de> Deserialize<'de> for HeaderMatcher`** that uses a `serde::de::MapAccess` visitor: collect the YAML keys into a small set; verify exactly one of the seven mode keys is present (else `UnknownHeaderMatcherMode` or `MultipleHeaderMatcherModes` — the latter is the symmetric error if two mode keys coexist); collect `name`, `invert_match`, the chosen mode key + its value, and any unknown keys; reject unknown keys with `deny_unknown_fields`-equivalent logic; construct the `HeaderMatcher` value. ~60 LoC. Same approach for `StringMatcher`'s field-name oneof (`exact` / `prefix` / `suffix` / `safe_regex` / `contains`), with the `ignore_case` field as a peer. The worked YAML → Rust example in §3 D1 above pins the wire shape.

2. **`regex::Regex` compiled at config-load time and held in `Arc<regex::Regex>`.** `Arc` keeps clone cheap (the route walker may clone a `HeaderMatcher` per request if the route walker takes ownership; in practice `&HeaderMatcher` borrowing suffices and `Arc` is mostly for the cross-thread-safety story — the per-listener HCM config is `Arc<HCMConfig>` shared across connection handlers per parent SPEC §6 signpost 15). Validator catches unparseable regex with `ConfigError::InvalidRegex { source: regex::Error }`; the validator runs at config-load time (`envoy_config::load_bootstrap`) so unparseable patterns are detected before any request is served.

3. **AND semantics across multiple HeaderMatchers on a Route.** Per Envoy v1.33.0 default `headers_match_options: ALL`, multiple HeaderMatchers on the same Route are AND-combined: ALL must match for the route to match. The route walker (HCM, in `crates/envoy-http1/`) implements this by iterating the `Vec<HeaderMatcher>` and short-circuit-returning false on the first non-match. `HeaderMatcher::matches` itself is per-matcher; the AND combination lives in the route walker.

4. **Header NAME matching is case-insensitive per HTTP/1.1 §3.2; header VALUE matching is case-sensitive by default.** RFC 7230 §3.2 mandates case-insensitive name matching. envoy-rust's `HeaderMatcher::matches` uses `eq_ignore_ascii_case` on the name (ASCII per HTTP/1.1's header name grammar; non-ASCII header names are rejected at parse time by the HCM landed in 04.1 per `httparse`'s discipline). Value matching is case-sensitive by default; `StringMatcher.ignore_case: bool` flips it for the value (only applies to Exact / Prefix / Suffix / Contains; SafeRegex callers express case insensitivity via the `(?i)` inline flag — Envoy-compat behavior).

5. **`invert_match: true` inverts the entire mode-specific match result** (after the mode-specific match runs, before AND-combination across sibling HeaderMatchers). XOR semantics: `result = mode_result ^ invert_match`. For `present_match: true, invert_match: true`, the matcher matches when the header is **absent**. The matcher-runtime tests cover the cross-product of mode × invert_match values that warrant explicit coverage (4 cross-cuts named in D1's test list).

6. **`RangeMatch` parses the header value as `i64` (decimal).** Non-parseable values (e.g., `x-version: vBETA`) cause the matcher to **fail** (return false) — NOT a `ConfigError::InvalidValue` error. The matcher just doesn't match; the route walker proceeds to the next route. The interval is **half-open**: `start <= value < end` per Envoy proto Int64Range. Boundary tests (`value == start`, `value == end`, `value == start - 1`, `value == end - 1`) are explicitly enumerated in D1's matcher-runtime tests.

7. **`PresentMatch(true)` and `PresentMatch(false)` semantics.** The `true` case is the obvious one: matcher matches iff the header is present. The `false` case is **subtle**: per Envoy proto `present_match: false` is equivalent to "no presence requirement" — i.e., the matcher always returns true. (This is unlike the symmetric reading where `false` would mean "header must be absent"; that semantics is achieved via `present_match: true, invert_match: true`.) Document at the `HeaderMatcherMode::PresentMatch` rustdoc; matcher-runtime tests cover all 4 cells (true × present, true × absent, false × present, false × absent) per D1's test list.

8. **`StringMatcher::Contains` is a substring match — only reachable through the modern `string_match` field.** Envoy v1.33.0 deliberately does not have a top-level `HeaderMatcherMode::ContainsMatch`; the modern generic `StringMatcher` is the only path to `Contains` semantics for header values. Document at the `HeaderMatcherMode` rustdoc + the `StringMatcherMode::Contains` rustdoc. The 7 HeaderMatcher modes ARE: ExactMatch, PrefixMatch, SuffixMatch, SafeRegexMatch, RangeMatch, PresentMatch, StringMatch(StringMatcher). The 5 StringMatcher variants ARE: Exact, Prefix, Suffix, SafeRegex, Contains.

9. **Validator runs at config-load time; compiled regexes stored on the parsed `SafeRegex.compiled: Option<Arc<regex::Regex>>` field.** This is a **non-serde field** (skipped in deserialize via the hand-rolled `impl Deserialize for SafeRegex`; absent from the field list serde sees). After deserialization, the validator walks the parsed `RouteConfiguration` and, for each `SafeRegex` it finds (in `HeaderMatcherMode::SafeRegexMatch` or `StringMatcherMode::SafeRegex`), calls `regex::Regex::new(&safe_regex.regex)`, wraps `Ok(re)` in `Arc::new(re)`, and stores it back on `safe_regex.compiled`. On `Err(e)`, the validator returns `ConfigError::InvalidRegex { source: e }`. The route walker (HCM) consumes the compiled regex via `safe_regex.compiled.as_ref().expect("validator ensured compiled")` — the `expect` is the same precedent as phase 02.2's `cluster_mgr.get(&tcp_proxy_cfg.cluster).expect("validator ensured present")` and phase 03.1's `TransportSocketTypedConfig::Downstream(ctx) else { unreachable!("validator rejects upstream on listener") }`.

10. **Cross-cut: `HeaderMatcher::matches(headers: &[(String, String)]) -> bool`.** Inherent method on the parsed `HeaderMatcher`; consumed by the route walker in HCM. Signature picks `&[(String, String)]` to match the header storage shape established in 04.1 (per parent SPEC §6 signpost 2: `Vec<(String, String)>` ordered by emission order, case-preserving storage, case-insensitive lookup). The route walker calls `route.match.headers.iter().all(|m| m.matches(request_headers))` (AND-combination) after the path-side oneof matches.

11. **Fuzz seed: 1 new file exercising several modes simultaneously.** `route_with_header_matchers.yaml` exercises 5 of the 7 modes (exact_match, safe_regex_match, range_match, present_match, string_match-with-contains-and-ignore_case) in a single Route's `headers:` Vec. The fuzzer mutates the YAML; the parse_bootstrap target exercises the hand-rolled Deserialize + the validator's regex-compile pass + the `Int64Range` bounds check. The fuzzer never executes the regex against header values (the parse_bootstrap target only exercises serde + validate, not matcher runtime); plausible-but-irrelevant regex pattern (`^v[0-9]+$`).

12. **`#![forbid(unsafe_code)]`** is unchanged on `crates/envoy-config/src/lib.rs`; D-3.8 carries forward. `regex` itself uses internal unsafe but it's behind its crate's allowlist; no envoy-rust-owned code carries unsafe.

13. **`anyhow` boundary** is unchanged. envoy-config returns `ConfigError` (typed) from `validate`; the harness's outer Result<()> may use `anyhow` per phase 00's posture, but no new `anyhow` boundaries introduced in 04.2.

14. **regex version pinning.** `regex = "1"` is pinned at the major-version line per the existing workspace convention (`serde = "1"`, `tokio = "1"`, etc.). Plan-writer verifies the latest `regex` 1.x stable line at execution time and, if a non-default feature gate is needed (e.g., `unicode-perl` for `\p{Letter}` patterns), opts out — 04.2's matcher needs do not exercise unicode classes (only ASCII patterns appear in the test corpus + the fuzz seed). Default features suffice; if a future phase needs more, that phase extends.

15. **`StringMatcher.ignore_case` semantics — Envoy-compat.** `ignore_case: true` flips the value match to case-insensitive for Exact / Prefix / Suffix / Contains. **It does NOT affect SafeRegex matching** (per Envoy proto: regex callers express case insensitivity via the `(?i)` inline flag). Documented at the `StringMatcher.ignore_case` rustdoc + tested in `string_match_safe_regex_ignore_case_no_effect`.

16. **HCM's route walker integration — additive.** The route walker landed in 04.1 (per parent SPEC §3 D3.1) walks `route_config` via VH-`domains`-then-Route-`match` first-match-wins. 04.2 extends the per-route match check from "path-side oneof matches" to "path-side oneof matches AND `route.match.headers.iter().all(|m| m.matches(request_headers))`". The change to HCM is small (~10 LoC: one extra `&&` clause on the existing match-check) and happens in `crates/envoy-http1/`'s route-walker module. ~5 HCM-side unit tests are added covering: no headers field (unchanged behavior); single-header-matcher-route selected; single-header-matcher-route skipped (matcher fails); multi-header-matcher AND combination (all match → selected); multi-header-matcher AND combination (one fails → skipped). These ~5 HCM tests are part of D1's test budget (the matcher-runtime tests in D1 cover `HeaderMatcher::matches` in isolation; these HCM tests cover the route-walker integration).

17. **Test-data plain-old-data: the `SafeRegex` PartialEq impl compares only the `regex: String` field.** The `Option<Arc<regex::Regex>>` compiled form has no stable equality (`regex::Regex` doesn't impl PartialEq). Tests that assert parsed-RouteConfiguration equality compare structurally; the compiled-form field is opaque to PartialEq. This makes test setup mechanical: construct expected values with `compiled: None`; assert equality post-parse but pre-validate. After validate runs, tests that need the compiled form check `safe_regex.compiled.as_ref().is_some()` separately.

18. **Hand-rolled Deserialize alternative considered: `#[serde(flatten)]` + presence checks.** The alternative to a hand-rolled Deserialize impl is `#[serde(flatten)]` on a wrapper struct + a post-deserialize `try_from` that inspects which mode field is `Some`. This is shorter but interacts poorly with `deny_unknown_fields` (flatten + deny_unknown_fields is a known serde footgun: flatten expands the unknown-field set unpredictably). The hand-rolled visitor is the cleaner choice and is the chosen approach. ~60 LoC for HeaderMatcher's visitor + ~50 LoC for StringMatcher's visitor.

19. **Cross-cutting tests live in matcher-runtime unit tests (`crates/envoy-config/src/bootstrap.rs::tests` or a sibling `matcher.rs::tests`).** Module decomposition (separate `matcher.rs` vs. inlined in `bootstrap.rs`) is decided at PLAN.md writeup time; both are valid. The recommendation is: keep schema + Deserialize in `bootstrap.rs`; move `HeaderMatcher::matches` + `StringMatcher::matches` + the matcher-runtime tests to a sibling `matcher.rs` if the test count justifies it (~25 matcher-runtime tests is on the edge — either inline or sibling is fine).

20. **Cargo.lock sync at state-4.** Established phase precedent (parent SPEC §8): the `Cargo.lock` updates from `regex = "1"` + transitive surface (`regex-syntax`, `aho-corasick`, `memchr`) land as a dedicated commit at the state-4 phase-done gate, not folded into Task-1's ADR commit. Reviewer notes the Cargo.lock diff in the state-4 review pass.

---

## 7. ADRs expected from this sub-phase

**One ADR lands during 04.2 execution**, appended to `docs/envoy-rust/DECISIONS.md` at Task 1 alongside the runtime-dep addition to `crates/envoy-config/Cargo.toml`. Mirrors phase 03.1 Task 1's ADR-0018 + ADR-0019 inline-landing pattern.

### ADR-0021 — `regex` permitted as a foundation for header / route matching

- Date: 2026-04-26 (or whatever date 04.2 Task 1 lands; backdated to ADR landing day).
- Status: accepted.
- Context: Phase 04.2 lands all 7 of Envoy's `HeaderMatcher` modes — `exact_match`, `prefix_match`, `suffix_match`, `safe_regex_match`, `range_match`, `present_match`, `string_match` (with the `StringMatcher` tagged union which itself has a `safe_regex` variant). Two of those modes — `safe_regex_match` and `string_match.safe_regex` — require a regex implementation. The Rust `regex` crate is the de-facto ecosystem default (RE2-compatible NFA engine, no backtracking, no catastrophic regex blow-ups; well-maintained; first-party `rust-lang` org). Not on the D-3.2 permitted-foundations list at phase-03.2 close (ADR-0019 was the latest ADR; the latest pre-04 permitted-foundations grant covered tokio-rustls + rustls-pemfile under the rustls grant).
- Options considered: (i) **defer `safe_regex_match` to a later phase** — rejected; the parent-04 brainstorm decision (per ADR-0020's context section + parent-04 SPEC §3 D6.2) was to land all 7 HeaderMatcher modes in 04.2 coherently; deferring one mode would scatter the matcher coverage across phases for arbitrary reasons; (ii) **hand-roll a regex engine** — rejected; reinvents wheels D-3.2 explicitly tells us not to; the `regex` crate is mature and ecosystem-standard; (iii) **add `regex = "1"` to the permitted-foundations list narrowly scoped to header / route matching at config-load time** (decision); (iv) **add `regex = "1"` to the permitted-foundations list with broad scope** — rejected; D-3.2's spirit is one-foundation-per-purpose; broader scopes warrant their own scope-extension ADRs at the time the broader use surfaces.
- Decision: extend the D-3.2 permitted-foundations list to cover `regex = "1"` as a runtime dep on `crates/envoy-config/`, narrowly scoped to `HeaderMatcher::SafeRegex` + `StringMatcher::SafeRegex` compilation at config-load time. NOT permitted for general-purpose use elsewhere; future filter-framework regex needs (e.g., URL path templates in a future router-knob phase, header-rewrite patterns in a future filter-framework phase, Lua filter `string.find` in a future Lua-filter phase) require an explicit scope-extension ADR that names this ADR and broadens the grant.
- Rationale: removes the per-phase-ADR churn that would otherwise dog later regex-using phases (HCM-internal regex would still warrant its own ADR if/when it surfaces — the narrow scope here is deliberate). `regex` is the Rust-ecosystem default; treating its first use as the foundation grant is the cheapest, most honest formalization. Compiling regexes at config-load time (validator pass) means unparseable patterns are caught before any request is served.
- Consequences: `crates/envoy-config/Cargo.toml`'s `[dependencies]` section gains `regex = "1"` at this commit. `Cargo.lock` gains `regex` + transitive surface (`regex-syntax`, `aho-corasick`, `memchr`) as a dedicated commit at the 04.2 state-4 phase-done gate per established phase precedent. `cargo deny check` remains clean: `regex` is dual-licensed MIT/Apache-2.0 (already on the `deny.toml` allow-list since phase 00); transitive deps are also covered (regex-syntax MIT/Apache-2.0; aho-corasick MIT/Unlicense; memchr MIT/Unlicense). Plan-writer cross-checks `deny.toml` at 04.2 Task 1 alongside the ADR landing; updates the `[licenses]` allow-list only if a fresh transitive license surfaces (not anticipated). Future scope-extension ADRs that broaden the grant (e.g., HCM internal regex, filter-framework regex) name this ADR explicitly.
- Provenance: this ADR was projected as the next-sequential available ADR number in parent-04 SPEC §7 (`docs/envoy-rust/phases/04-http1/SPEC.md`, committed at SHA `805433e`); ADR-0020 (parent-04 split decision) lands at parent-04 state-2 = the commit landing this SPEC alongside 04.1's, 04.2's, and 04.3's sub-phase SPECs; ADR-0021 lands at 04.2 Task 1.

Additional ADRs may be required during 04.2 execution per D-3.5 if:

- `cargo deny check` flips red on a new transitive license from the `regex` chain. Most likely a no-op (per the consequences section above); a non-trivial extension lands its own ADR (likely ADR-0022) at the time it trips.
- A serde-side ergonomic surfaces during the hand-rolled Deserialize impl that warrants documenting (e.g., a posture decision on how to disambiguate `present_match: false` from "no `present_match` field at all"). Likely a PROGRESS.md note, not an ADR; ADR only if the policy affects multiple later matcher-using phases.

---

## 8. Artifacts this sub-phase produces

Created during execution (relative to repo root):

- `docs/envoy-rust/phases/04.2-route-matchers/PLAN.md`
- `docs/envoy-rust/phases/04.2-route-matchers/PROGRESS.md`
- `docs/envoy-rust/phases/04.2-route-matchers/REVIEW.md`
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/route_with_header_matchers.yaml`
- `tests/fixtures/0007-http1-direct-response/inputs/payload-matcher.bin` (new probe input alongside the 04.1-landed `inputs/payload.bin`)

Amended during execution:

- `crates/envoy-config/src/bootstrap.rs` — add `HeaderMatcher`, `HeaderMatcherMode`, `SafeRegex`, `Int64Range`, `StringMatcher`, `StringMatcherMode` types; add the additive `headers: Vec<HeaderMatcher>` field on the existing `RouteMatch` struct; add hand-rolled `Deserialize` impls for `HeaderMatcher` + `StringMatcher` + `SafeRegex` (the field-name oneof discrimination + the `compiled: None` setup for SafeRegex); add `HeaderMatcher::matches` + `StringMatcher::matches` inherent methods (or in a sibling `matcher.rs` per §6 signpost 19); extend `validate` with regex compile + Int64Range bounds-check passes + new `ConfigError` variants `EmptyHeaderName`, `InvalidRegex`, `InvalidInt64Range`, `UnknownHeaderMatcherMode`; add ~20 new validator unit tests + ~25 matcher-runtime unit tests.
- `crates/envoy-config/src/lib.rs` — re-export the new public types; extend `ConfigError` enum with the four new variants.
- `crates/envoy-config/Cargo.toml` — add `regex = "1"` to the runtime `[dependencies]` section under ADR-0021.
- `tests/fixtures/0007-http1-direct-response/envoy.yaml` — add a second route (matcher route, `prefix: "/api/"` + `headers: [{ name: "x-foo", exact_match: "bar" }]`, `direct_response: { status: 418, body: { inline_string: "teapot\n" } }`) placed first in the `routes:` Vec so first-match-wins reaches it; the existing 04.1-landed default route stays second.
- `tests/fixtures/0007-http1-direct-response/envoy-rust.yaml` — same `routes:` shape (per-side divergences from 04.1 unchanged: bind address, no admin block).
- `tests/fixtures/0007-http1-direct-response/expectations.yaml` — restructure from single-probe to two-probe shape (`probes: [default-route, matcher-route]`) per §3 D3 illustration; if 04.1 lands a single-probe-only schema, harness `tests/differential/src/lib.rs::run_fixture` is also extended (~30 LoC) to iterate over a probe list.
- `tests/fixtures/0007-http1-direct-response/README.md` — add one paragraph noting the 04.2-added matcher route + its property; add ADR-0021 to the README's ADR-references list.
- `tests/differential/src/lib.rs` — possibly extend `run_fixture` for probe-list iteration (per above) and the `Driver::Http1` variant to carry `extra_headers: Option<Vec<(String, String)>>` (so the matcher probe can inject `X-Foo: bar`); cross-check 04.1's landed `Driver::Http1` shape at SPEC writeup — if 04.1's variant already accommodates per-probe extra headers, no harness change needed.
- `docs/envoy-rust/DECISIONS.md` — ADR-0021 appended at Task 1.
- `docs/envoy-rust/ROADMAP.md` — row `04.2` `status` `in-progress` → `done` at the state-6 phase-done commit; parent row `04` stays `in-progress` (flips at 04.3's state-6 commit per the ROADMAP-schema invariant "parent flips to `done` only after all sub-phases are `done`").
- `docs/envoy-rust/STATE.md` — at the state-6 phase-done commit: active → `04.3-router-upstream`, lifecycle state → 3 (PLAN.md does not exist yet; 04.3's SPEC was landed at parent-04 state-2 alongside this one). Next-skill: `superpowers:writing-plans` scoped to sub-phase 04.3.
- `Cargo.lock` — synced as a dedicated commit at the 04.2 state-4 phase-done gate per the established phase precedent (mirrors phase-01 `4955252`, phase-02.1 `dea4d16`, phase-02.2 `2146014`, phase-03.1 `eb039e6`, phase-03.2 `85633e7`); new transitive surface from `regex` + `regex-syntax` + `aho-corasick` + `memchr`.
- `deny.toml` — likely no-op at 04.2 Task 1 (per ADR-0021 consequences: regex's MIT/Apache-2.0 + transitives MIT/Unlicense are all on the existing allow-list); cross-check at Task 1 alongside the ADR landing. If a fresh license surfaces, the `[licenses]` allow-list extension lands in the same Task-1 commit as ADR-0021.

Not touched in 04.2 (belong to 04.1, 04.3, or are frozen):

- `docs/envoy-rust/phases/04-http1/SPEC.md` (parent) — unedited; remains the design artifact committed at SHA `805433e`.
- `docs/envoy-rust/phases/04.1-hcm-direct-response/{SPEC.md,PLAN.md,PROGRESS.md,REVIEW.md}` — closed at the 04.1 phase-done commit; unedited in 04.2.
- `docs/envoy-rust/phases/04.3-router-upstream/SPEC.md` — landed at parent-04 state-2 alongside this SPEC; unedited in 04.2 (its PLAN/PROGRESS/REVIEW land in 04.3 execution).
- `crates/envoy-http1/` — landed at 04.1 (codec + HCM + route walker + per-listener config). 04.2 does NOT modify the crate beyond the route walker's per-route match-check extension (the ~10 LoC `&&` clause + ~5 HCM unit tests per §6 signpost 16) — those edits land alongside D1's matcher-runtime work, in this sub-phase's plan as a small per-task addition on the relevant task. If the planner prefers, the route-walker integration lives in `crates/envoy-http1/` files explicitly amended; the artifact-list is amended at PLAN.md writeup.
- `crates/envoy-tls/`, `crates/envoy-tcp/`, `crates/envoy-listener/`, `crates/envoy-cluster/`, `crates/envoy-bin/` — unchanged. The matcher work is purely in envoy-config + (per signpost 16) a ~10 LoC route-walker tweak in envoy-http1.
- `tests/fixtures/0001-tcp-echo/` through `tests/fixtures/0006-tls-sni/` — unedited; their fixtures must remain green at the 04.2 state-4 gate.
- `tests/helpers/{tcp,tls}-echo-server/`, `tests/helpers/http1-echo-server/` (the third doesn't exist yet — lands in 04.3) — unchanged.
- `BEHAVIOR_CONTRACT.md` — no edits in 04.2 (matchers are config-side; no new response headers).
- `rust-toolchain.toml`, `docs/envoy-rust/ENVOY_TARGET.md` — frozen per D-3.7 / D-3.9.

---

## 9. Final commit message format (for state 6 of the 04.2 lifecycle)

```
phase 04.2: HTTP route header matchers + ADR-0021 (regex permitted)

All 7 of Envoy's HeaderMatcher modes (exact_match, prefix_match, suffix_match,
safe_regex_match, range_match, present_match, string_match) plus the
StringMatcher tagged union + invert_match: bool land additively on
RouteMatch.headers in envoy-config. ADR-0021 lands regex = "1" as a runtime
dep on envoy-config narrowly scoped to header / route matching at config-load
time; cargo deny check stays clean. Hand-rolled Deserialize impls model the
field-name oneof shape (Envoy's HeaderMatcher proto uses field-name
discrimination, not @type tagged-union — different from phase 03.1's
TransportSocketTypedConfig). SafeRegex compiles at validate time into
Arc<regex::Regex>; non-parseable patterns surface as
ConfigError::InvalidRegex. HeaderMatcher::matches + StringMatcher::matches
expose the per-matcher truth predicate consumed by HCM's route walker;
multi-matcher AND semantics live in the route walker per Envoy default
headers_match_options: ALL. ~20 new envoy-config validator tests + ~25
matcher-runtime unit tests + 1 new fuzz seed (route_with_header_matchers).
Fixture 0007 (landed in 04.1) gains a second route demonstrating production
matcher use: prefix /api/ + X-Foo: bar selects a 418 teapot; the existing
GET /healthz probe still falls through to the 200 default route. Both proxies
select the same route on each probe.

Differential surface: tests/fixtures/0001-tcp-echo green (unchanged);
  tests/fixtures/0002-static-admin-ready green (unchanged);
  tests/fixtures/0003-tcp-proxy green (unchanged);
  tests/fixtures/0004-tls-downstream green (unchanged);
  tests/fixtures/0005-tls-upstream green (unchanged);
  tests/fixtures/0006-tls-sni green (unchanged);
  tests/fixtures/0007-http1-direct-response green (HTTP/1.1 listener;
  direct_response route action; matcher fan-out exercised via the amended
  matcher-bearing route — both proxies select the same route given the same
  request).
Conformance: none.
```

The 04.2 state-6 commit flips ROADMAP row `04.2` from `in-progress` to `done`. Parent row `04` stays `in-progress` (flips at 04.3's state-6 phase-done commit per the ROADMAP-schema invariant "parent flips to `done` only after all sub-phases are `done`"). STATE.md advances to phase `04.3` lifecycle state 3 (PLAN.md does not exist yet; 04.3's SPEC was landed at parent-04 state-2 alongside this one). Next-skill: `superpowers:writing-plans` scoped to sub-phase 04.3.
