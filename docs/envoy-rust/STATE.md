# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `02`
**slug:** `02-tcp-proxy`
**directory:** `docs/envoy-rust/phases/02-tcp-proxy/` (exists; contains `SPEC.md`).
**status:** phase 02 lifecycle **state 2 (SPEC.md exists, PLAN.md does not)** — row exists in `ROADMAP.md` with `status: planned`; `SPEC.md` landed at commit `50349da`; no PLAN / PROGRESS / REVIEW yet.

Phase 01 (`01-static-bootstrap-config`) is **done** as of commit `aef36ce` whose subject matches SPEC §8 and includes `[ADR-0008, ADR-0009, ADR-0010, ADR-0011, ADR-0012]`. `ROADMAP.md` row 01 is `done`. Phase-01 `REVIEW.md` verdict is **Approved** (I1 closed in-phase; I3/I4/M1 folded into phase 02 per brainstorm Q5).

Phase 00 (`00-bootstrap`) is **done** as of commit `e5afc35`.

## Next expected skill

**`superpowers:writing-plans`** — the next session writes `docs/envoy-rust/phases/02-tcp-proxy/PLAN.md` against the committed `SPEC.md`. Per the user's standing preference (saved in auto-memory), downstream execution uses `superpowers:subagent-driven-development` over inline `executing-plans`.

Inputs the plan-writer must read:

- `BOOTSTRAP_PROMPT.md` §4 (on-disk artifact layout), §5 (phase lifecycle, especially state 2's split gate: > ~25 tasks OR > ~1500 LoC estimated), §6 (splitting policy), §7 (differential test contract), §8 row 02.
- `docs/envoy-rust/MISSION.md`, `docs/envoy-rust/BEHAVIOR_CONTRACT.md`, `docs/envoy-rust/DECISIONS.md` (ADR-0001..0012).
- `docs/envoy-rust/phases/02-tcp-proxy/SPEC.md` (authoritative phase contract).
- `docs/envoy-rust/phases/01-static-bootstrap-config/PLAN.md` and `PROGRESS.md` (shape reference — phase 01's plan is the closest precedent for granularity and TDD framing).
- `docs/envoy-rust/phases/00-bootstrap/SPEC.md` / `REVIEW.md` (for phase-00 deferrals that remain open; see Notes).

### Splitting guidance summary

Phase-02 SPEC §5 estimates **~22 tasks** and **~2060 LoC** of net change. At ~22 tasks the §6 task gate holds (25 tasks), but the LoC estimate is **~37% above the §6 LoC gate (1500)**. The plan-writer reads this as a soft signal that a split is likely, confirms by line-counting the actual plan text, and if the gate is crossed, splits at the SPEC §5 boundary:

- **02.1 — Config schema + cluster manager + tcp-echo-server helper.** All `envoy-config` extensions (16 new tests); `envoy-cluster` crate complete with round-robin unit tests; `tcp-echo-server` helper crate; phase-01 rollover I3 (`decode_chunked` unit tests, harness-only). No `envoy-listener`, no `envoy-tcp`, no fixture 0003.
- **02.2 — Listener + TCP proxy + fixture 0003 + remaining rollovers.** `envoy-listener` + `envoy-tcp` crates; `envoy-bin` wiring; harness extensions (`TcpProxyBackend`, `render_yaml` expansion, upstream `with_host`); fixture 0003 end-to-end green; phase-01 rollovers I4 + M1; ADRs 0014 + 0015 (ADR-0013 lands with 02.1).

Do **not** pre-emptively split. Only split if the plan actually crosses the gate. Splitting (if triggered) updates `ROADMAP.md` (row 02 becomes the parent; rows 02.1 and 02.2 added as children), adds an ADR documenting the split (next sequential, likely ADR-0016), updates this file to point at `02.1`, and exits cleanly per `BOOTSTRAP_PROMPT.md` §6.2.

## Last commit

Phase-02 SPEC commit: `50349da` — `phase 02: SPEC — listener + TCP proxy filter + static cluster + round-robin LB (plaintext)`. Adds `docs/envoy-rust/phases/02-tcp-proxy/SPEC.md` (772 lines). No code changes.

## Last updated

2026-04-24 (phase 02 advanced from state 1 to state 2; SPEC committed).

## Notes

### Phase-01 rollovers (now absorbed into phase-02 SPEC)

Per phase-02 brainstorm Q5, all three phase-01 REVIEW §9 starter items are folded into phase 02 as minor cleanup deliverables (SPEC §D9):

- **I3** — four unit tests for `decode_chunked` in `tests/differential/src/lib.rs` (empty, chunk-extension, truncated, trailer).
- **I4** — tighten admin 8 KiB header cap in `crates/envoy-bin/src/admin.rs:158–170` from "effectively ~9 KiB" to an exact `MAX_REQUEST_HEAD` boundary (~2-line fix + updated test).
- **M1** — retarget the stale `TODO(phase-01)` comment in `tests/differential/src/subject.rs:25–32` (doc-only; points the SIGKILL→SIGTERM deferral at "a future phase that takes `nix` under its own ADR," naming no specific target phase).

### Phase-00 deferrals still open

- Minors M1, M2, M4, M5, M6, M7, M8 (see `docs/envoy-rust/phases/00-bootstrap/REVIEW.md`). None block phase 02.
- Important I3 (SIGKILL → SIGTERM graceful termination of the subject subprocess): still deferred. The `nix` crate remains the stated blocker (not on D-3.2 permitted-foundations list). Phase-01 and phase-02 both chose not to take it. A future phase that genuinely needs `nix` adds it under a new ADR and closes this item.
- N2 (phase-00 deferred Minor — `deny_unknown_fields` regression-test gap on deeper struct levels): **closed** by phase-01 Task 4 Step 4 via five new regression tests.

### Phase-01 ADR ledger (for reference)

ADR-0008 (envoy-config extraction), ADR-0009 (cargo-fuzz + libfuzzer-sys as fuzz-only dev deps), ADR-0010 (nightly toolchain, explicit `+nightly` CI invocation; workspace-root pin stays stable), ADR-0011 (phase-01 defers response-header equivalence to phase 04; `server: envoy-rust` tolerated until then), ADR-0012 (nested nightly pin in fuzz subcrate; narrowly supersedes ADR-0010 on that single sub-point while preserving its main decision).

### Phase-02 ADRs expected

Per phase-02 SPEC §7:

- **ADR-0013** — YAML-native `typed_config` deserialization until the xDS/protos family lands (defers `envoy-protos` crate + `prost` / proto-tree vendoring).
- **ADR-0014** — Cross-container host reachability via `host.docker.internal` + `host-gateway` (for the fixture-0003 in-tree backend).
- **ADR-0015** — Phase-02 TCP proxy runs with Envoy's default `enable_half_close: false`.

Additional ADRs may be required during execution per D-3.5 if upstream Envoy v1.33.0's admin schema rejects `port_value: 0`, if `cargo deny check` flips on new transitive surface, or if `ubuntu-latest`'s Docker refuses `host-gateway` (all considered unlikely; verified at execution time).

### Doctrine reminders

- Any deviation from the state machine requires `superpowers:systematic-debugging` before proceeding — see §1 Step E of `BOOTSTRAP_PROMPT.md`.
- Consult `docs/envoy-rust/SKILL_ROUTING.md` for the full phase lifecycle state machine.
- `BOOTSTRAP_PROMPT.md` §5.1: one state per session; do not chain states. The sole exception was §10's first-session bootstrap, which is unavailable to every subsequent session.
