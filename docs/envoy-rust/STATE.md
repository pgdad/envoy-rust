# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `00`
**slug:** `00-bootstrap`
**directory:** `docs/envoy-rust/phases/00-bootstrap/`
**status:** `in-progress` (state 4 re-verification green on CI; next session enters state 5 re-review)

## Next expected skill

`superpowers:requesting-code-review` — re-review pass (state 5) scoped
to phase 00 against HEAD `a1c8194`, with the three Important items and
the folded-in M3 from the prior REVIEW.md all resolved. Produce an
updated REVIEW.md (supersede or append per the file's own conventions).
If the verdict is **Approved**, the following session advances to
state 6 (final commit per BOOTSTRAP_PROMPT §5.3 + ROADMAP row → done).
If any new Important/Critical issues surface, route back to state 3 per
`SKILL_ROUTING.md` line 42.

State 4 re-verification closed in this session against CI run
`24859537419` (HEAD `a1c81942292559246805029744083ff1605f1c2f`,
`ubuntu-latest`, total job time **1m 3s**, conclusion **success**). All
five phase-done gate commands exited 0 with zero regressions vs. the
Attempt 2 baseline (`24856364702`, HEAD `5355311`):

- `cargo fmt --all -- --check` → success (no diff).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` → success; cache-warm Finished in 3.78s.
- `cargo build --workspace --all-targets` → success; cache-warm Finished in 8.93s.
- `cargo test --workspace` → success; **28 passed, 0 failed, 1 ignored** (the ignored is the docker-gated `upstream::tests::starts_upstream_envoy_and_exposes_host_port`). `echo_fixture` passed in 8.01s. All three new loop-back regression tests (`drive_tcp_rejects_trailing_bytes_after_echo`, the tightened `rejects_duplicate_config_flag`, and the four M3 `deny_unknown_fields` tests) all pass.
- `cargo deny check` → success; tail `advisories ok, bans ok, licenses ok, sources ok`. Seven informational warnings carried forward unchanged from prior runs (1× duplicate `wit-bindgen`, 6× unmatched-license allowances) — not failures.

Pushed commits `e1771c3..a1c8194` to `origin/main` to fire CI; the new
tip `a1c8194` is documentation-only (`STATE.md` + `PROGRESS.md`) so the
code under verification equals the review-target HEAD `fca3aba`. Full
step-by-step CI evidence is quoted in the new "State 4 —
Re-verification" section of `phases/00-bootstrap/PROGRESS.md`.

State 3 loop-back detail (four atomic commits with per-task
spec-compliance + code-quality reviews via
`superpowers:subagent-driven-development`) remains in the earlier
"State 3 (loop-back) — REVIEW fixes" section of PROGRESS.md:

- **I1** → `245a65f` `phase 00: drive_tcp trailing-byte check [ADR-0007]`
  — trailing-byte poll (100ms deadline) after `read_exact`, regression
  test `drive_tcp_rejects_trailing_bytes_after_echo`, **ADR-0007**
  landed append-only in `DECISIONS.md` cross-referencing ADR-0006. The
  phase-00 final-commit bracketed ADR list is
  `[ADR-0002, ADR-0003, ADR-0004, ADR-0005, ADR-0006, ADR-0007]`.
- **I2** → `ba17ee3` `phase 00: assert! wrap on rejects_duplicate_config_flag`
  — `matches!(...)` wrapped in `assert!(...)` so the test actually
  asserts the `ArgvError::Trailing(_)` variant.
- **I3** (rustdoc portion) → `18bbfde`
  `phase 00: Subject rustdoc — SIGKILL, not SIGTERM` — struct-level doc
  corrected, `TODO(phase-01)` block added citing the `nix`-crate blocker
  for the functional switch. No behavior change.
- **M3** (folded in) → `fca3aba`
  `phase 00: deny_unknown_fields on YAML schemas` — attribute on 9
  YAML-parsed structs plus four root/nested regression tests.
  `BodyRule` correctly skipped (unit-variant enum).

## Last commit

`phase 00: state 3 loop-back close — PROGRESS entries + STATE advance`
(`a1c8194`) — documentation-only: appended the "State 3 (loop-back) —
REVIEW fixes" section to PROGRESS.md summarising the four loop-back fix
commits and advanced STATE.md to route the next session into state 4.
The code under state 4 re-verification is therefore identical to HEAD
`fca3aba`.

## Last updated

2026-04-23 (state 4 re-verification CI-green; routing advanced to state 5 re-review)

## Notes

- SPEC.md landed 2026-04-23; PLAN.md landed 2026-04-23; implementation (state 3) completed 2026-04-23 via `superpowers:subagent-driven-development`; state 4 verification completed 2026-04-23 after one iteration (initial CI failure on `echo_fixture` → `superpowers:systematic-debugging` → ADR-0006 + harness fix → CI green); state 5 review 2026-04-23 returned Approved-with-fixes (3 Important + 8 Minor); state 3 loop-back closed 2026-04-23 (four atomic fix commits with per-task spec + code-quality review); **state 4 re-verification 2026-04-23 green on CI run `24859537419` with zero regressions**.
- Per `BOOTSTRAP_PROMPT.md` §5.1, sessions move exactly one state forward. This session ran state 4 re-verification and now exits; the next session enters state 5 (`superpowers:requesting-code-review`) for the re-review pass against HEAD `a1c8194`. If that review returns Approved, the following session advances to state 6 (final phase commit + ROADMAP row → done).
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
