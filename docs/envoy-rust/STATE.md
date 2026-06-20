# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** _none_ — **phase `29` (`29-lb-maglev`) is CLOSED.** The state-6 deterministic close-out landed at THIS commit: ROADMAP row `29` flipped `in-progress → done` (rows `00`–`29` are now ALL `done`; `29` is the highest defined row — there is no row 30 yet) and STATE advances to **AWAITING NEXT PLANNING**. There is **NO active phase directory**; the four phase-29 artifacts (`SPEC.md` + `PLAN.md` + `PROGRESS.md` + `REVIEW.md`) remain at `docs/envoy-rust/phases/29-lb-maglev/`, CLOSED. The most-recently-closed phase directories — 29 `29-lb-maglev`, 28 `28-lb-ring-hash`, 27 `27-xds-eds-hot-reload`, 26 `26-xds-rds-hot-reload` — each carry all 4 artifacts.

**status:** **AWAITING NEXT PLANNING — no active phase.** Phase 29 (`MAGLEV` consistent-hashing LB — the SECOND/last deterministic byte-exact-differentiable LB policy after phase-28 `RING_HASH`) closed APPROVED: the state-5 `REVIEW.md` verdict was 0 Critical / 0 Important / 2 Minor (M29-1/M29-2 — the shared `Http1HashSweep` differential driver's RING_HASH-worded `bail!` messages + comments, cosmetic/failure-output-only), all non-gating — phase-30 carry-forwards, NOT a §5.2 state-3 re-entry. The §7.5 gate (a)-(f) was COMPLETE at state-4/5 (AUTHORITATIVE Linux CI anchor run [`27851283501`](https://github.com/pgdad/envoy-rust/actions/runs/27851283501) @ code-HEAD `1f2ad7b` — fixture `0037-lb-maglev` cross-proxy STRONG + all `0001`–`0036` green + h2spec + the `parse_bootstrap`/`jwt_parse` fuzz; (f) the approved `REVIEW.md`). The 8 task code commits `40f4e39`…`d4e31f5` landed during state-3; ADR-0072 locked the Maglev algorithm; ADR-0073 UNFIRED (single-phase). 14th consecutive clean state-5. The next session runs the **next-NEW-PHASE brainstorm** (`superpowers:brainstorming`) to SELECT + SCOPE phase `30` from `MISSION.md`'s remaining §9 feature-families — the Load-balancing family's two DETERMINISTIC byte-exact-differentiable consistent-hash policies (`ring_hash` 28 + `maglev` 29) are now BOTH done; the remaining LB policies are `least_request` / `random` (non-deterministic → need a contract-relaxation ADR before a differential phase) + `subset LB` / `locality-weighted LB` / `priority load balancing` / `panic thresholds`; other open families: network-filters, the rest of the HTTP-filters family (≈10 filters), HTTP/3+QUIC, gRPC (still ADR-0014/H2-trailers-blocked), the xDS gRPC/ADS transport, observability, runtime/hot-restart, WASM. That brainstorm creates `docs/envoy-rust/phases/30-<slug>/` + `SPEC.md` at lifecycle state-1. No code change this session (doc-only close-out); no new ADR — DECISIONS.md ledger head stays **ADR-0072** (count 73; next available **ADR-0073** — reserved-but-unfired, now free). ADR-0014 in force; ADR-0028 open.

> Historical `## Active phase` status narratives — every superseded `**status:**` paragraph (all closed phases + the active phase's prior sub-state pointers, incl. the phase-25 state-1 brainstorm pointer) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Next expected skill

Per `BOOTSTRAP_PROMPT.md` §5 state 0→1 + `SKILL_ROUTING.md`: phase 29 is CLOSED (ROADMAP row `29` = `done`); STATE is **AWAITING NEXT PLANNING** with no active phase. -> the next session runs the **next-NEW-PHASE brainstorm** (`superpowers:brainstorming`) to SELECT + SCOPE phase `30` from `MISSION.md`'s §9 feature-families (next sequential top-level id `30`; next available ADR `ADR-0073`). The new-feature-family brainstorm collapses state 0→1 in ONE docs-only commit: it picks the phase, adds the ROADMAP row, creates `docs/envoy-rust/phases/30-<slug>/` + `SPEC.md`, and lands the scoping ADR. **Candidate picks** (the brainstorm decides): the Load-balancing family is the natural continuation — `ring_hash`+`maglev` (the two deterministic byte-exact policies) are done, so the remaining LB work is `subset LB` / `locality-weighted LB` / `priority load balancing` / `panic thresholds` (deterministic, differentiable) or `least_request`/`random` (non-deterministic → need a contract-relaxation ADR first); other open families per `MISSION.md` §9. Weigh the 2 open phase-30 carry-forwards (M29-1/M29-2 — the differential driver's RING_HASH-worded diagnostics; a cheap fold whenever the differential driver is next touched). Per §5.1 (one state per session) the brainstorm is the NEXT session's single act.

> Historical `## Next expected skill` narratives — every superseded next-skill pointer (all closed phases + the active phase's prior sub-state pointers) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Last commit

**Phase-29 state-6 deterministic close-out — phase 29 CLOSED / AWAITING NEXT PLANNING (THIS commit):** the `BOOTSTRAP_PROMPT.md` §5 state-6 close-out (no skill invocation — the phase-21…28 state-6 precedents). This docs-only commit (1) flips **ROADMAP row `29`** `in-progress → done` + amends its summary with the close-out outcome (REVIEW.md APPROVED 0C/0I/2 Minor + the §7.5 gate (a)-(f) COMPLETE at CI `27851283501` @ `1f2ad7b` + the 8 task commits `40f4e39`…`d4e31f5`); (2) advances this STATE.md Active phase `29` state-5-complete/state-6-next → **AWAITING NEXT PLANNING** (the state-5 top-section blocks demoted to `_Historical_` + RELOCATED to STATE_HISTORY.md per ADR-0035 / §4.1 inv. 9); (3) RELOCATES the now-closed phase-29 Notes subsections (the consumed `### Phase-29 carry-forwards` [M28-1..M28-3] + state-1 brainstorm / state-2 PLAN-write / state-3 implementation / state-4 verification / state-5 code review) verbatim into STATE_HISTORY.md, leaving a breadcrumb; (4) opens the new `### Phase-30 carry-forwards` live block (M29-1/M29-2). NO production/test/fixture/Cargo change — docs-only → the CI run at this push is vacuous-green (the phase's differential evidence remains the state-4 CI anchor `27851283501` @ `1f2ad7b`). **DECISIONS.md ledger head: ADR-0072** (count 73; ADR-0073 reserved-but-unfired — now free; next available ADR-0073). ADR-0014 in force; ADR-0028 open. No `unsafe`. Per §5.1 (one state per session) the NEXT session runs the phase-30 state-1 NEW-PHASE brainstorm (`superpowers:brainstorming`).

> Historical `## Last commit` narratives — every superseded last-commit block (all closed phases + the active phase's prior sub-state commits) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.


## Last updated

2026-06-20 (phase-29 **state-6 deterministic close-out — phase 29 CLOSED / AWAITING NEXT PLANNING**. The `BOOTSTRAP_PROMPT.md` §5 state-6 close-out [no skill]. Flips ROADMAP row `29` `in-progress → done` + amends its summary; advances STATE Active phase → AWAITING NEXT PLANNING [the state-5 top-section blocks demoted to `_Historical_` + relocated to STATE_HISTORY.md per ADR-0035 / §4.1 inv. 9]; relocates the now-closed phase-29 Notes subsections [the consumed `### Phase-29 carry-forwards` M28-1..M28-3 + state-1→state-5 subsections] verbatim to STATE_HISTORY.md; opens the new `### Phase-30 carry-forwards` live block [M29-1/M29-2]. Phase 29 [`MAGLEV` LB] closed APPROVED — 0C/0I/2 Minor; §7.5 gate (a)-(f) COMPLETE at CI `27851283501` @ `1f2ad7b`. Docs-only → vacuous-green CI. **DECISIONS.md ledger head: ADR-0072** [count 73; next available ADR-0073, now free]. ADR-0014 in force; ADR-0028 open. No `unsafe`. Per §5.1 the NEXT session runs the phase-30 state-1 NEW-PHASE brainstorm.)

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

### HTTP-filters-family carry-forwards (from the `25.2` REVIEW.md - NOT yet consumed; weigh whenever the HTTP-filters family is re-entered)

> These were never obligations on the xDS phase 26; they remain live for whenever an HTTP-filters-family phase resumes.

- **(1) [non-goal - architectural]** Over-limit request bodies are FULLY buffered before the 413 rejection (no streaming watermark). Documented deferred non-goal; differentially byte-identical to Envoy for the bounded fixture sizes. Revisit only if a streaming `decode_data` watermark path is ever planned.
- **(2) [doc precision]** The BEHAVIOR_CONTRACT 413-row "verified byte-exact against v1.33.0" phrasing - fixture `0033` is H1-only; the H2 over-limit path is covered by the in-process synth-decorator backstop, NOT differentially. Consider narrowing the phrasing if an H2 over-limit fixture is ever added.
- **(3) [coverage]** No standalone `== effective route limit` unit assertion (the boundary is exercised only via the over/under probes).
- **(4) [coverage]** No differential at-limit (`==`) probe in `0033` (within-limit `<` and over-limit `>` are both covered; the exact boundary is not differentially probed).
- _(2)-(4) are cheap polish, (1) is architectural and only relevant to a future streaming phase._

### Phase-30 carry-forwards (from the phase-29 `REVIEW.md` M29-1..M29-2 - NOT yet consumed; weigh at the next phase's planning / whenever the differential driver is next touched)

> Both are NON-BLOCKING Minors (REVIEW.md APPROVED 0C/0I); neither an Envoy-equivalence divergence (the differential gate is green). Both attach to the shared `Http1HashSweep` differential driver, so they are naturally weighed when the next LB-family / consistent-hash phase (or any phase touching `tests/differential/src/lib.rs`) re-enters that code.

- **M29-1** the shared generic `Http1HashSweep` differential driver's five operator-facing `bail!` failure messages (`tests/differential/src/lib.rs` ~:4344–4392) hard-code RING_HASH/ring/ADR-0070 vocabulary, so a MAGLEV (fixture 0037) mismatch would print RING_HASH-worded diagnostics. Cosmetic — failure-output-only (it cannot affect a passing test or any production path; the `up1`/`su1` markers + the offending key still identify the real problem). **Fix:** thread a `policy_label: &str` / the fixture name into the driver and interpolate it, or genericize to "consistent-hash LB" + cite the active ADR. Cheapest when the differential driver is next touched; benefits BOTH fixture 0036 (RING_HASH) and 0037 (MAGLEV).
- **M29-2** the same RING_HASH wording in the driver's COMMENTS (`tests/differential/src/lib.rs` ~:4341–4377: "ring distribution", "RING_HASH selection for this key") — same root cause as M29-1; fold into the M29-1 cleanup.
