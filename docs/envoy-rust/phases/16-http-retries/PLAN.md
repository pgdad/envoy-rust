# Phase 16 (`16-http-retries`) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (the project default per `feedback_execution_style`; implementers dispatched SERIALLY per `feedback_serial_subagent_dispatch`) to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax. Run `cargo clippy --workspace --all-targets --all-features -- -D warnings` PER TASK (NOT deferred to state-4) per `project_state3_arc_skips_clippy`.

**Goal:** Make the HTTP router arm (H1 + H2) retry a failed upstream attempt when the route configures a `retry_policy` (`retry_on` + `num_retries` + `retriable_status_codes`), re-picking a health/ejection-aware endpoint and replaying the request, emitting the 3-stat retry observability subset + the (opt-in-gated) `x-envoy-attempt-count` response header, proven bilaterally by fixture `0024-upstream-retry-on-5xx`.

**Architecture:** A retry loop wraps the EXISTING H1 (`serve_connection` dispatch seam) and H2 (`handle_one_stream` dispatch seam) router arms. No new crate, no new top-level Cargo dep, no new request-body buffering (H1 sends an empty body / H2 replays a buffered `Bytes` clone — body-replay is already safe, ADR-0044 §0). Retry classification lives on a parsed `RetryConfig` in `envoy-config` (shared by both protocols); a tiny attempt-counter is loop-local per protocol. The 3 retry counters live on `Cluster` next to the existing `upstream_rq_*`. Per the §6.2 empirical reconciliation (ADR-0045): `cluster.<name>.upstream_rq_total` moves to per-ATTEMPT counting; `upstream_rq_5xx` stays keyed on the COMPLETING (downstream-returned) response — both inert (identical to today) when no `retry_policy` is configured, preserving the 23 existing fixtures.

**Tech Stack:** Rust (workspace crates `envoy-config`, `envoy-cluster`, `envoy-http1`, `envoy-http2`); `serde`/`serde_yaml` config; `envoy-stats` `Counter`; `tokio::time::sleep` for back-off; `testcontainers` differential harness; `cargo fuzz` (`parse_bootstrap`).

---

## §6.2 empirical lock-ins (verified against `envoyproxy/envoy:v1.33.0`, 2026-05-31; landed as ADR-0045)

The state-2 PLAN-write ran the HEAVY SPEC §6.2 verification in Docker (fail-once-then-succeed + always-503 backends; `%RESPONSE_FLAGS%` access log; admin `/stats`; cross-checked against real backend request counts). Findings — **the three marked ✦ DIVERGE materially from the SPEC projection and are reconciled by ADR-0045:**

- **L1 — `retry_on` token sets.** `5xx` = any 500–599 (confirmed: a 500 retried under `5xx`). `gateway-error` = 502/503/504 ONLY (confirmed: a 500 did NOT retry under `gateway-error`). `connect-failure` = connection-level failure; `reset` = upstream reset before response headers (per v1.33 docs; not differentially driven). `retriable-status-codes` = the additive `retriable_status_codes` list.
- **L2 ✦ — UNKNOWN-token posture = ACCEPT-AND-IGNORE (NOT reject).** Empirically: `retry_on: "5xx,bogus-token-xyz"` booted `state: LIVE` with no error/warning; the valid sibling token still applied. **Lock-in:** `validate_retry_policy` parses the comma list, silently drops unrecognized tokens, applies the recognized ones. NO `ConfigError` variant for unknown tokens. (The SPEC §3 D2 left this open; this resolves it Envoy-faithfully.)
- **L3 — default `num_retries` = 1** (confirmed: `num_retries` omitted → backend saw 2 attempts on the always-503 path). Proto: `retry_on` = `string`; `num_retries` = `google.protobuf.UInt32Value` (wrapper → omitted means router default 1, modeled as `Option<u32>` defaulting to 1 at resolution); `retriable_status_codes` = `repeated uint32`.
- **L4 — retry stat values.** Success path (503→200): `upstream_rq_retry: 1`, `upstream_rq_retry_success: 1`, `upstream_rq_retry_limit_exceeded: 0`. Limit-exceeded path (503→503): `1 / 0 / 1`. Matches projection exactly.
- **L5 ✦ — per-attempt vs per-request counting (the §6.2-CRITICAL item).** `cluster.<name>.upstream_rq_total` counts per upstream ATTEMPT: a single downstream request that made 2 attempts → `upstream_rq_total: 2`. The NO-retry control cluster → `upstream_rq_total: 1`. The retried-away (intermediate) attempt's response code goes to a separate `cluster.<name>.retry.upstream_rq_{503,5xx,completed}` sub-scope (Envoy-only; allow-listed), while the MAIN `upstream_rq_5xx`/`upstream_rq_<code>` reflect the COMPLETING (downstream-returned) attempt's class only. **Reconciliation (ADR-0045):** envoy-rust moves `upstream_rq_total.inc()` to per-attempt (inside the loop); keeps `upstream_rq_5xx.inc()` on the final/completing response only. envoy-rust does NOT replicate the `retry.*` sub-scope (allow-listed). HCM downstream per-class counters (`http.<prefix>.downstream_rq_{2,5}xx`) stay strictly per-downstream-request (already correct — fired on the final response). **Regression safety:** for a 1-attempt request (no retry) per-attempt == per-request, so fixtures 0020 (`upstream_rq_total: 10`, `upstream_rq_5xx: 3`) / 0022 stay byte-identical.
- **L6 ✦ — `x-envoy-attempt-count` is GATED, not automatic.** Empirically the header was ABSENT on every response (and not injected request-side) with a plain `retry_policy`. It requires the VirtualHost flag `include_attempt_count_in_response: true`. **Reconciliation (ADR-0045):** envoy-rust adds `include_attempt_count_in_response: bool` to its `VirtualHost` schema, emits `x-envoy-attempt-count: <total attempts>` on the downstream response ONLY when that flag is true. Fixture 0024 sets the flag true on both proxies' configs. (Request-side `x-envoy-attempt-count` / `include_request_attempt_count` is DEFERRED — not needed for the fixture.)
- **L7 — default back-off** = exponential, base interval 25 ms, max 250 ms (per v1.33 docs; `upstream_rq_retry_backoff_exponential` incremented per retry). Timing is NOT differentially asserted (BEHAVIOR_CONTRACT "Timing tolerances": no opt-in).
- **L8 — per-attempt outlier recording.** Both attempts were recorded against the host (`/clusters`: `rq_total::2`, `rq_error::2`). envoy-rust calls `record_response` per attempt (already the plan).
- **L9 — limit-exceeded wire shape.** Final response = the LAST upstream 503 verbatim (status 503, body `fail\n` content-length 5, `x-envoy-upstream-service-time` present) — NOT a synthetic local reply. `%RESPONSE_FLAGS%` = `URX`; `URX` is access-log-only and is NOT a response header (confirmed). One access-log record per downstream request. Matches projection → no body reconciliation needed.
- **L10 — full Envoy-side retry stat set (for `allowlist_envoy_only`):** `upstream_rq_retry_overflow`, `upstream_rq_retry_backoff_exponential`, `upstream_rq_retry_backoff_ratelimited`, `retry_or_shadow_abandoned`, `circuit_breakers.{default,high}.rq_retry_open`, and the `cluster.<name>.retry.upstream_rq_{503,5xx,completed}` sub-scope (appears only when a retry fires).
- **L11 — harness IPv4 trap (carry-forward).** On macOS Docker, `host.docker.internal` STRICT_DNS resolves to an unreachable IPv6 unless `dns_lookup_family: V4_ONLY` is set (else silent `URX,UF` upstream failure). Fixture 0024 MUST set `dns_lookup_family: V4_ONLY` on its cluster (the phase-05.4 ADR-0024 posture; all existing upstream fixtures already do).

