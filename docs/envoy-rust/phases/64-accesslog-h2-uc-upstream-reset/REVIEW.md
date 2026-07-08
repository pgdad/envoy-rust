# Phase 64 — `64-accesslog-h2-uc-upstream-reset` — REVIEW (state-5 code-review)

> Produced by a fresh `general-purpose` subagent dispatched with `superpowers:requesting-code-review`, no prior session context. Git range reviewed: `1e142d9` (state-2 PLAN-write) `..` `e4c69bd` (state-4 verification docs commit) — covering phase 64's own 9 commits (`84465f8`..`8cea6c1`), the sibling out-of-band maintenance-workstream commits that landed post-state-3 and touch phase-64-adjacent surface (`57601a6`, `de71c27`, `6b3625e`), and the state-4 verification docs-only commit (`e4c69bd`). The reviewer was explicitly instructed to independently verify the three maintenance commits did not silently break phase 64's own artifacts (not merely trust their commit messages), and to read the CURRENT post-refactor state of the touched files directly rather than relying on the diff alone.

## Strengths

- **The core `hcm.rs` fix is exactly what SPEC/PLAN specify, with no drift.** `crates/envoy-http2/src/hcm.rs:384-395` (`synth_h2_reset()`, status 503), the `reset_for_log_h2` declaration (`:577`), its post-loop set (`:896-897`), its threading through `finalize_h2_stream` (`:944`, `:1000`), and the derive branch (`:1091-1099`, correctly ordered `URX → UF → UC → rcd-match`) all match PLAN's Task 1 verbatim. Verified by direct read, not by trusting `PROGRESS.md`.
- **The comment sweep correctly distinguishes active-state vs. historical prose**, per the project's own D-3.4/D-3.5 convention. `synth_h2_connect_failure()`'s doc comment (`:1219-1221`) and `synth_h2_no_healthy_upstream()`'s doc comment (`:1241-1244`) — both describing *today's* mechanism — are correctly updated to name `synth_h2_reset()`. The M63-1 anchor (`:847-850`) has both stale `synth-502` mentions corrected to `synth-503`. Meanwhile genuinely historical phase-57/63-authored comments (`:2858-2865`, narrating those phases' own pre-fix states) are correctly left verbatim.
- **The in-process backstop test asserts the right thing.** `h2_upstream_reset_access_log_carries_uc_flag` (`hcm.rs:4962-5033`) asserts both `status == 503` *and* the exact logged line `{"rc":503,"rf":"UC"}\n` — not merely "any non-2xx." It was genuinely fail-first (`PROGRESS.md` quotes the pre-fix `left: 502 / right: 503` failure).
- **The additivity claim is independently verifiable and correct.** Re-running the grep across `0009/0010/0018/0021/0064-0068` confirms exactly the divergence PLAN/README describe (`0021`/`0066` circuit_breakers, `0065`/`0068` comment-only `127.0.0.1:1`, `0067` retry_policy against an always-responding `Http2EchoBackend`). No existing H2 fixture reaches `AcquireOutcome::Sent(Err(e))`.
- **The three post-hoc maintenance commits genuinely preserved phase 64's artifacts.** `de71c27`'s own commit message is unusually transparent about *why* it was needed (a rebase integration break — `Http2CloseBackend` called a function the sweep had gated `#[cfg(test)]`) and what it changed. Directly comparing the pre- and post-refactor `Http2CloseBackend::spawn`/`Drop` bodies confirms byte-for-byte identical spawn args (`--port <p> --close-before-response`), env (`RUST_LOG=warn`), stdio (null/inherit), `kill_on_drop(true)`, the same 2s `wait_h2_accept_ready` budget, and identical `Drop` semantics (now via the shared `kill_and_reap`, which is a verbatim inline of the original loop). The `H2_CLOSE_BACKEND_PORT` marker-scan/spawn block and its push into both `upstream_kvs`/`subject_kvs` (`tests/differential/src/lib.rs:3340-3355`, `:3388-3390`, `:3484-3486`) survived the 3,918-line `run_fixture` decomposition intact and correctly gated.
- **`helper-common`'s argv skeleton is a clean, behavior-preserving extraction.** `--close-before-response` composes via the documented `extra` callback (`http2-echo-server/src/main.rs:55-70`) with no change to parse semantics, help text, or error messages.
- **No unsafe code, no new dependency, no wire/behavior regression.** `grep -n unsafe` across the whole diff only matches `#![forbid(unsafe_code)]` declarations. The only `Cargo.lock` changes are the `helper-common` extraction (consolidating pre-existing `thiserror`/`tracing-subscriber` deps that were already present per-binary) and an unrelated `aws-lc-rs` dev-dep drop from `envoy-filter` (part of the sibling sweep, not phase 64, and `Cargo.toml` comments explain why it's no longer needed).
- **`BEHAVIOR_CONTRACT.md`'s M56-1 closure claim is accurate**: the `%RESPONSE_FLAGS%` row now lists all six H2 values (`NR/UH/UO/URX/UF/UC`), matching H1's own six-flag completion — verified by direct read of the row text, not by trusting the phase's own narrative.

## Issues

### Critical (Must Fix)

None found.

### Important (Should Fix)

None found. The one candidate — whether `wait_h2_accept_ready`'s handshake-only readiness probe could race with `--close-before-response`'s stream-level reset — does not hold up: `wait_h2_accept_ready` (`tests/differential/src/backend.rs:538-561`) only does a TCP connect + `h2::client::handshake`, spawning a background task to drive the connection; it never calls `send_request`, so it never triggers `conn.accept()` on the server side to return a stream. Each subsequent real connection (from either proxy) is handled by an independent per-connection task in `http2-echo-server`'s accept loop, so the probe connection sitting idle does not block or interfere with the fixture's actual request. This matches the SPEC/PLAN claim.

### Minor (Nice to Have)

1. **`crates/envoy-http2/src/hcm.rs:236`** — the phase-16-authored comment "The synth response shape on every failure path is unchanged (synth_h2_502 / synth_h2_overflow)" still names `synth_h2_502`, a function that no longer exists anywhere in the file (it's now split into `synth_h2_reset()` and `synth_h2_connect_failure()`). This wasn't named in PLAN §3 item 8's comment-sweep scope, and is defensible as historical narrative under D-3.4/D-3.5 (consistent with several other deliberately-untouched `synth_h2_502` mentions at `:1211-1218`, `:1238-1244`, `:1257-1267`), but it's the one instance where the referenced name is now *completely* absent from the codebase rather than renamed-with-a-pointer. A future drive-by could add a one-line forward-pointer here for clarity; not worth a dedicated fix.
2. **The `Http2CloseBackend`/`Http2EchoBackend` readiness-probe pattern (pre-existing, not phase-64-introduced)** leaves one idle H2 client connection + background driver task per successful probe attempt, cleaned up only when the backend subprocess is killed at test-end (`kill_on_drop(true)`). Bounded and harmless, but worth noting as an inherited minor inefficiency rather than a phase-64 regression.

## Recommendations

- None required for merge. If a future phase wants to reduce test-harness duplication further, `Http2CloseBackend` and `Http2EchoBackend` are now close enough (both thin wrappers over `spawn_helper_backend` + `wait_h2_accept_ready`) that they could plausibly be unified with a boolean/enum discriminator — not urgent, since the current duplication is small and the maintenance workstream already did the heavy consolidation.
- The deferred `M64-1` (H2-side deterministic `UC` rcd) and the un-recon'd retry-exhausted-reset combination are both reasonably scoped deferrals, consistent with the H1 precedent (phase 53→54) and clearly logged as carry-forwards — no action needed now.

## Assessment

**Ready to merge?** Yes.

**Reasoning:** Direct inspection of the current (post-refactor) source — not just the phase's own narrative — confirms the `hcm.rs` fix, comment sweep, test-harness additions, fixture, differential test, and `BEHAVIOR_CONTRACT.md` update all match SPEC/PLAN precisely, the additivity invariant holds under independent re-verification, and the three sibling maintenance commits are genuinely behavior-preserving for every phase-64 artifact checked (spawn args, readiness probe, Drop semantics, marker wiring). No unsafe code, no new dependency, no regression found.

## §5.2 disposition

**APPROVED — 0 Critical / 0 Important / 2 Minor.** Per `BOOTSTRAP_PROMPT.md` §5.2, since `REVIEW.md` is APPROVED, the phase does **NOT** re-enter at state 3. The two Minor items are noted as open cosmetic carry-forwards (not blocking); they are cheap enough to fold into whatever future phase next touches `crates/envoy-http2/src/hcm.rs`'s comment block or the differential harness's backend structs, per the project's standing carry-forward convention — they do not warrant a dedicated follow-up phase. The next session is the **§5 state-6 close-out** (a SEPARATE session per §5.1 one-state-per-session and per memory `closeout-and-pick-are-separate-sessions`).
