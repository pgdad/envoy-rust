# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** _(none — awaiting next planning)_. The last phase **31** (`31-http-filter-cdn-loop`, `envoy.filters.http.cdn_loop` — the RFC 8586 CDN-Loop request-header filter) CLOSED 2026-06-21 at its state-6 close-out (ROADMAP row `31` → `done`). **ROADMAP rows `00`-`31` are ALL `done`.** No phase is currently active; the NEXT session brainstorms the next phase (`superpowers:brainstorming`) — see `## Next expected skill` for the candidate picks.
**slug:** _(none — awaiting next planning)_
**directory:** _(none — the last phase `31-http-filter-cdn-loop` is CLOSED; its artifacts [SPEC + PLAN + PROGRESS + REVIEW] remain at `docs/envoy-rust/phases/31-http-filter-cdn-loop/`)_

**status:** **PHASE 31 (`31-http-filter-cdn-loop`) CLOSED 2026-06-21 — AWAITING NEXT PLANNING.** The state-6 deterministic close-out: flipped ROADMAP row `31` `in-progress` → `done` (amended its summary with the CLOSED facts + the §7.5 CI anchor); advanced STATE here. **REVIEW.md APPROVED (0 Critical / 0 Important / 3 Minor); the §7.5 gate GREEN at the AUTHORITATIVE Linux CI run `27915239054` @ `a2051b2`** (fixture `0039-http-filter-cdn-loop` cross-proxy STRONG byte-exact + all `0001`-`0038` green simultaneously + h2spec ≥95% + the `parse_bootstrap`/`cdn_loop_parse` fuzz + build/clippy/fmt/test/deny). The phase shipped the RFC 8586 `cdn_loop` HTTP filter (the 9th concrete HTTP-filter-family phase) across the 7 task commits `71e43cd`…`583e7c2` + the state-4 CI-fuzz-wiring fix `a2051b2` + the state-5 close `177a866`; `#![forbid(unsafe_code)]` holds. **DECISIONS.md ledger head: ADR-0077** (count 78; next-available **ADR-0078**, reserved-but-UNFIRED — the §6.1 split that did not fire). ADR-0014 in force; ADR-0028 open. The NEXT session brainstorms the next phase (`superpowers:brainstorming`) — see `## Next expected skill`.

