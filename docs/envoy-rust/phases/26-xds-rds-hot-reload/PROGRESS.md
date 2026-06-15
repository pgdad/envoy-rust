# Phase 26 (`26-xds-rds-hot-reload`) — PROGRESS

> State-by-state + task-by-task progress log for phase 26 (file-based RDS hot-reload).
> The state-2 PLAN-write entry is below; each state-3 task appends its own entry
> (one code commit + one PROGRESS commit per task, per `feedback_execution_style`).
> **NOTE:** this phase's §6.2 empirical verification is DEFERRED to state-3 **Task 1**
> (Linux-only — the reload trigger is macOS-unobservable per ADR-0049 / SPEC §5.7);
> the cadence is shifted one state vs the normal verify-at-PLAN-write flow. See the
> PLAN STATUS banner.

---

## State-2 PLAN-write (this commit) — DRAFT, §6.2 DEFERRED to state-3 Task 1 (Linux)

- **Skill:** `superpowers:writing-plans`.
- **Mode:** **macOS-deferred** (a deliberate, user-approved departure). The §6.2 empirical verification against `envoyproxy/envoy:v1.33.0` MUST run on Linux (the RDS hot-reload trigger is unobservable on macOS Docker Desktop virtiofs — SPEC §0 finding 4 / §5.7 / ADR-0049 Provenance). This PLAN-write was on macOS, so §6.2 was NOT run; it becomes **state-3 Task 1** (run FIRST, on Linux).
- **Authored:** `PLAN.md` (header + goal + architecture + the STATUS-DEFERRED banner + the §6.2 projections [all `[§6.2-PENDING]`] + the macOS-verified SPEC-correction anchors C1–C6 + the §6.1 split-gate decision [single-phase projected] + the file structure + Tasks 1–10 + self-review) + this `PROGRESS.md` skeleton + the Task-1 preamble (below).
- **Code anchors locked NOW (macOS, read-only, HEAD `1785a0d42`):** C1 `HCMConfig.route_config: Arc<RouteConfiguration>` (`crates/envoy-http1/src/hcm.rs:122`, construct `:209`, read `:1226`/`:1253`); C2 the H2 read path (`crates/envoy-http2/src/hcm.rs:468`) + the type-alias re-export; C3 the `Scheduler::spawn` watcher template (`crates/envoy-health/src/scheduler.rs:40`); C4 the envoy-bin spawn site (`crates/envoy-bin/src/main.rs:180-194`, token `:91`). These are §6.2-independent → the foundation Tasks 2–3 can proceed before Task 1.
- **§6.2-dependent shapes NOT locked** (Task 1 supplies them): the file-change operation / `watched_directory` need (P2); the reload-counter values (P4); the bad-reload disposition (P5); the config_dump-version shape (P6).
- **ADR posture:** **ADR-0066 NOT fired** at this PLAN-write (cannot be evaluated without §6.2). It fires inline at **Task 1** iff §6.2 diverges materially. **DECISIONS.md ledger head stays ADR-0065** until Task 1. ADR-0067 (split) is a Task-1 decision. ADR-0014 in force; ADR-0028 open.
- **ROADMAP:** row `26` flips `planned → in-progress` at this commit (STATE now points at it). **STATE:** advances to `26` state-2-complete / state-3-next (next skill `superpowers:subagent-driven-development`), **with the §6.2-on-Linux blocking precondition recorded** (state-3 begins with Task 1 on Linux, or the §6.2-independent Tasks 2–3 on any platform; the §6.2-gated Tasks 4/6/7/8/9 are BLOCKED on Task 1). Superseded state-1/state-2 top-section blocks relocated to `STATE_HISTORY.md` (ADR-0035 / §4.1 inv. 9).
- **No production/test change at state-2** (docs-only PLAN-write commit).

### Task-1 preamble — the §6.2 PROTOCOL (run on LINUX; the deferred verify-at-PLAN-write)

Task 1 (PLAN §"Task 1") runs the SPEC §6.2 7-item checklist on a Linux host / Linux CI against `envoyproxy/envoy:v1.33.0` (digest `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`), the ADR-0063 sidecar methodology adapted for a reloadable RDS file:
1. Stand up Envoy with an `rds`-configured HCM (`route_config_name: local_route`, `path_config_source.path` → a mounted file) + a host backend + admin scrapes; confirm initial routing + initial `rds.*` values.
2. **Resolve the file-change operation (P2 — load-bearing):** does Envoy's default file-watch reload on in-place truncate-rewrite? on atomic-rename? is `watched_directory` required? → decides Task 9 (the `watched_directory` schema field) + whether ADR-0066 fires.
3. Resolve P1 (settle latency → Task 7 wait bound), P4 (reload counter values), P5 (bad-reload disposition: `update_failure` vs `update_rejected`, last-good kept), P6 (config_dump version/`last_updated` + route shape; 0026/0027 indices unaffected).
4. Resolve P3 (two distinguishable single-endpoint clusters via `upstream_rq_total`).
5. Lock the shapes: replace every `[§6.2-PENDING]` in PLAN.md + the §2.1/§2.2 BEHAVIOR_CONTRACT projections; fire **ADR-0066** iff P2(`watched_directory`)/P4/P5/P6 diverged (commit title `[ADR-0066]`), else record "no reconciliation"; re-evaluate the §6.1 split (ADR-0067).
6. Commit the lock-ins; preserve the probe transcript here per ADR-0049 Provenance.

---

## Task 1 — §6.2 empirical verification on Linux (fires ADR-0066 if divergent)

_(pending — state-3, Linux)_

## Task 2 — route-table-handle migration (`Arc<RouteConfiguration>` → swappable handle) [§6.2-INDEPENDENT]

_(pending)_

## Task 3 — `RdsWatcher` periodic primitive (skeleton) [§6.2-INDEPENDENT]

_(pending)_

## Task 4 — reload pipeline (re-parse → re-validate → atomic swap; warm-reject) [BLOCKED on Task 1]

_(pending)_

## Task 5 — per-HCM `rds.*` counters tick per reload (thread phase-20 handles)

_(pending)_

## Task 6 — `/config_dump` `RoutesConfigDump` through the swappable handle + version update [BLOCKED on Task 1]

_(pending)_

## Task 7 — harness mid-test file-rewrite + settle-then-probe [BLOCKED on Task 1]

_(pending)_

## Task 8 — fixture `0034-xds-rds-hot-reload` + Docker wrapper (Linux-CI-authoritative) + in-process backstop [BLOCKED on Task 1]

_(pending)_

## Task 9 — CONDITIONAL: `ConfigSource.watched_directory` field + fuzz seed (fires only if Task 1 P2 requires it)

_(pending — conditional)_

## Task 10 — state-4 phase-done verification + STATE advance to state-5-next

_(pending)_
