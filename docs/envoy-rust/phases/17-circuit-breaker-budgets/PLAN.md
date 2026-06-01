# Phase 17 (`17-circuit-breaker-budgets`) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (the project default per `feedback_execution_style`; implementers dispatched SERIALLY per `feedback_serial_subagent_dispatch`) to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax. Run `cargo clippy --workspace --all-targets --all-features -- -D warnings` PER TASK (NOT deferred to state-4) per `project_state3_arc_skips_clippy`.

**Goal:** Make the cluster-scoped circuit-breaker budgets (`max_retries` retry breaker + `max_requests` request breaker + `track_remaining` gauge family) enforceable and observable on both the H1 and H2 router arms, proven bilaterally by fixture `0025-upstream-circuit-breaker-retry-budget` anchored on the deterministic zero-cap regime (ADR-0046 §0 — empirically CONFIRMED at this PLAN-write).

**Architecture:** Budget state + the new stat handles live in `envoy_cluster::Cluster` (RAII acquire/release guards over atomic active-counts — the ADR-0046 §5.4 cluster-scoped ownership boundary; NOT the per-protocol pools). The `max_retries` gate is one more conjunct at the phase-16 retry-decision points (H1 `hcm.rs:776-792`, H2 `hcm.rs:587-604`); the `max_requests` gate wraps the dispatch entry — the `RequestBudgetGuard` is acquired ONCE before the retry loop, spans all attempts, and is released after the final response (§6.2 lock-in L9b). Budget-overflow counters tick INSIDE the failed `try_acquire_*` (single source of truth, §5.3); the request-overflow local reply reuses the phase-15 `synth_overflow`/`synth_h2_overflow` helpers and ADDITIONALLY ticks `upstream_rq_5xx` per the L3 reconciliation (ADR-0047). The breaker `*_open` gauges are MOMENTARY — 1 only while at-or-over-capacity slots are actively held (L4 / ADR-0047) — so fixture 0025 asserts them value-exact at 0; the non-zero gauge edges + the >0-cap concurrency regime are in-process-backstop-only.

**Tech Stack:** Rust (workspace crates `envoy-config`, `envoy-cluster`, `envoy-http1`, `envoy-http2`, `envoy-bin`); `serde`/`serde_yaml`; `envoy-stats` `Counter`/`Gauge`; `testcontainers` differential harness; `cargo fuzz` (`parse_bootstrap`).

---

## §6.2 empirical lock-ins (verified against `envoyproxy/envoy:v1.33.0`, digest `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`, 2026-06-01; divergences landed as ADR-0047)

The state-2 PLAN-write ran the HEAVY SPEC §6.2 verification in Docker (5-cluster topology: `max_retries: 0` + retry / default-budget + `track_remaining` / `max_requests: 0` / `max_requests: 0` + retry / `max_requests: 1` + retry; always-503 + fail-once-then-succeed + counting backends; `%RESPONSE_FLAGS%` access log; admin `/stats`; every claim cross-checked against backend request counts). Findings — **the two marked ✦ DIVERGE materially from the SPEC projection and are reconciled by ADR-0047:**

