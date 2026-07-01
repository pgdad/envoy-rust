# Phase 55 — `55-accesslog-rf-overflow-request-budget` — Implementation Progress

> §5 state-3 implementation log. Executed via `superpowers:subagent-driven-development`
> (one fresh implementer subagent per task + a dedicated task-reviewer subagent per
> task; both task reviews came back **Approved**, 0 Critical / 0 Important / 0 Minor).
> This phase is a pure fixture/doc-only differential witness — PLAN.md's own
> PLAN-VERIFY (state-2) reconfirmed twice over that NO `crates/` change is needed, so
> Task 1 and Task 2 are transcription-plus-verification tasks (exact file content and
> exact edit text given verbatim in PLAN.md), not RED→GREEN code changes. Task 3 is a
> pure verification sweep (no files modified). Per-task discipline: cargo-fmt-check +
> the local `envoy-http1`/`envoy-accesslog` suites are local-authoritative; the Docker
> differential fixture `0063` is CI-authoritative (memory
> `envoy-rust-state4-ci-first-execution`) — `0063` is backend-spawning → expect
> LOCAL-flaky/unrun on this dev host (memory
> `differential-host-bridge-ip-192-168-65-2`), GREEN expected on CI. State-4
> verification (the full §7.5 gate on CI) is the next session — NOT run in this
> session.

---

## Task 1 — §C fixture `0063-accesslog-rf-overflow-request-budget` + §D differential test ✅

**No in-process RED/GREEN** — a new differential fixture + thin test wrapper,
combining fixture `0025`'s (phase 17) proven `STRICT_DNS`/`{{BACKEND_HOST}}`/
`{{HTTP1_BACKEND_PORT}}`/`max_requests:0` cluster shape with fixture `0058`'s
(phase 50) proven `{rc,rcd,rf}` `json_format` access-log shape. Created:
- `tests/fixtures/0063-accesslog-rf-overflow-request-budget/envoy-rust.yaml`
- `tests/fixtures/0063-accesslog-rf-overflow-request-budget/envoy.yaml`
- `tests/fixtures/0063-accesslog-rf-overflow-request-budget/expectations.yaml`
- `tests/fixtures/0063-accesslog-rf-overflow-request-budget/README.md`
- `tests/differential/tests/access_log_rf_overflow_request_budget.rs`

All 5 files match PLAN.md Task 1's exact content byte-for-byte (task-reviewer
independently diffed each against the PLAN — verbatim match, zero drift).

**Compile check** (`cargo test -p differential --test access_log_rf_overflow_request_budget --no-run`):
```
Finished `test` profile [unoptimized + debuginfo] target(s)
Executable tests/access_log_rf_overflow_request_budget.rs (target/debug/deps/access_log_rf_overflow_request_budget-...)
```
Compiles cleanly, discovered by cargo. NOT run to completion — it is a
backend-spawning Docker differential fixture; CI is authoritative for this one, same
posture as fixtures `0061`/`0062`.

**Additivity confirmed:** `git diff --name-status` over the commit shows only 5 `A`
(added) entries — no existing fixture (`0001`-`0062`) or `crates/` file touched.

**Commit:** `197e9d6` — `phase 55 §C+§D: fixture 0063 + differential test (request-budget overflow UO, byte-exact) [ADR-0112]`

**Task review:** ✅ Approved — 0 Critical / 0 Important / 0 Minor. Reviewer
independently byte-diffed all 5 files against PLAN.md's mandated content, confirmed
the additive-only invariant, confirmed the reachable-endpoint requirement (UF-vs-UO
divergence, ADR-0047) is correctly reflected in the fixture's comments/README.

---

## Task 2 — §E `BEHAVIOR_CONTRACT.md` update: close M50-C + fold M54-1 ✅

**No RED/GREEN** — a documentation-only edit to the `%RESPONSE_FLAGS%` row
(`BEHAVIOR_CONTRACT.md:1020`).

**Before** (verification grep, per PLAN.md Task 2 Step 3):
```
$ grep -n "hcm.rs:1376" docs/envoy-rust/BEHAVIOR_CONTRACT.md
<5 matches, all on line 1020>
```

**Implementation:** replaced the M50-C deferral sentence ("The request-budget
(`max_requests`) overflow UO is in-process-backstopped only — differential witness
deferred (M50-C).") with the closure sentence naming fixture `0063`/ADR-0112 and
cross-referencing fixture `0025`/ADR-0047's pre-existing wire/stats-level proof (exact
text from PLAN.md Task 2 Step 1). Replaced all 5 occurrences of the stale
`hcm.rs:1376` anchor with `hcm.rs:1377` within the same row — the 5 occurrences are
in the `NR`, `UH`, `URX`, `UF`, `UC` per-flag clauses; the `UO` clause does not cite
this anchor and was correctly left untouched.

