# Phase 17 (`17-circuit-breaker-budgets`) — PROGRESS

> Running log, updated by the executor on each task completion (the 06.2 → 16 cadence).
> One entry per PLAN task; quote the verifying command output. The state-3 arc runs
> `cargo clippy --workspace --all-targets --all-features -- -D warnings` PER TASK
> (NOT deferred to state-4) per `project_state3_arc_skips_clippy`.

**PLAN:** `docs/envoy-rust/phases/17-circuit-breaker-budgets/PLAN.md`
**SPEC:** `docs/envoy-rust/phases/17-circuit-breaker-budgets/SPEC.md`
**Scope ADRs:** ADR-0046 (minimum-viable budget scope + the zero-cap-always-trips finding + the cluster-scoped ownership boundary); ADR-0047 (§6.2 reconciliation — the overflow co-firing counters [`upstream_rq_pending_overflow` + `upstream_rq_5xx`; `upstream_cx_total` known divergence] + the momentary `*_open` gauge semantic [fixture asserts at 0]).

---

## State-2 PLAN-write (this commit)

- Performed the HEAVY SPEC §6.2 empirical verification against `envoyproxy/envoy:v1.33.0` (digest `sha256:56da5afd…`; Docker; foreground general-purpose subagent; 5-cluster topology; every claim cross-checked against backend request counts). Findings L1–L12 locked into PLAN.md "§6.2 empirical lock-ins". **The §0 zero-cap-always-trips projection is CONFIRMED** (L1/L2 — the most consequential items match). Two material divergences (**L3** the overflow local reply co-fires `upstream_rq_5xx`/`upstream_rq_503`/`upstream_rq_completed` + `upstream_cx_total` prefetch while `upstream_rq_total` stays 0; **L4** the `*_open` breaker gauges are MOMENTARY — 0 at rest and after sequential probes, never latched) → **ADR-0047 landed**.
- **The opportunistic L11 item RESOLVED the M16-3 carryforward:** Envoy DOES emit `x-envoy-attempt-count` on overflow local replies (value 1) under `include_attempt_count_in_response: true` — envoy-rust's existing synth-path emission is empirically CORRECT. Fixture 0025 closes M16-3 with bilateral coverage (per-probe `x-envoy-attempt-count` value assertions).
- Performed the PLAN-time SPEC-correction pass (read-only Explore subagent) against HEAD `ee61fc744`. **ZERO drift** — all 20 SPEC §0/§3 anchors confirmed accurate (the first phase since the correction-pass discipline began with no corrections needed). Confirmations recorded in PLAN.md "PLAN-time SPEC corrections".
- Evaluated the §6.1 split gate against the §6.2-refined surface (~1450–1550 LoC / 11 tasks) → **single un-split phase; ADR-0048 does NOT fire.**
- Flipped ROADMAP row `17` `planned → in-progress`. Advanced STATE.md to `17` state-2-complete / state-3-next.

## Task 1 — `envoy-config` schema (`Thresholds` budget fields + validator acceptance)

**Preamble (read before starting):**
- **Goal:** Add `max_requests`/`max_retries`/`track_remaining` to the `Thresholds` struct (`crates/envoy-config/src/bootstrap.rs:1319-1328`). NO `validate_circuit_breakers` semantic change required — `0` is valid for both new caps (the always-open-breaker configs, L1/L2); zero new `ConfigError` variants. TDD: serde round-trip + deny_unknown_fields rejection (deferred `retry_budget`/`max_connection_pools`) + validator-accepts-zero tests first.
- **§6.2 lock-ins that bind this task:** L1/L2 (`0` caps are valid, meaningful configs — the validator must NOT reject them, in contrast to `max_connections == 0` → `InvalidMaxConnections`); L5 (defaults 3/1024 are resolved at `Cluster::from_bootstrap` [Task 3], NOT in the schema — the schema keeps `Option<u32>`).
- **Anchors (verified at HEAD `ee61fc744` — ZERO drift):** `Thresholds` `bootstrap.rs:1319-1328` (`#[serde(deny_unknown_fields)]`; fields priority/max_connections/max_pending_requests); `validate_circuit_breakers` `:2729-2767`; the 4 existing CB `ConfigError` variants in `lib.rs:445-469`.
- **Carry-forward warning:** the phase-15 pool test modules (`crates/envoy-http1/src/pool.rs` + `crates/envoy-http2/src/pool.rs` `#[cfg(test)]`) construct `Thresholds` struct literals — if exhaustive, they break on the new fields and MUST be extended with `max_requests: None, max_retries: None, track_remaining: None` in the SAME commit (the phase-16 Task-1 workspace-compile lesson).
- **Verification:** `cargo test -p envoy-config` (PASS) + `cargo build --workspace --all-targets` + `cargo clippy --workspace --all-targets --all-features -- -D warnings` + `cargo fmt --all -- --check`.

