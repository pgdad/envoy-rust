# Phase 63 — `63-accesslog-h2-uf-connect-failure` — REVIEW

> State-5 code-review output (`superpowers:requesting-code-review`). Dispatched
> a fresh `general-purpose` subagent with NO prior session context — it
> independently re-derived every load-bearing claim from the diff, the code,
> the fixtures, and the docs rather than trusting the handoff summary.

**Range reviewed:** `a16cfe9..9fc4985` (`3c6ef1d`, `2861a6e`, `3c46492`,
`84a4df7`, `9683782`, `c3a1ab7`, `ba9336c`, `3c7e0b4`, `9fc4985`)

## Verbatim subagent verdict

### Strengths

- **The core mechanism is correct and exactly matches the described design.** Read `crates/envoy-http2/src/hcm.rs` directly (not the diff summary): the SOLE `AcquireOutcome::ConnectFailure` arm (line 396) is fed by exactly three upstream branches (H1-fork dial :261-269, H2-pool `acquire()` :286-294, no-pool fallback :323-331), confirmed by reading the surrounding `match acquire { ... }` in full. All three funnel into one `synth_h2_connect_failure()` call (line 403), a clean, minimal fix.
- **The unconditional post-loop computation is correctly unconditional.** `connect_failure_for_log_h2 = matches!(final_outcome_h2, Some(AttemptOutcome::ConnectFailure))` (hcm.rs:873-876) sits outside the `if attempts > 1 && !retry_budget_blocked` gate (hcm.rs:850), so a single connect-failure attempt with no `retry_policy` correctly flags `UF` — verified by reading the actual control flow, not just PROGRESS.md's claim.
- **`finalize_h2_stream` has exactly one call site** (line 907), confirmed via `grep -n "finalize_h2_stream("` — the new parameter is safe to add positionally.
- **`AttemptOutcome` is genuinely `Copy`** (`#[derive(Debug, Clone, Copy, PartialEq, Eq)]`, `crates/envoy-config/src/bootstrap.rs:1942`) — the loop-scoped capture pattern has no move/borrow hazard, as claimed.
- **`URX`-before-`UF` ordering matches the H1 precedent exactly** — confirmed by reading both derive sites (`envoy-http2/src/hcm.rs:1053-1067` and `envoy-http1/src/hcm.rs:~1305-1546`); the same `else if` cascade shape.
- **The new backstop test is genuinely new**, not a mutated existing test — `h2_connect_failure_access_log_carries_uf_flag` builds its own inline `HttpConnectionManagerConfig` with a wired `FileSink`, distinct from the two counter/retry-semantics tests it sits beside. It exercises the real end-to-end path (`drive_h2_once` → file read-back) and asserts the literal byte-exact line `{"rc":503,"rf":"UF"}\n`.
- **The two 502→503 flips are surgical** — diff confirms only the status literal and its adjoining string-literal/doc-comment prose changed in both `h2_connect_failure_retried_on_connect_failure_policy` and `h2_connect_failure_synth_does_not_tick_upstream_rq_5xx`; all counter/retry assertions are byte-identical to before.
- **Fixture `0068` is exactly the claimed `0066`-minus-`circuit_breakers` shape** — independently diffed both YAML pairs against `0066`; the only hunks are node-id/log-path/vhost-name plus the `circuit_breakers` block removal and an added explanatory comment. Confirmed no `{{BACKEND_PORT}}` token in either file via grep.
- **The `0001`-`0067` additivity claim holds up under independent re-derivation** — re-grepped `0009/0010/0018/0021/0064/0065/0066/0067` myself: `0021`'s `circuit_breakers` gates a *reachable* backend (not a dead endpoint); `0065`'s `127.0.0.1:1` is excluded pre-dial by a subset-miss; `0066`'s `circuit_breakers` pending-gate rejects pre-connect; `0067`'s `retry_policy` drives a real always-503 upstream. None reaches `AcquireOutcome::ConnectFailure`.
- **BEHAVIOR_CONTRACT wording is internally consistent** — the `%RESPONSE_FLAGS%` and `%RESPONSE_CODE_DETAILS%` rows both correctly state the H2 `UF` rcd stays `via_upstream` (non-deterministic, omitted), consistent with the existing H1 `UF` row and the H2 `NR`/`UH`/`UO`/`URX` sentences; no contradictions found.
- **No new crate/dependency** (`git diff` on all `Cargo.toml`/`Cargo.lock` is empty), **no new `Op`/`AccessLogRecord` field/`ConfigError` variant**, **`#![forbid(unsafe_code)]` intact** in `envoy-http2/src/lib.rs`.
- **TDD RED states are plausible**: Task 1's RED (`left: 502, right: 503`) matches what the pre-fix `synth_h2_502()`-calling arm would produce; Task 2's RED (`rf:"-"`) matches the derive's `_ => "-"` fallthrough since `via_upstream` doesn't match any of the `NR`/`UH`/`UO` rcd-string arms and the boolean didn't exist yet.
- **Independent CI corroboration succeeded**: `gh run view 28863138464` → `headSha 3c7e0b49...`, both jobs `success`; `gh run view 28866048616` → `headSha 9fc4985160...`, both jobs `success`. Both SHAs are commits inside the reviewed range (confirmed via `git log a16cfe9..9fc4985`).