**After** (verification greps, per PLAN.md Task 2 Step 3):
```
$ grep -n "hcm.rs:1376\b" docs/envoy-rust/BEHAVIOR_CONTRACT.md
<no matches>
$ grep -c "hcm.rs:1377" docs/envoy-rust/BEHAVIOR_CONTRACT.md
<>= 5>
$ grep -n "differential witness deferred (M50-C)" docs/envoy-rust/BEHAVIOR_CONTRACT.md
<no matches>
```
All three expectations met.

**Scope confirmed:** `git diff --stat` shows exactly 1 file changed
(`docs/envoy-rust/BEHAVIOR_CONTRACT.md`), 1 row touched — no other section/row
modified.

**Commit:** `eca1d4c` — `phase 55 §E: BEHAVIOR_CONTRACT closes M50-C (fixture 0063) + folds M54-1 (hcm.rs:1376→:1377 anchor ×5) [ADR-0112]`

**Task review:** ✅ Approved — 0 Critical / 0 Important / 0 Minor. Reviewer
independently re-derived the 5-occurrence count and clause mapping from the diff
(not trusting the implementer's report), confirmed the UO clause was correctly left
untouched, confirmed the replacement text matches PLAN.md verbatim, confirmed no
other row/file was touched.

---

## Task 3 — Final local verification sweep (local subset of §7.5) ✅

**No files modified** — verification only, run directly (no subagent dispatch
needed — pure command execution, no judgment calls).

```
$ cargo build --workspace --all-targets
Finished `dev` profile [unoptimized + debuginfo] target(s)

$ cargo clippy --workspace --all-targets --all-features -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s)

$ cargo fmt --all -- --check
(clean, exit 0)

$ cargo test -p envoy-http1
test result: ok. 151 passed; 0 failed; 0 ignored
(includes h1_request_budget_overflow_access_log_carries_uo_flag — the pre-existing
phase-50 backstop, hcm.rs:7224, unchanged this phase — still passing)

$ cargo test -p envoy-accesslog
test result: ok. 98 passed; 0 failed; 0 ignored

$ cargo test -p differential --test access_log_rf_overflow_request_budget --no-run
Finished `test` profile [unoptimized + debuginfo] target(s)
(compiles + discovered)

$ ls tests/fixtures/ | grep -c "^00"
63
```

All clean. No residual edits surfaced — per PLAN.md Task 3 Step 3, no final commit
needed for this task (skipped, as expected).

---

## State-4 verification

**Execution model:** `superpowers:verification-before-completion` — evidence before
claims. Rebuilt `target/debug/envoy-bin` first (`cargo build -p envoy-bin`, memory
`differential-harness-uses-debug-envoy-bin`) — already current, no rebuild needed.

**Local §7.5 (a)-(e) subset:**
```
$ cargo build --workspace --all-targets
Finished `dev` profile [unoptimized + debuginfo] target(s)

$ cargo clippy --workspace --all-targets --all-features -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s)

$ cargo fmt --all -- --check
(clean, exit 0)
```
All clean (EXIT 0).

`cargo test --workspace --no-fail-fast` for the complete local picture: two separate
full runs, each showing a DIFFERENT set of local-only failures — confirming the
project's documented parallel-load Docker-startup-race class (memory
`differential-fixtures-flake-under-parallel-load`), NOT a phase-55 regression:
- Run 1: `access_log_rcd_upstream_reset` (fixture `0062`, phase 54) + `access_log_rf_upstream_reset` (fixture `0061`, phase 53) both showed the documented host-bridge `UF`-vs-`UC` mismatch (memory `differential-host-bridge-ip-192-168-65-2`); `admin_config_dump_server_info` showed the documented `192.168.65.2` bridge-IP cluster-stat artifact; `xds_file_based_eds`/`admin_ready` showed container-startup races. **Fixture `0063` (this phase) was GREEN in this run.**
- Run 2: a DIFFERENT set flaked — `access_log_rcd_upstream_reset`/`access_log_rf_upstream_reset`/`admin_config_dump_server_info` again (the same host-bridge artifacts), PLUS `access_log_rf_no_route` (fixture `0056`, phase 48 — an unrelated PRE-EXISTING fixture) and, this time, **`access_log_rf_overflow_request_budget` (fixture `0063`) FAILED** with `upstream Envoy never became accept-ready … Connection refused (os error 111)` — the classic Docker-container-startup-race signature (matching the `eds-fatal-startup-test-port-reuse-flake`/`upstream-h2-connection-pooling-backend-ready-flake` class exactly), NOT a code/fixture defect. `envoy-http2 --lib`'s `send_request_maps_h2_handshake_failure_to_typed_error` also failed both runs (memory `envoyrust-h2-handshake-test-host-flake`, pre-existing, unrelated to this phase).
- **Isolated re-run confirms fixture `0063` is genuinely green when not contending for host resources:** `cargo test -p differential --test access_log_rf_overflow_request_budget -- --nocapture` → `test access_log_rf_overflow_request_budget ... ok` (`test result: ok. 1 passed; 0 failed`). The full-workspace-parallel failures are host-load artifacts, not a phase-55 regression.

