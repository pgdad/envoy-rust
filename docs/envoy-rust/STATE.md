# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase


**id:** **PHASE 40 (`40-accesslog-omit-empty-values`) — STATE-2 PLAN-WRITE COMPLETE (PLAN.md authored + §6.2 recon locked [ADR-0096 FIRES] + plan-reviewed → APPROVED); STATE-3 IMPLEMENTATION NEXT.** ROADMAP rows `00`-`39` are `done`; `40` is `in-progress`. The next session is the state-3 implementation (`superpowers:executing-plans` / `subagent-driven-development`) — see `## Next expected skill`.
**slug:** `40-accesslog-omit-empty-values`
**directory:** `docs/envoy-rust/phases/40-accesslog-omit-empty-values/` (contains `SPEC.md` + `PLAN.md`)

**status:** **PHASE 40 — STATE-2 PLAN-WRITE COMPLETE; STATE-3 IMPLEMENTATION NEXT.** This session (the §5 state-2 PLAN-write, routed via `superpowers:writing-plans`) ran the §6.2 LOCAL reconnaissance against live `envoyproxy/envoy:v1.33.0` (a file logger with `omit_empty_values: true`, 5 cases), fired **ADR-0096 (FIRES — material divergence)**, authored `PLAN.md` (6 TDD tasks; plan-reviewed by a fresh subagent → APPROVED, 0 Critical / 0 Important / 2 Minor M40-A/M40-B folded), and confirmed the §6.1 split does NOT fire (**ADR-0097 reserved-but-unfired**). **§6.2 LOCKED (ADR-0096 — the SPEC's "drop-empty" projection is VOID):** §A `omit_empty_values` does NOT drop keys/entries; §B it SWAPS the absent-operator `-` sentinel for the EMPTY STRING `""` in the command-operator MULTI-SEGMENT render (`render_value_segments`→`render_op`), for BOTH `text_format` AND `json_format`; §C single-operator-TYPED `json_format` values are UNAFFECTED (`encode_single_op`: absent→`null`, unchanged); §D the swap applies RECURSIVELY (nested objects + lists; single-op nulls at depth stay `null`); §E all-single-absent → keys survive as `null` (not dropped); plain `bool`, NO new `ConfigError` variant. **PLAN:** a `omit_empty_values: bool` field on `SubstitutionFormatString` + an `omit_empty: bool` param threaded into `render_value_segments`/`render_op` (the four `.unwrap_or("-")` sites → `.unwrap_or(if omit {""} else {"-"})`) + flag-carry on `CompiledFormat`/`CompiledJsonFormat` + the HCM wiring; `encode_single_op` UNCHANGED; fixture `0048` (byte-exact sentinel swap + flag-off control) + backstop + seed + BEHAVIOR_CONTRACT. The `### Phase-40 state-2 PLAN-write` Notes subsection (below) carries the detail. `#![forbid(unsafe_code)]` holds; NO new crate/dependency/fuzz-target; NO new `ConfigError` variant. **DECISIONS.md ledger head: ADR-0096** (count 96; next-available **ADR-0097**, reserved-but-unfired). ADR-0014 in force; ADR-0028 open; ADR-0049 governs config-validity. **ROADMAP row `40` `in-progress`; rows `00`-`39` `done`.** The next session is the state-3 implementation — see `## Next expected skill`.

> Historical `## Active phase` status narratives — every superseded `**status:**` paragraph (all closed phases incl. the phase-37 state-1..5 sub-state pointers) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Next expected skill

Per `BOOTSTRAP_PROMPT.md` §5 state 3 + `SKILL_ROUTING.md`: phase 40's `SPEC.md` + `PLAN.md` exist and the implementation is incomplete → the next session is the **state-3 implementation** (`superpowers:executing-plans` / `subagent-driven-development`), TDD per task, appending to a NEW `PROGRESS.md`. `PLAN.md` has 6 TDD tasks (T1 `omit_empty_values` config field / T2 thread `omit_empty` into `render_value_segments`→`render_op` + text `CompiledFormat` [pass `false` at the json call site to avoid a T2→T3 compile-red] / T3 thread into the recursive `json_format` render [`encode_single_op` UNCHANGED] / T4 HCM wiring / T5 fixture `0048` byte-exact differential / T6 fuzz seed + BEHAVIOR_CONTRACT). Implement against the §6.2 facts LOCKED in ADR-0096 (§A-§E — sentinel SWAP `-`→`""`, NOT key-drop) — do NOT revert to the SPEC's void "drop keys" model. The differential runs `target/debug/envoy-bin` — rebuild (`cargo build -p envoy-bin`) before fixture `0048`. Do NOT start state-4 verification in the same session — §5.1 one state per session.

**Confirm CI green on the phase-40 baseline** (the state-1 commit `c5e00c6` + this state-2 commit are doc-only). The documented LOCAL host false-REDs (`admin_config_dump_server_info` bridge-IP; `envoy-http2` h2-handshake host-flake; differential fixtures under parallel load) are NOT regressions; CI is authoritative.

**Open carry-forward Minors (NONE blocks):**
- **M39-1** (mirror-enum sync doc-pointer) + **M39-2** (unbounded recursion depth) — ADJACENT (the `json_format` encoder is touched at T3); fold M39-1's `// keep in sync` doc-pointer if cheap. **CF-39-1** (numeric literal leaves) — adjacent, stays live.
- **M38-2** (`%DYNAMIC_METADATA%` single-op JSON quoting) + M38-1 (folded-equivalent) — stay live.
- **M37-2/M37-1 + M36-1/M36-2/M36-3 + M34-* + M33-* + the empty-`metadata_match` doc-comment + M29-1/M29-2 + M30-1/M30-2 + the phase-31 cosmetics + the HTTP-filters-family (1)-(4)** — fold into the phase that next touches each surface.

**Phase 40 is at state-2-complete (PLAN.md APPROVED; §6.2 locked by ADR-0096).** Phases 39/38/37/36/35/34 closed + CI-GREEN. The next session is the state-3 implementation (§5.1 one state per session).
> Historical `## Next expected skill` narratives — every superseded next-skill pointer (all closed phases incl. the phase-37 state-1..5 sub-state pointers) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Last commit

**Phase-40 state-2 PLAN-write — §6.2 recon + ADR-0096 (FIRES) + PLAN.md (6 TDD tasks, APPROVED) + STATE advance, push + confirm CI green (THIS commit):** the §5 state-2 PLAN-write (routed via `superpowers:writing-plans`). Ran the §6.2 LOCAL reconnaissance against live `envoyproxy/envoy:v1.33.0` (5 cases: json present/empty mix + control, text, nested+list, single-absent), fired **ADR-0096 (FIRES — material divergence)**: `omit_empty_values` does NOT drop keys (the SPEC's "drop-empty" projection VOID); it SWAPS the absent-operator `-` sentinel for `""` in the command-operator MULTI-SEGMENT render, for BOTH text + json, recursively; single-op-typed values (→`null`) UNAFFECTED; NO new `ConfigError` variant. Authored `PLAN.md` (6 TDD tasks; plan-reviewed by a fresh subagent → APPROVED, 0 Critical / 0 Important / 2 Minor M40-A [the `.unwrap_or` sites are in `render_op`] / M40-B [the T2→T3 compile-red] both folded), confirmed the §6.1 split does NOT fire (**ADR-0097 reserved-but-unfired**; ~120-250 LoC / ~6 tasks). Advanced STATE to phase-40 state-3-next (state-1 four top-section narratives relocated to STATE_HISTORY.md per ADR-0035; the `### Phase-40` Notes subsection updated to state-2). **THIS docs-only commit** (PLAN + DECISIONS + STATE + STATE_HISTORY). NO code change. `#![forbid(unsafe_code)]` holds; NO new crate/dependency/fuzz-target; NO new `ConfigError` variant. **DECISIONS.md ledger head: ADR-0096** (count 96; next-available **ADR-0097**). ADR-0014 in force; ADR-0028 open. The next session is the state-3 implementation.

> Historical `## Last commit` narratives — every superseded last-commit block (all closed phases incl. the phase-37 state-1..5 sub-state commits) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.


## Last updated

2026-06-27 (phase-40 **STATE-2 PLAN-WRITE COMPLETE — STATE-3 IMPLEMENTATION NEXT**. Ran the §6.2 LOCAL reconnaissance against live `envoyproxy/envoy:v1.33.0` [`omit_empty_values: true`, 5 cases], fired **ADR-0096 [FIRES — material divergence]**: §A `omit_empty_values` does NOT drop keys [the SPEC's "drop-empty" projection VOID]; §B it SWAPS the absent-operator `-` sentinel for `""` in the command-operator MULTI-SEGMENT render [`render_value_segments`→`render_op`], for BOTH `text_format` + `json_format`; §C single-op-typed json values UNAFFECTED [`encode_single_op` absent→`null`]; §D recursive; §E all-single-absent survive as `null`, plain bool, NO new `ConfigError` variant. Authored `PLAN.md` [6 TDD tasks; plan-reviewed by a fresh subagent → APPROVED, 0 Critical / 0 Important / 2 Minor M40-A/M40-B folded], confirmed the §6.1 split does NOT fire [**ADR-0097 reserved**]. Advanced STATE to state-3-next [state-1 four top-section narratives relocated to STATE_HISTORY.md per ADR-0035; the `### Phase-40` Notes subsection updated to state-2]. `#![forbid(unsafe_code)]` holds; NO new crate/dependency/fuzz-target; NO new `ConfigError` variant. **ROADMAP row `40` `in-progress`; rows `00`-`39` `done`.** **DECISIONS.md ledger head: ADR-0096** [count 96; next-available ADR-0097]. ADR-0014 in force; ADR-0028 open. The next session is the state-3 implementation [`superpowers:executing-plans` / `subagent-driven-development`].)

> Historical `## Last updated` notes — every superseded last-updated note (all closed phases incl. the phase-37 state-1..5 sub-state notes) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.


## Notes

> Historical Notes subsections for fully-closed phases 00-07 (ADR-numbering notes, per-phase rollovers, ADR ledgers, and the earlier-phase-carryforward + phase-00-deferral snapshots) are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

### Doctrine reminders

- Any deviation from the state machine requires `superpowers:systematic-debugging` before proceeding — see §1 Step E of `BOOTSTRAP_PROMPT.md`.
- Consult `docs/envoy-rust/SKILL_ROUTING.md` for the full phase lifecycle state machine.
- `BOOTSTRAP_PROMPT.md` §5.1: one state per session; do not chain states. Phase 40 is at state-2-complete (`PLAN.md` APPROVED; §6.2 locked by ADR-0096); the next session is the state-3 implementation (`superpowers:executing-plans` / `subagent-driven-development`) — TDD per task, append to `PROGRESS.md`; do NOT also start state-4 verification that session.
- The reviewer's R2 disposition decision (option (a) retroactive split of 05.1 vs option (b) free-standing post-05.1 sub-phase) was settled at the 05.1 state-6 commit in favor of option (b); 05.4 is the chosen sibling sub-phase. Future-reviewers reading STATE.md should understand that 05.1 is structurally closed at the preamble landing; 05.4 is a SIBLING under parent-05, not a child of 05.1; and the execution order ran 05.1 → 05.4 → 05.2 → 05.3, with 05.3 the closing sub-phase that flips parent-05 to `done`.
### Phase-40 state-2 PLAN-write (active phase)

- **Pick (ADR-0095):** phase 40 = **`omit_empty_values`** — Envoy's `SubstitutionFormatString` bool knob. The cheapest-strong remaining encoder knob over the now-recursive `json_format`.
- **§6.2 LOCKED (ADR-0096, the recon FIRES — the SPEC's "drop-empty" projection is VOID):** §A `omit_empty_values` does NOT drop keys/entries; §B it SWAPS the absent-operator `-` sentinel for the EMPTY STRING `""` in the command-operator MULTI-SEGMENT render (`render_value_segments`→`render_op`, the four `.unwrap_or("-")` sites), for BOTH `text_format` AND `json_format` (CASE-1/3); §C single-operator-TYPED `json_format` values UNAFFECTED (`encode_single_op`: absent→`null`, CASE-5 `{"only_absent":null}`); §D RECURSIVE (CASE-4 `{"arr":["a=",null],"nested":{"mixed":"v=","single":null}}`); §E plain `bool`, NO new `ConfigError` variant.
- **PLAN (6 TDD tasks, APPROVED):** T1 `omit_empty_values: bool` config field / T2 thread `omit_empty` into `render_value_segments`→`render_op` + text `CompiledFormat` (pass `false` at the json call site to avoid a T2→T3 compile-red) / T3 thread into the recursive `json_format` render (`encode_single_op` UNCHANGED) / T4 HCM wiring / T5 fixture `0048` byte-exact differential / T6 fuzz seed + BEHAVIOR_CONTRACT.
- **§6.1 split does NOT fire** (~120-250 LoC / ~6 tasks; **ADR-0097 reserved-but-unfired**).
- **Carry-forwards:** M39-1/M39-2 ADJACENT (encoder touched at T3); CF-39-1 + M38-2/M38-1 + M37-*/M36-*/M34-*/M33-* + older stay live.

> Historical Notes subsection for fully-closed phase 39 (the `### Phase-39 state-5 code-review (active phase)` narrative — phase 39 used a rename-in-place active-phase Notes subsection across its state-1..5 arc [the sub-state narratives superseded in place each session; the four top-section blocks relocated per-session], so only the state-5 subsection remained in `## Notes` at close — relocated at the phase-39 state-6 close-out when ROADMAP row `39` flipped `done`) is preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.
> Historical Notes subsection for fully-closed phase 38 (the `### Phase-38 state-2 PLAN-write (active phase)` narrative — phase 38 used a single active-phase Notes subsection for its whole arc; the state-3/4/5 sub-state narratives were superseded in the four-top-section archive blocks per-session, so only this state-2 subsection remained in `## Notes` at close — relocated at the phase-38 state-6 close-out when ROADMAP row `38` flipped `done`) is preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.


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

> Historical Notes subsection for fully-closed phase 32 (the `### Phase-32 carry-forwards` block — the open Minors that fed phase 32 but were NOT consumed: the empty-`metadata_match`→fallback doc-comment + M29-1/M29-2 + M30-1 + M30-2 + the phase-31 cosmetic Minors M-2/M-3; they continue as carry-forwards for the future phase that touches their surface — re-listed in `## Next expected skill` above alongside the 6 new phase-32 REVIEW.md Minors M32-1…M32-6), relocated at the phase-32 state-6 close-out when row `32` flipped `done`, is preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

> Historical Notes subsection for fully-closed phases 33+34 (the `### Phase-33 carry-forwards` block — the active-phase carry-forwards Notes that lived through phases 33 and 34: the now-CONSUMED 6 phase-32 REVIEW.md Minors M32-1…M32-6 [folded + landed at the phase-33 state-3] + the "Other still-live carry-forwards" list [the empty-`metadata_match`→fallback doc-comment + M29-1/M29-2 + M30-1 + M30-2 + the phase-31 cosmetic Minors M-2/M-3 + the HTTP-filters-family (1)-(4) — those still-live ones re-listed in `## Next expected skill` above alongside the new phase-34 Minors M34-1/M34-2/M34-3 + the phase-33 M33-1/M33-2]), relocated at the phase-34 state-6 close-out when row `34` flipped `done`, is preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

> Historical Notes subsections for fully-closed phase 35 (the `### Phase-35 state-1 brainstorm` / `### Phase-35 state-2 PLAN-write` / `### Phase-35 state-3 implementation` / `### Phase-35 state-4 verification` / `### Phase-35 state-5 code-review` narratives, relocated at the phase-35 state-6 close-out when ROADMAP row `35` flipped `done`) are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

> Historical Notes subsection for fully-closed phase 36 (the `### Phase-36 state-5 code-review` narrative — phase 36 used a rename-in-place Notes discipline, so the state-1..4 sub-state narratives were superseded in place each session and only the four top-section blocks were relocated per-session — plus the now-CLOSED detailed M35-1 carry-forward bullet [CONSUMED by phase-36 F2], relocated at the phase-36 state-6 close-out when ROADMAP row `36` flipped `done`) is preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

> Historical Notes subsection for fully-closed phase 37 (the `### Phase-37 state-3 implementation` active-phase narrative, relocated at the phase-37 state-6 close-out when ROADMAP row `37` flipped `done`) is preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

### HTTP-filters-family carry-forwards (from the `25.2` REVIEW.md - NOT yet consumed; weigh whenever the HTTP-filters family is re-entered)

> These were never obligations on the xDS phase 26; they remain live for whenever an HTTP-filters-family phase resumes.

- **(1) [non-goal - architectural]** Over-limit request bodies are FULLY buffered before the 413 rejection (no streaming watermark). Documented deferred non-goal; differentially byte-identical to Envoy for the bounded fixture sizes. Revisit only if a streaming `decode_data` watermark path is ever planned.
- **(2) [doc precision]** The BEHAVIOR_CONTRACT 413-row "verified byte-exact against v1.33.0" phrasing - fixture `0033` is H1-only; the H2 over-limit path is covered by the in-process synth-decorator backstop, NOT differentially. Consider narrowing the phrasing if an H2 over-limit fixture is ever added.
- **(3) [coverage]** No standalone `== effective route limit` unit assertion (the boundary is exercised only via the over/under probes).
- **(4) [coverage]** No differential at-limit (`==`) probe in `0033` (within-limit `<` and over-limit `>` are both covered; the exact boundary is not differentially probed).
- _(2)-(4) are cheap polish, (1) is architectural and only relevant to a future streaming phase._
