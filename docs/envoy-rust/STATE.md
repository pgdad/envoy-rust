# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `27` - next xDS / next ROADMAP-family pick (identity + scope **TBD at the state-1 brainstorm**), lifecycle **state-1-next** (no phase artifacts yet; the state-1 brainstorm picks + scopes the phase, fires next-available **ADR-0067**, and authors its `SPEC.md` skeleton + a new phase dir `docs/envoy-rust/phases/27-<slug>/`). Phase `26` is now **CLOSED** (state-6-complete — `26-xds-rds-hot-reload` `done`). ROADMAP rows `00`-`25.2` + parent `25` + row `26` are ALL `done`; the sequential ROADMAP rows END at `26`, so the next phase's identity is chosen from the `ROADMAP.md` family-heading candidate lists at the brainstorm (the **xDS / dynamic-config family** is the natural continuation — this phase built the watcher + atomic-swap primitive that CDS/LDS/EDS hot-reload now layer onto; other live candidates: the LB family opener, the gRPC/ADS transport [still ADR-0014/H2-trailers-blocked], network filters).
**slug:** _(assigned at the phase-27 state-1 brainstorm)_
**directory:** _(created at the phase-27 state-1 brainstorm - `docs/envoy-rust/phases/27-<slug>/`)_; no phase artifacts yet. The closed phase `docs/envoy-rust/phases/26-xds-rds-hot-reload/` carries all 4 artifacts (`SPEC.md` + `PLAN.md` + `PROGRESS.md` + `REVIEW.md`, `REVIEW.md` APPROVED). Reuse sources for the next xDS hot-reload step: the phase-26 `RdsWatcher` + swappable-handle primitive (`crates/envoy-http1/src/rds_watcher.rs`), the phase-20 RDS load path, and the `envoy-health::Scheduler` periodic-task primitive (`crates/envoy-health/src/scheduler.rs:40`).

**status:** **PHASE 26 (`26-xds-rds-hot-reload`) CLOSED - PHASE 27 OPEN at state-1-next (identity/scope TBD at the brainstorm).** This commit is the phase-26 **state-6 deterministic close-out** (BOOTSTRAP §6.1 step 4 - no skill, docs-only). It flipped ROADMAP row `26` `in-progress` -> `done` (done-summary cites: all 34 Docker-gated fixtures 0001–0034 green simultaneously [fixture 0034 Linux-CI-authoritative + fixture 0028 idle-watcher regression witness]; state-4 §7.5 gate GREEN at AUTHORITATIVE Linux CI `27708943522`; state-5 `REVIEW.md` APPROVED 0C/0I/8-Minor), advanced `STATE.md` to a phase-`27` placeholder at lifecycle state-1-next, relocated the superseded phase-26 state-5/state-6 top-section blocks + the `### Phase-26 carry-forwards` + `### Phase-26 state-1 brainstorm` Notes subsections verbatim to `STATE_HISTORY.md` (ADR-0035 / §4.1 inv. 9), and recorded the 8 `REVIEW.md` Minor carry-forwards in `## Notes` for the phase-27 brainstorm. NO production/test change (docs-only - the state-4 §7.5 gate already proved green at CI `27708943522`; state-5 `REVIEW.md` APPROVED 0C/0I/8-Minor). NO new ADR - the close surfaced no decision (**ADR-0067 stays UNFIRED**; **DECISIONS.md ledger head remains ADR-0066**, count 67). ADR-0014 in force; ADR-0028 open. No `unsafe`. Per §5.1 (one state per session) this session EXITS after the close-out; the NEXT session runs phase 27's state-1 `superpowers:brainstorming` to pick + scope the next phase.

