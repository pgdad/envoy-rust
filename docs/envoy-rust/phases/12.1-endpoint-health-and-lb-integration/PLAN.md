# Phase 12.1 (`12.1-endpoint-health-and-lb-integration`) — PLAN

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development`
> per `feedback_execution_style` auto-memory and per the established 06.x / 07.x / 08.x /
> 09 / 10 / 11 cadence. Tasks 1-7 implement the phase per `SPEC.md`. Steps use `- [ ]`
> checkbox syntax for tracking.

**Goal.** Land the **config + per-endpoint health-state + load-balancer-integration + stats
foundation slice** of active HTTP health checking (parent-12 deliverables D1/D2/D3/D5/D6,
carved into 12.1 by ADR-0036): `envoy-config` parses + validates a cluster `health_checks`
block (HTTP-only, 0-or-1) + `common_lb_config.healthy_panic_threshold`; `envoy-cluster`
gains a per-endpoint `EndpointHealth` state machine and a `Cluster::pick()` that excludes
unhealthy endpoints + honors the panic threshold; `envoy-stats` registers the
`cluster.<name>.membership_healthy` gauge. **No new differential fixture, no probe task** —
regression-equivalence via the 18 existing Docker-gated fixtures staying green proves the
machinery is inert when `health_checks` is unconfigured (the 05.1/07.1 foundation-slice
pattern). The active-HC probe task that *drives* `EndpointHealth`, the fixture `0019`, and
the no-healthy-upstream synth-503 body reconciliation (ADR-0037) all land in **12.2**.

**Architecture.** The config schema (`HealthCheck` / `HttpHealthCheck` / `CommonLbConfig` /
`Percent`) lands in `crates/envoy-config/src/bootstrap.rs`, reusing the existing
`parse_duration` (`bootstrap.rs:2289`, integer-only) + `Int64Range` (`bootstrap.rs:1080`,
half-open `[start, end)`) primitives directly — no duration/range primitive is duplicated.
The validator `validate_health_checks` lands inside the per-cluster loop of
`validate()` (`bootstrap.rs:1543-1617`), producing 6 new `ConfigError` variants (in
`crates/envoy-config/src/lib.rs`). The `EndpointHealth` state machine lands in a new
`crates/envoy-cluster/src/health.rs` module (NOT a new crate — `envoy-health` is a 12.2
deliverable; keeping the STATE in `envoy-cluster` lets `pick()` read it cycle-free per
parent SPEC §5.1). The runtime `Cluster` (`crates/envoy-cluster/src/cluster.rs:32`) gains
two fields — `endpoint_health: Option<Vec<Arc<EndpointHealth>>>` (aligned by index with
`endpoints`; `None` when no `health_checks`) + `panic_threshold: f64` — and `pick()`
(`cluster.rs:129`) gains a health-aware arm that is byte-for-byte the phase-02 round-robin
when `endpoint_health` is `None` (the §5.4 inert-when-unconfigured invariant). The
`membership_healthy` gauge is owned by each endpoint's `EndpointHealth` (shared `Arc<Gauge>`)
and updated inline at each Healthy/Unhealthy flip (one source of truth, NOT polled — the
08.2 inline-CAS pattern).

**Tech Stack.** Zero new top-level Cargo deps. Zero new workspace path-deps (`envoy-cluster`
already depends on `envoy-config` + `envoy-stats`). Zero new crates (`envoy-health` is 12.2).
Primitives used: `std::sync::atomic::{AtomicU8, AtomicU32, Ordering}`, `std::sync::Arc`,
`envoy_stats::Gauge`. No `unsafe` (every crate root keeps `#![forbid(unsafe_code)]`). No H2
framing-path touch (h2spec ≥95% holds vacuously). The `parse_bootstrap` fuzz corpus extends
18 → 19 with one new health-check seed.

---

## 0. Architecture lock-ins

These decisions are settled at PLAN-write; subagents implement them as written and do NOT
re-litigate. Numbered for cross-reference from PROGRESS.

1. **No split, no nest-split.** 12.1 is ~900-1000 LoC (production ~430, tests ~470,
   doc/corpus ~80), comfortably under the `BOOTSTRAP_PROMPT.md` §6.1 ~1500-LoC / ~25-task
   gate — the parent-12 split (ADR-0036) already absorbed the over-gate scope into 12.1 +
   12.2. 7 tasks.

