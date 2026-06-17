# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `26` - `26-xds-rds-hot-reload` (xDS / dynamic-config family: **file-based RDS hot reload** - a running HCM whose RDS-supplied route table is re-read on file change and atomically swapped onto live traffic WITHOUT a restart; the family's prime follow-up + the FIRST post-construction live mutation in the project), lifecycle **state-4 verification COMPLETE / state-5-next** (`SPEC.md` + `PLAN.md` + `PROGRESS.md` present with ALL 10 PLAN tasks landed [Task 9 N/A] incl. the `## Task 10` state-4 §7.5-gate evidence; `REVIEW.md` ABSENT -> the next skill is `superpowers:requesting-code-review`). The fifth concrete xDS-family phase (after 18 CDS / 19 LDS / 20 RDS / 21 EDS). ROADMAP rows `00`-`25.2` + parent `25` are `done`; row `26` is `in-progress`.
**slug:** `26-xds-rds-hot-reload`
**directory:** `docs/envoy-rust/phases/26-xds-rds-hot-reload/` - carries **`SPEC.md` + `PLAN.md` + `PROGRESS.md`** (the `PLAN.md` §6.2 lock-ins are final per Task 1; `REVIEW.md` lands at state-5). Reuse sources: the phase-20 RDS load path (`docs/envoy-rust/phases/20-xds-file-based-rds/`), the `envoy-health::Scheduler` periodic-task primitive (`crates/envoy-health/src/scheduler.rs:40`), and fixture `0028-xds-file-based-rds` (the §5.2 idle-watcher regression witness).

**status:** **PHASE 26 (`26-xds-rds-hot-reload`) STATE-4 VERIFICATION COMPLETE - the §7.5 phase-done gate is GREEN, incl. the AUTHORITATIVE native-Linux CI differential (all 34 fixtures incl. `0034` RDS hot-reload); next is state-5 `superpowers:requesting-code-review` -> `REVIEW.md`** (lifecycle state-4-complete / state-5-next). Ran `superpowers:verification-before-completion` for Task 10. **THE LOAD-BEARING TASK-10 FINDING:** CI had been RED at the `fmt` step for the ENTIRE phase (Tasks 1/4/8 committed fmt-dirty code; the per-task discipline ran clippy but deferred `fmt --check` to this state-4 gate), so the CI `build + test + lint` job's clippy/build/**test(differential incl. 0034)**/deny steps NEVER RAN — the authoritative differential anchor had never executed. Task 10 ran `cargo fmt --all` (6 files, cosmetic line-wrapping, rustfmt 1.95.0; commit `e052fc6`), unblocking CI; then two further never-locally-exercisable fixture/harness bugs surfaced on CI + were root-caused via `superpowers:systematic-debugging` and fixed (fixture 0034's unsubstituted `{{ADMIN_PORT}}` → the non-admin-scraping admin convention, commit `9e1216e`; the testcontainers `exec` exit-code-`None` race → `CmdWaitFor::exit_code(0)`, commit `ad40b29`). **§7.5 gate GREEN — authoritative anchor = native-Linux CI run `27708943522`** (at code-HEAD `a1a306b`): fmt+clippy+build+test(34 differential fixtures + h2spec)+deny all ✓, the separate fuzz job (`parse_bootstrap`+`jwt_parse`) ✓, and **`test xds_rds_hot_reload_fixture ... ok`** (the 0034 reload bilaterally proven on a real bind-mount-inotify runner). Local gates (fmt / 4 standalone builds / workspace build / clippy / `cargo test` minus the Docker crates / `cargo deny check`) also green; fuzz N/A (no new fuzzer this phase). **DECISIONS.md ledger head: ADR-0066** (count 67; ADR-0067 reserved + UNFIRED). ADR-0014 in force; ADR-0028 open. No `unsafe`. Per §5.1 the NEXT session runs phase-26 state-5 (`superpowers:requesting-code-review`).