> Historical `## Active phase` status narratives — every superseded `**status:**` paragraph (all closed phases + the active phase's prior sub-state pointers, incl. the phase-25 state-1 brainstorm pointer) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Next expected skill

Per `BOOTSTRAP_PROMPT.md` §5 state 1 + `SKILL_ROUTING.md`: phase `27` has NO artifacts yet (`SPEC.md` absent) -> the next session runs **`superpowers:brainstorming`** (state 1). The brainstorm: (1) picks the next phase's identity from the `ROADMAP.md` family-heading candidate lists (the sequential rows END at `26`, so there is NO pre-numbered row `27`; the **xDS / dynamic-config family** is the natural continuation now that phase 26 built the watcher + atomic-swap primitive — CDS/LDS/EDS hot-reload layer onto it; the brainstorm may also surface whether the LB-family opener or another family should lead); (2) scopes it (minimum-viable cut + explicit non-goals) and fires its scoping ADR (next available **ADR-0067**); (3) authors the `SPEC.md` skeleton + a new phase dir `docs/envoy-rust/phases/27-<slug>/`. The 8 carry-forwards from the phase-26 `REVIEW.md` (see `## Notes`) should be weighed during scoping — notably M26-1 (the H1 read-once double-snapshot) and M26-2 (the discriminator guard). Per §5.1 the state-1 brainstorm is the NEXT session's single state.

> Historical `## Next expected skill` narratives — every superseded next-skill pointer (all closed phases + the active phase's prior sub-state pointers) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.

## Last commit

**Phase-26 state-6 deterministic close-out - phase 26 CLOSED (THIS commit):** the docs-only bookkeeping commit (BOOTSTRAP §6.1 step 4 - no skill) closing phase `26-xds-rds-hot-reload`. It flips ROADMAP row `26` `in-progress` -> `done` (done-summary citing all 34 Docker-gated fixtures `0001`-`0034` green simultaneously; state-4 §7.5 gate GREEN [AUTHORITATIVE Linux CI `27708943522`]; state-5 `REVIEW.md` APPROVED 0C/0I/8-Minor), advances `STATE.md` to a phase-`27` placeholder at state-1-next (next: `superpowers:brainstorming`), relocates the superseded phase-26 state-5/state-6 top-section blocks + the `### Phase-26 carry-forwards` + `### Phase-26 state-1 brainstorm` Notes subsections verbatim to `STATE_HISTORY.md` (ADR-0035), and records the 8 `REVIEW.md` Minor carry-forwards in `## Notes`. NO production/test change (docs-only); NO new ADR (**ADR-0067 UNFIRED**; ledger head **ADR-0066**, count 67). ADR-0014 in force; ADR-0028 open. No `unsafe`. Per §5.1 the NEXT session runs phase 27's state-1 brainstorm.

> Historical `## Last commit` narratives — every superseded last-commit block (all closed phases + the active phase's prior sub-state commits) — are preserved verbatim in [STATE_HISTORY.md](STATE_HISTORY.md) per ADR-0035.


## Last updated

2026-06-18 (phase-26 **state-6 deterministic close-out - phase 26 CLOSED** - docs-only bookkeeping per BOOTSTRAP §6.1 step 4, no skill. Flipped ROADMAP row `26` `in-progress` -> `done` [done-summary cites all 34 Docker-gated fixtures `0001`-`0034` green simultaneously [fixture 0034 Linux-CI-authoritative + fixture 0028 idle-watcher regression witness]; state-4 §7.5 gate GREEN at AUTHORITATIVE Linux CI `27708943522`; state-5 `REVIEW.md` APPROVED 0C/0I/8-Minor]. Advanced `STATE.md` to a phase-`27` placeholder at state-1-next [next: `superpowers:brainstorming` - pick + scope the next phase from the ROADMAP family candidate lists [the xDS family's CDS/LDS/EDS hot-reload now layer onto this phase's watcher+swap primitive], fire next-available ADR-0067, author `SPEC.md` skeleton + a `docs/envoy-rust/phases/27-<slug>/` dir]. Relocated the superseded phase-26 state-5/state-6 top-section blocks + the `### Phase-26 carry-forwards` + `### Phase-26 state-1 brainstorm` Notes subsections verbatim to `STATE_HISTORY.md` [ADR-0035 / §4.1 inv. 9]. Recorded the 8 `REVIEW.md` Minor carry-forwards in `## Notes`. NO production/test change; NO new ADR [no decision surfaced -> ADR-0067 UNFIRED]; ledger head **ADR-0066** [count 67]. ADR-0014 in force; ADR-0028 open. No `unsafe`. Per §5.1 the phase-27 state-1 brainstorm is the NEXT session.)

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

### HTTP-filters-family carry-forwards (from the `25.2` REVIEW.md - NOT yet consumed; weigh whenever the HTTP-filters family is re-entered)

> These were never obligations on the xDS phase 26; they remain live for whenever an HTTP-filters-family phase resumes.

- **(1) [non-goal - architectural]** Over-limit request bodies are FULLY buffered before the 413 rejection (no streaming watermark). Documented deferred non-goal; differentially byte-identical to Envoy for the bounded fixture sizes. Revisit only if a streaming `decode_data` watermark path is ever planned.
- **(2) [doc precision]** The BEHAVIOR_CONTRACT 413-row "verified byte-exact against v1.33.0" phrasing - fixture `0033` is H1-only; the H2 over-limit path is covered by the in-process synth-decorator backstop, NOT differentially. Consider narrowing the phrasing if an H2 over-limit fixture is ever added.
- **(3) [coverage]** No standalone `== effective route limit` unit assertion (the boundary is exercised only via the over/under probes).
- **(4) [coverage]** No differential at-limit (`==`) probe in `0033` (within-limit `<` and over-limit `>` are both covered; the exact boundary is not differentially probed).
- _(2)-(4) are cheap polish, (1) is architectural and only relevant to a future streaming phase._

### Phase-27 carry-forwards (from the phase-26 `REVIEW.md` M26-1..M26-8 - weigh at the phase-27 state-1 brainstorm / planning)

> All 8 are NON-BLOCKING Minors (REVIEW.md APPROVED 0C/0I); M26-1 + M26-2 are the only substantive items and neither is an Envoy-equivalence divergence (the differential gate is green). Several attach to the xDS hot-reload watcher+swap primitive, so they are naturally weighed when the next xDS hot-reload phase re-enters that code.

- **M26-1 [substantive]** The H1 request path snapshots the route table TWICE (`resolve_route` at `crates/envoy-http1/src/hcm.rs:691` for per-route filter-config threading; `build_response` at `:766`→`:1346` for the routing decision). A reload landing between the two reads applies OLD-table per-route config with NEW-table routing — a narrow ~1s-poll-window gap vs the §5.4 "read the route-table Arc ONCE at request entry" wording. Bounded + benign (no panic; routing self-consistent) and NOT an Envoy-equivalence divergence. Resolve when next touched by EITHER threading one snapshot from `serve_connection` through `build_response`, OR narrowing the §5.4 wording (BEHAVIOR_CONTRACT §2.2) to "the routing decision is read-once" (D-3.3). The H2 path already resolves once.
- **M26-2 [substantive]** `wait_for_reload_convergence` (`tests/differential/src/lib.rs:1257-1268`) can spuriously "converge" if the discriminator has neither `expected_status` nor `expected_body` (both `Option`; both-`None` → `status_ok && body_ok` true on the first poll). Fixture 0034 sets both (safe today). Suggested: `bail!` in the `Http1RdsReload` arm if the discriminator has neither field, or document the invariant. CI-path robustness hardening.
- **M26-3** mtime-only change detection (`crates/envoy-http1/src/rds_watcher.rs:177-182`): two reloads within one (≥1s) mtime tick would miss the second. Never bites the single-reload contract; worth a one-line caveat on `read_mtime` (or a secondary file-length compare) since this is the project's first live-mutation primitive.
- **M26-4** fixture 0034 uses `direct_response` bodies (`rds-v1`→`rds-v2`) rather than the SPEC §1 two-cluster routing-flip (one-backend harness can't distinguish two clusters in a differential response; the cluster/counter/config_dump proofs live in the backstop). A deliberate, documented deviation — not a defect.
- **M26-5** stale Task-3-era comments on `RdsWatcher::spawn` (`rds_watcher.rs:114-120`) + the `main.rs` spawn site still say "skeleton / no-op stub" after Tasks 4/5 landed the real pipeline + counters. Cosmetic; refresh.
- **M26-6** duplicated atomic-rename helper across test crates (`atomic_rename_over` in `tests/differential/src/lib.rs:1127` vs `atomic_rename_rds` in `crates/envoy-bin/tests/xds_rds_hot_reload.rs:399`) with a subtle `.yaml`-preservation/cleanup divergence. Consistent with the already-tracked deferred shared-test-support-crate item; noted so it is intentional, not silent drift.
- **M26-7** redundant accessor call in the config_dump miss path (`crates/envoy-admin/src/endpoint.rs` calls `handler.live_route_configs()` twice). Cosmetic (bind once if touched).
- **M26-8** `RouteSnapshot::as_ref` is an inherent method shadowing the conventional `AsRef::as_ref` (`crates/envoy-admin/src/endpoint.rs`); a name like `route_config()` would be marginally clearer. Trivial.