## PLAN-time SPEC corrections (verified against HEAD `0fa80aba9`)

All SPEC §3 code anchors confirmed accurate EXCEPT:
- **`ConfigError` is in `crates/envoy-config/src/lib.rs` (~line 44+), NOT `bootstrap.rs`.** Recent variant style: `lib.rs:445` `UnsupportedMultipleCircuitBreakerThresholds`, `:452` `UnsupportedCircuitBreakerPriority`, `:469` `UnsupportedNonZeroMaxPendingRequests`.
- **Deep-clone sites (CRITICAL — easy to miss):** `crates/envoy-http1/src/hcm.rs:240` `clone_route_action` (currently clones only `cluster` at `:249-250`) MUST also clone the new `retry_policy`; `crates/envoy-http1/src/hcm.rs:211` `clone_route_config` (clones `VirtualHost` fields at `:220`) MUST also clone `include_attempt_count_in_response`. If these are not updated, the new fields are silently dropped when the HCMConfig clones the `Arc<RouteConfiguration>`.
- **`Driver` enum is in `tests/differential/src/lib.rs`** (not `backend.rs`); `Http1ProbeList` exists. The stateful backend knob lands in `tests/differential/src/backend.rs` `spawn_with_per_path` (~`:278-329`, `--per-path` parsing at `:293-294`) AND in the spawned helper binary it shells to.
- **Fuzz corpus is at 27 seeds** (SPEC said 22 — drift). SUCCESS array `crates/envoy-config/src/bootstrap.rs:3726-3749`, reject array `:3756-3760`, test `fuzz_corpus_seeds_parse_or_reject_cleanly` `:3723-3776`; `.gitignore` allow-list `crates/envoy-config/fuzz/.gitignore:1-28`. Adding a seed → 27→28 + edit BOTH the `.gitignore` allow-list AND the SUCCESS array atomically.
- **Schema anchors confirmed:** `RouteConfiguration` `bootstrap.rs:909`; `VirtualHost` `:916`; `Route` `:923`; `RouteAction` enum `:939`; `RouteAction_Route` `:953-955`; `RouteMatch` `:1080`; `parse_duration` `:2467-2494`; `CircuitBreakers` `:1172`/`Thresholds` `:1186`; `validate_circuit_breakers` `:2591+`; `validate_outlier_detection` `:2638+`.
- **Route-dispatch seam (H1):** `serve_connection` `crates/envoy-http1/src/hcm.rs:273`; virtual-host walk `:945-985`; the proxy arm `match &route.action { RouteAction::Route(ar) => BuildOutcome::Proxy { cluster: ar.cluster.clone(), .. } }` at `:983-986`; the dispatch body `:465-706`; counters `crates/envoy-http1/src/router.rs:95-98`; `x-envoy-upstream-service-time` injection `router.rs:54,151-155`; `record_response` `hcm.rs:706`; connect-failure synth-502 `:666-688`; empty-body dispatch `:488`.
- **Route-dispatch seam (H2):** `handle_one_stream` `crates/envoy-http2/src/hcm.rs:132`; dispatch `:238-481`; counters `:473-476`; `record_response` `:481`; `x-envoy-upstream-service-time` `:513-516`; body clone `:290`; protocol fork H1-upstream `:325-336` / H2-upstream `:338-438`.
- **Cluster:** struct stat fields `crates/envoy-cluster/src/cluster.rs:106-111`; stat registration in `from_bootstrap` `:700-711`; `record_response` `:281-321`; `pick` `:212-256`.

