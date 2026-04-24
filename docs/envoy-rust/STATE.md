# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `01`
**slug:** `01-static-bootstrap-config`
**directory:** `docs/envoy-rust/phases/01-static-bootstrap-config/` (exists; contains `SPEC.md`, `PLAN.md`, `PROGRESS.md`, `REVIEW.md`)
**status:** phase 01 lifecycle **state 6 (pending)** — reviewed and approved; final phase-done commit next (flips `ROADMAP.md` row 01 → `done` and advances this file to phase 02).

State-5 re-review verdict: **I1 Closed — no new issues.** ADR-0012 lands
the nested nightly pin in `crates/envoy-config/fuzz/rust-toolchain.toml`
on the record (narrowly supersedes ADR-0010). CI re-verification run
24893585436 green on HEAD `e32240c`. `REVIEW.md` front-matter verdict
now reads **Approved**; full close-out section at §9 with check table.

Forward-tracked rollovers into phase 02: I3 (`decode_chunked` unit
tests), I4 (admin 8 KiB cap tightening), M1 (retarget stale
`TODO(phase-01)` in `tests/differential/src/subject.rs`).

Phase 00 (`00-bootstrap`) is **done** as of commit `e5afc35`.

## Next expected skill

**Final phase-done commit per SPEC §8** (message:
`phase 01: Static bootstrap config loader + admin /ready [ADR-0008, ADR-0009, ADR-0010, ADR-0011, ADR-0012]`).
That commit flips `docs/envoy-rust/ROADMAP.md` row 01 status to
`done`, rewrites this file for phase 02 (slug `02-tcp-proxy`, next
skill `superpowers:brainstorming`), and appends a final entry to
`PROGRESS.md`. After that commit lands, phase 01 is complete and the
next session enters phase 02 at lifecycle state 1.

## Last commit

State-5 re-review approved:
`phase 01: state 5 re-review Approved — REVIEW.md I1 close-out + STATE advance to state 6`.

## Last updated

2026-04-24 (state 5 re-review approved; ADR-0012 closes I1; STATE
advanced to state 6 pending the final phase-done commit).

## Notes

- Pre-existing phase-00 deferred Minors M1, M2, M4, M5, M6, M7, M8
  remain open; none blocked phase 01. See
  `docs/envoy-rust/phases/00-bootstrap/REVIEW.md`.
- N2 (phase-00 deferred Minor — `deny_unknown_fields` regression-test
  gap on `StaticResources`, `Address`, `SocketAddress`, `FilterChain`,
  `NetworkFilter`) was closed by `PLAN.md` Task 4 Step 4 via five new
  regression tests (`rejects_unknown_static_resources_field`, etc.).
  No remaining phase-00 carryover on this front.
- The phase-00 I3 SIGKILL→SIGTERM functional switch remains deferred.
  Phase-01 SPEC did not pick it up; the `nix` crate remains the stated
  blocker (not on the D-3.2 permitted-foundations list). Phase-01
  review M1 (retarget stale `TODO(phase-01)` in
  `tests/differential/src/subject.rs`) is tracked forward to phase 02
  as part of clearing this breadcrumb.
- ADR-0012 (nested nightly pin in fuzz subcrate; narrowly supersedes
  ADR-0010) landed during state-5 remediation (commit `bda4e52`) to
  close REVIEW.md §Issues/Important I1. This slot had been named
  informally in a prior STATE.md note for a conditional
  "`cargo deny` `libfuzzer-sys` license advisory" ADR that was never
  needed; the actual ADR-0012 in `DECISIONS.md` is the nested-pin ADR.
- Phase-01 state-4 CI-fix commits (`5b852ce`, `97c1576`, `20ffb5b`):
  (1) `drive_http_get` chunked-encoding blind spot exposed by upstream
  Envoy v1.33.0's `/ready` response; (2) cargo-fuzz toolchain-override
  interaction with the workspace-root `rust-toolchain.toml`. Both root
  causes are documented in `PROGRESS.md` State-4 section; ADR-0012
  formally legitimates the nested-pin portion of fix (2).
- Phase-02 starter items (carry forward from phase-01 REVIEW.md §9):
  - I3 — add 4 unit tests for `decode_chunked` in `tests/differential/src/lib.rs` (empty, extension, truncated, trailer).
  - I4 — tighten admin 8 KiB header cap in `crates/envoy-bin/src/admin.rs` to an exact boundary (currently effectively ~9 KiB; ~2-line fix).
  - M1 — retarget stale `TODO(phase-01)` in `tests/differential/src/subject.rs:25–32` (phase-00 I3 deferral now targets phase 04 or later, since phase 01 did not pick it up).
- Any deviation from the state machine requires
  `superpowers:systematic-debugging` before proceeding — see §1 Step E
  of `BOOTSTRAP_PROMPT.md`.
- Consult `docs/envoy-rust/SKILL_ROUTING.md` for the full phase
  lifecycle state machine.