### Issues

#### Critical (Must Fix)

None found.

#### Important (Should Fix)

None found. The un-recon'd retry-exhausted-connect-failure combination is well-defined by construction (the `URX`-checked-first ordering renders `URX` deterministically regardless of the un-verified live-Envoy disposition), matching SPEC §4's explicit deferral, so this is not a functional gap.

#### Minor (Nice to Have)

1. **Stale comment left un-updated by this phase's own status fix.** `crates/envoy-http2/src/hcm.rs:836` (in the post-loop reconciliation comment block) still reads: `"synth local replies (the no-healthy-upstream synth-503, connect-failure synth-502, reset synth-502, and overflow synth-503 paths) do NOT tick it"`. After this phase's Task 1, the connect-failure synth is 503, not 502 — only the *reset* arm (the sibling `synth_h2_502()` call site, hcm.rs:390) is still 502. This comment predates phase 63 (present verbatim in the pre-phase-63 tree at the equivalent line) and was not touched during the 502→503 correction, so it now misdescribes the connect-failure synth's status. Low impact (comment-only, doesn't affect behavior or tests) but worth a one-line fix in a future phase that touches this region: `"connect-failure synth-502"` → `"connect-failure synth-503"`.

#### Observations (no action needed)

- The `3c7e0b4` commit ("state-3 implementation — all 7 PLAN.md tasks landed") is docs-only (`STATE.md`/`STATE_HISTORY.md`, +64/-21 lines) — it is not a squash of the six task commits; those commits (`3c6ef1d`..`ba9336c`) are its actual ancestors with their real diffs intact. No history-rewriting concern.
- `synth_h2_502()`'s other call site (the post-connect `Sent(Err(e))` "Reset" arm, hcm.rs:384-395) is correctly left untouched at 502, consistent with SPEC §4's explicit deferral of the `UC` slice to a future phase.

### Recommendations

- When a future phase lands the `UC` slice (`synth_h2_502()`'s remaining call site), fold in the stale-comment fix at hcm.rs:836 noted above while that region is being touched anyway.
- No other process or architecture recommendations — this phase is a small, well-isolated, well-mirrored unit of work.

### Assessment

**Ready to merge?** Yes.

**Reasoning:** Every claim in the task description was independently re-derived from the actual code, tests, fixtures, and BEHAVIOR_CONTRACT text rather than trusted — the core `AcquireOutcome::ConnectFailure` mechanism, the unconditional post-loop `UF` computation, the single `finalize_h2_stream` call site, the genuinely-new backstop test, the surgical two-test 502→503 flips, the `0066`-minus-`circuit_breakers` fixture shape, the `0001`-`0067` additivity invariant, `#![forbid(unsafe_code)]`/no-new-dependency, and both CI runs' green conclusions all check out exactly as described. The only finding is a single pre-existing, now-slightly-stale doc comment with zero behavioral impact.

## §5.2 re-entry-point disposition

**APPROVED — 0 Critical / 0 Important / 1 Minor.** Per `BOOTSTRAP_PROMPT.md` §5.2,
phase 63 does **NOT** re-enter at state 3. The single Minor (the stale
`connect-failure synth-502` comment at `hcm.rs:836`, pre-existing and
un-introduced by this phase) is folded into the open carry-forward list as a
new Minor (**M63-1**) — non-blocking, to be fixed whenever a future phase next
touches that comment block (per the Recommendations above, ideally alongside
the `UC` slice of M56-1). This phase now advances to **state-6 close-out**,
which is its own future session (per memory `closeout-and-pick-are-separate-sessions`
— not chained into this one).
