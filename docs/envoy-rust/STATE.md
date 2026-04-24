# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `01`
**slug:** `01-static-bootstrap-config`
**directory:** `docs/envoy-rust/phases/01-static-bootstrap-config/` (exists; contains `SPEC.md` and `PLAN.md`)
**status:** phase 01 lifecycle **state 3** — `SPEC.md` exists (commit `43f489e`); `PLAN.md` exists (commit `859b1cb`); implementation incomplete.

Phase 00 (`00-bootstrap`) is **done** as of commit `e5afc35`.

## Next expected skill

Per the phase lifecycle state machine (`SKILL_ROUTING.md` lines 23–27,
verbatim from `BOOTSTRAP_PROMPT.md` §5 state 3): the next session —
operating as the state-3 session of phase 01 — invokes
**`superpowers:subagent-driven-development`** scoped to this phase,
executing `PLAN.md` task-by-task with a fresh subagent per task plus the
two-stage (spec-compliance + code-quality) review cadence the skill
mandates, appending a section to `PROGRESS.md` on each task completion.

Every implementation task inside `PLAN.md` enforces
`superpowers:test-driven-development` per doctrine D-3.1.

**Plan splitting gate already evaluated** (BOOTSTRAP_PROMPT.md §5 state 2 /
§6.1; SPEC §5):

- Task count: 19 (bound: ~25).
- Estimated net LoC change: ~1900 gross, ~50/50 split between code and
  tests/ADR-prose (bound: ~1500 net code).
- Decision: **kept unified** per SPEC §5's "thresholds exist to catch
  overscoping, not to enforce a shape" rubric and its explicit "do not
  pre-emptively split" directive. If a single task during execution
  blows past its own bite-sized budget, the executor splits phase 01
  at the SPEC §5 pre-identified 01.1 / 01.2 cut line immediately per
  the deviation protocol — no ADR needed for an in-execution split.

Inputs the state-3 session should read, in order, before launching the
first subagent:

1. `docs/envoy-rust/MISSION.md` (mission — unchanged).
2. `docs/envoy-rust/STATE.md` (this file — to confirm routing).
3. `docs/envoy-rust/ROADMAP.md` (phase 01 row: "Static bootstrap config
   loader (node, admin, static_resources skeleton)", depends-on `00`).
4. `docs/envoy-rust/DECISIONS.md` (all landed ADRs through `ADR-0007`;
   `ADR-0008`..`ADR-0011` land during this phase per `PLAN.md` Tasks 1 + 8;
   a conditional `ADR-0012` may land if `cargo deny check` flips red on
   the `libfuzzer-sys` transitive chain — see `PLAN.md` Task 7 Step 4).
5. `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (equivalence rules — phase 01
   exercises rows 1 (status exact) + 2 (body byte-exact); header
   allow-list stays empty per ADR-0011, landed in `PLAN.md` Task 8).
6. `docs/envoy-rust/SKILL_ROUTING.md` (routing reference).
7. `docs/envoy-rust/phases/01-static-bootstrap-config/SPEC.md` (the
   authoritative design contract — referenced at every task under the
   phrase "Source of truth: SPEC.md" at the top of `PLAN.md`).
8. `docs/envoy-rust/phases/01-static-bootstrap-config/PLAN.md` (the
   operational plan; 19 tasks; task boundaries are the natural
   subagent-dispatch boundaries).
9. `docs/envoy-rust/phases/00-bootstrap/SPEC.md` +
   `docs/envoy-rust/phases/00-bootstrap/PROGRESS.md` (context on what
   phase 00 left in place: the `envoy-bin` YAML loader being moved into
   `envoy-config`, the `tests/differential` harness skeleton being
   extended, `drive_tcp` + ADR-0006/0007 contract, the seven pre-existing
   deferred Minors M1/M2/M4–M8, and the N2 closure (5 deeper-struct
   `deny_unknown_fields` regression tests) that `PLAN.md` Task 4 picks
   up per STATE.md's prior line 87–90 note).

## Last commit

State-2 phase-01 plan output:
`phase 01: PLAN.md — static bootstrap config loader` (commit `859b1cb`).
Creates `docs/envoy-rust/phases/01-static-bootstrap-config/PLAN.md`
(~3350 lines; 19 tasks). No ADRs landed in this session — the four
phase-01 ADRs (0008–0011) are scheduled by `PLAN.md` Tasks 1 + 8 during
state-3 execution.

## Last updated

2026-04-24 (state 2 complete — phase 01 PLAN.md committed; STATE
advanced to state 3).

## Notes

- Per `BOOTSTRAP_PROMPT.md` §5.1, sessions move exactly one state
  forward. This session executed phase-01's state 2
  (`superpowers:writing-plans` → `PLAN.md` + commit + this STATE
  advance) and exits. The next session enters phase 01 at state 3 via
  `superpowers:subagent-driven-development`.
- `PLAN.md` §5 "Plan completion" prescribes that the session finishing
  the plan's 19 tasks also advances STATE.md to state 4 (verified) —
  because that final session moves through both state-3 work (the 19
  task commits) and the state-4 phase-done gate in Task 19.
- N2 (phase-00 deferred Minor — `deny_unknown_fields` regression-test
  gap on `StaticResources`, `Address`, `SocketAddress`, `FilterChain`,
  `NetworkFilter`) is closed by `PLAN.md` Task 4 Step 4 via five new
  regression tests (`rejects_unknown_static_resources_field`, etc.).
  No remaining phase-00 carryover on this front.
- Pre-existing phase-00 deferred Minors M1, M2, M4, M5, M6, M7, M8
  remain open; none block phase 01. See
  `docs/envoy-rust/phases/00-bootstrap/REVIEW.md`.
- The phase-00 I3 SIGKILL→SIGTERM functional switch remains deferred
  from phase-00 state-5 re-review. Phase-01 SPEC does **not** pick it up;
  execution may touch `Subject::shutdown` if a task genuinely needs it
  (none of the 19 tasks do today), at which point a new ADR lands per
  D-3.5 — the `nix` crate is still the stated blocker because it is not
  on the D-3.2 permitted-foundations list.
- Any deviation from the state machine requires
  `superpowers:systematic-debugging` before proceeding — see §1 Step E
  of `BOOTSTRAP_PROMPT.md`.
- Consult `docs/envoy-rust/SKILL_ROUTING.md` for the full phase
  lifecycle state machine.
