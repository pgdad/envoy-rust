# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `01`
**slug:** `01-static-bootstrap-config`
**directory:** `docs/envoy-rust/phases/01-static-bootstrap-config/` (exists; contains `SPEC.md`)
**status:** phase 01 lifecycle **state 2** — `SPEC.md` exists (committed at `43f489e`); `PLAN.md` does not yet exist.

Phase 00 (`00-bootstrap`) is **done** as of prior session. ROADMAP.md
phase-00 row is `done`; all six phase-00 ADRs (`ADR-0002`..`ADR-0007`)
are landed in `DECISIONS.md`; the differential fixture
`tests/fixtures/0001-tcp-echo` is green on CI.

## Next expected skill

Per the phase lifecycle state machine (`SKILL_ROUTING.md` lines 17–21,
verbatim from `BOOTSTRAP_PROMPT.md` §5 state 2): the next session —
operating as the state-2 session of phase 01 — invokes
**`superpowers:writing-plans`** scoped to this phase, producing
`docs/envoy-rust/phases/01-static-bootstrap-config/PLAN.md`.

**Phase-02 splitting gate** (§5 state 2 / §6.1 of `BOOTSTRAP_PROMPT.md`):
if `PLAN.md` exceeds ~25 numbered tasks or ~1500 LoC of estimated net
change, split phase 01 into `01.1` and `01.2`. The SPEC already
identifies a natural cut (§5 of `SPEC.md`):

- **01.1** — `envoy-config` crate extraction + fuzz target + ADR-0008/0009/0010.
- **01.2** — admin HTTP endpoint + harness grammar + fixture `0002` + ADR-0011.

Do **not** pre-emptively split; apply the gate only if the plan actually
crosses the threshold.

Inputs the state-2 session should read, in order, before planning:

1. `docs/envoy-rust/MISSION.md` (project mission — unchanged).
2. `docs/envoy-rust/STATE.md` (this file — to confirm routing).
3. `docs/envoy-rust/ROADMAP.md` (phase 01 row: "Static bootstrap config
   loader (node, admin, static_resources skeleton)", depends-on `00`).
4. `docs/envoy-rust/DECISIONS.md` (all landed ADRs through `ADR-0007`;
   `ADR-0008`..`ADR-0011` will be added during phase-01 execution per
   SPEC §D9).
5. `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (equivalence rules —
   phase 01 exercises rows 1 (status exact) + 2 (body byte-exact);
   header allow-list stays empty per ADR-0011).
6. `docs/envoy-rust/SKILL_ROUTING.md` (routing reference).
7. `docs/envoy-rust/phases/01-static-bootstrap-config/SPEC.md` (the
   authoritative design contract for this phase).
8. `docs/envoy-rust/phases/00-bootstrap/SPEC.md` +
   `docs/envoy-rust/phases/00-bootstrap/PROGRESS.md` (what's already
   wired: the `envoy-bin` YAML loader, the `tests/differential`
   harness skeleton, `drive_tcp` + ADR-0007 trailing-byte poll).

## Last commit

State-1 phase-01 brainstorm output:
`phase 01: SPEC.md — static bootstrap config loader + admin /ready`
(commit `43f489e`). Creates
`docs/envoy-rust/phases/01-static-bootstrap-config/SPEC.md` (846
lines). No ADRs landed in this session — ADR-0008..ADR-0011 are
scheduled by SPEC §D9 to land during state-3 execution.

## Last updated

2026-04-23 (state 1 complete — phase 01 SPEC.md committed; STATE
advanced to state 2)

## Notes

- Per `BOOTSTRAP_PROMPT.md` §5.1, sessions move exactly one state
  forward. This session executed phase-01's state 1
  (`superpowers:brainstorming` → SPEC.md + commit + this STATE
  advance) and exits. The next session enters phase 01 at state 2 via
  `superpowers:writing-plans`.
- SPEC.md §5 pre-identifies a clean 01.1/01.2 split at the
  envoy-config-extraction boundary — use it only if the §6.1
  thresholds fire.
- Deferred phase-00 items still open (tracked on `STATE.md` until
  closed by a future Minor-cleanup commit outside phase 01):
  - **N2** — `deny_unknown_fields` regression-test coverage gap on
    5 deeper structs (`StaticResources`, `Address`, `SocketAddress`,
    `FilterChain`, `NetworkFilter`). Phase 01 moves these structs
    into `crates/envoy-config/src/bootstrap.rs`; the planner may
    opportunistically close N2 there (add 5 regression tests —
    trivial) or defer further. No block on phase 01.
  - Seven pre-existing Minors (M1, M2, M4, M5, M6, M7, M8) from the
    phase-00 state-5 review. See `phases/00-bootstrap/REVIEW.md`.
- Phase 00's differential harness scaffolding (`tests/differential`
  crate, `run_fixture`, `drive_tcp`, `testcontainers` upstream Envoy
  subprocess, `Subject` local subprocess) is to be **extended, not
  rebuilt**, per SPEC §D5.
- Consult `docs/envoy-rust/SKILL_ROUTING.md` for the full phase
  lifecycle state machine.
- Any deviation from the state machine requires
  `superpowers:systematic-debugging` before proceeding — see §1
  Step E of `BOOTSTRAP_PROMPT.md`.
