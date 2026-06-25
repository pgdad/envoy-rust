# Phase 36 — `36-rbac-matcher-value-enrichment` — Implementation Progress

> State-3 implementation running log (`superpowers:subagent-driven-development`, TDD per task).
> One entry per completed PLAN task. The §A facts are empirically locked by ADR-0088 (do NOT re-derive).
> Scope: F1 `present_match` `ValueMatcher` variant on the RBAC `metadata` condition; F2 `safe_regex`
> `StringMatcher` compilation on the RBAC path (header + metadata), closing carry-forward M35-1.

---

## Task 1 — F1 config: `present_match` `ValueMatcher` variant + `matches_resolved` ✅

- Added `PresentMatch(bool)` arm to `ValueMatcher` (`bootstrap.rs`) + Deserialize visitor `"present_match"` arm + `KEYS = ["string_match", "present_match"]` (Serialize is derive-based via `#[serde(rename)]`).
- `matcher.rs`: made `ValueMatcher::matches` exhaustive (`PresentMatch(want) => *want`) + added presence-aware `matches_resolved(Option<&str>)` (§A1 `present && want`).
- Deleted obsolete `rbac_metadata_rejects_present_match_value`; added `rbac_metadata_accepts_present_match_value`, `rbac_metadata_present_match_false_parses`, `rbac_metadata_rejects_other_value_matcher_keys` (bootstrap.rs) + `value_matcher_present_match_resolved_semantics` (matcher.rs).
- TDD: tests written first, watched fail (no variant/method), implemented, watched pass.
- Gate: `cargo test -p envoy-config` green.

## Task 2 — F1 runtime: presence-aware `eval_metadata` (`present && want`) ✅

- Restructured `eval_metadata` (`crates/envoy-filter/src/rbac.rs`) to resolve the metadata path to `Option<&str>` and call `m.value.matches_resolved(resolved)` — so `present_match` observes KEY PRESENCE (§A1 `present && want`); `string_match` unchanged (present AND value-matches).
- Added `present_matcher` helper + tests `metadata_present_match_true_matches_present_key`, `metadata_present_match_true_no_match_when_absent`, `metadata_present_match_false_never_matches`.
- TDD: tests written first (Step-2: all 3 already passed with old `eval_metadata` via Task 1's exhaustive `matches` arm — recorded honestly per PLAN); restructured to `matches_resolved`, watched pass.
- Gate: `cargo test -p envoy-filter rbac` green (32/32 passed).

## Task 3 — F2 config: public SafeRegex compile helpers ✅

- Added public `HeaderMatcher::compile_safe_regexes`, `StringMatcher::compile_safe_regex`, `ValueMatcher::compile_safe_regexes` (`bootstrap.rs`), all reusing the private `compile_safe_regex` free fn + `ConfigError::InvalidRegex`. `present_match` → no-op. Route-config `validate_header_matcher` UNCHANGED.
- Repurposed `rbac_metadata_value_safe_regex_is_parse_accepted` → `rbac_metadata_value_safe_regex_parse_accepted_and_compilable` (M35-1 limitation note removed; now asserts the parsed value compiles via the helper, anchored `^(prod|staging)$`).
- Added `header_matcher_compile_safe_regexes_compiles_and_rejects` + `value_matcher_compile_safe_regexes_compiles_string_and_noops_present`.
- TDD: tests first, watched fail (no methods), implemented, watched pass.
- Gate: `cargo test -p envoy-config` green.

## Task 4 — F2 runtime: fallible RBAC lowering compiles SafeRegex (closes M35-1) ✅

- Made `lower_permission`/`lower_principal` fallible (`-> Result<_, FilterError>`); the `Header`/`Metadata` arms clone the matcher, call `compile_safe_regexes()` (header) / `value.compile_safe_regexes()` (metadata), map `ConfigError::InvalidRegex → FilterError::InvalidConfig`; combinator arms thread `?`. `build_from_config`'s policies closure now fallible + `collect::<Result<_,_>>()?`.
- A malformed RBAC `safe_regex` is now BOOT-fatal (not a first-request panic) — M35-1 CONSUMED.
- Tests: `metadata_safe_regex_value_matches_without_panic`, `header_safe_regex_matches_without_panic` (the panic-regression GUARD — PANICKED on the pre-fix tree per Step 2), `malformed_rbac_safe_regex_is_boot_fatal_not_panic`. All anchored `^(prod|staging)$` (§A3b).
- TDD: tests first, watched panic/fail (Step 2 recorded), implemented, watched pass.
- Gate: `cargo test -p envoy-filter` green (201/201 passed).

