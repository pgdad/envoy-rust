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