> Historical `## Active phase` status narratives — every superseded `**status:**` paragraph (all closed phases + the active phase's prior sub-state pointers, incl. the phase-25 state-1 brainstorm pointer) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Next expected skill

Per `BOOTSTRAP_PROMPT.md` §5 state 0/1 + `SKILL_ROUTING.md`: no phase is active (phase `31` CLOSED) -> the next session runs **`superpowers:brainstorming`** to pick + scope the next phase (create its `docs/envoy-rust/phases/NN-slug/` dir + `SPEC.md`; append the pick ADR — next-available **ADR-0078**). **Candidate picks (from the phase-31 brainstorm, ADR-0076):** (i) the HTTP-filters family's next deterministic byte-exact locally-observable maximal-reuse filter; (ii) the **Observability access-log-operators phase** — a STRONG family-opener deferred at the phase-31 brainstorm (moderate differential risk from excluding timing operators), which would also unlock `header_to_metadata`/`set_metadata` (they need a `%DYNAMIC_METADATA%` access-log operator to be observable); (iii) a return to the Load-balancing family's remaining policies (need a contract-relaxation ADR for `least_request`/`random`, or active-HC health state for `priority`/`panic`/`locality-weighted`); (iv) a config-hardening phase (consume M30-2 `lb_policy` serde-default + other parser-strictness divergences). **CARRY-FORWARDS to weigh at the next brainstorm** (NOT consumed by phase 31; recorded in the relocated `### Phase-31 carry-forwards` + the still-live `### HTTP-filters-family carry-forwards` blocks): the empty-`metadata_match`→fallback doc-comment; M29-1/M29-2 + M30-1 (the `Http1HashSweep` driver RING_HASH-worded diagnostics / duplicated `extract_marker` — fold when that driver is next touched); M30-2 (`lb_policy` serde-default); the phase-31 cosmetic Minors M-2/M-3; and the HTTP-filters-family (1)-(4) buffer carry-forwards (still live below).

> Historical `## Next expected skill` narratives — every superseded next-skill pointer (all closed phases + the active phase's prior sub-state pointers) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Last commit

**Phase-31 state-6 close-out — ROADMAP row `31` → `done`, STATE → AWAITING NEXT PLANNING (THIS commit):** the deterministic close (BOOTSTRAP_PROMPT.md §5 state 6; ROADMAP.md + STATE.md + STATE_HISTORY.md only, NO code change). REVIEW.md was APPROVED (0C/0I/3 Minor) at state-5; the §7.5 gate is GREEN at the AUTHORITATIVE Linux CI run `27915239054` @ `a2051b2`. THIS commit (1) flips **ROADMAP row `31`** `in-progress` → `done` + amends its summary with the CLOSED facts; (2) advances STATE `31` state-5-complete/state-6-next → `done` / AWAITING NEXT PLANNING (the state-5 top-section blocks demoted to `_Historical_` + RELOCATED to STATE_HISTORY.md per ADR-0035 / §4.1 inv. 9; the five `### Phase-31 …` Notes subsections + the `### Phase-31 carry-forwards` block RELOCATED to a new STATE_HISTORY.md `## Notes` section, leaving the breadcrumb). `#![forbid(unsafe_code)]` holds. **DECISIONS.md ledger head: ADR-0077** (count 78; next ADR-0078). ADR-0014 in force; ADR-0028 open. The NEXT session brainstorms the next phase (`superpowers:brainstorming`).

> Historical `## Last commit` narratives — every superseded last-commit block (all closed phases + the active phase's prior sub-state commits) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.


## Last updated

2026-06-21 (phase-31 **CLOSED — AWAITING NEXT PLANNING** - the state-6 deterministic close-out. Flipped ROADMAP row `31` `in-progress` → `done` (amended its summary with the CLOSED facts + the §7.5 CI anchor `27915239054` @ `a2051b2`); advanced STATE `31` → `done` / awaiting next planning [the state-5 top-section blocks relocated to STATE_HISTORY.md per ADR-0035; the five `### Phase-31 …` Notes subsections + the `### Phase-31 carry-forwards` block relocated to a new STATE_HISTORY.md `## Notes` section, leaving the breadcrumb]. REVIEW.md APPROVED 0C/0I/3 Minor; the §7.5 gate GREEN on CI `27915239054`. The RFC 8586 `cdn_loop` filter shipped across `71e43cd`…`583e7c2` + `a2051b2` + `177a866`. `#![forbid(unsafe_code)]` holds. **DECISIONS.md ledger head: ADR-0077** [count 78; next ADR-0078]. ADR-0014 in force; ADR-0028 open. The NEXT session brainstorms the next phase [`superpowers:brainstorming`].)

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

### HTTP-filters-family carry-forwards (from the `25.2` REVIEW.md - NOT yet consumed; weigh whenever the HTTP-filters family is re-entered)

> These were never obligations on the xDS phase 26; they remain live for whenever an HTTP-filters-family phase resumes.

- **(1) [non-goal - architectural]** Over-limit request bodies are FULLY buffered before the 413 rejection (no streaming watermark). Documented deferred non-goal; differentially byte-identical to Envoy for the bounded fixture sizes. Revisit only if a streaming `decode_data` watermark path is ever planned.
- **(2) [doc precision]** The BEHAVIOR_CONTRACT 413-row "verified byte-exact against v1.33.0" phrasing - fixture `0033` is H1-only; the H2 over-limit path is covered by the in-process synth-decorator backstop, NOT differentially. Consider narrowing the phrasing if an H2 over-limit fixture is ever added.
- **(3) [coverage]** No standalone `== effective route limit` unit assertion (the boundary is exercised only via the over/under probes).
- **(4) [coverage]** No differential at-limit (`==`) probe in `0033` (within-limit `<` and over-limit `>` are both covered; the exact boundary is not differentially probed).
- _(2)-(4) are cheap polish, (1) is architectural and only relevant to a future streaming phase._