2. **No ADR lands in the 12.1 lifecycle** (SPEC §7). DECISIONS.md ledger head is **ADR-0037**
   at 12.1 start; next available **ADR-0038**. The `EndpointHealth` memory-ordering choice
   (`Relaxed`, lock-in #11) is covered by the existing `cluster.rs` `pick()` `Relaxed`
   precedent — no durable-record ADR. A 12.1 ADR lands ONLY if execution surfaces a genuine
   unforeseen ambiguity (unlikely).

3. **The §6.2 empirical verification is DONE** (parent-12 state-2 commit `4f9ba04`;
   findings in SPEC §2 + STATE.md `### Phase-12 state-2 split decision` + ADR-0037).
   **Do NOT re-run Docker.** The locked facts 12.1 bakes in: initial endpoint state =
   **Unhealthy/pending** (item 1); panic default **50%**, **strictly-below** (`<`),
   `Percent { value: f64 }` (item 3); stat names `cluster.<name>.health_check.{attempt,
   success,failure}` + `membership_healthy` (item 4); default `expected_statuses` = exactly
   200, `Int64Range` half-open `[start, end)` (item 5); **integer-second durations only**
   (item 6 — `parse_duration` rejects `0.5s`).

4. **D6 stats-wiring decision: land the `membership_healthy` gauge at 12.1; DEFER the 3
   `health_check.{attempt,success,failure}` counters to 12.2** (SPEC §6 / §3 D6). Rationale:
   the 3 counters are incremented only inside the 12.2 probe task; registering a
   counter that stays 0-forever with no increment site at 12.1 invites a clippy dead-handle
   /unused-variable friction. The gauge, by contrast, is updated inline by `EndpointHealth`
   transitions whose `record_*` methods are exercised by 12.1 unit tests, so it is live. The
   12.2 PLAN-writer registers the 3 counters at the D4 probe-task site.

5. **Config schema derives match the existing `Cluster` cascade exactly:**
   `#[derive(Debug, Serialize, Deserialize, PartialEq)]` + `#[serde(deny_unknown_fields)]`.
   **NOT** `Clone` (the parent-12 SPEC §D1 sketch showed `#[derive(Debug, Clone, Deserialize,
   PartialEq)]` — that is wrong against HEAD: the on-disk `Cluster` at `bootstrap.rs:55`
   derives `Debug, Serialize, Deserialize, PartialEq` with NO `Clone` and WITH `Serialize`).
   The `Serialize` derive is load-bearing (the 08.1 `Bootstrap` Serialize cascade feeds
   `/config_dump`). `Int64Range` already derives `Clone` and is reused as-is.

6. **`http_health_check` is `Option<HttpHealthCheck>` (`#[serde(default)]`), validator-required.**
   The parent-12/12.1 SPEC §3 D1 sketched it as a required (non-`Option`) field. **PLAN-time
   correction:** if it were required, a missing checker would fail at serde parse
   (`ConfigError::Yaml`/missing-field) and TCP/gRPC checkers would fail at
   `deny_unknown_fields` — so the `UnsupportedHealthCheckType` variant the SPEC §3 D2
   recommends landing would NEVER be constructed → a `dead_code` lint under
   `-D warnings`. Making it `Option` + having the validator reject `None` with
   `UnsupportedHealthCheckType` keeps the variant **live** (constructed by the validator)
   AND honors the SPEC's forward-compat-clarity recommendation. This is the cleanest way to
   land the variant without dead code.

7. **6 new `ConfigError` variants** (SPEC §3 D2 "~4-6"): `UnsupportedMultipleHealthChecks
   { cluster: String }`, `UnsupportedHealthCheckType { cluster: String }`,
   `InvalidHealthCheckThreshold { cluster: String, field: &'static str }`,
   `InvalidHealthCheckTiming { cluster: String, field: &'static str }`,
   `EmptyHealthCheckPath { cluster: String }`, `InvalidPanicThreshold { cluster: String,
   value: f64 }`. Each carries `cluster: String` per the established error-context
   discipline. `expected_statuses` range validity delegates to the existing
   `ConfigError::InvalidInt64Range` (no new variant).

8. **`Percent` is a new envoy-config type** `{ value: f64 }` matching upstream `type.v3.Percent`
   (§6.2 item-3 confirmed `{ value: 0 }` accepted). NOT the phase-11 `FractionalPercent`
   (numerator/denominator — structurally distinct). Re-exported from `lib.rs`.

9. **`EndpointHealth` STATE lives in `envoy-cluster` (new `health.rs` module), NOT a new
   crate.** `envoy-health` is a 12.2 deliverable. No new path-dep; no cycle.

10. **`EndpointHealth` is constructed ONLY when the cluster has a `health_checks` entry**
    (the §5.4 inert-when-unconfigured invariant). A cluster with no `health_checks` carries
    `endpoint_health: None` ⇒ `pick()` is byte-for-byte phase-02 round-robin ⇒ the 18 existing
    fixtures see ZERO behavior change (acceptance gate (b)).

11. **`EndpointHealth` atomics use `Ordering::Relaxed`** (consistent with the `cluster.rs`
    `pick()` cursor; SPEC §5.2 + §7). The 12.2 probe task is single-writer per endpoint
    (one task per (cluster, endpoint)), so `record_success`/`record_failure` never race each
    other for a given endpoint; `pick()` reads `is_healthy()` concurrently — `Relaxed` loads
    suffice (no happens-before dependency, exactly as the cursor).

12. **Initial state = Unhealthy** (§6.2 item-1). A freshly-constructed `EndpointHealth`
    starts Unhealthy with both consecutive counters at 0; the `membership_healthy` gauge
    therefore starts at 0 for a configured-HC cluster (all endpoints unhealthy). The gauge
    `inc()`s on a flip→Healthy, `dec()`s on a flip→Unhealthy.

13. **`pick()` panic semantics (§6.2 item-3, strictly-below):** `healthy_percent =
    100.0 * healthy_count / total`; if `healthy_percent < panic_threshold` → panic →
    round-robin over ALL endpoints. `panic_threshold` default **50.0**; `{ value: 0 }`
    disables panic (`0.0 < 0.0` is false → a 0-healthy cluster returns `None`). The
    non-panic healthy round-robin builds the healthy-index list and round-robins the shared
    cursor over it; an empty healthy set (non-panic) returns `None` (→ the pre-built
    synth-503 path, unchanged at 12.1).

14. **`pick()` does NOT touch the synth-503 writer path** (`hcm.rs:582`/`:918`). 12.1's
    change makes `pick()` → `None` *reachable* for configured-HC clusters; 12.2's task +
    fixture *exercise* it and land the ADR-0037 body reconciliation. 12.1 writes no
    synth-503 code.

15. **The `membership_healthy` gauge is registered in `from_bootstrap`** (when
    `health_checks` is configured) and the resulting `Arc<Gauge>` is cloned into each
    endpoint's `EndpointHealth`. Registration uses the existing `ClusterError::StatsRegistration`
    error-mapping pattern (`cluster.rs:424-450`). Idempotent same-name re-registration is
    safe (the registry contract).

16. **The runtime `Cluster` struct literal sites must ALL gain the two new fields.** Three
    in-crate sites (`cluster.rs:451` production `from_bootstrap`; `:503` `mk_handle` test
    helper; `:782` `cluster_name_returns_configured_name` test) gain `endpoint_health` +
    `panic_threshold` (the inert default `None` / `50.0` for the two test helpers).

17. **The config `Cluster` struct literal sites must ALL gain the two new fields.** Adding
    `health_checks` + `common_lb_config` to `envoy_config::Cluster` breaks every by-hand
    `Cluster { ... }` literal across the workspace (struct literals require all fields even
    `#[serde(default)]` ones). The 4 files carrying such literals at HEAD `4f9ba04`:
    `crates/envoy-cluster/src/cluster.rs` (`:699`, `:731` tests),
    `crates/envoy-config/src/bootstrap.rs` (tests),
    `crates/envoy-http2/src/hcm.rs` (test),
    `crates/envoy-bin/tests/http2_router_upstream.rs` (test). Each adds
    `health_checks: vec![], common_lb_config: None,` (mechanical compile-fix; Task 1).

18. **Subagent-driven execution at state 3** per `feedback_execution_style`: each task below
    is dispatched to a fresh subagent with two-stage review (spec-compliance + code-quality)
    per the 06.x → 11 cadence. The state-2 PLAN-write (this commit) is the controller's
    authoring pass — NOT a subagent dispatch.

19. **No carryforward engaged by 12.1.** The 06.3 REVIEW I2 synthetic-backend down-payment is
    a 12.2 deliverable (12.1 ships no fixture/harness). The inherited carryforward inventory
    (parent SPEC standing list) carries forward UNCHANGED — 12.1 touches no HTTP-filter file.

20. **TDD on every task** per `superpowers:test-driven-development`: write the failing test,
    run it red, implement minimally, run it green, commit. One commit per task per the 06.x →
    11 one-commit-per-task cadence.

---

## 1. PLAN-write SPEC corrections (read against HEAD `4f9ba04`)

Per the 06.2 → 11 "N PLAN-write SPEC corrections" pattern, the PLAN-writer read the 12.1
SPEC §3 surfaces against HEAD and flagged mechanical drift. These corrections land in the
PROGRESS Task 1 preamble and are reflected in the task code below.

1. **Config `Cluster` derives `Debug, Serialize, Deserialize, PartialEq` (NO `Clone`).** The
   parent-12 SPEC §D1 sketch (`#[derive(Debug, Clone, Deserialize, PartialEq)]`) is wrong
   against HEAD — the on-disk `Cluster` (`bootstrap.rs:55`) has `Serialize` (not `Clone`).
   New structs match the on-disk cascade (lock-in #5).

2. **`http_health_check` becomes `Option<HttpHealthCheck>` (validator-required)** to keep
   `UnsupportedHealthCheckType` a live (constructed) variant (lock-in #6) — the SPEC sketched
   it required.

3. **`ConfigError` lives in `crates/envoy-config/src/lib.rs:43`** (the new variants land
   there); `validate()` + the new `validate_health_checks` sub-validator live in
   `bootstrap.rs`. (Same correction as phases 09/10/11.)

4. **`parse_duration` is `pub fn parse_duration(s: &str) -> Result<std::time::Duration,
   String>`** (`bootstrap.rs:2289`) — integer + `s`/`ms`/`us` suffix; rejects `0.5s` (parses
   the numeric part as `u64`). Re-confirmed. The validator parses `timeout`/`interval` via it.

5. **`Int64Range` is `{ start: i64, end: i64 }`** (`bootstrap.rs:1080`), derives
   `Debug, Clone, Serialize, Deserialize, PartialEq` + `deny_unknown_fields`; half-open
   `[start, end)`; validated `start >= end` → `ConfigError::InvalidInt64Range`. Reused as-is
   for `expected_statuses`.

6. **The runtime `Cluster` (`cluster.rs:32`) currently has fields** `name, endpoints, cursor,
   upstream_protocol, cx_total, cx_active, upstream_rq_total, upstream_rq_5xx`; `pick()` at
   `:129` is `fn pick(&self) -> Option<SocketAddr>` (round-robin over `self.endpoints`);
   `ClusterHandle::pick_endpoint() -> Option<SocketAddr>` at `:152` delegates. 12.1 appends
   `endpoint_health` + `panic_threshold`.

7. **`envoy-stats` API:** `StatsRegistry::register_gauge(&str) -> Result<Arc<Gauge>,
   StatsError>` (idempotent same-kind); `Gauge::{inc(), dec(), set(i64), value() -> i64}`.
   Re-confirmed (`registry.rs:69`, `gauge.rs`).

8. **`synth_status` at `hcm.rs:918`** (the empty-body 503) is NOT touched at 12.1 — the
   no-healthy-upstream body reconciliation (ADR-0037) is a 12.2 deliverable (D6.2).

---

## File Structure

- **Modify** `crates/envoy-config/src/bootstrap.rs` — add `HealthCheck` / `HttpHealthCheck` /
  `CommonLbConfig` / `Percent` structs; add `health_checks` + `common_lb_config` fields to
  `Cluster`; add `validate_health_checks` sub-validator + its call in the per-cluster loop;
  add positive/negative parse + validate tests; extend the fuzz corpus SUCCESS array.
- **Modify** `crates/envoy-config/src/lib.rs` — add 6 `ConfigError` variants; re-export the
  4 new schema types.
- **Create** `crates/envoy-cluster/src/health.rs` — the `EndpointHealth` state machine.
- **Modify** `crates/envoy-cluster/src/lib.rs` — `mod health;` + `pub use health::EndpointHealth;`.
- **Modify** `crates/envoy-cluster/src/cluster.rs` — `Cluster` gains `endpoint_health` +
  `panic_threshold`; `pick()` gains the health-aware arm; `from_bootstrap` constructs
  `EndpointHealth` + registers the gauge when `health_checks` configured; update the 3
  in-crate `Cluster { }` literals + the 2 config `Cluster { }` test literals.
- **Modify** `crates/envoy-http2/src/hcm.rs`, `crates/envoy-bin/tests/http2_router_upstream.rs`
  — mechanical config-`Cluster`-literal compile-fix (lock-in #17).
- **Create** `crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_health_check.yaml`.
- **Modify** `crates/envoy-config/fuzz/.gitignore` — allow-list the new seed.
- **Modify** `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — add the `membership_healthy` stat row.

---

## Task 1: envoy-config schema (D1) + config-`Cluster`-literal compile-fix

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (add structs near `Int64Range` ~`:1080`;
  add fields to `Cluster` ~`:55`)
- Modify: `crates/envoy-config/src/lib.rs:10-25` (re-export block)
- Modify (compile-fix): `crates/envoy-cluster/src/cluster.rs` (`:699`, `:731`),
  `crates/envoy-config/src/bootstrap.rs` (test literals),
  `crates/envoy-http2/src/hcm.rs`, `crates/envoy-bin/tests/http2_router_upstream.rs`

- [ ] **Step 1: Write the failing test** (append to `bootstrap.rs` `#[cfg(test)] mod tests`)

```rust
#[test]
fn parses_cluster_with_http_health_check_and_panic_threshold() {
    let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: hc_backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      common_lb_config:
        healthy_panic_threshold: { value: 0 }
      health_checks:
        - timeout: 1s
          interval: 1s
          healthy_threshold: 1
          unhealthy_threshold: 2
          http_health_check:
            path: /healthz
            expected_statuses:
              - { start: 200, end: 201 }
      load_assignment:
        cluster_name: hc_backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: localhost, port_value: 7000 } }
admin:
  address:
    socket_address: { address: 127.0.0.1, port_value: 9901 }
"#;
    let bootstrap = crate::parse_bootstrap(yaml).expect("valid");
    let cluster = &bootstrap.static_resources.clusters[0];
    assert_eq!(cluster.health_checks.len(), 1);
    let hc = &cluster.health_checks[0];
    assert_eq!(hc.timeout, "1s");
    assert_eq!(hc.interval, "1s");
    assert_eq!(hc.healthy_threshold, 1);
    assert_eq!(hc.unhealthy_threshold, 2);
    let http = hc.http_health_check.as_ref().expect("http checker present");
    assert_eq!(http.path, "/healthz");
    assert_eq!(http.expected_statuses, vec![crate::Int64Range { start: 200, end: 201 }]);
    assert!(http.host.is_none());
    let clb = cluster.common_lb_config.as_ref().expect("common_lb_config present");
    assert_eq!(clb.healthy_panic_threshold.as_ref().unwrap().value, 0.0);
}

#[test]
fn cluster_without_health_checks_defaults_to_empty_vec_and_none() {
    let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
admin:
  address:
    socket_address: { address: 127.0.0.1, port_value: 9901 }
"#;
    let bootstrap = crate::parse_bootstrap(yaml).expect("valid");
    let cluster = &bootstrap.static_resources.clusters[0];
    assert!(cluster.health_checks.is_empty());
    assert!(cluster.common_lb_config.is_none());
}

#[test]
fn cluster_rejects_unknown_health_check_field() {
    // deny_unknown_fields rejects TCP/gRPC checkers + deferred upstream knobs.
    let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      health_checks:
        - timeout: 1s
          interval: 1s
          healthy_threshold: 1
          unhealthy_threshold: 1
          tcp_health_check: {}
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
admin:
  address:
    socket_address: { address: 127.0.0.1, port_value: 9901 }
"#;
    assert!(crate::parse_bootstrap(yaml).is_err());
}
```

- [ ] **Step 2: Run it red**

Run: `cargo test -p envoy-config parses_cluster_with_http_health_check_and_panic_threshold cluster_without_health_checks_defaults cluster_rejects_unknown_health_check_field`
Expected: FAIL — `no field health_checks on type Cluster` / `cannot find type HealthCheck`.

- [ ] **Step 3: Add the new schema structs** (place adjacent to `Int64Range`, ~`bootstrap.rs:1080`)

```rust
/// 12.1 (parent-12 D1): per-cluster active HTTP health check. Phase-12 supports
/// exactly 0 or 1 entry per cluster, HTTP-only (the validator rejects >1 and
/// non-HTTP checkers). Reuses `parse_duration` for `timeout`/`interval` and
/// `Int64Range` for `expected_statuses`. The probe task that consumes this lands
/// in 12.2 (the `envoy-health` crate).
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HealthCheck {
    /// Per-probe response timeout; parsed via `parse_duration` (integer s/ms/us).
    pub timeout: String,
    /// Interval between probes; parsed via `parse_duration`.
    pub interval: String,
    /// Consecutive successes to mark an endpoint Healthy.
    pub healthy_threshold: u32,
    /// Consecutive failures to mark an endpoint Unhealthy.
    pub unhealthy_threshold: u32,
    /// The HTTP checker. Optional at the schema level so a config omitting it
    /// (or carrying a deferred TCP/gRPC checker, which `deny_unknown_fields`
    /// rejects) surfaces as `ConfigError::UnsupportedHealthCheckType` at
    /// validate time rather than a bare serde missing-field error. The
    /// validator (Task 2) requires it present.
    #[serde(default)]
    pub http_health_check: Option<HttpHealthCheck>,
}

/// 12.1 (parent-12 D1): the HTTP health-check probe shape.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HttpHealthCheck {
    /// REQUIRED probe path (e.g. `/healthz`); validator rejects empty.
    pub path: String,
    /// OPTIONAL `:authority`/`Host` on the probe; defaults to the cluster name
    /// per upstream (§6.2 item-5). Consumed by the 12.2 probe task.
    #[serde(default)]
    pub host: Option<String>,
    /// OPTIONAL accepted status ranges; default = exactly 200 (§6.2 item-5).
    /// Reuses `Int64Range` (half-open `[start, end)`).
    #[serde(default)]
    pub expected_statuses: Vec<Int64Range>,
}

/// 12.1 (parent-12 D1): the subset of `common_lb_config` phase-12 consumes.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CommonLbConfig {
    /// Default 50% per upstream; `{ value: 0 }` disables panic routing.
    #[serde(default)]
    pub healthy_panic_threshold: Option<Percent>,
}

/// 12.1 (parent-12 D1): upstream `type.v3.Percent { value: double }` (§6.2 item-3).
/// Distinct from the phase-11 `FractionalPercent` (numerator/denominator).
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Percent {
    pub value: f64,
}
```

- [ ] **Step 4: Add the two fields to `Cluster`** (`bootstrap.rs:55`, after
  `typed_extension_protocol_options`)

```rust
    /// 12.1 (parent-12 D1): OPTIONAL active HTTP health checks. Phase-12 supports
    /// exactly 0 or 1, HTTP-only (validator-enforced). Empty ⇒ the cluster's
    /// endpoints are implicitly healthy and `pick()` is phase-02 round-robin
    /// (the §5.4 inert-when-unconfigured invariant).
    #[serde(default)]
    pub health_checks: Vec<HealthCheck>,
    /// 12.1 (parent-12 D1): OPTIONAL common LB config; phase-12 consumes only
    /// `healthy_panic_threshold`.
    #[serde(default)]
    pub common_lb_config: Option<CommonLbConfig>,
```

- [ ] **Step 5: Re-export the 4 new types** (`lib.rs:10-25`, alphabetical within the block) —
  add `CommonLbConfig`, `HealthCheck`, `HttpHealthCheck`, `Percent` to the `pub use bootstrap::{ ... }` list.

- [ ] **Step 6: Compile-fix the by-hand config-`Cluster` literals** (lock-in #17). Add
  `health_checks: vec![],` and `common_lb_config: None,` to each `Cluster { ... }` literal in:
  `crates/envoy-cluster/src/cluster.rs` (`:699`, `:731`),
  `crates/envoy-config/src/bootstrap.rs` (any test literal),
  `crates/envoy-http2/src/hcm.rs`,
  `crates/envoy-bin/tests/http2_router_upstream.rs`.

  Find them with: `grep -rn "typed_extension_protocol_options:" crates/*/src crates/*/tests`
  (each by-hand `Cluster` literal carries that field).

- [ ] **Step 7: Run the new tests green + workspace build**

Run: `cargo test -p envoy-config parses_cluster_with_http_health_check_and_panic_threshold cluster_without_health_checks_defaults cluster_rejects_unknown_health_check_field`
Expected: PASS.
Run: `cargo build --workspace --all-targets`
Expected: clean (all `Cluster` literals fixed).

- [ ] **Step 8: fmt + commit**

Run: `cargo fmt --all -- --check` (Expected: clean)

```bash
git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs \
        crates/envoy-cluster/src/cluster.rs crates/envoy-http2/src/hcm.rs \
        crates/envoy-bin/tests/http2_router_upstream.rs
git commit -m "phase 12.1: task 1 — D1 envoy-config health-check schema (HealthCheck/HttpHealthCheck/CommonLbConfig/Percent)"
```

---

## Task 2: envoy-config validator (D2) + 6 ConfigError variants

**Files:**
- Modify: `crates/envoy-config/src/lib.rs` (append 6 variants to `ConfigError`)
- Modify: `crates/envoy-config/src/bootstrap.rs` (add `validate_health_checks`; call it in
  the per-cluster loop ~`:1543-1617`)

- [ ] **Step 1: Write the failing tests** (append to `bootstrap.rs` tests)

```rust
/// Helper: build a single-cluster bootstrap YAML wrapping a `health_checks:` block.
fn hc_yaml(health_checks_block: &str, common_lb_config_block: &str) -> String {
    format!(
        r#"
static_resources:
  listeners: []
  clusters:
    - name: hc_backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
{common_lb_config_block}
{health_checks_block}
      load_assignment:
        cluster_name: hc_backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: {{ socket_address: {{ address: localhost, port_value: 7000 }} }}
admin:
  address:
    socket_address: {{ address: 127.0.0.1, port_value: 9901 }}
"#
    )
}

const VALID_HC: &str = r#"      health_checks:
        - timeout: 1s
          interval: 1s
          healthy_threshold: 1
          unhealthy_threshold: 1
          http_health_check:
            path: /healthz"#;

#[test]
fn validate_accepts_well_formed_health_check() {
    assert!(crate::parse_bootstrap(&hc_yaml(VALID_HC, "")).is_ok());
}

#[test]
fn validate_rejects_multiple_health_checks() {
    let two = r#"      health_checks:
        - timeout: 1s
          interval: 1s
          healthy_threshold: 1
          unhealthy_threshold: 1
          http_health_check: { path: /healthz }
        - timeout: 1s
          interval: 1s
          healthy_threshold: 1
          unhealthy_threshold: 1
          http_health_check: { path: /healthz2 }"#;
    let err = crate::parse_bootstrap(&hc_yaml(two, "")).unwrap_err();
    assert!(matches!(err, crate::ConfigError::UnsupportedMultipleHealthChecks { ref cluster } if cluster == "hc_backend"), "got {err:?}");
}

#[test]
fn validate_rejects_missing_http_checker() {
    let no_http = r#"      health_checks:
        - timeout: 1s
          interval: 1s
          healthy_threshold: 1
          unhealthy_threshold: 1"#;
    let err = crate::parse_bootstrap(&hc_yaml(no_http, "")).unwrap_err();
    assert!(matches!(err, crate::ConfigError::UnsupportedHealthCheckType { ref cluster } if cluster == "hc_backend"), "got {err:?}");
}

#[test]
fn validate_rejects_zero_threshold() {
    let zero = r#"      health_checks:
        - timeout: 1s
          interval: 1s
          healthy_threshold: 0
          unhealthy_threshold: 1
          http_health_check: { path: /healthz }"#;
    let err = crate::parse_bootstrap(&hc_yaml(zero, "")).unwrap_err();
    assert!(matches!(err, crate::ConfigError::InvalidHealthCheckThreshold { ref cluster, field } if cluster == "hc_backend" && field == "healthy_threshold"), "got {err:?}");
}

#[test]
fn validate_rejects_subsecond_decimal_duration() {
    // §6.2 item-6: parse_duration rejects "0.5s" → surfaces as InvalidHealthCheckTiming.
    let half = r#"      health_checks:
        - timeout: 0.5s
          interval: 1s
          healthy_threshold: 1
          unhealthy_threshold: 1
          http_health_check: { path: /healthz }"#;
    let err = crate::parse_bootstrap(&hc_yaml(half, "")).unwrap_err();
    assert!(matches!(err, crate::ConfigError::InvalidHealthCheckTiming { ref cluster, field } if cluster == "hc_backend" && field == "timeout"), "got {err:?}");
}

#[test]
fn validate_rejects_zero_duration() {
    let zero = r#"      health_checks:
        - timeout: 0s
          interval: 1s
          healthy_threshold: 1
          unhealthy_threshold: 1
          http_health_check: { path: /healthz }"#;
    let err = crate::parse_bootstrap(&hc_yaml(zero, "")).unwrap_err();
    assert!(matches!(err, crate::ConfigError::InvalidHealthCheckTiming { ref cluster, field } if cluster == "hc_backend" && field == "timeout"), "got {err:?}");
}

#[test]
fn validate_rejects_empty_path() {
    let empty = r#"      health_checks:
        - timeout: 1s
          interval: 1s
          healthy_threshold: 1
          unhealthy_threshold: 1
          http_health_check: { path: "" }"#;
    let err = crate::parse_bootstrap(&hc_yaml(empty, "")).unwrap_err();
    assert!(matches!(err, crate::ConfigError::EmptyHealthCheckPath { ref cluster } if cluster == "hc_backend"), "got {err:?}");
}

#[test]
fn validate_rejects_inverted_expected_status_range() {
    let bad = r#"      health_checks:
        - timeout: 1s
          interval: 1s
          healthy_threshold: 1
          unhealthy_threshold: 1
          http_health_check:
            path: /healthz
            expected_statuses:
              - { start: 300, end: 200 }"#;
    let err = crate::parse_bootstrap(&hc_yaml(bad, "")).unwrap_err();
    assert!(matches!(err, crate::ConfigError::InvalidInt64Range { start: 300, end: 200 }), "got {err:?}");
}

#[test]
fn validate_rejects_out_of_range_panic_threshold() {
    let clb = "      common_lb_config:\n        healthy_panic_threshold: { value: 150 }";
    let err = crate::parse_bootstrap(&hc_yaml(VALID_HC, clb)).unwrap_err();
    assert!(matches!(err, crate::ConfigError::InvalidPanicThreshold { ref cluster, value } if cluster == "hc_backend" && value == 150.0), "got {err:?}");
}

#[test]
fn validate_accepts_zero_panic_threshold() {
    let clb = "      common_lb_config:\n        healthy_panic_threshold: { value: 0 }";
    assert!(crate::parse_bootstrap(&hc_yaml(VALID_HC, clb)).is_ok());
}
```

- [ ] **Step 2: Run red**

Run: `cargo test -p envoy-config validate_rejects_multiple_health_checks validate_rejects_missing_http_checker validate_rejects_zero_threshold validate_rejects_subsecond_decimal_duration validate_rejects_empty_path validate_rejects_inverted_expected_status_range validate_rejects_out_of_range_panic_threshold`
Expected: FAIL — `no variant UnsupportedMultipleHealthChecks` etc.

- [ ] **Step 3: Add the 6 `ConfigError` variants** (append to the enum in `lib.rs`, after the
  phase-10/11 RBAC/fault variants)

```rust
    /// 12.1: cluster has more than one `health_checks` entry (phase-12 supports 0 or 1).
    #[error("cluster '{cluster}' has more than one health_checks entry; phase 12 supports at most one")]
    UnsupportedMultipleHealthChecks { cluster: String },

    /// 12.1: cluster's health check has no `http_health_check` (TCP/gRPC/custom defer).
    #[error("cluster '{cluster}' health check is not an http_health_check; phase 12 supports HTTP health checks only")]
    UnsupportedHealthCheckType { cluster: String },

    /// 12.1: `healthy_threshold` or `unhealthy_threshold` is zero (must be >= 1).
    #[error("cluster '{cluster}' health check {field} must be >= 1")]
    InvalidHealthCheckThreshold { cluster: String, field: &'static str },

    /// 12.1: `timeout`/`interval` failed `parse_duration` or parsed to zero.
    /// §6.2 item-6: a sub-second decimal `0.5s` fails `parse_duration` and surfaces here.
    #[error("cluster '{cluster}' health check {field} is not a positive integer-second duration (e.g. `1s`)")]
    InvalidHealthCheckTiming { cluster: String, field: &'static str },

    /// 12.1: `http_health_check.path` is empty.
    #[error("cluster '{cluster}' http_health_check.path must be non-empty")]
    EmptyHealthCheckPath { cluster: String },

    /// 12.1: `common_lb_config.healthy_panic_threshold.value` is outside [0.0, 100.0].
    #[error("cluster '{cluster}' healthy_panic_threshold value {value} is outside [0.0, 100.0]")]
    InvalidPanicThreshold { cluster: String, value: f64 },
```

- [ ] **Step 4: Add `validate_health_checks`** (a free fn in `bootstrap.rs`, near
  `validate_access_logs`)

```rust
/// 12.1 (parent-12 D2): validate a cluster's `health_checks` + `common_lb_config`.
/// Returns the first error encountered (validator-wide convention). HTTP-only,
/// 0-or-1; TCP/gRPC/custom checkers are rejected (the schema's
/// `http_health_check: Option<_>` surfaces a non-HTTP checker as
/// `UnsupportedHealthCheckType`; `deny_unknown_fields` rejects unknown checker keys).
fn validate_health_checks(cluster: &Cluster) -> Result<(), crate::ConfigError> {
    if cluster.health_checks.len() > 1 {
        return Err(crate::ConfigError::UnsupportedMultipleHealthChecks {
            cluster: cluster.name.clone(),
        });
    }
    if let Some(hc) = cluster.health_checks.first() {
        let http = hc.http_health_check.as_ref().ok_or_else(|| {
            crate::ConfigError::UnsupportedHealthCheckType {
                cluster: cluster.name.clone(),
            }
        })?;
        if hc.healthy_threshold < 1 {
            return Err(crate::ConfigError::InvalidHealthCheckThreshold {
                cluster: cluster.name.clone(),
                field: "healthy_threshold",
            });
        }
        if hc.unhealthy_threshold < 1 {
            return Err(crate::ConfigError::InvalidHealthCheckThreshold {
                cluster: cluster.name.clone(),
                field: "unhealthy_threshold",
            });
        }
        for (field, raw) in [("timeout", &hc.timeout), ("interval", &hc.interval)] {
            match parse_duration(raw) {
                Ok(d) if !d.is_zero() => {}
                _ => {
                    return Err(crate::ConfigError::InvalidHealthCheckTiming {
                        cluster: cluster.name.clone(),
                        field,
                    });
                }
            }
        }
        if http.path.is_empty() {
            return Err(crate::ConfigError::EmptyHealthCheckPath {
                cluster: cluster.name.clone(),
            });
        }
        for range in &http.expected_statuses {
            if range.start >= range.end {
                return Err(crate::ConfigError::InvalidInt64Range {
                    start: range.start,
                    end: range.end,
                });
            }
        }
    }
    if let Some(clb) = &cluster.common_lb_config {
        if let Some(p) = &clb.healthy_panic_threshold {
            if !(0.0..=100.0).contains(&p.value) {
                return Err(crate::ConfigError::InvalidPanicThreshold {
                    cluster: cluster.name.clone(),
                    value: p.value,
                });
            }
        }
    }
    Ok(())
}
```

- [ ] **Step 5: Call it in the per-cluster loop** of `validate()` (`bootstrap.rs`, inside
  `for cluster in &bootstrap.static_resources.clusters { ... }`, just before the loop's
  closing brace at ~`:1617`)

```rust
        // 12.1: validate the cluster's active-HC config (HTTP-only, 0-or-1) +
        // common_lb_config panic threshold.
        validate_health_checks(cluster)?;
```

- [ ] **Step 6: Run green + workspace test**

Run: `cargo test -p envoy-config validate_`
Expected: PASS (all 11 validate_* tests).
Run: `cargo test --workspace`
Expected: PASS (no regression in the cross-crate `Cluster`-literal tests fixed in Task 1).

- [ ] **Step 7: fmt + commit**

```bash
git add crates/envoy-config/src/lib.rs crates/envoy-config/src/bootstrap.rs
git commit -m "phase 12.1: task 2 — D2 validate_health_checks + 6 ConfigError variants"
```

---

## Task 3: EndpointHealth state machine (D3) in envoy-cluster

**Files:**
- Create: `crates/envoy-cluster/src/health.rs`
- Modify: `crates/envoy-cluster/src/lib.rs` (`mod health;` + re-export)
- Test: in `health.rs` `#[cfg(test)] mod tests`

- [ ] **Step 1: Write the failing tests** (`health.rs` test module)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn mk(healthy_threshold: u32, unhealthy_threshold: u32) -> (EndpointHealth, Arc<envoy_stats::Gauge>) {
        let reg = envoy_stats::StatsRegistry::new();
        let gauge = reg.register_gauge("cluster.t.membership_healthy").expect("gauge");
        let eh = EndpointHealth::new(healthy_threshold, unhealthy_threshold, Arc::clone(&gauge));
        (eh, gauge)
    }

    #[test]
    fn starts_unhealthy_with_gauge_zero() {
        let (eh, gauge) = mk(1, 1);
        assert!(!eh.is_healthy());
        assert_eq!(gauge.value(), 0);
    }

    #[test]
    fn flips_healthy_after_healthy_threshold_successes() {
        let (eh, gauge) = mk(2, 1);
        eh.record_success();
        assert!(!eh.is_healthy(), "1 < threshold 2");
        assert_eq!(gauge.value(), 0);
        eh.record_success();
        assert!(eh.is_healthy(), "2 == threshold 2");
        assert_eq!(gauge.value(), 1, "gauge inc on flip to healthy");
    }

    #[test]
    fn flips_unhealthy_after_unhealthy_threshold_failures() {
        let (eh, gauge) = mk(1, 2);
        eh.record_success(); // -> Healthy
        assert!(eh.is_healthy());
        assert_eq!(gauge.value(), 1);
        eh.record_failure();
        assert!(eh.is_healthy(), "1 failure < threshold 2");
        assert_eq!(gauge.value(), 1);
        eh.record_failure();
        assert!(!eh.is_healthy(), "2 failures == threshold 2");
        assert_eq!(gauge.value(), 0, "gauge dec on flip to unhealthy");
    }

    #[test]
    fn opposite_result_resets_the_consecutive_counter() {
        let (eh, _gauge) = mk(3, 3);
        eh.record_success();
        eh.record_success(); // 2 successes
        eh.record_failure(); // resets success counter to 0
        eh.record_success();
        eh.record_success(); // only 2 again
        assert!(!eh.is_healthy(), "counter was reset by the intervening failure");
    }

    #[test]
    fn repeated_success_while_healthy_does_not_double_increment_gauge() {
        let (eh, gauge) = mk(1, 1);
        eh.record_success(); // flip -> Healthy, gauge 1
        eh.record_success(); // already healthy; no further inc
        eh.record_success();
        assert_eq!(gauge.value(), 1, "gauge increments only on the edge");
    }
}
```

- [ ] **Step 2: Run red**

Run: `cargo test -p envoy-cluster health::tests`
Expected: FAIL — `cannot find type EndpointHealth` / `module health not found`.

- [ ] **Step 3: Implement `health.rs`**

```rust
//! 12.1 (parent-12 D3): per-endpoint active-health-check state machine.
//!
//! The STATE lives in `envoy-cluster` (not a new crate) so `Cluster::pick()`
//! reads it cycle-free (parent SPEC §5.1). The TASK that *mutates* it via
//! `record_success`/`record_failure` lands in 12.2's `envoy-health` crate.
//! Initial state is Unhealthy (§6.2 item-1): an active-HC endpoint is not
//! healthy until the first `healthy_threshold` consecutive successes.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};

const UNHEALTHY: u8 = 0;
const HEALTHY: u8 = 1;

/// Per-endpoint active-health-check state. Shared (`Arc`) so the 12.2 probe
/// task can mutate it while `pick()` (D5) reads it. Single-writer per endpoint
/// (one probe task per (cluster, endpoint)), so `record_*` never race each
/// other for a given endpoint; `pick()` reads `is_healthy()` concurrently with
/// `Relaxed` loads (no happens-before dependency — the `cluster.rs` `pick()`
/// cursor `Relaxed` precedent).
#[derive(Debug)]
pub struct EndpointHealth {
    state: AtomicU8,
    consecutive_success: AtomicU32,
    consecutive_failure: AtomicU32,
    healthy_threshold: u32,
    unhealthy_threshold: u32,
    /// Shared `cluster.<name>.membership_healthy` gauge; `inc()` on a flip to
    /// Healthy, `dec()` on a flip to Unhealthy (the single source of truth for
    /// the healthy-endpoint count — NOT polled, the 08.2 inline pattern).
    membership_healthy: Arc<envoy_stats::Gauge>,
}

impl EndpointHealth {
    /// Construct an endpoint that starts Unhealthy (gauge contributes 0).
    pub fn new(
        healthy_threshold: u32,
        unhealthy_threshold: u32,
        membership_healthy: Arc<envoy_stats::Gauge>,
    ) -> Self {
        Self {
            state: AtomicU8::new(UNHEALTHY),
            consecutive_success: AtomicU32::new(0),
            consecutive_failure: AtomicU32::new(0),
            healthy_threshold,
            unhealthy_threshold,
            membership_healthy,
        }
    }

    /// Record a probe success. Resets the failure counter; transitions
    /// Unhealthy → Healthy after `healthy_threshold` consecutive successes
    /// (incrementing the membership gauge on that edge).
    pub fn record_success(&self) {
        self.consecutive_failure.store(0, Ordering::Relaxed);
        let n = self.consecutive_success.fetch_add(1, Ordering::Relaxed) + 1;
        if self.state.load(Ordering::Relaxed) == UNHEALTHY && n >= self.healthy_threshold {
            self.state.store(HEALTHY, Ordering::Relaxed);
            self.membership_healthy.inc();
        }
    }

    /// Record a probe failure. Resets the success counter; transitions
    /// Healthy → Unhealthy after `unhealthy_threshold` consecutive failures
    /// (decrementing the membership gauge on that edge).
    pub fn record_failure(&self) {
        self.consecutive_success.store(0, Ordering::Relaxed);
        let n = self.consecutive_failure.fetch_add(1, Ordering::Relaxed) + 1;
        if self.state.load(Ordering::Relaxed) == HEALTHY && n >= self.unhealthy_threshold {
            self.state.store(UNHEALTHY, Ordering::Relaxed);
            self.membership_healthy.dec();
        }
    }

    /// Whether the endpoint is currently Healthy (read by `pick()`).
    pub fn is_healthy(&self) -> bool {
        self.state.load(Ordering::Relaxed) == HEALTHY
    }
}
```

- [ ] **Step 4: Wire the module** (`crates/envoy-cluster/src/lib.rs`)

```rust
mod cluster;
mod health;

pub use cluster::{
    Cluster, ClusterError, ClusterHandle, ClusterManager, UpstreamProtocol, from_bootstrap,
};
pub use health::EndpointHealth;
```

- [ ] **Step 5: Run green**

Run: `cargo test -p envoy-cluster health::tests`
Expected: PASS (5 tests).

- [ ] **Step 6: fmt + commit**

```bash
git add crates/envoy-cluster/src/health.rs crates/envoy-cluster/src/lib.rs
git commit -m "phase 12.1: task 3 — D3 EndpointHealth state machine (initial Unhealthy; threshold transitions; membership gauge edges)"
```

---

## Task 4: pick() unhealthy-exclusion + panic threshold (D5) + from_bootstrap wiring

**Files:**
- Modify: `crates/envoy-cluster/src/cluster.rs` (`Cluster` struct `:32`; `pick()` `:129`;
  `from_bootstrap` `:323-468`; the 2 in-crate `Cluster { }` test literals at `:503`, `:782`)

- [ ] **Step 1: Write the failing tests** (append to the `cluster.rs` `tests` module; add a
  health-aware handle helper)

```rust
    /// 12.1: build a ClusterHandle whose endpoints carry EndpointHealth, all
    /// starting Unhealthy, with the given panic threshold. Returns the handle +
    /// the per-endpoint EndpointHealth Arcs so tests can drive transitions.
    fn mk_handle_with_health(
        name: &str,
        endpoints: Vec<SocketAddr>,
        healthy_threshold: u32,
        unhealthy_threshold: u32,
        panic_threshold: f64,
    ) -> (ClusterHandle, Vec<Arc<crate::EndpointHealth>>) {
        let registry = envoy_stats::StatsRegistry::new();
        let cx_total = registry.register_counter(&format!("cluster.{name}.upstream_cx_total")).unwrap();
        let cx_active = registry.register_gauge(&format!("cluster.{name}.upstream_cx_active")).unwrap();
        let upstream_rq_total = registry.register_counter(&format!("cluster.{name}.upstream_rq_total")).unwrap();
        let upstream_rq_5xx = registry.register_counter(&format!("cluster.{name}.upstream_rq_5xx")).unwrap();
        let gauge = registry.register_gauge(&format!("cluster.{name}.membership_healthy")).unwrap();
        let health: Vec<Arc<crate::EndpointHealth>> = endpoints
            .iter()
            .map(|_| Arc::new(crate::EndpointHealth::new(healthy_threshold, unhealthy_threshold, Arc::clone(&gauge))))
            .collect();
        let handle = ClusterHandle {
            inner: Arc::new(Cluster {
                name: name.to_string(),
                endpoints,
                cursor: AtomicUsize::new(0),
                upstream_protocol: UpstreamProtocol::default(),
                cx_total,
                cx_active,
                upstream_rq_total,
                upstream_rq_5xx,
                endpoint_health: Some(health.clone()),
                panic_threshold,
            }),
        };
        (handle, health)
    }

    #[test]
    fn pick_excludes_unhealthy_endpoints() {
        let eps = mk_endpoints(2);
        // panic disabled (value 0) so a partially-unhealthy set does not panic-route.
        let (handle, health) = mk_handle_with_health("b", eps.clone(), 1, 1, 0.0);
        // Make endpoint 0 healthy, endpoint 1 stays unhealthy.
        health[0].record_success();
        let picks: Vec<SocketAddr> = (0..4).map(|_| handle.pick_endpoint().unwrap()).collect();
        assert!(picks.iter().all(|&p| p == eps[0]), "only the healthy endpoint is picked: {picks:?}");
    }

    #[test]
    fn pick_returns_none_when_no_healthy_and_panic_disabled() {
        let eps = mk_endpoints(2);
        let (handle, _health) = mk_handle_with_health("b", eps, 1, 1, 0.0);
        // All endpoints start Unhealthy; panic disabled → None.
        assert!(handle.pick_endpoint().is_none());
    }

    #[test]
    fn pick_panics_to_all_when_below_threshold() {
        let eps = mk_endpoints(2);
        // default 50% panic threshold; 0 healthy → 0% < 50% → panic → round-robin ALL.
        let (handle, _health) = mk_handle_with_health("b", eps.clone(), 1, 1, 50.0);
        let picks: Vec<SocketAddr> = (0..4).map(|_| handle.pick_endpoint().unwrap()).collect();
        assert_eq!(picks, vec![eps[0], eps[1], eps[0], eps[1]], "panic mode round-robins over all endpoints");
    }

    #[test]
    fn pick_does_not_panic_at_exactly_the_threshold() {
        let eps = mk_endpoints(2);
        // 1 of 2 healthy = 50% ; threshold 50 ; 50 < 50 is false → no panic → only healthy.
        let (handle, health) = mk_handle_with_health("b", eps.clone(), 1, 1, 50.0);
        health[0].record_success();
        let picks: Vec<SocketAddr> = (0..4).map(|_| handle.pick_endpoint().unwrap()).collect();
        assert!(picks.iter().all(|&p| p == eps[0]), "strictly-below: 50% is not < 50% so no panic: {picks:?}");
    }

    #[tokio::test]
    async fn from_bootstrap_no_health_checks_pick_unchanged() {
        // Regression-equivalence: a cluster with no health_checks behaves exactly
        // as phase-02 round-robin (endpoint_health is None).
        let mgr = build_cluster_mgr(THREE_ENDPOINT_YAML).await;
        let handle = mgr.get("backend").expect("cluster");
        let picks: Vec<SocketAddr> = (0..3).map(|_| handle.pick_endpoint().unwrap()).collect();
        assert_eq!(picks, vec![
            "127.0.0.1:10001".parse().unwrap(),
            "127.0.0.1:10002".parse().unwrap(),
            "127.0.0.1:10003".parse().unwrap(),
        ]);
    }

    #[tokio::test]
    async fn from_bootstrap_with_health_checks_starts_all_unhealthy() {
        // A configured-HC cluster (panic disabled) with no probe task → all
        // endpoints start Unhealthy → pick() returns None (the 12.2 task drives
        // them healthy). This is the inert-and-unexercised-seam (§1 SPEC note).
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: hc_backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      common_lb_config:
        healthy_panic_threshold: { value: 0 }
      health_checks:
        - timeout: 1s
          interval: 1s
          healthy_threshold: 1
          unhealthy_threshold: 1
          http_health_check: { path: /healthz }
      load_assignment:
        cluster_name: hc_backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: localhost, port_value: 7000 } }
admin:
  address:
    socket_address: { address: 127.0.0.1, port_value: 9901 }
"#;
        let mgr = build_cluster_mgr(yaml).await;
        let handle = mgr.get("hc_backend").expect("cluster");
        assert!(handle.pick_endpoint().is_none(), "all endpoints start unhealthy + panic disabled");
    }
```

- [ ] **Step 2: Run red**

Run: `cargo test -p envoy-cluster pick_excludes pick_returns_none pick_panics pick_does_not_panic from_bootstrap_no_health from_bootstrap_with_health`
Expected: FAIL — `no field endpoint_health on Cluster`.

- [ ] **Step 3: Add the two `Cluster` fields** (`cluster.rs:32`, after `upstream_rq_5xx`)

```rust
    /// 12.1 (parent-12 D3/D5): per-endpoint active-health-check state, aligned by
    /// index with `endpoints`. `None` when the cluster has no `health_checks`
    /// configured (the §5.4 inert-when-unconfigured invariant) — `pick()` is then
    /// byte-for-byte phase-02 round-robin. `Some` carries one `Arc<EndpointHealth>`
    /// per (resolved) endpoint; the 12.2 probe task mutates them while `pick()`
    /// reads them.
    pub(crate) endpoint_health: Option<Vec<Arc<crate::EndpointHealth>>>,
    /// 12.1 (parent-12 D5): `common_lb_config.healthy_panic_threshold` percentage
    /// (default 50.0). Read by `pick()` only when `endpoint_health` is `Some`.
    pub(crate) panic_threshold: f64,
```

- [ ] **Step 4: Rewrite `pick()`** (`cluster.rs:129`)

```rust
    /// Picks the next endpoint in round-robin order. When the cluster has no
    /// active health checks (`endpoint_health` is `None`) this is exactly the
    /// phase-02 round-robin (the §5.4 inert-when-unconfigured invariant). When
    /// health checks are configured, unhealthy endpoints are excluded and the
    /// panic threshold (§6.2 item-3) is honored. `Relaxed` ordering is
    /// sufficient for the cursor (SPEC §6 signpost 3) and the health reads
    /// (single-writer per endpoint; no happens-before dependency).
    fn pick(&self) -> Option<SocketAddr> {
        if self.endpoints.is_empty() {
            // `from_bootstrap` rejects empty clusters; this is defense-in-depth.
            return None;
        }
        let total = self.endpoints.len();
        let health = match &self.endpoint_health {
            None => {
                let i = self.cursor.fetch_add(1, Ordering::Relaxed);
                return Some(self.endpoints[i % total]);
            }
            Some(h) => h,
        };
        let healthy_count = health.iter().filter(|h| h.is_healthy()).count();
        let healthy_percent = 100.0 * (healthy_count as f64) / (total as f64);
        // Panic threshold (strictly-below): route over ALL endpoints when the
        // healthy fraction is below the threshold. `value: 0` disables panic
        // (`0.0 < 0.0` is false), so a 0-healthy cluster falls through to None.
        if healthy_percent < self.panic_threshold {
            let i = self.cursor.fetch_add(1, Ordering::Relaxed);
            return Some(self.endpoints[i % total]);
        }
        // Round-robin over the healthy endpoints only.
        let healthy_idx: Vec<usize> = (0..total).filter(|&i| health[i].is_healthy()).collect();
        if healthy_idx.is_empty() {
            // No healthy endpoints + panic not engaged → None → the pre-built
            // synth-503 path fires (unchanged at 12.1; body reconciliation is 12.2).
            return None;
        }
        let i = self.cursor.fetch_add(1, Ordering::Relaxed);
        Some(self.endpoints[healthy_idx[i % healthy_idx.len()]])
    }
```

- [ ] **Step 5: Construct `EndpointHealth` + register the gauge in `from_bootstrap`**
  (`cluster.rs`, after the `upstream_rq_5xx` registration ~`:450`, before the
  `Arc::new(Cluster { ... })` at `:451`)

```rust
        // 12.1 (parent-12 D3/D5/D6): if the cluster configures an active health
        // check (validator guarantees 0 or 1), build per-endpoint EndpointHealth
        // (all starting Unhealthy) + register the membership_healthy gauge. No
        // health checks ⇒ endpoint_health: None ⇒ pick() is phase-02 round-robin
        // (§5.4). The probe task that drives these lands in 12.2; at 12.1 they
        // stay at their initial Unhealthy state.
        let (endpoint_health, panic_threshold) = if let Some(hc) = cfg.health_checks.first() {
            let membership_healthy = registry
                .register_gauge(&format!("cluster.{}.membership_healthy", cfg.name))
                .map_err(|e| ClusterError::StatsRegistration {
                    cluster: cfg.name.clone(),
                    message: e.to_string(),
                })?;
            let health: Vec<Arc<crate::EndpointHealth>> = endpoints
                .iter()
                .map(|_| {
                    Arc::new(crate::EndpointHealth::new(
                        hc.healthy_threshold,
                        hc.unhealthy_threshold,
                        Arc::clone(&membership_healthy),
                    ))
                })
                .collect();
            let panic_threshold = cfg
                .common_lb_config
                .as_ref()
                .and_then(|c| c.healthy_panic_threshold.as_ref())
                .map(|p| p.value)
                .unwrap_or(50.0);
            (Some(health), panic_threshold)
        } else {
            (None, 50.0)
        };
```

  Then add the two fields to the `Arc::new(Cluster { ... })` literal at `:451`:

```rust
            endpoint_health,
            panic_threshold,
```

- [ ] **Step 6: Add the two fields to the 2 in-crate test `Cluster { }` literals**
  (`mk_handle` at `:503` and `cluster_name_returns_configured_name` at `:782`): add
  `endpoint_health: None,` and `panic_threshold: 50.0,` (the inert default).

- [ ] **Step 7: Run green + workspace build/test**

Run: `cargo test -p envoy-cluster`
Expected: PASS (new health-aware pick tests + the existing round-robin tests unchanged).
Run: `cargo build --workspace --all-targets && cargo test --workspace`
Expected: clean.

- [ ] **Step 8: clippy (the dead-handle check, lock-in #4) + fmt + commit**

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Expected: clean (the gauge is held + used via `EndpointHealth`; no dead handle).

```bash
git add crates/envoy-cluster/src/cluster.rs
git commit -m "phase 12.1: task 4 — D5 pick() unhealthy-exclusion + panic threshold + from_bootstrap EndpointHealth/gauge wiring"
```

---

## Task 5: membership_healthy gauge contract row (D6)

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (the `## Stat-name mapping` section)
- Test: `crates/envoy-cluster/src/cluster.rs` (a focused gauge-registration assertion)

- [ ] **Step 1: Write the failing test** (append to the `cluster.rs` tests)

```rust
    #[tokio::test]
    async fn from_bootstrap_registers_membership_healthy_gauge_at_zero() {
        // D6: a configured-HC cluster registers cluster.<name>.membership_healthy;
        // it reads 0 at construction (all endpoints start Unhealthy). The 3
        // health_check.{attempt,success,failure} counters defer to 12.2 (lock-in #4).
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: hc_backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      health_checks:
        - timeout: 1s
          interval: 1s
          healthy_threshold: 1
          unhealthy_threshold: 1
          http_health_check: { path: /healthz }
      load_assignment:
        cluster_name: hc_backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: localhost, port_value: 7000 } }
admin:
  address:
    socket_address: { address: 127.0.0.1, port_value: 9901 }
"#;
        let bootstrap = envoy_config::parse_bootstrap(yaml).expect("parse");
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let _mgr = from_bootstrap(&bootstrap, Arc::clone(&registry)).await.expect("build");
        let gauge = registry.register_gauge("cluster.hc_backend.membership_healthy").expect("gauge present");
        assert_eq!(gauge.value(), 0, "all endpoints start Unhealthy");
    }

    #[tokio::test]
    async fn from_bootstrap_no_health_checks_registers_no_membership_gauge() {
        // Inert-when-unconfigured: no membership_healthy gauge for a plain cluster.
        // (snapshot has no such name.)
        let mgr_registry = Arc::new(envoy_stats::StatsRegistry::new());
        let bootstrap = envoy_config::parse_bootstrap(THREE_ENDPOINT_YAML).expect("parse");
        let _mgr = from_bootstrap(&bootstrap, Arc::clone(&mgr_registry)).await.expect("build");
        let has_gauge = mgr_registry
            .snapshot()
            .iter()
            .any(|(name, _)| name == "cluster.backend.membership_healthy");
        assert!(!has_gauge, "no membership gauge when health_checks unconfigured");
    }
```

- [ ] **Step 2: Run red**

Run: `cargo test -p envoy-cluster from_bootstrap_registers_membership from_bootstrap_no_health_checks_registers_no`
Expected: the first PASSES already (Task 4 registered the gauge) — that is acceptable; the
second is the real new assertion and PASSES too. If either FAILS, the gauge wiring from Task
4 is wrong — fix in `cluster.rs`, not here. (This task's substance is the contract row.)

- [ ] **Step 3: Add the BEHAVIOR_CONTRACT row.** In `docs/envoy-rust/BEHAVIOR_CONTRACT.md`,
  in the `## Stat-name mapping` section, append after the last existing entries block:

```markdown
**12.1 entries (active health checking):**

| Stat name | Equivalence | Rationale |
|---|---|---|
| `cluster.<name>.membership_healthy` | value-exact (steady state) | Gauge; the count of currently-healthy endpoints in the cluster. Registered at `from_bootstrap` time only when the cluster configures `health_checks`; updated inline at each `EndpointHealth` Healthy/Unhealthy flip (one source of truth, NOT polled — the 08.2 `server.live` pattern). At 12.1, with no probe task, a configured-HC cluster's gauge reads its initial value 0 (all endpoints start Unhealthy per §6.2 item-1); 12.2's probe task drives it to the converged steady state. Inert when `health_checks` is unconfigured (no such gauge registered). The 3 `cluster.<name>.health_check.{attempt,success,failure}` counters defer to 12.2 where the probe task increments them (12.1 D6 lock-in). |
```

- [ ] **Step 4: Run the tests + commit**

Run: `cargo test -p envoy-cluster from_bootstrap_registers_membership from_bootstrap_no_health_checks_registers_no`
Expected: PASS.

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md crates/envoy-cluster/src/cluster.rs
git commit -m "phase 12.1: task 5 — D6 membership_healthy gauge contract row + registration assertions"
```

---

## Task 6: fuzz corpus seed (D-corpus) — corpus 18 → 19

**Files:**
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_health_check.yaml`
- Modify: `crates/envoy-config/fuzz/.gitignore` (allow-list)
- Modify: `crates/envoy-config/src/bootstrap.rs` (the `fuzz_corpus_seeds_parse_or_reject_cleanly`
  SUCCESS array, ~`:3384`)

> Per the 09/10/11 Task-6 lesson: the new seed file, the `.gitignore` allow-list entry, AND
> the SUCCESS-array extension land in the **same commit** (a gitignored seed the test reads
> would otherwise fail in CI on a clean checkout).

- [ ] **Step 1: Create the seed** (`cluster_health_check.yaml`; integer-second durations per
  §6.2 item-6; exercises D1 schema + D2 validator parse path)

```yaml
static_resources:
  listeners: []
  clusters:
    - name: hc_backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      common_lb_config:
        healthy_panic_threshold: { value: 0 }
      health_checks:
        - timeout: 1s
          interval: 1s
          healthy_threshold: 1
          unhealthy_threshold: 2
          http_health_check:
            path: /healthz
            expected_statuses:
              - { start: 200, end: 201 }
      load_assignment:
        cluster_name: hc_backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: localhost
                      port_value: 7000
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
```

- [ ] **Step 2: Allow-list it** — add to `crates/envoy-config/fuzz/.gitignore` (after the
  `hcm_fault_filter.yaml` line):

```
!corpus/parse_bootstrap/cluster_health_check.yaml
```

- [ ] **Step 3: Extend the SUCCESS array** — in `bootstrap.rs`
  `fuzz_corpus_seeds_parse_or_reject_cleanly` (~`:3402`), append to the success-seed slice
  (after `"fuzz/corpus/parse_bootstrap/hcm_fault_filter.yaml",`):

```rust
            "fuzz/corpus/parse_bootstrap/cluster_health_check.yaml",
```

- [ ] **Step 4: Run the corpus test green**

Run: `cargo test -p envoy-config fuzz_corpus_seeds_parse_or_reject_cleanly`
Expected: PASS (19 success seeds + 3 reject + minimal).

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_health_check.yaml \
        crates/envoy-config/fuzz/.gitignore crates/envoy-config/src/bootstrap.rs
git commit -m "phase 12.1: task 6 — D-corpus parse_bootstrap seed cluster_health_check.yaml (corpus 18->19)"
```

---

## Task 7: state-4 phase-done verification + STATE advance

**Files:**
- Modify: `docs/envoy-rust/phases/12.1-endpoint-health-and-lb-integration/PROGRESS.md`
  (per-task narrative + the §7.5 gate evidence)
- Modify: `docs/envoy-rust/STATE.md` (advance to state-5-next)

> This is the state-4 verification task per `BOOTSTRAP_PROMPT.md` §7.5 + the 05.3 → 11
> evidence-discipline chain. It is docs-only (no code). Run every gate, quote the output
> into PROGRESS, then advance STATE.

- [ ] **Step 1: Run the 5 stable-toolchain gates and capture output**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace
cargo deny check
```
Expected: all clean. Quote the `test result:` line + clippy `Finished` + deny summary into PROGRESS.

- [ ] **Step 2: Run the 18 Docker-gated differential fixtures** (regression-equivalence — the
  load-bearing 12.1 differential proof per acceptance gate (b))

```bash
cargo test -p differential -- --include-ignored
```
Expected: fixtures `0001-tcp-echo` through `0018-http-filter-fault` green simultaneously
(health-check machinery inert when unconfigured). Quote the pass count.

- [ ] **Step 3: Confirm h2spec ≥95% held** (gate (c)). 12.1 touches no H2 framing path, so
  the gate holds vacuously at the parent-05 baseline 99.31%. Re-run if the local h2spec runner
  is available; otherwise rely on CI. Quote.

- [ ] **Step 4: Run the `parse_bootstrap` fuzz target on the 19-seed corpus** (gate (d))

```bash
cd crates/envoy-config && cargo +nightly fuzz run parse_bootstrap -- -runs=200000 ; cd -
```
Expected: clean (no crash). Quote the iteration count. (If nightly/cargo-fuzz unavailable
locally, the `fuzz_corpus_seeds_parse_or_reject_cleanly` unit test + CI cover this; note in
PROGRESS.)

- [ ] **Step 5: Push and confirm CI green.** Push the branch; confirm the single CI run lights
  gates (a)–(e) simultaneously (no new fixture; the 18 stay green). Capture the run ID + HEAD
  SHA + completion timestamp into PROGRESS per the 05.3 → 11 evidence discipline.

```bash
git push
gh run list --branch "$(git branch --show-current)" --limit 1
```

- [ ] **Step 6: Advance STATE.md.** Set Active phase status to `12.1 lifecycle state
  4-complete / state-5-next (implementation verified; REVIEW.md pending)`; set Next expected
  skill to `superpowers:requesting-code-review` (state 5). Append a `### Phase-12.1 state-3
  execution arc` Notes subsection summarizing Tasks 1-7 + the gate evidence. Preserve all
  prior subsections verbatim.

- [ ] **Step 7: Commit the state-4 verification**

```bash
git add docs/envoy-rust/phases/12.1-endpoint-health-and-lb-integration/PROGRESS.md \
        docs/envoy-rust/STATE.md
git commit -m "phase 12.1: task 7 — state-4 phase-done verification + STATE advance to state-5-next"
```

---

## Self-Review (PLAN-writer's checklist, run against the 12.1 SPEC)

**1. Spec coverage.** D1 schema → Task 1. D2 validator + ConfigError variants → Task 2. D3
`EndpointHealth` → Task 3. D5 `pick()` + panic threshold + `from_bootstrap` → Task 4. D6
`membership_healthy` gauge + BEHAVIOR_CONTRACT row → Tasks 4 (registration) + 5 (contract +
assertion). D-corpus fuzz seed → Task 6. State-4 verification (18 fixtures + 5 gates + h2spec
+ fuzz on the 19-seed corpus) → Task 7. The §5.4 inert-when-unconfigured invariant is proven
by `from_bootstrap_no_health_checks_*` tests (Tasks 4-5). All SPEC §3 deliverables mapped.

**2. Placeholder scan.** Every code step shows complete code; every command shows expected
output. No "TBD"/"add validation"/"similar to Task N".

**3. Type consistency.** `EndpointHealth::new(healthy_threshold, unhealthy_threshold,
membership_healthy: Arc<Gauge>)`, `record_success`, `record_failure`, `is_healthy` — used
identically in Tasks 3 and 4. `Cluster.endpoint_health: Option<Vec<Arc<EndpointHealth>>>` +
`panic_threshold: f64` — defined in Task 4 step 3, constructed in Task 4 step 5, defaulted in
the test literals (Task 4 step 6). The 6 `ConfigError` variants (Task 2 step 3) match the
validator's constructions (Task 2 step 4) and the test assertions (Task 2 step 1). `Percent {
value: f64 }`, `HealthCheck.http_health_check: Option<HttpHealthCheck>` consistent across
Tasks 1-2. `register_gauge` returns `Arc<Gauge>`; `Gauge::{inc,dec,value}` used consistently.

---

*End of 12.1 PLAN. 7 tasks; ~900-1000 LoC; no split; no ADR. The seam (config + EndpointHealth
+ LB-integration + stats) lands here; the probe task that drives it + the differential fixture
land in 12.2.*
