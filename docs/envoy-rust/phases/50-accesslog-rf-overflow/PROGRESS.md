# Phase 50 — `50-accesslog-rf-overflow` Implementation Progress

> State-3 running log (`superpowers:subagent-driven-development`, TDD per task —
> fresh implementer subagent per task + independent spec-compliance review per
> task). Append-only per-task entries. The full §7.5 verification gate (Docker
> differential `0058` green + all `0001`-`0057` byte-identical + h2spec + fuzz +
> deny, quoted in) runs at the state-4 session AFTER this one
> (`superpowers:verification-before-completion`); the Docker differential is
> CI-authoritative there (memory `envoy-rust-state4-ci-first-execution`).

**Goal:** Witness the THIRD non-`-` `%RESPONSE_FLAGS%` value — `UO`
(UpstreamOverflow) — BYTE-EXACT on the circuit-breaker overflow 503 path, by
(§A) discriminating the overflow outcome at envoy-rust's H1 retry-loop
result-consumption site (the `attempt.outcome.is_none()` overflow result inside
the `if let Some(endpoint)` branch) and setting `response_code_details_for_log =
Some("upstream_reset_before_response_started{overflow}")`, (§A′) tagging the
pre-route request-budget overflow arm with the same rcd, and (§B) extending the
phase-48/49 `%RESPONSE_FLAGS%` derive by one arm
(`Some("upstream_reset_before_response_started{overflow}") => "UO"`). Additive →
all `0001`-`0057` byte-identical.

**Scope lock:** ADR-0107. NO new `Op` / `AccessLogRecord` field / crate /
dependency / fuzz-target / `ConfigError` variant. `#![forbid(unsafe_code)]`
holds. H1-only (H2 deferred — M45-1).

