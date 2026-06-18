# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `26` - `26-xds-rds-hot-reload` (xDS / dynamic-config family: **file-based RDS hot reload** - a running HCM whose RDS-supplied route table is re-read on file change and atomically swapped onto live traffic WITHOUT a restart; the family's prime follow-up + the FIRST post-construction live mutation in the project), lifecycle **state-5 review COMPLETE (`REVIEW.md` APPROVED) / state-6-next** (`SPEC.md` + `PLAN.md` + `PROGRESS.md` + `REVIEW.md` all present; `REVIEW.md` APPROVED [0 Critical / 0 Important / 8 Minor non-blocking] -> the next step is the state-6 close-out). The fifth concrete xDS-family phase (after 18 CDS / 19 LDS / 20 RDS / 21 EDS). ROADMAP rows `00`-`25.2` + parent `25` are `done`; row `26` is `in-progress` (flips `done` at the state-6 close-out).
**slug:** `26-xds-rds-hot-reload`
**directory:** `docs/envoy-rust/phases/26-xds-rds-hot-reload/` - carries **`SPEC.md` + `PLAN.md` + `PROGRESS.md` + `REVIEW.md`** (`REVIEW.md` APPROVED at state-5). Reuse sources: the phase-20 RDS load path (`docs/envoy-rust/phases/20-xds-file-based-rds/`), the `envoy-health::Scheduler` periodic-task primitive (`crates/envoy-health/src/scheduler.rs:40`), and fixture `0028-xds-file-based-rds` (the §5.2 idle-watcher regression witness).

**status:** **PHASE 26 (`26-xds-rds-hot-reload`) STATE-5 CODE REVIEW COMPLETE - `REVIEW.md` APPROVED (0 Critical / 0 Important / 8 Minor non-blocking); next is the state-6 close-out** (lifecycle state-5-complete / state-6-next). Ran `superpowers:requesting-code-review` — TWO-stage (spec-compliance THEN code-quality), each a fresh `superpowers:code-reviewer` subagent over the full phase-26 production+test diff (`c8b2ffc`..`73712df`, ~2990 insertions). **Stage 1 (spec): APPROVE** (0C/0I/3 Minor); **Stage 2 (quality): APPROVE-WITH-MINORS** (0C/0I/5 Minor). Both confirmed: all 10 PLAN deliverables real + non-stubbed; the concurrency core (read-once §5.4 snapshot, reparse-outside-lock single-move swap, `ResolvedRoute`) correct; the error classifier provably exhaustive; the counter taxonomy + warm-reject buckets exactly per ADR-0066; both intended divergences (unknown-cluster warm-reject; the ADR-0028 H1×H2 deferral) honestly documented per D-3.3; `watched_directory` correctly N/A. The **8 Minors** (`REVIEW.md` M26-1..M26-8; headline: M26-1 the H1 double-snapshot vs the §5.4 read-once wording [bounded, NOT an Envoy-equivalence divergence — differential green]; M26-2 the non-discriminating-discriminator CI-path guard) are NON-BLOCKING and carried to phase-27 planning per the established Minor-carry-forward pattern. Per §5.2 an approved review with no Critical/Important does NOT re-enter state-3. **§7.5 gate (a)-(e) GREEN at CI `27708943522`; (f) satisfied by this approved `REVIEW.md`.** **NEXT = state-6 close-out** (one docs-only commit `phase 26: <title> [ADR-0065, ADR-0066]` per §5.3, touching ROADMAP.md + STATE.md only [NO code], flip ROADMAP row `26` -> `done`, advance STATE to the next phase / awaiting-next-planning [the next session brainstorms phase 27]). **DECISIONS.md ledger head: ADR-0066** (count 67; ADR-0067 reserved + UNFIRED). ADR-0014 in force; ADR-0028 open. No `unsafe`. Per §5.1 the NEXT session runs phase-26 state-6.

