# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `00`
**slug:** `00-bootstrap`
**directory:** `docs/envoy-rust/phases/00-bootstrap/`
**status:** `in-progress` (lifecycle state 4 — implementation complete, verification pending)

## Next expected skill

`superpowers:verification-before-completion` — scoped to phase 00. Runs the §7.5 phase-done gate end-to-end and quotes the command outputs into `docs/envoy-rust/phases/00-bootstrap/PROGRESS.md` per state 4 of the lifecycle (§5).

The gate for phase 00 is:

```
cargo build   --workspace --all-targets
cargo clippy  --workspace --all-targets --all-features -- -D warnings
cargo fmt     --all -- --check
cargo test    --workspace
cargo deny    check
```

All must exit 0. `cargo test --workspace` is the critical item — it runs
`tests/differential/tests/echo.rs::echo_fixture`, which pulls
`envoyproxy/envoy:v1.33.0` (pinned per ADR-0004) and drives the fixture at
both upstream Envoy (via testcontainers) and envoy-rust (subprocess). That
test cannot be run on this dev host (Docker daemon has an IPv6 routing bug
documented in the PROGRESS.md Task 3 deviation); CI on `ubuntu-latest` runs
it per `.github/workflows/ci.yml`.

No fuzz targets ship in phase 00 (no parsers land until phase 01), and no
conformance suites (first is `h2spec` at phase 05). The gate is therefore the
five cargo commands above, plus the green CI run (§7.5.a via CI).

## Last commit

`phase 00: task 15 progress entry` (`71ae8c8`) — closes out Task 15 of the plan. The plan's 15 tasks, ADR-0005, and their PROGRESS.md entries are all landed on `main`; 31 commits were made during state 3. Test count at the end of state 3: envoy-bin 13 passed (5 config + 6 argv + 2 echo), differential lib 8 passed (6 helpers + 2 subject) + 1 ignored (container-start, Docker-gated). `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok`.

## Last updated

2026-04-23

## Notes

- SPEC.md landed in `phase 00: spec brainstormed` (2026-04-23). PLAN.md landed in `phase 00: plan drafted` (2026-04-23). Implementation (state 3) completed 2026-04-23 via `superpowers:subagent-driven-development`.
- Per `BOOTSTRAP_PROMPT.md` §5.1, sessions move exactly one state forward. This session advanced state 3 → state 4 and now exits; the next session picks up at state 4 and runs the verification gate.
- Deviations captured during state 3 (see `docs/envoy-rust/phases/00-bootstrap/PROGRESS.md` for full detail):
  - **ADR-0005 landed (new)** — `skip-tree` in cargo-deny 0.19.4 only affects the `multiple-versions` check, not `[bans] deny`. Corrected the plan's Task 4 Step 6 by using `wrappers` on `hyper`/`hyper-util`/`tower-service` plus `[advisories].ignore` for RUSTSEC-2025-0111 (tokio-tar) and RUSTSEC-2025-0134 (rustls-pemfile). Final commit message for state 6 must therefore reference `[ADR-0002, ADR-0003, ADR-0004, ADR-0005]`.
  - Local Docker daemon IPv6 routing bug: Task 3 resolved the `envoyproxy/envoy:v1.33.0` digest via the Docker Hub public API (canonical multi-arch index digest); Task 10's `#[ignore]`d integration test and Task 14's acceptance test were NOT run locally and will be validated by CI.
  - Task 5 clippy gate: `envoy-bin` is a binary-only crate (no `[lib]`); plan used `--lib` selector. Ran via `--bin envoy-bin` with same test-name selector. Also, pub items defined in Tasks 5–7 but not consumed until Task 8 fired `dead_code` under `-D warnings`; a crate-root `#![allow(dead_code)]` with a removal note was carried across Tasks 5–7 and removed in Task 8.
  - Task 7 tokio features: plan's Cargo.toml for envoy-bin omitted the `time` and `sync` features required by `echo::tests`; added them in the Task 7 commit.
  - Task 13 import order: `-D warnings` rejects items after `#[cfg(test)] mod tests`; hoisted `use` statements and moved `drive_tcp`/`run_fixture` above the tests module per clippy's `items-after-test-module` lint.
- The plan's final commit (state 6) carries the full phase title and ADR list per §5.3. With ADR-0005 now landed, the bracketed list for that commit is `[ADR-0002, ADR-0003, ADR-0004, ADR-0005]`.
- Consult `docs/envoy-rust/SKILL_ROUTING.md` for the full phase lifecycle state machine.
- Any deviation from the state machine requires `superpowers:systematic-debugging` before proceeding — see §1 Step E of `BOOTSTRAP_PROMPT.md`.
