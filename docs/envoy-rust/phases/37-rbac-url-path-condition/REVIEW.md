# Phase 37 — `37-rbac-url-path-condition` — Code Review (state-5)

> **Skill:** `superpowers:requesting-code-review` (state-5). A fresh `superpowers:code-reviewer`
> subagent was dispatched with CRAFTED context (the phase-37 implementation diff `58445c9..cfe03b5`,
> `SPEC.md` [scope locked by ADR-0089], `PLAN.md` [7 TDD tasks, ground truth locked by ADR-0090],
> `BEHAVIOR_CONTRACT.md` `### Phase 37`, and the fidelity invariants: query-strip semantic, RE2
> full-match/anchored discipline, fallible boot-fatal lowering, ADR-0049 config-validity, and the
> scope-negatives) — NOT session history (D-3.4 context isolation). The orchestrator independently
> re-read the full diff and verified every load-bearing claim.

## Verdict: **APPROVE** (0 Critical, 0 Important, 1 new Minor)

§7.5 phase-done gate (f) — `REVIEW.md` approved — is SATISFIED. Combined with the state-4-verified
(a)–(e) (PROGRESS.md `## State-4 verification`: build/clippy/fmt/deny exit 0; `cargo test --workspace`
GREEN modulo the documented `admin_config_dump_server_info` bridge-IP `192.168.65.2` false-RED;
`parse_bootstrap` fuzz 200000 runs no crash; differential `rbac_url_path` → `1 passed` vs live Envoy
v1.33.0; h2spec unaffected; CI green on `cfe03b5`), the FULL §7.5 phase-done gate is COMPLETE. Per §5.2
routing, APPROVE with 0 Critical / 0 Important → NO state-3 re-entry; advance to state-5-complete /
state-6-next (the deterministic ROADMAP row `37` → `done` flip is the SEPARATE state-6 session). The
single new Minor (M37-2) is a non-blocking carry-forward.

---

## Review surface

**The phase-37 implementation diff: `58445c9..cfe03b5`** (`58445c9` = the state-2 PLAN commit, last
commit before any code; `cfe03b5` = HEAD, the state-4 doc-only marker). The implementation landed as a
per-task commit series `3b69a78`..`b5b420c` (T1 PathMatcher, T2 Permission::UrlPath, T3 Principal::UrlPath,
T4 backstop, T5 config-validity §D, T6 fixture 0045, T7 parse seed + BEHAVIOR_CONTRACT); `f73cc25`
("state-3 COMPLETE") and `cfe03b5` ("state-4 COMPLETE") are doc-only STATE/PROGRESS advances. The
reviewed source/test/fixture files (`git diff --stat 58445c9..cfe03b5`, 14 files, +1060/-23):

- `crates/envoy-config/src/bootstrap.rs` — a thin DERIVED `#[serde(deny_unknown_fields)] pub struct
  PathMatcher { pub path: StringMatcher }`; `Permission::UrlPath(PathMatcher)` + `Principal::UrlPath(
  PathMatcher)` each with `#[serde(rename = "url_path")]`, the `KEYS` entry, the hand-rolled visitor arm,
  and the `validate_permission_tree`/`validate_principal_tree` leaf arm; the parse-validity in-code tests.
- `crates/envoy-config/src/lib.rs` — the `PathMatcher` re-export.
- `crates/envoy-filter/src/rbac.rs` — `RuntimePermission::UrlPath(StringMatcher)` +
  `RuntimePrincipal::UrlPath(StringMatcher)` (holds the inner `StringMatcher` directly — the `PathMatcher`
  wrapper is trivial); the `eval_permission`/`eval_principal` arms `sm.matches(strip_query(&req.path))`;
  the `lower_permission`/`lower_principal` arms calling the fallible `compile_safe_regex()?`; the free
  `strip_query` helper; the in-process backstop tests + the config-validity boot-fatal backstop test.
- `tests/fixtures/0045-http-rbac-url-path/` (envoy.yaml + envoy-rust.yaml + expectations.yaml + README)
  + `tests/differential/tests/rbac_url_path.rs`.
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_rbac_url_path.yaml` (+ the `.gitignore`
  `!`-un-ignore line).
- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` `### Phase 37` subsection.

## Verification (orchestrator + reviewer; matches the state-4 gate evidence)

- `git diff 58445c9..cfe03b5` read in full; reviewer independently re-ran the unit suite (url_path: 7
  passing in `envoy-filter`, 8 in `envoy-config`), all green.
