# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `02.1`
**slug:** `02.1-config-cluster`
**directory:** `docs/envoy-rust/phases/02.1-config-cluster/` (exists; contains `SPEC.md`).
**status:** phase 02.1 lifecycle **state 2 (SPEC.md exists, PLAN.md does not)** — row exists in `ROADMAP.md` with `status: planned`; `SPEC.md` landed in this session alongside the ADR-0013 split decision; no PLAN / PROGRESS / REVIEW yet.

Phase 02 (`02-tcp-proxy`) was split during the plan-writing step at state 2 via **ADR-0013** (landed in this session). The parent row's `status` is now `in-progress` with `sub-phases = 02.1, 02.2`. Parent `SPEC.md` remains in-tree unedited at `docs/envoy-rust/phases/02-tcp-proxy/SPEC.md` as the committed design artifact from SHA `50349da`; sub-phase SPECs supersede it for execution purposes. Rationale: SPEC §5 estimated ~2060 LoC of net change (~37% above the `BOOTSTRAP_PROMPT.md` §6 LoC gate of ~1500); the SPEC-designed 02.1/02.2 split boundary applies. Full split rationale and renumbering scheme live in ADR-0013.

Sub-phase 02.2 (`02.2-listener-tcp-proxy`) has its SPEC.md landed in this session but is `planned` — it depends on 02.1 being `done`, and STATE.md will advance from 02.1 to 02.2 only after 02.1's final commit.

Phase 01 (`01-static-bootstrap-config`) is **done** as of commit `aef36ce`; phase 00 (`00-bootstrap`) is **done** as of commit `e5afc35`.

## Next expected skill

**`superpowers:writing-plans`** — the next session writes `docs/envoy-rust/phases/02.1-config-cluster/PLAN.md` against the committed `02.1-config-cluster/SPEC.md`. Per the user's standing preference (saved in auto-memory), downstream execution uses `superpowers:subagent-driven-development` over inline `executing-plans`.

Inputs the plan-writer must read:

