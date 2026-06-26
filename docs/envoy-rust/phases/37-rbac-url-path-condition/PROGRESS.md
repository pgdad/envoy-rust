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

---

## Task 2 — `Permission::UrlPath` end-to-end — DONE

**TDD:** wrote `permission_parses_url_path_and_json_round_trips` (bootstrap.rs,
YAML parse + JSON round-trip) and `url_path_permission_exact_matches_and_strips_query`
(rbac.rs, + new `req_with_path` helper) FIRST; confirmed RED (`no variant ... UrlPath
... for enum Permission` / `... RuntimePermission`).

**Implemented (whole task before commit — an enum variant breaks the exhaustive
matches in BOTH crates):**
- `bootstrap.rs`: `Permission::UrlPath(PathMatcher)` variant (+ `#[serde(rename = "url_path")]`),
  `"url_path"` in `KEYS`, the visitor arm `"url_path" => Permission::UrlPath(map.next_value::<PathMatcher>()?)`,
  and the `validate_permission_tree` leaf arm `Permission::UrlPath(_) => Ok(())`.
- `rbac.rs`: `strip_query(path) = path.split('?').next().unwrap_or(path)` free fn;
  `RuntimePermission::UrlPath(StringMatcher)` variant; `eval_permission` arm
  `sm.matches(strip_query(&req.path))`; `lower_permission` arm cloning `pm.path` +
  `compile_safe_regex()?` (phase-36 fallible path) → boot-fatal on bad regex.

**Evidence:** `cargo test -p envoy-config permission_parses_url_path` → `1 passed`;
`cargo test -p envoy-filter url_path_permission` → `1 passed`; `cargo build --workspace`
→ `Finished` (clean — both crates' exhaustive matches updated, workspace-green).

**Commit:** `phase 37: Permission::UrlPath variant + query-stripped eval + fallible lowering [ADR-0090]`
