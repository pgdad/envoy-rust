# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** _(none — awaiting next planning)_. The last phase **32** (`32-accesslog-command-operators` — the Observability-family opener: the configurable access-log command-operator substitution engine + a per-`FileAccessLog` `log_format` field + the curated DETERMINISTIC operator set) CLOSED 2026-06-22 at its state-6 close-out (ROADMAP row `32` → `done`). **ROADMAP rows `00`-`32` are ALL `done`** (`32` is the highest defined row — there is no row 33 yet). No phase is currently active; the NEXT session brainstorms the next phase (`superpowers:brainstorming`) — see `## Next expected skill` for the candidate picks.
**slug:** _(none — awaiting next planning)_
**directory:** _(none — the last phase `32-accesslog-command-operators` is CLOSED; its four artifacts [SPEC + PLAN + PROGRESS + REVIEW] remain at `docs/envoy-rust/phases/32-accesslog-command-operators/`)_

**status:** **PHASE 32 (`32-accesslog-command-operators`) CLOSED 2026-06-22 — AWAITING NEXT PLANNING.** The state-6 deterministic close-out: flipped ROADMAP row `32` `in-progress` → `done` (amended its summary with the CLOSED facts + the §7.5 CI anchor); advanced STATE here. **REVIEW.md APPROVED (0 Critical / 0 Important / 6 Minor, all carry-forward); the §7.5 gate (a)-(e) GREEN at the AUTHORITATIVE Linux CI run `27941931062` @ `ecb62d3`** (fixture `0040-accesslog-command-operators` cross-proxy byte-exact custom-format line + all `0001`-`0039` green simultaneously incl. the byte-identical `0012` regression-equivalence witness + h2spec ≥95% + the 4 fuzz targets 0 crashes incl. the new `accesslog_format_parse` + build/clippy/fmt/test/deny). The phase generalized the phase-06.2 hardcoded Envoy-v3 default-format emitter into a configurable command-operator substitution engine (the FIRST concrete Observability-family phase) across the 8 task commits `7917c8a`…`cb7a191` + the state-4 STATE commit `bac972c` + the state-5 close `a21b47e` + the state-6 close (THIS commit); `#![forbid(unsafe_code)]` holds. **DECISIONS.md ledger head: ADR-0079** (count 80; next-available **ADR-0080**, reserved-but-UNFIRED — the §6.1 split that did not fire). ADR-0014 in force; ADR-0028 open. The NEXT session brainstorms the next phase (`superpowers:brainstorming`) — see `## Next expected skill`.

