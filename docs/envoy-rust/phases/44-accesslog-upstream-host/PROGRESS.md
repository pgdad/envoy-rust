# Phase 44 — `44-accesslog-upstream-host` — PROGRESS

**Scope (ADR-0101):** differentially WITNESS the `%UPSTREAM_HOST%` access-log command operator (the resolved upstream endpoint `<ip>:<port>`) BYTE-EXACT on a real upstream — the gap fixture `0051` deliberately excluded. FIXTURE-ONLY (witnesses an operator implemented since phase 06); NO `src/` change.

## State-3 implementation (commits `9100feb`, `b8d6ea8`)
- **T1 (`9100feb`)** — fixture `tests/fixtures/0052-accesslog-upstream-host/` (`envoy.yaml`/`envoy-rust.yaml`/`expectations.yaml`/`README.md`): a STATIC `{{BACKEND_IP}}` single-endpoint `{{HTTP1_BACKEND_PORT}}` cluster (from `0036`; NOT `0051`'s STRICT_DNS; NOT `0036`'s `_1_`/`_2_` markers) + a `json_format` logging `%UPSTREAM_HOST%` (`uh`) plus the deterministic anchors `%UPSTREAM_CLUSTER%`/`%RESPONSE_CODE_DETAILS%`/`%REQ(:METHOD)%`/`%PROTOCOL%`; the `http1_access_log_byte_exact` driver (cross-proxy-EQUALITY, NO static literal). Plus the differential test `tests/differential/tests/access_log_upstream_host.rs`.
- **T2 (`b8d6ea8`)** — BEHAVIOR_CONTRACT `%UPSTREAM_HOST%` row (`:1029`) → DIFFERENTIALLY WITNESSED byte-exact by `0052`. Fuzz seed SKIPPED — `crates/envoy-config/fuzz/corpus/parse_bootstrap/json_format_logger.yaml:27` already carries `%UPSTREAM_HOST%` (plan-review M1).

## State-4 §7.5 verification gate — GREEN

**AUTHORITATIVE evidence: GitHub Actions CI run `28304511513`** (branch `main`, headSha `7e46e9f8ff42e1e220ae8ee9e9c5a81a646cc2cd`, conclusion **success**, 2026-06-27T23:02:46Z → 23:07:56Z). Both jobs green: `build + test + lint` (5m5s) + `fuzz` (4m1s).

| §7.5 item | Result | Evidence (verbatim from run `28304511513`) |
|---|---|---|
| (a) fixture `0052` green | ✅ | `test access_log_upstream_host ... ok` (`Running tests/access_log_upstream_host.rs`, 23:05:08) — both proxies emit the byte-identical `%UPSTREAM_HOST%` line via the shared `{{BACKEND_IP}}:{{HTTP1_BACKEND_PORT}}`. |
| (b) all `0001`-`0051` green simultaneously | ✅ | `test result: ok. 151 passed; 0 failed; 2 ignored` (workspace) + every differential fixture binary `test result: ok. 1 passed` — FIXTURE-ONLY phase (no operator/record-field/`src/` change) → all prior fixtures stay byte-identical. |
| (c) h2spec ≥95% | ✅ | `test h2spec_pass_rate_gate ... ok` (`tests/h2spec_runner.rs`; h2spec v2.6.0). |
| (d) fuzz clean | ✅ | `fuzz` job green: `parse_bootstrap` + `jwt_parse` + `cdn_loop_parse` + `accesslog_format_parse` (30s each) all completed without a crash. |
| (e) build/clippy/fmt/test/deny clean | ✅ | `fmt` ✓, `clippy` ✓, `build` ✓, `test` ✓; `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok`. |
| (f) `REVIEW.md` | deferred | state-5 code-review (next session). |

**Note:** the usual "State-4 = CI's first real execution" red-at-fmt did NOT occur — the hand-authored `access_log_upstream_host.rs` was already fmt-clean (`cargo fmt --check` exit 0 locally), so the state-3 push (`7e46e9f`) went green on the first CI run, which IS the §7.5-authoritative run (it ran the full Docker differential incl. `0052`). No `style: cargo fmt` commit was needed.

**Doctrine:** `#![forbid(unsafe_code)]` holds; NO new crate/dependency/fuzz-target; NO new `Op`/`AccessLogRecord` field/`ConfigError` variant; NO `src/` change. **§6.1 split did NOT fire** (2 tasks); **ADR-0102 reserved-but-UNFIRED**. Ledger head ADR-0101.

_Next: state-5 code-review (`superpowers:requesting-code-review`) → `REVIEW.md`._
