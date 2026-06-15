# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `26` - `26-xds-rds-hot-reload` (xDS / dynamic-config family: **file-based RDS hot reload** - a running HCM whose route table is RDS-supplied re-reads its edited RDS file WITHOUT a restart, atomically swapping the live route table; the family's repeatedly-deferred prime follow-up, and the FIRST phase that mutates a running manager's state post-construction), lifecycle **state-1-complete / state-2-next** (`SPEC.md` present [authored at THIS state-1 brainstorm commit]; `PLAN.md` + `PROGRESS.md` + `REVIEW.md` ABSENT -> the next skill is the state-2 PLAN-write). The fifth concrete xDS-family phase (after 18 CDS / 19 LDS / 20 RDS / 21 EDS). ROADMAP rows `00`-`25.2` + parent `25` are all `done`; the new row `26` is `planned` (flips `in-progress` when the state-2 PLAN-write points STATE at it).
**slug:** `26-xds-rds-hot-reload`
**directory:** `docs/envoy-rust/phases/26-xds-rds-hot-reload/` - carries **`SPEC.md`** (authored at this brainstorm; `PLAN.md` + `PROGRESS.md` + `REVIEW.md` land at state-2 onward). Reuse sources: the phase-20 RDS load path (`docs/envoy-rust/phases/20-xds-file-based-rds/`), the `envoy-health::Scheduler` periodic-task primitive, and fixture `0028-xds-file-based-rds` (the idle-watcher regression witness).

**status:** **PHASE 26 (`26-xds-rds-hot-reload`) STATE-1 BRAINSTORM COMPLETE - `SPEC.md` + `ADR-0065` authored at THIS commit** (lifecycle state-1-complete / state-2-next). This session ran `superpowers:brainstorming`: it PIVOTED from the (exhausted) HTTP-filters family to the xDS / dynamic-config family's prime follow-up - **hot reload** - scoped minimum-viable to **file-based RDS hot-reload** (poll-based file watch + atomic route-table swap + per-reload `rds.*` counter advancement + `RoutesConfigDump` version update + fixture `0034`). RDS is the FIRST hot-reload target because its live mutation is the least invasive (the route table is already `route_config: Arc<RouteConfiguration>` and route matching is per-request stateless -> an atomic pointer swap; no drain / socket churn / pool-health-outlier lifecycle). The pick + the five §0 findings + the minimum-viable scope boundary are locked by **ADR-0065** (the scoping ADR, fired at this commit). NO production/test change at state-1 (the SPEC + ADR + ROADMAP row + STATE advance are the deliverables). **DECISIONS.md ledger head: ADR-0065** (count 66; next available **ADR-0066**, reserved for the §6.2 reconciliation; **ADR-0067** reserved for the §6.1 split [projected NOT to fire]). ADR-0014 in force; ADR-0028 open. No `unsafe`. Per §5.1 (one state per session) this session EXITS after the brainstorm commit; the NEXT session runs phase 26's state-2 PLAN-write via `superpowers:writing-plans` (authoring `PLAN.md` + `PROGRESS.md` skeleton + Task-1 preamble, running the §6.2 empirical verification [which MUST run on Linux per ADR-0049 - the reload trigger is macOS-unobservable], flipping ROADMAP row `26` to `in-progress`, and firing ADR-0066 iff §6.2 diverges).

> Historical `## Active phase` status narratives — every superseded `**status:**` paragraph (all closed phases + the active phase's prior sub-state pointers, incl. the phase-25 state-1 brainstorm pointer) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Next expected skill

Per `BOOTSTRAP_PROMPT.md` §5 state 2 + `SKILL_ROUTING.md`: phase `26` has `SPEC.md` but no `PLAN.md` -> the next session runs **`superpowers:writing-plans`** (state 2). The PLAN-write: (1) runs the §6.2 empirical verification against `envoyproxy/envoy:v1.33.0` - **on Linux** (the reload trigger is unobservable on macOS Docker virtiofs per ADR-0049 / SPEC §0 finding 4); (2) organizes deliverables D1-D8 (SPEC §3) into tasks + evaluates the §6.1 split gate (projected single-phase, ~1200-1600 LoC / ~10-13 tasks); (3) authors `PLAN.md` + `PROGRESS.md` skeleton + the Task-1 preamble; (4) flips ROADMAP row `26` `planned -> in-progress` + advances STATE to state-2-complete / state-3-next; (5) fires **ADR-0066** iff §6.2 forces the `watched_directory` schema field or the reload-counter / bad-reload / config_dump-version shapes diverge materially. Per §5.1 the state-2 PLAN-write is the NEXT session's single state.

> Historical `## Next expected skill` narratives — every superseded next-skill pointer (all closed phases + the active phase's prior sub-state pointers) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Last commit

**Phase-26 state-1 brainstorm - SPEC + ADR-0065 (THIS commit):** the brainstorm commit authoring phase 26 (`26-xds-rds-hot-reload`). This session ran `superpowers:brainstorming`, pivoting from the exhausted HTTP-filters family to xDS hot reload, scoped minimum-viable to file-based RDS hot-reload. THIS commit lands: `docs/envoy-rust/phases/26-xds-rds-hot-reload/SPEC.md` (the §0 findings + D1-D8 deliverables + §4 deferrals + §5 invariants + §6.2 PLAN-write checklist); **ADR-0065** (the scoping ADR - the family pivot + findings + scope boundary; DECISIONS.md ledger head -> ADR-0065, count 66); a new ROADMAP row `26` (`status: planned`, no existing row flips); the STATE advance to `26` state-1-complete / state-2-next (next: `superpowers:writing-plans`); the relocation of the superseded phase-26-placeholder top-section blocks verbatim to `STATE_HISTORY.md` (ADR-0035); and a `### Phase-26 state-1 brainstorm` Notes subsection. NO production/test change. ADR-0014 in force; ADR-0028 open. No `unsafe`. The commit title carries `[ADR-0065]`. Per §5.1 the NEXT session runs the state-2 PLAN-write.

> Historical `## Last commit` narratives — every superseded last-commit block (all closed phases + the active phase's prior sub-state commits) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.


## Last updated

2026-06-15 (phase-26 **state-1 brainstorm COMPLETE** - ran `superpowers:brainstorming`. PIVOTED from the exhausted HTTP-filters family [every remaining member external-service / engine / timing / byte-fragile after buffer] to the xDS family's prime follow-up - **hot reload** - scoped minimum-viable to **file-based RDS hot-reload**: a poll-based file watcher [the 5th periodic-background primitive] + an atomic route-table `Arc` swap + per-reload `rds.*` counter advancement + `RoutesConfigDump` version update + fixture `0034` [Linux-CI-authoritative per ADR-0049 - the reload trigger is macOS-unobservable]. RDS first = the least-invasive live mutation [route table already a swappable `Arc`, matching per-request stateless]. Authored `SPEC.md` + **ADR-0065** [scoping ADR; DECISIONS.md ledger head -> ADR-0065, count 66; ADR-0066 reserved for §6.2 reconciliation, ADR-0067 for the §6.1 split]. Added ROADMAP row `26` `planned`. Advanced `STATE.md` to `26` state-1-complete / state-2-next [next: `superpowers:writing-plans`] and relocated the superseded phase-26-placeholder top-section blocks verbatim to `STATE_HISTORY.md` [ADR-0035 / §4.1 inv. 9]. NO production/test change; the 4 `25.2` REVIEW.md carry-forwards [§ Notes] roll into the PLAN-write. ADR-0014 in force; ADR-0028 open. No `unsafe`. Per §5.1 the phase-26 state-2 PLAN-write is the NEXT session.)

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
