# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `00`
**slug:** `00-bootstrap`
**directory:** `docs/envoy-rust/phases/00-bootstrap/`
**status:** `in-progress` (state 5 re-review Approved; next session executes state 6 — final phase commit + ROADMAP row flip → `done`)

## Next expected skill

No skill invocation. Per the phase lifecycle state machine (see
`SKILL_ROUTING.md` lines 44–48), state 6 is a bookkeeping step —
**final commit + ROADMAP update + STATE advance** — not a skill call.
The next session must:

1. Read this file, then `docs/envoy-rust/phases/00-bootstrap/REVIEW.md`
   (verdict "Approved with fixes (new Minor only) — advance to state 6").
2. **Optionally close N1 first** (non-blocking): append a one-line
   PROGRESS.md entry noting SPEC §D3.2's "Any field not covered here
   … is ignored" is superseded by the landed `deny_unknown_fields`
   behavior from commit `fca3aba`. Alternatively land ADR-0008
   documenting the tightening. Either is optional; neither blocks
   phase-done. N2 (regression-test coverage gap on 5 of 9 attribute
   sites) is deferred to a later Minor-cleanup commit.
3. Make the **phase-00 final commit** per `BOOTSTRAP_PROMPT.md` §5.3
   with the format below. The state-5 documentation updates
   (REVIEW.md supersession, State 5 re-review PROGRESS.md section,
   this STATE.md) are already committed in the state-5-exit doc-only
   commit preceding state 6, so the state-6 final commit only needs to
   include the ROADMAP row flip, the state-6 STATE.md advance to phase
   01, and — if chosen — the optional N1 closure note (either a brief
   PROGRESS.md entry or ADR-0008):

   ```
   phase 00: bootstrap differential harness + envoy-bin TCP echo [ADR-0002, ADR-0003, ADR-0004, ADR-0005, ADR-0006, ADR-0007]

   <1–3 sentence summary of phase 00 scope: pinned upstream Envoy
   v1.33.0, differential harness + first fixture 0001-tcp-echo,
   envoy-bin scaffold passing byte-exact equivalence with loop-back
   fixes for trailing-byte detection and unknown-field rejection.>

   Differential surface: tests/fixtures/0001-tcp-echo (green)
   Conformance: n/a — no conformance suites for phase 00
   ```

4. Flip the ROADMAP.md phase-00 row from `in-progress` to `done`.
5. Advance STATE.md to phase 01 (or "awaiting next planning" if the
   roadmap pointer is to be re-derived from ROADMAP.md).
6. Exit. The next fresh session enters phase 01 at lifecycle state 0
   (or state 1 if a row exists but no directory).

## State 5 re-review — Approved (2026-04-23)

Re-review pass ran this session against range `b42f18d..a1c8194`
(code-relevant HEAD `fca3aba`; the trailing documentation-only commits
`a1c8194` and `880efcd` update STATE.md / PROGRESS.md without touching
code). The `superpowers:code-reviewer` subagent superseded the prior
REVIEW.md in place and returned verdict **Approved with fixes (new
Minor only) — advance to state 6**.

All three prior-REVIEW Important items resolved:

- **I1** — `drive_tcp` trailing-byte blind spot → **Fixed** by
  `245a65f` + ADR-0007 (100ms poll after `read_exact`, regression test
  `drive_tcp_rejects_trailing_bytes_after_echo` structurally reproduces
  the silent-pass, ADR-0006 append-only-preserved — git-diff verified).
- **I2** — `rejects_duplicate_config_flag` discarded assertion →
  **Fixed** by `ba17ee3` (`assert!(matches!(…), …)` wrapper asserts the
  `ArgvError::Trailing(_)` variant; test now fails on any other variant).
- **I3** — `Subject::shutdown` rustdoc mismatch (rustdoc portion) →
  **Fixed** by `18bbfde` (struct doc accurately states SIGKILL;
  `TODO(phase-01)` block names `nix`-crate D-3.2 blocker and phase-01
  ADR gating). Functional SIGKILL→SIGTERM switch deferred per prior
  REVIEW's own recommendation.
- **M3** (folded in) — `deny_unknown_fields` on YAML schemas →
  **Fixed** by `fca3aba` (attribute on all 9 documented sites; four
  TDD-verified regression tests assert against serde-canonical
  `"unknown field"` marker).

Two new Minors surfaced in re-review (neither blocking):

- **N1** — `deny_unknown_fields` tightens SPEC §D3.2's "ignored"
  language to hard-reject without a dedicated ADR or PROGRESS.md note.
  Close with a one-line PROGRESS.md entry at state 6 or a brief
  ADR-0008. Strictly safer + prior-REVIEW-recommended.
- **N2** — regression tests cover only 4 of 9 `deny_unknown_fields`
  attribute sites. Self-identified in the State 3 loop-back. Batch
  into a future Minor-cleanup commit.