> Historical `## Active phase` status narratives — every superseded `**status:**` paragraph (all closed phases + the active phase's prior sub-state pointers, incl. the phase-25 state-1 brainstorm pointer) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Next expected skill

Per `BOOTSTRAP_PROMPT.md` §5 state 0/1 + `SKILL_ROUTING.md`: no phase is active (phase `32` CLOSED; ROADMAP rows `00`-`32` ALL `done`) -> the next session runs **`superpowers:brainstorming`** to pick + scope the next phase (create its `docs/envoy-rust/phases/NN-slug/` dir + `SPEC.md`; append the pick ADR — next-available **ADR-0080**). **Candidate picks (the Observability family is now OPEN after phase 32):** (i) the next Observability surface the command-operator engine unlocks — the deferred `header_to_metadata`/`set_metadata` HTTP filters now become differentially observable via a `%DYNAMIC_METADATA%` operator that slots additively into the phase-32 engine (the critical-path unlock phase 32 enabled), or the `json_format`/`typed_json_format` access-log format (deferred ADR-0078 §2.2), or a `%START_TIME(fmt)%`/additional-operator extension; (ii) the next deterministic byte-exact locally-observable HTTP filter; (iii) the Load-balancing family's remaining policies (need a contract-relaxation ADR for `least_request`/`random`, or active-HC health state for `priority`/`panic`/`locality-weighted`); (iv) a config-hardening phase (consume M30-2 `lb_policy` serde-default + other parser-strictness divergences). **CARRY-FORWARDS to weigh at the next brainstorm** (NOT consumed by phase 32; the `### Phase-32 carry-forwards` block is relocated to STATE_HISTORY.md at this close, the `### HTTP-filters-family carry-forwards` block stays live below): the **6 phase-32 REVIEW.md Minors M32-1…M32-6** (the `command_operator.rs`/accesslog/fixture-0040 polish — `enum Side`; empty-alt+`:0`; named error-field diagnostics; the in-crate default-equivalence single-record narrowing; the vestigial 0-byte `inputs/payload.bin`; the `render` 256-byte pre-alloc — fold on a future accesslog touch); the empty-`metadata_match`→fallback doc-comment; M29-1/M29-2 + M30-1 (the `Http1HashSweep` driver diagnostics / duplicated `extract_marker`); M30-2 (`lb_policy` serde-default); the phase-31 cosmetic Minors M-2/M-3; and the HTTP-filters-family (1)-(4) buffer carry-forwards.

> Historical `## Next expected skill` narratives — every superseded next-skill pointer (all closed phases + the active phase's prior sub-state pointers) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Last commit

**Phase-32 state-6 close-out — phase 32 CLOSED / AWAITING NEXT PLANNING (THIS commit):** the deterministic state-6 close-out (`BOOTSTRAP_PROMPT.md` §5 state 6). Flipped ROADMAP row `32` `in-progress` → `done` (amended its summary with the CLOSED facts + the §7.5 CI anchor `27941931062` @ `ecb62d3`); advanced STATE `32` state-5-complete/state-6-next → **AWAITING NEXT PLANNING** (no active phase; rows `00`-`32` ALL `done`). REVIEW.md APPROVED (0 Critical / 0 Important / 6 Minor, all carry-forward); §7.5 gate (a)-(e) GREEN on CI run `27941931062`. THIS docs-only commit touches ROADMAP.md + STATE.md + STATE_HISTORY.md only (NO code change); the phase-32 state-5 top-section blocks + the `### Phase-32 carry-forwards` Notes subsection are demoted to `_Historical_` + RELOCATED to STATE_HISTORY.md per ADR-0035 / §4.1 inv. 9. `#![forbid(unsafe_code)]` holds. **DECISIONS.md ledger head: ADR-0079** (count 80; next-available **ADR-0080**, reserved-but-UNFIRED). ADR-0014 in force; ADR-0028 open. The NEXT session brainstorms the next phase (`superpowers:brainstorming`).

> Historical `## Last commit` narratives — every superseded last-commit block (all closed phases + the active phase's prior sub-state commits) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.


## Last updated

2026-06-22 (phase-32 **CLOSED — AWAITING NEXT PLANNING** — the state-6 deterministic close-out. Flipped ROADMAP row `32` `in-progress` → `done` (rows `00`-`32` now ALL `done`); advanced STATE `32` state-5-complete/state-6-next → AWAITING NEXT PLANNING. REVIEW.md APPROVED (0 Critical / 0 Important / 6 Minor, all carry-forward); §7.5 gate (a)-(e) GREEN on the AUTHORITATIVE Linux CI run `27941931062` @ `ecb62d3`. Docs-only commit (ROADMAP.md + STATE.md + STATE_HISTORY.md); the phase-32 state-5 top-section blocks + the `### Phase-32 carry-forwards` Notes subsection relocated to STATE_HISTORY.md per ADR-0035 / §4.1 inv. 9. `#![forbid(unsafe_code)]` holds. **DECISIONS.md ledger head: ADR-0079** [count 80; ADR-0080 reserved-but-UNFIRED]. ADR-0014 in force; ADR-0028 open. The NEXT session brainstorms the next phase [`superpowers:brainstorming`].)

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

> Historical Notes subsection for fully-closed phase 32 (the `### Phase-32 carry-forwards` block — the open Minors that fed phase 32 but were NOT consumed: the empty-`metadata_match`→fallback doc-comment + M29-1/M29-2 + M30-1 + M30-2 + the phase-31 cosmetic Minors M-2/M-3; they continue as carry-forwards for the future phase that touches their surface — re-listed in `## Next expected skill` above alongside the 6 new phase-32 REVIEW.md Minors M32-1…M32-6), relocated at the phase-32 state-6 close-out when row `32` flipped `done`, is preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

### HTTP-filters-family carry-forwards (from the `25.2` REVIEW.md - NOT yet consumed; weigh whenever the HTTP-filters family is re-entered)

> These were never obligations on the xDS phase 26; they remain live for whenever an HTTP-filters-family phase resumes.

- **(1) [non-goal - architectural]** Over-limit request bodies are FULLY buffered before the 413 rejection (no streaming watermark). Documented deferred non-goal; differentially byte-identical to Envoy for the bounded fixture sizes. Revisit only if a streaming `decode_data` watermark path is ever planned.
- **(2) [doc precision]** The BEHAVIOR_CONTRACT 413-row "verified byte-exact against v1.33.0" phrasing - fixture `0033` is H1-only; the H2 over-limit path is covered by the in-process synth-decorator backstop, NOT differentially. Consider narrowing the phrasing if an H2 over-limit fixture is ever added.
- **(3) [coverage]** No standalone `== effective route limit` unit assertion (the boundary is exercised only via the over/under probes).
- **(4) [coverage]** No differential at-limit (`==`) probe in `0033` (within-limit `<` and over-limit `>` are both covered; the exact boundary is not differentially probed).
- _(2)-(4) are cheap polish, (1) is architectural and only relevant to a future streaming phase._
