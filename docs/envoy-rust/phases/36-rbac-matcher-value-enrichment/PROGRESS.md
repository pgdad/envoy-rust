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

## Task 5 — Differential fixture `0044` (F1 present/absent + F2 safe_regex match/miss) ✅

- Created `tests/fixtures/0044-http-rbac-matcher-value-enrichment/` (envoy.yaml, envoy-rust.yaml, expectations.yaml, README.md): H1 direct_response + `[header_to_metadata (x-tier->tier, x-present->present_probe), rbac (two OR'd ALLOW policies f2_regex + f1_present), router]`. 4 probes: a F2 staging->200, b F2 dev->403, c F1 x-present->200, d F1 absent->403. ANCHORED `^(prod|staging)$` (§A3b); 19-byte 403 body (ADR-0034).
- Created `tests/differential/tests/rbac_matcher_value_enrichment.rs` (mirrors 0043 entry).
- Rebuilt `envoy-bin` (both `--release` and debug) before the differential (stale-binary guard: the debug binary dated Jun 24 was missing Tasks 1-4; rebuilt to Jun 25).
- Differential result: pass — `cargo test -p differential rbac_matcher_value_enrichment` green (1/1 passed, 7.60s). Both proxies byte-identical on all 4 probes (Docker available locally).

## Task 6 — Fuzz seeds (existing `parse_bootstrap`, NO new target) ✅

- Added two corpus seeds under `crates/envoy-config/fuzz/corpus/parse_bootstrap/`: `rbac_present_match.yaml` (a `[header_to_metadata, rbac]` chain whose RBAC `metadata` value is `present_match: true`) and `rbac_safe_regex.yaml` (same with `value: { string_match: { safe_regex: { regex: "^(prod|staging)$" } } }`, anchored §A3b).
- Per memory `fuzz-corpus-seed-gitignored-by-default`: added `!rbac_present_match.yaml` + `!rbac_safe_regex.yaml` un-ignore lines to `crates/envoy-config/fuzz/.gitignore`. Verified both tracked via `git ls-files`; `git check-ignore` reports neither ignored.
- NO new fuzz target (memory `new-fuzz-target-needs-a-ci-yml-step`): the existing `parse_bootstrap` ci.yml step picks up the new seeds, so no ci.yml change.
- Validity check: `cargo +nightly fuzz run --fuzz-dir crates/envoy-config/fuzz parse_bootstrap <both seeds>` executed each seed once cleanly (2 ms / 0 ms, no crash/panic). Full short-budget fuzz run is the state-4 §7.5 gate (d) concern.

