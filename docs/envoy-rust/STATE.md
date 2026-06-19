# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `28` - next xDS / next ROADMAP-family pick (identity + scope **TBD at the state-1 brainstorm**), lifecycle **state-1-next** (no phase artifacts yet; the state-1 brainstorm picks + scopes the phase, fires next-available **ADR-0069**, and authors its `SPEC.md` skeleton + a new phase dir `docs/envoy-rust/phases/28-<slug>/`). Phase `27` is now **CLOSED** (state-6-complete — `27-xds-eds-hot-reload` `done`). ROADMAP rows `00`-`26` + row `27` are ALL `done`; the sequential ROADMAP rows END at `27`, so the next phase's identity is chosen from the `ROADMAP.md` family-heading candidate lists at the brainstorm (the **xDS / dynamic-config family** remains a natural continuation — phases 26+27 built the watcher + atomic-swap primitive that CDS/LDS hot-reload now layer onto; other live candidates: the LB family opener, the gRPC/ADS transport [still ADR-0014/H2-trailers-blocked], network filters).
**slug:** _(assigned at the phase-28 state-1 brainstorm)_
**directory:** _(created at the phase-28 state-1 brainstorm - `docs/envoy-rust/phases/28-<slug>/`)_; no phase artifacts yet. The closed phase `docs/envoy-rust/phases/27-xds-eds-hot-reload/` carries all 4 artifacts (`SPEC.md` + `PLAN.md` + `PROGRESS.md` + `REVIEW.md`, `REVIEW.md` APPROVED). Reuse sources for the next xDS hot-reload step: the phase-26/27 `XdsFileWatcher` + swappable-handle + warm-reject reload primitives (`crates/envoy-cluster/src/xds_watch.rs`, `crates/envoy-cluster/src/eds_reload.rs`, `crates/envoy-http1/src/rds_watcher.rs`), the phase-21 EDS / phase-20 RDS load paths, the `envoy-health::Scheduler` periodic-task primitive (`crates/envoy-health/src/scheduler.rs:40`), and the mid-test-atomic-rename + second-distinguishable-backend harness capability (`tests/differential/src/lib.rs`, `tests/helpers/http1-echo-server/`).

