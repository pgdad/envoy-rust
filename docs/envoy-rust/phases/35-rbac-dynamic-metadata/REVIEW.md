# Phase 35 — `35-rbac-dynamic-metadata` — Code Review (state-5)

> **Skill:** `superpowers:requesting-code-review` (state-5). A fresh `superpowers:code-reviewer`
> subagent was dispatched with CRAFTED context (the phase-35 diff `c05426f~1..bc13090`, `PLAN.md`,
> the ADR-0086 §A empirically-locked facts incl. the three material divergences A5/A6/A7, and the
> doctrine constraints D-3.2/D-3.4/D-3.8) — NOT session history (D-3.4 context isolation).

## Verdict: **APPROVE** (0 Critical, 0 Important)

§7.5 phase-done gate (f) — `REVIEW.md` approved — is SATISFIED. Combined with the state-4-verified
(a)–(e) (CI run `28125206968` @ `d1cf8a3` = `completed/success`), the FULL §7.5 phase-done gate is
COMPLETE. Per §5.2 routing, APPROVE with 0 Critical / 0 Important → advance to state-5-complete /
state-6-next (the deterministic ROADMAP row `35` → `done` flip is the SEPARATE state-6 session).

---

## Review surface

- `crates/envoy-config/src/{bootstrap.rs,lib.rs,matcher.rs}` — the `MetadataMatcher`/
  `MetadataPathSegment`/`ValueMatcher` trio + the hand-rolled `ValueMatcher` "exactly one map key"
  `Deserialize`; the `Metadata` arm on the `Permission`/`Principal` enums + visitor `"metadata"`
  arms + KEYS; `validate_metadata_matcher` + the two tree-validator arms;
  `ConfigError::RbacMetadataMatcherInvalid`; `impl ValueMatcher::matches`.
- `crates/envoy-filter/src/rbac.rs` — `RuntimePermission::Metadata`/`RuntimePrincipal::Metadata` +
  `eval_metadata` + the lowering arms + the in-process producer→consumer backstop tests.
- `tests/fixtures/0043-http-rbac-dynamic-metadata/` + `tests/differential/tests/rbac_dynamic_metadata.rs`.
- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (phase-35 subsection) +
  `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_rbac_metadata.yaml`.

## Verification run (reviewer, fresh local; all green — matches the green CI run)

- `cargo test -p envoy-config` → 506 passed, 0 failed.
- `cargo test -p envoy-filter rbac` → 29 passed, 0 failed.
- `cargo clippy -p envoy-config -p envoy-filter --all-targets -- -D warnings` → clean.
- `grep -rn unsafe` over both crates' `src/` → only the two `#![forbid(unsafe_code)]` lines + a comment.

---

## Strengths (file:line)

- **A5/A6 stricter-than-Envoy rejects correctly encoded.** `validate_metadata_matcher`
  (`bootstrap.rs:4053-4073`) checks `m.path.len() != 1`, which subsumes BOTH the empty-`path` (len 0)
  AND the multi-segment (len ≥ 2) case in one check (A4/A5). The empty-`filter` check precedes it.
  Proven by `rbac_metadata_multi_segment_path_is_fatal`, `rbac_metadata_empty_path_is_fatal`,
  `rbac_metadata_empty_filter_is_fatal` — all driven through the full `parse_bootstrap` entry-point
  (real listener→HCM→`validate_rbac_config`→tree-validator chain), not a hand-built `cfg`.
- **Hand-rolled `ValueMatcher` Deserialize is correct** (`bootstrap.rs:1349-1389`): rejects zero keys,
  rejects unknown keys (`unknown_field` → A6 boot-fatal), rejects >1 key (trailing `next_key` probe).
  `#[derive(Serialize)]` with `#[serde(rename = "string_match")]` + the same `"string_match"` literal
  in the hand-rolled Deserialize make the round-trip sound — confirmed by
  `rbac_metadata_permission_json_round_trips` (also asserts verbatim snake_case keys, backing the A1
  `/config_dump` claim). The A6 reject is proven by `rbac_metadata_rejects_present_match_value`.