## Task 1 — `Thresholds` budget fields (`max_requests`/`max_retries`/`track_remaining`) (commit `902a5aa48`)

**Landed.** `crates/envoy-config/src/bootstrap.rs` (single file): the 3 new `Option` fields on `Thresholds`
(`max_requests: Option<u32>` / `max_retries: Option<u32>` / `track_remaining: Option<bool>`, each
`#[serde(default, skip_serializing_if = "Option::is_none")]`; `deny_unknown_fields` retained); the
`validate_circuit_breakers` doc-comment explaining the zero-cap acceptance asymmetry (`max_requests: 0`/
`max_retries: 0` = always-open breakers [L1/L2], ACCEPTED, vs `max_connections: 0` = `InvalidMaxConnections`
[phase-13 rationale]); the `CircuitBreakers` struct doc updated to reflect the new accepted-field set.
**Zero new `ConfigError` variants; zero semantic validation changes** (PLAN lock-in). The pre-existing test
`cluster_circuit_breakers_rejects_phase13_deferred_threshold_fields` renamed to
`..._rejects_still_deferred_threshold_fields` (its unknown-field probe switched `max_requests: 5` →
`max_connection_pools: 5` since the former is now a valid field — no coverage lost). 7 new TDD tests
(parse 0/0/true; parse 5/3/false; absent → None; `retry_budget` rejected; `max_connection_pools` rejected;
validator accepts zero caps; existing rejections still fire with budget fields present).

**Pool-literal carry-forward warning: did NOT materialize.** `grep Thresholds crates/envoy-http{1,2}/src/pool.rs`
→ no struct literals (the pools read thresholds via `.and_then()` accessors) — no fold-in needed; the commit
touches exactly 1 file.

