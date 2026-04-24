# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `02.1`
**slug:** `02.1-config-cluster`
**directory:** `docs/envoy-rust/phases/02.1-config-cluster/` (exists; contains `SPEC.md`, `PLAN.md`, `PROGRESS.md`, and `REVIEW.md`).
**status:** phase 02.1 lifecycle **state 5 (REVIEW.md approved; state-6 phase-done commit next)** — `SPEC.md` landed at `1c38ca9`; `PLAN.md` landed at `0779308`; all 13 PLAN.md tasks implemented (`6d1f8d6..cadeaa6`); state-4 phase-done gate cleared on CI run `24909836488` (PROGRESS.md §"State 4") with state-4 follow-up at `95a26a7`; `Cargo.lock` sync at `dea4d16` (REVIEW §3 I1 close-out); REVIEW.md landed with verdict **Approved** in the same commit that flips this STATE.md from state 3 to state 5. ROADMAP row `02.1` remains `status: planned` until the phase-done commit.

Phase 02 (`02-tcp-proxy`) was split during state 2 via **ADR-0013** (landed at commit `1c38ca9`). The parent row's `status` is `in-progress` with `sub-phases = 02.1, 02.2`. Parent `SPEC.md` remains in-tree unedited at `docs/envoy-rust/phases/02-tcp-proxy/SPEC.md` as the committed design artifact from SHA `50349da`; sub-phase SPECs supersede it for execution purposes.

Sub-phase 02.2 (`02.2-listener-tcp-proxy`) has its SPEC.md landed at `1c38ca9` but is `planned` — it depends on 02.1 being `done`, and STATE.md will advance from 02.1 to 02.2 in the state-6 phase-done commit per `BOOTSTRAP_PROMPT.md` §5.3 / SKILL_ROUTING.md state 6.

Phase 01 (`01-static-bootstrap-config`) is **done** as of commit `aef36ce`; phase 00 (`00-bootstrap`) is **done** as of commit `e5afc35`.

## Next expected skill

Per the phase lifecycle state machine (`SKILL_ROUTING.md` lines 44–48, verbatim from `BOOTSTRAP_PROMPT.md` §5 state 6): the next session — operating as the state-6 session of phase 02.1 — lands the **phase-done commit** with message format per `BOOTSTRAP_PROMPT.md` §5.3:

```
phase 02.1: <title> [ADR-0014]

<3–6 paragraph narrative covering landed surface, differential/conformance
status, gate evidence (CI runs 24909836488 state-4 and any state-5/6 CI
runs), rollovers carried forward to 02.2 or later phases.>
```

The commit flips `ROADMAP.md` row `02.1` status from `planned` to `done`, and advances this STATE.md from phase 02.1 to phase 02.2 (slug `02.2-listener-tcp-proxy`, lifecycle state 1, next skill `superpowers:brainstorming` scoped to the sub-phase SPEC). Parent row `02` remains `in-progress` — it flips to `done` in the same commit as 02.2's final phase-done commit per `ROADMAP.md` schema ("parent flips to `done` only after all sub-phases are `done`").

State 6 is a docs-only commit (no code changes). No further review is required — REVIEW.md's §7 final verdict stands.

Inputs the state-6 session should read, in order:

1. `docs/envoy-rust/MISSION.md` (mission — unchanged).
2. `docs/envoy-rust/STATE.md` (this file — to confirm routing).
3. `docs/envoy-rust/ROADMAP.md` (phase 02 row; phase 02.1 row still `planned` at session entry).
4. `docs/envoy-rust/DECISIONS.md` (all landed ADRs through `ADR-0014`).
5. `docs/envoy-rust/SKILL_ROUTING.md` state 6 block.
6. `docs/envoy-rust/phases/02.1-config-cluster/SPEC.md` §8 ("Commit discipline" / "Phase-done commit" — whichever section captures the commit-message conventions).
7. `docs/envoy-rust/phases/02.1-config-cluster/REVIEW.md` (Approved verdict — §7 close-out).
8. `docs/envoy-rust/phases/02.1-config-cluster/PROGRESS.md` (state-4 gate evidence, for the commit narrative).
9. `docs/envoy-rust/phases/01-static-bootstrap-config/` (shape precedent — commit `aef36ce` is the state-6 phase-done commit; `f436c29` is the state-5 REVIEW-landing precedent).
10. `docs/envoy-rust/phases/02.2-listener-tcp-proxy/SPEC.md` (destination phase — state 6 advances STATE.md here; lifecycle state 1, next-skill `superpowers:brainstorming`).

## Last commit

REVIEW.md landing commit (this session): lands `docs/envoy-rust/phases/02.1-config-cluster/REVIEW.md` with state-5 verdict **Approved** and advances this STATE.md from state 3 to state 5. Preceded in the same session by `dea4d16` (Cargo.lock sync — REVIEW §3 I1 close-out; phase-01 precedent `4955252`).

## Last updated

2026-04-24 (phase 02.1 lifecycle state advanced from 3 to 5; REVIEW.md committed with verdict Approved; next-skill flips from `superpowers:subagent-driven-development` to the state-6 phase-done commit per `BOOTSTRAP_PROMPT.md` §5 state 6).

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
