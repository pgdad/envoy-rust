# Phase 37 — `37-rbac-url-path-condition` — PROGRESS

> State-3 implementation log (`superpowers:executing-plans`, TDD per task per
> `superpowers:test-driven-development`). Ground truth: **ADR-0090** §A–§D
> (empirically locked vs live `envoyproxy/envoy:v1.33.0`). PLAN: `PLAN.md` (7 TDD
> tasks). Append-only; one entry per task on completion.

---

## Task 1 — `PathMatcher` config struct + export — DONE

**TDD:** wrote 4 failing tests first (`path_matcher_parses_exact_and_round_trips`,
`path_matcher_empty_is_missing_path_error`, `path_matcher_empty_string_matcher_is_missing_mode_error`,
`path_matcher_unknown_subkey_is_denied`) in `bootstrap.rs` `rbac_tests`; confirmed
RED (`cannot find type PathMatcher in the crate root`); added the thin derived
`#[serde(deny_unknown_fields)] pub struct PathMatcher { pub path: StringMatcher }`
after `MetadataPathSegment` (`bootstrap.rs`) + exported it from `lib.rs` (after
`PathConfigSource`); confirmed GREEN.

**Evidence:** `cargo test -p envoy-config path_matcher` → `4 passed` (the 4 new
PathMatcher tests; a 5th pre-existing `parses_route_with_path_matcher` matched the
filter incidentally and also passed).

**ADR-0090 §D:** a thin DERIVED struct suffices — the required `path` field +
`deny_unknown_fields` + the inner `StringMatcher` visitor's "missing mode key"
error cover §D cases 1–3 (empty `PathMatcher` → missing `path`; `path: {}` →
missing mode key; unknown sub-key → `deny_unknown_fields`). No hand-rolled visitor.

**Commit:** `phase 37: PathMatcher config struct (RBAC url_path) [ADR-0090]`
