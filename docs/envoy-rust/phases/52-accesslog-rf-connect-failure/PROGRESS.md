# Phase 52 — `52-accesslog-rf-connect-failure` — PROGRESS

> **State-3 implementation log** (`superpowers:executing-plans` + `superpowers:test-driven-development`). Running log, appended per task. The state-4 verification (the FULL §7.5 gate incl. the `0060` Docker differential on CI) is the SESSION AFTER (§5.1: one state per session).

**Pick (ADR-0109):** differentially witness the FIFTH non-`-` `%RESPONSE_FLAGS%` value `UF` (UpstreamConnectionFailure) byte-exact on the upstream-connect-refused 503 path, AND correct envoy-rust's connect-failure synth status 502→503 to match Envoy.

**ADR-0111 did NOT fire** — the state-2 §6.2 recon confirmed every §A–§G fact, and the state-3 implementation uncovered no SPEC-overturning surprise. Ledger head stays **ADR-0109**. §6.1 split did NOT fire (ADR-0110 stays reserved-but-unfired).

---

## Task 1 — connect-failure synth status 502→503 (3 arms + 3 warns + 3 tests) ✅

- **Files:** `crates/envoy-http1/src/hcm.rs`.
- **TDD:** flipped the three connect-failure unit-test assertions to expect `503 Service Unavailable` (+ their comments/messages) → ran → **all three FAILED** with actual `HTTP/1.1 502 Bad Gateway` (RED confirmed) → changed `synth_status(502, close)`→`synth_status(503, close)` at the three `AcquireOutcome::ConnectFailure` arms (`hcm.rs:501`/`:530`/`:547`) + the three `tracing::warn!` strings (`:499`/`:528`/`:545`, "returning 502"→"returning 503") → ran → **all three PASSED**.
- **Untouched (per §A/§4):** the reset/send-fail arm `synth_status(502, close)` at `:618` + its warn `:615` ("upstream request failed — returning 502") + the `AttemptOutcome::Reset` path — deferred M52-1 (`UC`).
- **Tests flipped:** `route_walk_returns_upstream_connect_on_refused_port`, `connect_failure_retried_on_connect_failure_policy`, `connect_failure_synth_does_not_tick_upstream_rq_5xx`. The co-located `rq_5xx 0` / `rq_total 0` assertions kept their VALUES (a connect-failure synth has no real upstream response → still does not tick `upstream_rq_5xx`, regardless of 502→503).
- **Verify:** `cargo test -p envoy-http1` → **147 passed; 0 failed** (no collateral).
- **Commit:** `2d42490` "phase 52 task 1: connect-failure synth status 502->503 (3 arms + warns + 3 tests)".

## Task 2 — thread `connect_failure_for_log` + render `UF` + in-process backstop ✅

- **Files:** `crates/envoy-http1/src/hcm.rs`.
- **TDD:** added the §F backstop `h1_connect_failure_access_log_carries_uf_flag` (a kernel-refused `127.0.0.1:1` endpoint, NO retry_policy, `{rc,rf}` FILE json access-log; mirrors the URX backstop verbatim minus rcd) → ran → **FAILED** at the final `assert_eq!` with `{"rc":503,"rf":"-"}` (503 from Task 1, but no `UF` derive branch yet → `via_upstream` falls to `_ => "-"`; RED confirmed) → implemented → ran → **PASSED** (`{"rc":503,"rf":"UF"}`).
- **Implementation:**
  - Declared `let mut connect_failure_for_log = false;` after `retry_limit_exceeded_for_log` (`hcm.rs:~854`).
  - Added loop-scoped `#[allow(unused_assignments)] let mut final_outcome: Option<AttemptOutcome> = None;` after `final_retriable` (`~:982`) + per-iteration capture `final_outcome = attempt.outcome;` immediately before the `final_retriable = match attempt.outcome` decision (`~:1083`). Required because the loop `break` carries only `(response, upstream_response)`, NOT `attempt.outcome`. `AttemptOutcome` is `Copy` (imported `hcm.rs:12` — no new `use`).
  - Set `connect_failure_for_log = matches!(final_outcome, Some(AttemptOutcome::ConnectFailure));` post-loop, AFTER the `if attempts > 1 && !retry_budget_blocked { … }` retry-split block, before `drop(retry_guard_slot)` (`~:1173`). Unconditional (a single no-retry connect-failure has `attempts==1`).
  - Added `else if connect_failure_for_log { "UF" }` between the `URX` branch and the rcd-`match` in the `%RESPONSE_FLAGS%` derive (`hcm.rs:1343`), + extended the derive comment block.
- **Verify:** `cargo test -p envoy-http1` → **148 passed; 0 failed** — the URX/UO/UH/NR in-process backstops unchanged (their paths never set `connect_failure_for_log`).
- **Commit:** `4d6774e` "phase 52 task 2: thread connect_failure_for_log + render UF + in-process backstop".