## §6.1 split-gate decision

Re-estimated against the §6.2-refined surface (which ADDS the `include_attempt_count_in_response` VirtualHost field + gated emit, and SIMPLIFIES the unknown-token path to accept-and-ignore): **~1450–1650 LoC / ~13 implementation tasks** — at the boundary but NOT over the `BOOTSTRAP_PROMPT.md` §6.1 ~1500-LoC / ~25-task gate. **Single un-split phase.** The work is tightly coupled (the per-attempt counting reconciliation spans both protocols and must keep fixtures green; one fixture; shared `RetryConfig`), so a 16.1/16.2 H1/H2 split would fragment the reconciliation for little benefit. **ADR-0046 (the reserved split ADR) does NOT fire.** (If a single task's sub-steps blow up past ~10 items mid-execution, §6.1 permits a mid-execution split.)

---

## File structure

- `crates/envoy-config/src/bootstrap.rs` — add `RetryPolicy` struct + `RouteAction_Route.retry_policy` field + `VirtualHost.include_attempt_count_in_response` field; the `RetryConfig`/`RetryOn` resolved-types + `is_retriable`; `validate_retry_policy`; fuzz SUCCESS-array edit.
- `crates/envoy-config/src/lib.rs` — (only if a semantic `ConfigError` variant proves needed; per L2 likely none).
- `crates/envoy-cluster/src/cluster.rs` — 3 retry `Counter` fields + accessors + registration in `from_bootstrap`.
- `crates/envoy-http1/src/hcm.rs` — H1 retry loop; thread `retry_policy` + vhost flag through `BuildOutcome::Proxy`; update `clone_route_action`/`clone_route_config`; per-attempt `upstream_rq_total`; `x-envoy-attempt-count` emit; back-off.
- `crates/envoy-http1/src/router.rs` — relocate `upstream_rq_total` to per-attempt; keep `upstream_rq_5xx` on completing response; `x-envoy-attempt-count` header injection (reuse the `x-envoy-upstream-service-time` machinery).
- `crates/envoy-http2/src/hcm.rs` — H2 retry loop mirror; per-attempt counting; `x-envoy-attempt-count` emit.
- `tests/differential/src/backend.rs` (+ the helper binary it spawns) — stateful fail-then-succeed knob.
- `tests/fixtures/0024-upstream-retry-on-5xx/` — `envoy.yaml`, `envoy-rust.yaml`, `inputs/`, `expectations.yaml`, `README.md`.
- `tests/differential/tests/upstream_retry.rs` — Docker-gated wrapper.
- `crates/envoy-bin/tests/upstream_retry.rs` — in-process backstop (both paths).
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/route_retry_policy.yaml` + `crates/envoy-config/fuzz/.gitignore` — fuzz seed (27→28).
- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — retry stat rows + `x-envoy-attempt-count` header row + per-attempt-counting clarification.

---

### Task 1: `envoy-config` schema — `RetryPolicy` + `retry_policy` field + `include_attempt_count_in_response`

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (`RouteAction_Route` `:953-955`; `VirtualHost` `:916`; add `RetryPolicy` struct nearby)
- Test: `crates/envoy-config/src/bootstrap.rs` (`#[cfg(test)]` module — mirror the existing `circuit_breakers`/`outlier_detection` serde tests)

- [ ] **Step 1: Write failing serde tests.** Add tests: (a) a route YAML with `retry_policy: { retry_on: "5xx", num_retries: 1 }` round-trips into `RouteAction_Route { cluster, retry_policy: Some(RetryPolicy { retry_on: "5xx", num_retries: Some(1), retriable_status_codes: [] }) }`; (b) a route with NO `retry_policy` → `retry_policy: None`; (c) `retry_policy` with a deferred field `per_try_timeout: 1s` → parse ERROR (serde `deny_unknown_fields`); (d) a VirtualHost YAML with `include_attempt_count_in_response: true` → field `true`; absent → `false`.
- [ ] **Step 2: Run tests, verify they fail** (`RetryPolicy` undefined / field missing). Run: `cargo test -p envoy-config retry_policy -- --nocapture`. Expected: FAIL (compile error / unknown field).
- [ ] **Step 3: Add the schema.** Insert near `RouteAction_Route`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RetryPolicy {
    #[serde(default)]
    pub retry_on: String, // comma-separated condition tokens
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub num_retries: Option<u32>, // Envoy default 1 (resolved at RetryConfig::from)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retriable_status_codes: Vec<u32>,
}
```

Extend `RouteAction_Route`:

```rust
pub struct RouteAction_Route {
    pub cluster: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_policy: Option<RetryPolicy>,
}
```

Extend `VirtualHost` with `#[serde(default)] pub include_attempt_count_in_response: bool,` (add it to the struct at `:916`; `#[serde(default)]` makes it default-`false`).

- [ ] **Step 4: Run tests, verify pass.** Run: `cargo test -p envoy-config retry_policy`. Expected: PASS.
- [ ] **Step 5: clippy + fmt + commit.**

```bash
cargo fmt --all && cargo clippy -p envoy-config --all-targets --all-features -- -D warnings
git add crates/envoy-config/src/bootstrap.rs
git commit -m "phase 16 Task 1: RetryPolicy schema + retry_policy field + include_attempt_count_in_response [ADR-0045]"
```

---

### Task 2: `envoy-config` — resolved `RetryConfig` + `retry_on` tokenization (accept-and-ignore) + `validate_retry_policy`

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (add `RetryOn`, `RetryConfig`, `RetryConfig::from(&RetryPolicy)`, `is_retriable`, `validate_retry_policy`)
- Test: `crates/envoy-config/src/bootstrap.rs` test module

- [ ] **Step 1: Write failing tests** for the resolved type and classifier:

```rust
#[test]
fn retry_on_parses_known_tokens_and_ignores_unknown() {
    // L2: accept-and-ignore unknown tokens (Envoy-faithful, empirically verified)
    let p = RetryPolicy { retry_on: "5xx,bogus-token-xyz".into(), num_retries: None, retriable_status_codes: vec![] };
    let rc = RetryConfig::from(&p);
    assert_eq!(rc.num_retries, 1); // L3: default 1
    assert!(rc.is_retriable(503, AttemptOutcome::Response));   // 5xx matches
    assert!(rc.is_retriable(500, AttemptOutcome::Response));   // 5xx = 500-599 (L1)
    assert!(!rc.is_retriable(404, AttemptOutcome::Response));  // not retriable
}

#[test]
fn gateway_error_is_502_503_504_only() {
    let p = RetryPolicy { retry_on: "gateway-error".into(), num_retries: Some(2), retriable_status_codes: vec![] };
    let rc = RetryConfig::from(&p);
    assert_eq!(rc.num_retries, 2);
    assert!(rc.is_retriable(503, AttemptOutcome::Response));
    assert!(!rc.is_retriable(500, AttemptOutcome::Response)); // L1: 500 NOT in gateway-error
}

#[test]
fn connect_failure_and_reset_and_retriable_status_codes() {
    let p = RetryPolicy { retry_on: "connect-failure,reset,retriable-status-codes".into(), num_retries: Some(1), retriable_status_codes: vec![409] };
    let rc = RetryConfig::from(&p);
    assert!(rc.is_retriable(0, AttemptOutcome::ConnectFailure));
    assert!(rc.is_retriable(0, AttemptOutcome::Reset));
    assert!(rc.is_retriable(409, AttemptOutcome::Response)); // retriable_status_codes
    assert!(!rc.is_retriable(503, AttemptOutcome::Response)); // 5xx token NOT present
}
```

- [ ] **Step 2: Run, verify fail.** Run: `cargo test -p envoy-config retry_on -- --nocapture` and `... gateway_error ... connect_failure`. Expected: FAIL (types undefined).
- [ ] **Step 3: Implement.** Add (near `RetryPolicy`):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptOutcome { Response, ConnectFailure, Reset }

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RetryOn {
    pub on_5xx: bool,
    pub on_gateway_error: bool,
    pub on_connect_failure: bool,
    pub on_reset: bool,
    pub on_retriable_status_codes: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryConfig {
    pub on: RetryOn,
    pub num_retries: u32,
    pub retriable_status_codes: Vec<u32>,
}

impl RetryConfig {
    pub fn from(p: &RetryPolicy) -> Self {
        let mut on = RetryOn::default();
        for tok in p.retry_on.split(',').map(str::trim).filter(|t| !t.is_empty()) {
            match tok { // L2: unrecognized tokens silently ignored
                "5xx" => on.on_5xx = true,
                "gateway-error" => on.on_gateway_error = true,
                "connect-failure" => on.on_connect_failure = true,
                "reset" => on.on_reset = true,
                "retriable-status-codes" => on.on_retriable_status_codes = true,
                _ => {}
            }
        }
        RetryConfig { on, num_retries: p.num_retries.unwrap_or(1), retriable_status_codes: p.retriable_status_codes.clone() }
    }

    pub fn is_retriable(&self, status: u16, outcome: AttemptOutcome) -> bool {
        match outcome {
            AttemptOutcome::ConnectFailure => self.on.on_connect_failure,
            AttemptOutcome::Reset => self.on.on_reset,
            AttemptOutcome::Response => {
                (self.on.on_5xx && (500..=599).contains(&status))
                    || (self.on.on_gateway_error && matches!(status, 502 | 503 | 504))
                    || (self.on.on_retriable_status_codes
                        && self.retriable_status_codes.contains(&(status as u32)))
            }
        }
    }
}

/// Validator hook (mirrors validate_circuit_breakers / validate_outlier_detection).
/// L2: retry_on tokens are accept-and-ignore, so this is currently infallible;
/// it exists so future semantic rejections have a home and so the validator
/// surface is symmetric with the other route/cluster validators.
fn validate_retry_policy(_route: &Route) -> Result<(), crate::ConfigError> { Ok(()) }
```

Wire `validate_retry_policy` into the existing route-validation walk (find where routes/clusters are validated — search for the `validate_circuit_breakers` call site and add a sibling per-route call). Deferred `retry_policy` fields are already rejected by `deny_unknown_fields` at parse (Task 1 test (c)).

- [ ] **Step 4: Run, verify pass.** Run: `cargo test -p envoy-config retry`. Expected: PASS.
- [ ] **Step 5: clippy + fmt + commit.**

```bash
cargo fmt --all && cargo clippy -p envoy-config --all-targets --all-features -- -D warnings
git add crates/envoy-config/src/bootstrap.rs
git commit -m "phase 16 Task 2: RetryConfig + retry_on tokenization (accept-and-ignore) + validate_retry_policy [ADR-0045]"
```

---

### Task 3: `envoy-cluster` — 3 retry counters (`upstream_rq_retry{,_success,_limit_exceeded}`)

**Files:**
- Modify: `crates/envoy-cluster/src/cluster.rs` (struct fields `:106-111`; registration `:700-711`; add accessors)
- Test: `crates/envoy-cluster/src/cluster.rs` test module

- [ ] **Step 1: Write failing test.** A `Cluster::from_bootstrap` over a minimal cluster registers `cluster.<name>.upstream_rq_retry`, `cluster.<name>.upstream_rq_retry_success`, `cluster.<name>.upstream_rq_retry_limit_exceeded` in the `StatsRegistry`, each readable at `0`, and the accessors `cluster.upstream_rq_retry()` etc. return the handles. (Mirror the existing `upstream_rq_total`/`upstream_rq_5xx` registration test.)
- [ ] **Step 2: Run, verify fail.** Run: `cargo test -p envoy-cluster upstream_rq_retry`. Expected: FAIL (accessor/field missing).
- [ ] **Step 3: Implement.** Add 3 `pub(crate) upstream_rq_retry{,_success,_limit_exceeded}: Arc<envoy_stats::Counter>` fields next to `upstream_rq_total`/`upstream_rq_5xx` (`:106-111`); register them in `from_bootstrap` next to the existing two (`:700-711`) via `registry.register_counter(&format!("cluster.{}.upstream_rq_retry", cfg.name))` etc.; add `pub fn upstream_rq_retry(&self) -> &Arc<Counter>` accessors (mirror the existing accessor style). **Unconditional registration at 0** (cluster-construct time; a route's retry config is not known here — registering for every cluster keeps the names present and inert per L5; the 23 existing fixtures do not assert these names so they stay green).
- [ ] **Step 4: Run, verify pass.** Run: `cargo test -p envoy-cluster upstream_rq_retry`. Expected: PASS.
- [ ] **Step 5: clippy + fmt + commit.**

```bash
cargo fmt --all && cargo clippy -p envoy-cluster --all-targets --all-features -- -D warnings
git add crates/envoy-cluster/src/cluster.rs
git commit -m "phase 16 Task 3: cluster upstream_rq_retry{,_success,_limit_exceeded} counters (inert-at-0)"
```

---

### Task 4: H1 retry loop + per-attempt `upstream_rq_total` reconciliation + `x-envoy-attempt-count` + back-off

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` (`clone_route_action` `:240`; `clone_route_config` `:220`; `BuildOutcome::Proxy` `:985`; dispatch seam `:465-706`; connect-failure arm `:666-688`)
- Modify: `crates/envoy-http1/src/router.rs` (counters `:95-98`; `x-envoy-attempt-count` injection near `:151-155`)
- Test: `crates/envoy-http1/src/hcm.rs` test module (in-process H1)

- [ ] **Step 1: Write failing test.** An in-process H1 HCM test with a stateful backend (use the existing in-crate test harness; a backend returning 503 then 200) + a route whose `retry_policy = Some({retry_on:"5xx", num_retries:1})` + vhost `include_attempt_count_in_response: true`: assert the downstream response is `200`, carries `x-envoy-attempt-count: 2`, the cluster's `upstream_rq_retry`==1, `upstream_rq_retry_success`==1, `upstream_rq_total`==2, `upstream_rq_5xx`==0. A second test: always-503 backend → downstream `503`, `x-envoy-attempt-count: 2`, `upstream_rq_retry`==1, `upstream_rq_retry_limit_exceeded`==1, `upstream_rq_total`==2, `upstream_rq_5xx`==1. A third (regression): NO `retry_policy` → 1 attempt, `upstream_rq_total`==1, no `x-envoy-attempt-count` header.
- [ ] **Step 2: Run, verify fail.** Run: `cargo test -p envoy-http1 retry`. Expected: FAIL.
- [ ] **Step 3: Implement.**
  - Update `clone_route_action` (`:249-250`) to also clone `retry_policy: ar.retry_policy.clone()`; update `clone_route_config` (`:220`) to clone `include_attempt_count_in_response: vh.include_attempt_count_in_response`.
  - Extend `BuildOutcome::Proxy` to carry the resolved `Option<RetryConfig>` (from `ar.retry_policy.as_ref().map(RetryConfig::from)`) and the `include_attempt_count_in_response: bool` (from the matched vhost). Thread both to the dispatch seam.
  - Wrap the dispatch (`:465-706`) in a loop: `let rc = retry_config; let max = rc.as_ref().map_or(0, |r| r.num_retries); let mut attempts = 0u32;` loop { attempts += 1; pick_endpoint → acquire → send → receive (or connect-failure); `cluster.upstream_rq_total().inc();` (PER ATTEMPT — L5); `cluster.record_response(endpoint, status);` (per attempt — L8); classify outcome (`AttemptOutcome::Response`/`ConnectFailure`); `let retriable = rc.as_ref().map_or(false, |r| r.is_retriable(status, outcome));` if retriable && attempts <= max { `cluster.upstream_rq_retry().inc();` back-off (Step: `if let Some(d) = backoff(attempts) { tokio::time::sleep(d).await }`); continue } else break with this response }.
  - After loop: `if status/100==5 { cluster.upstream_rq_5xx().inc(); }` (COMPLETING response only — L5; do NOT inc inside the loop). If `attempts > 1` (a retry happened): on success `cluster.upstream_rq_retry_success().inc()`; on still-retriable-but-exhausted `cluster.upstream_rq_retry_limit_exceeded().inc()`.
  - **Move the existing `router.rs:95-98` `upstream_rq_total`/`upstream_rq_5xx` increments OUT of `construct_proxied_response`** — `upstream_rq_total` now fires per-attempt in the hcm loop; `upstream_rq_5xx` fires once post-loop on the completing status. (Keep `construct_proxied_response` building the response; just remove its counter side-effects so the loop owns them — single-source-of-truth.)
  - Connect-failure arm (`:666-688`): instead of immediately returning synth-502, classify as `AttemptOutcome::ConnectFailure`; if `retriable`, loop; else surface synth-502 as today.
  - `x-envoy-attempt-count` (router.rs near `:151-155`, reusing the `x-envoy-upstream-service-time` push pattern): when `include_attempt_count_in_response`, push `("x-envoy-attempt-count".to_string(), attempts.to_string())` onto the response headers. Define `pub const X_ENVOY_ATTEMPT_COUNT: &str = "x-envoy-attempt-count";`.
  - Back-off helper: `fn backoff(attempt: u32) -> Option<Duration>` returning exponential base 25 ms (e.g. `25ms << (attempt-1)` capped at 250 ms) — L7. Place in `envoy-http1` (or duplicate in H2; trivial).
- [ ] **Step 4: Run, verify pass.** Run: `cargo test -p envoy-http1 retry`. Expected: PASS. Also run `cargo test -p envoy-http1` (full) to confirm no regression in existing HCM tests.
- [ ] **Step 5: clippy + fmt + commit.** (Watch for `collapsible_if`/`match` lints in the classification — L `project_state3_arc_skips_clippy`.)

```bash
cargo fmt --all && cargo clippy -p envoy-http1 --all-targets --all-features -- -D warnings
git add crates/envoy-http1/src/hcm.rs crates/envoy-http1/src/router.rs
git commit -m "phase 16 Task 4: H1 retry loop + per-attempt upstream_rq_total + x-envoy-attempt-count [ADR-0045]"
```

---

### Task 5: H2 retry loop mirror + per-attempt counting + `x-envoy-attempt-count`

**Files:**
- Modify: `crates/envoy-http2/src/hcm.rs` (`handle_one_stream` `:132`; dispatch `:238-481`; counters `:473-476`; `record_response` `:481`; `x-envoy-upstream-service-time` `:513-516`; route/vhost clone sites — find the H2 analogues of `clone_route_action`/`clone_route_config`, or the HCMConfig construction path)
- Test: `crates/envoy-http2/src/hcm.rs` test module (in-process H2)

- [ ] **Step 1: Write failing test.** Mirror Task 4's three tests on the H2 arm (success retry→200 + `x-envoy-attempt-count: 2` + counters; always-503 → limit-exceeded; no-retry regression → `upstream_rq_total`==1, no header). Use the existing H2 in-crate test harness + a stateful in-test backend.
- [ ] **Step 2: Run, verify fail.** Run: `cargo test -p envoy-http2 retry`. Expected: FAIL.
- [ ] **Step 3: Implement.** Mirror Task 4 on `handle_one_stream`: thread the resolved `Option<RetryConfig>` + `include_attempt_count_in_response` from the matched route/vhost (update the H2 route/vhost clone path if it has one; if H2 reads `RouteConfiguration` directly without a clone helper, thread from the walk site); wrap the `:238-481` dispatch (covering BOTH the H1-upstream `:325-336` and H2-upstream `:338-438` forks — the loop is protocol-agnostic) in the same loop; move `upstream_rq_total` (`:473`) to per-attempt and keep `upstream_rq_5xx` (`:474-476`) on the completing response; `record_response` (`:481`) per attempt; push `x-envoy-attempt-count` near `:513-516` when the flag is set; reuse the back-off helper. Share `RetryConfig`/`AttemptOutcome` from `envoy-config` (no duplication of the classifier).
- [ ] **Step 4: Run, verify pass.** Run: `cargo test -p envoy-http2 retry` and full `cargo test -p envoy-http2`. Expected: PASS.
- [ ] **Step 5: clippy + fmt + commit.**

```bash
cargo fmt --all && cargo clippy -p envoy-http2 --all-targets --all-features -- -D warnings
git add crates/envoy-http2/src/hcm.rs
git commit -m "phase 16 Task 5: H2 retry loop + per-attempt counting + x-envoy-attempt-count [ADR-0045]"
```

---

### Task 6: Stateful fail-then-succeed synthetic-backend harness primitive

**Files:**
- Modify: `tests/differential/src/backend.rs` (`spawn_with_per_path` `:278-329`) + the spawned helper binary (find it via the `--per-path` consumer; likely `tests/helpers/` or a `bin` in the differential crate)
- Test: an in-crate harness test exercising the new knob (or covered by Task 10's backstop)

- [ ] **Step 1: Write failing test.** A harness test: spawn the backend with `--retry-script /retry-success=fail:1`; issue 2 sequential GETs to `/retry-success`; assert the FIRST returns 503 and the SECOND returns 200; issue GET `/retry-exhausted` (configured `--per-path /retry-exhausted=503`) and assert 503.
- [ ] **Step 2: Run, verify fail.** Run: `cargo test -p envoy-differential backend_retry_script` (adjust crate name to the differential crate). Expected: FAIL (flag unknown).
- [ ] **Step 3: Implement.** Add a `--retry-script PATH=fail:N` CLI arg to the helper binary: a per-path `AtomicU64` request counter; for a path in the retry-script map, return 503 for the first `N` requests then 200 (body `fail\n` / `ok\n`). Keep the existing stateless `--per-path PATH=STATUS` working (always-503 path). Add `spawn_with_retry_script(...)` (or extend `spawn_with_per_path` with an optional retry-script param) in `backend.rs`.
- [ ] **Step 4: Run, verify pass.** Expected: PASS.
- [ ] **Step 5: clippy + fmt + commit.**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets --all-features -- -D warnings
git add tests/differential/src/backend.rs tests/helpers/  # adjust to actual helper path
git commit -m "phase 16 Task 6: stateful fail-then-succeed synthetic-backend harness knob"
```

---

### Task 7: Fixture `0024-upstream-retry-on-5xx` + Docker-gated wrapper

**Files:**
- Create: `tests/fixtures/0024-upstream-retry-on-5xx/{envoy.yaml,envoy-rust.yaml,expectations.yaml,README.md}` and `inputs/` (probe list)
- Create: `tests/differential/tests/upstream_retry.rs`

- [ ] **Step 1: Write the Docker-gated wrapper test** (mirror `tests/differential/tests/` shape from 13.1/14.2/15, e.g. the circuit-breaker test): a `#[tokio::test]` gated on the Docker availability cfg used by the suite, loading fixture `0024`, driving `Driver::Http1ProbeList` with the two paths, diffing under `expectations.yaml`.
- [ ] **Step 2: Write the fixture configs.** `envoy.yaml` + `envoy-rust.yaml` (identical): an H1 listener (HCM, `stat_prefix: ingress_http`) + a STRICT_DNS cluster `backend` with **`dns_lookup_family: V4_ONLY`** (L11) to the stateful backend; a virtual host with **`include_attempt_count_in_response: true`** (L6) and two routes: `match {prefix: "/retry-success"} → {cluster: backend, retry_policy: {retry_on:"5xx", num_retries:1}}` and `match {prefix: "/retry-exhausted"} → {cluster: backend, retry_policy: {retry_on:"5xx", num_retries:1}}`. Backend launched (in the wrapper) with `--retry-script /retry-success=fail:1 --per-path /retry-exhausted=503`.
- [ ] **Step 3: Write `expectations.yaml`.** Sequential probes via `Http1ProbeList`: GET `/retry-success` → status 200, header `x-envoy-attempt-count: 2` (value-exact); GET `/retry-exhausted` → status 503, body `fail\n`, header `x-envoy-attempt-count: 2`. `expected_stats` (cumulative over both probes on cluster `backend`): `upstream_rq_retry: 2`, `upstream_rq_retry_success: 1`, `upstream_rq_retry_limit_exceeded: 1`, `upstream_rq_total: 4`, `upstream_rq_5xx: 1`; HCM `http.ingress_http.downstream_rq_2xx: 1`, `downstream_rq_5xx: 1`, `downstream_rq_total: 2`. `allowlist_envoy_only:` the L10 names (`cluster.backend.upstream_rq_retry_overflow`, `..._backoff_exponential`, `..._backoff_ratelimited`, `cluster.backend.retry_or_shadow_abandoned`, `cluster.backend.circuit_breakers.default.rq_retry_open`, `cluster.backend.circuit_breakers.high.rq_retry_open`, `cluster.backend.retry.upstream_rq_503`, `cluster.backend.retry.upstream_rq_5xx`, `cluster.backend.retry.upstream_rq_completed`). Allow-list `server`/`date`/`x-envoy-upstream-service-time` per the standing header discipline.
- [ ] **Step 4: Run** (Docker-gated). Run: `cargo test -p envoy-differential --test upstream_retry -- --ignored` (or the suite's Docker-gate invocation). Expected: PASS (both proxies agree). If the differential reveals `upstream_rq_total`/`upstream_rq_5xx` values differ from L5's model, treat it as a `superpowers:systematic-debugging` item — the differential (real Envoy) is the source of truth; reconcile the increment sites and/or the asserted values.
- [ ] **Step 5: commit.**

```bash
git add tests/fixtures/0024-upstream-retry-on-5xx/ tests/differential/tests/upstream_retry.rs
git commit -m "phase 16 Task 7: fixture 0024-upstream-retry-on-5xx + Docker-gated wrapper"
```

---

### Task 8: In-process backstop (both paths)

**Files:**
- Create: `crates/envoy-bin/tests/upstream_retry.rs`

- [ ] **Step 1: Write the backstop test** (mirror `crates/envoy-bin/tests/` shape from 14.2/15). Boot `envoy-bin` with a synthesized bootstrap (route `retry_policy: {retry_on:"5xx", num_retries:1}`, vhost `include_attempt_count_in_response: true`) + an in-process stateful backend (503-then-200 on `/retry-success`; always-503 on `/retry-exhausted`). Assert BOTH paths: success → 200 + `x-envoy-attempt-count: 2` + scrape `/stats` for `cluster.<name>.upstream_rq_retry`/`_success`/`upstream_rq_total: 2`/`upstream_rq_5xx: 0`; limit-exceeded → 503 + `x-envoy-attempt-count: 2` + `upstream_rq_retry`/`_limit_exceeded`/`upstream_rq_total: 2`/`upstream_rq_5xx: 1`. (Timing-robust; no cross-proxy fragility — this is the deterministic guard for the per-attempt reconciliation L5.)
- [ ] **Step 2: Run, verify fail** (before the feature is wired end-to-end through envoy-bin, if any wiring is missing). Run: `cargo test -p envoy-bin --test upstream_retry`. Then iterate to **Step 3: PASS.**
- [ ] **Step 4: clippy + fmt + commit.**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets --all-features -- -D warnings
git add crates/envoy-bin/tests/upstream_retry.rs
git commit -m "phase 16 Task 8: in-process retry backstop (success + limit-exceeded paths)"
```

---

### Task 9: Fuzz corpus seed (27→28) + atomic `.gitignore`/SUCCESS-array edit

**Files:**
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/route_retry_policy.yaml`
- Modify: `crates/envoy-config/fuzz/.gitignore` (`:1-28` allow-list); `crates/envoy-config/src/bootstrap.rs` (`fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS array `:3726-3749`)

- [ ] **Step 1: Write the seed** — a minimal valid bootstrap whose route carries `retry_policy: {retry_on: "5xx,gateway-error,connect-failure,reset,retriable-status-codes", num_retries: 2, retriable_status_codes: [409, 429]}` and a vhost `include_attempt_count_in_response: true`.
- [ ] **Step 2: Add it to BOTH** the `.gitignore` allow-list (`!corpus/parse_bootstrap/route_retry_policy.yaml`) AND the `fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS array (the 09/10/11/12.2/13.1/15 atomic-edit lesson — they must stay in sync).
- [ ] **Step 3: Run the corpus gate.** Run: `cargo test -p envoy-config fuzz_corpus_seeds_parse_or_reject_cleanly`. Expected: PASS (the new seed parses cleanly; count 27→28).
- [ ] **Step 4: Short-budget fuzz smoke** (matches the §7.5(d) gate). Run: `cargo +nightly fuzz run parse_bootstrap -- -runs=100000 -max_total_time=60` (from `crates/envoy-config`). Expected: no crash.
- [ ] **Step 5: commit.**

```bash
git add crates/envoy-config/fuzz/corpus/parse_bootstrap/route_retry_policy.yaml crates/envoy-config/fuzz/.gitignore crates/envoy-config/src/bootstrap.rs
git commit -m "phase 16 Task 9: parse_bootstrap fuzz seed route_retry_policy.yaml (corpus 27->28)"
```

---

### Task 10: BEHAVIOR_CONTRACT extensions

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md`

- [ ] **Step 1: Add the "16 entries (HTTP retries)" Stat-name rows** under "Stat-name mapping": `cluster.<name>.upstream_rq_retry` / `_success` / `_limit_exceeded` (value-exact; fixture 0024: 2/1/1) with the inert-when-unconfigured rationale; PLUS a clarification paragraph on the per-attempt `upstream_rq_total` (per upstream attempt) vs completing-only `upstream_rq_5xx` (L5), noting the Envoy-only `cluster.<name>.retry.*` sub-scope is allow-listed.
- [ ] **Step 2: Add the `x-envoy-attempt-count` row** under "Header allow-list": value-exact (total upstream attempts); emitted on the downstream response ONLY when the route's VirtualHost sets `include_attempt_count_in_response: true` (L6); reuses the `x-envoy-upstream-service-time` injection machinery.
- [ ] **Step 3: Add a note** under "Response body" (or a brief row) recording the limit-exceeded wire shape (L9): final = last upstream response verbatim (NOT a synth); `URX` is access-log-only, not a response header.
- [ ] **Step 4: commit.**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 16 Task 10: BEHAVIOR_CONTRACT retry stat + x-envoy-attempt-count rows [ADR-0045]"
```

---

### Task 11: State-4 phase-done verification + STATE advance to state-5-next

**Files:**
- Modify: `docs/envoy-rust/phases/16-http-retries/PROGRESS.md`; `docs/envoy-rust/STATE.md`

- [ ] **Step 1: Run the full §7.5 gate suite** and quote each into PROGRESS (the 05.3→15 evidence discipline): `cargo build --workspace --all-targets`; `cargo clippy --workspace --all-targets --all-features -- -D warnings`; `cargo fmt --all -- --check`; `cargo test --workspace`; `cargo deny check`; the short-budget `parse_bootstrap` fuzz run; the Docker-gated differential suite (all 24 fixtures `0001`–`0024` green simultaneously); the `h2spec` ≥95% gate.
- [ ] **Step 2: Run the standalone-crate builds** (L `project_isolated_crate_build_blindspot`, SPEC §6.7): `cargo build -p envoy-config`, `-p envoy-cluster`, `-p envoy-http1`, `-p envoy-http2` — quote each in PROGRESS.
- [ ] **Step 3: Quote per-gate evidence** in PROGRESS (CI run URL + HEAD SHA + completion timestamp + per-gate output).
- [ ] **Step 4: Advance STATE.md** to `16` state-4-complete / state-5-next (Next expected skill → `superpowers:requesting-code-review`). Commit.

```bash
git add docs/envoy-rust/phases/16-http-retries/PROGRESS.md docs/envoy-rust/STATE.md
git commit -m "phase 16 Task 11: state-4 phase-done verification + STATE advance to state-5-next"
```

> **State 5 (code review → REVIEW.md) and State 6 (close-out: commit + flip ROADMAP row 16 in-progress→done + STATE → awaiting next planning) are LATER sessions** per §5.1 one-state-per-session. Phase 16 is a non-split top-level phase → it flips its OWN row alone at state 6.

---

## Self-review

- **Spec coverage:** D1 → Task 1 (+ `include_attempt_count_in_response` per L6); D2 → Task 2; D3 → Task 4; D4 → Task 5; D5 (3 stats + per-attempt reconciliation) → Tasks 3+4+5; D6 → Task 6; D7 (back-off) → Task 4 step 3 (helper, reused in Task 5); D8.1 → Task 7; D8.2 → Task 7; D8.3 → Task 8; D8.4 → Task 9; D9 → Task 10; state-4 → Task 11. All D1–D9 covered.
- **Type consistency:** `RetryPolicy` (schema) → `RetryConfig`/`RetryOn`/`AttemptOutcome` (resolved) used identically in Tasks 2/4/5; `is_retriable(status: u16, outcome: AttemptOutcome)` signature stable across H1/H2; counter accessors `upstream_rq_retry()`/`_success()`/`_limit_exceeded()` consistent Task 3→4→5→8; `X_ENVOY_ATTEMPT_COUNT` const Task 4→5.
- **No placeholders:** every code step shows the code; the one "find the H2 route/vhost clone path" in Task 5 is a locate-then-mirror instruction with the exact H1 pattern to copy (Task 4 step 3) — acceptable because the H2 structure was confirmed analogous at PLAN-time.
- **Regression guard:** L5 reconciliation is inert for 1-attempt requests → fixtures 0020/0022 byte-exact (verified no fixture asserts the new names). The differential (Task 7 step 4) is the source-of-truth backstop if the L5 counting model needs adjustment.
