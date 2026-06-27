# Phase 42 — `42-accesslog-response-code-details` — REVIEW

> **Lifecycle state 5 (code-review output).** Routed via `superpowers:requesting-code-review`;
> performed by a fresh `superpowers:code-reviewer` subagent with precisely-crafted context (the
> implementation diff + SPEC + PLAN + ADR-0099 §A–§C — NOT session history). Reviews the phase-42
> `%RESPONSE_CODE_DETAILS%` implementation (diff `f7d96ce..344dbd6`; the 6 TDD task commits
> `247d1e6`..`a41206a` + the `344dbd6` state-4 fmt fix).

## Verdict: **APPROVE** — 0 Critical / 0 Important / 2 Minor (confirmations of the bounded-vocabulary deferral; no new blockers)

The implementation is a faithful, minimal mirror of the phase-41 `%ROUTE_NAME%` precedent. It meets every
SPEC/PLAN/ADR-0099 requirement, the highest-risk part — the shared `BuildOutcome::Synth` widening — is purely
ADDITIVE, byte-preservation of fixtures `0001`-`0049` holds, the bounded-vocabulary deferral is honestly
scoped + documented, and every operator behavior has a test. The reviewer ran `cargo test -p envoy-accesslog
-p envoy-http1 -p envoy-http2` (all pass; the one host-flaky `…h2_handshake…` test is the documented
pre-existing flake, CI-authoritative-green, not part of this change). The full suite + differential passed on
the AUTHORITATIVE CI run `28301067467` @ `344dbd6` (`completed/success`).

## Verification (all UPHELD)
- **Operator code is an exact `Op::RouteName` clone:** `command_operator.rs` (the no-arg keyword dispatch
  rejecting BOTH `(...)` and `:N` — the §6.2-locked strict-no-arg grammar; the `render_op`
  `unwrap_or(empty_or_dash)` arm) and `json_format.rs` (the `encode_single_op` `quote_opt` arm) mirror the
  precedent character-for-character. Text present→string / absent→`-`; json present→quoted / absent→`null`;
  mixed-leaf→string with the `-` sentinel — all correct + unit-tested.
- **The `BuildOutcome::Synth` widening is purely ADDITIVE (the load-bearing safety property):**
  `Synth(Response)` → `Synth(Response, Option<&'static str>)` (the enum is in `envoy-http1`, SHARED — H2
  reuses it via `envoy_http1::build_response`). All 5 H1 construction sites tagged correctly
  (`synth_direct_response`→`Some("direct_response")`; `synth_501`/`synth_400`/`synth_404`×2→`None`); both
  reader arms (H1 writer-arm, H2 `Synth` match) only READ the new `Option<&'static str>` into a log variable
  — ZERO effect on response bytes / status / routing. The `&'static str` choice (no allocation until
  `.map(str::to_owned)` at the log-build site) is clean.
- **Byte-preservation airtight:** the new `response_code_details` field defaults `None` + the operator is
  new + no pre-`0050` fixture references it → all `0001`-`0049` stay byte-identical; no `Cargo.toml`/
  `Cargo.lock` change.
- **HCM plumbing per the PLAN:** the H1 proxy-success arm + the H2 proxy-success arm set `Some("via_upstream")`;
  the H2 `response_code_details_for_log_h2` parameter is threaded into `finalize_h2_stream` (mirroring
  phase-41 `route_name_for_log_h2`); both record builds assign the field. HCM-sets-`direct_response` covered
  on both H1 and H2 by new integration tests.
- **Doctrine clean:** `#![forbid(unsafe_code)]` intact in all 4 touched crates; no new crate/dependency; no
  new `ConfigError` variant; exactly ONE new `AccessLogRecord` field; exactly ONE new `Op` variant; the fuzz
  seed `response_code_details.yaml` is git-tracked with its explicit `!`-un-ignore line.
- **Fixture `0050`** asserts the byte-exact line `{"method":"GET","proto":"HTTP/1.1","rcd":"d=direct_response",
  "single_rcd":"direct_response"}` (keys in UTF-8 byte order; compact separators), live-captured from
  v1.33.0.

## Findings

**Critical:** none.  **Important:** none.

**Minor (confirmations / carry-forward — NOT blockers):**
- **M42-1 (new carry-forward) — `via_upstream` is set on endpoint-PICK, not response-success.** `hcm.rs`
  (H1 `:984`, H2 `:680`) set `response_code_details = Some("via_upstream")` as soon as an endpoint is *picked*
  (`attempt.endpoint.is_some()`), regardless of whether the attempt then fails (connect error / reset /
  retries-exhausted → 503). Real Envoy emits a DIFFERENT detail for those failure paths
  (`upstream_connect_error`, `upstream_reset_before_response_started{...}`, etc.). This is the inner mechanic
  of the bounded-vocabulary deferral and is **NOT a correctness gap for this phase** — no fixture exercises
  any proxy path (the access-log fixture family is `direct_response`-only, `clusters: []`), so the divergent
  value is unobservable. **Carry-forward:** a future "proxy access-log fixture" / "full detail vocabulary"
  phase must make `via_upstream` success-path-only and add the failure-path details. No action now.
- **`via_upstream` is implemented but not differentially witnessed** — only `direct_response` is fixture-backed
  (`0050`); `via_upstream` is set + render-unit-tested only. This is explicit in the SPEC (§2.1/§2.2), PLAN
  (Task 6 note), and the BEHAVIOR_CONTRACT row. Honestly documented; noted for completeness.

## Strengths
- The highest-risk part (widening the SHARED `BuildOutcome::Synth` enum across H1 + H2) was handled exactly
  right: purely additive, the detail READ only by the new operator, every construction site compiler-enforced.
- `%RESPONSE_CODE_DETAILS%` is a clean, minimal mirror of `%ROUTE_NAME%`/`%UPSTREAM_HOST%`; every ADR-0099 §C
  behavior has a dedicated test (parse-rejects-paren-and-`:N`, text/json present+absent+mixed, H1/H2 set).
- The bounded-vocabulary discipline (ADR-0098 §B "rabbit hole" risk) is honored + honestly documented — the
  operator is complete; only the SET of distinct detail strings is bounded.

---

_Reviewed at state-5. **APPROVE** (0 Critical / 0 Important / 2 Minor, non-blocking; M42-1 is a new
carry-forward). The §7.5 (a)-(e) gate was GREEN at state-4 (AUTHORITATIVE CI `28301067467` @ `344dbd6`
`completed/success`). With (f) `REVIEW.md` APPROVE, the full §7.5 (a)-(f) gate is COMPLETE → the next session
is the state-6 phase-close (flip ROADMAP row `42` → `done`, advance STATE to awaiting-next-planning)._
