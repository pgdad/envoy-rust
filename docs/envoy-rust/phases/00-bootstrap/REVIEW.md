# Phase 00 — Code Review

- **Reviewer:** superpowers:code-reviewer (subagent, state 5 re-review pass)
- **Range:** `b42f18d..a1c8194` (code-relevant HEAD: `fca3aba`; commits
  `a1c8194` and `880efcd` are documentation-only STATE.md / PROGRESS.md
  updates that do not affect the code under review).
- **Date:** 2026-04-23
- **Lifecycle state:** 5 (review, re-review pass after state-3 loop-back)
- **Verdict:** **Approved with fixes (new Minor only)** — advance to state 6.

This re-review supersedes the prior REVIEW.md (verdict "Approved with
fixes", loop-back to state 3) after the four atomic loop-back commits
landed and CI re-verified green (run `24859537419`, 1m 3s, HEAD
`a1c81942292559246805029744083ff1605f1c2f`). All three Important items
(I1, I2, I3 rustdoc portion) and the folded-in Minor (M3) are fixed.
Two new Minor observations (N1, N2) surfaced during re-review; neither
blocks phase-done commit. All seven previously-deferred Minors (M1, M2,
M4, M5, M6, M7, M8) remain open per prior REVIEW classification.

---

## Re-review summary

| ID | Prior severity | Fix commit | Outcome |
|---|---|---|---|
| I1 — `drive_tcp` trailing-byte blind spot | Important | `245a65f` (+ ADR-0007) | **Fixed.** 100ms `tokio::time::timeout` poll after `read_exact`; bails on any non-zero read. Regression test `drive_tcp_rejects_trailing_bytes_after_echo` asserts `drive_tcp` now returns `Err(...trailing bytes...)` against a server that writes echo + `b"EXTRA"`. Would have passed against the pre-fix code (confirmed by TDD per PROGRESS.md line 310). ADR-0007 appended; ADR-0006 text untouched (append-only verified by `git diff 5355311..fca3aba -- docs/envoy-rust/DECISIONS.md` — the only change is the 20-line ADR-0007 append). |
| I2 — `rejects_duplicate_config_flag` discarded assertion | Important | `ba17ee3` | **Fixed.** `matches!(err, ArgvError::Trailing(_));` (discarded expression) → `assert!(matches!(err, ArgvError::Trailing(_)), "got {err:?}");`. Test would now fail if `parse_argv` returned any other `ArgvError` variant (mentally walked through `NoConfigFlag` / `UnknownFlag` / `MissingValue`). Path-tightening (`Trailing(p) if p == "/b"`) deliberately skipped per prior REVIEW's "optional" labeling; rationale adequate in the commit body. |
| I3 — `Subject::shutdown` rustdoc mismatch | Important (rustdoc portion only; functional SIGTERM switch deferred) | `18bbfde` | **Fixed (rustdoc portion).** Struct-level doc now reads "sends SIGKILL (via tokio's `start_kill`) and waits for the process to exit"; a `TODO(phase-01)` block names the `nix`-crate blocker (not on D-3.2 permitted-foundations) and the phase-01 ADR gating. Functional SIGKILL→SIGTERM switch deliberately deferred per prior REVIEW's own recommendation — still accepted. |
| M3 — `deny_unknown_fields` on YAML schemas | Minor (folded in per prior REVIEW Recommendations) | `fca3aba` | **Fixed.** `#[serde(deny_unknown_fields)]` present on all 9 documented attribute sites: `Bootstrap`, `StaticResources`, `Listener`, `Address`, `SocketAddress`, `FilterChain`, `NetworkFilter` in `crates/envoy-bin/src/config.rs`; `Expectations`, `Equivalence` in `tests/differential/src/lib.rs`. `BodyRule` correctly skipped (unit-variant enum; unknown discriminants covered by pre-existing `expectations_reject_unknown_rule`). Four new regression tests present and assert against the serde-canonical `"unknown field"` marker. Coverage gap on 5 of the 9 sites noted as new Minor N2. |

All four loop-back commits are atomic, have scope-appropriate commit
messages, and the CI run they produced is green end-to-end (28 passed, 0
failed, 1 ignored — the Docker-gated test runs via the integration
`echo_fixture` path which itself passes).

---

## Strengths

(Prior Strengths retained; two new entries from the loop-back commits.)

- **Narrative discipline is exemplary.** Every deviation is written down
  (`docs/envoy-rust/phases/00-bootstrap/PROGRESS.md`). Each in-phase
  course correction (ADR-0005 for cargo-deny wrappers, ADR-0006 for the
  half-close fix, ADR-0007 for the trailing-byte poll) is grounded in
  concrete upstream evidence and enumerates rejected options with
  reasons, not just the chosen one. PROGRESS.md's "State 3 (loop-back)"
  and "State 4 — Re-verification" sections continue the State-4 template
  the prior review flagged as exemplary — concrete commit SHAs, quoted
  test-output lines, and CI URLs.
- **ADR-0006's root-cause analysis** is the kind of investigation the
  project wants to see from the harness on every future divergence —
  file and line numbers at the pinned Envoy tag, confirmation of the
  Envoy project's own client pattern in the echo integration test, and
  a D-3.2-grounded rejection of "patch upstream Envoy."
- **ADR-0007 (new) is a clean example of closing a reviewer-identified
  blind spot without editing an earlier ADR.** The ADR explicitly
  preserves ADR-0006 ("this ADR does not supersede ADR-0006; it lands
  on top of it") per MISSION.md D-3.5's append-only doctrine. Options
  (a)/(b)/(c) literally mirror the prior REVIEW.md's own three-option
  recommendation, making the decision trail self-verifying. The
  ~100ms per-connection idle-cost justification is quantified
  (~200ms per fixture, negligible vs. container start budget) and
  flagged as revisitable under a future ADR — not edited in place.
- **deny.toml `wrappers` lists** are mechanically correct against the
  actual `Cargo.lock` transitive graph.
- **`#![forbid(unsafe_code)]`** is present at both crate roots; submodules
  correctly do not repeat it.
- **D-3.2 permitted-foundations audit** of direct deps is clean. No new
  direct deps were added in the loop-back commits (`nix` was deliberately
  deferred to phase 01 — see I3 outcome).
- **TDD discipline on the loop-back commits.** Each of the four fixes
  landed with a failing-test-first pass per PROGRESS.md's "State 3
  (loop-back) — REVIEW fixes" section. The I1 regression test
  (`drive_tcp_rejects_trailing_bytes_after_echo`) and M3 regression
  tests (`expectations_reject_unknown_field`, `equivalence_reject_unknown_field`,
  `rejects_unknown_bootstrap_field`, `rejects_unknown_listener_field`)
  all structurally fail against the pre-fix code — verified by reading
  the pre-fix `drive_tcp` (no trailing-byte poll → pre-fix test would
  return `Ok(echoed)` rather than `Err`) and the pre-fix structs
  (no `deny_unknown_fields` → pre-fix test would deserialize into `Ok`).
- **Fixture layout is future-proof.** `tests/fixtures/0001-tcp-echo/`
  ships the five-file shape that the phase 02 TCP-proxy fixture will
  attach to without harness changes.
- **Concurrency hygiene in the subject's echo loop** — unchanged from
  the first pass.

---

## Issues

### Critical (must fix — blocks phase completion)

None. Nothing in the loop-back commits or the residual phase-00 code
violates a SPEC deliverable (modulo the documented `deny_unknown_fields`
tightening — see N1), breaks a MISSION.md doctrine, silently corrupts
the byte-exact differential assertion, or introduces a correctness bug
on the hot path.

### Important (should fix before phase completion)

None. All three prior Important items (I1, I2, I3 rustdoc portion)
resolved per the table above. The functional portion of I3 (SIGKILL →
SIGTERM + drain + SIGKILL-escalate) is deferred to phase 01 under its
own ADR, with a `TODO(phase-01)` block in the code pointing at the
`nix`-crate D-3.2 blocker. The deferral was recommended by the prior
REVIEW.md and is still accepted.

### Minor (nice to have; can defer)

#### N1 (new) — `deny_unknown_fields` tightens behavior beyond SPEC §D3.2's "ignored" language

- **Files:** `crates/envoy-bin/src/config.rs:1–48` (7 structs),
  `tests/differential/src/lib.rs:22–33` (2 structs).
- **Observation.** SPEC §2 D3.2 explicitly reads: *"Any field not
  covered here (including fields upstream Envoy requires at its schema
  level) is ignored by envoy-rust in phase 00."* The M3 fix flips this
  from silent-ignore to hard-reject. The phase-00 `envoy-rust.yaml`
  fixture is narrow enough that no functional regression occurs, but
  the SPEC wording was not updated and no ADR was landed for the
  tightening. The M3 commit message notes the rationale ("early-failure
  signal when unrecognized fields appear") but does not call out the
  SPEC deviation.
- **Why it is Minor, not Important.** The tightening is strictly safer
  for a scaffolding phase (reject-on-typo) and is explicitly
  recommended by the prior REVIEW.md's Recommendations section. SPEC is
  treated as an immutable historical artifact per the phase lifecycle;
  the on-disk behavior follows the landed ADRs and the code. However,
  there is no ADR for this particular deviation — M3 was folded in as
  Minor without its own ADR, and neither PROGRESS.md nor DECISIONS.md
  records the SPEC §D3.2 "ignored" → "rejected" flip.
- **Recommendation.** Either (a) acknowledge the deviation in a brief
  ADR-0008 alongside the phase-00 final commit, or (b) record it as a
  one-line PROGRESS.md entry naming the SPEC clause being superseded.
  Option (b) is cheaper and enough for phase 00 (no new dep, no new
  schema surface). Do not block phase-done on this; the landed behavior
  is strictly tighter than SPEC called for and no fixture is broken.

#### N2 (new) — `deny_unknown_fields` regression tests only cover 4 of 9 attribute sites

- **Files:** `crates/envoy-bin/src/config.rs` (tests at lines 166–201),
  `tests/differential/src/lib.rs` (tests at lines 227–250).
- **Observation.** The four new regression tests assert unknown-field
  rejection at `Bootstrap` (root), `Listener` (nested-depth), and the
  two differential-harness structs (`Expectations`, `Equivalence`). The
  5 deeper config structs (`StaticResources`, `Address`, `SocketAddress`,
  `FilterChain`, `NetworkFilter`) carry the attribute but are not
  individually regression-tested — a future silent removal of the
  attribute on any of those 5 would not be caught by the current test
  matrix. PROGRESS.md "State 3 (loop-back)" section line 378–381
  already calls this out as a judgment-call Minor.
- **Why it is Minor.** Phase 00 ships one fixture (`envoy-rust.yaml`),
  the integration-level `echo_fixture` test indirectly exercises the
  whole tree, and a silent attribute removal would be caught by code
  review of any later schema-touching change. Not blocking phase-done.
  A small follow-up commit can add 5 more regression tests to close the
  gap — cheap and low-risk, but not required before state 6.

### Minor (pre-existing; deferred from prior REVIEW)

All seven items below are unchanged from the prior REVIEW.md. The prior
REVIEW classified them "nice-to-have; can defer"; none are fixed in the
loop-back (none were in scope). They remain open for future phases.

- **M1.** `parse_argv` accepts `-c --config-path` as path `"--config-path"`.
  Resolve when argv grows (phase 01 or 08) under a `clap`/`lexopt` ADR.
- **M2.** `Subject::shutdown` error-message context is thin (no PID or
  binary path in bail strings). CI-triage breadcrumb. Fold into I3's
  phase-01 follow-up.
- **M4.** `envoy-rust.yaml` (bind `127.0.0.1`) and `envoy.yaml` (bind
  `0.0.0.0`) differ on bind address; correct but worth a one-sentence
  README note.
- **M5.** `ENVOY_TARGET.md` records a digest that is not enforced at
  pull time. Audit record only under testcontainers 0.23.
- **M6.** `run_fixture` discards partial-read diagnostic info on
  `read_exact` errors. Only material once flakes appear.
- **M7.** `UpstreamProxy._container` field prefix-underscore is
  load-bearing for Drop-based shutdown — rename + comment suggested.
- **M8.** `Cargo.toml` `resolver = "2"` with edition 2024 — `resolver =
  "3"` available as a future workspace-grooming nit.

---

## Spec conformance matrix

Updated rows flagged with **[updated-in-re-review]**. All other rows
unchanged from the prior REVIEW.md.

| Deliverable | SPEC § | Landed? | Notes |
|---|---|---|---|
| D1 — Workspace members `crates/envoy-bin` + `tests/differential` | §2 D1 | Yes | `Cargo.toml:3–6`; `#![forbid(unsafe_code)]` present at both roots. |
| D1 — No HTTP / filter / `clap` deps in envoy-bin | §2 D1 | Yes | `crates/envoy-bin/Cargo.toml:12–18`: only permitted foundations. No new direct deps in the loop-back. |
| D2 — `ENVOY_TARGET.md` fully populated | §2 D2 | Yes | Unchanged. |
| D3.1 — argv `-c` / `--config-path` only, exit 2 on parse error | §2 D3.1 | Yes | `main.rs:39–60`; exit 2 via `ExitCode::from(2)` at `main.rs:86`. I2 now has a real assertion [updated-in-re-review]. |
| D3.2 — YAML parser for narrow Bootstrap shape | §2 D3.2 | Yes, with tightening (N1) [updated-in-re-review] | `crates/envoy-bin/src/config.rs:1–48`. Now rejects (rather than ignores) unknown fields via `#[serde(deny_unknown_fields)]` on all 7 structs. Behavior is strictly tighter than SPEC's "ignored" clause; see N1. |
| D3.3 — TCP accept loop + echo | §2 D3.3 | Yes | Unchanged. |
| D3.4 — `tracing-subscriber` + `ENVOY_RUST_LOG` | §2 D3.4 | Yes | Unchanged. |
| D3.5 — SIGTERM/SIGINT + 5s drain | §2 D3.5 | Yes | Drain path in `echo.rs:52–61` still not exercised by the harness (see I3 deferral); harness uses SIGKILL for deterministic between-fixture teardown. Unit tests cover the drain path. |
| D3.6 — Single-listener only | §2 D3.6 | Yes | Unchanged. |
| D4 — `run_fixture(Path) -> anyhow::Result<()>` | §2 D4 | Yes | `lib.rs:146`. |
| D4 step 1 — read `expectations.yaml` | §2 D4.1 | Yes | `lib.rs:147`. `Expectations`/`Equivalence` now reject unknown fields (see N1). |
| D4 step 2 — testcontainers launch of pinned Envoy | §2 D4.2 | Yes | Unchanged. Digest not enforced (M5). |
| D4 step 3 — envoy-rust spawn via `CARGO_BIN_EXE` | §2 D4.3 | Partial | Unchanged. `locate_envoy_bin` fallback (`subject.rs:59–85`) is functionally equivalent. |
| D4 step 4 — 10s accept-ready budget, exp backoff | §2 D4.4 | Yes | Unchanged. |
| D4 step 5 — drive payload and assert byte-exact | §2 D4.5 | Yes via ADR-0006 + ADR-0007 [updated-in-re-review] | `drive_tcp` now reads `payload.len()` then polls 100ms for trailing bytes and bails on any non-zero read. Byte-exact contract (BEHAVIOR_CONTRACT row 2, "no extra bytes") is restored end-to-end. Happy-path test `drive_tcp_round_trips_without_half_close` still green; new regression test `drive_tcp_rejects_trailing_bytes_after_echo` proves the silent-pass is closed. |
| D4 step 6 — Drop-guarded cleanup | §2 D4.6 | Yes | `Subject::drop` at `subject.rs:46–53` (SIGKILL best-effort per I3 deferral); `UpstreamProxy` via `ContainerAsync::Drop`. |
| D4 step 7 — Ok(()) on success | §2 D4.7 | Yes | `lib.rs:204`. |
| D4 — `tests/differential/tests/echo.rs` | §2 D4 | Yes | Unchanged. |
| D5 — five fixture files | §2 D5 | Yes | Payload still 18 bytes; verified. |
| D6 — CI workflow, five gate commands, `ubuntu-latest` | §2 D6 | Yes | Five-command gate re-verified green on run `24859537419` (1m 3s; 28 passed, 0 failed, 1 Docker-gated ignored, the integration `echo_fixture` passes in 8.01s). |
| D7 — ADR-0002, ADR-0003, ADR-0004 | §2 D7 | Yes | Plus ADR-0005, ADR-0006 (phase execution), and ADR-0007 (state-3 loop-back). Phase-00 final-commit ADR list is `[ADR-0002, ADR-0003, ADR-0004, ADR-0005, ADR-0006, ADR-0007]`. |

No spec deliverable is missing from the landed diff. Three deliverables
(D3.2 tightening via `deny_unknown_fields`, D4 step 3 `CARGO_BIN_EXE`
fallback, D4 step 5 via ADR-0006+ADR-0007) are reached via documented
alternatives. N1 flags the D3.2 tightening as lacking a dedicated ADR or
PROGRESS.md SPEC-deviation note; non-blocking.

---

## ADR assessment

- **ADR-0002 (GitHub Actions).** Sound; unchanged.
- **ADR-0003 (Rust edition 2024).** Sound; unchanged.
- **ADR-0004 (Envoy `v1.33.0`).** Sound; unchanged. Digest-not-enforced
  caveat (M5) still open.
- **ADR-0005 (cargo-deny wrappers + advisory ignores).** Sound; unchanged.
- **ADR-0006 (`drive_tcp` rewrite).** Sound and append-only-preserved
  across the loop-back (git-diff-verified: the only change to
  `DECISIONS.md` between `5355311` and `fca3aba` is the 20-line
  append of ADR-0007). ADR-0006's "Consequences" trailing-byte
  blindspot is now closed by ADR-0007.
- **ADR-0007 (`drive_tcp` trailing-byte poll).** **New in this
  loop-back.** Sound. (1) It enumerates the three REVIEW.md I1 options
  (a/b/c) verbatim and selects option (a), the prior REVIEW's
  recommendation. (2) Cross-references ADR-0006 as the source of the
  blind spot and explicitly preserves ADR-0006's force ("this ADR does
  not supersede ADR-0006"). (3) Quantifies the ~100ms per-connection
  idle cost and argues it acceptable for phase 00's two-drive_tcp
  workflow (~200ms). (4) Names a future revisit path (peek-based probe
  or smaller deadline) as its own future ADR, preserving append-only.
  Regression test `drive_tcp_rejects_trailing_bytes_after_echo`
  structurally reproduces the silent-pass and asserts the harness now
  fails it. **Edge-case assessment:** the 100ms poll does not detect
  trailing bytes written after t=100ms — acknowledged in the ADR as a
  phase-00 design tradeoff. A server that writes `payload.len()` bytes
  then closes immediately (no trailing bytes, fast close) lands in the
  `Ok(Ok(0))` arm (expected); a server that holds the connection open
  silently lands in the timeout `Err` arm (expected). A server that
  writes trailing bytes before closing lands in `Ok(Ok(n))` (the
  regression test exactly this shape). Happy-path and three failure
  modes all handled.

---

## Recommendations

- **Close N1 during state 6 final commit.** Add a one-line PROGRESS.md
  entry naming SPEC §D3.2's "ignored" language as superseded by the
  landed `deny_unknown_fields` behavior, or land a brief ADR-0008.
  Either is a <5-minute edit; phase-done is not contingent on it.
- **Defer N2 to a future Minor-cleanup commit.** Five additional
  regression tests (one per `StaticResources`, `Address`,
  `SocketAddress`, `FilterChain`, `NetworkFilter`) would close the
  coverage gap. Batch with other M-series cleanups.
- **Phase-01 follow-ups (carried over from prior REVIEW).** The
  functional SIGKILL → SIGTERM + drain + SIGKILL-escalate switch (I3
  functional portion) requires adopting `nix` under its own ADR — not
  on the D-3.2 permitted-foundations list for phase 00. The existing
  `TODO(phase-01)` block in `subject.rs:24–32` is the handoff.
- **Regression-test discipline template.** The loop-back commits
  (especially `245a65f` with its `drive_tcp_rejects_trailing_bytes_after_echo`
  test and `fca3aba`'s four `"unknown field"` regression tests) are
  textbook examples of "ship the test that would have caught the
  symptom if the pre-fix code had shipped." Use this shape as the
  default for every future harness-behavior ADR.

---

## Final verdict

**Approved with fixes (new Minor only) — advance to state 6.**

All three prior-REVIEW Important items and the folded-in M3 are fixed
across four atomic commits (`245a65f`, `ba17ee3`, `18bbfde`, `fca3aba`),
re-verified green on CI (run `24859537419`), and each fix lands with a
regression test that structurally fails against the pre-fix code. ADR-0007
is a clean append that closes ADR-0006's acknowledged blind spot without
editing the earlier ADR — exactly the narrative discipline MISSION.md
D-3.5 asks for. The two new Minors surfaced here (N1: document the
`deny_unknown_fields` SPEC tightening; N2: regression-test gap on 5 of
9 attribute sites) are strictly nice-to-have and neither blocks
phase-done; both can be addressed during the state-6 commit or batched
with future Minor cleanups. The phase is ready for state 6 per
`SKILL_ROUTING.md` — the final phase-00 commit with ADR list
`[ADR-0002, ADR-0003, ADR-0004, ADR-0005, ADR-0006, ADR-0007]`, ROADMAP
row flip to `done`, and STATE advance to phase 01.
