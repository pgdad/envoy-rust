# Phase 04.2 Progress

## Task 1 — ADR-0021 (regex permitted) + envoy-config Cargo dep + 4 ConfigError variants stub (2026-04-27)

- Commit: 984aedded536d5cce0114f9fbeb814bb0846a337
- Change: appended ADR-0021 to docs/envoy-rust/DECISIONS.md (regex = "1" narrowly permitted as a foundation for header / route matching at config-load time); added `regex = "1"` to crates/envoy-config/Cargo.toml [dependencies]; added `Unlicense` to deny.toml [licenses] allow list (transitive aho-corasick + memchr are MIT/Unlicense dual-licensed); added 4 ConfigError variants in lib.rs (EmptyHeaderName, InvalidRegex, InvalidInt64Range, UnknownHeaderMatcherMode); Cargo.lock updated with regex + regex-syntax + aho-corasick + memchr entries.
- Verification: `cargo build --workspace --all-targets` → clean; `cargo deny check` → advisories ok, bans ok, licenses ok, sources ok; `cargo clippy --workspace --all-targets --all-features -- -D warnings` → clean; `cargo fmt --all -- --check` → clean; `cargo test -p envoy-config --lib` → 75 passed (unchanged from 04.1 close).
- Tests added: none in Task 1.
- ADRs: ADR-0021 (this task). ADR ledger head: 21.
- Deviations from PLAN:
  1. **Cargo.lock landed inline at Task 1 rather than at the state-4 dedicated-commit cadence.** ADR-0021's Consequences section (now landed and append-only per D-3.5) states the lock will sync "as a dedicated commit at the 04.2 state-4 phase-done gate per established phase precedent." Reality: PLAN.md Task 1 Step 11's `git add ... Cargo.lock` stages the lock inline, so it landed in commit `984aedde` alongside the ADR + Cargo.toml + deny.toml + lib.rs changes. The PLAN itself anticipates this branch — Task 12 Step 2 says "If Cargo.lock has been clean since Task 1 — possible if Task 1 already committed it inline — skip this step and document in PROGRESS that the inline commit at Task 1 satisfied the sync, deviating from the M5-recommended dedicated cadence. Either path is doctrine-conformant." Per D-3.5 ADR-0021 itself remains unedited; this PROGRESS note is the audit trail. The 04.1 phase used the same inline-at-scaffold cadence (per 04.1 PROGRESS Task 4 / Task 17 + 04.1 REVIEW M5).

## Task 2 — envoy-config schema: Int64Range + SafeRegex (2026-04-27)

- Commit: d6f034450a8ab102919ef6a6fcc8cd209b83bbd1
- Change: appended Int64Range (i64 half-open range struct, deny_unknown_fields) and SafeRegex (regex: String + non-serde compiled: Option<Arc<regex::Regex>>) types after DirectResponse in bootstrap.rs. SafeRegex carries a hand-rolled `impl<'de> Deserialize<'de>` that reads only `regex: String`, rejects any other key, and sets compiled: None (validator extension in Task 5 will fill compiled). SafeRegex's custom PartialEq compares only the regex String — matches SPEC §6 signpost 17 (regex::Regex has no stable equality).
- Tests added (4): parses_int64_range, rejects_unknown_field_in_int64_range, parses_safe_regex, safe_regex_partial_eq_compares_only_regex_string.
- Verification: `cargo test -p envoy-config --lib` → 79 passed; clippy + fmt + build clean.
- Deviations: none.

### Task 2 follow-up — review fix (2026-04-27)

- Commit: 5a6b950db5da2191e220e20ac4f7422b506313b0
- Change: amended the `SafeRegex` Deserialize doc-comment in bootstrap.rs to replace the misleading "`#[serde(skip)]` would leave the field absent" justification with the accurate template-setting rationale (Tasks 3+4 reuse the visitor pattern for field-name oneof discrimination on StringMatcher / HeaderMatcher, where `#[serde(untagged)]` + `#[serde(tag)]` are both wrong; the two-phase init contract is named explicitly). Comment-only change; no semantic impact on Task 2 code.
- Verification: gate commands clean (build / clippy / fmt / test / deny all exit 0; 79 tests passing — unchanged from Task 2).

## Task 3 — envoy-config schema: StringMatcher + StringMatcherMode (2026-04-27)

