# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase


**id:** **PHASE 40 (`40-accesslog-omit-empty-values`) — STATE-5 CODE-REVIEW COMPLETE (REVIEW.md APPROVE: 0 Critical / 0 Important / 1 new Minor M40-1); STATE-6 PHASE-CLOSE NEXT.** ROADMAP rows `00`-`39` are `done`; `40` is `in-progress`. The full §7.5 (a)-(f) gate is COMPLETE. The next session is the state-6 phase-close (flip ROADMAP row `40` → `done`, advance STATE to awaiting-next-planning) — see `## Next expected skill`.
**slug:** `40-accesslog-omit-empty-values`
**directory:** `docs/envoy-rust/phases/40-accesslog-omit-empty-values/` (contains `SPEC.md` + `PLAN.md` + `PROGRESS.md` + `REVIEW.md`)

**status:** **PHASE 40 — STATE-5 CODE-REVIEW COMPLETE (REVIEW.md APPROVE); STATE-6 PHASE-CLOSE NEXT.** This session (the §5 state-5 code-review, routed via `superpowers:requesting-code-review`) dispatched a fresh `superpowers:code-reviewer` subagent against the phase-40 implementation (diff `cccaaaf`..`0114afa`) with precisely-crafted context (the diff + SPEC + PLAN + ADR-0096 §A-§E, NOT session history). **Verdict: APPROVE — 0 Critical / 0 Important / 1 new Minor (M40-1: the fixture-`0048` README overstates `0047` as the flag-off multi-segment `-`-sentinel witness — doc-accuracy only, the in-process backstop covers it; carry-forward).** The reviewer independently verified §A-§E (the sentinel SWAP `-`→`""` in all four `render_op` absent sites for BOTH text + json; `encode_single_op` UNCHANGED so single-op absent stays `null`; recursive; no key-drop), ran `cargo test -p envoy-accesslog -p envoy-config -p envoy-http1` (77/532/134 green) + fmt clean, and confirmed `Cargo.lock`/`Cargo.toml` byte-unchanged (no new dep), `#![forbid(unsafe_code)]`, NO new `ConfigError`/fuzz-target, the default-off byte-preservation, and the H2-reuses-H1-wiring. **The full §7.5 (a)-(f) gate is COMPLETE:** (a)-(e) GREEN at state-4 (AUTHORITATIVE CI `28297297375` @ `c4f95b1` `completed/success`); (f) `REVIEW.md` APPROVE at this state-5. `#![forbid(unsafe_code)]` holds; NO new crate/dependency/fuzz-target; NO new `ConfigError` variant. The `### Phase-40 state-5 code-review` Notes subsection (below) carries the detail. **DECISIONS.md ledger head: ADR-0096** (count 96; next-available **ADR-0097**, reserved-but-unfired). ADR-0014 in force; ADR-0028 open; ADR-0049 governs config-validity. **ROADMAP row `40` `in-progress`; rows `00`-`39` `done`.** The next session is the state-6 phase-close — see `## Next expected skill`.

> Historical `## Active phase` status narratives — every superseded `**status:**` paragraph (all closed phases incl. the phase-37 state-1..5 sub-state pointers) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Next expected skill

Per `BOOTSTRAP_PROMPT.md` §5 state 6 + `SKILL_ROUTING.md`: phase 40 is REVIEWED + APPROVED (the full §7.5 (a)-(f) gate is COMPLETE) → the next session is the **state-6 phase-close** (the terminal step; route via `superpowers:finishing-a-development-branch`). The close-out steps (doc-only; NO code, NO re-running the §7.5 gate): (1) flip **ROADMAP row `40`** `in-progress` → `done` (close-out block citing the state-4 CI run `28297297375` @ `c4f95b1` [(a)-(e)] + the state-5 REVIEW.md APPROVE [(f)] + the implementation commit range `86971ce`..`0114afa`); (2) advance **STATE.md** to **awaiting-next-planning**: relocate the phase-40 state-5 four top-section narratives + the active-phase Notes subsection to STATE_HISTORY.md per ADR-0035, set `## Next expected skill` to "new-phase pick + state-1 brainstorm (`superpowers:brainstorming`)"; (3) carry the open Minors forward (NONE blocks): **M40-1** (fixture-`0048` README doc-accuracy) + the still-live M39-1/M39-2 + CF-39-1 + M38-2/M38-1 + M37-2/M37-1 + M36-* + M34-* + M33-* + older. Do ONE state per session (§5.1): STOP after the close-out + STATE advance; the next-phase pick (state-1 brainstorm for phase 41) is the SESSION AFTER.

