# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `00`
**slug:** `00-bootstrap`
**directory:** `docs/envoy-rust/phases/00-bootstrap/`
**status:** `in-progress` (state 3 loop-back fixes landed; next session re-enters state 4)

## Next expected skill

`superpowers:verification-before-completion` — re-run the five
phase-done gate commands on CI (`ubuntu-latest`) for HEAD `fca3aba`,
quote outputs into a new "State 4 — Re-verification" section of
`phases/00-bootstrap/PROGRESS.md`, then advance STATE to state 5 for the
re-review pass. REVIEW.md's final recommendation expects **Approved**
on this round (all three Important items resolved, plus M3 folded in).

State 3 loop-back completed in this session via
`superpowers:subagent-driven-development` with four atomic commits,
each paired with a spec-compliance review and a code-quality review
pass before moving to the next task. Per-task detail lives in
`phases/00-bootstrap/PROGRESS.md` under "State 3 (loop-back) — REVIEW
fixes":

- **I1** → `245a65f` `phase 00: drive_tcp trailing-byte check [ADR-0007]`
  — trailing-byte poll (100ms deadline) after `read_exact`, new
  regression test `drive_tcp_rejects_trailing_bytes_after_echo`, and
  **ADR-0007** landed append-only in `DECISIONS.md` cross-referencing
  ADR-0006. Phase-00 final-commit bracketed ADR list now extends to
  `[ADR-0002, ADR-0003, ADR-0004, ADR-0005, ADR-0006, ADR-0007]`.
- **I2** → `ba17ee3` `phase 00: assert! wrap on rejects_duplicate_config_flag`
  — `matches!(...)` wrapped in `assert!(...)` so the test actually
  asserts the `ArgvError::Trailing(_)` variant.
- **I3** (rustdoc portion) → `18bbfde`
  `phase 00: Subject rustdoc — SIGKILL, not SIGTERM` — struct-level doc
  corrected, `TODO(phase-01)` block added citing the `nix`-crate
  blocker for the functional switch. No behavior change.
- **M3** (folded in) → `fca3aba`
  `phase 00: deny_unknown_fields on YAML schemas` — attribute on 9
  YAML-parsed structs plus four root/nested regression tests.
  `BodyRule` correctly skipped (unit-variant enum).

Verification gate ran locally green before the loop-back closed
(`cargo build / clippy -D warnings / fmt --check / test --workspace /
deny check` all exit 0; 28 tests passed + 1 docker-gated ignored).
The `echo_fixture` acceptance test passed locally this session (Docker
was cooperative); CI on `ubuntu-latest` remains the authoritative
validator per the Task 3 local-Docker caveat, which is the purpose of
the next session's state 4 pass.

State 4 (verification) on HEAD `5355311` remains complete from the
prior iteration: all five phase-done gate commands exited 0 on
`ubuntu-latest` CI (workflow run `24856364702`, URL
https://github.com/pgdad/envoy-rust/actions/runs/24856364702). The next
session's state 4 re-pass targets the new HEAD (`fca3aba`) to confirm
no regressions from the four loop-back commits.

## Last commit

`phase 00: deny_unknown_fields on YAML schemas` (`fca3aba`) — closes
the state-3 loop-back. Four commits (`245a65f`, `ba17ee3`, `18bbfde`,
`fca3aba`) land ADR-0007 plus fixes for REVIEW.md I1, I2, I3 (rustdoc),
and M3. This commit itself adds `#[serde(deny_unknown_fields)]` to all
9 YAML-parsed structs and four root/nested regression tests.

## Last updated

2026-04-23 (state 3 loop-back close)

## Notes

- SPEC.md landed 2026-04-23; PLAN.md landed 2026-04-23; implementation (state 3) completed 2026-04-23 via `superpowers:subagent-driven-development`; state 4 verification completed 2026-04-23 after one iteration (initial CI failure on `echo_fixture` → `superpowers:systematic-debugging` → ADR-0006 + harness fix → CI green).
- Per `BOOTSTRAP_PROMPT.md` §5.1, sessions move exactly one state forward. Prior sessions ran state 4 (verified) → state 5 (REVIEW.md: Approved with fixes; 3 Important + 8 Minor). Per `SKILL_ROUTING.md` line 42, state 5 with issues routed the next session back to state 3. This session ran that state 3 loop-back (four atomic fix commits with spec + code-quality review per task) and now exits. The next session re-enters state 4 (`superpowers:verification-before-completion`) against the new HEAD `fca3aba`.
- Deviations captured during state 3 (see `docs/envoy-rust/phases/00-bootstrap/PROGRESS.md` for full detail):
  - **ADR-0005 landed** — `skip-tree` in cargo-deny 0.19.4 only affects the `multiple-versions` check, not `[bans] deny`. Corrected the plan's Task 4 Step 6 by using `wrappers` on `hyper`/`hyper-util`/`tower-service` plus `[advisories].ignore` for RUSTSEC-2025-0111 (tokio-tar) and RUSTSEC-2025-0134 (rustls-pemfile).
  - Local Docker daemon DNS/IPv6 routing bug: Task 3 resolved the `envoyproxy/envoy:v1.33.0` digest via the Docker Hub public API; Task 10's `#[ignore]`d integration test and Task 14's acceptance test were NOT run locally and relied on CI for validation.
  - Task 5 clippy gate: `envoy-bin` is binary-only (no `[lib]`); ran via `--bin envoy-bin`. Crate-root `#![allow(dead_code)]` carried across Tasks 5–7 and removed in Task 8.
  - Task 7 tokio features: added `"time"` and `"sync"` to envoy-bin's tokio feature list to make `echo::tests` compile.
  - Task 13 import order: hoisted `use` statements and moved `drive_tcp`/`run_fixture` above the test module per clippy's `items-after-test-module` lint.
- Deviation captured during state 4 (see the new "State 4" section of PROGRESS.md):
  - **ADR-0006 landed** — upstream Envoy v1.33.0's default `ConnectionImpl` drops the echo filter's queued write when the client half-closes before reading (traced to `source/common/network/connection_impl.cc` at ref `v1.33.0`, lines 698–715). There is no listener-level YAML surface to enable half-close in v1.33.0. The harness's `drive_tcp` was rewritten to match Envoy's echo-filter 1:1 byte contract (`read_exact(payload.len())` then graceful close), superseding SPEC §D4 point 5's wording for this helper. Commit `5355311`.
- The plan's final commit (state 6) carries the full phase title and ADR list per BOOTSTRAP_PROMPT §5.3. With ADR-0005, ADR-0006, and ADR-0007 all landed, the bracketed list for that commit is `[ADR-0002, ADR-0003, ADR-0004, ADR-0005, ADR-0006, ADR-0007]`.
- Loop-back deviation (state 3, this session): ADR-0007 landed append-only in `DECISIONS.md` to close REVIEW.md I1's trailing-byte blind-spot. Per DECISIONS.md preamble line 7 ("landed ADRs are never edited") ADR-0006 text is unchanged; ADR-0007 cross-references it as the source of the blind-spot it mitigates.
- Consult `docs/envoy-rust/SKILL_ROUTING.md` for the full phase lifecycle state machine.
- Any deviation from the state machine requires `superpowers:systematic-debugging` before proceeding — see §1 Step E of `BOOTSTRAP_PROMPT.md`.