**Verification (quoted):**
- TDD RED: 6 compile errors (`no field 'max_requests' on type '&Thresholds'`) before the fields landed.
- `cargo test -p envoy-config` → `test result: ok. 302 passed; 0 failed` (295 + 7 new).
- `cargo build --workspace --all-targets` → clean.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` → clean (exit 0).
- `cargo fmt --all -- --check` → clean (exit 0).
- `git show --stat HEAD` → 1 file changed (`bootstrap.rs` +346/−14).

**Two-stage review:** spec-compliance **✅ compliant** (first pass; reviewer independently re-ran all gates +
verified the renamed test lost no coverage + verified the pool.rs no-literal claim); code-quality **Approved**
(zero Critical / zero Important / 3 Minors — stale `CircuitBreakers` doc + 2 test-coverage nits — **all 3
fixed in-task** before the commit was finalized [amended pre-push]).

---

## Task 2 — `BudgetState` + RAII budget guards (`crates/envoy-cluster/src/budget.rs`) (commit `12817303e`)

**Landed.** New module `crates/envoy-cluster/src/budget.rs` (522 lines incl. tests) + `lib.rs` module decl/re-exports
(2 lines): `BudgetState` (resolved caps + plain `AtomicI64` active-counts + stat handles), `try_acquire_retry(self:
&Arc<Self>) -> Option<RetryBudgetGuard>` / `try_acquire_request(...) -> Option<RequestBudgetGuard>` (CAS
compare-and-increment under the cap; a `0` cap always fails = the always-open breaker [L1/L2]), RAII guards whose
`Drop` decrements the active count + re-syncs gauges. **Overflow counters tick INSIDE the failed acquire** (§5.3
single source of truth; retry → `upstream_rq_retry_overflow`, request → `upstream_rq_pending_overflow` [L3], once
per failed call, never on CAS contention). **Momentary gauge semantic (L4 / ADR-0047):** `*_open` = 1 iff
`active > 0 AND active >= max`, updated on acquire success/failure + guard Drop; zero-cap gauges provably stay 0.
**`remaining_*` gauges (L8):** registered ONLY when `track_remaining: true`, initialized to cap values, floored at 0;
absent (not present-at-0) otherwise. `upstream_rq_pending_overflow` registration is idempotent-shared with the
phase-15 pool handle (verified against `envoy-stats::registry` same-kind semantics). Guards are Send+Sync (hold only
`Arc<BudgetState>`) — safe to hold across `.await` in the Tasks 4–7 retry loops. 10 TDD tests.

**Public API for Tasks 3–7:** `BudgetState::new(max_retries, max_requests, track_remaining, &StatsRegistry,
cluster_name) -> Result<Arc<Self>, StatsError>`; `try_acquire_retry` / `try_acquire_request`; `RetryBudgetGuard` /
`RequestBudgetGuard`.

**Verification (quoted):**
- TDD RED: `cargo test -p envoy-cluster budget` → 0 tests / compile failure before the module landed.
- `cargo test -p envoy-cluster` → `test result: ok. 80 passed; 0 failed` (70 + 10 new).
- `cargo build --workspace --all-targets` → clean; `cargo clippy --workspace --all-targets --all-features -- -D warnings` → clean (exit 0); `cargo fmt --all -- --check` → clean (exit 0).
- `git show --stat HEAD` → 2 files changed (`budget.rs` +522 new; `lib.rs` +2).

**Two-stage review:** spec-compliance **✅ compliant** (first pass; reviewer verified the CAS loop has no TOCTOU, the
overflow tick fires once per failed call [not on contention], the L4 formula, the L8 absence-via-snapshot assertion,
and the idempotent shared registration). Code-quality **Approved** (zero Critical / zero Important / 3 Minors — the
PLAN-skeleton-inherited `Arc<AtomicI64>` double-wrap [hot-path allocation], a missing gauge-interleaving doc note,
and a `remaining_rq` cycle test gap — **all 3 fixed in-task** [amended pre-push]). Reviewer confirmed guards are
Send+Sync (the highest-risk Tasks-4–7 integration concern).

---

## Task 3 — `Cluster` budget integration + conditional stat registration (commit `b714b43e8`)

**Landed.** `crates/envoy-cluster/src/cluster.rs` (+397): `Cluster` gains `budget: Option<Arc<BudgetState>>` + the
**unconditional** `upstream_rq_retry_overflow` counter handle (registered for EVERY cluster in `from_bootstrap`
next to the phase-16 retry counters, inert at 0 — the L12/fixture-0011-safe posture; idempotent-Arc-shared with
`BudgetState`'s own registration when a budget exists). Budget resolution is **conditional on
`circuit_breakers.is_some()`** (the phase-15 conditional-registration discipline — clusters without
`circuit_breakers` register ZERO `circuit_breakers.default.*` gauges, keeping the 24 existing fixtures' stat
surfaces untouched): `thresholds.first()` → `max_retries.unwrap_or(3)` / `max_requests.unwrap_or(1024)` /
`track_remaining.unwrap_or(false)` (L5 defaults). `crates/envoy-cluster/src/budget.rs` (+41): new
`#[must_use] #[derive(Debug)] pub enum BudgetAcquisition<G> { Unlimited, Acquired(G), Rejected }` (the
clippy-friendly three-state shape — no `Option<Option<_>>` at the H1/H2 call sites) + hand-rolled `Debug` impls on
both guards (the `PoolGuard` precedent). `Cluster::try_acquire_retry()/try_acquire_request()` +
**`ClusterHandle` delegates** (mirroring the `upstream_rq_retry()` delegate pattern — the H1/H2 HCMs hold
`ClusterHandle` from `cluster_mgr.get()`, so Tasks 4–7 call the gates directly on what they already hold). 5 TDD
integration tests (a: no-CB → Unlimited + zero conditional gauges via full snapshot scan + unconditional overflow
counter at 0; b: `max_retries: 0` → Rejected + default-1024 request budget; c: `track_remaining` conditionality;
d: empty threshold → L5 defaults 3/1024 proven by exhaustion; e: the shared-Arc idempotent-registration contract).

**Verification (quoted):**
- `cargo test -p envoy-cluster` → `test result: ok. 85 passed; 0 failed` (80 + 5 new).
- `cargo build --workspace --all-targets` → clean; `cargo clippy --workspace --all-targets --all-features -- -D warnings` → clean; `cargo fmt --all -- --check` → clean.
- `git show --stat HEAD` → 3 files changed (+439/−1).

