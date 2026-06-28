# Phase 46 — `46-accesslog-rcd-route-not-found` — STATE-5 Code Review

**Reviewer:** fresh `superpowers:code-reviewer` subagent (no session history), dispatched over the phase-46 implementation diff `git diff 69bf726^..07e27ac` (7 files; the only `src/` change is the ONE-line edit in `crates/envoy-http1/src/hcm.rs` plus a backstop test) against `SPEC.md` / `PLAN.md` / `PROGRESS.md` / project doctrine.

## Overall verdict: ✅ APPROVED

A tiny, exemplary, well-scoped diff. Every claim in the SPEC/PLAN is independently verified against the actual code. No deviations from plan; no defects found.

## Findings by severity

- **Critical:** none
- **Important:** none
- **Minor:** none

## What was verified (confidence: high)

**1. The edit is on the correct arm (the check that matters most).** `hcm.rs:1554` — the no-matching-ROUTE arm (immediately after `tracing::warn!(… "request rejected: no matching route")`, inside the `vh.routes.iter().find(...)` `None` branch) — now returns `BuildOutcome::Synth(synth_404(close), Some("route_not_found"))`. The no-matching-VIRTUAL_HOST (host-miss) arm at `:1535` correctly STAYS `…None` (host-miss = deferred carry-forward M46-1; its detail is unwitnessed at v1.33.0 and must not be set).

**2. The route-miss 404 access-log line carries the detail.** Traced end-to-end: the route-miss `Synth(resp, Some("route_not_found"))` → the writer-arm (`:864-866`, `response_code_details_for_log = details.map(str::to_owned)`) → the `AccessLogRecord` built unconditionally below the match (`:1249`, `response_code_details: response_code_details_for_log`) → emitted to sinks (`:1261-1262`). Synth arms set `outgoing` and fall through to the unified write/log site — NO early return bypasses the access-log.

**3. The 404 is unchanged.** `synth_404` (`:1816`, delegates to `synth_status(404, close)`) and the 404 status/body/headers/flags are byte-untouched; the change is purely additive (sets a previously-`None` detail string).

**4. The backstop test** `h1_route_miss_access_log_carries_route_not_found_rcd` drives the full H1 dispatch with a `domains:["*"]` vhost (host-miss arm never hit) carrying a SINGLE `prefix:"/specific"` route, probes `GET /nomatch` (misses → route-miss arm), and asserts both the unchanged 404 (`HTTP/1.1 404 `, `content-length: 0`) AND the logged line `{"rc":404,"rcd":"route_not_found"}\n`. Genuine fail-first (the old `None` rendered `rcd:null`).

**5. The `0054` fixture** clones the `0050` `direct_response`/`clusters:[]` shape; `domains:["*"]` + a single `prefix:"/specific"` route; `/nomatch` probe with `expected_status: 404`. The two yamls differ ONLY in benign per-side deltas (admin, bind `0.0.0.0`→`127.0.0.1`, log path, `generate_request_id`) — the route table + vhost + `json_format` are byte-identical; the `json_format` logs only deterministic operators.

**6. The differential test** `access_log_rcd_route_not_found.rs` is a faithful clone of `access_log_rcd_no_healthy.rs` (only the doc-comment + fixture dir differ).

**7. The BEHAVIOR_CONTRACT row** is accurate (route-miss → `route_not_found` at `hcm.rs:1553`; host-miss deferred M46-1; connect/overflow M45-2; H2 M45-1).

**8. Doctrine.** `#![forbid(unsafe_code)]` intact (`lib.rs:1`); no new `Op`/`AccessLogRecord` field/crate/dependency/fuzz-target/`ConfigError` variant; the additive invariant (→ `0001`-`0053` byte-identical) is CI-proven (run `28308192315`: 151 passed + 79 fixture binaries green).

## Disposition

No Critical/Important findings to fold; no Minor findings to carry forward. Phase 46 is APPROVED for state-6 close-out (flip ROADMAP row `46` → `done`). The deferred carry-forwards stay live: **M46-1** (host-miss 404 detail), **M45-1** (H2 failure-path details + an H2 access-log driver), **M45-2** (connect-failure/overflow non-deterministic detail strings); **M42-1** (the broader failure-path vocabulary) continues.
