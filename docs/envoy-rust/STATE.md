# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `01`
**slug:** `01-static-bootstrap-config`
**directory:** `docs/envoy-rust/phases/01-static-bootstrap-config/` (not yet created)
**status:** phase 01 entering lifecycle **state 1** — row exists in `ROADMAP.md` with status `planned`, phase directory does not yet exist.

Phase 00 (`00-bootstrap`) is **done** as of this session. ROADMAP.md
phase-00 row flipped `planned → done`; all six phase-00 ADRs
(`ADR-0002`..`ADR-0007`) are landed in `DECISIONS.md`; the differential
fixture `tests/fixtures/0001-tcp-echo` is green on CI (run
`24859537419`); state-5 re-review returned Approved with new Minors
only (N1 closed in state 6 via PROGRESS.md deviation note, N2 deferred
to a future Minor-cleanup commit, seven pre-existing Minors M1/M2/M4–M8
carried forward).

## Next expected skill

Per the phase lifecycle state machine (`SKILL_ROUTING.md` lines 12–15,
verbatim from `BOOTSTRAP_PROMPT.md` §5 state 1): the next session —
operating as the first session of phase 01 — creates
`docs/envoy-rust/phases/01-static-bootstrap-config/` and invokes
**`superpowers:brainstorming`** scoped to this phase only, producing
`SPEC.md`. Do **not** invoke project-level brainstorming (that is the
state-0 action, reserved for adding/refining a ROADMAP row that does
not yet exist — phase 01's row is already present and its title and
summary are fixed).

Inputs the state-1 session should read, in order, before brainstorming:

1. `docs/envoy-rust/MISSION.md` (project mission — unchanged).
2. `docs/envoy-rust/STATE.md` (this file — to confirm routing).
3. `docs/envoy-rust/ROADMAP.md` (phase 01 row: "Static bootstrap config
   loader (node, admin, static_resources skeleton)", depends-on `00`,
   summary "config parses; admin `/ready` behaves like Envoy").
4. `docs/envoy-rust/DECISIONS.md` (all landed ADRs through `ADR-0007`
   — relevant context: ADR-0002 workspace layout, ADR-0003 pinned
   toolchain + MSRV, ADR-0004 Envoy v1.33.0 target pin, ADR-0005
   cargo-deny wrappers, ADR-0006 `drive_tcp` rewrite, ADR-0007
   trailing-byte poll).
5. `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (equivalence rules —
   especially §config-parse, §admin endpoints, §lifecycle).
6. `docs/envoy-rust/SKILL_ROUTING.md` (routing reference).
7. `docs/envoy-rust/phases/00-bootstrap/SPEC.md` and
   `docs/envoy-rust/phases/00-bootstrap/PROGRESS.md` (phase-00
   outputs — what's already wired: the `envoy-bin` YAML loader with
   `deny_unknown_fields`, the minimal static-listener echo schema,
   the differential harness with `run_fixture` orchestrator).

## Last commit

State-6 final phase-00 commit (format per `BOOTSTRAP_PROMPT.md` §5.3):
`phase 00: bootstrap differential harness + envoy-bin TCP echo
[ADR-0002, ADR-0003, ADR-0004, ADR-0005, ADR-0006, ADR-0007]`. Includes
the N1 closure note appended to `phases/00-bootstrap/PROGRESS.md`, the
ROADMAP.md phase-00 row flip `planned → done`, and this STATE.md
advance to phase 01.

## Last updated

2026-04-23 (state 6 complete — phase 00 `done`; STATE advanced to
phase 01 lifecycle state 1)

## Notes

- Per `BOOTSTRAP_PROMPT.md` §5.1, sessions move exactly one state
  forward. This session executed phase-00's state 6 (final commit +
  ROADMAP flip + STATE advance) and exits. The next session enters
  phase 01 at state 1 via `superpowers:brainstorming`.
- Deferred phase-00 items (to address in a future Minor-cleanup commit
  outside phase 00's scope, so they do not block phase 01 entry):
  - **N2** — `deny_unknown_fields` regression-test coverage gap:
    regression tests cover root `Bootstrap` + nested `Listener`; the
    5 deeper structs `StaticResources`, `Address`, `SocketAddress`,
    `FilterChain`, `NetworkFilter` carry the attribute but are not
    individually regression-tested. Surfaced in state-5 re-review;
    batched forward.
  - Seven pre-existing Minors (M1, M2, M4, M5, M6, M7, M8) from the
    state-5 review, all labeled "nice to have; can defer". See
    `phases/00-bootstrap/REVIEW.md` for full detail.
- Phase 01's SPEC.md brainstorming should scope *only* to the static
  bootstrap config loader: the `node` field, the `admin` stanza
  (including admin `/ready`), and the `static_resources` skeleton.
  Listeners and clusters inside `static_resources` remain stubs
  (phase 02 adds TCP proxy + cluster + round-robin). The differential
  surface this phase must light up is "config parses; admin `/ready`
  behaves like Envoy" — i.e. a fixture that loads a static bootstrap
  and hits `/ready` against both reference Envoy and envoy-rust.
- Phase 00's differential harness scaffolding (`tests/differential`
  crate, `run_fixture`, `drive_tcp`, `testcontainers` upstream Envoy
  subprocess, `Subject` local subprocess) is already present and
  should be extended, not rebuilt, for phase 01's admin fixture.
- Consult `docs/envoy-rust/SKILL_ROUTING.md` for the full phase
  lifecycle state machine.
- Any deviation from the state machine requires
  `superpowers:systematic-debugging` before proceeding — see §1
  Step E of `BOOTSTRAP_PROMPT.md`.