**Two-stage review:** spec-compliance **✅ compliant** (first pass; reviewer verified the conditional-registration
regression-safety property via the snapshot-scan test, the L5 defaults, the Unlimited zero-side-effect path, the
non-duplicated overflow tick, and that `ClusterManager::get` → `ClusterHandle` makes the API reachable for Tasks
4–7). Code-quality **Approved** (zero Critical / ONE Important — missing `#[must_use]` on `BudgetAcquisition`,
the guard-lifetime hazard class for Tasks 4–7 — / 2 Minors [guard `Debug` impls; a test guard-drop-in-`matches!`
example] — **all 3 fixed in-task** [amended pre-push]).

---

## Task 4 — H1 retry-budget gate (`max_retries`) (commit `03d3b0ba7`)

**Landed.** `crates/envoy-http1/src/hcm.rs` (+299/−6, single file): the retry-budget gate composed as one more
CONJUNCT inside the phase-16 retry decision (`final_retriable && attempts <= max_retries` preserved unchanged;
the `match cluster.try_acquire_retry()` sits inside it — §5.5 composes-with-never-replaces). **Guard threading:**
`retry_guard_slot: Option<RetryBudgetGuard>` + `retry_budget_blocked: bool` declared before the loop;
`Unlimited` arm = phase-16 behavior byte-identical (regression-critical, proven by test (c));
`Acquired(guard)` arm = tick `upstream_rq_retry` + back-off + park the guard in the slot + `continue` (each
reassignment drops the PRIOR guard → the slot is held across the back-off sleep AND the in-flight retried
attempt — constraint iii); `Rejected` arm = set the flag + fall through to `break` (the would-be-retried
response surfaces downstream VERBATIM — L6; no retry-counter ticks — the overflow already ticked inside the
failed acquire, §5.3). **Post-loop split** guarded `if attempts > 1 && !retry_budget_blocked` → a budget-blocked
exit ticks NEITHER `_success` NOR `_limit_exceeded` (L7 exclusivity; the flag — not the attempts check — is
load-bearing for >0-cap mid-sequence exhaustion). Preserved untouched: per-attempt `upstream_rq_total` /
`record_response`, the completing-response `upstream_rq_5xx` gate (`995445b52`), gated `x-envoy-attempt-count`.
3 TDD tests + an `Option<u32>`-parameterized cluster-helper (a: blocked — 503 verbatim + attempt-count 1 +
overflow=1/retry=0/success=0/limit=0/rq_total=1 + backend saw exactly 1; b: default-cap-3 control — 200 +
attempt-count 2 + retry=1/success=1/overflow=0/limit=0; c: no-circuit_breakers Unlimited regression — same
as (b) + no budget stats registered).

**Verification (quoted):**
- TDD RED: test (a) failed pre-gate with `x-envoy-attempt-count: 2` (the retry fired); tests (b)/(c) passed
  pre-gate (proving the control paths are phase-16-identical).
- `cargo test -p envoy-http1` → `test result: ok. 95 passed; 0 failed` (92 + 3 new). `cargo test -p envoy-cluster` → 85 passed.
- `cargo build --workspace --all-targets` → clean; `cargo clippy --workspace --all-targets --all-features -- -D warnings` → clean (the match-inside-if shape trips no lint); `cargo fmt --all -- --check` → clean.
- `git show --stat HEAD` → 1 file changed.

**Two-stage review:** spec-compliance **✅ compliant** (first pass; reviewer traced the guard lifetime across
back-off + in-flight retry, the Rejected fall-through, the L7 exclusivity flag, and diffed the preserved
phase-16 lines character-identical). Code-quality **Approved** (zero Critical / ONE Important [an inaccurate
`drop()` comment — matters because Task 6 mirrors these comments verbatim] / 3 Minors [declaration-comment
precision; helper `Option<u32>` parameterization; missing `limit_exceeded == 0` assertions on tests (b)/(c)] —
**all 4 fixed in-task** [amended pre-push]).

---

## Task 5 — H1 request-budget gate (`max_requests`) + overflow local reply (commit `ce9136c0b`)