- **L1 — `max_retries: 0` IS an always-open retry breaker (THE §0 anchor — CONFIRMED).** One GET against an always-503 backend behind `{max_retries: 0}` + `retry_policy {retry_on: "5xx", num_retries: 1}`: the FIRST attempt is dispatched (backend receives exactly 1 request; `upstream_rq_total: 1`; `upstream_cx_total: 1`); the would-be retry is budget-rejected — `upstream_rq_retry_overflow: 1`, `upstream_rq_retry: 0` — and the backend's real 503 surfaces downstream verbatim. Zero concurrency needed. Fixture-0025 probe 1 anchors here. (ADR-0047 does NOT fire on this item.)
- **L2 — `max_requests: 0` IS an always-open request breaker (CONFIRMED).** One GET against `{max_requests: 0}`: rejected with the 503 overflow local reply — body byte-identical to the phase-15 81-byte `upstream connect error or disconnect/reset before headers. reset reason: overflow` (hexdump-verified, no trailing newline) + `x-envoy-overloaded: true`; the backend is NEVER contacted (backend log empty; `upstream_rq_total: 0`).
- **L3 ✦ — the `max_requests`-overflow counter identity + co-firing counters (DIVERGES → ADR-0047).** The overflow counter IS `cluster.<name>.upstream_rq_pending_overflow` (the SAME counter phase 15 wired for `max_pending_requests` — the SPEC §2.1 projection held). BUT the overflow local reply ALSO ticks `upstream_rq_503: 1` + `upstream_rq_5xx: 1` + `upstream_rq_completed: 1` + `external.upstream_rq_503: 1`, AND `upstream_cx_total: 1` (connection-pool prefetch) — all while `upstream_rq_total` stays `0`. **Reconciliation (ADR-0047):** envoy-rust's request-budget overflow ticks `upstream_rq_pending_overflow` AND `upstream_rq_5xx` (the two co-firing names envoy-rust emits — a deliberate, narrowly-scoped departure from the phase-16 "synth local replies never tick `upstream_rq_5xx`" posture, justified by this empirical finding); `upstream_rq_503`/`upstream_rq_completed`/`external.*` stay un-emitted (Envoy-only, unasserted). `upstream_cx_total` is NOT asserted on probe 3's cluster (Envoy 1 [prefetch] vs envoy-rust 0 [pool never contacted] — known divergence recorded in BEHAVIOR_CONTRACT). The SPEC §1 probe-3 `upstream_cx_total: 0` assertion is DROPPED.
- **L4 ✦ — the breaker `*_open` gauges are MOMENTARY, not latched (DIVERGES → ADR-0047).** `circuit_breakers.default.{rq_retry_open, rq_open}` read **0 at rest AND 0 after every sequential probe** — for ALL configs INCLUDING the zero caps. They never latch to 1 in the sequential regime. The empirically-consistent semantic: **gauge = 1 iff `active > 0 AND active >= max`** (at-or-over capacity with at least one actively-held slot) — consistent with the phase-15 finding that `cx_open` is 1 exactly when `upstream_cx_active == max_connections` (cap > 0, slots held) AND with the zero-cap reading of 0 (a zero-cap breaker never has held slots). **Reconciliation (ADR-0047):** fixture 0025 asserts `rq_retry_open: 0` + `rq_open: 0` value-exact (the bilateral name+value check — the fixture-0023 `cx_open: 0` pattern); the SPEC §1 probe-1 `rq_retry_open: 1` and probe-3 `rq_open: 1` assertions are DROPPED; the non-zero gauge edge is asserted in the in-process backstop only (>0-cap concurrency regime, where slots ARE held).
- **L5 — defaults (CONFIRMED).** With `circuit_breakers` configured but the field unset: `max_retries` default = **3**; `max_requests` default = **1024** (also observed: `max_connections` 1024, `max_pending_requests` 1024, `max_connection_pools` = u64::MAX sentinel). Read via the at-rest `remaining_*` gauges under `track_remaining: true`. Fixture-0025 probe 2 asserts `remaining_retries: 3` + `remaining_rq: 1024` — a bilateral differential proof of the defaults.
- **L6 — budget-blocked-retry wire shape (CONFIRMED).** The downstream response is the backend's real 503 VERBATIM (status + the backend's distinctive body + `x-envoy-upstream-service-time` present; NO `x-envoy-overloaded`; content-length = the backend body's length, not 81). Access-log `%RESPONSE_FLAGS%` = `UO` (access-log-only — never a response header). With the vhost flag set, `x-envoy-attempt-count: 1`.
- **L7 — counter exclusivity on budget-blocked retry (CONFIRMED).** `upstream_rq_retry_overflow: 1` with `upstream_rq_retry: 0` AND `upstream_rq_retry_success: 0` AND `upstream_rq_retry_limit_exceeded: 0` AND `upstream_rq_total: 1`. No `cluster.<name>.retry.*` sub-scope appears (it is created only when a retry actually executes).
- **L8 — `track_remaining` absent-vs-0 (CONFIRMED).** Without `track_remaining: true`, the `remaining_*` gauges are entirely ABSENT from `/stats` (grep exit 1) — NOT present-at-0. With it, Envoy emits all 5 under `default` only: `remaining_cx`, `remaining_pending`, `remaining_rq`, `remaining_retries`, `remaining_cx_pools` (the last reads the u64::MAX sentinel). envoy-rust emits ONLY `remaining_retries` + `remaining_rq` (minimum-viable per SPEC D6; the other 3 track pool-owned/meaningless quantities — Envoy-only, unasserted).
- **L9 — gate ordering + request-budget lifetime (CONFIRMED).** **(a)** With `max_requests: 0` AND a `retry_policy`, the REQUEST breaker fires first: 81-byte overflow local reply, `upstream_rq_pending_overflow: 1`, `upstream_rq_retry_overflow: 0`, no upstream contact — the retry budget is never consulted. **(b)** A request counts ONCE against `max_requests` for its entire lifetime INCLUDING retries: `max_requests: 1` + a sequential fail-once-then-succeed retry → final 200, `upstream_rq_pending_overflow: 0`, backend saw 2 requests, `remaining_rq` back to 1 after. Implementation: the `RequestBudgetGuard` is acquired ONCE at dispatch entry (before the retry loop) and released after the final response.
- **L10 — control path (CONFIRMED).** Default-budget retry proceeds normally: final 200, `upstream_rq_retry: 1`, `upstream_rq_retry_success: 1`, `upstream_rq_retry_overflow: 0`, `upstream_rq_total: 2`, `x-envoy-attempt-count: 2`, backend saw 2 requests. (Envoy also ticks `upstream_rq_retry_backoff_exponential: 1` — Envoy-only, unasserted.)
- **L11 — M16-3 RESOLVED (the opportunistic item).** With `include_attempt_count_in_response: true`, Envoy emits `x-envoy-attempt-count` on ALL response paths INCLUDING the synthesized overflow local reply (value `1` — one admitted attempt, even though no upstream request was sent). envoy-rust's existing synth-path emission (`: 1`) — flagged by the phase-16 review as an "unverified extrapolation" — is now empirically VERIFIED CORRECT. Disposition: no code change to existing paths; the M16-3 code-comment overstatement can be corrected opportunistically at Task 5; fixture 0025 sets the vhost flag and asserts `x-envoy-attempt-count` per probe (`1`/`2`/`1`), giving M16-3 bilateral differential coverage. The NEW request-budget overflow local reply MUST also carry `x-envoy-attempt-count: 1` when the flag is set (Tasks 5/7).
- **L12 — full Envoy-side gauge enumeration (for BEHAVIOR_CONTRACT + fixture notes).** Per cluster with `circuit_breakers`: `circuit_breakers.{default,high}.{cx_open, cx_pool_open, rq_open, rq_pending_open, rq_retry_open}` (10 gauges, always emitted). With `track_remaining: true`: + the 5 `circuit_breakers.default.remaining_*` gauges (default priority only). envoy-rust phase-17 emission: `default.rq_open` + `default.rq_retry_open` (conditional on `circuit_breakers` configured — the phase-15 `cx_open` discipline) + `default.remaining_retries`/`default.remaining_rq` (conditional on `track_remaining: true`); plus the unconditional `upstream_rq_retry_overflow` counter (joins the phase-16 retry-counter family — Envoy emits it for every cluster, the same posture that kept fixture 0011's set-diff green at phase 16).

## PLAN-time SPEC corrections (verified against HEAD `ee61fc744`)

A read-only Explore subagent verified all 20 SPEC §0/§3 code anchors against HEAD. **ZERO drift found** — all anchors are accurate. Confirmations a task-implementer needs:

- `Thresholds` `crates/envoy-config/src/bootstrap.rs:1319-1328` (priority + max_connections + max_pending_requests; `deny_unknown_fields`); `validate_circuit_breakers` `:2729-2767`; `ConfigError` is in `crates/envoy-config/src/lib.rs` (the 4 existing CB variants at `:445`/`:452`/`:462`/`:469`).
- H1: retry-decision `crates/envoy-http1/src/hcm.rs:776-792`; `run_attempt` `:309-316`; `AttemptResult` `:281-299` (fields `response`/`endpoint`/`outcome`/`upstream_response`); dispatch entry `:701-748` (`BuildOutcome::Proxy` arm; `cluster = config.cluster_mgr.get(&cluster_name)` → `Arc<Cluster>` held as `&` through the loop); completing `upstream_rq_5xx` gate `:805-807`; `synth_overflow(close)` `:1245-1268`.
- H2: retry-decision `crates/envoy-http2/src/hcm.rs:587-604`; `run_h2_attempt` `:173-179`; dispatch entry `:515-553`; completing `upstream_rq_5xx` gate `:615-617`; `synth_h2_overflow()` `:857-872`.
- Pools: conditional CB-stat registration H1 `crates/envoy-http1/src/pool.rs:460-510`, H2 `crates/envoy-http2/src/pool.rs:609-650` (gated on `cfg.circuit_breakers.is_some()`; reads Thresholds via `.and_then(|cb| cb.thresholds.first()).and_then(|t| t.max_*)`).
- Cluster: struct `crates/envoy-cluster/src/cluster.rs:78-145` (the 3 phase-16 retry counters registered UNCONDITIONALLY); `from_bootstrap` registration `:742-809`.
- envoy-stats: `register_counter(&str) -> Result<Arc<Counter>>` / `register_gauge(&str) -> Result<Arc<Gauge>>` (idempotent same-kind); `Gauge::{set, inc, dec, value}`.
- Harness: `Driver::Http1KeepAlive` `tests/differential/src/lib.rs:167-173`; `Http1KeepAliveRequest` `:314-344` supports `expected_status` + `expected_body` (`Http1BodyRule::ByteExact`) + `require_header_present` + `require_header_absent` + `require_header_value` (`Http1HeaderValueRule {name, value}`) + cumulative `expected_stats`. The stateful backend `tests/helpers/health-aware-http1-backend` supports `--per-path PATH=STATUS` (stateless; 503 body `service unavailable\n`, 20 bytes) + `--retry-script PATH=fail:N` (cyclic windows; bodies `fail\n`/`ok\n`).
- Fuzz: 28 seeds; `cluster_circuit_breakers.yaml` exists (max_connections: 4 + max_pending_requests: 0 — extend IN PLACE, no new seed file needed → no `.gitignore`/SUCCESS-array churn); SUCCESS array + `.gitignore` allow-list confirmed in sync.
- **No deep-clone edits needed:** `circuit_breakers`/`Thresholds` is CLUSTER config (looked up at dispatch time via `cluster_mgr.get`), not route config — `clone_route_action`/`clone_route_config` are untouched by phase 17 (verified: they clone only route-level fields).
- Backstops: `crates/envoy-bin/tests/upstream_retry.rs` (phase-16; boot + request + stats-scrape pattern) + `crates/envoy-bin/tests/upstream_circuit_breaker.rs` (phase-15) are the templates. Docker wrapper template: `tests/differential/tests/upstream_circuit_breaker.rs` (`differential::run_fixture(&dir)`).

## §6.1 split-gate decision

Re-estimated against the §6.2-refined surface (which DROPS the latched-gauge fixture assertions [simpler], ADDS the `upstream_rq_5xx` overflow tick + the per-probe `x-envoy-attempt-count` assertions [small]): **~1450–1550 LoC / 11 tasks** (production ~415, tests ~650, fixture/harness/backstop ~525, docs ~80) — at the boundary but NOT over the `BOOTSTRAP_PROMPT.md` §6.1 ~1500-LoC / ~25-task gate. **Single un-split phase.** The work is tightly coupled (one `BudgetState` consumed by both protocols; one fixture asserts all three probes; the gauge semantics span D3–D7), so a 17.1/17.2 split would fragment the budget primitive from its only callers for little benefit. **ADR-0048 (the reserved split ADR) does NOT fire.** (If a single task's sub-steps blow up past ~10 items mid-execution, §6.1 permits a mid-execution split.)

---

## File structure

- `crates/envoy-config/src/bootstrap.rs` — `Thresholds` gains `max_requests`/`max_retries`/`track_remaining`; `validate_circuit_breakers` accepts the new fields (0 included); fuzz-seed SUCCESS coverage via the in-place `cluster_circuit_breakers.yaml` extension.
- `crates/envoy-cluster/src/budget.rs` — NEW module: `BudgetState` (caps + atomic active-counts + stat handles), `RetryBudgetGuard`, `RequestBudgetGuard` (RAII).
- `crates/envoy-cluster/src/cluster.rs` — `Cluster` gains `budget: Option<BudgetState>` + `upstream_rq_retry_overflow` counter (unconditional) + `try_acquire_retry()`/`try_acquire_request()` delegation; `from_bootstrap` budget resolution + conditional stat registration.
- `crates/envoy-cluster/src/lib.rs` — `pub mod budget;` + re-exports.
- `crates/envoy-http1/src/hcm.rs` — retry-budget conjunct at `:776-792`; request-budget gate at the `:701-748` dispatch entry; overflow local reply + `upstream_rq_5xx` tick + `x-envoy-attempt-count: 1`.
- `crates/envoy-http2/src/hcm.rs` — the H2 mirrors (retry-budget at `:587-604`; request-budget at `:515-553`).
- `tests/fixtures/0025-upstream-circuit-breaker-retry-budget/` — `envoy.yaml`, `envoy-rust.yaml`, `expectations.yaml`, `README.md`.
- `tests/differential/tests/upstream_circuit_breaker_budgets.rs` — Docker-gated wrapper.
- `crates/envoy-bin/tests/upstream_circuit_breaker_budgets.rs` — in-process backstop (4 paths incl. the >0-cap concurrency regime).
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_circuit_breakers.yaml` — extended in place (no new file).
- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — "17 entries (circuit-breaker budgets)" stat rows + wire-shape notes + the §5.4 registration-seam note.

---

### Task 1: `envoy-config` schema — `Thresholds` budget fields + validator acceptance

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (`Thresholds` `:1319-1328`; `validate_circuit_breakers` `:2729-2767`)
- Test: `crates/envoy-config/src/bootstrap.rs` `#[cfg(test)]` module (mirror the phase-15 circuit-breaker serde/validator tests)

- [ ] **Step 1: Write failing serde + validator tests.** (a) a cluster YAML with `circuit_breakers: {thresholds: [{priority: DEFAULT, max_requests: 0, max_retries: 0, track_remaining: true}]}` parses with the three new fields populated; (b) the new fields absent → all three `None`; (c) a threshold with the still-deferred `retry_budget: {budget_percent: 20}` → parse ERROR (`deny_unknown_fields`); (d) same for `max_connection_pools: 1`; (e) `validate_circuit_breakers` ACCEPTS `max_requests: 0` and `max_retries: 0` (the always-open configs — contrast with the existing `max_connections == 0` rejection); (f) the existing rejections still fire (multiple thresholds; HIGH priority; `max_pending_requests > 0`).

```rust
#[test]
fn thresholds_parse_budget_fields() {
    let yaml = r#"
name: c
type: STRICT_DNS
lb_policy: ROUND_ROBIN
circuit_breakers:
  thresholds:
    - priority: DEFAULT
      max_requests: 0
      max_retries: 0
      track_remaining: true
load_assignment:
  cluster_name: c
  endpoints: []
"#;
    let c: Cluster = serde_yaml::from_str(yaml).unwrap();
    let t = &c.circuit_breakers.as_ref().unwrap().thresholds[0];
    assert_eq!(t.max_requests, Some(0));
    assert_eq!(t.max_retries, Some(0));
    assert_eq!(t.track_remaining, Some(true));
}

#[test]
fn thresholds_reject_deferred_budget_fields() {
    // retry_budget + max_connection_pools stay rejected by deny_unknown_fields
    let yaml = r#"
thresholds:
  - priority: DEFAULT
    retry_budget: { budget_percent: 20 }
"#;
    assert!(serde_yaml::from_str::<CircuitBreakers>(yaml).is_err());
}

#[test]
fn validate_circuit_breakers_accepts_zero_budget_caps() {
    // max_requests: 0 / max_retries: 0 are the always-open-breaker configs (§0 / L1 / L2)
    // — ACCEPTED, in contrast to max_connections == 0 (InvalidMaxConnections).
    let c = cluster_with_thresholds(Thresholds {
        priority: Some(RoutingPriority::Default),
        max_connections: None,
        max_pending_requests: None,
        max_requests: Some(0),
        max_retries: Some(0),
        track_remaining: None,
    });
    assert!(validate_circuit_breakers(&c).is_ok());
}
```

- [ ] **Step 2: Run tests, verify they fail.** Run: `cargo test -p envoy-config circuit_breaker -- --nocapture`. Expected: FAIL (unknown fields `max_requests`/`max_retries`/`track_remaining`).
- [ ] **Step 3: Add the three fields to `Thresholds`** (`:1319-1328`):

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Thresholds {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<RoutingPriority>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_connections: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_pending_requests: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_requests: Option<u32>, // 17 D1: request-budget cap (0 = always-open; L2)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<u32>, // 17 D1: retry-budget cap (0 = always-open; L1)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track_remaining: Option<bool>, // 17 D1: emit remaining_* gauges (L8)
}
```

No `validate_circuit_breakers` change is REQUIRED (the new fields need no semantic rejection — `0` is valid for both caps per L1/L2; zero new `ConfigError` variants). Add a comment in the validator documenting WHY `max_requests: 0`/`max_retries: 0` are accepted while `max_connections: 0` is rejected (the always-open-breaker semantic vs the phase-13 connection-cap rationale).

- [ ] **Step 4: Run tests, verify pass.** Run: `cargo test -p envoy-config circuit_breaker`. Expected: PASS. Also `cargo test -p envoy-config` (full crate) + `cargo build --workspace` (the new fields are `Option` — no exhaustive-literal breaks expected, but the phase-16 Task-1 lesson says verify; the phase-15 pool tests construct `Thresholds` literals at `crates/envoy-http1/src/pool.rs` + `crates/envoy-http2/src/pool.rs` test modules and MUST be extended with `max_requests: None, max_retries: None, track_remaining: None` in the same commit if they use exhaustive struct literals).
- [ ] **Step 5: clippy + fmt + commit.**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets --all-features -- -D warnings
git add crates/envoy-config/src/bootstrap.rs crates/envoy-http1/src/pool.rs crates/envoy-http2/src/pool.rs
git commit -m "phase 17 Task 1: Thresholds budget fields (max_requests/max_retries/track_remaining) [ADR-0047]"
```

---

### Task 2: `envoy-cluster` — `BudgetState` + RAII guards (the D3 architectural core)

**Files:**
- Create: `crates/envoy-cluster/src/budget.rs`
- Modify: `crates/envoy-cluster/src/lib.rs` (`pub mod budget;` + re-exports)
- Test: `crates/envoy-cluster/src/budget.rs` `#[cfg(test)]` module

- [ ] **Step 1: Write failing tests** for the acquisition semantics:

```rust
#[test]
fn zero_cap_always_fails_acquisition() {
    // L1/L2: a 0 cap is an always-open breaker — acquisition fails from construction.
    let b = BudgetState::new(0, 0, false, &test_registry(), "c").unwrap();
    assert!(b.try_acquire_retry().is_none());
    assert!(b.try_acquire_request().is_none());
}

#[test]
fn guard_release_frees_the_slot() {
    let b = BudgetState::new(3, 1024, false, &test_registry(), "c").unwrap();
    let g1 = b.try_acquire_retry().expect("slot 1");
    let g2 = b.try_acquire_retry().expect("slot 2");
    let g3 = b.try_acquire_retry().expect("slot 3");
    assert!(b.try_acquire_retry().is_none()); // cap 3 reached
    drop(g1);
    assert!(b.try_acquire_retry().is_some()); // slot freed
    drop((g2, g3));
}

#[test]
fn overflow_counter_ticks_inside_failed_acquire() {
    // §5.3 single source of truth: the failed try_acquire_retry ticks
    // upstream_rq_retry_overflow; the failed try_acquire_request ticks
    // upstream_rq_pending_overflow (L3). Callers never tick these directly.
    let reg = test_registry();
    let b = BudgetState::new(0, 0, false, &reg, "c").unwrap();
    assert!(b.try_acquire_retry().is_none());
    assert_eq!(counter_value(&reg, "cluster.c.upstream_rq_retry_overflow"), 1);
    assert!(b.try_acquire_request().is_none());
    assert_eq!(counter_value(&reg, "cluster.c.upstream_rq_pending_overflow"), 1);
}
```

- [ ] **Step 2: Run, verify fail.** Run: `cargo test -p envoy-cluster budget`. Expected: FAIL (module does not exist).
- [ ] **Step 3: Implement `budget.rs`.**

```rust
//! 17 D3: cluster-scoped circuit-breaker budget primitives (ADR-0046 §5.4).
//! Budget state lives on the CLUSTER (not the per-protocol pools): max_retries /
//! max_requests are cluster-wide concepts spanning both protocol pools.
//! Stat side-effects (overflow counters + gauges) live INSIDE the acquire/release
//! paths — single source of truth (§5.3); H1/H2 callers never touch them directly.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

pub struct BudgetState {
    max_retries: u32,
    max_requests: u32,
    active_retries: Arc<AtomicI64>,
    active_requests: Arc<AtomicI64>,
    // Overflow counters (tick inside the failed acquire — §5.3 / L3 / L7):
    rq_retry_overflow: Arc<envoy_stats::Counter>,
    rq_pending_overflow: Arc<envoy_stats::Counter>,
    // Momentary breaker gauges (L4 / ADR-0047): 1 iff active > 0 AND active >= max.
    rq_retry_open: Arc<envoy_stats::Gauge>,
    rq_open: Arc<envoy_stats::Gauge>,
    // remaining_* gauges (L8): registered ONLY when track_remaining: true.
    remaining_retries: Option<Arc<envoy_stats::Gauge>>,
    remaining_rq: Option<Arc<envoy_stats::Gauge>>,
}

impl BudgetState {
    /// Compare-and-increment under the cap; on failure tick the retry-overflow
    /// counter (L7: exactly one counter ticks on a budget-blocked retry).
    pub fn try_acquire_retry(self: &Arc<Self>) -> Option<RetryBudgetGuard> { /* CAS loop */ }
    /// Same for the request budget; on failure tick upstream_rq_pending_overflow (L3).
    pub fn try_acquire_request(self: &Arc<Self>) -> Option<RequestBudgetGuard> { /* CAS loop */ }
    fn update_retry_gauges(&self) { /* rq_retry_open per L4 semantic; remaining_retries = max - active, floored 0 */ }
    fn update_request_gauges(&self) { /* rq_open + remaining_rq, same shapes */ }
}

/// RAII: releases the retry-budget slot + updates gauges on drop (the 13.x PoolGuard discipline).
pub struct RetryBudgetGuard { state: Arc<BudgetState> }
impl Drop for RetryBudgetGuard { fn drop(&mut self) { /* active_retries -= 1; update gauges */ } }

/// RAII: releases the request-budget slot on drop. Acquired ONCE per downstream
/// request at dispatch entry; spans the entire retry loop (L9b).
pub struct RequestBudgetGuard { state: Arc<BudgetState> }
impl Drop for RequestBudgetGuard { fn drop(&mut self) { /* active_requests -= 1; update gauges */ } }
```

The acquire CAS loop: `loop { let cur = active.load(Acquire); if cur >= max as i64 { overflow_counter.inc(); update_gauges(); return None; } if active.compare_exchange(cur, cur+1, AcqRel, Acquire).is_ok() { update_gauges(); return Some(Guard{..}); } }`. The gauge update per L4: `open.set(if active > 0 && active >= max as i64 { 1 } else { 0 })`; `remaining.set((max as i64 - active).max(0))`.

`BudgetState::new(max_retries, max_requests, track_remaining, registry, cluster_name)` registers: `cluster.<name>.upstream_rq_retry_overflow` (counter), `cluster.<name>.upstream_rq_pending_overflow` (counter — IDEMPOTENT-shared with the pools' phase-15 handle: `register_counter` on an existing name returns the same `Arc`), `cluster.<name>.circuit_breakers.default.rq_retry_open` + `.rq_open` (gauges), and — only when `track_remaining` — `.remaining_retries` + `.remaining_rq` (gauges, initialized to the cap values per L5/L8).

- [ ] **Step 4: Run, verify pass.** Run: `cargo test -p envoy-cluster budget` then `cargo test -p envoy-cluster` (full). Expected: PASS.
- [ ] **Step 5: clippy + fmt + commit.**

```bash
cargo fmt --all && cargo clippy -p envoy-cluster --all-targets --all-features -- -D warnings
git add crates/envoy-cluster/src/budget.rs crates/envoy-cluster/src/lib.rs
git commit -m "phase 17 Task 2: BudgetState + RAII budget guards (cluster-scoped, ADR-0046 SS5.4) [ADR-0047]"
```

---

### Task 3: `envoy-cluster` — `Cluster` budget integration + conditional registration

**Files:**
- Modify: `crates/envoy-cluster/src/cluster.rs` (`Cluster` struct `:78-145`; `from_bootstrap` `:742-809`)
- Test: `crates/envoy-cluster/src/cluster.rs` test module

- [ ] **Step 1: Write failing tests.** (a) a cluster WITHOUT `circuit_breakers` → `cluster.budget()` is `None`, `try_acquire_retry()`/`try_acquire_request()` always succeed (returning `None` guard / a no-op marker — see Step 3 design), and NO `circuit_breakers.default.rq_open`/`rq_retry_open`/`remaining_*` stats are registered (assert via registry scrape) — but `upstream_rq_retry_overflow` IS registered (unconditional, inert at 0; L12); (b) a cluster WITH `circuit_breakers: {thresholds: [{max_retries: 0}]}` → budget present, retry acquisition fails, `max_requests` resolved to default 1024 (L5); (c) a cluster with `circuit_breakers` + `track_remaining: true` → `remaining_retries`/`remaining_rq` registered at the cap values; without → ABSENT (L8); (d) defaults: `circuit_breakers: {thresholds: [{}]}` → max_retries 3 / max_requests 1024 (L5).
- [ ] **Step 2: Run, verify fail.** Run: `cargo test -p envoy-cluster budget_integration`. Expected: FAIL.
- [ ] **Step 3: Implement.**
  - `Cluster` gains `pub(crate) budget: Option<Arc<BudgetState>>` + `pub(crate) upstream_rq_retry_overflow: Arc<envoy_stats::Counter>` (the unconditional handle — registered next to the phase-16 retry counters at `:742-809`; `BudgetState` shares the same underlying counter via idempotent registration when present).
  - `from_bootstrap`: after the existing retry-counter registration, resolve the budget: `if let Some(cb) = cfg.circuit_breakers.as_ref() { let t = cb.thresholds.first(); let max_retries = t.and_then(|t| t.max_retries).unwrap_or(3); let max_requests = t.and_then(|t| t.max_requests).unwrap_or(1024); let track = t.and_then(|t| t.track_remaining).unwrap_or(false); budget = Some(Arc::new(BudgetState::new(max_retries, max_requests, track, registry, &cfg.name)?)); }` (L5 defaults: 3 / 1024).
  - Public API on `Cluster`: `pub fn try_acquire_retry(&self) -> BudgetAcquisition<RetryBudgetGuard>` and `pub fn try_acquire_request(&self) -> BudgetAcquisition<RequestBudgetGuard>` where `enum BudgetAcquisition<G> { Unlimited, Acquired(G), Rejected }` — `Unlimited` when `budget` is `None` (no circuit_breakers → never gate; zero stat side-effects), `Acquired(guard)` / `Rejected` when present. (This three-state shape keeps the H1/H2 call sites to a single `match` with no `Option<Option<_>>` nesting — a clippy-friendly shape per `project_state3_arc_skips_clippy`.)
- [ ] **Step 4: Run, verify pass.** Run: `cargo test -p envoy-cluster` (full). Expected: PASS.
- [ ] **Step 5: clippy + fmt + commit.**

```bash
cargo fmt --all && cargo clippy -p envoy-cluster --all-targets --all-features -- -D warnings
git add crates/envoy-cluster/src/cluster.rs crates/envoy-cluster/src/budget.rs crates/envoy-cluster/src/lib.rs
git commit -m "phase 17 Task 3: Cluster budget integration + conditional stat registration [ADR-0047]"
```

---

### Task 4: H1 retry-budget gate (`max_retries`)

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` (retry-decision `:776-792`)
- Test: `crates/envoy-http1/src/hcm.rs` test module

- [ ] **Step 1: Write failing tests.** (a) **budget-blocked retry (L1/L6/L7):** in-process H1 HCM + always-503 backend + route `retry_policy {retry_on: "5xx", num_retries: 1}` + cluster `circuit_breakers {thresholds: [{max_retries: 0}]}` + vhost flag true → downstream response is the backend's 503 VERBATIM (status + body; NOT the 81-byte overflow body; no `x-envoy-overloaded`), `x-envoy-attempt-count: 1`, `upstream_rq_retry_overflow == 1`, `upstream_rq_retry == 0`, `upstream_rq_retry_limit_exceeded == 0`, `upstream_rq_total == 1`, backend saw exactly 1 request; (b) **budget-allowed control (L10):** same but `max_retries` unset (default 3) + fail-once-then-succeed backend → 200, `x-envoy-attempt-count: 2`, `upstream_rq_retry == 1`, `upstream_rq_retry_success == 1`, `upstream_rq_retry_overflow == 0`, `upstream_rq_total == 2`; (c) **regression:** no `circuit_breakers` at all + retry → identical to (b) except no budget stats registered.
- [ ] **Step 2: Run, verify fail.** Run: `cargo test -p envoy-http1 budget`. Expected: FAIL.
- [ ] **Step 3: Implement.** At the retry-decision point (`:776-792`), the budget becomes one more conjunct (§5.5 — composes with, never replaces):

```rust
if final_retriable && attempts <= max_retries {
    // 17 D4: the retry-budget gate (ADR-0046). A retriable outcome with
    // attempts remaining ADDITIONALLY requires a retry-budget slot. On
    // Rejected, the failed try_acquire_retry has already ticked
    // upstream_rq_retry_overflow (§5.3); the would-be-retried response
    // surfaces downstream verbatim (L6) — fall through to the break.
    match cluster.try_acquire_retry() {
        envoy_cluster::BudgetAcquisition::Unlimited => {
            cluster.upstream_rq_retry().inc();
            if let Some(d) = RetryConfig::backoff(attempts) {
                tokio::time::sleep(d).await;
            }
            continue;
        }
        envoy_cluster::BudgetAcquisition::Acquired(_retry_guard) => {
            // Hold the guard across the back-off + the next attempt: assign it
            // to a loop-scoped slot dropped at the next iteration's completion.
            cluster.upstream_rq_retry().inc();
            if let Some(d) = RetryConfig::backoff(attempts) {
                tokio::time::sleep(d).await;
            }
            retry_guard_slot = Some(_retry_guard);
            continue;
        }
        envoy_cluster::BudgetAcquisition::Rejected => {
            // Budget-blocked: do NOT tick upstream_rq_retry (the retry never
            // happens), do NOT tick _limit_exceeded (L7 exclusivity).
            // final_retriable stays true but the post-loop success/limit split
            // must NOT fire either — mark the block.
            retry_budget_blocked = true;
        }
    }
}
break (attempt.response, attempt.upstream_response);
```

Where `retry_guard_slot: Option<RetryBudgetGuard>` is declared before the loop (each `Some` assignment drops the prior guard — i.e., the guard for retry N is held until retry N+1 fires or the loop exits) and `retry_budget_blocked: bool` is declared before the loop, checked in the post-loop `retry_success`/`limit_exceeded` split: a budget-blocked exit ticks NEITHER (L7). NOTE the exact loop-variable threading is the implementer's to finalize against the real loop shape at `:748-819` — the binding constraints are: (i) overflow tick happens inside `try_acquire_retry` (NOT here); (ii) on `Rejected` the response surfaces verbatim with no further retry-counter ticks; (iii) the guard lifetime covers the retried attempt; (iv) `Unlimited` behaves byte-identically to phase-16 (regression).

- [ ] **Step 4: Run, verify pass.** Run: `cargo test -p envoy-http1` (full — the phase-16 retry tests must stay green). Expected: PASS.
- [ ] **Step 5: clippy + fmt + commit.** (The new `match` inside `if` is a `collapsible_if`/`single_match_else` lint candidate — run clippy NOW per `project_state3_arc_skips_clippy`.)

```bash
cargo fmt --all && cargo clippy -p envoy-http1 --all-targets --all-features -- -D warnings
git add crates/envoy-http1/src/hcm.rs
git commit -m "phase 17 Task 4: H1 retry-budget gate (max_retries) [ADR-0047]"
```

---

### Task 5: H1 request-budget gate (`max_requests`)

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` (dispatch entry `:701-748`; `synth_overflow` caller wiring)
- Test: `crates/envoy-http1/src/hcm.rs` test module

- [ ] **Step 1: Write failing tests.** (a) **request-breaker overflow (L2/L3/L11):** in-process H1 HCM + cluster `circuit_breakers {thresholds: [{max_requests: 0}]}` (no retry_policy) + vhost flag true → downstream 503 with the 81-byte overflow body + `x-envoy-overloaded: true` + `x-envoy-attempt-count: 1`, backend NEVER contacted, `upstream_rq_pending_overflow == 1`, `upstream_rq_5xx == 1` (the L3 reconciliation), `upstream_rq_total == 0`, `upstream_rq_retry_overflow == 0` (L9a exclusivity); (b) **gate ordering (L9a):** `max_requests: 0` AND `retry_policy` → same as (a); the retry budget is never consulted; (c) **request-budget lifetime (L9b):** `max_requests: 1` + retry_policy + fail-once-then-succeed backend → final 200 (the sequential retry does NOT overflow `max_requests: 1`), `upstream_rq_pending_overflow == 0`; (d) **regression:** no `circuit_breakers` → no behavior change.
- [ ] **Step 2: Run, verify fail.** Run: `cargo test -p envoy-http1 request_budget`. Expected: FAIL.
- [ ] **Step 3: Implement.** At the dispatch entry (after the cluster lookup `:709-712`, BEFORE the retry loop):

```rust
// 17 D5: the request-budget gate (max_requests; ADR-0046/ADR-0047). Acquired
// ONCE per downstream request; the guard spans the entire retry loop (L9b);
// released on drop after the final response is written. Fires BEFORE the
// retry loop and BEFORE any pool contact (L9a gate ordering).
let _request_guard = match cluster.try_acquire_request() {
    envoy_cluster::BudgetAcquisition::Unlimited => None,
    envoy_cluster::BudgetAcquisition::Acquired(g) => Some(g),
    envoy_cluster::BudgetAcquisition::Rejected => {
        // The failed acquire already ticked upstream_rq_pending_overflow (§5.3).
        // L3 (ADR-0047): Envoy's overflow local reply ALSO ticks upstream_rq_5xx
        // — mirror it here (the ONLY synth path that ticks it; the phase-16
        // completing-response gate for all other paths is unchanged).
        cluster.upstream_rq_5xx().inc();
        let mut resp = synth_overflow(close);
        // L11: the overflow local reply carries x-envoy-attempt-count: 1 when
        // the vhost flag is set (Envoy-verified; closes M16-3 differentially).
        if include_attempt_count_in_response {
            resp.headers.push((X_ENVOY_ATTEMPT_COUNT.to_string(), "1".to_string()));
        }
        // ... write resp downstream via the existing synth-response writer arm
        // (the same path the pool PendingOverflow arm uses), record the access
        // log + downstream_rq_5xx as that arm does, and return from the
        // request handling (no pool contact, no retry loop).
    }
};
```

The exact early-return mechanics mirror the existing `PoolError::PendingOverflow` arm (`hcm.rs:569` region) — the implementer reuses that arm's response-writing/accounting path. Also correct the M16-3 code-comment overstatement at the existing synth-path attempt-count emission site (the comment may now cite this PLAN's L11 empirical verification instead of "extrapolation").

- [ ] **Step 4: Run, verify pass.** Run: `cargo test -p envoy-http1` (full). Expected: PASS.
- [ ] **Step 5: clippy + fmt + commit.**

```bash
cargo fmt --all && cargo clippy -p envoy-http1 --all-targets --all-features -- -D warnings
git add crates/envoy-http1/src/hcm.rs
git commit -m "phase 17 Task 5: H1 request-budget gate (max_requests) + overflow local reply [ADR-0047]"
```

---

### Task 6: H2 retry-budget gate (mirror of Task 4)

**Files:**
- Modify: `crates/envoy-http2/src/hcm.rs` (retry-decision `:587-604`)
- Test: `crates/envoy-http2/src/hcm.rs` test module

- [ ] **Step 1: Write failing tests.** Mirror Task 4's three tests on the H2 arm (budget-blocked / budget-allowed control / no-circuit_breakers regression), using the existing H2 in-crate test harness + stateful in-test backends. Sibling-test naming parity with Task 4 (the 13.x→16 `h2_`-prefix discipline).
- [ ] **Step 2: Run, verify fail.** Run: `cargo test -p envoy-http2 budget`. Expected: FAIL.
- [ ] **Step 3: Implement.** Mirror Task 4's gate verbatim at the H2 retry-decision point (`:587-604`), using the same `BudgetAcquisition` three-state match, the same guard-slot threading, and the same blocked-exit counter exclusivity. The H1/H2 sibling parity (same match arms, same comments, same variable names modulo `h2_` prefixes) is the review gate — the phase-16 T5 review caught exactly this class of asymmetry.
- [ ] **Step 4: Run, verify pass.** Run: `cargo test -p envoy-http2` (full). Expected: PASS.
- [ ] **Step 5: clippy + fmt + commit.**

```bash
cargo fmt --all && cargo clippy -p envoy-http2 --all-targets --all-features -- -D warnings
git add crates/envoy-http2/src/hcm.rs
git commit -m "phase 17 Task 6: H2 retry-budget gate (max_retries mirror) [ADR-0047]"
```

---

### Task 7: H2 request-budget gate (mirror of Task 5)

**Files:**
- Modify: `crates/envoy-http2/src/hcm.rs` (dispatch entry `:515-553`; `synth_h2_overflow` caller wiring)
- Test: `crates/envoy-http2/src/hcm.rs` test module

- [ ] **Step 1: Write failing tests.** Mirror Task 5's four tests on the H2 arm (overflow / gate-ordering / lifetime / regression).
- [ ] **Step 2: Run, verify fail.** Run: `cargo test -p envoy-http2 request_budget`. Expected: FAIL.
- [ ] **Step 3: Implement.** Mirror Task 5's gate at the H2 dispatch entry (`:515-553`): acquire before the loop, `Rejected` → `cluster.upstream_rq_5xx().inc()` + `synth_h2_overflow()` + the gated `x-envoy-attempt-count: 1` push + the existing H2 synth-response finalize path (`finalize_h2_stream`). H1/H2 parity per Task 6's discipline.
- [ ] **Step 4: Run, verify pass.** Run: `cargo test -p envoy-http2` (full). Expected: PASS.
- [ ] **Step 5: clippy + fmt + commit.**

```bash
cargo fmt --all && cargo clippy -p envoy-http2 --all-targets --all-features -- -D warnings
git add crates/envoy-http2/src/hcm.rs
git commit -m "phase 17 Task 7: H2 request-budget gate (max_requests mirror) [ADR-0047]"
```

---

### Task 8: Fixture `0025-upstream-circuit-breaker-retry-budget` + Docker-gated wrapper

**Files:**
- Create: `tests/fixtures/0025-upstream-circuit-breaker-retry-budget/{envoy.yaml,envoy-rust.yaml,expectations.yaml,README.md}`
- Create: `tests/differential/tests/upstream_circuit_breaker_budgets.rs`

- [ ] **Step 1: Write the Docker-gated wrapper** (mirror `tests/differential/tests/upstream_circuit_breaker.rs`):

```rust
#[tokio::test]
async fn upstream_circuit_breaker_budgets_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..").join("..")
        .join("tests/fixtures/0025-upstream-circuit-breaker-retry-budget");
    differential::run_fixture(&dir).await.expect("fixture passes");
}
```

The wrapper launches the stateful backend with `--per-path /budget-blocked=503 --retry-script /budget-allowed=fail:1` (mirrors the fixture-0024 wrapper's backend launch).

- [ ] **Step 2: Write the fixture configs** (`envoy.yaml` + `envoy-rust.yaml`, identical modulo nothing — same file content): an H1 listener (HCM `stat_prefix: ingress_http`) + a vhost with **`include_attempt_count_in_response: true`** (L11) + three routes/clusters (all STRICT_DNS single-endpoint, **`dns_lookup_family: V4_ONLY`**):
  1. `/budget-blocked` → cluster `budget_zero`: `circuit_breakers: {thresholds: [{priority: DEFAULT, max_retries: 0, track_remaining: true}]}` + `retry_policy: {retry_on: "5xx", num_retries: 1}`; backend path `--per-path /budget-blocked=503` (always-503, body `service unavailable\n`).
  2. `/budget-allowed` → cluster `budget_default`: `circuit_breakers: {thresholds: [{priority: DEFAULT, track_remaining: true}]}` (NO caps — defaults 3/1024 per L5) + the same `retry_policy`; backend path `--retry-script /budget-allowed=fail:1` (503-then-200).
  3. `/rq-blocked` → cluster `rq_zero`: `circuit_breakers: {thresholds: [{priority: DEFAULT, max_requests: 0}]}`, NO retry_policy, NO track_remaining; backend never contacted (point at the same backend host/port).
- [ ] **Step 3: Write `expectations.yaml`** (`Driver::Http1KeepAlive`, three sequential probes, `settle_ms: 200`):

```yaml
driver:
  kind: http1_keep_alive
  requests:
    - method: GET
      path: /budget-blocked
      host: budget_zero
      expected_status: 503
      expected_body: { kind: byte_exact, body: "service unavailable\n" }     # L6: backend 503 VERBATIM
      require_header_absent: x-envoy-overloaded                              # L6: NOT a local reply
      require_header_value: { name: x-envoy-attempt-count, value: "1" }      # L11
    - method: GET
      path: /budget-allowed
      host: budget_default
      expected_status: 200
      expected_body: { kind: byte_exact, body: "ok\n" }                      # L10: retry proceeded
      require_header_value: { name: x-envoy-attempt-count, value: "2" }
    - method: GET
      path: /rq-blocked
      host: rq_zero
      expected_status: 503
      expected_body: { kind: byte_exact, body: "upstream connect error or disconnect/reset before headers. reset reason: overflow" }  # L2
      require_header_present: x-envoy-overloaded
      require_header_value: { name: x-envoy-attempt-count, value: "1" }      # L11 (M16-3 closure)
  settle_ms: 200
  expected_stats:
    # probe-1 cluster (budget_zero) — L1/L7 counter exclusivity + L4/L8 gauges:
    - { name: cluster.budget_zero.upstream_rq_retry_overflow,                       value: 1 }
    - { name: cluster.budget_zero.upstream_rq_retry,                                value: 0 }
    - { name: cluster.budget_zero.upstream_rq_retry_success,                        value: 0 }
    - { name: cluster.budget_zero.upstream_rq_retry_limit_exceeded,                 value: 0 }
    - { name: cluster.budget_zero.upstream_rq_total,                                value: 1 }
    - { name: cluster.budget_zero.circuit_breakers.default.rq_retry_open,           value: 0 }   # L4: momentary
    - { name: cluster.budget_zero.circuit_breakers.default.remaining_retries,       value: 0 }   # cap 0
    # probe-2 cluster (budget_default) — L10 control + L5 defaults:
    - { name: cluster.budget_default.upstream_rq_retry,                             value: 1 }
    - { name: cluster.budget_default.upstream_rq_retry_success,                     value: 1 }
    - { name: cluster.budget_default.upstream_rq_retry_overflow,                    value: 0 }
    - { name: cluster.budget_default.upstream_rq_total,                             value: 2 }
    - { name: cluster.budget_default.circuit_breakers.default.remaining_retries,    value: 3 }    # L5 default
    - { name: cluster.budget_default.circuit_breakers.default.remaining_rq,         value: 1024 } # L5 default
    - { name: cluster.budget_default.circuit_breakers.default.rq_retry_open,        value: 0 }
    # probe-3 cluster (rq_zero) — L2/L3/L9a:
    - { name: cluster.rq_zero.upstream_rq_pending_overflow,                         value: 1 }
    - { name: cluster.rq_zero.upstream_rq_5xx,                                      value: 1 }    # L3 (ADR-0047)
    - { name: cluster.rq_zero.upstream_rq_total,                                    value: 0 }
    - { name: cluster.rq_zero.upstream_rq_retry_overflow,                           value: 0 }    # L9a exclusivity
    - { name: cluster.rq_zero.circuit_breakers.default.rq_open,                     value: 0 }    # L4: momentary
    # HCM downstream (cumulative over 3 probes):
    - { name: http.ingress_http.downstream_rq_2xx,                                  value: 1 }
    - { name: http.ingress_http.downstream_rq_5xx,                                  value: 2 }
    - { name: http.ingress_http.downstream_rq_total,                                value: 3 }
```

NOT asserted (with a README + expectations comment explaining why): `cluster.rq_zero.upstream_cx_total` (Envoy 1 [pool prefetch] vs envoy-rust 0 — the L3/ADR-0047 known divergence); `cluster.budget_zero.upstream_rq_5xx` (Envoy ticks the verbatim-503 as completing [1]; envoy-rust ticks it too via the phase-16 completing gate [1] — actually bilateral, but left unasserted to keep probe-1's assertion set focused on the L7 exclusivity claims; implementer MAY add it if the first Docker run confirms 1/1); `remaining_cx`/`remaining_pending`/`remaining_cx_pools` + all `circuit_breakers.high.*` + `default.cx_pool_open`/`rq_pending_open` (Envoy-only, unasserted by the named-stat scrape).

- [ ] **Step 4: Write `README.md`** — the fixture's purpose (the three-probe budget coverage), the L3/L4 ADR-0047 re-anchoring (why gauges are asserted at 0; why `upstream_cx_total` is not asserted), AND the standing cyclic retry-script caution copied from fixture 0024's README (the parallel-drive fragility — fixture 0025 reuses the same stateful backend, so the caution applies verbatim).
- [ ] **Step 5: Run** (Docker-gated). Run: `cargo test -p differential --test upstream_circuit_breaker_budgets -- --nocapture` (with Docker available). Expected: PASS bilaterally. If any stat assertion diverges, the REAL ENVOY value is the source of truth (D-3.3) — adjust envoy-rust/the assertion via `superpowers:systematic-debugging`, NOT by loosening the expectation silently.
- [ ] **Step 6: Run the regression suite** (the other 24 fixtures; Docker-gated). Expected: all green (the budget machinery is inert for them — no existing fixture configures the new fields).
- [ ] **Step 7: commit.**

```bash
git add tests/fixtures/0025-upstream-circuit-breaker-retry-budget/ tests/differential/tests/upstream_circuit_breaker_budgets.rs
git commit -m "phase 17 Task 8: fixture 0025-upstream-circuit-breaker-retry-budget + Docker wrapper [ADR-0047]"
```

---

### Task 9: In-process backstop (4 paths incl. the >0-cap concurrency regime)

**Files:**
- Create: `crates/envoy-bin/tests/upstream_circuit_breaker_budgets.rs`

- [ ] **Step 1: Write the backstop** (mirror `crates/envoy-bin/tests/upstream_retry.rs` boot + request + stats-scrape pattern). Four test paths:
  - **(i) budget-blocked retry** (probe-1 equivalent): boot envoy-bin with the `budget_zero` config + an in-process always-503 backend → assert 503 verbatim body + `x-envoy-attempt-count: 1` + `upstream_rq_retry_overflow: 1` + `upstream_rq_retry: 0` + `upstream_rq_total: 1` (scraped from admin `/stats`).
  - **(ii) budget-allowed retry** (probe-2 equivalent): the `budget_default` config + a 503-then-200 backend → 200 + `x-envoy-attempt-count: 2` + `remaining_retries: 3` + `remaining_rq: 1024` post-settle.
  - **(iii) request-breaker overflow** (probe-3 equivalent): the `rq_zero` config → 503 + 81-byte body + `x-envoy-overloaded` + `upstream_rq_pending_overflow: 1` + `upstream_rq_5xx: 1` + zero backend contact (assert the backend's request counter is 0).
  - **(iv) the >0-cap CONCURRENCY regime (the differential fixture deliberately omits this — §0/D7.3(iv); the ONLY place it is asserted):** (iv-a) `max_requests: 1` + a slow in-process backend (e.g. 500 ms hold) + TWO concurrent in-process requests → exactly one 200 + one 503-overflow, `upstream_rq_pending_overflow: 1`, AND `circuit_breakers.default.rq_open` observed at `1` DURING the hold (scrape mid-flight — this is the L4 momentary-gauge non-zero edge) then `0` after; (iv-b) `max_retries: 1` + two concurrent retrying requests against an always-503 backend → exactly one `upstream_rq_retry_overflow` tick (one request wins the single retry slot; the other is budget-blocked).
- [ ] **Step 2: Run, iterate to pass.** Run: `cargo test -p envoy-bin --test upstream_circuit_breaker_budgets`. Expected: PASS (all 4 paths).
- [ ] **Step 3: clippy + fmt + commit.**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets --all-features -- -D warnings
git add crates/envoy-bin/tests/upstream_circuit_breaker_budgets.rs
git commit -m "phase 17 Task 9: in-process budget backstop (4 paths incl. >0-cap concurrency regime)"
```

---

### Task 10: Fuzz seed extension (in place) + BEHAVIOR_CONTRACT rows

**Files:**
- Modify: `crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_circuit_breakers.yaml` (extend IN PLACE — no new file, no `.gitignore`/SUCCESS-array churn)
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md`

- [ ] **Step 1: Extend the fuzz seed.** Add the three new fields to the existing threshold in `cluster_circuit_breakers.yaml`: `max_requests: 0`, `max_retries: 0`, `track_remaining: true`. Run the corpus gate: `cargo test -p envoy-config fuzz_corpus_seeds_parse_or_reject_cleanly`. Expected: PASS (the seed still parses cleanly; corpus stays at 28 — no array/`.gitignore` edit needed since no new file).
- [ ] **Step 2: Short-budget fuzz smoke.** Run: `cargo +nightly fuzz run parse_bootstrap -- -runs=100000 -max_total_time=60` (from `crates/envoy-config`). Expected: no crash.
- [ ] **Step 3: Add the "17 entries (circuit-breaker budgets)" BEHAVIOR_CONTRACT rows** under "Stat-name mapping": `upstream_rq_retry_overflow` (counter, value-exact, unconditional registration, fixture 0025: 1/0/0 per cluster); `circuit_breakers.default.rq_retry_open` + `.rq_open` (gauges, value-exact-at-0 bilaterally [L4 momentary semantic — 1 iff active>0 AND active>=max]; non-zero in-process only); `circuit_breakers.default.remaining_retries` + `.remaining_rq` (gauges, value-exact, registered ONLY when `track_remaining: true` [L8]; fixture 0025: 0/3 + n/a/1024). Plus: **(a)** the L3 paragraph — the `max_requests`-overflow reject ticks `upstream_rq_pending_overflow` AND `upstream_rq_5xx` (the ONLY synth path that ticks `upstream_rq_5xx`; supersedes the phase-16 "synthetic local replies do not tick" sentence narrowly for this path, per ADR-0047) while `upstream_rq_total` stays 0 and `upstream_cx_total` is a known divergence (Envoy prefetch 1 vs envoy-rust 0); **(b)** the §5.4 registration-seam note — the `circuit_breakers.default.*` namespace now has TWO registration sites (pools: `cx_open`; cluster: `rq_open`/`rq_retry_open`/`remaining_*`) + the idempotent-shared `upstream_rq_pending_overflow` handle; **(c)** the L12 Envoy-only enumeration (the names envoy-rust does not emit).
- [ ] **Step 4: Update the `x-envoy-attempt-count` Header allow-list row** — add the L11 finding: the header IS emitted on overflow local replies (value 1) when the vhost flag is set; verified empirically (closes M16-3); fixture 0025 asserts it on all three probes.
- [ ] **Step 5: commit.**

```bash
git add crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_circuit_breakers.yaml docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 17 Task 10: fuzz seed budget fields + BEHAVIOR_CONTRACT budget rows [ADR-0047]"
```

---

### Task 11: State-4 phase-done verification + STATE advance to state-5-next

**Files:**
- Modify: `docs/envoy-rust/phases/17-circuit-breaker-budgets/PROGRESS.md`; `docs/envoy-rust/STATE.md`

- [ ] **Step 1: Run the full §7.5 gate suite** and quote each into PROGRESS (the 05.3→16 evidence discipline): `cargo build --workspace --all-targets`; `cargo clippy --workspace --all-targets --all-features -- -D warnings`; `cargo fmt --all -- --check`; `cargo test --workspace`; `cargo deny check`; the short-budget `parse_bootstrap` fuzz run; the Docker-gated differential suite (**all 25 fixtures `0001`–`0025` green simultaneously**); the `h2spec` ≥95% gate.
- [ ] **Step 2: Run the standalone-crate builds** (`project_isolated_crate_build_blindspot` / SPEC §6.7): `cargo build -p envoy-config`, `-p envoy-cluster`, `-p envoy-http1`, `-p envoy-http2` — quote each in PROGRESS.
- [ ] **Step 3: Quote per-gate evidence** in PROGRESS (CI run URL + HEAD SHA + completion timestamp + per-gate output).
- [ ] **Step 4: Advance STATE.md** to `17` state-4-complete / state-5-next (Next expected skill → `superpowers:requesting-code-review` over the phase-17 commit range). Commit.

```bash
git add docs/envoy-rust/phases/17-circuit-breaker-budgets/PROGRESS.md docs/envoy-rust/STATE.md
git commit -m "phase 17 Task 11: state-4 phase-done verification + STATE advance to state-5-next [ADR-0047]"
```

> **State 5 (code review → REVIEW.md) and State 6 (close-out: §5.3-format commit + flip ROADMAP row 17 in-progress→done + STATE → awaiting next planning) are LATER sessions** per §5.1 one-state-per-session. Phase 17 is a non-split top-level phase → it flips its OWN row alone at state 6. **Named state-5 review focus:** (1) the H1/H2 sibling parity of both gates (Tasks 4-7); (2) the L3 `upstream_rq_5xx`-on-overflow narrow departure (does it leak to other synth paths?); (3) the L4 momentary-gauge semantic (active>0 AND active>=max) edge correctness; (4) the guard-lifetime threading in the retry loop (Task 4's `retry_guard_slot`); (5) the L9b request-guard span (acquired before, released after the loop — no early drop).

---

## Self-review

- **Spec coverage:** D1 → Task 1; D2 → Task 1 (zero new ConfigError variants — locked); D3 → Tasks 2+3; D4 → Tasks 4+6; D5 → Tasks 5+7; D6 → Tasks 2+3 (the remaining_* gauges live inside BudgetState); D7.1 → Task 8; D7.2 → Task 8; D7.3 → Task 9; D7.4 → Task 10 (in-place extension — the §1(d) PLAN-writer call resolved to NO new seed file); D8 → Task 10; state-4 → Task 11. All D1–D8 covered.
- **SPEC §1 fixture deltas (ADR-0047):** probe-1 `rq_retry_open: 1` → asserted at `0` (L4); probe-3 `rq_open: 1` → asserted at `0` (L4); probe-3 `upstream_cx_total: 0` → NOT asserted (L3); probe-3 gains `upstream_rq_5xx: 1` (L3); all probes gain `x-envoy-attempt-count` value assertions (L11).
- **Type consistency:** `BudgetState`/`RetryBudgetGuard`/`RequestBudgetGuard`/`BudgetAcquisition` defined in Task 2/3, consumed identically in Tasks 4/5/6/7/9; `try_acquire_retry()`/`try_acquire_request()` signatures stable; `X_ENVOY_ATTEMPT_COUNT` const reused from phase 16.
- **No placeholders:** every code step shows code or names the exact existing pattern to mirror (Tasks 6/7 mirror Tasks 4/5 — the H2 structure was confirmed analogous at PLAN-time with exact line anchors).
- **Regression guard:** the budget machinery is inert when unconfigured (BudgetAcquisition::Unlimited); fixtures 0020/0023 configure `circuit_breakers` WITHOUT the new fields → defaults 3/1024 never approached by sequential workloads + the new conditional gauges registered but never asserted by those fixtures (named-stat scrape) + `upstream_rq_retry_overflow` unconditional registration follows the proven phase-16 retry-counter posture (fixture 0011 set-diff stayed green). The Docker fixture (Task 8 step 5) is the source-of-truth backstop if any lock-in needs adjustment.