- `BOOTSTRAP_PROMPT.md` §4 (on-disk artifact layout), §5 (phase lifecycle), §6 (splitting policy), §7 (differential test contract), §8 row 02.
- `docs/envoy-rust/MISSION.md`, `docs/envoy-rust/BEHAVIOR_CONTRACT.md`, `docs/envoy-rust/DECISIONS.md` (ADR-0001..0013).
- `docs/envoy-rust/phases/02.1-config-cluster/SPEC.md` (authoritative sub-phase contract).
- `docs/envoy-rust/phases/01-static-bootstrap-config/PLAN.md` and `PROGRESS.md` (shape reference — phase 01's plan is the closest precedent for granularity and TDD framing).
- `docs/envoy-rust/phases/02-tcp-proxy/SPEC.md` (parent, for context on the full phase-02 design; execution uses the sub-phase SPEC).

### Splitting guidance summary

Sub-phase 02.1 estimates **~13 tasks** and **~980 LoC** of net change (SPEC §5). Both `BOOTSTRAP_PROMPT.md` §6 gates (> ~25 tasks OR > ~1500 LoC) hold comfortably. **Do not split 02.1 further.** If the plan as actually written crosses either gate mid-write, invoke `superpowers:systematic-debugging` before attempting a nested split — nested splits of a split sub-phase were not anticipated at the parent brainstorm and deserve a fresh root-cause analysis.

## Last commit

Phase-02 split commit (this session): updates `ROADMAP.md` (row 02 → `in-progress` with sub-phases; rows 02.1, 02.2 added with `status: planned`), appends `ADR-0013` to `DECISIONS.md`, lands `docs/envoy-rust/phases/02.1-config-cluster/SPEC.md` and `docs/envoy-rust/phases/02.2-listener-tcp-proxy/SPEC.md`, and advances this `STATE.md`. No code changes. Parent `02-tcp-proxy/SPEC.md` unedited.

## Last updated

2026-04-24 (phase 02 split into 02.1 + 02.2 at state 2; active phase advanced from 02 to 02.1; lifecycle state remains 2; next-skill remains `superpowers:writing-plans` but now scoped to 02.1).

## Notes

### ADR numbering after the split

The parent-phase SPEC (`02-tcp-proxy/SPEC.md`, committed at SHA `50349da`) projected three phase-02 ADRs numbered 0013 (typed_config), 0014 (host-docker + host-gateway), 0015 (enable_half_close false default). The ADR-0013 split decision (landed in this session) took the actual next-sequential number, so each projected ADR shifts by +1 in-tree:

- **ADR-0013** — split phase 02 into 02.1 + 02.2 (landed in this session).
- **ADR-0014** — YAML-native `typed_config` deserialization (lands during 02.1 execution; was parent-SPEC §7's ADR-0013).
- **ADR-0015** — cross-container host reachability via `host.docker.internal` + `host-gateway` (lands during 02.2 execution; was parent-SPEC §7's ADR-0014).
- **ADR-0016** — phase 02 TCP proxy runs with Envoy's default `enable_half_close: false` (lands during 02.2 execution; was parent-SPEC §7's ADR-0015).

The sub-phase SPECs cite ADR-0013 for the renumbering and rewrite each expected ADR with its actual number. The parent SPEC is preserved unedited per D-3.4 / D-3.5 (it's a committed historical artifact, not a living document).

### Phase-01 rollovers distributed across sub-phases

Per ADR-0013's split decision, phase-01 REVIEW §9 starter items are distributed:

- **I3** — four unit tests for `decode_chunked` in `tests/differential/src/lib.rs`: lands in **02.1** (harness-only; rides with 02.1's envoy-config / harness touches).
- **I4** — admin 8 KiB header cap tightening in `crates/envoy-bin/src/admin.rs:158–170`: lands in **02.2** (touches `envoy-bin::admin`, alongside 02.2's `envoy-bin` wiring).
- **M1** — retargeting the stale `TODO(phase-01)` comment in `tests/differential/src/subject.rs:25–32`: lands in **02.2** (sits alongside 02.2's harness work).

### Phase-00 deferrals still open

- Minors M1, M2, M4, M5, M6, M7, M8 (see `docs/envoy-rust/phases/00-bootstrap/REVIEW.md`). None block 02.1 or 02.2.
- Important I3 (SIGKILL → SIGTERM graceful termination of the subject subprocess): still deferred. The `nix` crate remains the stated blocker (not on D-3.2 permitted-foundations list). Phase-01 and phase-02 (across 02.1 and 02.2) all chose not to take it. A future phase that genuinely needs `nix` adds it under a new ADR and closes this item. 02.2's rollover M1 retargets the stale TODO comment to reflect this open-ended deferral.
- N2 (phase-00 deferred Minor — `deny_unknown_fields` regression-test gap on deeper struct levels): **closed** by phase-01 Task 4 Step 4 via five new regression tests.

### Phase-01 ADR ledger (for reference)

ADR-0008 (envoy-config extraction), ADR-0009 (cargo-fuzz + libfuzzer-sys as fuzz-only dev deps), ADR-0010 (nightly toolchain, explicit `+nightly` CI invocation; workspace-root pin stays stable), ADR-0011 (phase-01 defers response-header equivalence to phase 04; `server: envoy-rust` tolerated until then), ADR-0012 (nested nightly pin in fuzz subcrate; narrowly supersedes ADR-0010 on that single sub-point while preserving its main decision).

### Parent phase 02 lifecycle after the split

Per `ROADMAP.md` schema ("The parent flips to `done` only after all sub-phases are `done`"), row 02 stays `in-progress` until both 02.1 and 02.2 have landed their final commits. The canonical flip-to-`done` for row 02 happens in the **same commit** as 02.2's final commit (see `02.2-listener-tcp-proxy/SPEC.md` §8).

### Doctrine reminders

- Any deviation from the state machine requires `superpowers:systematic-debugging` before proceeding — see §1 Step E of `BOOTSTRAP_PROMPT.md`.
- Consult `docs/envoy-rust/SKILL_ROUTING.md` for the full phase lifecycle state machine.
- `BOOTSTRAP_PROMPT.md` §5.1: one state per session; do not chain states. The sole exception was §10's first-session bootstrap, which is unavailable to every subsequent session. The current session's split action (writing two sub-phase SPECs in one session) is itself the state-2 output per §6.2 — each sub-phase SPEC emerges as a redistribution of the committed parent SPEC, not as two separate brainstorming runs.
