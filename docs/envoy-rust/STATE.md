# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `32` - `32-accesslog-command-operators` (**Observability family opener**: the phase-06.2 hardcoded Envoy-v3 default-format emitter generalized into a configurable **command-operator substitution engine** + a per-`FileAccessLog` `log_format` text-format field + a curated DETERMINISTIC operator set). Lifecycle **state-3 implementation COMPLETE / state-4-next** (`SPEC.md` + `PLAN.md` + `PROGRESS.md` present [scope locked by **ADR-0078**; §6.2 wire facts by **ADR-0079**]; `REVIEW.md` absent -> the next step is the state-4 §7.5 verification gate `superpowers:verification-before-completion`). **ROADMAP rows `00`-`31` ALL `done`;** row `32` `in-progress`.
**slug:** `32-accesslog-command-operators`
**directory:** `docs/envoy-rust/phases/32-accesslog-command-operators/` - carries **`SPEC.md` + `PLAN.md` + `PROGRESS.md`** (state-1/2/3 outputs). `REVIEW.md` not yet authored. `PROGRESS.md` records the 8 TDD task commits (`7917c8a`…`cb7a191`), each two-stage-reviewed (spec-compliance THEN code-quality, fresh subagents) + committed separately, with per-task SHAs / dispositions / carry-forwards. Fixture `0040` cross-proxy byte-exact differential GREEN locally; fixture `0012` byte-preserved; all `0001`-`0039` untouched (the engine is inert/default for every listener without a `log_format`).

