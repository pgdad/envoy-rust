# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `01`
**slug:** `01-static-bootstrap-config`
**directory:** `docs/envoy-rust/phases/01-static-bootstrap-config/` (exists; contains `SPEC.md`, `PLAN.md`, `PROGRESS.md`)
**status:** phase 01 lifecycle **state 4** — implementation complete and verified; REVIEW pending.

All 19 PLAN tasks committed. State-4 phase-done gate passed (CI run
24891070573, HEAD `20ffb5bf52a59bcc3f00e636281fbfcfd321b307`, both `build`
and `fuzz` jobs green; all 5 local stable-toolchain commands exit 0). See
`PROGRESS.md` State-4 section for full details.

Phase 00 (`00-bootstrap`) is **done** as of commit `e5afc35`.

## Next expected skill

**`superpowers:requesting-code-review`** — the next session opens a code
review for phase 01 per `SKILL_ROUTING.md` state 5. The reviewer should
read `docs/envoy-rust/phases/01-static-bootstrap-config/SPEC.md` and
`PROGRESS.md` (especially the State-4 section) as context.

## Last commit

State-4 phase-done gate:
`phase 01: state 4 — phase-done gate verified` (commit `7768d01`).
Appends State-4 section to `PROGRESS.md`. HEAD after STATE advance will
be the STATE.md commit.

## Last updated

2026-04-24 (state 4 complete — phase-done gate verified; STATE advanced
to state 4; next session enters state 5 via
`superpowers:requesting-code-review`).

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
  blocker (not on the D-3.2 permitted-foundations list).
- ADR-0012 (conditional — `cargo deny` `libfuzzer-sys` license advisory)
  was not needed; `cargo deny check` passed throughout all phase-01 tasks.
- Three CI-fix commits (`5b852ce`, `97c1576`, `20ffb5b`) landed during
  the state-4 gate to resolve: (1) `drive_http_get` chunked-encoding
  blind spot exposed by upstream Envoy v1.33.0's `/ready` response, and
  (2) cargo-fuzz toolchain-override interaction with the workspace-root
  `rust-toolchain.toml`. Both root causes are documented in `PROGRESS.md`
  State-4 section.
- Any deviation from the state machine requires
  `superpowers:systematic-debugging` before proceeding — see §1 Step E
  of `BOOTSTRAP_PROMPT.md`.
- Consult `docs/envoy-rust/SKILL_ROUTING.md` for the full phase
  lifecycle state machine.
