# Phase 47 — `47-accesslog-rcd-host-not-found` — PROGRESS

**Scope (ADR-0104):** differentially WITNESS the failure-path `%RESPONSE_CODE_DETAILS%` string — `route_not_found` — BYTE-EXACT on the HOST-miss (no-matching-virtual_host) 404 path, SETTING it at envoy-rust's H1 no-matching-virtual_host `synth_404` arm (`hcm.rs:1535`; the detail was `None`). **CONSUMES carry-forward M46-1.** The THIRD differentially-witnessed failure-path detail (after `no_healthy_upstream` phase 45 + the route-miss `route_not_found` phase 46).

## State-3 implementation (commits `60980d5`, `0a8c8ec`, `344f01e`)
- **T1 (`60980d5`)** — the ONLY `src/` change: at the H1 route-walk no-matching-virtual_host arm (`crates/envoy-http1/src/hcm.rs:1535`, after `"request rejected: no matching virtual_host"`), `BuildOutcome::Synth(synth_404(close), None)` → `BuildOutcome::Synth(synth_404(close), Some("route_not_found"))`. The route-miss arm `:1554` left UNCHANGED (phase 46). NO 404 status/body/headers/flags change. A TDD file-capture backstop (`h1_host_miss_access_log_carries_route_not_found_rcd`, a `domains:["match.test"]` vhost + a `Host: nomatch.test` request) went red (`{"rc":404,"rcd":null}`) → green (`{"rc":404,"rcd":"route_not_found"}`); `envoy-http1` 141 passed (both the host-miss + the phase-46 route-miss backstops green).
- **T2 (`0a8c8ec`)** — fixture `tests/fixtures/0055-accesslog-rcd-host-not-found/` (a `direct_response`/`clusters:[]` listener from the `0054` template, a vhost `domains:["match.test"]` [NON-wildcard] + a catch-all `prefix:"/"` route; a `json_format` logging `%RESPONSE_CODE_DETAILS%`/`rcd` + `%RESPONSE_CODE%`/`%REQ(:METHOD)%`/`%PROTOCOL%`; a `host: nomatch.test` probe with `expected_status: 404`) + the differential test `tests/differential/tests/access_log_rcd_host_not_found.rs`.
- **T3 (`344f01e`)** — BEHAVIOR_CONTRACT `%RESPONSE_CODE_DETAILS%` row (`:1031`) → both route-walk 404 arms now carry `route_not_found` (host-miss `:1536` + route-miss `:1554`); **M46-1 CONSUMED**. Fuzz seed SKIPPED — `response_code_details.yaml` already covers `%RESPONSE_CODE_DETAILS%` (phase 42). fmt clean (no `style` commit needed).

## State-4 §7.5 verification gate — GREEN

**AUTHORITATIVE evidence: GitHub Actions CI run `28309736314`** (branch `main`, headSha `bfca9e30d8eb6b77c322fc987eb21434c739569b`, conclusion **success**, 2026-06-28T03:12:16Z → 03:17:25Z). Both jobs green: `build + test + lint` (5m6s) + `fuzz` (3m59s).

| §7.5 item | Result | Evidence (verbatim from run `28309736314`) |
|---|---|---|
| (a) fixture `0055` green | ✅ | `test access_log_rcd_host_not_found ... ok` — both proxies emit the byte-identical 404 line `{"method":"GET","proto":"HTTP/1.1","rc":404,"rcd":"route_not_found"}` (the host-miss synth-404 via the `domains:["match.test"]` vhost + the `Host: nomatch.test` probe). |
| (b) all `0001`-`0054` green simultaneously | ✅ | `test result: ok. 151 passed; 0 failed; 2 ignored` (workspace) + 80 differential fixture binaries each `test result: ok. 1 passed` (79 at phase 46 + the new `0055`) — additive phase (the detail set on a previously-`None` host-miss arm) → all prior fixtures byte-identical. |
| (c) h2spec ≥95% | ✅ | `test h2spec_pass_rate_gate ... ok` (`tests/h2spec_runner.rs`; h2spec v2.6.0). |
| (d) fuzz clean | ✅ | `fuzz` job green: `parse_bootstrap` + `jwt_parse` + `cdn_loop_parse` + `accesslog_format_parse` (30s each) all completed without a crash. |
| (e) build/clippy/fmt/test/deny clean | ✅ | the `build + test + lint` job is green end-to-end (`fmt` ✓ — clean at state-3; `clippy` ✓; `build` ✓; `test` ✓); `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok`. |
| (f) `REVIEW.md` | deferred | state-5 code-review (next session). |

**Doctrine:** `#![forbid(unsafe_code)]` holds; NO new crate/dependency/fuzz-target; NO new `Op`/`AccessLogRecord` field/`ConfigError` variant; the ONLY `src/` change is the `:1535` `Some("route_not_found")` detail set. **M46-1 CONSUMED.** **§6.1 split did NOT fire** (3 tasks); **ADR-0105 reserved-but-UNFIRED**. Ledger head ADR-0104.

_Next: state-5 code-review (`superpowers:requesting-code-review`) → `REVIEW.md`._