> Historical `## Active phase` status narratives — every superseded `**status:**` paragraph (all closed phases + the active phase's prior sub-state pointers, incl. the phase-25 state-1 brainstorm pointer) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Next expected skill

Per `BOOTSTRAP_PROMPT.md` §5 state 6 + `SKILL_ROUTING.md`: phase `26` has `SPEC.md` + `PLAN.md` + `PROGRESS.md` + `REVIEW.md` (APPROVED, 0 Critical / 0 Important / 8 Minor non-blocking) -> the next session performs the **state-6 close-out**: one docs-only commit (message `phase 26: file-based RDS hot reload [ADR-0065, ADR-0066]` per §5.3, touching ROADMAP.md + STATE.md only, NO code), flip ROADMAP row `26` `in-progress` -> `done`, and advance STATE to the next phase ("awaiting next planning" — the next session brainstorms phase 27 per `superpowers:brainstorming`, the first un-entered Feature-Family candidate). Relocate the superseded STATE.md narrative to STATE_HISTORY.md per ADR-0035. **The 8 `REVIEW.md` Minors (M26-1..M26-8) carry forward to phase-27 planning** (notably M26-1 the H1 read-once double-snapshot and M26-2 the discriminator guard). Per §5.1 one state per session (the close-out is its own session-step).

> Historical `## Next expected skill` narratives — every superseded next-skill pointer (all closed phases + the active phase's prior sub-state pointers) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Last commit

**Phase-26 state-5 code review (THIS session):** `superpowers:requesting-code-review`, two-stage (spec THEN quality) via fresh `superpowers:code-reviewer` subagents over `c8b2ffc`..`73712df`. Output: `docs/envoy-rust/phases/26-xds-rds-hot-reload/REVIEW.md` (APPROVED; 0 Critical / 0 Important / 8 Minor). This docs-only commit adds `REVIEW.md` + the state-5 PROGRESS entry + this STATE advance (superseded state-4 top-section blocks relocated to STATE_HISTORY.md per ADR-0035). Sits on the Task-10 commit `73712df`. **NEXT = state-6 close-out (`phase 26: ...` commit + ROADMAP row 26 -> done + STATE -> next phase).**

> Historical `## Last commit` narratives — every superseded last-commit block (all closed phases + the active phase's prior sub-state commits) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.


## Last updated

2026-06-18 (phase-26 **state-5 code review COMPLETE - `REVIEW.md` APPROVED** - ran `superpowers:requesting-code-review`, two-stage (spec-compliance APPROVE 0C/0I/3 Minor THEN code-quality APPROVE-WITH-MINORS 0C/0I/5 Minor) via fresh `superpowers:code-reviewer` subagents over the full phase-26 diff `c8b2ffc`..`73712df`. 0 Critical / 0 Important; 8 non-blocking Minor (`REVIEW.md` M26-1..M26-8, headline: the H1 double-snapshot §5.4-wording nuance + the non-discriminating-discriminator CI guard) carried to phase-27 planning. §7.5 gate (a)-(e) GREEN at CI `27708943522`, (f) satisfied. Docs-only session (REVIEW.md + PROGRESS state-5 entry + STATE advance + ADR-0035 relocation). **NEXT = state-6 close-out** (`phase 26: ...` commit + ROADMAP 26 -> done + STATE -> next phase). Ledger head **ADR-0066** (count 67; ADR-0067 reserved + UNFIRED). No `unsafe`. Per §5.1 the NEXT session runs phase-26 state-6.)

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

### Phase-26 carry-forwards (from the `25.2` REVIEW.md - weigh at the phase-26 state-2 PLAN-write)

- **(1) [non-goal - architectural]** Over-limit request bodies are FULLY buffered before the 413 rejection (no streaming watermark). Documented deferred non-goal; differentially byte-identical to Envoy for the bounded fixture sizes. Revisit only if a streaming `decode_data` watermark path is ever planned.
- **(2) [doc precision]** The BEHAVIOR_CONTRACT 413-row "verified byte-exact against v1.33.0" phrasing - fixture `0033` is H1-only; the H2 over-limit path is covered by the in-process synth-decorator backstop, NOT differentially. Consider narrowing the phrasing if an H2 over-limit fixture is ever added.
- **(3) [coverage]** No standalone `== effective route limit` unit assertion (the boundary is exercised only via the over/under probes).
- **(4) [coverage]** No differential at-limit (`==`) probe in `0033` (within-limit `<` and over-limit `>` are both covered; the exact boundary is not differentially probed).
- _None block phase 25; (2)-(4) are cheap polish, (1) is architectural and only relevant to a future streaming phase. (None are obligations on phase 26 - they are HTTP-filter-family items; phase 26 is xDS hot-reload. Carried for whenever the HTTP-filters family is re-entered.)_

### Phase-26 state-1 brainstorm

- **Pivot:** phase `26` LEAVES the HTTP-filters family for the **xDS / dynamic-config family's prime follow-up, hot reload**, scoped minimum-viable to **file-based RDS hot-reload** (`26-xds-rds-hot-reload`). The HTTP-filters light/deterministic vein is EXHAUSTED after buffer (ADR-0062): all eight shipped filters are header- or whole-body decode-side decisions, and every remaining candidate is external-service (ext_authz/ext_proc/global ratelimit/oauth2), embedded-engine (lua/wasm), or non-deterministic/byte-fragile (compression/adaptive_concurrency/bandwidth_limit/admission_control) - a doctrine departure for a clean next cut. xDS hot reload has been the named "prime follow-up" since phase 18 with explicitly-increasing ROI; RDS is the FIRST hot-reload target because its live mutation is the LEAST invasive - the route table is already `route_config: Arc<RouteConfiguration>` (`crates/envoy-http1/src/hcm.rs:122`) and route matching is per-request stateless, so hot-reload is an atomic pointer swap (no drain / socket churn / pool-health-outlier lifecycle). See ADR-0065.

- **Rejected alternatives (ADR-0065):** the remaining HTTP filters (external-service / engine / non-deterministic - doctrine departure); the LB-family opener (foundational but needs a NEW distinguishable-backend harness; deferred at the phase-25 brainstorm as "larger/riskier" - RDS hot-reload needs at most a two-distinguishable-cluster pair, a far smaller step); EDS/CDS/LDS hot-reload first (harder live mutations - endpoint-pool churn / cluster lifecycle / socket bind-drain; they layer onto this phase's watcher + atomic-swap primitive); the gRPC/ADS transport (still protos-blocked under ADR-0014 + H2-trailers-blocked); the Network-filters opener (low leverage).

- **Key scoping facts (SPEC §0):** the watcher is the FIFTH periodic-background primitive (poll-based mtime, `CancellationToken` discipline, mirroring `envoy-health::Scheduler` + the 13.x/14.2 sweepers) - NO new filesystem-watch dep; the swappable route-table handle is dep-free (`RwLock<Arc<…>>`/`tokio::sync::watch`, NOT a new `arc-swap` dep); the reload reuses the phase-20 RDS load path verbatim; minimum-viable adds ZERO new config schema (Envoy file-xDS is always-watching) UNLESS §6.2 forces `watched_directory`; reload is **warm-reject** (last-good table kept on a bad reload - the one ADR-0049 startup-all-fatal carve-out); fixture `0034` is **Linux-CI-authoritative** (the reload trigger is unobservable on macOS Docker virtiofs - the phase-21 watching-class precedent); the existing fixture `0028-xds-file-based-rds` gains an IDLE watcher (the regression witness that the route-table-handle migration is behavior-preserving).
