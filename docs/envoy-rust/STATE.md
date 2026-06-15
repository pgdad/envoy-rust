# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `26` — next HTTP-filter / next ROADMAP-family pick (identity + scope **TBD at the state-1 brainstorm**), lifecycle **state-1-next** (no phase artifacts yet; the state-1 brainstorm picks + scopes the phase, authors its `SPEC.md` skeleton + a new phase dir `docs/envoy-rust/phases/26-<slug>/`). Parent phase `25` is now **CLOSED** (state-6-complete — both sub-phases `25.1` + `25.2` `done`). ROADMAP rows `00`-`24` + `25.1` + `25.2` + parent `25` are ALL `done`; the sequential ROADMAP rows END at `25.2`, so the next phase's identity is chosen from the `ROADMAP.md` family-heading candidate lists (the HTTP-filters family is the natural continuation - un-shipped candidates incl. header manipulation, compression, global rate limit, ext_authz, ext_proc, oauth2, lua, wasm, adaptive concurrency; shipped so far: local_ratelimit/rbac/fault/jwt_authn/cors/csrf/buffer) at the brainstorm.
**slug:** _(assigned at the phase-26 state-1 brainstorm)_
**directory:** _(created at the phase-26 state-1 brainstorm - `docs/envoy-rust/phases/26-<slug>/`)_; no phase artifacts yet. The closed parent `docs/envoy-rust/phases/25-http-filter-buffer/` carries the parent `SPEC.md`; the closed sub-phases `25.1-h1-request-body-forwarding/` + `25.2-http-filter-buffer/` each carry all 4 artifacts (`REVIEW.md` APPROVED).

**status:** **PARENT PHASE 25 CLOSED - PHASE 26 OPEN at state-1-next (identity/scope TBD at the brainstorm).** This commit is the phase-25.2 **state-6 deterministic close-out** (BOOTSTRAP §6.1 step 4 - no skill, docs-only). It flipped BOTH ROADMAP row `25.2` AND parent row `25` to `done` (parent `25` closes because both sub-phases `25.1` [already `done`] + `25.2` [now `done`] are complete), advanced `STATE.md` to a phase-`26` placeholder at lifecycle state-1-next, relocated the superseded `25.2` state-5/state-6 top-section blocks + the `### Phase-25 state-1 brainstorm` Notes subsection verbatim to `STATE_HISTORY.md` (ADR-0035 / §4.1 inv. 9), and recorded the 4 `REVIEW.md` Minor carry-forwards in `## Notes` for the phase-26 brainstorm. NO production/test change (docs-only - the state-4 §7.5 gate already proved green at `fde99b984`; AUTHORITATIVE Linux CI `27510477930` SUCCESS per ADR-0049; state-5 `REVIEW.md` APPROVED 0C/0I/4-Minor). NO new ADR - the close surfaced no decision (**ADR-0065 stays UNFIRED**; **DECISIONS.md ledger head remains ADR-0064**, count 65). ADR-0014 in force; ADR-0028 open. No `unsafe`. Per §5.1 (one state per session) this session EXITS after the close-out; the NEXT session runs phase 26's state-1 `superpowers:brainstorming` to pick + scope the next phase.

