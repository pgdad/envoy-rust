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

---

## Task 3 — `Principal::UrlPath` end-to-end (symmetric) — DONE

**TDD:** wrote `principal_parses_url_path` (bootstrap.rs) and
`url_path_principal_matches_query_stripped` (rbac.rs) FIRST; confirmed RED
(`no variant ... UrlPath ... for enum Principal` / `... RuntimePrincipal`).

**Implemented (mirror of Task 2 for `Principal`):**
- `bootstrap.rs`: `Principal::UrlPath(PathMatcher)` variant + `#[serde(rename = "url_path")]`,
  `"url_path"` in `KEYS`, visitor arm, `validate_principal_tree` leaf `Principal::UrlPath(_) => Ok(())`.
- `rbac.rs`: `RuntimePrincipal::UrlPath(StringMatcher)` variant; `eval_principal`
  arm `sm.matches(strip_query(&req.path))`; `lower_principal` arm (clone +
  `compile_safe_regex()?`).

**Evidence:** `cargo test -p envoy-config principal_parses_url_path` → `1 passed`;
`cargo test -p envoy-filter url_path_principal` → `1 passed`; `cargo build --workspace`
→ `Finished` (clean).

**Commit:** `phase 37: Principal::UrlPath variant (symmetric) [ADR-0090]`

---

## Task 4 — backstop (modes, composition, DENY-inversion, anchored safe_regex) — DONE

**TDD:** these are pure backstop tests over behavior already built in Tasks 2/3;
ran them after writing — all GREEN with NO new implementation (the correct TDD
outcome for a backstop confirming an existing surface). Added to `rbac.rs` tests:
- `url_path_all_string_modes` — exact/prefix/suffix/contains match+miss matrix.
- `url_path_composes_and_inverts_under_deny` — `not_rule { url_path }` under
  `action: DENY` through `build_from_config` + `decode_headers` (the decision matrix):
  `/allowed`→Continue, `/other`→StopAndSend.
- `url_path_composes_in_and_or_rules` — `and_rules` (both prefixes) / `or_rules`
  (either prefix).
- `url_path_anchored_safe_regex_matches_without_panic` — ADR-0090 §C anchored
  `^/allowed/[0-9]+$` through the full filter: `/allowed/42`→Continue,
  `/allowed/42?q=1`→Continue (query-strip), `/allowed/xx`→StopAndSend,
  `/allowed`→StopAndSend (full-anchor; no first-request panic — compiled at lowering).

**Evidence:** `cargo test -p envoy-filter url_path` → `6 passed` (the 4 backstop +
Task-2 `url_path_permission_exact_matches_and_strips_query` + Task-3
`url_path_principal_matches_query_stripped`).

**Commit:** `phase 37: url_path backstop — modes, composition, DENY-inversion, anchored safe_regex [ADR-0090]`

---

## Task 5 — config-validity boot-fatal backstop (ADR-0090 §D) — DONE

**TDD:** pure guard tests (§D maps to existing error paths — NO new `ConfigError`
variant per ADR-0090 §D); ran after writing — both GREEN with NO new implementation.
- `bootstrap.rs` `rbac_url_path_empty_and_unknown_are_boot_fatal` — §D 1-3 THROUGH
  a full `Permission`: `url_path: {}` (missing `path`), `url_path: { path: {} }`
  (missing mode key), `url_path: { foo: bar }` (`deny_unknown_fields`) all `is_err()`.
- `rbac.rs` `url_path_malformed_safe_regex_is_build_error` — §D 4: a `safe_regex: "["`
  url_path is rejected at `build_from_config` (the lowering `compile_safe_regex()`)
  as `Err(FilterError::InvalidConfig { .. })` — boot-fatal, NOT a first-request panic.

**Evidence:** `cargo test -p envoy-config rbac_url_path_empty` → `1 passed`;
`cargo test -p envoy-filter url_path_malformed` → `1 passed`.

**Commit:** `phase 37: url_path config-validity boot-fatal backstop (ADR-0090 §D) [ADR-0090]`
