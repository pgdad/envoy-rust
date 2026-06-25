# Phase 36 — `36-rbac-matcher-value-enrichment` — Code Review (state-5)

> **Skill:** `superpowers:requesting-code-review` (state-5). A fresh `superpowers:code-reviewer`
> subagent was dispatched with CRAFTED context (the phase-36 diff `c0ad7dc..e8f618e` = the 7 task
> commits `0e41640`..`e8f618e`, `SPEC.md`, `PLAN.md §A` empirically-locked facts A1–A5 incl. the two
> material divergences A1/A3b, `BEHAVIOR_CONTRACT.md` phase-36 subsection, and ADR-0087/ADR-0088/
> ADR-0049/ADR-0084) — NOT session history (D-3.4 context isolation).

## Verdict: **APPROVE** (0 Critical, 0 Important, 2 Minor)

§7.5 phase-done gate (f) — `REVIEW.md` approved — is SATISFIED. Combined with the state-4-verified
(a)–(e) (AUTHORITATIVE Linux CI run `28199106154` @ `19c3fe9` = `completed/success`, both jobs), the
FULL §7.5 phase-done gate is COMPLETE. Per §5.2 routing, APPROVE with 0 Critical / 0 Important → NO
state-3 re-entry; advance to state-5-complete / state-6-next (the deterministic ROADMAP row `36` →
`done` flip is the SEPARATE state-6 session). The two Minors are non-blocking carry-forwards.

---

## Review surface (the diff `c0ad7dc..e8f618e -- crates/ tests/`)

- `crates/envoy-config/src/bootstrap.rs` — F1: the `PresentMatch(bool)` arm on the `ValueMatcher`
  enum + its hand-rolled `Deserialize` visitor (`"present_match"` arm + `KEYS = ["string_match",
  "present_match"]`) + the `#[serde(rename = "present_match")]` Serialize arm. F2: the public
  `HeaderMatcher::compile_safe_regexes` / `StringMatcher::compile_safe_regex` /
  `ValueMatcher::compile_safe_regexes` helpers (reusing the private `compile_safe_regex` +
  `ConfigError::InvalidRegex`). The two repurposed in-code tests.
- `crates/envoy-config/src/matcher.rs` — F1: `ValueMatcher::matches(&str)` made exhaustive
  (`PresentMatch(want) => *want`) + the new presence-aware `ValueMatcher::matches_resolved(Option<&str>)`.
- `crates/envoy-filter/src/rbac.rs` — F1: `eval_metadata` restructured to route through
  `matches_resolved` (`match = present && want`). F2: `lower_permission`/`lower_principal` made
  fallible + compile RBAC `SafeRegex` on the owned clone (both `Header` + `Metadata`), threaded
  through `build_from_config`. The in-process backstop tests (incl. the panic-regression guard).
- `tests/fixtures/0044-http-rbac-matcher-value-enrichment/` + `tests/differential/tests/rbac_matcher_value_enrichment.rs`.
- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (phase-36 subsection) +
  `crates/envoy-config/fuzz/corpus/parse_bootstrap/{rbac_present_match.yaml,rbac_safe_regex.yaml}` (+ `.gitignore` un-ignore).

## Verification (reviewer, fresh local; all green — matches the green CI run `28199106154`)

- `cargo test -p envoy-config` → full suite green.
- `cargo test -p envoy-filter rbac` → 201 passed, 0 failed.
- No `unsafe` introduced (both crates `#![forbid(unsafe_code)]`).

---

## Strengths (file:line)

- **F1 semantics implement the LOCKED §A1 fact exactly.** `matcher.rs` `matches_resolved`:
  `PresentMatch(want) => resolved.is_some() && *want` — precisely `present && want`. Correctly
  diverges from BOTH the SPEC's original `present == want` projection AND the
  `HeaderMatcherMode::PresentMatch` precedent (`matcher.rs:42-47`, `want ? value.is_some() : true`),
  which sits in the same file — the §2.1.2 / spec-review foot-gun was avoided.
- **`ValueMatcher::matches(&str)` kept exhaustive and correct** (the value-present arm returns
  `*want`, consistent with `present && want` when the value is known present). No exhaustiveness gap
  left open across the cross-crate red window (Task 1 closed `matcher.rs` in the same commit).
- **F2 recursion is complete.** Both `lower_permission` and `lower_principal` thread `?` through every
  recursive arm — Header, Metadata, AndRules/AndIds, OrRules/OrIds, NotRule/NotId. No arm silently
  drops the compile. `build_from_config` collects into `Result<_, _>` and propagates `?` up to the
  already-fallible boot path → malformed RBAC regex is BOOT-fatal (ADR-0049), not a first-request panic.
