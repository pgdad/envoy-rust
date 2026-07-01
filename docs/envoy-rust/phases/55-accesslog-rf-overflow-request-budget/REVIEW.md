# Phase 55 — `55-accesslog-rf-overflow-request-budget` — State-5 Code Review

**Reviewer:** fresh `general-purpose` subagent acting as senior code reviewer (no session history — independently re-derived every load-bearing claim against the actual tree/CI, not just the SPEC/PLAN/PROGRESS narrative)
**Diff range:** `773c2e4..4ceb095` (phase-54 close-out commit → phase-55 state-4 verification commit) — spans the state-1 brainstorm (`ed46866`), state-2 PLAN-write (`7544a6c`), state-3 Task 1 (`197e9d6`), state-3 Task 2 (`eca1d4c`), state-3 PROGRESS-update (`d8301c3`), and state-4 verification (`4ceb095`) commits
**Reviewed against:** `SPEC.md` + `PLAN.md` + `PROGRESS.md` + `BEHAVIOR_CONTRACT.md` (`%RESPONSE_FLAGS%` row) + `DECISIONS.md` ADR-0112 (pick + scope) + project coding standards (D-3.8 `#![forbid(unsafe_code)]`)
**Authoritative CI:** run `28540697796` @ `d8301c3` — full job log independently pulled and re-derived by the reviewer, not just the job-summary conclusion

## VERDICT: ✅ **APPROVED** (0 Critical / 1 Important, non-blocking — carry-forward / 2 Minor → carry-forward)

