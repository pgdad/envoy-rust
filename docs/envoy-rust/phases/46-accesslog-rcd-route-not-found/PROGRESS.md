# Phase 46 — `46-accesslog-rcd-route-not-found` — PROGRESS

**Scope (ADR-0103):** differentially WITNESS the failure-path `%RESPONSE_CODE_DETAILS%` string — `route_not_found` — BYTE-EXACT on the route-miss 404 path, SETTING it at envoy-rust's H1 no-matching-route `synth_404` arm (`hcm.rs:1553`; the detail was `None`). The SECOND clean failure-path detail (after phase 45's `no_healthy_upstream`), continuing carry-forward M42-1.

## State-3 implementation (commits `69bf726`, `f6e08de`, `07e27ac`)
- **T1 (`69bf726`)** — the ONLY `src/` change: at the H1 route-walk no-matching-route arm (`crates/envoy-http1/src/hcm.rs:1553`, after `"request rejected: no matching route"`), `BuildOutcome::Synth(synth_404(close), None)` → `BuildOutcome::Synth(synth_404(close), Some("route_not_found"))`. The host-miss arm `:1535` left `None` (M46-1). The writer-arm (`:864-866`) threads the detail → `response_code_details_for_log`; the record is built unconditionally (`:1247`). NO 404 status/body/headers/flags change. A TDD file-capture backstop (`h1_route_miss_access_log_carries_route_not_found_rcd`) went red (`{"rc":404,"rcd":null}`) → green (`{"rc":404,"rcd":"route_not_found"}`); `envoy-http1` 140 passed.
- **T2 (`f6e08de`)** — fixture `tests/fixtures/0054-accesslog-rcd-route-not-found/` (a `direct_response` listener from the `0050` template, `clusters:[]`, no upstream + a `domains:["*"]` vhost + a single `prefix:"/specific"` route; a `json_format` logging `%RESPONSE_CODE_DETAILS%`/`rcd` + `%RESPONSE_CODE%`/`%REQ(:METHOD)%`/`%PROTOCOL%`; a `/nomatch` probe with `expected_status: 404`) + the differential test `tests/differential/tests/access_log_rcd_route_not_found.rs`.
- **T3 (`07e27ac`)** — BEHAVIOR_CONTRACT `%RESPONSE_CODE_DETAILS%` row (`:1031`) → route-miss 404 path witnessed by `0054`. Fuzz seed SKIPPED — `response_code_details.yaml` already covers `%RESPONSE_CODE_DETAILS%` (phase 42). fmt clean (no `style` commit needed).

## State-4 §7.5 verification gate — GREEN

**AUTHORITATIVE evidence: GitHub Actions CI run `28308192315`** (branch `main`, headSha `074cae1159be8b2ea3afe178353450c3f181e72a`, conclusion **success**, 2026-06-28T01:57:16Z → 02:04:41Z). Both jobs green: `build + test + lint` (5m3s) + `fuzz` (7m22s).

| §7.5 item | Result | Evidence (verbatim from run `28308192315`) |
|---|---|---|
| (a) fixture `0054` green | ✅ | `test access_log_rcd_route_not_found ... ok` — both proxies emit the byte-identical 404 line `{"method":"GET","proto":"HTTP/1.1","rc":404,"rcd":"route_not_found"}` (the route-miss synth-404 via the single-`/specific`-route + `/nomatch` probe). |
| (b) all `0001`-`0053` green simultaneously | ✅ | `test result: ok. 151 passed; 0 failed; 2 ignored` (workspace) + 79 differential fixture binaries each `test result: ok. 1 passed` (78 at phase 45 + the new `0054`) — additive phase (the detail set on a previously-`None` route-miss arm) → all prior fixtures byte-identical. |
| (c) h2spec ≥95% | ✅ | `test h2spec_pass_rate_gate ... ok` (`tests/h2spec_runner.rs`; h2spec v2.6.0). |
| (d) fuzz clean | ✅ | `fuzz` job green: `parse_bootstrap` + `jwt_parse` + `cdn_loop_parse` + `accesslog_format_parse` (30s each) all completed without a crash. |
| (e) build/clippy/fmt/test/deny clean | ✅ | the `build + test + lint` job is green end-to-end (`fmt` ✓ — clean at state-3; `clippy` ✓; `build` ✓; `test` ✓); `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok`. |
| (f) `REVIEW.md` | deferred | state-5 code-review (next session). |

**Doctrine:** `#![forbid(unsafe_code)]` holds; NO new crate/dependency/fuzz-target; NO new `Op`/`AccessLogRecord` field/`ConfigError` variant; the ONLY `src/` change is the `:1553` `Some("route_not_found")` detail set. The host-miss 404 detail is deferred (M46-1). **§6.1 split did NOT fire** (3 tasks); **ADR-0104 reserved-but-UNFIRED**. Ledger head ADR-0103.

_Next: state-5 code-review (`superpowers:requesting-code-review`) → `REVIEW.md`._