- Commit: 90dcb47afdc076bcb1e4537a1fe80803fba50fd0
- Change: appended StringMatcher (mode + ignore_case) and StringMatcherMode (5 variants: Exact, Prefix, Suffix, SafeRegex, Contains) types after SafeRegex in bootstrap.rs. StringMatcher carries a hand-rolled `impl<'de> Deserialize<'de>` for the field-name oneof: collects all keys; allows at most one mode key; accepts ignore_case as a peer (default false); rejects unknown keys via M::Error::unknown_field. Added the 5th ConfigError variant UnknownStringMatcherMode in lib.rs (sibling of the 4 added in Task 1).
- Tests added (5): parses_string_matcher_exact, parses_string_matcher_contains_with_ignore_case, parses_string_matcher_safe_regex, rejects_unknown_string_matcher_mode_key, rejects_two_string_matcher_mode_keys.
- Verification: `cargo test -p envoy-config --lib` → 84 passed; clippy + fmt + build clean.
- Deviations: two test assertions were written as `assert_eq!(sm.ignore_case, false/true)` and had to be updated to `assert!(!sm.ignore_case)` / `assert!(sm.ignore_case)` to satisfy `clippy::bool_assert_comparison` (-D warnings). The PLAN's test code used assert_eq! with literal bools; functionally identical after fix.

### Task 3 follow-up — review fix (2026-04-27)

- Commit: 3d9f985815649e7ea4c021236a9804f09905cb1f
- Change: tightened the `rejects_two_string_matcher_mode_keys` assertion to verify the error message contains "multiple mode keys" or "mutually exclusive" (was only checking is_err()). Closes the assertion gap noted in code quality review — a future error-shape regression would now be caught. Mirrors the existing `rejects_unknown_string_matcher_mode_key` assertion style. Note: the plan specified `.err().expect()` but clippy::err_expect (-D warnings) requires `.expect_err()` instead; semantically identical.
- Verification: `cargo test -p envoy-config rejects_two_string_matcher_mode_keys` → 1 passed; full crate test 84 passed; all gate commands clean.

## Task 4 — envoy-config schema: HeaderMatcher + RouteMatch.headers (2026-04-27)

- Commit: 6237b0a
- Change: appended HeaderMatcher (name + mode + invert_match) and HeaderMatcherMode (7 variants: ExactMatch, PrefixMatch, SuffixMatch, SafeRegexMatch, RangeMatch, PresentMatch, StringMatch) types after StringMatcher in bootstrap.rs. HeaderMatcher carries a hand-rolled `impl<'de> Deserialize<'de>` for the field-name oneof: collects all keys; uses an inline `set_mode` helper to short-circuit on multi-mode-key collision; accepts invert_match as a peer (default false); rejects unknown keys via M::Error::unknown_field. Added `headers: Vec<HeaderMatcher>` field with #[serde(default)] to RouteMatch; added Clone derive on RouteMatch for matcher-runtime test ergonomics. Re-exported HeaderMatcher, HeaderMatcherMode, SafeRegex, Int64Range, StringMatcher, StringMatcherMode from lib.rs's pub use bootstrap{...} list. Extended all 6 RouteMatch literal sites in crates/envoy-http1/src/hcm.rs to include `headers: vec![]` or `headers: r.r#match.headers.clone()` — minimum compile-cascade fix; matching logic extension stays in Task 7.
- Tests added (6): parses_header_matcher_exact, parses_header_matcher_with_invert_match_true, parses_header_matcher_present_match_true, parses_header_matcher_string_match_contains, rejects_unknown_header_matcher_mode_key, parses_route_match_with_headers_vec_and_invert_match_default.
- Verification: `cargo test -p envoy-config --lib` → 90 passed; `cargo build --workspace --all-targets` clean (envoy-http1 hcm.rs updated for all 6 RouteMatch literal sites — 1 in clone_route_config + 5 in tests); clippy + fmt clean.
- Deviations: PLAN mentioned only the `clone_route_config` site in hcm.rs, but the compile cascade affected 5 additional RouteMatch literal sites in the test section of hcm.rs (lines 353, 427, 464, 478, 527). All 6 sites were patched with `headers: vec![]` / `.headers.clone()` as appropriate. Additionally, rustfmt reformatted one long `assert_eq!` line in the new test (`parses_route_match_with_headers_vec_and_invert_match_default`) into multi-line form; `cargo fmt --all` applied to satisfy the fmt gate.

### Task 4 follow-up — review fix (2026-04-27)

- Commit: 17f991ac1401328f1e551276412b0939e473ff7b
- Change: addressed 2 Important findings from code quality review:
  1. Added `rejects_two_header_matcher_mode_keys` test exercising `set_mode`'s guard branch (the schema-layer test for the multi-mode-key collision invariant; mirrors Task 3's `rejects_two_string_matcher_mode_keys`).
  2. Introduced `MODE_KEYS` const inside HeaderMatcher's Deserialize impl (sibling to ALL_KEYS) and refactored the missing-mode error to use `format!("...{MODE_KEYS:?}")` — matches StringMatcher's pattern and prevents the error string from drifting if a future variant is added.