- **Compile helpers reuse the existing private `compile_safe_regex` + `ConfigError::InvalidRegex`** and
  map to the existing `FilterError::InvalidConfig` — NO new error variant.
- **The panic-regression guard is genuine.** `header_safe_regex_matches_without_panic` would have
  panicked at `matcher.rs:90`'s `.expect(...)` on the pre-fix tree (`compiled == None`) — it truly
  exercises the M35-1 closure.
- **Anchored pattern locked everywhere** (§A3b): `^(prod|staging)$` in fixture
  `envoy.yaml`/`envoy-rust.yaml`, both fuzz seeds, the repurposed bootstrap test, and all `rbac.rs`
  backstops. No unanchored SafeRegex leaked (the M36-1 deferral stays clean).
- **Fuzz seeds are git-tracked** (the `*`-ignored corpus dir has the matching `!`-un-ignore lines;
  `git ls-files` lists both) — per memory `fuzz-corpus-seed-gitignored-by-default`.
- **Both repurposed tests handled correctly:** `rbac_metadata_rejects_present_match_value` →
  `rbac_metadata_accepts_present_match_value` (asserts acceptance + `PresentMatch(true)`);
  `rbac_metadata_value_safe_regex_is_parse_accepted` → `..._parse_accepted_and_compilable` (the stale
  "would panic at runtime" comment removed; a real `compile_safe_regexes()` + `compiled.is_some()`
  assertion added). No stale/false comments remain.
- **Regression equivalence preserved.** `matches_resolved`'s `StringMatch` arm
  (`resolved.is_some_and(|v| sm.matches(v))`) is byte-equivalent to the old `eval_metadata`
  `is_some_and(|v| m.value.matches(v))` — existing matcher behavior unchanged; `present_match` is
  additive (no existing config uses it).
- **Scope clean** (D-3.2): NO new `HttpFilterInstance` variant / crate / dependency / fuzz-target /
  `ConfigError`/`FilterError` variant; `#![forbid(unsafe_code)]` holds.

## Issues

### Critical
None.

### Important
None.

### Minor (non-blocking carry-forwards)

- **M36-2** — `ValueMatcher::matches(&str)` (`crates/envoy-config/src/matcher.rs`) is now
  production-dead: after T2, `eval_metadata` routes exclusively through `matches_resolved`; the only
  remaining `matches` callers are tests. It is `pub` (no dead-code lint), but the doc comment "Kept for
  the value-present call sites" is now inaccurate — there are no production value-present call sites.
  Fix: soften the comment (deliberately retained public-API surface / future callers) or fold the
  caller through `matches_resolved`. Fold when `matcher.rs` is next touched.
- **M36-3** — present-but-empty (§A2 / ADR-0084) correctness is enforced one filter UPSTREAM: RBAC's
  read path (`rbac.rs` `eval_metadata`) treats a stored `Some("")` as present (`is_some()` true); it is
  only correct because `header_to_metadata` writes nothing for an empty header, so RBAC never observes
  `Some("")`. Behavior is correct and matches the differential, but a one-line comment at
  `eval_metadata` noting the upstream-enforced invariant would harden it against a future producer that
  writes empty values. Doc-only; fold when `eval_metadata` is next touched.

## Assessment

**Ready to merge?** Yes.

**Reasoning:** Every LOCKED §A fact (A1 `present && want`, A4 fallible-lowering boot-fatal, A3b anchored
patterns, A5 stricter config-validity) is implemented exactly, all recursive lowering arms propagate the
compile `Result`, scope is clean (no new infrastructure), `#![forbid(unsafe_code)]` holds, and the full
envoy-config + envoy-filter suites pass with the Docker differential already green on CI `28199106154`.
The only findings are a now-vestigial `pub fn matches` (M36-2) and a documentation hardening (M36-3) —
neither blocks merge.

---

_Scope locked by **ADR-0087**; §6.2 reconciled by **ADR-0088** (the §A facts). State-5 verdict APPROVE
(0 Critical / 0 Important / 2 Minor → carry-forwards M36-2/M36-3). **M35-1 CONSUMED** by F2 (formal close
at state-6); **M36-1** (unanchored SafeRegex partial-vs-full) stays DEFERRED. The state-6 close-out (flip
ROADMAP row `36` → `done`, advance STATE to awaiting-next-planning) is the SEPARATE next session._
