# Phase 54 — `54-accesslog-rcd-upstream-reset` — State-5 Code Review

**Reviewer:** fresh `general-purpose` subagent acting as senior code reviewer (no session history — independently re-derived every load-bearing claim against the actual tree, not just the PROGRESS.md/STATE.md narrative)
**Diff range:** `10fd62d..5ad70eb` (phase-53 close-out commit → phase-54 state-4 verification commit)
**Reviewed against:** `SPEC.md` + `PLAN.md` + `PROGRESS.md` + `BEHAVIOR_CONTRACT.md` (`%RESPONSE_CODE_DETAILS%` / `UC` `%RESPONSE_FLAGS%` rows) + `DECISIONS.md` ADR-0111 (pick+scope) + ADR-0044 (replay-safety precedent) + project coding standards (D-3.8, `anyhow`-in-`envoy-bin`-only, no hot-path `unwrap`/`expect`/`panic!`)
**Authoritative CI:** run `28481385288` @ `352c0c0` (first green), re-confirmed at run `28506732445` @ `5ad70eb` (the state-4 verification commit) — both jobs `success` on both runs

## VERDICT: ✅ **APPROVED** (0 Critical / 0 Important / 3 Minor → carry-forward)

The implementation faithfully executes SPEC §A–§G and PLAN Tasks 1–5. The reviewer independently
re-derived — not just re-read — every load-bearing claim: the `!retry_limit_exceeded_for_log`
guard ordering, the `final_outcome` last-write-wins replay-safety argument (ADR-0044 precedent),
the complete deletion of the `reset_for_log` boolean (decl + every set-site + every read-site),
the fixture `0061`-vs-`0062` byte-preservation claim (direct diff), both new in-process backstop
tests (ran them directly — both pass and assert the exact derived rcd/flag strings), and the
cited CI green run. All held up. No findings block merge → advance STATE to the §5 state-6
close-out.

---

## Strengths

- **Guard correctness, directly verified.** `crates/envoy-http1/src/hcm.rs:~1200-1206`: the rcd-set
  is `if matches!(final_outcome, Some(AttemptOutcome::Reset)) && !retry_limit_exceeded_for_log`.
  The reviewer traced `retry_limit_exceeded_for_log`'s only write site (`~:1171`) and confirmed it
  executes strictly before the §A block in the same post-loop reconciliation region, so the guard
  reads a fully-resolved boolean — exactly what SPEC §3.1 required to be confirmed.
- **Replay-safety, directly verified against the ADR-0044 precedent.** `final_outcome = attempt.outcome;`
  (`~:1082`) is reassigned unconditionally every loop iteration (last-write-wins). A reset-then-
  retried-to-success request ends the loop with `final_outcome = Some(AttemptOutcome::Response)`,
  so the `matches!(..., Some(AttemptOutcome::Reset))` check correctly fails and the rcd is never
  overwritten. This is a proven property, not a re-asserted claim.
- **Match-arm soundness.** The new arm `Some("upstream_reset_before_response_started{connection_termination}") => { "UC" }`
  sits after the `{overflow} => "UO"` arm in the same `match response_code_details_for_log.as_deref()`;
  all string patterns are mutually exclusive, and the `retry_limit_exceeded_for_log`/
  `connect_failure_for_log` boolean checks are still evaluated *before* the match via the
  enclosing `if/else if/else`, preserving the `URX → UF → rcd-match` precedence chain unchanged.
