# Phase 45 — `45-accesslog-rcd-no-healthy` — STATE-5 Code Review

**Reviewer:** fresh `superpowers:code-reviewer` subagent (no session history), dispatched over the phase-45 implementation diff `git diff 8f35dd8^..77d4966` (7 files; the only `src/` change is `crates/envoy-http1/src/hcm.rs`) against `SPEC.md` / `PLAN.md` / `PROGRESS.md` / project doctrine.

## Overall verdict: ✅ APPROVED

A minimal, correct, well-scoped diff. The single `src/` change is a 6-line `else` branch that sets an existing access-log field (`%RESPONSE_CODE_DETAILS%`) on a previously-unset arm; everything else is a fixture, a differential test, and doc updates. The critical `endpoint: None` exclusivity check holds rigorously.

## Findings by severity

- **Critical:** none
- **Important:** none
- **Minor:** none

## What was verified (confidence: high)

**1. `endpoint: None` is EXCLUSIVELY the no-healthy `pick()->None` path (the single most important check).** The reviewer independently enumerated EVERY `AttemptResult` construction in `hcm.rs`:
- `:436-441` — no-healthy `pick()->None` → `endpoint: None` (**the ONLY one**)
- `:597-602` — real upstream response → `endpoint: Some`
- `:617-622` — send/recv failure (Reset) → `endpoint: Some`
- `:626-631` — `ConnectFailure` → `endpoint: Some`
- `:637-642` — `Overflow` → `endpoint: Some`

The struct doc (`:386-391`) independently codifies the invariant. So the `else` at `:996-1002` CANNOT mislabel any connect-fail / reset / overflow / real-response path as `no_healthy_upstream`.

**2. Retry-loop safety.** The no-healthy arm carries `outcome: None` (`:439`) → `final_retriable = false` (`:1026`) → the loop `break`s at `:1071` (never `continue`s). The `if`/`else` is total and runs at the top of every iteration, unconditionally overwriting `response_code_details_for_log`, so only the terminal attempt's value survives (identical last-attempt-wins semantics to the pre-existing `upstream_host_for_log`). A retry sequence (attempt-1 5xx `via_upstream` → re-pick no-healthy) ends correctly labelled `no_healthy_upstream`; no stale carry-over possible.

**3. The 503 is unchanged.** `synth_no_healthy_upstream` (`:1671`) and the `AttemptResult` struct are NOT touched by the diff — the change only sets the access-log detail field; status/body/headers/flags untouched. The backstop additionally asserts `HTTP/1.1 503 ` + body `no healthy upstream`.

**4. The backstop test** `h1_no_healthy_access_log_carries_no_healthy_upstream_rcd` drives the FULL H1 dispatch via `drive()` against a real NO_FALLBACK subset-miss cluster, genuinely exercising `pick_endpoint -> None`; asserts the captured file line equals `{"rc":503,"rcd":"no_healthy_upstream"}\n` + the 503/body. A faithful TDD fail-first (red `rcd:null` → green).

**5. The `0053` fixture pair** differs ONLY in the three benign per-side deltas (admin dropped, listener bind `0.0.0.0`→`127.0.0.1`, log path) — the STATIC `subset_cluster` (NO_FALLBACK, `keys:[stage]`, one `127.0.0.1:1` endpoint `{stage:prod}`) and the `metadata_match {stage:nonexistent}` route are byte-identical; the `json_format` logs only deterministic operators; the probe declares `expected_status: 503`. All 5 new files are git-tracked. The differential test is a faithful `run_fixture` clone.

**6. Doctrine.** `#![forbid(unsafe_code)]` intact (`lib.rs:1`); no new `Op`/`AccessLogRecord` field/crate/dependency/fuzz-target/`ConfigError` variant; no `Cargo.toml`/`Cargo.lock` change. The additive invariant holds (`0050`/`0051`/`0052` are success-path fixtures with no no-healthy arm) — CI-proven by run `28306490466` (151 passed, 78 fixture binaries green incl. `0053`, fuzz + deny + h2spec green). The BEHAVIOR_CONTRACT row is accurate, correctly deferring connect/overflow (M45-2) and H2 (M45-1).

## Disposition

No Critical/Important findings to fold; no Minor findings to carry forward. Phase 45 is APPROVED for state-6 close-out (flip ROADMAP row `45` → `done`). The new carry-forwards **M45-1** (H2 failure-path details + an H2 access-log differential driver) and **M45-2** (connect-failure/overflow non-deterministic detail strings) stay live for a future phase; the prior carry-forwards (M42-1 failure-path slice now OPENED, M40-1, M39-*, etc.) are untouched by this phase.