> Historical `## Active phase` status narratives — every superseded `**status:**` paragraph (all closed phases + the active phase's prior sub-state pointers, incl. the phase-25 state-1 brainstorm pointer) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Next expected skill

Per `BOOTSTRAP_PROMPT.md` §5 state 5 + `SKILL_ROUTING.md`: phase `26` has `SPEC.md` + `PLAN.md` + `PROGRESS.md` (ALL 10 PLAN tasks landed/N-A, incl. the Task-10 state-4 §7.5 gate which is GREEN) but no `REVIEW.md` -> the next session runs **`superpowers:requesting-code-review`** (state 5), producing `REVIEW.md`. If `REVIEW.md` surfaces issues -> back to state 3 (NOT 4) per §5.2; if approved -> state-6 close-out (commit `phase 26: <title> [ADR-0065, ADR-0066]`, ROADMAP row `26` -> `done`, STATE -> next phase / awaiting-next-planning). The differential evidence for the review is the AUTHORITATIVE native-Linux CI anchor `27708943522` (all 34 fixtures green incl. the `0034` RDS hot-reload; the reload is native-Linux-CI-authoritative per §5.7 / ADR-0066 — Docker-Desktop virtiofs makes it locally unobservable). The review surface = the phase-26 production deltas (the `RwLock<Arc<RouteConfiguration>>` swappable handle + `ResolvedRoute`; `RdsWatcher` + the reload pipeline / warm-reject taxonomy incl. the recorded unknown-cluster divergence; `config_dump` read-through-handle; the `Http1RdsReload` harness driver + fixture 0034 + the in-process backstop) + the Task-4 carry-forward (ADR-0028 H1×H2 re-validation deferral).

> Historical `## Next expected skill` narratives — every superseded next-skill pointer (all closed phases + the active phase's prior sub-state pointers) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Last commit

**Phase-26 state-4 Task 10 - §7.5 phase-done verification gate (THIS session):** the fmt-fix code commit `e052fc6` (6 files, cosmetic - unblocks the CI `fmt` gate that had been red the whole phase) + two CI-surfaced fix commits (`9e1216e` fixture-0034 admin-port convention; `ad40b29` RDS reload-exec `CmdWaitFor::exit_code(0)` wait) + the interleaved PROGRESS commits + this PROGRESS-finalize / STATE-advance commit. **The §7.5 gate is GREEN on the AUTHORITATIVE native-Linux CI run `27708943522`** (at code-HEAD `a1a306b`): fmt+clippy+build+test(all 34 differential fixtures + h2spec)+deny ✓, the fuzz job ✓, and `test xds_rds_hot_reload_fixture ... ok`. Sits on the Task-8 commits (`7b2ba94` + `5c6ebc1` + `31f4e0f`). **NEXT = state-5 `superpowers:requesting-code-review` -> `REVIEW.md`.**

> Historical `## Last commit` narratives — every superseded last-commit block (all closed phases + the active phase's prior sub-state commits) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.


## Last updated

2026-06-17 (phase-26 **state-4 verification COMPLETE - the §7.5 phase-done gate is GREEN incl. the authoritative native-Linux CI differential** - ran `superpowers:verification-before-completion` for Task 10. Load-bearing finding: CI was red at `fmt` for the WHOLE phase, so the differential anchor never ran; Task 10's `cargo fmt --all` fix (`e052fc6`) unblocked it, and two never-locally-exercisable CI-only bugs were root-caused via `superpowers:systematic-debugging` + fixed (fixture-0034 `{{ADMIN_PORT}}` convention `9e1216e`; testcontainers exec exit-code race `ad40b29`). Gate GREEN at native-Linux CI run **`27708943522`** (code-HEAD `a1a306b`): fmt+clippy+build+test(34 fixtures + h2spec)+deny ✓, fuzz ✓, `xds_rds_hot_reload_fixture ... ok`. Local gates also green; fuzz N/A (no new fuzzer). **NEXT = state-5 `superpowers:requesting-code-review`** -> `REVIEW.md`. Ledger head **ADR-0066** (count 67; ADR-0067 reserved + UNFIRED). No `unsafe`. Per §5.1 the NEXT session runs phase-26 state-5.)

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
