# Phase 45 — `45-accesslog-rcd-no-healthy` — PROGRESS

**Scope (ADR-0102):** differentially WITNESS the FIRST FAILURE-path `%RESPONSE_CODE_DETAILS%` string — `no_healthy_upstream` — BYTE-EXACT on the no-healthy-upstream 503 path, SETTING it at envoy-rust's H1 no-healthy synth arm (the detail was `None`). Opens the failure-path slice of carry-forward M42-1.

## State-3 implementation (commits `8f35dd8`, `58cea17`, `30442f5`, `77d4966`)
- **T1 (`8f35dd8`)** — the ONLY `src/` change: at the H1 `BuildOutcome::Proxy` arm (`crates/envoy-http1/src/hcm.rs:~990`), an `else` on the existing `if let Some(endpoint) = attempt.endpoint { … = Some("via_upstream") }` sets `response_code_details_for_log = Some("no_healthy_upstream".to_owned())` on the `pick()->None` no-healthy path (recon-confirmed EXCLUSIVE to no-healthy — `hcm.rs:438` is the only `endpoint: None`; non-retriable so the terminal value wins). NO `AttemptResult` struct / 503 status / body / headers / flags change. A TDD file-access-log-capture backstop (`h1_no_healthy_access_log_carries_no_healthy_upstream_rcd`) went red (`{"rc":503,"rcd":null}`) → green (`{"rc":503,"rcd":"no_healthy_upstream"}`); `envoy-http1` 139 passed.
- **T2 (`58cea17`)** — fixture `tests/fixtures/0053-accesslog-rcd-no-healthy/` (a STATIC `subset_cluster` with `lb_subset_config { fallback_policy: NO_FALLBACK, subset_selectors: [{ keys: [stage] }] }`, ONE literal `127.0.0.1:1` endpoint `{stage: prod}` + a route `metadata_match: {stage: nonexistent}` → subset-miss → 503; a `json_format` logging `%RESPONSE_CODE_DETAILS%`/`rcd` + `%RESPONSE_CODE%`/`%REQ(:METHOD)%`/`%PROTOCOL%`; `expected_status: 503` probe) + the differential test `tests/differential/tests/access_log_rcd_no_healthy.rs`.
- **T3 (`30442f5`)** — BEHAVIOR_CONTRACT `%RESPONSE_CODE_DETAILS%` row (`:1031`) → no-healthy failure path witnessed by `0053`. Fuzz seed SKIPPED — `crates/envoy-config/fuzz/corpus/parse_bootstrap/response_code_details.yaml` already covers `%RESPONSE_CODE_DETAILS%` (phase 42).
- **`77d4966`** — `style: cargo fmt` (rustfmt wrapped the long `else`-branch line) → fmt clean, so the state-3 push went green on the first CI run (no deferred red-at-fmt this phase).

## State-4 §7.5 verification gate — GREEN

**AUTHORITATIVE evidence: GitHub Actions CI run `28306490466`** (branch `main`, headSha `77d496662eb3f85c51555c8bdaede4a1f0905ba4`, conclusion **success**, 2026-06-28T00:34:18Z → 00:39:17Z). Both jobs green: `build + test + lint` (4m56s) + `fuzz` (3m56s).

| §7.5 item | Result | Evidence (verbatim from run `28306490466`) |
|---|---|---|
| (a) fixture `0053` green | ✅ | `test access_log_rcd_no_healthy ... ok` (`Running tests/access_log_rcd_no_healthy.rs`, 00:36:24) — both proxies emit the byte-identical 503 line `{"method":"GET","proto":"HTTP/1.1","rc":503,"rcd":"no_healthy_upstream"}` via the NO_FALLBACK subset-miss. |
| (b) all `0001`-`0052` green simultaneously | ✅ | `test result: ok. 151 passed; 0 failed; 2 ignored` (workspace) + 78 differential fixture binaries each `test result: ok. 1 passed` (was 77 at phase 44 + the new `0053`) — additive phase (the detail set on a previously-`None` no-healthy arm) → all prior fixtures byte-identical. |
| (c) h2spec ≥95% | ✅ | `test h2spec_pass_rate_gate ... ok` (`tests/h2spec_runner.rs`; h2spec v2.6.0). |
| (d) fuzz clean | ✅ | `fuzz` job green: `parse_bootstrap` + `jwt_parse` + `cdn_loop_parse` + `accesslog_format_parse` (30s each) all completed without a crash. |
| (e) build/clippy/fmt/test/deny clean | ✅ | the `build + test + lint` job is green end-to-end (`fmt` ✓ — applied at `77d4966`; `clippy` ✓; `build` ✓; `test` ✓); `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok`. |
| (f) `REVIEW.md` | deferred | state-5 code-review (next session). |

**Doctrine:** `#![forbid(unsafe_code)]` holds; NO new crate/dependency/fuzz-target; NO new `Op`/`AccessLogRecord` field/`ConfigError` variant; the ONLY `src/` change is the no-healthy `else`-branch detail set in `hcm.rs`. **§6.1 split did NOT fire** (3 tasks); **ADR-0103 reserved-but-UNFIRED**. Ledger head ADR-0102.

_Next: state-5 code-review (`superpowers:requesting-code-review`) → `REVIEW.md`._
