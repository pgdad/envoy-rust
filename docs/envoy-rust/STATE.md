# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `00`
**slug:** `00-bootstrap`
**directory:** `docs/envoy-rust/phases/00-bootstrap/`
**status:** `in-progress` (lifecycle state 2 — SPEC.md exists, PLAN.md pending)

## Next expected skill

`superpowers:writing-plans` — scoped to phase 00. Consumes `docs/envoy-rust/phases/00-bootstrap/SPEC.md` and produces `docs/envoy-rust/phases/00-bootstrap/PLAN.md`.

`PLAN.md` must be evaluated against the splitting thresholds in §6 of `BOOTSTRAP_PROMPT.md` (~25 tasks or ~1500 LoC). If either threshold is crossed, follow the split guidance in §5 of the SPEC (00.1 scaffolding / 00.2 echo fixture + harness) rather than inventing a new split.

## Last commit

`phase 00: spec brainstormed` (this commit — landed SPEC.md and flipped STATE to state 2).

## Last updated

2026-04-23

## Notes

- The `docs/envoy-rust/` scaffold was committed by `bootstrap: envoy-rust project scaffold` on 2026-04-23; SPEC.md was landed in the follow-up commit named above.
- Consult `docs/envoy-rust/SKILL_ROUTING.md` for the full phase lifecycle state machine.
- Any deviation from the state machine requires `superpowers:systematic-debugging` before proceeding — see §1 Step E of `BOOTSTRAP_PROMPT.md`.