The implementation faithfully executes SPEC §A–§G and PLAN Tasks 1–3. The reviewer independently re-derived — not just re-read — every load-bearing claim: fixture `0063`'s cluster/reachability shape (confirmed `{{HTTP1_BACKEND_PORT}}` unconditionally spawns a live `Http1EchoBackend`, `tests/differential/src/lib.rs:3209`, avoiding the ADR-0047 `UF`-vs-`UO` prefetch trap); the `BEHAVIOR_CONTRACT.md` edit (the M50-C deferral sentence fully replaced, not appended; all 5 `hcm.rs:1376`→`:1377` anchors corrected in the `NR`/`UH`/`URX`/`UF`/`UC` clauses while the `UO` clause's own text was correctly left untouched); the zero-`crates/`-diff claim (mechanically confirmed via `git diff --stat`/`git log` scoped to `crates/`); the purely-additive file list (`git diff --name-status`); and the CI green run (pulled the full log for `28540697796`, independently found `test access_log_rf_overflow_request_budget ... ok`, workspace-wide `134`/`0` ok/FAILED counts, `h2spec_pass_rate_gate ... ok`, `cargo deny check` clean, both jobs `success`). All held up. One Important finding surfaced (stale source-comment staleness, pre-existing to this phase's own edits but left uncorrected) — the reviewer's own assessment judged it non-blocking for a fixture/doc-only phase; folded as a new carry-forward rather than gating this review. No finding blocks merge → advance STATE to the §5 state-6 close-out.

---

## Strengths

- **Zero `crates/` change claim is mechanically true**, not just narratively asserted. `git diff --stat 773c2e4..4ceb095 -- crates/` and `git log --oneline 773c2e4..4ceb095 -- crates/` both return nothing. `#![forbid(unsafe_code)]` trivially holds.
- **Fixture `0063` is structurally sound and reachability-correct.** `envoy-rust.yaml`/`envoy.yaml` use `STRICT_DNS` + `circuit_breakers.thresholds:[{priority: DEFAULT, max_requests: 0}]` + a single endpoint at `{{BACKEND_HOST}}`:`{{HTTP1_BACKEND_PORT}}`. The reviewer confirmed in `tests/differential/src/lib.rs:3209` that `{{HTTP1_BACKEND_PORT}}` unconditionally spawns a real `Http1EchoBackend::spawn()` (no fixture-name allowlist gate) — the endpoint is genuinely reachable, correctly avoiding the ADR-0047 `upstream_cx_total` prefetch-divergence trap (an unreachable endpoint under `max_requests:0` would surface `UF`, not the intended `UO`, defeating the witness).
- **`BEHAVIOR_CONTRACT.md` edit is precise.** `grep -n "differential witness deferred (M50-C)"` → 0 matches (fully replaced, not appended alongside). `grep -n "hcm.rs:1376\b"` → 0 matches; `grep -c "hcm.rs:1377"` → all 5 corrected. The reviewer diffed the exact row: the `UO` clause's own text is untouched (it never cited a line-number anchor to begin with), exactly matching PLAN.md Task 2 Step 2's specification.
- **Diff is purely additive.** `git diff --name-status 773c2e4..4ceb095` shows only new files under `tests/fixtures/0063-...`, `tests/differential/tests/...`, and the phase-55 docs directory, plus edits confined to `BEHAVIOR_CONTRACT.md` (1 row), `DECISIONS.md` (pure append, confirmed via `git diff | grep '^-'` showing no deletions), `ROADMAP.md`, `STATE.md`, `STATE_HISTORY.md`. No `0001`-`0062` fixture touched.
- **CI ground truth independently reproduced, not narrative-trusted.** Pulled the full log for run `28540697796`: `test access_log_rf_overflow_request_budget ... ok`; workspace-wide `134` `test result: ok` / `0` `test result: FAILED`; `h2spec_pass_rate_gate ... ok`; `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok`; both jobs (`fuzz`, `build + test + lint`) `conclusion: success`. Matches PROGRESS.md's transcription.
- **Process discipline genuinely followed** — per-task subagent review, PLAN-VERIFY re-derivation (twice-confirmed `crates/` no-op), and the state-4 session pulling the full CI log rather than trusting the job-summary conclusion, all independently re-verified by the reviewer rather than accepted at face value.

---

## Findings

### Important (non-blocking; carry-forward) → **M55-1** — stale "M50-C ... deferred" comments in `crates/envoy-http1/src/hcm.rs`

Two source comments still assert M50-C is open/deferred, now contradicted by this very phase's own `BEHAVIOR_CONTRACT.md`/`PROGRESS.md` closure:

- `hcm.rs:948-950`: `"...In-process-backstopped (M50-C: its differential witness is deferred — 0058 exercises only the pool PendingOverflow arm)."`
- `hcm.rs:7221`: `"...the in-process proof for the budget arm (its differential witness is deferred: M50-C)."`

Both now read as false relative to the project's own ledger (`BEHAVIOR_CONTRACT.md` + `PROGRESS.md` both say M50-C is CONSUMED / CI-CONFIRMED CLOSED as of this phase). SPEC.md scoped "no `crates/` change" as a *functional* invariant — appropriately, since this phase's differential proof required zero behavior change — but these are pure comments; editing them carries zero risk to that invariant and would not have required re-running the differential. This phase went out of its way to fix the mirror-image staleness (M54-1, the `hcm.rs:1376`→`:1377` anchor) inside `BEHAVIOR_CONTRACT.md`; leaving the equivalent staleness sitting in the source file both docs point back to is an inconsistent application of the same standard, and risks confusing a future reader of `hcm.rs` into believing M50-C is still open.

**Not blocking** — this is a comment-only doc-staleness issue with no effect on correctness, tests, CI status, or the differential proof itself (matching the reviewer's own "Yes, with a follow-up note" assessment). **Fold into the next phase that touches `hcm.rs`'s request-budget region** (or address opportunistically as a trivial comment-only commit): update both comments to reflect M50-C's closed status (e.g. "witnessed byte-exact by fixture 0063, phase 55, ADR-0112" in place of "differential witness is deferred").

### Minor 1 (informational — no action required) — additional stale line-number anchors in the same `hcm.rs` region, pre-existing to this phase

Independent of the M50-C staleness above, the anchors cited *inside* those same comments have also drifted from the current file (confirmed via `grep -n`, not taken on the reviewer's own say-so):
- `hcm.rs:7217` cites `"BudgetAcquisition::Rejected at hcm.rs:911"` — the actual match arm is now at `hcm.rs:930` (off by 19).
- `hcm.rs:7219` cites `"calls synth_overflow at :923"` — the actual `synth_overflow(close)` call is now at `hcm.rs:942` (off by 19).
- `hcm.rs:946` / `hcm.rs:7219` cite the record-build derive as `:1277`/`":995"` — the actual derive block starts at `hcm.rs:1377` (the same anchor `BEHAVIOR_CONTRACT.md` was corrected to point at by this phase's own Task 2).

These predate phase 55 — `git diff` confirms `crates/` is untouched by this phase's commits, so none of this drift was introduced here — and are out of this phase's declared scope. Flagging as informational since they compound the same class of documentation drift the M54-1 carry-forward tracks; a future phase touching this region should fold them in alongside the M55-1 fix above.

### Minor 2 (informational — no action required) — `PROGRESS.md`'s CI-log claim is a valid inference, phrased as if it were a literal grep hit

`PROGRESS.md`'s `## State-4 verification` section states the CI log was pulled and "confirmed directly" the byte-identical `{"rc":503,...}` line. The CI log's step output only prints `test access_log_rf_overflow_request_budget ... ok` — it does not print the access-log file's contents. The inference is sound (the `http1_access_log_byte_exact` driver's assertion *is* the byte-exact comparison, so a passing test entails the line matched), but the wording slightly overstates what was literally observed in the log versus what was validly inferred from pass/fail semantics. Not a materially false claim — a wording nit only, no action required.

---

## Acceptance-gate cross-check

Per PROGRESS.md's `## State-4 verification` section, CI run `28540697796` @ `d8301c3` is documented GREEN and was independently reproduced by this review: fixture `0063` `ok` (both proxies emit `{"rc":503,"rcd":"upstream_reset_before_response_started{overflow}","rf":"UO"}`), **134 `test result: ok` / 0 FAILED** (one more green than phase 54's 133 — the net-new `0063`, confirming no pre-existing fixture regressed), `h2spec_pass_rate_gate ... ok` (unmoved, no H2 codec change), `cargo deny check` clean, `fuzz` job `success` (no new target, `ci.yml` unchanged per SPEC §2 SKIP). CI is authoritative per project convention; the documented evidence, independently reproduced by this review, satisfies §5 gate (a)–(e). (f) REVIEW.md is this review.

**Disposition:** Approved for close-out. The one Important finding (**M55-1**, stale `hcm.rs` "M50-C deferred" comments) is non-blocking for this fixture/doc-only phase and is CARRIED FORWARD; the 2 Minors are informational (no action). M53-2 + M53-3 (preserved by the §A guard, also covered by an in-process backstop) + M48-2 + M42-1 + M45-1 + the `DC`/retry-budget-overflow slices of M45-2 + M40-1 + M39-*/M38-*/CF-39-1 + older remain live from before this phase; none blocks. **M50-C is CONSUMED** (closed by this phase — CI-confirmed). → Advance STATE to the §5 state-6 close-out.