- **Scope-negatives confirmed by diff** (`git diff --stat`/`grep`): NO `Cargo.toml`/`Cargo.lock` change
  (no new crate, no new dependency — D-3.2); NO new `ConfigError` variant; NO new `HttpFilterInstance`
  variant; NO new cargo-fuzz target (the seed reuses `parse_bootstrap`); NO `unsafe` added
  (`#![forbid(unsafe_code)]` holds in both crate roots — D-3.8).
- Fuzz seed `git ls-files`-tracked (the gitignored-corpus trap was avoided via the `!`-un-ignore line).

---

## Strengths (file:line)

- **The query-strip semantic is byte-correct against ADR-0090 §B** (`crates/envoy-filter/src/rbac.rs`
  `strip_query`). `path.split('?').next().unwrap_or(path)` returns everything before the first `?`:
  `/allowed`→`/allowed`, `/allowed?x=1`→`/allowed`, `/allowed?`→`/allowed` (empty query), `/a?b?c`→`/a`
  (first `?` wins), `""`→`""`. No percent-decode of `%3F`, no dot-segment/slash/case normalization —
  exactly ADR-0090 §B (query-strip ONLY). `#fragment` is correctly NOT stripped here (it is rejected at
  the H1 codec 400 before the filter — M37-1, out of scope). The eval arms apply it symmetrically for
  Permission and Principal. The `unwrap_or` is dead-defensive (`split` always yields ≥1 element) but
  harmless.
- **The fallible lowering is provably boot-fatal, not a first-request panic** (`rbac.rs` `lower_permission`
  / `lower_principal` url_path arms). Both `clone()` the inner `StringMatcher`, call the fallible
  `compile_safe_regex()`, and map failure → `FilterError::InvalidConfig`; the cloned-and-compiled matcher
  is what gets stored in the runtime variant, so `matcher.rs`'s `.expect("validator ensured … compiled")`
  (the M35-1 panic site) can never fire with `compiled == None`. Test
  `url_path_malformed_safe_regex_is_build_error` proves `regex: "["` → `Err(InvalidConfig)` at
  `build_from_config`. Consistent with ADR-0049 (all config-validity startup-fatal) and the phase-36
  RBAC SafeRegex treatment it reuses.
- **Full symmetry.** `Permission::UrlPath` and `Principal::UrlPath` each get: the enum variant +
  `#[serde(rename="url_path")]`, the `KEYS` entry, the hand-rolled visitor arm, the `validate_*_tree` leaf
  arm, the `Runtime*::UrlPath` variant, the `eval_*` arm, and the `lower_*` arm. Both parse paths tested
  (`permission_parses_url_path_and_json_round_trips`, `principal_parses_url_path`).
- **`PathMatcher` is a thin DERIVED struct — no hand-rolled visitor needed** (`bootstrap.rs`).
  `#[serde(deny_unknown_fields)]` + the required `path` field + the inner `StringMatcher`'s own
  "missing mode key" error make all three §D parse-validity cases boot-fatal: `{}`→missing `path`;
  `path: {}`→missing mode key; `{foo: bar}`→unknown field. Each is tested both bare
  (`path_matcher_*`) and through a full `Permission` (`rbac_url_path_empty_and_unknown_are_boot_fatal`).
  This avoids a bespoke `ConfigError` variant (SPEC §3.4(a) projected a small new variant — ADR-0090
  correctly reused serde-layer rejection instead, a leaner outcome).
- **Fixture 0045 is a true discriminator.** Probe 3 (`/allowed?x=1` → 200 under
  `url_path:{exact:/allowed}`) genuinely separates a real query-stripped implementation from a naive
  whole-`:path` compare (which would 403). `exact` (not `prefix`) is the strong witness — `prefix:/allowed`
  would match `/allowed?x=1` even WITHOUT query-strip. The differential wrapper
  `tests/differential/tests/rbac_url_path.rs` exists (without it the fixture would never run) and the
  state-4 gate recorded `rbac_url_path` → `1 passed` against live Envoy v1.33.0.