Seven pre-existing Minors (M1, M2, M4, M5, M6, M7, M8) remain open
with prior classification. All were "nice to have; can defer" and
none were in scope for the loop-back. Carried forward.

## Last commit

`phase 00: state 4 re-verification — CI green, routing to state 5 re-review`
(`880efcd`) — documentation-only: appended the "State 4 —
Re-verification" section to PROGRESS.md with CI-run evidence and
advanced STATE.md to route the next session into state 5. No code
changes.

## Last updated

2026-04-23 (state 5 re-review Approved; routing advanced to state 6 final commit)

## Notes

- SPEC.md landed 2026-04-23; PLAN.md landed 2026-04-23; implementation (state 3) completed 2026-04-23 via `superpowers:subagent-driven-development`; state 4 verification completed 2026-04-23 after one iteration (initial CI failure on `echo_fixture` → `superpowers:systematic-debugging` → ADR-0006 + harness fix → CI green); state 5 review 2026-04-23 returned Approved-with-fixes (3 Important + 8 Minor); state 3 loop-back closed 2026-04-23 (four atomic fix commits with per-task spec + code-quality review); state 4 re-verification 2026-04-23 green on CI run `24859537419` with zero regressions; **state 5 re-review 2026-04-23 Approved with new Minors only — phase 00 ready for state 6 final commit**.
- Per `BOOTSTRAP_PROMPT.md` §5.1, sessions move exactly one state forward. This session ran state 5 re-review and now exits; the next session executes state 6 per §5.3 (final phase commit + ROADMAP row flip → `done` + STATE advance to phase 01).
- Phase-00 final-commit bracketed ADR list is `[ADR-0002, ADR-0003, ADR-0004, ADR-0005, ADR-0006, ADR-0007]`. ADR-0005 (cargo-deny wrappers), ADR-0006 (`drive_tcp` rewrite), and ADR-0007 (`drive_tcp` trailing-byte poll) all landed in-phase per D-3.5.
- Deviations captured during state 3 (see `docs/envoy-rust/phases/00-bootstrap/PROGRESS.md` for full detail):
  - **ADR-0005 landed** — `skip-tree` in cargo-deny 0.19.4 only affects the `multiple-versions` check, not `[bans] deny`. Corrected the plan's Task 4 Step 6 by using `wrappers` on `hyper`/`hyper-util`/`tower-service` plus `[advisories].ignore` for RUSTSEC-2025-0111 (tokio-tar) and RUSTSEC-2025-0134 (rustls-pemfile).
  - Local Docker daemon DNS/IPv6 routing bug: Task 3 resolved the `envoyproxy/envoy:v1.33.0` digest via the Docker Hub public API; Task 10's `#[ignore]`d integration test and Task 14's acceptance test were NOT run locally and relied on CI for validation.
  - Task 5 clippy gate: `envoy-bin` is binary-only (no `[lib]`); ran via `--bin envoy-bin`. Crate-root `#![allow(dead_code)]` carried across Tasks 5–7 and removed in Task 8.
  - Task 7 tokio features: added `"time"` and `"sync"` to envoy-bin's tokio feature list to make `echo::tests` compile.
  - Task 13 import order: hoisted `use` statements and moved `drive_tcp`/`run_fixture` above the test module per clippy's `items-after-test-module` lint.
- Deviation captured during state 4 (see the "State 4" section of PROGRESS.md):
  - **ADR-0006 landed** — upstream Envoy v1.33.0's default `ConnectionImpl` drops the echo filter's queued write when the client half-closes before reading (traced to `source/common/network/connection_impl.cc` at ref `v1.33.0`, lines 698–715). There is no listener-level YAML surface to enable half-close in v1.33.0. The harness's `drive_tcp` was rewritten to match Envoy's echo-filter 1:1 byte contract (`read_exact(payload.len())` then graceful close), superseding SPEC §D4 point 5's wording for this helper. Commit `5355311`.
- Loop-back deviation (state 3): ADR-0007 landed append-only in `DECISIONS.md` to close REVIEW.md I1's trailing-byte blind-spot. Per DECISIONS.md preamble line 7 ("landed ADRs are never edited") ADR-0006 text is unchanged; ADR-0007 cross-references it as the source of the blind-spot it mitigates.
- Re-review deviation candidate (state 5, this session): **N1** — `deny_unknown_fields` in commit `fca3aba` tightens SPEC §D3.2's "Any field not covered here … is ignored" clause to hard-reject without a dedicated ADR or PROGRESS.md SPEC-deviation note. REVIEW classifies this Minor and non-blocking; state 6 may close it with a one-line PROGRESS.md entry or ADR-0008. Either is optional.
- Consult `docs/envoy-rust/SKILL_ROUTING.md` for the full phase lifecycle state machine.
- Any deviation from the state machine requires `superpowers:systematic-debugging` before proceeding — see §1 Step E of `BOOTSTRAP_PROMPT.md`.
