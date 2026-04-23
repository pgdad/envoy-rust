# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `00`
**slug:** `00-bootstrap`
**directory:** `docs/envoy-rust/phases/00-bootstrap/`
**status:** `in-progress` (lifecycle state 3 — PLAN.md landed, implementation pending)

## Next expected skill

`superpowers:subagent-driven-development` — scoped to phase 00. Consumes `docs/envoy-rust/phases/00-bootstrap/PLAN.md` and produces the code + artifacts the plan specifies, with one fresh subagent per task and review between tasks.

The plan was evaluated against the §6 splitting thresholds (~25 tasks, ~1500 LoC) at the end of state 2 and found to be under both (15 tasks, ~700 LoC of code + ~200 LoC of config/YAML/CI). **No split.** If mid-execution any single task's sub-step count blows past ~10, revisit per §6 mid-execution trigger.

Each task ends with its own commit per the plan. The phase-done gate (§7.5) is evaluated in state 4, not here. PROGRESS.md is appended to after every task (executor creates it on task 1).

## Last commit

`phase 00: plan drafted` — this commit lands `docs/envoy-rust/phases/00-bootstrap/PLAN.md` and flips STATE to lifecycle state 3.

## Last updated

2026-04-23

## Notes

- SPEC.md landed in `phase 00: spec brainstormed` (2026-04-23). PLAN.md landed in `phase 00: plan drafted` (2026-04-23).
- Per `BOOTSTRAP_PROMPT.md` §5.1, sessions move exactly one state forward. This session advanced state 2 → state 3 and now exits; the next session picks up at state 3 and begins execution.
- The plan's final commit (state 6) carries the full phase title and ADR list per §5.3: `phase 00: Bootstrap [ADR-0002, ADR-0003, ADR-0004]`. Individual task commits use `phase 00: <task>` for traceability.
- Consult `docs/envoy-rust/SKILL_ROUTING.md` for the full phase lifecycle state machine.
- Any deviation from the state machine requires `superpowers:systematic-debugging` before proceeding — see §1 Step E of `BOOTSTRAP_PROMPT.md`.