- **The anchored-SafeRegex backstop locks M36-1 correctly** (`url_path_anchored_safe_regex_matches_without_panic`):
  `^/allowed/[0-9]+$` ALLOWs `/allowed/42`, ALLOWs `/allowed/42?q=1` (query-strip), DENYs `/allowed/xx`,
  and DENYs `/allowed` (full-anchor) — proving anchored==full for the locked pattern. The cross-cutting
  partial-vs-full fix stays deferred (M36-1 weighed, NOT consumed — folding it would touch the route-config
  SafeRegex path too, out of this leaf's scope).
- **Fuzz seed exercises both surfaces and the regex path** (`hcm_rbac_url_path.yaml`): a `safe_regex`
  url_path Permission AND an `exact` url_path Principal in one bootstrap, tracked via the `!`-un-ignore
  line in `fuzz/.gitignore`.

## Issues

### Critical
None.

### Important
None.

### Minor (new this phase — carry-forward, non-blocking)

- **M37-2 (coverage): the `lower_principal` url_path arm is not exercised end-to-end.**
  `crates/envoy-filter/src/rbac.rs` `lower_principal`'s `Principal::UrlPath(pm)` arm is compiled but no
  test drives `build_from_config` with a `Principal::UrlPath` carrying a `safe_regex` — the DENY-inversion
  and safe_regex/malformed build tests all use `Permission::UrlPath` + `Principal::Any(true)`, and
  `url_path_principal_matches_query_stripped` constructs the `RuntimePrincipal::UrlPath` directly
  (bypassing lowering). The arm is trivially symmetric to the Permission arm (which IS covered by
  `url_path_malformed_safe_regex_is_build_error`), so risk is low — but a malformed-regex-in-a-Principal
  regression would not be caught by the unit suite. *Fix (optional, fold into the next phase touching
  `rbac.rs`):* add one assertion driving `build_from_config` with
  `principals: vec![Principal::UrlPath(PathMatcher{ path: <bad safe_regex> })]` asserting
  `Err(InvalidConfig)`.

## Deferred invariants — correctly handled (NOT bugs)

- **RE2 full-match vs `regex::is_match` partial (M36-1).** `StringMatcher::matches` SafeRegex still uses
  `is_match` (partial/substring), not anchored full-match. Deliberately deferred; the fixture and backstop
  lock anchored `^…$` patterns where partial==full. NOT consumed this phase (SPEC §2.2). Correct.
- **`validate_*_tree` UrlPath returns `Ok(())` without inspecting the inner SafeRegex** (`bootstrap.rs`).
  Intentional and consistent with the `Header(_)`/metadata precedent — that path is an immutable borrow;
  SafeRegex validity is enforced (boot-fatal) at lowering. Correct (ADR-0090 §D).
- **M37-1 (`#fragment` → H1-codec 400 before url_path).** A separate codec request-target surface, OUT of
  phase-37 scope; recorded in BEHAVIOR_CONTRACT §37 and carried forward. Correct.

## Recommendations

- Fold M37-2 into the next phase that touches `crates/envoy-filter/src/rbac.rs` (one assertion; trivial).
- No other action. The implementation matches the SPEC scope, the PLAN ground truth, and the
  BEHAVIOR_CONTRACT §37 contract.

## Assessment

**Ready to merge: Yes.** The implementation is faithful and clean: the query-strip semantic is
byte-correct against ADR-0090 §B, an invalid `safe_regex` is provably boot-fatal (not a panic, not a
silent skip), Permission/Principal are fully symmetric, and every scope guard holds (no new
crate/dependency/`ConfigError`/`HttpFilterInstance`/fuzz-target; `#![forbid(unsafe_code)]` intact). The
single new Minor (M37-2, an untested-but-trivially-symmetric `lower_principal` arm) is a non-blocking
coverage gap for the carry-forward list. §7.5 gate (f) SATISFIED → advance to state-6.

---

## Carry-forward Minors after phase 37

**NEW this phase:** **M37-2** (`lower_principal` url_path arm not exercised end-to-end — fold into the
next `rbac.rs`-touching phase).

**Unchanged open carry-forwards (none blocks):** M37-1 (`#fragment` → H1-codec 400, codec surface) +
M36-1 (unanchored SafeRegex partial-vs-full — phase-37 anchored-LOCKED, NOT consumed) + M36-2
(`ValueMatcher::matches(&str)` production-dead doc-comment) + M36-3 (present-empty `eval_metadata`
hardening comment) + M34-1/M34-2/M34-3 + M33-1/M33-2 + the empty-`metadata_match` doc-comment +
M29-1/M29-2 + M30-1/M30-2 + the phase-31 cosmetics + the HTTP-filters-family (1)-(4). **M35-1 remains
CLOSED** (consumed by phase-36 F2).

---

_Reviewed against `SPEC.md` (scope locked by ADR-0089), `PLAN.md` (ground truth locked by ADR-0090), and
`BEHAVIOR_CONTRACT.md` `### Phase 37`. APPROVE (0 Critical, 0 Important, 1 new Minor M37-2). §7.5 gate (f)
SATISFIED — the phase-done gate is COMPLETE; the state-6 phase-close (ROADMAP row 37 → `done`) is the
next session._
