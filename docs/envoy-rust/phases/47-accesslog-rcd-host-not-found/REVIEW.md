# Phase 47 — `47-accesslog-rcd-host-not-found` — STATE-5 Code Review

**Reviewer:** fresh `superpowers:code-reviewer` subagent (no session history), dispatched over the phase-47 implementation diff `git diff 60980d5^..344f01e` (7 files; the only `src/` change is the ONE-line edit in `crates/envoy-http1/src/hcm.rs` plus a backstop test) against `SPEC.md` / `PLAN.md` / `PROGRESS.md` / project doctrine. The reviewer was instructed to distrust the brief and independently verify every claim against the live tree — it did, including a temporary fail-first reproduction (revert the arm to `None`, observe red, restore).

## Overall verdict: ✅ APPROVED

A tiny, exemplary, well-scoped diff — the sibling of phase 46 on the OTHER `synth_404` arm. Every claim in the SPEC/PLAN/PROGRESS was independently verified against the actual source. No defects at Critical or Important; two cosmetic Minors (carry-forward).

## Findings by severity

- **Critical:** none
- **Important:** none
- **Minor:** two (cosmetic; non-blocking → carry-forward as **M47-1**)

## What was verified (confidence: high)

**1. The edit is on the correct arm (the check that matters most).** `hcm.rs:1535-1536` — the host-miss arm (immediately after `tracing::warn!(… "request rejected: no matching virtual_host")`, the `vh = None` branch) — now returns `BuildOutcome::Synth(synth_404(close), Some("route_not_found"))`. The route-miss arm at `hcm.rs:1554-1555` (after `"request rejected: no matching route"`) is UNCHANGED, still carrying its phase-46 `Some("route_not_found")` (ADR-0103). The edit landed on the correct arm and did not touch the route-miss arm; the diff is exactly one logical line plus a comment.

**2. The host-miss 404 access-log line carries the detail.** Traced end-to-end: the host-miss `Synth(resp, Some("route_not_found"))` → the writer-arm (`hcm.rs:866`, `response_code_details_for_log = details.map(str::to_owned)`) → the `AccessLogRecord` built unconditionally below the match (`hcm.rs:1249`, `response_code_details: response_code_details_for_log`). NO early `return` between the match arm and the record build bypasses the access log.

**3. The 404 is unchanged.** `synth_404` (`hcm.rs:1817`) is not in the diff; the 404 status/body/headers/flags are byte-untouched — the change is purely additive (sets a previously-`None` detail string). The backstop additionally asserts `HTTP/1.1 404 ` + `content-length: 0`.

**4. The backstop test genuinely fails-first.** `h1_host_miss_access_log_carries_route_not_found_rcd` (`hcm.rs:5457`) drives a genuine host-miss — a NON-wildcard `domains:["match.test"]` vhost probed with a NON-EMPTY, non-matching `Host: nomatch.test` (past the empty-Host `synth_400` guard, into the host-miss arm). The reviewer reverted the arm to `None` and observed the test go red (`left: "{\"rc\":404,\"rcd\":null}\n"`), then restored the file — proving it exercises the changed line and is not vacuously green. The phase-46 route-miss backstop (`hcm.rs:5365`, `domains:["*"]` + `/specific` route probed at `/nomatch`) is fully isolated — its host always matches, so it never reaches the host-miss arm — and still passes. Both pass (`2 passed; 0 failed`).

**5. Fixture `0055`.** `envoy.yaml` vs `envoy-rust.yaml` differ ONLY in the four benign per-side deltas (admin block, listener bind `0.0.0.0`→`127.0.0.1`, `generate_request_id: false`, the mount log path); the vhost (`domains:["match.test"]`, non-wildcard), the catch-all route, and the `json_format` block are byte-identical. Probe is `host: nomatch.test`, `expected_status: 404` (a non-empty Host → the host-miss 404, NOT the codec-400 empty-Host path). The `json_format` logs only deterministic operators (`%RESPONSE_CODE%`, `%RESPONSE_CODE_DETAILS%`, `%REQ(:METHOD)%`, `%PROTOCOL%`) → the rendered line is byte-identical cross-proxy.

**6. The differential test** `access_log_rcd_host_not_found.rs` is a faithful clone of its phase-46 sibling `access_log_rcd_route_not_found.rs` (only the fn name + fixture dir differ); auto-discovered by CI's `cargo test --workspace` (`ci.yml:67`) — no manual registration gap (unlike a new fuzz target).

**7. BEHAVIOR_CONTRACT row.** The `%RESPONSE_CODE_DETAILS%` row accurately states BOTH route-walk synth-404 arms (route-miss + host-miss) now carry `route_not_found`, credits `0055` as the witness, **M46-1 CONSUMED**, with connect/overflow = M45-2 and H2 = M45-1 still deferred. Not over-claiming.

**8. Doctrine.** No new `Op` / `AccessLogRecord` field / crate / dependency / fuzz-target / `ConfigError` variant (`git diff` over `*.toml` and `crates/*/fuzz/*` is empty); `#![forbid(unsafe_code)]` intact (`crates/envoy-http1/src/lib.rs:1`). The additive invariant (→ `0001`-`0054` byte-identical) is CI-proven (run `28309736314`: 151 passed + 80 fixture binaries green incl. `0055`, h2spec, fuzz, deny).

## Minor findings (carry-forward — non-blocking)

- **M47-1 (line-number citation drift — cosmetic).** The in-code comment and the differential-test doc-comment cite the host-miss set-site as `:1535` and the route-miss arm as `:1553`, while the actual `return` statements sit at `:1536` and `:1555` (the cited lines are the comment/warn lines just above); the BEHAVIOR_CONTRACT uses yet another pair (`:1536` / `:1554`). Self-consistent enough to navigate by, but the three sources disagree by a line. Fix opportunistically if these files are touched again. (Folds into the broader cosmetic-citation carry-forward family; not load-bearing.)
- **(noted, no action)** The in-process backstop's `json_format` logs 2 keys (`rc`, `rcd`) vs the fixture's 4 (`method`/`proto`/`rc`/`rcd`). This is deliberate — the backstop is focused on the `rcd` field; the fixture carries the full cross-proxy contract. No action needed; recorded for completeness only.

## Disposition

No Critical/Important findings to fold. The two Minors are cosmetic and non-blocking → carried forward as **M47-1** (no code change this state-5 session → the commit stays doc-only and CI stays green). Phase 47 is APPROVED for state-6 close-out (flip ROADMAP row `47` → `done`). **M46-1 is CONSUMED.** The deferred carry-forwards stay live: **M45-1** (H2 failure-path details + an H2 access-log driver), **M45-2** (connect-failure/overflow non-deterministic detail strings); **M42-1** (the broader failure-path vocabulary) continues.
