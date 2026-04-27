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