**status:** **PHASE 32 STATE-3 IMPLEMENTATION COMPLETE / state-4-next.** Ran `superpowers:subagent-driven-development` — implemented the 8 `PLAN.md` tasks TDD-per-task, SERIAL on `main`, each two-stage-reviewed + committed separately (`7917c8a` t1 parser → `cd73763` t2 evaluator → `b5666ee` t3 default-re-expression + FileSink verbatim-emit refactor → `c869f91` t4 `log_format` config field + boot-fatal validator → `174344e` t5 config→FileSink wiring → `aad0c16` t6 fixture 0040 + byte-exact comparator → `9539796` t7 `accesslog_format_parse` fuzz + ci.yml + seed → `cb7a191` t8 BEHAVIOR_CONTRACT + close). **Cross-proxy byte-exact custom-format differential (fixture 0040) GREEN locally** (the phase's core target); `compiled_default_matches_legacy_concatenator` proves fixture `0012` byte-identical UNCHANGED; the `accesslog_format_parse` fuzz ran 200k runs / 0 crashes locally + its `ci.yml` step wired BY HAND (§7.5 gate (d)). PLAN reconciliations recorded in `PROGRESS.md`: (a) the §B name-validation rule (valid iff ≥1 backed branch) reconciling the two Task-1 plan tests; (b) H2 has NO production `FileSink::new` site — it inherits the config-derived format via `envoy_http2::HCMConfig::wrap` sharing the H1 `Arc<Http1HCMConfig>` (the only production sink site is H1 `hcm.rs:206`); (c) `FileSink::emit` gained a `flush()` to preserve fire-and-forget error surfacing after the single-write refactor. `#![forbid(unsafe_code)]` holds throughout. **DECISIONS.md ledger head: ADR-0079** (count 80; **ADR-0080** [§6.1 split] reserved-but-UNFIRED — did not fire). ADR-0014 in force; ADR-0028 open. Per §5.1 the NEXT session runs the phase-32 state-4 §7.5 verification gate — do NOT run it in this session.

> Historical `## Active phase` status narratives — every superseded `**status:**` paragraph (all closed phases + the active phase's prior sub-state pointers, incl. the phase-25 state-1 brainstorm pointer) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Next expected skill

Per `BOOTSTRAP_PROMPT.md` §5 state 4 + `SKILL_ROUTING.md`: phase `32` is `in-progress` with `SPEC.md` + `PLAN.md` + `PROGRESS.md` present and `REVIEW.md` absent -> the next session runs **`superpowers:verification-before-completion`** — the §7.5 phase-done gate: `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check`, the `accesslog_format_parse` cargo-fuzz short-budget run (+ the existing `parse_bootstrap`/`jwt_parse`/`cdn_loop_parse` targets), and the FULL Docker differential suite (all `0001`-`0040` green SIMULTANEOUSLY — incl. the new `0040` cross-proxy byte-exact custom-format line + the byte-preserved `0012`) + h2spec ≥95% — quoting every command output into `PROGRESS.md`. **The fixture-0040 differential already passed locally on this host; the AUTHORITATIVE green is the Linux CI run** (cargo-fmt-check + the full Docker differential first run authoritatively at this state-4 gate — budget CI iteration). Then state-5 `superpowers:requesting-code-review` (→ `REVIEW.md`). Do NOT chain past state-4 (§5.1).

> Historical `## Next expected skill` narratives — every superseded next-skill pointer (all closed phases + the active phase's prior sub-state pointers) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Last commit

**Phase-32 state-3 implementation COMPLETE — STATE → state-4-next (THIS commit):** the state-3 close (`BOOTSTRAP_PROMPT.md` §5 state 3 → state-4). The 8 TDD task commits `7917c8a`…`cb7a191` landed the command-operator engine end-to-end (parser → evaluator → default-re-expression + FileSink verbatim-emit → `log_format` config field + boot-fatal validator → config→FileSink wiring → fixture `0040` + byte-exact comparator [**differential GREEN locally**] → `accesslog_format_parse` fuzz + `ci.yml` + seed → BEHAVIOR_CONTRACT extension + `PROGRESS.md`). THIS commit advances STATE `32` state-2-complete/state-3-next → state-3-complete / state-4-next (the phase-32 state-2 top-section blocks demoted to `_Historical_` + RELOCATED to STATE_HISTORY.md per ADR-0035 / §4.1 inv. 9, leaving the breadcrumb). `#![forbid(unsafe_code)]` holds. **DECISIONS.md ledger head: ADR-0079** (count 80; next ADR-0080). ADR-0014 in force; ADR-0028 open. Per §5.1 the NEXT session runs the phase-32 state-4 §7.5 verification gate (`superpowers:verification-before-completion`).

> Historical `## Last commit` narratives — every superseded last-commit block (all closed phases + the active phase's prior sub-state commits) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.


## Last updated

2026-06-21 (phase-32 **state-3 implementation COMPLETE / state-4-next** — ran `superpowers:subagent-driven-development`; implemented the 8 `PLAN.md` tasks TDD-per-task SERIAL on `main`, each two-stage-reviewed + committed separately [`7917c8a`…`cb7a191`]. Cross-proxy byte-exact fixture `0040` differential GREEN locally; fixture `0012` byte-preserved; `accesslog_format_parse` fuzz 200k runs / 0 crashes + `ci.yml` wired by hand. Advanced STATE `32` state-2-complete/state-3-next → state-3-complete / state-4-next [the phase-32 state-2 top-section blocks relocated to STATE_HISTORY.md per ADR-0035, leaving the breadcrumb]. `#![forbid(unsafe_code)]` holds. **DECISIONS.md ledger head: ADR-0079** [count 80; ADR-0080 reserved-but-UNFIRED]. ADR-0014 in force; ADR-0028 open. The NEXT session runs the phase-32 state-4 §7.5 verification gate [`superpowers:verification-before-completion`].)

> Historical `## Last updated` notes — every superseded last-updated note (all closed phases + the active phase's prior sub-state notes) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.


## Notes

> Historical Notes subsections for fully-closed phases 00-07 (ADR-numbering notes, per-phase rollovers, ADR ledgers, and the earlier-phase-carryforward + phase-00-deferral snapshots) are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

### Doctrine reminders

- Any deviation from the state machine requires `superpowers:systematic-debugging` before proceeding — see §1 Step E of `BOOTSTRAP_PROMPT.md`.
- Consult `docs/envoy-rust/SKILL_ROUTING.md` for the full phase lifecycle state machine.
- `BOOTSTRAP_PROMPT.md` §5.1: one state per session; do not chain states. State-6 close-out commits touch ROADMAP.md + STATE.md only and carry no code changes; the next session writes PLAN.md for the next active phase per `superpowers:writing-plans`.
- The reviewer's R2 disposition decision (option (a) retroactive split of 05.1 vs option (b) free-standing post-05.1 sub-phase) was settled at the 05.1 state-6 commit in favor of option (b); 05.4 is the chosen sibling sub-phase. Future-reviewers reading STATE.md should understand that 05.1 is structurally closed at the preamble landing; 05.4 is a SIBLING under parent-05, not a child of 05.1; and the execution order ran 05.1 → 05.4 → 05.2 → 05.3, with 05.3 the closing sub-phase that flips parent-05 to `done`.

> Historical Notes subsections for fully-closed phases 05.4 / 08 / 09 / 10 (brainstorm, split, PLAN-write, execution-arc, rollovers, and ADR-ledger narratives) are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

> Historical Notes subsections for fully-closed phases 11–21 (brainstorm / split / PLAN-write / execution-arc + verification / code-review / rollovers narratives) are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

> Historical Notes subsections for fully-closed phase 22 (brainstorm / PLAN-write / execution-arc + state-4 verification / code-review / rollovers narratives) are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

> Historical Notes subsections for fully-closed phase 23 (state-1 brainstorm / state-2 PLAN-write / state-3 execution arc / state-4 verification gate / state-5 code-review narratives) are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

> Historical Notes subsections for fully-closed phase 24 (state-1 brainstorm / state-2 PLAN-write narratives) are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

> Historical Notes subsections for fully-closed sub-phase 25.1 (state-2 PLAN-write / state-3 implementation / state-4 verification / state-5 code-review narratives) and for parent phase 25 (the `### Phase-25 state-1 brainstorm` pick + recon-finding narrative, relocated at the 25.2 state-6 close-out when parent `25` flipped to `done`) are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

> Historical Notes subsection for fully-closed phase 26 (the `### Phase-26 state-1 brainstorm` pivot/rejected-alternatives/key-scoping narrative, relocated at the phase-26 state-6 close-out when row `26` flipped to `done`) is preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

> Historical Notes subsections for fully-closed phase 27 (the `### Phase-27 state-1 brainstorm` / `### Phase-27 state-2 PLAN-write` / `### Phase-27 state-4 verification` narratives + the now-consumed `### Phase-27 carry-forwards` [M26-1..M26-8] block, relocated at the phase-27 state-6 close-out when row `27` flipped to `done`) are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

> Historical Notes subsections for fully-closed phase 28 (the `### Phase-28 state-1 brainstorm` / `### Phase-28 state-2 PLAN-write` / `### Phase-28 state-3 implementation + state-4 verification` / `### Phase-28 state-5 code review` narratives + the now-consumed `### Phase-28 carry-forwards` [M27-1..M27-3] block, relocated at the phase-28 state-6 close-out when row `28` flipped to `done`) are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

> Historical Notes subsections for fully-closed phase 29 (the now-consumed `### Phase-29 carry-forwards` [M28-1..M28-3] block + the `### Phase-29 state-1 brainstorm` / `### Phase-29 state-2 PLAN-write` / `### Phase-29 state-3 implementation` / `### Phase-29 state-4 verification` / `### Phase-29 state-5 code review` narratives, relocated at the phase-29 state-6 close-out when row `29` flipped to `done`) are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

> Historical Notes subsections for fully-closed phase 30 (the `### Phase-30 carry-forwards` [M29-1/M29-2, which fed phase 30 but were NOT consumed — they continue as phase-31 carry-forwards] block + the `### Phase-30 state-1 brainstorm` / `### Phase-30 state-2 PLAN-write` / `### Phase-30 state-3 implementation` / `### Phase-30 state-4 verification` / `### Phase-30 state-5 code review` narratives, relocated at the phase-30 state-6 close-out when row `30` flipped to `done`) are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

> Historical Notes subsections for fully-closed phase 31 (the `### Phase-31 carry-forwards` [the empty-`metadata_match`→fallback doc-comment + M29-1/M29-2 + M30-1 + M30-2 — open Minors from the phase-30 REVIEW.md that fed phase 31 but were NOT consumed; they continue as carry-forwards for the next phase that touches the differential hash-sweep driver / the config parser] block + the `### Phase-31 state-1 brainstorm` / `### Phase-31 state-2 PLAN-write` / `### Phase-31 state-3 implementation` / `### Phase-31 state-4 verification` / `### Phase-31 state-5 code review` narratives, relocated at the phase-31 state-6 close-out when row `31` flipped to `done`) are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

### Phase-32 carry-forwards (open Minors re-stated at the phase-32 brainstorm; NOT consumed by phase 32 — Observability does not touch their surfaces)

> These open Minors (relocated to STATE_HISTORY.md at the phase-31 close in the `### Phase-31 carry-forwards` block) are re-stated here so they stay visible. Phase 32 is the access-log command-operator formatter — it touches NONE of their surfaces (the LB hash-sweep differential driver / the config parser / cdn_loop), so none is consumed this phase; each remains live for the future phase that touches its surface.

- **empty-`metadata_match`→fallback doc-comment** — an optional one-line clarifying comment at `crates/envoy-cluster/src/subset.rs` (subset-LB fallback path). Fold when the subset-LB / LB family is next touched.
- **M29-1 / M29-2 + M30-1** — the shared `Http1HashSweep` differential driver's RING_HASH-worded `bail!` diagnostics/comments (cosmetic, failure-output-only) + the route-select driver's duplicated `extract_marker` (`tests/differential/src/lib.rs`). Fold WHEN the hash-sweep / route-select differential driver is next touched.
- **M30-2** — `Cluster.lb_policy` has NO serde default. Weigh `#[serde(default)]` ROUND_ROBIN in a future config-hardening phase.
- **phase-31 cosmetic Minors M-2 / M-3** — M-2 the `count_cdn_id` empty-needle doc note; M-3 the cdn_loop cluster (`retain_mut` micro-clone / encode doc anchor / `split_on_comma` wrapper / test-helper prose). Doc/cosmetic; fold on a future `cdn_loop` touch.

### HTTP-filters-family carry-forwards (from the `25.2` REVIEW.md - NOT yet consumed; weigh whenever the HTTP-filters family is re-entered)

> These were never obligations on the xDS phase 26; they remain live for whenever an HTTP-filters-family phase resumes.

- **(1) [non-goal - architectural]** Over-limit request bodies are FULLY buffered before the 413 rejection (no streaming watermark). Documented deferred non-goal; differentially byte-identical to Envoy for the bounded fixture sizes. Revisit only if a streaming `decode_data` watermark path is ever planned.
- **(2) [doc precision]** The BEHAVIOR_CONTRACT 413-row "verified byte-exact against v1.33.0" phrasing - fixture `0033` is H1-only; the H2 over-limit path is covered by the in-process synth-decorator backstop, NOT differentially. Consider narrowing the phrasing if an H2 over-limit fixture is ever added.
- **(3) [coverage]** No standalone `== effective route limit` unit assertion (the boundary is exercised only via the over/under probes).
- **(4) [coverage]** No differential at-limit (`==`) probe in `0033` (within-limit `<` and over-limit `>` are both covered; the exact boundary is not differentially probed).
- _(2)-(4) are cheap polish, (1) is architectural and only relevant to a future streaming phase._