**status:** **PHASE 27 (`27-xds-eds-hot-reload`) CLOSED at the state-6 deterministic close-out — file-based EDS endpoint hot reload SHIPPED.** ROADMAP row `27` flipped `in-progress` → `done`; STATE now points at a **phase-28 placeholder at lifecycle state-1-next**. The phase delivered the FIRST of the three ADR-0065-deferred CDS/LDS/EDS hot-reload layers: a running PLAIN cluster whose endpoint set is EDS-supplied is re-read on atomic-rename and atomically hot-swapped onto live traffic WITHOUT a restart — D1 a swappable `RwLock<Arc<Vec<SocketAddr>>>` endpoint handle read-once-per-LB-selection; D2 the phase-26 `RdsWatcher` generalized into a domain-free `XdsFileWatcher` (`crates/envoy-cluster/src/xds_watch.rs`); D3+D4 the warm-reject reload pipeline (`eds_reload.rs`) + the V4 bad-reload taxonomy [apply-empty→`update_success`+503 MIRROR; IO/parse→`update_failure`; wrong-name/bad-IP→`update_rejected`; empty-envelope→`update_empty`] + envoy-bin wiring; D5 `EndpointsConfigDump` read-through-handle; D6 a second distinguishable backend; D7 fixture 0035 + D8 the in-process backstop + the BEHAVIOR_CONTRACT §2.1/§2.2 extension. **Close-out evidence:** all 35 Docker-gated fixtures `0001`–`0035` green simultaneously (fixture 0035 native-Linux-CI-authoritative per ADR-0049/0066; fixture 0029 idle-watcher regression witness); state-4 §7.5 gate GREEN at AUTHORITATIVE Linux CI `27818702552` @ `acac6d4`; state-5 `REVIEW.md` APPROVED 0 Critical / 0 Important / 3 non-gating Minor (M27-1..M27-3, the **twelfth consecutive clean state-5**, after 17–26). The state-6 commit is docs-only (ROADMAP.md + STATE.md + STATE_HISTORY.md; NO code) per §5.3 / BOOTSTRAP §6.1. The 3 `REVIEW.md` Minors carry to phase-28 planning (the `### Phase-28 carry-forwards` Notes subsection). **DECISIONS.md ledger head: ADR-0068** (count 69; **ADR-0069 reserved + UNFIRED** — the next phase's state-1 brainstorm fires it). ADR-0014 in force; ADR-0028 open. No `unsafe`. Per §5.1 the NEXT session runs the phase-28 state-1 brainstorm (`superpowers:brainstorming`).

> Historical `## Active phase` status narratives — every superseded `**status:**` paragraph (all closed phases + the active phase's prior sub-state pointers, incl. the phase-25 state-1 brainstorm pointer) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Next expected skill

Per `BOOTSTRAP_PROMPT.md` §5 state 0/1 + `SKILL_ROUTING.md`: phase `27` is CLOSED (`done`) and the sequential ROADMAP rows END at `27`, so STATE points at a phase-28 placeholder with NO artifacts yet -> the next session runs **`superpowers:brainstorming`** (state 1) to pick + scope phase `28` from the `ROADMAP.md` family-heading candidate lists (§9), fire next-available **ADR-0069**, create `docs/envoy-rust/phases/28-<slug>/`, and author its `SPEC.md`. The xDS / dynamic-config family is the natural continuation (phases 26+27 built the watcher + atomic-swap primitive that CDS/LDS hot-reload now layer onto), but the pick is open at the brainstorm. **Weigh the 3 phase-27 `REVIEW.md` Minors (M27-1..M27-3) at the phase-28 brainstorm/planning** (the `### Phase-28 carry-forwards` Notes subsection) — all non-blocking hardening nits. Per §5.1 the brainstorm is the NEXT session's single state.

> Historical `## Next expected skill` narratives — every superseded next-skill pointer (all closed phases + the active phase's prior sub-state pointers) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Last commit

**Phase-27 state-6 deterministic close-out (THIS commit):** the BOOTSTRAP §6.1 / §5 state-6 close-out — no skill, docs-only. Flips ROADMAP row `27` `in-progress` → `done` (done-summary cites: all 35 Docker-gated fixtures `0001`–`0035` green simultaneously [fixture 0035 native-Linux-CI-authoritative; fixture 0029 idle-watcher witness]; state-4 §7.5 gate GREEN [AUTHORITATIVE Linux CI `27818702552` @ `acac6d4`]; state-5 `REVIEW.md` APPROVED 0C/0I/3-Minor), and advances STATE to a phase-28 placeholder at state-1-next (next skill `superpowers:brainstorming`). Relocates the superseded phase-27 state-5 top-section blocks + the now-closed `### Phase-27 state-1 brainstorm` / `### Phase-27 state-2 PLAN-write` / `### Phase-27 state-4 verification` Notes subsections + the now-consumed `### Phase-27 carry-forwards` (M26-1..M26-8) block verbatim to `STATE_HISTORY.md` per ADR-0035 / §4.1 inv. 9; adds the `### Phase-28 carry-forwards` (M27-1..M27-3) Notes subsection. Docs-only (ROADMAP.md + STATE.md + STATE_HISTORY.md; NO code/test/DECISIONS change). Sits on the state-5 review commit `cf61b99`. NO new ADR (ledger head **ADR-0068**, count 69; **ADR-0069 reserved + UNFIRED**). ADR-0014 in force; ADR-0028 open. No `unsafe`. **NEXT = phase-28 state-1 brainstorm (`superpowers:brainstorming`).**

> Historical `## Last commit` narratives — every superseded last-commit block (all closed phases + the active phase's prior sub-state commits) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.


## Last updated

2026-06-19 (phase-27 **state-6 deterministic close-out — `27-xds-eds-hot-reload` CLOSED (`done`); file-based EDS endpoint hot reload SHIPPED** - the BOOTSTRAP §6.1 / §5 state-6 close-out [no skill, docs-only]. Flipped ROADMAP row `27` `in-progress` → `done` [done-summary: all 35 Docker-gated fixtures 0001–0035 green simultaneously; state-4 §7.5 gate GREEN at Linux CI `27818702552` @ `acac6d4`; state-5 REVIEW.md APPROVED 0C/0I/3-Minor], advanced STATE to a phase-28 placeholder at state-1-next [next skill `superpowers:brainstorming` — pick + scope phase 28 from the ROADMAP family-heading candidates, fire next-available ADR-0069]. Relocated the superseded state-5 top-section blocks + the closed-phase-27 Notes subsections [state-1 brainstorm / state-2 PLAN-write / state-4 verification] + the now-consumed Phase-27 (M26) carry-forwards block verbatim to `STATE_HISTORY.md` [ADR-0035 / §4.1 inv. 9]; added the `### Phase-28 carry-forwards` [M27-1..M27-3] subsection. Docs-only commit [ROADMAP.md + STATE.md + STATE_HISTORY.md; NO code]. Ledger head **ADR-0068** [count 69; **ADR-0069 reserved + UNFIRED**]. ADR-0014 in force; ADR-0028 open. No `unsafe`. Per §5.1 the NEXT session runs the phase-28 state-1 brainstorm.)

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

### HTTP-filters-family carry-forwards (from the `25.2` REVIEW.md - NOT yet consumed; weigh whenever the HTTP-filters family is re-entered)

> These were never obligations on the xDS phase 26; they remain live for whenever an HTTP-filters-family phase resumes.

- **(1) [non-goal - architectural]** Over-limit request bodies are FULLY buffered before the 413 rejection (no streaming watermark). Documented deferred non-goal; differentially byte-identical to Envoy for the bounded fixture sizes. Revisit only if a streaming `decode_data` watermark path is ever planned.
- **(2) [doc precision]** The BEHAVIOR_CONTRACT 413-row "verified byte-exact against v1.33.0" phrasing - fixture `0033` is H1-only; the H2 over-limit path is covered by the in-process synth-decorator backstop, NOT differentially. Consider narrowing the phrasing if an H2 over-limit fixture is ever added.
- **(3) [coverage]** No standalone `== effective route limit` unit assertion (the boundary is exercised only via the over/under probes).
- **(4) [coverage]** No differential at-limit (`==`) probe in `0033` (within-limit `<` and over-limit `>` are both covered; the exact boundary is not differentially probed).
- _(2)-(4) are cheap polish, (1) is architectural and only relevant to a future streaming phase._

### Phase-28 carry-forwards (from the phase-27 `REVIEW.md` M27-1..M27-3 - weigh at the phase-28 state-1 brainstorm / planning)

> All 3 are NON-BLOCKING Minors (REVIEW.md APPROVED 0C/0I); pure hardening nits, none an Envoy-equivalence divergence (the differential gate is green). The first two attach to the cluster endpoint-handle / LB selection code, so they are naturally weighed when the next cluster/LB or xDS hot-reload phase re-enters that code; the third is the long-deferred shared-test-support-crate extraction.

- **M27-1** Tighten `Cluster::store_endpoints` (`crates/envoy-cluster/src/cluster.rs`) from `pub` to `pub(crate)` to match the PLAN-of-record. It is effectively unreachable cross-crate today (no public API hands out an `&Cluster`/`Arc<Cluster>`; `into_inner` is `pub(crate)`), so this is NOT an actual write-surface leak — pure hardening. Resolve when next touched.
- **M27-2** Add a `pick()` slow-path `debug_assert_eq!(eps.len(), health.len())` (when `Some`) length-coupling (`crates/envoy-cluster/src/cluster.rs:344-355`). The slow path indexes `health[i]`/`ejection[i]` for `i in 0..eps.len()`, safe today by construction (plain clusters have `endpoint_health`/`outlier_detection` = `None`); the assert would turn a future regression (wiring a watcher onto an HC/OD cluster) into a loud test failure rather than a production index-panic. Defense-in-depth.
- **M27-3** The in-flight-isolation backstop uses a 400ms wall-clock sleep (`crates/envoy-bin/tests/xds_eds_hot_reload.rs:761`) — well-cushioned by a 2s slow-backend delay (very low flake risk) but the sole non-bounded timing assumption in the backstop. Note alongside the deferred **shared-test-support-crate extraction** (the M18-9/M26-6 item, now also the 3rd+ atomic-rename/`reserve_port` user — the reviewer reproduced a port-TOCTOU flake once under full parallel load): a `SO_REUSEADDR`-or-retry port helper + a bounded-wait replacement would remove both. The trigger is the THIRD file-reload driver (CDS/LDS/SDS hot-reload), which also triggers factoring the RDS/EDS reload-dispatch skeleton (the M26-track ~70-line duplication).