**Landed.** `crates/envoy-http1/src/hcm.rs` (+472/−157, single file; the deletion count is dominated by the
retry-loop reindent — `git diff -w` shows the real change is small): the request-budget gate at the
`BuildOutcome::Proxy` dispatch entry — `cluster.try_acquire_request()` called **exactly once**, bound into
`request_acquire`, then an `if let Rejected … else { <the retry loop> }` split. **PLAN deviation (approved at
spec review):** the PLAN sketched an early return, but this HCM is a unified-writer architecture — the Rejected
arm instead sets `outgoing = synth_overflow(close)` and FALLS THROUGH to the same writer site the pool-overflow
arm uses (encode-filters → wire write → `downstream_rq_5xx` per-class tick → access log), which is exactly the
PendingOverflow-arm-equivalent accounting the PLAN intended. **Rejected path:** ticks `upstream_rq_5xx` once
(the L3/ADR-0047 reconciliation — THE ONLY synth path that ticks it; the phase-16 completing-response gate
`995445b52` is byte-untouched per `git diff -w`) + the 81-byte `synth_overflow` body + `x-envoy-overloaded` +
gated `x-envoy-attempt-count: 1` (L11); no pool contact, no retry loop, `upstream_rq_total` stays 0.
**Acquired path:** `_request_guard` is arm-scoped — spans the ENTIRE retry loop and releases on arm exit
(L9b: a retrying request counts ONCE against `max_requests`). **L9a gate ordering** is structural: the gate is
lexically before the loop; Rejected never reaches `try_acquire_retry`. M16-3 comment correction: **N/A**
(grep confirms no hedging comment exists in H1 — nothing to correct). 4 TDD tests + 2 helpers (a: overflow —
503/81-byte/x-envoy-overloaded/attempt-count-1/backend-never-contacted/pending_overflow=1/upstream_rq_5xx=1/
rq_total=0/retry_overflow=0; b: gate ordering with retry_policy — retry budget never consulted; c: `max_requests: 1`
+ retry → 200/attempt-count-2/pending_overflow=0 [the guard-spans-the-loop proof]; d: no-CB regression).

**Verification (quoted):**
- TDD RED: tests (a)/(b) failed pre-gate (backend returned 200 — no gate); (c)/(d) passed pre-gate (no-op without the gate).
- `cargo test -p envoy-http1` → `test result: ok. 99 passed; 0 failed` (95 + 4 new).
- `cargo build --workspace --all-targets` + `cargo build -p envoy-http1` (standalone) → clean; clippy `-D warnings` → clean; fmt → clean.
- `git show --stat HEAD` → 1 file changed.

**Two-stage review:** spec-compliance **✅ compliant** (first pass; reviewer verified the CRITICAL single-acquire
property [no overflow double-tick — one call site, re-matched not re-invoked], the L9a structural ordering, the
guard span, the L3 tick isolation [no other synth path gained it], and the unified-writer fall-through equivalence
to the pool-overflow arm). Code-quality **Approved** (zero Critical / zero Important / 4 Minors [2 comment-precision
items that would have misled the Task-7 H2 mirror; a missing `upstream_rq_5xx` assertion in test (b); a helper
signature-asymmetry docstring note] — **all 4 fixed in-task** [amended pre-push]).

---

## Task 6 — H2 retry-budget gate (`max_retries` mirror) (commit `a468eac7f`)

**Landed.** `crates/envoy-http2/src/hcm.rs` (+323/−14, single file): the Task-4 retry-budget gate mirrored
verbatim onto the H2 retry loop in `handle_one_stream`'s `BuildOutcome::Proxy` arm — same `retry_guard_slot` /
`retry_budget_blocked` declarations, same three-arm `match cluster.try_acquire_retry()` (Unlimited / Acquired /
Rejected) inside the `final_retriable && attempts <= max_retries` conjunct, same Acquired-arm ordering (tick →
back-off → park guard → continue), same post-loop `if attempts > 1 && !retry_budget_blocked` guard, same
`drop(retry_guard_slot)` site, same comments (adapted only for H2 specifics). **Sibling parity verified
mechanically at spec review: the H2 gate is byte-identical to the H1 gate after normalizing indentation + the
`envoy_config::RetryConfig` path prefix.** Phase-16 H2 machinery (per-attempt `upstream_rq_total` /
`record_response`, the completing-response `upstream_rq_5xx` gate, gated `x-envoy-attempt-count`) untouched per
`git diff -w`. 3 TDD tests (`h2_`-prefixed mirrors of the Task-4 names + same assertion sets) + 2 test helpers
(`h1_backend_cluster_with_max_retries(addr, Option<u32>)` [the `h1_` prefix = upstream protocol, per the file's
existing convention] + `drive_h2_once_with_body` [the general form — `drive_h2_once` now delegates to it]).