- Verification: `cargo test -p envoy-config --lib` → 91 passed (was 90; +1); all gate commands clean.

## Task 5 — envoy-config validator: regex compile + Int64Range bounds + matcher walk (2026-04-27)

- Commit: 18a3313
- Change: extended `validate_hcm` (now `&mut`-signature) to walk `route.r#match.headers` Vec per route. Added `validate_header_matcher` helper (rejects empty name; dispatches by mode; range validates start < end; SafeRegex compile-pass via `compile_safe_regex` helper). Added `compile_safe_regex` helper that wraps `regex::Regex::new(&sr.regex)` and stores the compiled regex back on `safe_regex.compiled = Some(Arc::new(re))`. Switched `validate` and `validate_hcm` from `&Bootstrap`/`&HttpConnectionManagerConfig` to `&mut` to allow the compile-pass mutation; HCM dispatch arm in `validate` switched from `as_ref()` to `as_mut()`. `parse_bootstrap` absorbs the `&mut` internally; envoy-bin needs no change. `parse_then_validate` test helper updated. The TCP_PROXY_FILTER arm in `validate` was refactored to clone the cluster name before the immutable cluster lookup to satisfy the borrow checker (the `&mut listeners` loop held a mutable borrow on `bootstrap`; looking into `bootstrap.static_resources.clusters` requires resolving the borrows correctly).
- Tests added (10): rejects_empty_header_name, rejects_invalid_regex_in_safe_regex_match, rejects_invalid_regex_in_string_match_safe_regex, rejects_invalid_int64_range_start_eq_end, rejects_invalid_int64_range_start_gt_end, validator_compiles_safe_regex_match_into_arc, validator_accepts_all_seven_modes, validator_accepts_empty_headers_vec, validator_accepts_invert_match_true, validator_compiles_string_match_safe_regex_into_arc.
- Verification: `cargo test -p envoy-config --lib` → 101 passed; `cargo build --workspace --all-targets` clean (envoy-bin absorbs the &mut transparently); clippy + fmt clean.
- Deviations: The borrow checker required a small structural change in `validate`'s TCP_PROXY_FILTER arm: `tp.cluster.clone()` is extracted into `cluster_name` before the mutable listener borrow is active, and the cluster lookup uses `bootstrap.static_resources.clusters.iter()` directly (rather than the `clusters` alias that existed in the old code). This is semantically identical but necessary because the `&mut bootstrap.static_resources.listeners` loop borrow and a simultaneous `&bootstrap.static_resources.clusters` access would conflict under two-phase borrow rules. The PLAN did not anticipate this specific borrow conflict; the fix preserves the original check semantics exactly.

### Task 5 follow-up — review fix (2026-04-27)

- Commit: 48e615c
- Change: added a 3-line comment block to the TCP_PROXY_FILTER arm in `validate` clarifying that the `as_ref()` borrow is intentional (TCP_PROXY validation is read-only) — closes the asymmetry-readability gap with the HCM_FILTER arm's `as_mut()` (introduced in this Task 5). Comment-only change.
- Verification: gate commands clean; 101 tests still passing.

## Task 6 — envoy-config::matcher runtime + 28 matcher tests (2026-04-27)

- Commit: dfac122
- Change: created crates/envoy-config/src/matcher.rs with `impl HeaderMatcher::matches(&self, headers: &[(String, String)]) -> bool` and `impl StringMatcher::matches(&self, value: &str) -> bool`. Header name lookup uses eq_ignore_ascii_case (HTTP/1.1 §3.2). XOR with invert_match. SafeRegex variants take `safe_regex.compiled.as_ref().expect("validator ensured compiled")`. StringMatcher.ignore_case affects Exact/Prefix/Suffix/Contains (case-folded comparison) but not SafeRegex (Envoy proto: regex callers use `(?i)`). Half-open i64 range. Non-parseable RangeMatch values fail the match (not an error). present_match: false is "no presence requirement" (always true) per SPEC §6 signpost 7. Added pub mod matcher; to lib.rs.
- Tests added (28): per-mode boolean truth tables (3 each for ExactMatch / PrefixMatch / SuffixMatch / SafeRegexMatch; 5 for RangeMatch boundary; 4 for PresentMatch; 3 for StringMatch); cross-cuts (header name case-insensitivity; header value case-sensitivity by default; invert_match for ExactMatch + PresentMatch).
- Verification: `cargo test -p envoy-config --lib` → 129 passed; clippy + fmt + build clean.
- Deviations: `cargo fmt` reformatted several lines in matcher.rs (SuffixMatch arm collapsed to single line; Prefix ignore_case joined to one line; Contains ignore_case chain split differently; 8 test `hm(...)` calls split to multi-line). Semantically identical; `cargo fmt --all` applied before committing to satisfy the fmt gate.
