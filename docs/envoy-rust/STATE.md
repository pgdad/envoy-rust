# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** _none_ — **phase `30` (`30-lb-subset`) is CLOSED.** The state-6 deterministic close-out landed at THIS commit: ROADMAP row `30` flipped `in-progress → done` (rows `00`–`30` are now ALL `done`; `30` is the highest defined row — there is no row 31 yet) and STATE advances to **AWAITING NEXT PLANNING**. There is **NO active phase directory**; the four phase-30 artifacts (`SPEC.md` + `PLAN.md` + `PROGRESS.md` + `REVIEW.md`) remain at `docs/envoy-rust/phases/30-lb-subset/`, CLOSED. The most-recently-closed phase directories — 30 `30-lb-subset`, 29 `29-lb-maglev`, 28 `28-lb-ring-hash`, 27 `27-xds-eds-hot-reload` — each carry all 4 artifacts.

**status:** **AWAITING NEXT PLANNING — no active phase.** Phase 30 (`subset LB` — Envoy's metadata-based endpoint-subset load balancer; the THIRD concrete Load-balancing-family phase after RING_HASH 28 + MAGLEV 29) closed APPROVED: the state-5 `REVIEW.md` verdict was 0 Critical / 0 Important / 3 Minor (1 NEW empty-`metadata_match`→fallback doc-comment + M29-1/M29-2 the shared `Http1HashSweep` driver's RING_HASH-worded diagnostics + M30-1 the duplicated `extract_marker` + M30-2 the `lb_policy` serde-default divergence — all non-gating). The §7.5 gate (a)-(f) was COMPLETE at state-4/5 (AUTHORITATIVE Linux CI anchor run [`27881837635`](https://github.com/pgdad/envoy-rust/actions/runs/27881837635) @ code-HEAD `1acf78c` — fixture `0038-lb-subset` cross-proxy route-selection STRONG + all `0001`–`0037` green + h2spec + the `parse_bootstrap`/`jwt_parse` fuzz; (f) the approved `REVIEW.md`). The 9 task code commits `9e6eb6e`…`2783e85` landed during state-3; ADR-0074 locked the subset algorithm; ADR-0075 fired (the Task-2 `default_subset` flat-Struct correction, consuming the reserved §6.1-split slot). 15th consecutive clean state-5. The next session runs the **next-NEW-PHASE brainstorm** (`superpowers:brainstorming`) to SELECT + SCOPE phase `31` from `MISSION.md`'s remaining §9 feature-families — the Load-balancing family's three DETERMINISTIC byte-exact-differentiable policies (`ring_hash` 28 + `maglev` 29 + `subset LB` 30) are now done; the remaining LB policies are `least_request` / `random` (non-deterministic → need a contract-relaxation ADR before a differential phase) + `locality-weighted LB` / `priority load balancing` / `panic thresholds` (need HC/outlier health state); other open families: network-filters, the rest of the HTTP-filters family (≈10 filters), HTTP/3+QUIC, gRPC (still ADR-0014/H2-trailers-blocked), the xDS gRPC/ADS transport, observability, runtime/hot-restart, WASM. That brainstorm creates `docs/envoy-rust/phases/31-<slug>/` + `SPEC.md` at lifecycle state-1. No code change this session (doc-only close-out); no new ADR — DECISIONS.md ledger head stays **ADR-0075** (count 76; next available **ADR-0076**). ADR-0014 in force; ADR-0028 open.

> Historical `## Active phase` status narratives — every superseded `**status:**` paragraph (all closed phases + the active phase's prior sub-state pointers, incl. the phase-25 state-1 brainstorm pointer) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Next expected skill

Per `BOOTSTRAP_PROMPT.md` §5 state 0→1 + `SKILL_ROUTING.md`: phase 30 is CLOSED (ROADMAP row `30` = `done`); STATE is **AWAITING NEXT PLANNING** with no active phase. -> the next session runs the **next-NEW-PHASE brainstorm** (`superpowers:brainstorming`) to SELECT + SCOPE phase `31` from `MISSION.md`'s §9 feature-families (next sequential top-level id `31`; next available ADR `ADR-0076`). The new-feature-family brainstorm collapses state 0→1 in ONE docs-only commit: it picks the phase, adds the ROADMAP row, creates `docs/envoy-rust/phases/31-<slug>/` + `SPEC.md`, and lands the scoping ADR. **Candidate picks** (the brainstorm decides): the Load-balancing family's three deterministic byte-exact policies (`ring_hash` 28 + `maglev` 29 + `subset LB` 30) are done, so the remaining LB work is `least_request`/`random` (non-deterministic → need a contract-relaxation ADR first) or `locality-weighted`/`priority`/`panic thresholds` (need active-HC/outlier health state); other open families per `MISSION.md` §9 (network-filters; the rest of the HTTP-filters family; HTTP/3+QUIC; gRPC still ADR-0014/H2-trailers-blocked; the xDS gRPC/ADS transport; observability; runtime/hot-restart; WASM). Weigh the open phase-31 carry-forwards (the empty-map doc-comment [NEW]; M29-1/M29-2 + M30-1 — the differential driver's RING_HASH-worded diagnostics + duplicated `extract_marker`, a cheap fold whenever the differential driver is next touched; M30-2 — the `lb_policy` serde-default, weigh in a config-hardening phase). Per §5.1 (one state per session) the brainstorm is the NEXT session's single act.

> Historical `## Next expected skill` narratives — every superseded next-skill pointer (all closed phases + the active phase's prior sub-state pointers) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Last commit

**Phase-30 state-6 deterministic close-out — phase 30 CLOSED / AWAITING NEXT PLANNING (THIS commit):** the `BOOTSTRAP_PROMPT.md` §5 state-6 close-out (no skill invocation — the phase-21…29 state-6 precedents). This docs-only commit (1) flips **ROADMAP row `30`** `in-progress → done` + amends its summary with the close-out outcome (REVIEW.md APPROVED 0C/0I/3 Minor + the §7.5 gate (a)-(f) COMPLETE at CI `27881837635` @ `1acf78c` + the 9 task commits `9e6eb6e`…`2783e85`); (2) advances this STATE.md Active phase `30` state-5-complete/state-6-next → **AWAITING NEXT PLANNING** (the state-5 top-section blocks demoted to `_Historical_` + RELOCATED to STATE_HISTORY.md per ADR-0035 / §4.1 inv. 9); (3) RELOCATES the now-closed phase-30 Notes subsections (the `### Phase-30 carry-forwards` [M29-1/M29-2] + state-1 brainstorm / state-2 PLAN-write / state-3 implementation / state-4 verification / state-5 code review) verbatim into STATE_HISTORY.md, leaving a breadcrumb; (4) opens the new `### Phase-31 carry-forwards` live block (the empty-map doc-comment + M29-1/M29-2 + M30-1 + M30-2). NO production/test/fixture/Cargo change — docs-only → the CI run at this push is vacuous-green (the phase's differential evidence remains the state-4 CI anchor `27881837635` @ `1acf78c`). **DECISIONS.md ledger head: ADR-0075** (count 76; next available ADR-0076). ADR-0014 in force; ADR-0028 open. No `unsafe`. Per §5.1 (one state per session) the NEXT session runs the phase-31 state-1 NEW-PHASE brainstorm (`superpowers:brainstorming`).

> Historical `## Last commit` narratives — every superseded last-commit block (all closed phases + the active phase's prior sub-state commits) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.


## Last updated

2026-06-20 (phase-30 **state-6 deterministic close-out — phase 30 CLOSED / AWAITING NEXT PLANNING**. The `BOOTSTRAP_PROMPT.md` §5 state-6 close-out [no skill]. Flips ROADMAP row `30` `in-progress → done` + amends its summary; advances STATE Active phase → AWAITING NEXT PLANNING [the state-5 top-section blocks demoted to `_Historical_` + relocated to STATE_HISTORY.md per ADR-0035 / §4.1 inv. 9]; relocates the now-closed phase-30 Notes subsections [the `### Phase-30 carry-forwards` M29-1/M29-2 + state-1→state-5 subsections] verbatim to STATE_HISTORY.md; opens the new `### Phase-31 carry-forwards` live block [the empty-map doc-comment + M29-1/M29-2 + M30-1 + M30-2]. Phase 30 [`subset LB`] closed APPROVED — 0C/0I/3 Minor; §7.5 gate (a)-(f) COMPLETE at CI `27881837635` @ `1acf78c`. Docs-only → vacuous-green CI. **DECISIONS.md ledger head: ADR-0075** [count 76; next available ADR-0076]. ADR-0014 in force; ADR-0028 open. No `unsafe`. Per §5.1 the NEXT session runs the phase-31 state-1 NEW-PHASE brainstorm.)

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

### HTTP-filters-family carry-forwards (from the `25.2` REVIEW.md - NOT yet consumed; weigh whenever the HTTP-filters family is re-entered)

> These were never obligations on the xDS phase 26; they remain live for whenever an HTTP-filters-family phase resumes.

- **(1) [non-goal - architectural]** Over-limit request bodies are FULLY buffered before the 413 rejection (no streaming watermark). Documented deferred non-goal; differentially byte-identical to Envoy for the bounded fixture sizes. Revisit only if a streaming `decode_data` watermark path is ever planned.
- **(2) [doc precision]** The BEHAVIOR_CONTRACT 413-row "verified byte-exact against v1.33.0" phrasing - fixture `0033` is H1-only; the H2 over-limit path is covered by the in-process synth-decorator backstop, NOT differentially. Consider narrowing the phrasing if an H2 over-limit fixture is ever added.
- **(3) [coverage]** No standalone `== effective route limit` unit assertion (the boundary is exercised only via the over/under probes).
- **(4) [coverage]** No differential at-limit (`==`) probe in `0033` (within-limit `<` and over-limit `>` are both covered; the exact boundary is not differentially probed).
- _(2)-(4) are cheap polish, (1) is architectural and only relevant to a future streaming phase._

### Phase-31 carry-forwards (open Minors from the phase-30 `REVIEW.md` - NOT yet consumed; weigh at the next phase's planning / whenever the differential driver or config parser is next touched)

> All NON-BLOCKING (phase-30 `REVIEW.md` APPROVED 0C/0I); none is an Envoy-equivalence divergence (the §7.5 differential gate is green). M29-1/M29-2 + M30-1 attach to `tests/differential/src/lib.rs` (fold when that driver is next touched); M30-2 is a config-parser hardening item; the empty-map note is a one-line doc comment.

- **empty-`metadata_match`→fallback doc-comment (NEW, phase-30 review)** — `subset.rs:106-107` routes any empty-but-present `metadata_match` map to `fallback()`, an internally-consistent inference NOT observed against live Envoy at §6.2 (the oracle only saw absent or non-empty). No current route emits an empty map. **Fix (optional):** a one-line comment flagging the disposition as inferred (not §6.2-locked), so a future maintainer enabling an empty-map route knows it is unverified.
- **M29-1/M29-2** the shared generic `Http1HashSweep` differential driver's `bail!` failure messages + comments (`tests/differential/src/lib.rs`) hard-code RING_HASH/ring/ADR-0070 vocabulary. Cosmetic — failure-output-only (a passing test / production path is unaffected). UNTOUCHED by phase 30 (fixture 0038 uses the NEW `Http1RouteSelect` driver, NOT the hash-sweep). **Fix:** genericize to "consistent-hash LB" / thread a policy label. Cheapest when the differential driver is next touched.
- **M30-1** the new route-select driver's `extract_marker` duplicates the hash-sweep driver's copy (~13 lines, `tests/differential/src/lib.rs`). Fold a shared module-scope neutral-worded `extract_backend_marker` helper together with the M29-1/M29-2 cleanup when the hash-sweep driver is next touched.
- **M30-2** envoy-rust's `Cluster.lb_policy` has NO serde default; Envoy defaults it to ROUND_ROBIN, so a cluster config omitting `lb_policy` boots on Envoy but is REJECTED by envoy-rust (`missing field lb_policy`). Pre-existing parser-strictness divergence — weigh `#[serde(default)]` ROUND_ROBIN in a future config-hardening phase.