**Verification (quoted):**
- TDD RED: `h2_budget_blocked_retry_max_retries_zero` failed pre-gate with attempt-count 2 (the H2 loop ignored the budget).
- `cargo test -p envoy-http2` → `test result: ok. 65 passed; 0 failed` (62 + 3 new). `cargo test -p envoy-http1` → 99 passed (untouched).
- `cargo build --workspace --all-targets` + `cargo build -p envoy-http2` (standalone) → clean; clippy `-D warnings` → clean; fmt → clean.
- `git show --stat HEAD` → 1 file changed.

**Two-stage review:** spec-compliance **✅ compliant** (first pass; reviewer extracted both gates and diffed them
mechanically — IDENTICAL modulo indentation/path-prefix; arm ordering, post-loop guard, and preserved phase-16
lines all confirmed; the `h1_`-prefixed helper name confirmed as the file's established upstream-protocol
convention, not an asymmetry). Code-quality **Approved** (zero Critical / zero Important / 3 Minors — a 24-line
`drive_h2_once` copy [→ refactored to delegation], a vacuous `windows(80)` body assertion [→ direct
`starts_with`], and a naming note [no action] — **2 of 3 fixed in-task** [amended pre-push]).

---

## Task 7 — H2 request-budget gate (`max_requests` mirror) (commit `a977879e9`)

**Landed.** `crates/envoy-http2/src/hcm.rs` (+460/−144, single file; deletions dominated by the retry-loop
reindent under the new `else` wrapper): the Task-5 request-budget gate mirrored onto the H2 dispatch entry in
`handle_one_stream`'s `BuildOutcome::Proxy` arm — `cluster.try_acquire_request()` called **exactly once**
(bound into `request_acquire`; spec review confirmed exactly 1 production call site → no overflow double-tick),
same `if let Rejected … else { <guard match> + <the Task-6 retry loop, untouched> }` split. **H1/H2 structural
difference accommodated:** H1 assigns into a mutable `outgoing` flowing to its unified writer; H2's Rejected arm
produces `overflow_resp` as the arm's VALUE flowing into `finalize_h2_stream` (the same accounting path the H2
pool `PendingOverflow` arm uses — `downstream_rq_5xx` + access log fire identically). **Rejected path:** ticks
`upstream_rq_5xx` once (L3/ADR-0047; the H2 file now has exactly TWO tick sites — this arm + the phase-16
completing-response gate, both verified) + `synth_h2_overflow()` (81-byte body + `x-envoy-overloaded`) + gated
`x-envoy-attempt-count: 1` (L11; the literal-string form per the established H2 convention); no pool contact;
`upstream_rq_total` stays 0. **Acquired guard** spans the entire retry loop (L9b). Task-6 retry gate + phase-16
completing-response gate byte-untouched per `git diff -w`. 4 TDD tests (`h2_`-prefixed Task-5 mirrors, same
assertion sets) + `h1_backend_cluster_with_max_requests` helper (the `(manager, registry)` return shape per the
Task-5 docstring note); full reuse of the Task-6 helpers (`drive_h2_once_with_body` etc. — zero duplication).

**Verification (quoted):**
- TDD RED: tests (a)/(b) failed pre-gate (`left: 200 right: 503` — backend contacted, no gate); (c)/(d) passed pre-gate.
- `cargo test -p envoy-http2` → `test result: ok. 69 passed; 0 failed` (65 + 4 new). `cargo test -p envoy-http1` → 99 passed (untouched).
- `cargo build --workspace --all-targets` + `cargo build -p envoy-http2` (standalone) → clean; clippy `-D warnings` → clean; fmt → clean.
- `git show --stat HEAD` → 1 file changed.

**Two-stage review:** spec-compliance **✅ compliant** (first pass; single-acquire property, block-for-block H1
parity [only documented differences: `synth_h2_overflow()`, the value-flow vs mutable-assignment, protocol
identifiers], Rejected-path accounting flow, guard span, and both untouched-gate properties all verified).
Code-quality **Approved** (zero Critical / zero Important / zero actionable Minors; the literal-vs-const
`x-envoy-attempt-count` reference difference between H1/H2 confirmed as PRE-EXISTING phase-16 convention which
Task 7 correctly follows — noted as a possible future cleanup, not a defect).

**With Tasks 4–7 landed, both budget gates exist on both protocols. All four gate insertions are
in-process-tested; the bilateral differential proof is Task 8 (fixture 0025).**

---

_(Tasks 8–11 entries appended by the executor as each completes.)_