**CI is green on the implementation (`c4f95b1`, the authoritative full §7.5 suite).** The state-4/5 doc-only commits are pushed; confirm CI green.

**Open carry-forward Minors (NONE blocks):**
- **M40-1** (NEW, state-5 REVIEW.md) — the fixture-`0048` README/expectations overstate `0047` as the flag-off multi-segment `-`-sentinel cross-proxy witness (it has no multi-segment-absent leaf); the in-process backstop covers it. Doc-accuracy; soften the README when the access-log fixtures are next touched.
- **M39-1 / M39-2** (mirror-enum sync doc-pointer; unbounded recursion depth) + **CF-39-1** (numeric literal leaves) — stay live.
- **M38-2** (`%DYNAMIC_METADATA%` single-op JSON quoting) + M38-1 (folded-equivalent) — stay live.
- **M37-2/M37-1 + M36-1/M36-2/M36-3 + M34-* + M33-* + the empty-`metadata_match` doc-comment + M29-1/M29-2 + M30-1/M30-2 + the phase-31 cosmetics + the HTTP-filters-family (1)-(4)** — fold into the phase that next touches each surface.

**Phase 40 is at state-5-complete (REVIEW.md APPROVE; full §7.5 (a)-(f) COMPLETE).** Phases 39/38/37/36/35/34 closed + CI-GREEN. The next session is the state-6 phase-close (§5.1 one state per session).
> Historical `## Next expected skill` narratives — every superseded next-skill pointer (all closed phases incl. the phase-37 state-1..5 sub-state pointers) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Last commit

**Phase-40 state-5 code-review — REVIEW.md APPROVE (0 Crit / 0 Imp / 1 new Minor M40-1) + STATE advance, push + confirm CI green (THIS commit):** the §5 state-5 code-review (routed via `superpowers:requesting-code-review`). A fresh `superpowers:code-reviewer` subagent reviewed the phase-40 implementation (diff `cccaaaf`..`0114afa`) against SPEC + PLAN + ADR-0096 §A-§E → **APPROVE** (0 Critical / 0 Important / 1 new Minor M40-1 — fixture README doc-accuracy, carry-forward). The reviewer independently verified §A-§E + ran tests (77/532/134 green) + fmt clean + confirmed no new dep/variant/fuzz-target + the `encode_single_op` carve-out + default-off byte-preservation + the H2-reuses-H1-wiring. **The full §7.5 (a)-(f) gate is COMPLETE** [(a)-(e) GREEN at state-4 CI `28297297375` @ `c4f95b1`; (f) REVIEW.md APPROVE]. **THIS docs-only commit** (REVIEW.md + STATE + STATE_HISTORY); state-4 four top-section narratives relocated to STATE_HISTORY.md per ADR-0035. NO code change. `#![forbid(unsafe_code)]` holds; NO new crate/dependency/fuzz-target; NO new `ConfigError` variant. **DECISIONS.md ledger head: ADR-0096** (count 96). ADR-0014 in force; ADR-0028 open. The next session is the state-6 phase-close.

> Historical `## Last commit` narratives — every superseded last-commit block (all closed phases incl. the phase-37 state-1..5 sub-state commits) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.


## Last updated