> Historical `## Active phase` status narratives — every superseded `**status:**` paragraph (all closed phases + the active phase's prior sub-state pointers, incl. the phase-25 state-1 brainstorm pointer) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Next expected skill

Per `BOOTSTRAP_PROMPT.md` §5 state 1 + `SKILL_ROUTING.md`: phase `26` has NO artifacts yet (`SPEC.md` absent) -> the next session runs **`superpowers:brainstorming`** (state 1). The brainstorm: (1) picks the next phase's identity from the `ROADMAP.md` family-heading candidate lists (the HTTP-filters family is the natural continuation - the sequential rows END at `25.2`, so there is NO pre-numbered row `26`; the brainstorm may also surface whether a different family should lead); (2) scopes it (minimum-viable cut + explicit non-goals) and fires its scoping ADR (next available **ADR-0065**); (3) authors the `SPEC.md` skeleton + a new phase dir `docs/envoy-rust/phases/26-<slug>/`. The 4 carry-forwards from the `25.2` REVIEW.md (see `## Notes`) should be weighed during scoping. Per §5.1 the state-1 brainstorm is the NEXT session's single state.

> Historical `## Next expected skill` narratives — every superseded next-skill pointer (all closed phases + the active phase's prior sub-state pointers) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Last commit

**Phase-25.2 state-6 deterministic close-out - parent phase 25 CLOSED (THIS commit):** the docs-only bookkeeping commit (BOOTSTRAP §6.1 step 4 - no skill) closing sub-phase `25.2` and thereby parent phase `25`. It flips BOTH ROADMAP row `25.2` AND parent row `25` from `planned`/`in-progress` to `done` (with done-summaries citing all 33 Docker-gated fixtures `0001`-`0033` green simultaneously; state-4 §7.5 gate GREEN [AUTHORITATIVE Linux CI `27510477930` at `fde99b984`]; state-5 `REVIEW.md` APPROVED 0C/0I/4-Minor), advances `STATE.md` to a phase-`26` placeholder at state-1-next (next: `superpowers:brainstorming`), relocates the superseded `25.2` state-5/state-6 top-section blocks + the `### Phase-25 state-1 brainstorm` Notes subsection verbatim to `STATE_HISTORY.md` (ADR-0035), and records the 4 `REVIEW.md` Minor carry-forwards in `## Notes`. NO production/test change (docs-only); NO new ADR (**ADR-0065 UNFIRED**; ledger head **ADR-0064**). ADR-0014 in force; ADR-0028 open. No `unsafe`. Per §5.1 the NEXT session runs phase 26's state-1 brainstorm.

> Historical `## Last commit` narratives — every superseded last-commit block (all closed phases + the active phase's prior sub-state commits) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.


## Last updated

2026-06-15 (phase-25.2 **state-6 deterministic close-out - parent phase 25 CLOSED** - docs-only bookkeeping per BOOTSTRAP §6.1 step 4, no skill. Flipped BOTH ROADMAP row `25.2` AND parent row `25` to `done` [done-summaries cite all 33 Docker-gated fixtures `0001`-`0033` green; state-4 §7.5 gate GREEN at AUTHORITATIVE Linux CI `27510477930` on `fde99b984`; state-5 `REVIEW.md` APPROVED 0C/0I/4-Minor]. Advanced `STATE.md` to a phase-`26` placeholder at state-1-next [next: `superpowers:brainstorming` - pick + scope the next phase from the ROADMAP family candidate lists, fire next-available ADR-0065, author `SPEC.md` skeleton + a `docs/envoy-rust/phases/26-<slug>/` dir]. Relocated the superseded `25.2` state-5/state-6 top-section blocks + the `### Phase-25 state-1 brainstorm` Notes subsection verbatim to `STATE_HISTORY.md` [ADR-0035 / §4.1 inv. 9]. Recorded the 4 `REVIEW.md` Minor carry-forwards in `## Notes`. NO production/test change; NO new ADR [no decision surfaced -> ADR-0065 UNFIRED]; ledger head **ADR-0064**. ADR-0014 in force; ADR-0028 open. No `unsafe`. Per §5.1 the phase-26 state-1 brainstorm is the NEXT session.)

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

### Phase-26 carry-forwards (from the `25.2` REVIEW.md - weigh at the phase-26 state-1 brainstorm)

- **(1) [non-goal - architectural]** Over-limit request bodies are FULLY buffered before the 413 rejection (no streaming watermark). Documented deferred non-goal; differentially byte-identical to Envoy for the bounded fixture sizes. Revisit only if a streaming `decode_data` watermark path is ever planned.
- **(2) [doc precision]** The BEHAVIOR_CONTRACT 413-row "verified byte-exact against v1.33.0" phrasing - fixture `0033` is H1-only; the H2 over-limit path is covered by the in-process synth-decorator backstop, NOT differentially. Consider narrowing the phrasing if an H2 over-limit fixture is ever added.
- **(3) [coverage]** No standalone `== effective route limit` unit assertion (the boundary is exercised only via the over/under probes).
- **(4) [coverage]** No differential at-limit (`==`) probe in `0033` (within-limit `<` and over-limit `>` are both covered; the exact boundary is not differentially probed).
- _None block phase 25; (2)-(4) are cheap polish, (1) is architectural and only relevant to a future streaming phase._
