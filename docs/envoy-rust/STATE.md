# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `00`
**slug:** `00-bootstrap`
**directory:** `docs/envoy-rust/phases/00-bootstrap/`
**status:** `in-progress` (lifecycle state 1 — phase directory being created, SPEC.md pending)

## Next expected skill

`superpowers:brainstorming` — scoped to phase 00. Produces `docs/envoy-rust/phases/00-bootstrap/SPEC.md`.

After SPEC.md lands the next session enters lifecycle state 2, whose next expected skill is `superpowers:writing-plans`.

## Last commit

`bootstrap: envoy-rust project scaffold` (scaffolds this file).

## Last updated

2026-04-23

## Notes

- This is the very first session after repo bootstrap; `docs/envoy-rust/` was empty before this commit.
- Consult `docs/envoy-rust/SKILL_ROUTING.md` for the full phase lifecycle state machine.
- Any deviation from the state machine requires `superpowers:systematic-debugging` before proceeding — see §1 Step E of `BOOTSTRAP_PROMPT.md`.