## Task 3 — fixture `0060-accesslog-rf-connect-failure` ✅

- **Files (created):** `tests/fixtures/0060-accesslog-rf-connect-failure/{envoy.yaml,envoy-rust.yaml,expectations.yaml,README.md}`.
- The 0058 dead-endpoint pattern **minus `circuit_breakers`** (so both proxies DIAL `127.0.0.1:1` and the kernel refuses → connect-failure synth-503, instead of 0058's pending-gate pre-connect reject). `json_format` reduced to `{rc, rf}` (rcd OMITTED — the connect-failure rcd is the non-deterministic transport-failure reason). No backend spawned (literal `127.0.0.1:1`). ONE probe: `GET /`, `expected_status: 503`, whole-line `{"rc":503,"rf":"UF"}`.
- **Verify:** `diff envoy.yaml envoy-rust.yaml` → exactly THREE hunks (the `admin:` line, `0.0.0.0` vs `127.0.0.1` bind, `0060-envoy-mount` vs `0060-envoy-rust-mount` log path). Cluster/route/json_format byte-identical (the cluster comment is present in BOTH files — the 0058 discipline).
- **Commit:** `4ecbea9` "phase 52 task 3: fixture 0060 (connect-failure UF, 0058 pattern minus circuit_breakers)".

## Task 4 — differential test `access_log_rf_connect_failure.rs` ✅

- **Files (created):** `tests/differential/tests/access_log_rf_connect_failure.rs` — a structural clone of `access_log_rf_overflow.rs` pointed at `0060`. Auto-discovered integration-test binary (no registry edit); the dead-endpoint pattern needs no `needs_health_aware_backend` allowlist entry and no `--per-path` map arm.
- **Verify:** `cargo build -p differential --tests` → clean (new binary compiles). `cargo build -p envoy-bin` → clean (the debug binary the differential runs picks up the 503/`UF` change — memory `differential-harness-uses-debug-envoy-bin`).
- **NOTE:** the Docker differential `0060` itself (real Envoy vs envoy-rust) is **CI-authoritative** — this host's Docker flakes the differential fixtures (memories `differential-host-bridge-ip-192-168-65-2`, `differential-fixtures-flake-under-parallel-load`). The green assertion is deferred to the state-4 CI gate.
- **Commit:** `a4ea26d` "phase 52 task 4: differential test access_log_rf_connect_failure (fixture 0060)".

## Task 5 — BEHAVIOR_CONTRACT updates (§E) ✅

- **Files:** `docs/envoy-rust/BEHAVIOR_CONTRACT.md`.
- (1) The `%RESPONSE_FLAGS%` row: intro count "four"→"five witnessed failure paths" + added the `UF` per-flag-equivalence clause (config-deterministic single constant, NOT rcd-derived — keyed on the `connect_failure_for_log` final-outcome boolean; synth-503; rcd/body NOT witnessed), extended the value-exact enumeration (`+ UF connect-failure case`), added the witnessing sentence (`Phase 52 (ADR-0109) fixture 0060 witnesses UF …`), moved `UF` out of the "Other non-`-` flags (`UF`/`DC`) remain unwitnessed" set (leaving `DC`).
- (2) Stale connect-failure 502→503 sweep: the `pick()->None` row's "connect-fail 502" (→503), the `downstream_rq_5xx` row, the `upstream_rq_total` row ("Synth-502 paths"→"Synth-503 paths on connect-fail"), the `upstream_rq_5xx` row, and the per-attempt-reconciliation paragraph ("connect-failure synth-502"→"synth-503"). The reset/send-fail 502 mentions kept (M52-1). Re-grep confirmed no connect-failure row still says 502.
- (3) **Fold M51-1:** reconciled the `%RESPONSE_FLAGS%`-rule line-anchor `hcm.rs:1225`→`hcm.rs:1343` for the NR/UH/UO/URX/UF rules (all 3 explicit occurrences in the row). **NOTE — anchor value:** the PLAN named `:1305` (its pre-Task-2 value); the live `response_flags:` derive is at `hcm.rs:1343` AFTER this session's Task-2 comment block shifted it +38. Per the PLAN's own line-drift rule ("re-grep the named token, never trust the absolute line") and M51-1's purpose (anchor ACCURACY), I reconciled to the true live value `:1343`, not the stale `:1305`.
- **Commit:** `0453c1e` "phase 52 task 5: BEHAVIOR_CONTRACT — UF rule + connect-failure 503 + M51-1 anchor".

## Task 6 — stale-502 comment/doc sweep (§G non-edit-site comments) ✅

- **Files:** `crates/envoy-http1/src/hcm.rs` (`:484`, the `upstream_rq_5xx` comment, the `synth_no_healthy_upstream` doc, the test-module doc), `crates/envoy-http1/src/router.rs` (`X_ENVOY_ATTEMPT_COUNT` doc).
- Pure comment/doc edits: "connect-fail(ure) synth-502"→"synth-503" at each connect-failure mention; KEPT the reset/send-fail `502` (the `:615`/`:618` arm, the `reset synth-502` counter comment, `send-fail-502`).
- **Verify:** `grep -nE '502|Bad Gateway'` over both files → only the reset/send-fail arm (`:615` warn + `:618` synth), the reset-path counter comment, and `send-fail-502` remain; NO connect-failure 502. `cargo build -p envoy-http1 && cargo clippy -p envoy-http1 --all-targets -- -D warnings` → clean.
- **Commit:** `badee9f` "phase 52 task 6: sweep stale connect-failure synth-502 comments -> 503".

## Task 7 — local verification pass (non-Docker §7.5 subset) ✅

- **Step 1 `cargo build --workspace --all-targets`** → clean.
- **Step 2 `cargo clippy --workspace --all-targets --all-features -- -D warnings`** → clean (no `unused_assignments` on `final_outcome` — the `#[allow(unused_assignments)]` held).
- **Step 3 `cargo fmt --all -- --check`** → clean (exit 0).
- **Step 4 `cargo test --workspace`** → the deterministic non-Docker suite is GREEN (envoy-http1 148, envoy-accesslog 98, etc., incl. the new `h1_connect_failure_access_log_carries_uf_flag` backstop + the 3 flipped status tests). TWO known host flakes surfaced under full-workspace parallel load, **both pass in isolation** and both are on surfaces UNRELATED to this phase (NOT regressions):
  - `access_log_route_name` (differential, fixture 0049/phase 41) — "upstream Envoy never became accept-ready: Connection refused" → the Docker testcontainer host flake (memories `differential-fixtures-flake-under-parallel-load`, `differential-host-bridge-ip-192-168-65-2`). Re-ran in isolation → **ok**.
  - `missing_rds_file_is_fatal` (envoy-bin `xds_file_based_rds`) — the `reserve_port()` fatal-startup pattern → ephemeral-port-reuse flake under parallel cargo test (memory `eds-fatal-startup-test-port-reuse-flake`). Re-ran in isolation → **ok**.
  - `cargo test --workspace --exclude differential` confirmed the non-differential suite is otherwise green (only the RDS port-reuse flake, which passes in isolation).
- **Step 5 `cargo deny check`** → **advisories ok, bans ok, licenses ok, sources ok** (the `license-not-encountered` lines are pre-existing benign unmatched-allowance warnings).
- **No commit** (verification only — no fixes were required).

## Task 8 — PROGRESS.md + handoff to state-4 ✅ (this file)

- Wrote this PROGRESS.md; advanced `STATE.md` (the four top sections + the active-phase Notes subsection) to "phase 52 state-3 implementation COMPLETE → state-4 verification NEXT"; relocated the superseded state-2 narratives to `STATE_HISTORY.md` per ADR-0035; updated `next-prompt.txt` → the §5 state-4 verification.

---

## Deferred (out of scope — NOT implemented this phase)

- The reset/send-fail arm (`hcm.rs:615`/`:618`, `AttemptOutcome::Reset`) — stays `synth_status(502, close)`. NEW carry-forward **M52-1** (the `UC` flag + the reset 502→503 status; re-recon before witnessing).
- The connect-failure `%RESPONSE_CODE_DETAILS%` + response body (non-deterministic OS transport-failure reason — M45-2); NOT logged in `0060`, NOT compared by the driver.
- Retry-exhausted-connect-failure (`UF`+`URX` combination) — `0060` uses NO retry_policy; the `URX`-before-`UF` derive ordering renders it deterministically if both ever set.
- H2 connect-failure `%RESPONSE_FLAGS%` (M45-1) + the `DC` flag + the retry-budget-overflow slice of M45-2.
- Fuzz: `%RESPONSE_FLAGS%` is an existing operator → NO new fuzz target; `ci.yml` unchanged.

## Acceptance (§7.5 — re-run at state-4 on CI)

(a) fixture `0060` green (cross-proxy status `503` + whole-line `{"rc":503,"rf":"UF"}`) — **CI-authoritative**; (b) `0001`–`0059` all green simultaneously (additive — `connect_failure_for_log` is false on every existing fixture; the 502→503 change touches no existing GREEN fixture); (c) h2spec ≥95% (no HTTP/2 change); (d) `parse_bootstrap`/`accesslog_format_parse` fuzz clean (no new target); (e) build/clippy/fmt/test/deny clean (Tasks 1/2/6/7 — done locally); (f) `REVIEW.md` approved. `#![forbid(unsafe_code)]` holds; NO new crate/dependency/fuzz-target/`Op`/`AccessLogRecord` field/`ConfigError` variant.