2026-06-27 (phase-40 **STATE-5 CODE-REVIEW COMPLETE — STATE-6 PHASE-CLOSE NEXT**. A fresh `superpowers:code-reviewer` subagent reviewed the implementation [diff `cccaaaf`..`0114afa`] against SPEC + PLAN + ADR-0096 §A-§E → **REVIEW.md APPROVE [0 Critical / 0 Important / 1 new Minor M40-1 (fixture-`0048` README doc-accuracy) — carry-forward]**. Reviewer independently verified §A-§E [the sentinel SWAP in all four `render_op` sites both formats; `encode_single_op` UNCHANGED; recursive; no key-drop] + tests 77/532/134 green + fmt clean + no new dep/variant/fuzz-target + default-off byte-preservation + H2-reuses-H1-wiring. **The full §7.5 (a)-(f) gate is COMPLETE** [(a)-(e) GREEN at state-4 CI `28297297375` @ `c4f95b1` `completed/success`; (f) REVIEW.md APPROVE]. `#![forbid(unsafe_code)]` holds; NO new crate/dependency/fuzz-target; NO new `ConfigError` variant. **ROADMAP row `40` `in-progress`; rows `00`-`39` `done`.** **DECISIONS.md ledger head: ADR-0096** [count 96; next-available ADR-0097]. ADR-0014 in force; ADR-0028 open. The next session is the state-6 phase-close.)

> Historical `## Last updated` notes — every superseded last-updated note (all closed phases incl. the phase-37 state-1..5 sub-state notes) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.


## Notes

> Historical Notes subsections for fully-closed phases 00-07 (ADR-numbering notes, per-phase rollovers, ADR ledgers, and the earlier-phase-carryforward + phase-00-deferral snapshots) are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

### Doctrine reminders

- Any deviation from the state machine requires `superpowers:systematic-debugging` before proceeding — see §1 Step E of `BOOTSTRAP_PROMPT.md`.
- Consult `docs/envoy-rust/SKILL_ROUTING.md` for the full phase lifecycle state machine.
- `BOOTSTRAP_PROMPT.md` §5.1: one state per session; do not chain states. Phase 40 is at state-5-complete (REVIEW.md APPROVE; full §7.5 (a)-(f) COMPLETE); the next session is the state-6 phase-close (flip ROADMAP row `40` → `done`, advance STATE to awaiting-next-planning; route via `superpowers:finishing-a-development-branch`) — do NOT also start the next-phase pick that session.
- The reviewer's R2 disposition decision (option (a) retroactive split of 05.1 vs option (b) free-standing post-05.1 sub-phase) was settled at the 05.1 state-6 commit in favor of option (b); 05.4 is the chosen sibling sub-phase. Future-reviewers reading STATE.md should understand that 05.1 is structurally closed at the preamble landing; 05.4 is a SIBLING under parent-05, not a child of 05.1; and the execution order ran 05.1 → 05.4 → 05.2 → 05.3, with 05.3 the closing sub-phase that flips parent-05 to `done`.
### Phase-40 state-5 code-review (active phase)

- **Pick (ADR-0095):** phase 40 = **`omit_empty_values`**. §6.2 LOCKED by ADR-0096 (a SENTINEL SWAP `-`→`""`, NOT key-drop; §A-§E). Implementation commit range `86971ce`..`0114afa`.
- **State-4 verification (§7.5 (a)-(e) GREEN):** AUTHORITATIVE Linux CI `28297297375` @ `c4f95b1` `completed/success` (full differential suite `0001`-`0048` + h2spec + build/test/clippy/fmt/deny + fuzz). Evidence in `PROGRESS.md`.
- **State-5 code-review (§7.5 (f)):** a fresh `superpowers:code-reviewer` subagent → **REVIEW.md APPROVE (0 Critical / 0 Important / 1 new Minor M40-1)**. Independently verified §A-§E + tests 77/532/134 + fmt + no new dep/variant/fuzz-target + the `encode_single_op` carve-out + default-off byte-preservation.
- **Carry-forwards:** M40-1 (fixture-`0048` README doc-accuracy) NEW; M39-1/M39-2 + CF-39-1 + M38-2/M38-1 + M37-*/M36-*/M34-*/M33-* + older stay live.
- **Next:** state-6 phase-close (flip ROADMAP row `40` → `done`; STATE → awaiting-next-planning).

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