- **`eval_metadata` absent-semantics correct** (`rbac.rs:88-94`): the
  `get(filter).and_then(get(key)).is_some_and(matches)` chain returns false for absent namespace,
  absent key, and value-mismatch — each covered by a dedicated test.
- **`path[0]` panic-safety is real, not assumed.** The production HCM path runs `validate_http_filters`
  (`bootstrap.rs:3215`) — which rejects `path.len() != 1` boot-fatal — BEFORE any config reaches
  `RbacFilter::build_from_config`. The doc-comment at `rbac.rs:85-87` correctly states the invariant.
- **Test pyramid is a genuine three tiers.** Unit (config parse/validate + rbac eval) + in-process
  backstop + cross-proxy differential. The backstop drives through the REAL lowering: `mid_chain_*`
  use `FilterPipeline::build_from_config` with real `HeaderToMetadata` + `Rbac` filters
  (`rbac.rs:709-757`); `metadata_composes_in_and_rules` / `metadata_principal_and_deny_inversion`
  build an `envoy_config::RbacConfig` and call `build_from_config` — exercising the
  `lower_permission`/`lower_principal` Metadata arms, not bypassing them with hand-constructed
  `RuntimePermission`. Deny probes reach the same lookup path (mismatch + absent), not allow-all.
- **Producer-before-consumer order correct** in both fixture sides (`[header_to_metadata, rbac,
  router]`) and the in-process `h2m_then_rbac_pipeline`.
- **DRY**: one `validate_metadata_matcher` helper + one `eval_metadata` helper, both called from the
  symmetric Permission/Principal arms. No duplication.
- **Doctrine clean**: no `unsafe` (both crates `#![forbid(unsafe_code)]`); no new `HttpFilterInstance`
  variant (reuses phase-10 `Rbac`); no new crate/dep; fuzz seed reuses the existing `parse_bootstrap`
  target and is git-tracked (`!`-un-ignore present in `fuzz/.gitignore`).

## Issues

### Critical
None.

### Important
None.

### Minor
- **M35-1 (carry-forward, NOT re-raised as new).** A `safe_regex` StringMatcher inside an RBAC
  `metadata` (or `header`) value parses at config-load but is never compiled → a runtime `matches`
  would panic. Explicitly documented in the `validate_metadata_matcher` NOTE (`bootstrap.rs:4041-4051`),
  the BEHAVIOR_CONTRACT §2.2 deferrals, and asserted parse-accepted-only by
  `rbac_metadata_value_safe_regex_is_parse_accepted`. Mirrors the pre-existing `Permission::Header`
  behavior. Fix home = compile RBAC SafeRegex at `rbac.rs` lowering time (covers both); a future phase.
- **Observation (not a defect).** `ConfigError::RbacMetadataMatcherInvalid` formats `path` with `{path}`
  (unquoted) in the `#[error]` string (`lib.rs:487`) vs the PLAN draft's `{path:?}`. The chosen form is
  consistent with how the RBAC tree `path` strings are built; no action needed.

## Recommendations

- None blocking. Optionally, a future phase could add a `debug_assert!(m.path.len() == 1)` inside
  `eval_metadata` to make the validator invariant self-documenting at the read site — the current
  doc-comment already states it, and production cannot reach `eval_metadata` with an unvalidated config.

## Assessment

**Ready to merge: Yes.** The implementation faithfully encodes every empirically-locked fact (A1–A7),
correctly implements both stricter-than-Envoy divergences (A5 path-len≠1 incl. empty-path; A6
non-`string_match` reject), is panic-safe in production via the boot-time validator, honors all
doctrine constraints (no unsafe / no new variant / no new dep / no new fuzz target), and is backed by
a genuine three-tier test pyramid exercising real lowering and anti-trivial deny paths. All local
verification is green and matches the green CI run `28125206968` @ `d1cf8a3`. The sole carry-forward
(M35-1 SafeRegex) is documented and out of phase-35 scope.

---

_Reviewer: fresh `superpowers:code-reviewer` subagent (D-3.4 context isolation). Findings folded
verbatim. §7.5 (f) SATISFIED → state-5-complete; state-6 close-out (ROADMAP `35` → `done`) is the
separate next session._