- **`reset_for_log` is genuinely, fully deleted** — declaration, per-request set-site, and the
  derive's `else if reset_for_log { "UC" }` branch are all gone. `grep -rn "reset_for_log"
  crates/ docs/ tests/` returns zero matches as live code/config; the remaining textual hits are
  historical "was retired" prose, which is appropriate.
- **Byte-preservation claim, independently spot-checked.** Diffing `tests/fixtures/0061-*/` against
  `tests/fixtures/0062-*/` directly: the only functional difference is the added
  `rcd: "%RESPONSE_CODE_DETAILS%"` json_format key plus path/node-id renames. `0061` logs
  `{rc,rf}`-only, so the new rcd-set on the reset path cannot change its emitted line — the `UC`
  flag now comes out of the rcd-match instead of the boolean, output-equivalent.
- **Backstop tests actually assert what they claim** — both run directly by the reviewer:
  `h1_upstream_reset_access_log_carries_uc_flag` passes and asserts the full logged line
  `{"rc":503,"rcd":"upstream_reset_before_response_started{connection_termination}","rf":"UC"}`;
  `h1_retry_exhausted_reset_keeps_via_upstream_rcd_and_urx_flag` passes and asserts
  `{"rc":503,"rcd":"via_upstream","rf":"URX"}` under `retry_on:"reset", num_retries:1`. Both assert
  on the exact derived rcd/flag strings, satisfying SPEC §3's M3 requirement that the negative
  case be a real, required backstop (fixture `0062` cannot exercise the retry-exhausted path).
- **CI verification claim independently reproduced.** `gh run view 28506732445` confirms both jobs
  `success`, matching PROGRESS.md's re-confirmation claim. The four documented LOCAL-ONLY failures
  (fixtures `0062`/`0061`, `admin_config_dump_server_info`, `xds_file_based_eds_fixture`) are all
  plausible pre-existing host artifacts — none touches `hcm.rs`'s reset/rcd logic in a way that
  would implicate this phase's change.
- **Standards clean.** `#![forbid(unsafe_code)]` intact; all `unwrap()`/`expect()` introduced are
  inside `#[tokio::test]` functions (exempt). `cargo clippy -p envoy-http1 --all-targets
  --all-features -- -D warnings` clean locally. No dependency/`Cargo.lock`/`Cargo.toml` changes.

---

## Findings (all Minor → carry-forward)

### Minor 1 → **M54-1** (doc precision) — off-by-one anchor citation `hcm.rs:1376`
Both `BEHAVIOR_CONTRACT.md` and `STATE.md` cite `hcm.rs:1376` as the `response_flags:` derive
site. The actual `response_flags: if retry_limit_exceeded_for_log {` line is at `hcm.rs:1377`;
`:1376` is the last line of the immediately-preceding phase-54 comment block. This is a 1-line
imprecision inherited from Task 4's re-grep (the pattern matched the comment line above the code,
not the code itself). It still unambiguously points a reader at the right spot — not worth
reopening the phase for. **Fold into the next phase that touches this BEHAVIOR_CONTRACT row or
this `hcm.rs` region:** nudge the anchor to `:1377`.

### Minor 2 (informational — no action required) — stale `:1055` in-comment reference
The §A rcd-set comment (Task 1) references an in-loop write region at `:1055`, already noted by
the phase's own Task-1 task-reviewer as cosmetic drift (the actual region shifted slightly after
the `reset_for_log` decl deletion). Confirmed still present in the final diff; harmless, a pure
comment line-number drift with no functional impact. No action required.

### Minor 3 (informational — no action required) — `BEHAVIOR_CONTRACT.md` table-cell growth
The `%RESPONSE_FLAGS%` row's per-flag equivalence prose is now six flags' worth of paragraph-style
rules packed into one long table cell (a pre-existing convention this phase continued, not
introduced). Not an issue for this phase; worth a note for a future phase if a seventh/eighth flag
makes the cell unwieldy enough to warrant a sub-table or separate reference doc.

---

## Acceptance-gate cross-check

Per PROGRESS.md's `## State-4 verification` section, CI run `28481385288` @ `352c0c0`
(re-confirmed at `28506732445` @ `5ad70eb`) is documented GREEN: fixture `0062` `ok` byte-exact
on native Linux (both proxies emit
`{"rc":503,"rcd":"upstream_reset_before_response_started{connection_termination}","rf":"UC"}`),
fixture `0061` + `admin_config_dump_server_info` + `xds_file_based_eds_fixture` all `ok`
(confirming the 4 LOCAL-ONLY failures were host artifacts, not regressions), **133
`test result: ok` / 0 FAILED** (all `0001`-`0061` additive-clean, one more green than phase 53's
132), `h2spec_pass_rate_gate ... ok` (unmoved, no H2 codec change), `cargo deny check` clean,
fuzz 4-target clean (no new target). CI is authoritative per project convention; the documented
evidence, independently reproduced by this review, satisfies §5 gate (a)–(e). (f) REVIEW.md is
this review.

**Disposition:** Approved for close-out. The 3 Minors are doc-precision / informational; none
gates the phase. NEW carry-forward: **M54-1** (the `:1376`→`:1377` anchor nudge); Minors 2–3 are
informational (no action). M53-2 + M53-3 (preserved by the §A guard, now also covered by an
in-process backstop) + M50-C + M48-2 + M42-1 + M45-1 + the `DC`/retry-budget-overflow slices of
M45-2 + older remain live from before this phase; none blocks. → Advance STATE to the §5 state-6
close-out.
