# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `02.1`
**slug:** `02.1-config-cluster`
**directory:** `docs/envoy-rust/phases/02.1-config-cluster/` (exists; contains `SPEC.md` and `PLAN.md`).
**status:** phase 02.1 lifecycle **state 3 (PLAN.md exists, implementation incomplete)** — `SPEC.md` landed at commit `1c38ca9` (alongside the ADR-0013 split decision); `PLAN.md` landed at commit `0779308` (this session). ROADMAP row `02.1` remains `status: planned` until the phase-done commit per `PLAN.md` Task 13.

Phase 02 (`02-tcp-proxy`) was split during state 2 via **ADR-0013** (landed at commit `1c38ca9`). The parent row's `status` is `in-progress` with `sub-phases = 02.1, 02.2`. Parent `SPEC.md` remains in-tree unedited at `docs/envoy-rust/phases/02-tcp-proxy/SPEC.md` as the committed design artifact from SHA `50349da`; sub-phase SPECs supersede it for execution purposes.

Sub-phase 02.2 (`02.2-listener-tcp-proxy`) has its SPEC.md landed at `1c38ca9` but is `planned` — it depends on 02.1 being `done`, and STATE.md will advance from 02.1 to 02.2 only after 02.1's phase-done commit (`PLAN.md` Task 13, Step 7 of the future state-6 session).

Phase 01 (`01-static-bootstrap-config`) is **done** as of commit `aef36ce`; phase 00 (`00-bootstrap`) is **done** as of commit `e5afc35`.

## Next expected skill

Per the phase lifecycle state machine (`SKILL_ROUTING.md` lines 23–27, verbatim from `BOOTSTRAP_PROMPT.md` §5 state 3): the next session — operating as the state-3 session of phase 02.1 — invokes **`superpowers:subagent-driven-development`** scoped to this phase, executing `PLAN.md` task-by-task with a fresh subagent per task plus the two-stage (spec-compliance + code-quality) review cadence the skill mandates, appending a section to `PROGRESS.md` on each task completion.

Every implementation task inside `PLAN.md` enforces `superpowers:test-driven-development` per doctrine D-3.1.

Per the user's standing preference (auto-memory `feedback_execution_style`), execution uses `superpowers:subagent-driven-development` over inline `executing-plans` — do not present the two-option fork.

**Plan splitting gate already evaluated** (BOOTSTRAP_PROMPT.md §5 state 2 / §6.1; SPEC §5; PLAN self-review §4):

- Task count: 13 (bound: ~25).
- Estimated net LoC change: ~980 (bound: ~1500).
- Decision: **kept unified**. Both gates hold comfortably. Per the sub-phase discussion in `STATE.md`'s splitting guidance (landed at `1c38ca9`), **do not split 02.1 further**. If a single task during execution blows past its own bite-sized budget, the executor invokes `superpowers:systematic-debugging` before attempting a nested split — nested splits of a split sub-phase were not anticipated at the parent brainstorm and deserve a fresh root-cause analysis.

Inputs the state-3 session should read, in order, before launching the first subagent:

1. `docs/envoy-rust/MISSION.md` (mission — unchanged).
2. `docs/envoy-rust/STATE.md` (this file — to confirm routing).
3. `docs/envoy-rust/ROADMAP.md` (phase 02 row: `in-progress` with sub-phases `02.1, 02.2`; phase 02.1 row: "Config schema + cluster manager + echo-server helper"; depends-on `01`).
4. `docs/envoy-rust/DECISIONS.md` (all landed ADRs through `ADR-0013`; `ADR-0014` lands during this phase per `PLAN.md` Task 1 — see the renumbering note in §Notes below).
5. `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (equivalence rules — phase 02.1 does not ship a new differential fixture; existing fixtures `0001-tcp-echo` and `0002-static-admin-ready` remain green unchanged).
6. `docs/envoy-rust/SKILL_ROUTING.md` (routing reference).
7. `docs/envoy-rust/phases/02.1-config-cluster/SPEC.md` (the authoritative sub-phase design contract — referenced at every task under the phrase "Source of truth: SPEC.md" at the top of `PLAN.md`).
8. `docs/envoy-rust/phases/02.1-config-cluster/PLAN.md` (the operational plan; 13 tasks; task boundaries are the natural subagent-dispatch boundaries).
9. `docs/envoy-rust/phases/02-tcp-proxy/SPEC.md` (parent SPEC — for context on the full phase-02 design; execution follows the 02.1 sub-phase SPEC, not the parent).
10. `docs/envoy-rust/phases/01-static-bootstrap-config/PLAN.md` + `PROGRESS.md` (shape reference — phase 01's plan + progress file is the precedent for task granularity, TDD framing, and PROGRESS-formatting conventions `PLAN.md` Task 1 adopts).

## Last commit

Plan commit (this session): lands `docs/envoy-rust/phases/02.1-config-cluster/PLAN.md` (2992 lines, 13 tasks). No code changes; no other documents touched by this commit. The state-advance commit following this one touches only `docs/envoy-rust/STATE.md`.

## Last updated

2026-04-24 (phase 02.1 lifecycle state advanced from 2 to 3; PLAN.md committed at `0779308`; next-skill flips from `superpowers:writing-plans` to `superpowers:subagent-driven-development`).

## Notes

### ADR numbering after the phase-02 split

The parent-phase SPEC (`02-tcp-proxy/SPEC.md`, committed at SHA `50349da`) projected three phase-02 ADRs numbered 0013 (typed_config), 0014 (host-docker + host-gateway), 0015 (enable_half_close false default). The ADR-0013 split decision (landed at `1c38ca9`) took the actual next-sequential number, so each projected ADR shifts by +1 in-tree:

- **ADR-0013** — split phase 02 into 02.1 + 02.2 (landed at `1c38ca9`).
- **ADR-0014** — YAML-native `typed_config` deserialization (lands during 02.1 execution at Task 1; was parent-SPEC §7's ADR-0013).
- **ADR-0015** — cross-container host reachability via `host.docker.internal` + `host-gateway` (lands during 02.2 execution; was parent-SPEC §7's ADR-0014).
- **ADR-0016** — phase 02 TCP proxy runs with Envoy's default `enable_half_close: false` (lands during 02.2 execution; was parent-SPEC §7's ADR-0015).

The sub-phase SPECs cite ADR-0013 for the renumbering and rewrite each expected ADR with its actual number. The parent SPEC is preserved unedited per D-3.4 / D-3.5 (it's a committed historical artifact, not a living document).

### Phase-01 rollovers distributed across sub-phases

Per ADR-0013's split decision, phase-01 REVIEW §9 starter items are distributed:

- **I3** — four unit tests for `decode_chunked` in `tests/differential/src/lib.rs`: lands in **02.1** Task 11 (harness-only; rides with 02.1's envoy-config / harness touches).
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
- `BOOTSTRAP_PROMPT.md` §5.1: one state per session; do not chain states. This session's output is the state-2 → state-3 transition (plan commit + STATE advance). The next session begins state-3 execution via `superpowers:subagent-driven-development`.