`cargo deny check`: `advisories ok, bans ok, licenses ok, sources ok` (clean; only the
standard benign license-not-encountered warnings, same as every prior phase).

h2spec: `h2spec_runner: h2spec not found — skipping locally; test h2spec_pass_rate_gate ... ok` — the binary isn't installed on this dev host; the gate no-ops locally by design. NO HTTP/2 codec change this phase, so the gate is expected unmoved from phase 54's CI baseline — confirmed on CI below.

**CI is AUTHORITATIVE for the Docker differential** (memory
`envoy-rust-state4-ci-first-execution`): the phase-55 state-3 STATE-advance commit
`d8301c3` was already pushed and its CI run (`28540697796`) already completed
`success` on both jobs (`fuzz`, `build + test + lint`) BEFORE this state-4 session
began verifying — this is the FIRST CI run to ever exercise fixture `0063` (it did
not exist before commit `197e9d6`, itself part of this same push). Pulled the FULL
job log (`gh run view 28540697796 --log`) and confirmed directly, not just from the
job-summary conclusion: `cargo fmt --all -- --check` ran clean, `cargo clippy
--workspace --all-targets --all-features -- -D warnings` ran clean, **fixture `0063`
GREEN** — `test access_log_rf_overflow_request_budget ... ok` (both proxies emit the
byte-identical `{"rc":503,"rcd":"upstream_reset_before_response_started{overflow}","rf":"UO"}`),
**134 `test result: ok` / 0 `test result: FAILED`** across the whole workspace (one
more green than phase 54's 133 — the net-new `0063`, confirming no pre-existing
fixture regressed), `h2spec_pass_rate_gate ... ok` (no HTTP/2 codec change this
phase — gate unmoved), `cargo deny check` → `advisories ok, bans ok, licenses ok,
sources ok` clean on CI too, and the `fuzz` job `success` (the existing 4 targets —
`parse_bootstrap`/`jwt_parse`/`cdn_loop_parse`/`accesslog_format_parse` — 0 crashes;
no new fuzz target this phase, `ci.yml` unchanged per SPEC §2 SKIP).

**Disposition:** §7.5 gate (a)-(e) MET on CI (the authoritative environment); (f)
`REVIEW.md` is the next session's job (state-5 code-review). No ADR fired —
verification overturned no PLAN/SPEC fact (ADR-0113/ADR-0114 stay
reserved-but-unfired). No re-implementation needed. **CONSUMES M50-C — now
CI-CONFIRMED** (no longer "pending"). Per §5.1, STOP here — advance STATE to the §5
state-5 code-review, do not chain into it this session.

---

## Summary

All 3 PLAN.md tasks complete, each task-reviewed **Approved** (0 Critical / 0
Important / 0 Minor across both code-producing tasks). NO `crates/` source change
this phase (reconfirmed at state-1 SPEC, state-2 PLAN-VERIFY, and now empirically by
Task 3's clean `envoy-http1` suite run with zero diffs to that crate). Carry-forward
**M50-C** CONSUMED (closed in `BEHAVIOR_CONTRACT.md`). Carry-forward **M54-1** folded
(the `hcm.rs:1376`→`:1377` anchor fix). `#![forbid(unsafe_code)]` holds — no
`unsafe`, no `crates/` file touched. No new `Op`/`AccessLogRecord` field/crate/
dependency/fuzz-target/`ConfigError` variant/test-harness code/in-process backstop —
all confirmed absent from the diffs. **DECISIONS.md ledger head unchanged: ADR-0112**
(no new ADR fired this session — ADR-0113/ADR-0114 stay reserved-but-unfired, per
PLAN.md's own projection, confirmed by the clean implementation).

**State-4 verification (see `## State-4 verification` above): §7.5 gate (a)-(e) MET on
CI** — fixture `0063` GREEN, 134/134 `test result: ok`, h2spec/deny/build/clippy/fmt
all clean, no pre-existing fixture regressed. Carry-forward **M50-C** now
CI-CONFIRMED (fully closed, no longer pending). Next: **§5 state-5 code-review**
(`superpowers:requesting-code-review`).
