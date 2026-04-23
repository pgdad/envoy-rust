# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `00`
**slug:** `00-bootstrap`
**directory:** `docs/envoy-rust/phases/00-bootstrap/`
**status:** `in-progress` (lifecycle state 5 — verified, code review pending)

## Next expected skill

`superpowers:requesting-code-review` — scoped to phase 00. Produces
`docs/envoy-rust/phases/00-bootstrap/REVIEW.md` per state 5 of the lifecycle
(`docs/envoy-rust/SKILL_ROUTING.md`). If review surfaces issues the session
loops back to state 3 per the state machine (NOT state 4).

State 4 (verification) is complete: all five phase-done gate commands
(`cargo build / clippy / fmt --check / test --workspace / deny check`)
exited 0 on `ubuntu-latest` CI for commit `5355311` (workflow run
`24856364702`, URL
https://github.com/pgdad/envoy-rust/actions/runs/24856364702),
including the Docker-gated `echo_fixture` acceptance test which ran the
full differential end-to-end against `envoyproxy/envoy:v1.33.0`. Quoted
step outputs live in `phases/00-bootstrap/PROGRESS.md` under the
"State 4 — Phase-done gate verification" section.

## Last commit

`phase 00: drive_tcp read_exact fix for upstream Envoy [ADR-0006]` (`5355311`)
— lands ADR-0006 and rewrites the harness's `drive_tcp` to match Envoy
v1.33.0's echo-filter contract (read exactly `payload.len()` bytes
instead of half-close + `read_to_end`). Supersedes SPEC §D4 point 5's
wording for the harness helper only; no fixture YAML or `envoy-bin`
change. Unblocks `echo_fixture` on CI.

## Last updated

2026-04-23

## Notes

- SPEC.md landed 2026-04-23; PLAN.md landed 2026-04-23; implementation (state 3) completed 2026-04-23 via `superpowers:subagent-driven-development`; state 4 verification completed 2026-04-23 after one iteration (initial CI failure on `echo_fixture` → `superpowers:systematic-debugging` → ADR-0006 + harness fix → CI green).
- Per `BOOTSTRAP_PROMPT.md` §5.1, sessions move exactly one state forward. This session advanced state 4 → state 5 (verified) and now exits; the next session picks up at state 5 and runs `superpowers:requesting-code-review`.
- Deviations captured during state 3 (see `docs/envoy-rust/phases/00-bootstrap/PROGRESS.md` for full detail):
  - **ADR-0005 landed** — `skip-tree` in cargo-deny 0.19.4 only affects the `multiple-versions` check, not `[bans] deny`. Corrected the plan's Task 4 Step 6 by using `wrappers` on `hyper`/`hyper-util`/`tower-service` plus `[advisories].ignore` for RUSTSEC-2025-0111 (tokio-tar) and RUSTSEC-2025-0134 (rustls-pemfile).
  - Local Docker daemon DNS/IPv6 routing bug: Task 3 resolved the `envoyproxy/envoy:v1.33.0` digest via the Docker Hub public API; Task 10's `#[ignore]`d integration test and Task 14's acceptance test were NOT run locally and relied on CI for validation.
  - Task 5 clippy gate: `envoy-bin` is binary-only (no `[lib]`); ran via `--bin envoy-bin`. Crate-root `#![allow(dead_code)]` carried across Tasks 5–7 and removed in Task 8.
  - Task 7 tokio features: added `"time"` and `"sync"` to envoy-bin's tokio feature list to make `echo::tests` compile.
  - Task 13 import order: hoisted `use` statements and moved `drive_tcp`/`run_fixture` above the test module per clippy's `items-after-test-module` lint.
- Deviation captured during state 4 (see the new "State 4" section of PROGRESS.md):
  - **ADR-0006 landed** — upstream Envoy v1.33.0's default `ConnectionImpl` drops the echo filter's queued write when the client half-closes before reading (traced to `source/common/network/connection_impl.cc` at ref `v1.33.0`, lines 698–715). There is no listener-level YAML surface to enable half-close in v1.33.0. The harness's `drive_tcp` was rewritten to match Envoy's echo-filter 1:1 byte contract (`read_exact(payload.len())` then graceful close), superseding SPEC §D4 point 5's wording for this helper. Commit `5355311`.
- The plan's final commit (state 6) carries the full phase title and ADR list per BOOTSTRAP_PROMPT §5.3. With ADR-0005 and ADR-0006 both landed, the bracketed list for that commit is `[ADR-0002, ADR-0003, ADR-0004, ADR-0005, ADR-0006]`.
- Consult `docs/envoy-rust/SKILL_ROUTING.md` for the full phase lifecycle state machine.
- Any deviation from the state machine requires `superpowers:systematic-debugging` before proceeding — see §1 Step E of `BOOTSTRAP_PROMPT.md`.