**Live line numbers re-verified against disk before/while editing (M48-1/M49-3
ACTIONED):** plan-write citations were stale (T1's §A insertion shifted
everything below by ~21 lines; T2's §A′ insertion shifted by ~10 more). Final
post-T2 anchors (used in the in-code comments after the T1/T2 follow-up
re-anchor commit `8653138`): §A pool-overflow discriminator (`if
attempt.outcome.is_none()`) `hcm.rs:1020`; the no-healthy `else` arm
`:1031-1032`; §A′ request-budget arm tag `:932` (after `synth_overflow(close)`
at `:923`); §B derive arm (`=> "UO"`) `:1277` (match opens `:1274`); the two
pool-overflow arms `AcquireOutcome::Overflow(synth_overflow(close))` at `:508`
(`max_connections`) / `:515` (`max_pending_requests`); the `synth_404` returns
host-miss `:1591` / route-miss `:1610`.

**Process note (subagent-driven):** each task was implemented by a fresh
`general-purpose` implementer subagent (full task text supplied — no plan-file
read), TDD RED→GREEN, then independently spec-reviewed by a fresh
`superpowers:code-reviewer` subagent (T1/T2/T3) or by the controller reading the
committed diff (T4, prose-only). All reviews returned ✅ spec compliant.

---

## Task 1 — §A pool-overflow discriminator + §B derive arm (RED→GREEN) — ✅ DONE

- **RED:** Added `#[tokio::test(flavor = "multi_thread")]` backstop
  `h1_pool_overflow_access_log_carries_uo_flag` in
  `crates/envoy-http1/src/hcm.rs` (configured `pool_mgr: Some(...)` via
  `H1PoolManager::for_bootstrap`, STATIC cluster with
  `circuit_breakers.thresholds:[{max_connections:1, max_pending_requests:0}]` +
  dead endpoint `127.0.0.1:1`, `{rc,rcd,rf}` FileSink, `GET /` probe). Pre-change
  the final `assert_eq!` failed exactly as predicted:
  - `left:  "{\"rc\":503,\"rcd\":\"via_upstream\",\"rf\":\"-\"}\n"`
  - `right: "{\"rc\":503,\"rcd\":\"upstream_reset_before_response_started{overflow}\",\"rf\":\"UO\"}\n"`
  (status + 81-byte overflow body asserts passed first → the assertion is the
  failing point, not the harness wiring.)
- **GREEN — §A:** inside `if let Some(endpoint) = attempt.endpoint {` replaced the
  unconditional `response_code_details_for_log = Some("via_upstream".to_owned())`
  with an `attempt.outcome.is_none()`-discriminated set (overflow →
  `"upstream_reset_before_response_started{overflow}"`, else `"via_upstream"`).
  The `else` no-healthy (`endpoint:None`) branch UNCHANGED. Covers BOTH pool arms
  (`:508`/`:515` both collapse to `AcquireOutcome::Overflow` → `outcome:None`).
- **GREEN — §B:** added the third derive arm
  `Some("upstream_reset_before_response_started{overflow}") => "UO"` to the
  `response_flags: match response_code_details_for_log.as_deref()` block; the
  `route_not_found => "NR"`, `no_healthy_upstream => "UH"`, and `_ => "-"` arms
  UNCHANGED. Fixed the drifted `synth_404` citations in the derive comment block
  (M49-2) + kept the record-build anchor convention (M49-3).
- **Result:** `cargo test -p envoy-http1 access_log_carries` → 7 passed, 0
  failed (new test + the `UH`/route-miss/host-miss backstops). `cargo test -p
  envoy-http1` → **145 passed, 0 failed**. `cargo fmt -p envoy-http1` clean.
- **Commit:** `49814f7`. **Spec review:** ✅ spec compliant (independent fresh
  `superpowers:code-reviewer`: §A discriminates on `outcome.is_none()` inside the
  `endpoint:Some` branch, overflow literal byte-identical at all sites, NR/UH/`_`
  arms untouched, only `hcm.rs` changed — additive).

## Task 2 — §A′ request-budget overflow tag (RED→GREEN) — ✅ DONE

- **RED:** Added `h1_request_budget_overflow_access_log_carries_uo_flag`
  (`cluster_mgr_with_endpoint_max_requests("backend", port, 0)`, `pool_mgr:
  None` → the budget gate fires before any pool/dispatch). Pre-change failed
  exactly as predicted — the budget arm left rcd at its `None` init:
  - `left:  "{\"rc\":503,\"rcd\":null,\"rf\":\"-\"}\n"`
  - `right: "{\"rc\":503,\"rcd\":\"upstream_reset_before_response_started{overflow}\",\"rf\":\"UO\"}\n"`
  (status `503` asserted first.)
- **GREEN — §A′:** inside `if let envoy_cluster::BudgetAcquisition::Rejected =
  request_acquire {`, immediately after `outgoing = synth_overflow(close);`
  (`:923`), added the one assignment
  `response_code_details_for_log = Some("upstream_reset_before_response_started{overflow}".to_owned())`.
  This arm BYPASSES the retry loop, so it is tagged directly (not via the §A
  `:1020` discriminator); the existing §B derive arm maps it => `"UO"`.
- **Result:** `cargo test -p envoy-http1 overflow` → 9 passed, 0 failed (incl.
  the new budget test, the pool test, `request_budget_overflow_max_requests_zero`,
  `synth_overflow_emits_81_byte_body_and_x_envoy_overloaded`). `cargo test -p
  envoy-http1` → **146 passed, 0 failed**.
- **Adaptation:** the discriminant type path is `envoy_cluster::BudgetAcquisition`
  (not `envoy_config::` as the plan prose stated); no edit needed to that line.
- **Commit:** `e74264a`. **Spec review:** ✅ spec compliant (independent fresh
  `superpowers:code-reviewer`: assignment inside the `Rejected` block right after
  `synth_overflow(close)`, literal matches the derive arm, fires ONLY on the
  budget arm, Task-1 artifacts untouched, only `hcm.rs` changed — 94 insertions /
  0 deletions, additive).

## T1/T2 follow-up — comment line-citation precision (M49-2/M49-3) — ✅ DONE

- Both spec reviews flagged the in-code comment `:NNN` citations as slightly
  stale (lines shifted as T1/T2 edits landed). Since `hcm.rs` lines are final
  after T2 (T3 touches no source, T4 only docs), re-anchored all UO
  derive-comment citations to the now-final numbers (pool discriminator `:1020`,
  derive arm `:1277`, budget arm `:932`, `synth_404` returns `:1591`/`:1610`).
  Comment-only; no behavior change; `cargo fmt -p envoy-http1 --check` clean.
- **Commit:** `8653138`. This properly CONSUMES the M49-2/M49-3 citation-precision
  intent for the surfaces phase 50 touches (rather than leaving fresh drift).

## Task 3 — Fixture `0058-accesslog-rf-overflow` + differential test — ✅ DONE

- Created 5 new files (additive; no existing file touched): the paired
  `envoy.yaml`/`envoy-rust.yaml` (STATIC `backend_cluster` with
  `circuit_breakers max_connections:1/max_pending_requests:0` + dead
  `127.0.0.1:1`, H1 HCM listener routing `/` → `backend_cluster`, FileAccessLog
  `json_format {rc, rcd, rf}`), `expectations.yaml` (`http1_access_log_byte_exact`
  driver, one `GET /` probe, `expected_status: 503`), `README.md`, and
  `tests/differential/tests/access_log_rf_overflow.rs` (structural clone of
  `access_log_rf_no_healthy.rs` → `0058`).
- **Per-side divergence reconciled to the real `0057` convention** (the plan's
  "node line differs" prose was inaccurate): `node:` line IDENTICAL on both
  sides; `admin:` block + listener bind `0.0.0.0` only in `envoy.yaml`,
  `127.0.0.1` in `envoy-rust.yaml`; access-log mount path differs. `diff` of the
  two `0058` yamls = the exact same divergence set as the two `0057` yamls,
  nothing more. `json_format` deliberately uses the 3 keys `{rc, rcd, rf}` (vs
  `0057`'s 5) per the state-0 recon line.
- **Differential RAN AND PASSED** (Docker available on this host): the envoy-rust
  side logged `WARN pending-request overflow (max_pending_requests:0) — returning
  503 cluster=backend_cluster`, then `test access_log_rf_overflow ... ok` (1
  passed, 7.66s) — the `http1_access_log_byte_exact` driver asserted the emitted
  JSON line byte-identical between Envoy v1.33.0 and envoy-rust
  (`{"rc":503,"rcd":"upstream_reset_before_response_started{overflow}","rf":"UO"}`).
  (Per memory `envoy-rust-state4-ci-first-execution` the Docker differential is
  CI-authoritative at the state-4 gate regardless; this local green is
  corroborating, not the gate.)
- **Commit:** `ba5b206`. **Spec review:** ✅ spec compliant (independent fresh
  `superpowers:code-reviewer`: exactly 5 added files / 307 insertions / 0
  deletions, divergence pattern matches `0057`, schema matches, `cargo build -p
  differential --tests` clean).

## Task 4 — §E BEHAVIOR_CONTRACT updates (+ M49-3 re-anchor) — ✅ DONE

- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` only (3 rows edited in place; no other
  file touched):
  - `%RESPONSE_FLAGS%` row (`:1020`): added the **`UO` per-flag equivalence rule**
    (config-deterministic single brace-free constant, derived 1:1 from
    `%RESPONSE_CODE_DETAILS%` = `upstream_reset_before_response_started{overflow}`,
    set on both pool-overflow arms + the request-budget arm), moved `UO` OUT of
    the unwitnessed list (now `UF`/`DC`/`URX`), updated the value-exact summary to
    include the `UO` case, and added the fixture-`0058` witnessing sentence + the
    M50-C deferral.
  - `%RESPONSE_CODE_DETAILS%` row (`:1031`): added
    `upstream_reset_before_response_started{overflow}` to the rcd disposition
    enumeration (the `outcome:None` overflow discriminator + pool arms
    `:508`/`:515` + the request-budget arm), added `0058` as the FOURTH witnessed
    failure-path detail, and SCOPED the M45-2/ADR-0102 §B deferral to
    connect-failure ONLY (the overflow brace content `overflow` is a FIXED
    reset-reason enum → witnessed byte-exact deterministic).
  - overflow circuit-breaker row (`:37`): re-anchored the stale `hcm.rs:542`/`:569`
    citations to the actual `AcquireOutcome::Overflow` arms `hcm.rs:508`/`:515`
    (M49-3), and appended the witnessed-byte-exact note. The wire-shape
    equivalence statement (status + 81-byte body + `x-envoy-overloaded`)
    UNCHANGED.
- **Result:** `cargo test -p envoy-http1` → 146 passed, 0 failed; `cargo test -p
  envoy-accesslog` → 98 passed, 0 failed (prose-only edits — no test references
  the changed lines). `0056`/`0057` fixtures + `NR`/`UH` derive arms untouched.
- **Commit:** `e30586f`. **Spec review:** ✅ spec compliant (controller read the
  committed diff: 3 insertions / 3 deletions, all three rows updated per plan
  intent, only `BEHAVIOR_CONTRACT.md` changed).

---

## State-3 summary

- **All 4 PLAN tasks DONE** (T1 §A+§B pool-overflow / T2 §A′ budget-overflow / T3
  fixture `0058` + differential / T4 BEHAVIOR_CONTRACT §E) + the T1/T2 follow-up
  citation-precision commit. Commit chain: `49814f7` → `e74264a` → `8653138` →
  `ba5b206` → `e30586f`.
- **Additive proven:** the only source change is the §A `outcome:None` overflow
  discriminator (all other `endpoint:Some` outcomes keep `via_upstream`), the §A′
  budget-arm tag, and the §B one-arm derive extension; the `NR`/`UH` arms are
  byte-identical → `0001`-`0057` byte-identical. `#![forbid(unsafe_code)]` holds.
  No new `Op`/`AccessLogRecord` field/crate/dependency/fuzz-target/`ConfigError`
  variant.
- **Controller pre-push sanity sweep (NOT the §7.5 gate — that is state-4):**
  `cargo fmt --all -- --check` CLEAN; `cargo build --workspace --all-targets`
  Finished OK; `cargo clippy -p envoy-http1 --all-targets --all-features -- -D
  warnings` Finished OK; the local Docker differential for `0058` passed. The
  FULL §7.5 (a)-(f) gate (all `0001`-`0058` differential green simultaneously +
  h2spec + fuzz + deny + workspace clippy/test) runs at the state-4 session,
  CI-authoritative.
- **Carry-forward consumption (recorded at state-3 close):**
  - **CONSUMED by phase 50:** the `UO` slice of **M45-2** (+ the
    overflow-rcd-deterministic refinement scoping M45-2/ADR-0102 §B to
    connect-failure ONLY); **M49-3** on the overflow circuit-breaker row + the
    record-build derive anchor; **M49-2** on the derive-comment `synth_404`
    citations.
  - **NEW carry-forward — M50-C:** the request-budget (`max_requests`) overflow
    UO/rcd (set by §A′ at `hcm.rs:932`) is in-process-backstopped only (T2); its
    differential witness is deferred (no `max_requests` access-log fixture). **The
    unverified part is the rcd STRING, not just the flag:**
    `upstream_reset_before_response_started{overflow}` was recon-confirmed ONLY on
    the pool path (`max_pending_requests`); Envoy's request-level breaker may emit
    a DIFFERENT `%RESPONSE_CODE_DETAILS%` while still flagging `UO` — whoever
    witnesses the budget arm must RE-RECON the rcd string against live Envoy. Fold
    when a request-budget access-log fixture is next added.
  - **Still live (NONE blocks):** M48-2, M42-1, M45-1, the connect-failure slice
    of M45-2 (`UF`/`DC`/`URX`), M40-1, M39-*, M38-*, CF-39-1, M37-*, M36-*,
    M34-*, M33-*, the empty-`metadata_match` doc-comment, M29-*, M30-*, the
    phase-31 cosmetics, the HTTP-filters-family (1)-(4).
- **§6.1 split did NOT fire** (4 tasks / ~70-120 LoC ≪ thresholds); ADR-0108 stays
  reserved-but-unfired. **No §6.2-reconciliation ADR** (the recon overturned no
  SPEC fact).
- **Next session = §5 state-4 verification gate** (`superpowers:verification-before-completion`):
  run the full §7.5 (a)-(f) and quote all command outputs into this PROGRESS.md;
  the Docker differential (`0058` green + `0001`-`0057` byte-identical) is
  CI-authoritative.
