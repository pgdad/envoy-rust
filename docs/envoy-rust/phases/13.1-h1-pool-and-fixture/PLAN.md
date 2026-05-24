# Phase 13.1 (`13.1-h1-pool-and-fixture`) — Implementation PLAN

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (per `feedback_execution_style`) to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. The state-3 controller dispatches one fresh subagent per task with two-stage review per the 12.1 / 12.2 / phase-11 state-3 cadence.

**Goal:** Land the H1 connection-pool primitive + envoy-config `circuit_breakers` schema + H1 router-arm pool integration + configurable-status synthetic backend + fixture 0020 (the named full-closure site for **06.3 REVIEW I2 (a)**) + `Driver::Http1KeepAlive` harness extension + in-process H1 backstop + `parse_bootstrap` corpus seed.

**Architecture:** A new `H1Pool` primitive at `crates/envoy-http1/src/pool.rs` (per-cluster, per-endpoint idle keep-alive list; `PoolGuard` RAII owning a `ConnGaugeGuard`; idle sweeper as the second periodic-background primitive). The cycle-resolution adopts **external `H1PoolManager` injection** (NOT a field on `envoy-cluster::Cluster`): the bin constructs an `Arc<H1PoolManager>` after `from_bootstrap`, then plumbs it into the HCM configuration alongside the existing `Arc<ClusterManager>`. The H1 HCM proxy arm at `crates/envoy-http1/src/hcm.rs:514` migrates from per-call `Client::connect()` to `pool_mgr.get(cluster_name).acquire(endpoint, host).await`; the `cluster.<name>.upstream_cx_total` increment-site migrates with it, into the pool's `acquire()` connect-on-miss branch (one source of truth). No new top-level Cargo dep; no new trait; no `unsafe`.

**Tech Stack:** Rust 2024 + tokio + tokio-util + bytes + envoy-stats + envoy-http1 internal types. Hand-rolled per D-3.2 (`per-protocol connection pooling ... Must be written from scratch`).

---

## File Structure

**New files (production):**
- `crates/envoy-http1/src/pool.rs` — `H1Pool` + `PoolGuard` + `H1PoolManager` + `PoolError` + idle sweeper. Module declared in `crates/envoy-http1/src/lib.rs`. ~280 LoC + ~250 LoC tests.

**Modified files (production):**
- `crates/envoy-http1/src/lib.rs` — `pub mod pool;` + re-exports.
- `crates/envoy-config/src/bootstrap.rs` — `Cluster` gets `circuit_breakers: Option<CircuitBreakers>` field; new `CircuitBreakers` / `Thresholds` / `RoutingPriority` types. ~80 LoC + ~120 LoC tests.
- `crates/envoy-config/src/lib.rs` — `validate_circuit_breakers` sub-validator + 3 new `ConfigError` variants. ~70 LoC + ~120 LoC tests.
- `crates/envoy-http1/src/hcm.rs` — proxy-arm dispatch at `:508`-`:527` migrates from `Client::connect` + `cluster.cx_total().inc()` into `pool_mgr.get(cluster_name).acquire(endpoint, host)`. The `tier-1 cached_upstream` micro-cache becomes dead code and is removed (the pool subsumes it). HCM constructor gains an `Arc<H1PoolManager>` field. ~60 LoC modify + ~140 LoC tests.
- `crates/envoy-bin/src/main.rs` — after the existing `cluster_mgr` construction (`:124`) and BEFORE the existing health-scheduler spawn (`:134`), build `Arc<H1PoolManager>` over the bootstrap's H1 clusters; thread it into HCM-listener serve sites. ~30 LoC + ~30 LoC.
- `tests/helpers/health-aware-http1-backend/src/main.rs` — additive `--per-path PATH=STATUS[,PATH=STATUS,...]` flag; per-path response shaping; deterministic 3xx/4xx/5xx body bytes. ~50 LoC additive + ~50 LoC tests.
- `tests/differential/src/lib.rs` — `Driver::Http1KeepAlive { requests }` variant + dispatch arm implementing single-downstream-conn N-sequential-requests over keep-alive H1. ~50 LoC + ~80 LoC tests.
- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — 2 new rows under cluster-upstream-connection namespace: `cluster.<name>.upstream_cx_destroy` + `cluster.<name>.upstream_cx_http1_total`. NO tightening of the existing `upstream_cx_total` row (defers to 13.2). ~20 LoC.
- `crates/envoy-config/fuzz/.gitignore` — one new allow-list line for `cluster_circuit_breakers.yaml`. 1 LoC.
- `crates/envoy-config/src/bootstrap.rs::tests::fuzz_corpus_seeds_parse_or_reject_cleanly` — extend SUCCESS array with the new seed (20 → 21 entries). 1 LoC.

**New files (tests + fixtures):**
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_circuit_breakers.yaml` — new corpus seed. ~25 LoC YAML.
- `tests/fixtures/0020-upstream-connection-pooling-and-per-class-counters/envoy.yaml` — reference Envoy config. ~80 LoC.
- `tests/fixtures/0020-upstream-connection-pooling-and-per-class-counters/envoy-rust.yaml` — identical to `envoy.yaml`. ~80 LoC.
- `tests/fixtures/0020-upstream-connection-pooling-and-per-class-counters/expectations.yaml` — bilateral assertion grammar; `Driver::Http1KeepAlive` invocation; admin-stats scrape. ~120 LoC.
- `tests/differential/tests/upstream_connection_pooling_and_per_class_counters.rs` — Docker-gated wrapper mirroring the 12.2 `upstream_active_health_check.rs` shape. ~50 LoC.
- `crates/envoy-bin/tests/upstream_connection_pooling.rs` — in-process H1 backstop (subprocess discipline per 09 REVIEW M3). ~250 LoC.

**State-2 PLAN-write deliverables (THIS commit, separate from state-3 task commits):**
- CREATE `docs/envoy-rust/phases/13.1-h1-pool-and-fixture/PLAN.md` (this file).
- CREATE `docs/envoy-rust/phases/13.1-h1-pool-and-fixture/PROGRESS.md` (skeleton + Task 1 preamble).
- MODIFY `docs/envoy-rust/ROADMAP.md` (row `13.1` `planned → in-progress`).
- MODIFY `docs/envoy-rust/STATE.md` (4 top-pointer rewrites + append `### Phase-13.1 state-2 PLAN-write` Notes subsection).

---

## Architecture Lock-Ins

These bind state-3 execution. Read all 18 before starting Task 1.

1. **Cycle-resolution: external `H1PoolManager` injection (NOT field-on-`Cluster`).** The SPEC §5.1 left the seam open ("bin-wired injection ... no new trait"). PLAN-write reads the existing `crates/envoy-bin/src/main.rs:124-140` flow (where `cluster_mgr` is constructed via `from_bootstrap` then `health_scheduler` is spawned bin-side AFTER + alongside, NOT a field on `cluster_mgr`) and confirms the analogous shape applies to the H1 pool: a sibling `Arc<H1PoolManager>` is constructed bin-side after `cluster_mgr`, holding one `Arc<H1Pool>` per H1 cluster (lookup by cluster name). NO modification to `envoy-cluster::Cluster`'s struct shape; NO new trait declared in `envoy-cluster`; NO new top-level Cargo dep. The HCM proxy arm consults `pool_mgr` (passed in alongside `cluster_mgr` at HCM-config construction time) to acquire a connection. **Rationale:** mirrors the 12.2 `envoy-health::Scheduler` precedent verbatim; avoids interior-mutability or trait-object indirection on the load-bearing `Cluster`; keeps the H1Pool's `ClientStream` type private to envoy-http1.

2. **Default-enabled pool with hardcoded defaults when `circuit_breakers` is absent (§5.4).** When a cluster's bootstrap YAML carries no `circuit_breakers` block, the pool manager registers a pool with `max_connections = 1024` (the upstream Envoy v1.33 default per §6.2 item-i) + `idle_timeout = Duration::from_secs(60)` (the phase-13 hardcoded default per §2 item-iii — the config-side `idle_timeout` knob defers per §4). The 19 existing fixtures (`0001`-`0019`) configure no `circuit_breakers`; they pool transparently with these defaults. Regression-equivalence: all 19 existing fixtures' `upstream_cx_total` assertions are `name-required, value-may-differ` (presence-only), which pool-based accounting cannot regress. Verified at Task 4 by running the existing 19-fixture suite green simultaneously.

3. **`upstream_cx_total` BEHAVIOR_CONTRACT row stays `name-required, value-may-differ` at 13.1.** Phase 13.1 does NOT tighten the row — the H2 cluster fixture `0010` still emits per-call accounting (the H2 pool defers to 13.2), and the row mentions no protocol carve-out, so tightening globally would falsify the H2 surface. The row tightening is **13.2's headline contract-surface deliverable** (D7.1; the 06.3 REVIEW I2 (b) full-closure site). Task 5 (D7) wires the 2 new pool stat rows (`upstream_cx_destroy` + `upstream_cx_http1_total`) but explicitly leaves row `:89` unchanged. PROGRESS at Task 5 names the 13.2 site.

4. **Discriminating-observable fixture-shape lock-in (§2 item-iv): fixture 0020 driver MUST use `Driver::Http1KeepAlive`.** With separate downstream curls, `upstream_cx_total: N` for N requests (pool returns conn AFTER downstream-close, by which time the next downstream conn has triggered a fresh upstream connect). With a single downstream H1 keep-alive conn issuing N sequential requests, `upstream_cx_total: 1` (full pool reuse). The new driver variant (Task 7 D10) lands alongside the new fixture (Task 7 D9.1) — both files in one commit. The driver opens ONE TCP conn to the proxy, sends N HTTP/1.1 requests sequentially (Connection: keep-alive default), reads N responses, closes.

5. **The H1 router-arm migration removes the existing `tier-1 cached_upstream` micro-cache.** The current `hcm.rs:502-527` carries a per-HCM-task `cached_upstream: Option<(String, SocketAddr, ClientStream)>` that caches ONE upstream stream per HCM session — a degenerate single-slot pool. With the new `H1PoolManager` providing per-cluster pooling, this micro-cache is dead. Task 4 removes it cleanly (the variable + the surrounding match arms collapse). PROGRESS at Task 4 attributes the removal honestly + notes the new pool is its strict superset.

6. **The `cx_total.inc()` migrates from `hcm.rs:514` into `H1Pool::acquire()`'s connect-on-miss branch.** The current site at `hcm.rs:514` (`cluster.cx_total().inc();` immediately after a successful `Client::connect`) becomes dead code at Task 4 — the pool's `acquire()` performs the connect-on-miss + the `inc()` in one place. PROGRESS at Task 4 attributes the migration; the migrated stat semantics are identical (one increment per established TCP conn). The TCP-proxy increment at `crates/envoy-tcp/src/lib.rs:108` is **UNTOUCHED** (TCP pool defers per §4).

7. **`ConnGaugeGuard` reused (§5.6).** `crates/envoy-cluster/src/cluster.rs:18-26` `ConnGaugeGuard` is REUSED unchanged. The new `PoolGuard` owns one `ConnGaugeGuard` per acquire. Drop ordering: `PoolGuard::Drop` takes the stream (preventing pool-return on `invalidate()`-flagged guards) → drops the `ConnGaugeGuard` field → `cx_active.dec()` → asynchronously returns the stream to the pool's idle list via `tokio::spawn` (so the synchronous `Drop` doesn't block on a tokio mutex `.await`). Idle pool members do NOT count toward `upstream_cx_active`.

8. **Idle sweeper as the second periodic-background primitive (§5.5).** One `tokio::spawn`-ed task per `H1Pool`, holding `tokio::time::interval(idle_timeout / 4)` (i.e. 15s with the 60s default). Each tick locks `idle`, walks per-endpoint lists, evicts entries with `last_returned.elapsed() > idle_timeout` (each eviction increments `cx_destroy`). Cleanly cancellable on `CancellationToken` per the 12.2 `envoy-health::Scheduler` precedent. The bin owns the `JoinHandle` and aborts on shutdown.

9. **No new top-level Cargo dep (§5.3).** Verified at PLAN-write — the pool uses only existing-pulled crates (`tokio::sync::Mutex`, `tokio::time::interval`, `tokio_util::sync::CancellationToken`, `std::collections::HashMap`, `bytes`, `tracing`, `thiserror`, `envoy_stats`). NO `dashmap`/`deadpool`/`bb8`/`mobc`/`parking_lot` (the existing envoy-http1 crate doesn't pull `parking_lot`; the H1Pool's mutex is `tokio::sync::Mutex` since the pool holds the lock across `.await` in the connect-on-miss branch).

10. **`ClientStream` visibility — no widening needed.** `crates/envoy-http1/src/client.rs:56` `ClientStream` already has `pub(crate)` fields (`stream`, `host`, `buf`). The new sibling `pool.rs` module accesses them directly. No visibility change.

11. **D8 backend extension is additive (no rename, no sibling crate).** The 12.2 `tests/helpers/health-aware-http1-backend/src/main.rs` gains a `--per-path PATH=STATUS[,PATH=STATUS,...]` flag. The crate name + helper-script invocation + the existing `--healthz-status` / `--data-status` / `--data-body` flags carry forward unchanged (12.2 fixture 0019 keeps consuming them). Per-path lookup happens BEFORE the existing `/healthz` special-case; if `--per-path` maps a path to a status, that status (+ deterministic body) wins. Unmapped paths fall through to the existing logic. Deterministic body bytes (PLAN-time pinned): 301 → `"moved\n"` (6 bytes); 404 → `"not found\n"` (10 bytes); 500 → `"server error\n"` (13 bytes); 503 → `"service unavailable\n"` (20 bytes). Any 2xx in `--per-path` uses the same body as `--data-body` (preserves existing semantics).

12. **D11 fuzz-seed sibling files edited together.** New seed `cluster_circuit_breakers.yaml` lands together with: (a) one new line in `crates/envoy-config/fuzz/.gitignore` allow-list; (b) one new entry in `crates/envoy-config/src/bootstrap.rs::tests::fuzz_corpus_seeds_parse_or_reject_cleanly`'s SUCCESS array (20 → 21 entries). Per the 09/10/11/12 Task-7 lesson — these three edits commit as one atomic change.

13. **PROGRESS commit message format per SPEC §8.** State-3 task commits per the 12.2 precedent: `phase 13.1: Task N — <short title>` (no `[ADR-NNNN]` bracket unless an ADR fires at the task per SPEC §7). The state-4 verification + STATE advance lands at Task 10 with the same prefix.

14. **State-4 evidence discipline.** Task 10 quotes per-gate evidence into PROGRESS: real CI run URL + HEAD SHA + completion timestamp + per-gate quoted output (5 stable-toolchain gates, each Docker-gated fixture line, h2spec pass rate, parse_bootstrap fuzz iteration count). Mirrors the 12.2 state-4 narrative shape verbatim.

15. **Carryforward closures attributed at 13.1.** **06.3 REVIEW I2 (a)** (per-class downstream_rq_3xx/4xx/5xx + cluster.upstream_rq_5xx wire-level bilateral coverage) FULLY CLOSED at Task 7 (fixture 0020's per-class assertions over the configurable backend). **06.3 REVIEW I2 (b)** (`upstream_cx_total` value-exact row tightening) primitive landed at Task 3 (the H1 pool itself), but the row tightening DEFERS to 13.2. PROGRESS at Task 7 attributes the (a) closure honestly + names the (b) deferral site (13.2 D7.1).

16. **No new ADR projected at the 13.1 lifecycle (SPEC §7).** DECISIONS.md ledger head stays **ADR-0038**; next available **ADR-0039**. An ADR lands only if execution surfaces a genuine ambiguity warranting durable record (e.g., a non-obvious cycle-resolution that diverges from lock-in #1, OR a non-obvious pool-return-on-protocol-error decision). Neither projected. State-3 tasks do NOT pre-allocate ADR numbers.

17. **State-4 verification fixture count: 20 Docker-gated fixtures green simultaneously (0001-0020).** The new fixture 0020 lands at Task 7. The 19 pre-existing fixtures must regress-equivalence under the new default-enabled pool (lock-in #2). The h2spec gate stays at the parent-05 baseline ≥95% (no H2 codec touch at 13.1). The `parse_bootstrap` fuzz target runs clean on the new 21-seed corpus (lock-in #12).

18. **Subagent-driven execution (`feedback_execution_style`).** State-3 controller dispatches ONE fresh subagent per task, with two-stage review per the 12.2 cadence (per-task review immediately + 3-cluster batch review at state 5). The controller is responsible for the TDD discipline + PROGRESS append at each task close + the per-task commit.

---

## Task 1: `envoy-config` schema extension (`Cluster.circuit_breakers`)

**Goal:** Add `Cluster.circuit_breakers: Option<CircuitBreakers>` + `CircuitBreakers` + `Thresholds` + `RoutingPriority` types to `envoy-config` per SPEC D1. Schema only; validator lands at Task 2.

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (extend `Cluster` struct at `:56`; add new types alongside `HealthCheck` etc.)
- Test: same file's `#[cfg(test)] mod tests` block

**Architectural notes:**
- All new types carry `#[serde(deny_unknown_fields)]` per the established envoy-config discipline. This is what rejects the phase-13-deferred threshold fields (`max_pending_requests`/`max_requests`/`max_retries`/`max_connection_pools`/`track_remaining`/`retry_budget`) at parse time.
- `RoutingPriority` uses `#[serde(rename_all = "SCREAMING_SNAKE_CASE")]` so `Default` → `"DEFAULT"` on the wire (matches upstream Envoy's proto enum projection per §6.2 item-i).
- `Cluster` derives `Serialize` (for `/config_dump` per 08.1). The new types MUST derive `Serialize` too — verify by re-running the existing config_dump tests after the schema change.

- [ ] **Step 1: Write the failing test for the positive parse path**

Add to `crates/envoy-config/src/bootstrap.rs::tests`:

```rust
#[test]
fn cluster_circuit_breakers_parses_minimal_shape() {
    let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: pooled
      type: STATIC
      lb_policy: ROUND_ROBIN
      circuit_breakers:
        thresholds:
          - priority: DEFAULT
            max_connections: 4
      load_assignment:
        cluster_name: pooled
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 8080 }
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
    let bootstrap = crate::parse_bootstrap(yaml).expect("parse");
    let cluster = &bootstrap.static_resources.clusters[0];
    let cb = cluster.circuit_breakers.as_ref().expect("circuit_breakers present");
    assert_eq!(cb.thresholds.len(), 1);
    assert_eq!(cb.thresholds[0].priority, Some(crate::RoutingPriority::Default));
    assert_eq!(cb.thresholds[0].max_connections, Some(4));
}

#[test]
fn cluster_circuit_breakers_omitted_yields_none() {
    let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: c
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: c
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 1 }
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
    let bootstrap = crate::parse_bootstrap(yaml).expect("parse");
    assert!(bootstrap.static_resources.clusters[0].circuit_breakers.is_none());
}

#[test]
fn cluster_circuit_breakers_rejects_phase13_deferred_threshold_fields() {
    // deny_unknown_fields rejects max_pending_requests etc.
    let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: c
      type: STATIC
      lb_policy: ROUND_ROBIN
      circuit_breakers:
        thresholds:
          - max_connections: 1
            max_pending_requests: 5
      load_assignment:
        cluster_name: c
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 1 }
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
    let err = crate::parse_bootstrap(yaml).expect_err("must reject");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("max_pending_requests") || msg.contains("unknown field"),
        "expected unknown-field error mentioning max_pending_requests; got: {msg}"
    );
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p envoy-config -- cluster_circuit_breakers`
Expected: FAIL — types don't exist; `cluster.circuit_breakers` field doesn't exist.

- [ ] **Step 3: Implement the schema in `crates/envoy-config/src/bootstrap.rs`**

Add the three new types alongside `HealthCheck` (find via `grep -n 'pub struct HealthCheck\|pub struct CommonLbConfig' crates/envoy-config/src/bootstrap.rs`):

```rust
/// 13.1 D1 (parent-13 D1): per-cluster circuit-breaker thresholds. At
/// phase-13 scope ONLY `thresholds[0].{priority?, max_connections?}` are
/// supported; the phase-13-deferred fields per parent SPEC §4 (`max_pending_requests`,
/// `max_requests`, `max_retries`, `max_connection_pools`, `track_remaining`,
/// `retry_budget`) are rejected by `deny_unknown_fields`. The validator at
/// `validate_circuit_breakers` (Task 2) enforces at-most-one entry + DEFAULT-only
/// priority + non-zero max_connections.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CircuitBreakers {
    #[serde(default)]
    pub thresholds: Vec<Thresholds>,
}

/// 13.1 D1: a single circuit-breaker threshold entry. See `CircuitBreakers`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Thresholds {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<RoutingPriority>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_connections: Option<u32>,
}

/// 13.1 D1: Envoy `RoutingPriority` enum. Phase-13 supports DEFAULT only; the
/// validator rejects HIGH explicitly (the only other variant in upstream
/// Envoy v1.33). Serializes/deserializes as `"DEFAULT"` / `"HIGH"` per the
/// upstream proto JSON enum convention.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RoutingPriority {
    Default,
    High,
}
```

Then extend `Cluster` (at `:56`) — locate the existing `pub health_checks: Vec<HealthCheck>,` line and add the new field AFTER `common_lb_config` (to keep field grouping logical):

```rust
    // ... existing fields through common_lb_config ...
    /// 13.1 D1 (parent-13 D1): per-cluster circuit-breaker configuration.
    /// `None` means defaults (the §5.4 default-enabled-pool reads
    /// `max_connections: 1024` per upstream Envoy v1.33). See `CircuitBreakers`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub circuit_breakers: Option<CircuitBreakers>,
```

Also re-export the new types in `crates/envoy-config/src/lib.rs` (find via `grep -n 'pub use crate::bootstrap::' crates/envoy-config/src/lib.rs`):

```rust
pub use crate::bootstrap::{
    // ... existing re-exports ...
    CircuitBreakers, RoutingPriority, Thresholds,
};
```

- [ ] **Step 4: Update existing `Cluster` construction sites in tests for the new field**

The defense-in-depth tests in `crates/envoy-cluster/src/cluster.rs` (around `:803, :825`) construct `Cluster` by hand and will fail-compile without `circuit_breakers`. Add `circuit_breakers: None,` to those sites — locate via:

```bash
grep -rn 'health_checks: vec!\[\],\s*common_lb_config: None,' crates/ tests/
```

For each match, add `circuit_breakers: None,` after `common_lb_config: None,`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p envoy-config -- cluster_circuit_breakers` + `cargo test --workspace`
Expected: PASS (3 new tests + 0 regressions; the existing `Cluster`-by-hand constructors compile after Step 4).

- [ ] **Step 6: Verify gates clean**

Run:
- `cargo build --workspace --all-targets`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo fmt --all -- --check`

Expected: all clean.

- [ ] **Step 7: PROGRESS append + commit**

Append a `### Task 1 — envoy-config Cluster.circuit_breakers schema (D1)` subsection to `docs/envoy-rust/phases/13.1-h1-pool-and-fixture/PROGRESS.md` quoting the test names + file paths touched + per-gate clean outputs (per the 12.2 task-close cadence).

Commit:
```
phase 13.1: Task 1 — envoy-config Cluster.circuit_breakers schema (D1)
```

---

## Task 2: `envoy-config` validator extension (`validate_circuit_breakers`)

**Goal:** Add 3 `ConfigError` variants + `validate_circuit_breakers` sub-validator wired at the cluster-validation site. Rejects (a) >1 thresholds entries, (b) HIGH priority, (c) `max_connections: 0`.

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (add error variants + `validate_circuit_breakers` fn + wire call)
- Test: same file

**Architectural notes:**
- 3 new `ConfigError` variants, each carrying `cluster: String` per the established discipline.
- Wire at the existing `validate_health_checks(cluster)?` call site in `parse_bootstrap` (find via `grep -n 'validate_health_checks(cluster)' crates/envoy-config/src/bootstrap.rs` — line `:1686`). Call `validate_circuit_breakers(cluster)?` immediately after.

- [ ] **Step 1: Write failing tests (positive + 3 negative)**

Add to `crates/envoy-config/src/bootstrap.rs::tests`:

```rust
#[test]
fn validate_circuit_breakers_accepts_minimal() {
    // Same YAML as cluster_circuit_breakers_parses_minimal_shape above; round-trips through validator.
    let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: c
      type: STATIC
      lb_policy: ROUND_ROBIN
      circuit_breakers:
        thresholds:
          - priority: DEFAULT
            max_connections: 4
      load_assignment:
        cluster_name: c
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 1 }
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
    crate::parse_bootstrap(yaml).expect("parses and validates");
}

#[test]
fn validate_circuit_breakers_rejects_multiple_thresholds() {
    let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: c
      type: STATIC
      lb_policy: ROUND_ROBIN
      circuit_breakers:
        thresholds:
          - max_connections: 1
          - max_connections: 2
      load_assignment:
        cluster_name: c
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 1 }
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
    let err = crate::parse_bootstrap(yaml).expect_err("must reject");
    assert!(
        matches!(
            err,
            crate::ConfigError::UnsupportedMultipleCircuitBreakerThresholds { ref cluster }
                if cluster == "c"
        ),
        "got {err:?}"
    );
}

#[test]
fn validate_circuit_breakers_rejects_high_priority() {
    let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: c
      type: STATIC
      lb_policy: ROUND_ROBIN
      circuit_breakers:
        thresholds:
          - priority: HIGH
            max_connections: 1
      load_assignment:
        cluster_name: c
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 1 }
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
    let err = crate::parse_bootstrap(yaml).expect_err("must reject");
    assert!(
        matches!(
            err,
            crate::ConfigError::UnsupportedCircuitBreakerPriority { ref cluster, priority: crate::RoutingPriority::High }
                if cluster == "c"
        ),
        "got {err:?}"
    );
}

#[test]
fn validate_circuit_breakers_rejects_zero_max_connections() {
    let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: c
      type: STATIC
      lb_policy: ROUND_ROBIN
      circuit_breakers:
        thresholds:
          - max_connections: 0
      load_assignment:
        cluster_name: c
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 1 }
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
    let err = crate::parse_bootstrap(yaml).expect_err("must reject");
    assert!(
        matches!(
            err,
            crate::ConfigError::InvalidMaxConnections { ref cluster, value: 0 }
                if cluster == "c"
        ),
        "got {err:?}"
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p envoy-config -- validate_circuit_breakers`
Expected: FAIL — variants don't exist; validator not wired.

- [ ] **Step 3: Add the 3 `ConfigError` variants**

Find the existing `pub enum ConfigError` in `crates/envoy-config/src/bootstrap.rs` (search for `pub enum ConfigError`). Add three variants near the 12.1 health-check group:

```rust
    /// 13.1 D2: `circuit_breakers.thresholds` carries >1 entry. Phase-13 supports
    /// exactly 0 or 1 entry (DEFAULT priority only). Multi-priority circuit-breaking
    /// defers per parent SPEC §4.
    #[error(
        "cluster '{cluster}' carries multiple circuit_breakers.thresholds entries — phase-13 supports at most one (DEFAULT priority only)"
    )]
    UnsupportedMultipleCircuitBreakerThresholds { cluster: String },
    /// 13.1 D2: `circuit_breakers.thresholds[0].priority` is non-DEFAULT.
    /// Phase-13 supports DEFAULT only. HIGH priority defers per parent SPEC §4.
    #[error(
        "cluster '{cluster}' carries circuit_breakers.thresholds[0].priority = {priority:?} — phase-13 supports DEFAULT only"
    )]
    UnsupportedCircuitBreakerPriority {
        cluster: String,
        priority: crate::RoutingPriority,
    },
    /// 13.1 D2: `circuit_breakers.thresholds[0].max_connections: 0` is structurally
    /// meaningless — it would prevent any upstream connection. Reject explicitly.
    #[error("cluster '{cluster}' carries circuit_breakers.thresholds[0].max_connections = {value} — must be >= 1")]
    InvalidMaxConnections { cluster: String, value: u32 },
```

- [ ] **Step 4: Implement `validate_circuit_breakers` + wire the call**

Add the sub-validator near `validate_health_checks` (search for `fn validate_health_checks(cluster:` at `:2419`):

```rust
/// 13.1 D2 (parent-13 D2): validate a cluster's `circuit_breakers` block.
/// At phase-13 scope: at-most-one thresholds entry; DEFAULT priority only (or absent);
/// non-zero `max_connections`. Phase-13-deferred threshold fields per parent SPEC §4
/// are rejected by `deny_unknown_fields` automatically at parse time.
fn validate_circuit_breakers(cluster: &Cluster) -> Result<(), crate::ConfigError> {
    let Some(cb) = cluster.circuit_breakers.as_ref() else {
        return Ok(());
    };
    if cb.thresholds.len() > 1 {
        return Err(crate::ConfigError::UnsupportedMultipleCircuitBreakerThresholds {
            cluster: cluster.name.clone(),
        });
    }
    if let Some(t) = cb.thresholds.first() {
        if let Some(priority) = t.priority {
            if priority != crate::RoutingPriority::Default {
                return Err(crate::ConfigError::UnsupportedCircuitBreakerPriority {
                    cluster: cluster.name.clone(),
                    priority,
                });
            }
        }
        if let Some(value) = t.max_connections {
            if value == 0 {
                return Err(crate::ConfigError::InvalidMaxConnections {
                    cluster: cluster.name.clone(),
                    value,
                });
            }
        }
    }
    Ok(())
}
```

Wire the call at `parse_bootstrap`'s cluster-validation loop. Locate `validate_health_checks(cluster)?;` at line `:1686` and insert immediately after:

```rust
        validate_health_checks(cluster)?;
        validate_circuit_breakers(cluster)?;        // 13.1 D2
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p envoy-config`
Expected: PASS (4 new + all existing).

- [ ] **Step 6: Verify gates clean**

Run:
- `cargo build --workspace --all-targets`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo fmt --all -- --check`

Expected: all clean.

- [ ] **Step 7: PROGRESS append + commit**

Commit:
```
phase 13.1: Task 2 — envoy-config validate_circuit_breakers + 3 ConfigError variants (D2)
```

---

## Task 3: `H1Pool` primitive + `H1PoolManager` + idle sweeper (D3)

**Goal:** New module `crates/envoy-http1/src/pool.rs` carrying `H1Pool` + `PoolGuard` + `H1PoolManager` + `PoolError`. Unit-tested in isolation; not yet wired to the HCM (that's Task 4).

**Files:**
- Create: `crates/envoy-http1/src/pool.rs`
- Modify: `crates/envoy-http1/src/lib.rs` (declare + re-export)
- Test: inline `#[cfg(test)] mod tests` in `pool.rs`

**Architectural notes per lock-ins #1, #7, #8, #9, #10:**
- `tokio::sync::Mutex` over `std::sync::Mutex` because `acquire()` holds the lock across `.await` in the connect-on-miss branch.
- The async `Drop` problem: synchronous `Drop` cannot `.await`. Solution: spawn a tokio task to push the stream onto the idle list, mirroring the `tokio::spawn` pattern in 12.2's `health-aware-http1-backend` request-handling.
- `PoolGuard::stream_mut` returns `&mut ClientStream` so the HCM proxy arm can call `send_request(req).await` through it.
- `PoolGuard::invalidate()` marks the stream as un-returnable; Drop then destroys (and increments `cx_destroy`) instead of returning to pool.

- [ ] **Step 1: Write failing unit tests (acquire/reuse/cap/invalidate/idle-sweep/stats)**

Create `crates/envoy-http1/src/pool.rs` with this initial structure (tests included; implementation stubbed to fail):

```rust
//! 13.1 D3: per-cluster, per-endpoint H1 connection pool. Holds an idle
//! keep-alive list of `ClientStream`s; `acquire()` reuses an idle stream
//! or connects a new one (subject to `max_connections` cap). `PoolGuard`
//! is the per-acquire RAII handle; Drop returns the stream to the pool's
//! idle list (success) or destroys it (on `invalidate()` flag, e.g. on
//! protocol error). One `H1Pool` per cluster lives inside `H1PoolManager`,
//! keyed by cluster name; the manager is constructed bin-side at startup
//! and looked up by the H1 HCM proxy arm via `manager.get(cluster_name)`.

#![allow(clippy::type_complexity)]

use crate::client::{Client, ClientStream};
use crate::error::Http1Error;
use envoy_cluster::ConnGaugeGuard;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

/// Phase-13 hardcoded H1 pool defaults (§5.4 + §2 item-iii deferral).
const DEFAULT_MAX_CONNECTIONS: u32 = 1024;
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(60);
/// Sweeper tick interval: `idle_timeout / 4` (15s at the default 60s timeout).
const SWEEPER_DIVISOR: u32 = 4;

/// Errors returned by `H1Pool::acquire`.
#[derive(Debug, thiserror::Error)]
pub enum PoolError {
    /// Pool is at `max_connections` AND no idle stream available.
    #[error("upstream pool overflow: cluster='{cluster}', max_connections={max}")]
    Overflow { cluster: String, max: u32 },
    /// `Client::connect()` failed on the connect-on-miss branch.
    #[error(transparent)]
    Connect(#[from] Http1Error),
}

struct IdleEntry {
    stream: ClientStream,
    last_returned: Instant,
}

/// One pool per cluster. Holds idle keep-alive streams per endpoint, plus
/// the established-count counter (idle + in-flight) for max_connections.
pub struct H1Pool {
    cluster_name: String,
    max_connections: u32,
    idle_timeout: Duration,
    /// Per-endpoint idle list. `tokio::sync::Mutex` because `acquire()`
    /// holds the lock across an `.await` in the connect-on-miss branch.
    idle: Mutex<HashMap<SocketAddr, Vec<IdleEntry>>>,
    /// Per-endpoint total established conn count (idle + in-flight).
    established: Mutex<HashMap<SocketAddr, u32>>,
    /// Per-cluster `upstream_cx_total` — shared Arc with `Cluster.cx_total`
    /// (the same `envoy_stats::Counter` handle; pool's `acquire()` connect-on-miss
    /// is the SOLE incrementer at 13.1 per lock-in #6).
    cx_total: Arc<envoy_stats::Counter>,
    /// Per-cluster `upstream_cx_destroy` — incremented at every pool eviction.
    cx_destroy: Arc<envoy_stats::Counter>,
    /// Per-cluster `upstream_cx_http1_total` — incremented at every H1 connect-on-miss.
    cx_http1_total: Arc<envoy_stats::Counter>,
    /// Per-cluster `upstream_cx_active` gauge handle — shared Arc with `Cluster.cx_active`.
    /// Each `PoolGuard` owns a `ConnGaugeGuard` created via this handle.
    cx_active: Arc<envoy_stats::Gauge>,
}

/// Per-acquire RAII handle. Owns one `ConnGaugeGuard` (gauge decrements on
/// drop) + holds the borrowed `ClientStream` until Drop returns it to the
/// pool's idle list (success) or destroys it (`invalidate()`-flagged path).
pub struct PoolGuard {
    pool: Arc<H1Pool>,
    endpoint: SocketAddr,
    stream: Option<ClientStream>,
    _cx_active_guard: ConnGaugeGuard,
}

impl PoolGuard {
    /// Borrow the underlying `ClientStream` mutably for `send_request`.
    /// Panics if called after `invalidate()` — invalidated guards are intended
    /// to drop immediately, not to send more requests.
    pub fn stream_mut(&mut self) -> &mut ClientStream {
        self.stream.as_mut().expect("stream_mut after invalidate")
    }

    /// Mark the stream as un-returnable. Drop will destroy + increment
    /// `cx_destroy` instead of returning to the pool's idle list. Call on
    /// any protocol-level error that may have left the stream in a
    /// half-broken state.
    pub fn invalidate(&mut self) {
        if let Some(_stream) = self.stream.take() {
            // Stream dropped here (TCP close); destroy bookkeeping done in Drop below.
        }
    }
}

impl Drop for PoolGuard {
    fn drop(&mut self) {
        let pool = Arc::clone(&self.pool);
        let endpoint = self.endpoint;
        match self.stream.take() {
            Some(stream) => {
                // Return-to-pool: synchronous Drop cannot .await, so spawn.
                tokio::spawn(async move {
                    let mut idle = pool.idle.lock().await;
                    idle.entry(endpoint).or_default().push(IdleEntry {
                        stream,
                        last_returned: Instant::now(),
                    });
                });
            }
            None => {
                // Destroy path (invalidated): decrement established + count destroy.
                pool.cx_destroy.inc();
                tokio::spawn(async move {
                    let mut est = pool.established.lock().await;
                    if let Some(n) = est.get_mut(&endpoint) {
                        *n = n.saturating_sub(1);
                    }
                });
            }
        }
        // _cx_active_guard's Drop fires here → upstream_cx_active.dec().
    }
}

impl H1Pool {
    /// Build a new pool. `cx_total`/`cx_active` come from the existing cluster
    /// stat handles (shared `Arc`); `cx_destroy`/`cx_http1_total` are
    /// registered by the caller (see `H1PoolManager::for_bootstrap`).
    pub fn new(
        cluster_name: String,
        max_connections: u32,
        idle_timeout: Duration,
        cx_total: Arc<envoy_stats::Counter>,
        cx_destroy: Arc<envoy_stats::Counter>,
        cx_http1_total: Arc<envoy_stats::Counter>,
        cx_active: Arc<envoy_stats::Gauge>,
    ) -> Arc<Self> {
        Arc::new(Self {
            cluster_name,
            max_connections,
            idle_timeout,
            idle: Mutex::new(HashMap::new()),
            established: Mutex::new(HashMap::new()),
            cx_total,
            cx_destroy,
            cx_http1_total,
            cx_active,
        })
    }

    /// Acquire a stream to `endpoint`. Reuses an idle stream if any; otherwise
    /// creates a new TCP connection (subject to `max_connections`). On
    /// overflow + no idle, returns `PoolError::Overflow`.
    pub async fn acquire(
        self: &Arc<Self>,
        endpoint: SocketAddr,
        host: &str,
    ) -> Result<PoolGuard, PoolError> {
        // Try idle reuse first (synchronous pop under lock).
        {
            let mut idle = self.idle.lock().await;
            if let Some(list) = idle.get_mut(&endpoint) {
                if let Some(entry) = list.pop() {
                    // Reuse: established count unchanged (was already counted at original connect).
                    // Bind cx_active_guard via a fresh per-PoolGuard.
                    let _cx_active_guard = self.acquire_cx_active_guard();
                    return Ok(PoolGuard {
                        pool: Arc::clone(self),
                        endpoint,
                        stream: Some(entry.stream),
                        _cx_active_guard,
                    });
                }
            }
        }
        // Connect-on-miss: enforce cap.
        {
            let mut est = self.established.lock().await;
            let n = est.entry(endpoint).or_insert(0);
            if *n >= self.max_connections {
                return Err(PoolError::Overflow {
                    cluster: self.cluster_name.clone(),
                    max: self.max_connections,
                });
            }
            *n += 1;
        }
        // Connect (lock released — connect is the slow path).
        let stream = match Client::connect(endpoint, host).await {
            Ok(s) => s,
            Err(e) => {
                // Roll back the established count.
                let mut est = self.established.lock().await;
                if let Some(n) = est.get_mut(&endpoint) {
                    *n = n.saturating_sub(1);
                }
                return Err(PoolError::Connect(e));
            }
        };
        // Fire the two connect-on-miss counters (lock-in #6 + lock-in #3 namespacing).
        self.cx_total.inc();
        self.cx_http1_total.inc();
        let _cx_active_guard = self.acquire_cx_active_guard();
        Ok(PoolGuard {
            pool: Arc::clone(self),
            endpoint,
            stream: Some(stream),
            _cx_active_guard,
        })
    }

    /// Internal: build a `ConnGaugeGuard` for the `cx_active` gauge via inc+wrap.
    /// Inlined to keep `ConnGaugeGuard` construction private to envoy-cluster (the
    /// only public path through `Cluster::cx_active_guard` requires a `Cluster`).
    /// 13.1 deviates: pool callers don't hold a `Cluster`; the inc+wrap pattern
    /// is duplicated here against the shared `Arc<Gauge>` (load-bearing: the
    /// gauge handle is the SAME Arc the cluster holds).
    fn acquire_cx_active_guard(&self) -> ConnGaugeGuard {
        self.cx_active.inc();
        // Need a constructor; envoy-cluster's `ConnGaugeGuard` field is private.
        // 13.1 PLAN-time: add a `ConnGaugeGuard::from_gauge` pub(crate)-or-pub
        // constructor in envoy-cluster (Task 3 Step 3 modifies envoy-cluster too).
        ConnGaugeGuard::from_gauge(Arc::clone(&self.cx_active))
    }

    /// Spawn the idle-timeout sweeper task. The returned `JoinHandle` is owned
    /// by the caller (typically `H1PoolManager` -> envoy-bin). Aborts cleanly
    /// when `token` cancels.
    pub fn spawn_idle_sweeper(
        self: &Arc<Self>,
        token: CancellationToken,
    ) -> tokio::task::JoinHandle<()> {
        let pool = Arc::clone(self);
        let interval_period = pool.idle_timeout / SWEEPER_DIVISOR;
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(interval_period);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = token.cancelled() => return,
                    _ = tick.tick() => pool.sweep_once().await,
                }
            }
        })
    }

    async fn sweep_once(self: &Arc<Self>) {
        let now = Instant::now();
        let mut idle = self.idle.lock().await;
        let mut est = self.established.lock().await;
        for (endpoint, list) in idle.iter_mut() {
            let before = list.len();
            list.retain(|entry| now.duration_since(entry.last_returned) < self.idle_timeout);
            let evicted = before - list.len();
            if evicted > 0 {
                if let Some(n) = est.get_mut(endpoint) {
                    *n = n.saturating_sub(evicted as u32);
                }
                for _ in 0..evicted {
                    self.cx_destroy.inc();
                }
            }
        }
    }
}

/// Per-bootstrap registry of `Arc<H1Pool>` keyed by cluster name. Constructed
/// bin-side after `from_bootstrap`. The H1 HCM proxy arm looks up its pool via
/// `manager.get(cluster_name)`.
pub struct H1PoolManager {
    pools: HashMap<String, Arc<H1Pool>>,
    /// Idle-sweeper JoinHandles, one per pool. Owned for lifetime parity with
    /// envoy-bin's `health_scheduler.shutdown().await`; aborted on token cancel.
    _sweepers: Vec<tokio::task::JoinHandle<()>>,
}

impl H1PoolManager {
    /// Build the pool registry from the parsed bootstrap + the constructed
    /// `ClusterManager` (the latter is the source of the existing `Arc<Counter>`
    /// for `upstream_cx_total` + `Arc<Gauge>` for `upstream_cx_active`, both
    /// already registered at `from_bootstrap` time). One pool per cluster
    /// (default-enabled per §5.4 lock-in #2); H2 clusters' pools defer to 13.2
    /// — at 13.1 the manager builds pools ONLY for H1 clusters (i.e.,
    /// `cluster.upstream_protocol() == Http1`).
    pub fn for_bootstrap(
        bootstrap: &envoy_config::Bootstrap,
        cluster_mgr: &envoy_cluster::ClusterManager,
        registry: Arc<envoy_stats::StatsRegistry>,
        token: CancellationToken,
    ) -> Result<Arc<Self>, envoy_stats::StatsError> {
        let mut pools: HashMap<String, Arc<H1Pool>> = HashMap::new();
        let mut sweepers: Vec<tokio::task::JoinHandle<()>> = Vec::new();
        for cfg in &bootstrap.static_resources.clusters {
            let handle = cluster_mgr
                .get(&cfg.name)
                .expect("cluster present in mgr (built from same bootstrap)");
            if handle.upstream_protocol() != envoy_cluster::UpstreamProtocol::Http1 {
                continue;
            }
            let max_connections = cfg
                .circuit_breakers
                .as_ref()
                .and_then(|cb| cb.thresholds.first())
                .and_then(|t| t.max_connections)
                .unwrap_or(DEFAULT_MAX_CONNECTIONS);
            let cx_destroy = registry
                .register_counter(&format!("cluster.{}.upstream_cx_destroy", cfg.name))?;
            let cx_http1_total = registry
                .register_counter(&format!("cluster.{}.upstream_cx_http1_total", cfg.name))?;
            // Re-register cx_total + cx_active for the shared Arc (idempotent
            // same-kind contract — envoy-stats returns the same Arc on second
            // register).
            let cx_total = registry
                .register_counter(&format!("cluster.{}.upstream_cx_total", cfg.name))?;
            let cx_active = registry
                .register_gauge(&format!("cluster.{}.upstream_cx_active", cfg.name))?;
            let pool = H1Pool::new(
                cfg.name.clone(),
                max_connections,
                DEFAULT_IDLE_TIMEOUT,
                cx_total,
                cx_destroy,
                cx_http1_total,
                cx_active,
            );
            sweepers.push(pool.spawn_idle_sweeper(token.clone()));
            pools.insert(cfg.name.clone(), pool);
        }
        Ok(Arc::new(Self { pools, _sweepers: sweepers }))
    }

    /// Look up the pool for `cluster_name`. Returns `None` if no H1 cluster
    /// with that name exists (e.g., HCM routes to an H2 cluster — the H2 path
    /// stays on per-call `Client::connect` until 13.2).
    pub fn get(&self, cluster_name: &str) -> Option<&Arc<H1Pool>> {
        self.pools.get(cluster_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU16;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Per-test counter/gauge registration via a fresh registry.
    fn mk_pool(
        cluster: &str,
        max_connections: u32,
        idle_timeout: Duration,
    ) -> (Arc<H1Pool>, Arc<envoy_stats::Counter>, Arc<envoy_stats::Counter>, Arc<envoy_stats::Counter>, Arc<envoy_stats::Gauge>) {
        let registry = envoy_stats::StatsRegistry::new();
        let cx_total = registry
            .register_counter(&format!("cluster.{cluster}.upstream_cx_total"))
            .unwrap();
        let cx_destroy = registry
            .register_counter(&format!("cluster.{cluster}.upstream_cx_destroy"))
            .unwrap();
        let cx_http1_total = registry
            .register_counter(&format!("cluster.{cluster}.upstream_cx_http1_total"))
            .unwrap();
        let cx_active = registry
            .register_gauge(&format!("cluster.{cluster}.upstream_cx_active"))
            .unwrap();
        let pool = H1Pool::new(
            cluster.to_string(),
            max_connections,
            idle_timeout,
            Arc::clone(&cx_total),
            Arc::clone(&cx_destroy),
            Arc::clone(&cx_http1_total),
            Arc::clone(&cx_active),
        );
        (pool, cx_total, cx_destroy, cx_http1_total, cx_active)
    }

    /// In-process echo backend that responds to each request with a minimal
    /// 200 OK. Returns the bound address; accepts forever until dropped.
    async fn echo_backend() -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut sock, _) = listener.accept().await.unwrap();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    loop {
                        let n = sock.read(&mut buf).await.unwrap_or(0);
                        if n == 0 { return; }
                        // Find end-of-headers; reply once per request.
                        if buf[..n].windows(4).any(|w| w == b"\r\n\r\n") {
                            let _ = sock.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: keep-alive\r\n\r\n").await;
                        }
                    }
                });
            }
        });
        addr
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn acquire_from_empty_pool_creates_connection_and_fires_counters() {
        let addr = echo_backend().await;
        let (pool, cx_total, _cx_destroy, cx_http1_total, cx_active) =
            mk_pool("c", 4, Duration::from_secs(60));
        let guard = pool.acquire(addr, "host.example").await.expect("acquire");
        assert_eq!(cx_total.value(), 1, "cx_total fires on connect-on-miss");
        assert_eq!(cx_http1_total.value(), 1, "cx_http1_total fires on connect-on-miss");
        assert_eq!(cx_active.value(), 1, "cx_active increments via guard");
        drop(guard);
        // Drop spawns return-to-pool; give the runtime a tick.
        tokio::task::yield_now().await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(cx_active.value(), 0, "cx_active decrements on guard drop");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn acquire_after_return_reuses_idle_stream_without_incrementing_cx_total() {
        let addr = echo_backend().await;
        let (pool, cx_total, _cx_destroy, _cx_http1_total, _cx_active) =
            mk_pool("c", 4, Duration::from_secs(60));
        let g1 = pool.acquire(addr, "h").await.expect("acquire 1");
        drop(g1);
        tokio::time::sleep(Duration::from_millis(50)).await;
        // Re-acquire: pool reuses; cx_total stays at 1.
        let _g2 = pool.acquire(addr, "h").await.expect("acquire 2");
        assert_eq!(cx_total.value(), 1, "reuse must not re-fire cx_total");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn acquire_returns_overflow_when_at_cap() {
        let addr = echo_backend().await;
        let (pool, _, _, _, _) = mk_pool("c", 1, Duration::from_secs(60));
        let _g1 = pool.acquire(addr, "h").await.expect("first acquire");
        let err = pool.acquire(addr, "h").await.expect_err("second acquire must overflow");
        assert!(matches!(err, PoolError::Overflow { ref cluster, max: 1 } if cluster == "c"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn invalidate_destroys_stream_and_increments_cx_destroy() {
        let addr = echo_backend().await;
        let (pool, _cx_total, cx_destroy, _, _) = mk_pool("c", 4, Duration::from_secs(60));
        let mut g = pool.acquire(addr, "h").await.expect("acquire");
        g.invalidate();
        drop(g);
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(cx_destroy.value(), 1, "invalidate path fires cx_destroy");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn idle_sweeper_evicts_past_deadline_entries() {
        let addr = echo_backend().await;
        let (pool, _cx_total, cx_destroy, _, _) = mk_pool("c", 4, Duration::from_millis(100));
        let token = CancellationToken::new();
        let sweeper = pool.spawn_idle_sweeper(token.clone());
        let g = pool.acquire(addr, "h").await.expect("acquire");
        drop(g);
        tokio::time::sleep(Duration::from_millis(50)).await;
        // Pre-sweep: 1 idle, 0 destroyed.
        assert_eq!(cx_destroy.value(), 0);
        // Wait > 2× idle_timeout to ensure at least one sweeper tick fires + evicts.
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(cx_destroy.value() >= 1, "sweeper must evict idle entry past deadline");
        token.cancel();
        let _ = sweeper.await;
    }
}
```

- [ ] **Step 2: Add `ConnGaugeGuard::from_gauge` pub constructor in envoy-cluster**

The `ConnGaugeGuard`'s `gauge` field is private. The pool's `acquire_cx_active_guard` needs a constructor that takes an `Arc<Gauge>` directly. Add to `crates/envoy-cluster/src/cluster.rs` near the existing `impl Drop for ConnGaugeGuard`:

```rust
impl ConnGaugeGuard {
    /// 13.1 D3: construct a guard from a pre-incremented `Arc<Gauge>` handle.
    /// The caller MUST have called `gauge.inc()` already; Drop calls `gauge.dec()`.
    /// Mirrors `Cluster::cx_active_guard()`'s `inc + wrap` pattern, exposed for
    /// `envoy-http1::H1Pool` (which doesn't hold a `Cluster` reference but
    /// shares the `Arc<Gauge>` via the StatsRegistry's same-kind-idempotency).
    pub fn from_gauge(gauge: Arc<envoy_stats::Gauge>) -> Self {
        Self { gauge }
    }
}
```

- [ ] **Step 3: Declare module in `crates/envoy-http1/src/lib.rs`**

Find the existing `pub mod` lines (`grep -n 'pub mod\|mod ' crates/envoy-http1/src/lib.rs | head -20`) and add:

```rust
pub mod pool;
```

Add `pub use` re-exports for the public API (find existing `pub use` lines and append):

```rust
pub use pool::{H1Pool, H1PoolManager, PoolError, PoolGuard};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p envoy-http1 -- pool`
Expected: 5 new tests pass.

- [ ] **Step 5: Verify gates clean**

Run all 5 stable-toolchain gates. Expected: clean.

- [ ] **Step 6: PROGRESS append + commit**

Commit:
```
phase 13.1: Task 3 — H1Pool primitive + PoolGuard RAII + idle sweeper + H1PoolManager (D3)
```

---

## Task 4: H1 router-arm pool integration + tier-1 cache removal (D4)

**Goal:** Modify the H1 HCM proxy arm at `crates/envoy-http1/src/hcm.rs:508-527` to dispatch through `H1Pool::acquire()` instead of per-call `Client::connect()`. Remove the dead `tier-1 cached_upstream` micro-cache (lock-in #5). Plumb `Arc<H1PoolManager>` into the HCM constructor (called from envoy-bin at Task 4 Step 4, but the HCM struct change lands here).

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` (proxy-arm dispatch + HCM constructor signature)
- Test: same file's `#[cfg(test)] mod tests` block (new H1 integration test exercising pool reuse)

**Architectural notes:**
- The HCM has a config-bearing struct (`HcmConfig` or similar; locate via `grep -n 'pub struct.*Config\|impl.*HcmConfig' crates/envoy-http1/src/hcm.rs | head -10`). Add `pool_mgr: Option<Arc<H1PoolManager>>` field — `Option` because not every HCM-construction site has a pool manager (some tests don't). The proxy arm uses the pool if present; falls back to per-call `Client::connect` if absent (preserves existing-test behavior).
- The `cached_upstream` variable + the surrounding `match cached_upstream.take()` arm are dead with the pool; remove cleanly.
- The `cluster.cx_total().inc()` site at `:514` is dead (the pool fires it); remove.

- [ ] **Step 1: Write failing integration test**

Add to `crates/envoy-http1/src/hcm.rs::tests` (or a new `#[cfg(test)] mod pool_integration_tests` sibling block):

```rust
#[tokio::test(flavor = "multi_thread")]
async fn h1_hcm_pool_reuses_upstream_conn_across_sequential_requests() {
    // Drive 5 sequential requests through the HCM over a single downstream
    // keep-alive conn against an in-process upstream backend; assert
    // `upstream_cx_total` ends at 1 (pool reuse) rather than 5.
    // Test scaffold: build a minimal Bootstrap, construct ClusterManager,
    // construct H1PoolManager, build HcmConfig with the pool_mgr, drive 5
    // requests via tokio::net::TcpStream against the HCM's listener.
    // (Test body: ~80 LoC; see Task 8's in-process H1 backstop for the
    // fully-fledged subprocess shape — this one is in-crate using inline
    // task::spawn'd HCM serving.)
    //
    // PLAN-time signpost: the test wires together `ClusterManager` +
    // `H1PoolManager` + a 2-listener spawn (HCM + backend) + an in-process
    // downstream conn. The exact in-crate harness wiring is discoverable at
    // task time via the existing `cluster_cx_active_round_trip_through_h1_call`
    // pattern at `crates/envoy-cluster/src/cluster.rs:1308+`.
    unimplemented!("see task signpost above");
}
```

(Implementation discovered during step 3.)

- [ ] **Step 2: Run the test to verify it fails (compile error)**

Run: `cargo build -p envoy-http1 --tests`
Expected: FAIL — the `pool_mgr` field doesn't exist on the HCM config; `H1PoolManager` not yet wired.

- [ ] **Step 3: Wire `Arc<H1PoolManager>` into the HCM config + migrate the proxy arm**

Locate the HCM config struct:

```bash
grep -n 'pub struct .*Config\|cluster_mgr' crates/envoy-http1/src/hcm.rs | head -20
```

Add the optional pool-manager field alongside the cluster manager. The exact constructor signature is discoverable at task time; the modification shape is: one new `Option<Arc<H1PoolManager>>` field + threaded through HCM-config builders.

Modify `crates/envoy-http1/src/hcm.rs:502-527` (the proxy-arm dispatch around the `tier-1 upstream reuse` block):

```rust
                        // 06.3 D15.3.b: RAII guard increments
                        // `cluster.<name>.upstream_cx_active` ... (existing comment retained)
                        let _cx_guard = cluster.cx_active_guard();

                        let start = std::time::Instant::now();

                        // 13.1 D4: dispatch through H1Pool when a pool manager is
                        // configured (the production path; bin-wired per lock-in #1).
                        // Tests without a pool manager fall through to per-call
                        // Client::connect for backwards-compat.
                        let stream_or_synth: Result<
                            Either<PoolGuardOrStream, ClientStream>,
                            Response,
                        > = if let Some(pool_mgr) = pool_mgr_opt.as_ref() {
                            match pool_mgr.get(cluster_name) {
                                Some(pool) => match pool.acquire(endpoint, &host_header).await {
                                    Ok(guard) => Ok(Either::A(PoolGuardOrStream::Pool(guard))),
                                    Err(envoy_http1::PoolError::Connect(source)) => {
                                        tracing::warn!(
                                            cluster = %cluster.name(),
                                            addr = %endpoint,
                                            error = ?source,
                                            "upstream connect failed (pool) — returning 502",
                                        );
                                        Err(synth_status(502, close))
                                    }
                                    Err(envoy_http1::PoolError::Overflow { .. }) => {
                                        tracing::warn!(
                                            cluster = %cluster.name(),
                                            "pool overflow — returning 503",
                                        );
                                        Err(synth_status(503, close))
                                    }
                                },
                                None => {
                                    // Pool manager has no entry for this cluster (H2 cluster
                                    // or post-defer scenario); fall through to per-call.
                                    match Client::connect(endpoint, &host_header).await {
                                        Ok(s) => {
                                            cluster.cx_total().inc();
                                            Ok(Either::B(s))
                                        }
                                        Err(source) => {
                                            tracing::warn!(
                                                cluster = %cluster.name(),
                                                addr = %endpoint,
                                                error = ?source,
                                                "upstream connect failed — returning 502",
                                            );
                                            Err(synth_status(502, close))
                                        }
                                    }
                                }
                            }
                        } else {
                            // No pool manager (test path).
                            match Client::connect(endpoint, &host_header).await {
                                Ok(s) => {
                                    cluster.cx_total().inc();
                                    Ok(Either::B(s))
                                }
                                Err(source) => {
                                    tracing::warn!(
                                        cluster = %cluster.name(),
                                        addr = %endpoint,
                                        error = ?source,
                                        "upstream connect failed — returning 502",
                                    );
                                    Err(synth_status(502, close))
                                }
                            }
                        };

                        match stream_or_synth {
                            Ok(stream_handle) => {
                                let send_result = match &mut stream_handle {
                                    Either::A(PoolGuardOrStream::Pool(g)) => {
                                        g.stream_mut().send_request(out_req).await
                                    }
                                    Either::B(s) => s.send_request(out_req).await,
                                };
                                match send_result {
                                    Ok(upstream_response) => {
                                        let elapsed_ms = start.elapsed().as_millis();
                                        outgoing = crate::router::construct_proxied_response(
                                            &cluster,
                                            upstream_response,
                                            elapsed_ms,
                                            close,
                                        );
                                        // PoolGuard's Drop returns to pool on success (lock-in #7).
                                    }
                                    Err(source) => {
                                        // On send failure, invalidate the pool guard so Drop destroys.
                                        if let Either::A(PoolGuardOrStream::Pool(g)) = &mut stream_handle {
                                            g.invalidate();
                                        }
                                        tracing::warn!(
                                            cluster = %cluster.name(),
                                            addr = %endpoint,
                                            error = ?source,
                                            "upstream request failed — returning 502",
                                        );
                                        outgoing = synth_status(502, close);
                                    }
                                }
                            }
                            Err(resp) => {
                                outgoing = resp;
                            }
                        }
```

The exact `Either` helper + `PoolGuardOrStream` enum can be defined privately at the top of `hcm.rs`. The tier-1 `cached_upstream` variable + the now-dead `Some((cname, addr, s))` cache-hit arm are removed cleanly per lock-in #5.

- [ ] **Step 4: Implement the integration test body**

Wire the test from Step 1: build a single-cluster Bootstrap pointing at an in-process echo backend (use the `echo_backend()` helper pattern from Task 3); construct `ClusterManager` via `from_bootstrap`; construct `H1PoolManager::for_bootstrap`; build the HCM config with the pool manager; spawn the HCM listener; drive 5 sequential GET / requests over a single downstream keep-alive TcpStream; scrape `cluster.backend.upstream_cx_total` — assert it equals 1.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p envoy-http1`
Expected: PASS (new integration test + all existing).

- [ ] **Step 6: Verify gates clean**

Run all 5 stable-toolchain gates. Expected: clean. The `cargo test --workspace` MUST also be clean — if any other crate's test relied on the tier-1 cache observable behavior, address it (likely none — the cache was a transparent perf optimization).

- [ ] **Step 7: PROGRESS append + commit**

PROGRESS attributes: (a) the `tier-1 cached_upstream` removal per lock-in #5; (b) the `cx_total.inc()` migration from `hcm.rs:514` to `H1Pool::acquire()` per lock-in #6.

Commit:
```
phase 13.1: Task 4 — H1 router-arm dispatch through H1Pool::acquire (D4)
```

---

## Task 5: H1 pool stats wiring + BEHAVIOR_CONTRACT rows (D7-H1)

**Goal:** Land 2 new BEHAVIOR_CONTRACT rows for `cluster.<name>.upstream_cx_destroy` + `cluster.<name>.upstream_cx_http1_total`. Verify (via existing integration tests + a new registration-presence test) that the counters are registered + reachable. EXPLICITLY leave the existing `upstream_cx_total` row at `:89` UNCHANGED (defers to 13.2 per lock-in #3).

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (insert 2 rows under the existing 06.1 / 06.3 stat-name-mapping block; near the existing `cluster.<name>.upstream_cx_*` rows)
- Test: `crates/envoy-http1/src/pool.rs::tests` — add registration-presence test against `H1PoolManager::for_bootstrap`

Note: the counter REGISTRATION sites already land at Task 3 inside `H1PoolManager::for_bootstrap`. Task 5 only adds the contract documentation + a registration-presence test, since the values are exercised by Tasks 7/8.

- [ ] **Step 1: Write failing registration-presence test**

Append to `crates/envoy-http1/src/pool.rs::tests`:

```rust
#[tokio::test(flavor = "multi_thread")]
async fn h1_pool_manager_registers_cx_destroy_and_cx_http1_total_per_h1_cluster() {
    let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: c1
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: c1
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 8080 } }
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
    let bootstrap = envoy_config::parse_bootstrap(yaml).expect("parse");
    let registry = Arc::new(envoy_stats::StatsRegistry::new());
    let mgr = envoy_cluster::from_bootstrap(&bootstrap, Arc::clone(&registry))
        .await
        .expect("cluster mgr");
    let token = CancellationToken::new();
    let _pool_mgr = H1PoolManager::for_bootstrap(&bootstrap, &mgr, Arc::clone(&registry), token)
        .expect("pool mgr");
    // The 2 new counters MUST be present in the registry by name.
    assert!(registry.snapshot().counters().any(|c| c.name() == "cluster.c1.upstream_cx_destroy"));
    assert!(registry.snapshot().counters().any(|c| c.name() == "cluster.c1.upstream_cx_http1_total"));
}
```

(If the `registry.snapshot()` API is different, the implementer adapts — find via `grep -n 'fn snapshot\|impl StatsRegistry' crates/envoy-stats/src/lib.rs`.)

- [ ] **Step 2: Run the test to verify it passes (Task 3 already wired the registrations)**

Run: `cargo test -p envoy-http1 -- h1_pool_manager_registers`
Expected: PASS — Task 3 already registers both counters in `H1PoolManager::for_bootstrap`.

- [ ] **Step 3: Add BEHAVIOR_CONTRACT rows**

Open `docs/envoy-rust/BEHAVIOR_CONTRACT.md`. Find the 06.1 entries block (line `:84-89`) carrying the `cluster.<name>.upstream_cx_total` row. Append a new **13.1 entries** block AFTER the 12.2 entries block (after line `:151`):

```markdown
**13.1 entries (H1 connection pool):**

| Stat name | Equivalence | Rationale |
|---|---|---|
| `cluster.<name>.upstream_cx_destroy` | value-exact (0-failures case) | Counter; incremented at every pool eviction (idle-sweeper past-deadline; `PoolGuard::invalidate()` flag on protocol error; max-connections rollback on connect failure). Under the deterministic harness load with no forced-close + the hardcoded 60 s idle timeout (well past the ~5 s fixture settle window per §5.4 lock-in), no idle eviction fires during fixture lifetime → both proxies emit 0 within the fixture window. Future fixtures exercising forced-close or longer settle would harden the disposition. |
| `cluster.<name>.upstream_cx_http1_total` | value-exact | Counter; one increment per H1 pool connect-on-miss (fires at the same site as the existing `upstream_cx_total` for H1 clusters — the H1 pool's `acquire()` connect-on-miss branch per 13.1 D3 + D4). Under the fixture 0020 single-downstream-keep-alive-conn driver issuing 10 sequential requests → both proxies emit 1 (full pool reuse). The `upstream_cx_total` BEHAVIOR_CONTRACT row at `:89` (06.1 initial entry) STAYS `name-required, value-may-differ` AT 13.1 — the row tightening to `value-exact` is the **13.2 D7.1 deliverable** (the 06.3 REVIEW I2 (b) full-closure site, firing only when both H1 + H2 pool uniformly). |
```

- [ ] **Step 4: Verify gates clean**

Run: `cargo fmt --all -- --check` + `cargo build --workspace --all-targets`
Expected: clean (the BEHAVIOR_CONTRACT change is docs-only; the test is mechanical).

- [ ] **Step 5: PROGRESS append + commit**

PROGRESS attributes: NO `upstream_cx_total` row tightening at 13.1 (lock-in #3 + SPEC §3 D7 explicit). Names the 13.2 D7.1 site.

Commit:
```
phase 13.1: Task 5 — H1 pool stats wiring + BEHAVIOR_CONTRACT rows (D7-H1)
```

---

## Task 6: Configurable-status backend extension (D8)

**Goal:** Extend `tests/helpers/health-aware-http1-backend/src/main.rs` with a `--per-path PATH=STATUS[,...]` flag + deterministic per-class body bytes (lock-in #11).

**Files:**
- Modify: `tests/helpers/health-aware-http1-backend/src/main.rs`
- Test: same file's `#[cfg(test)] mod tests` block (new; the 12.2 helper has no test block — Task 6 adds one)

- [ ] **Step 1: Write failing test for `parse_per_path` helper + the per-path serve path**

Add to `tests/helpers/health-aware-http1-backend/src/main.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_per_path_parses_multiple_entries() {
        let m = parse_per_path("/301=301,/404=404,/500=500").expect("parse");
        assert_eq!(m.get("/301"), Some(&301u16));
        assert_eq!(m.get("/404"), Some(&404u16));
        assert_eq!(m.get("/500"), Some(&500u16));
        assert_eq!(m.len(), 3);
    }

    #[test]
    fn parse_per_path_rejects_malformed() {
        assert!(parse_per_path("notakvpair").is_err());
        assert!(parse_per_path("/x=notanumber").is_err());
    }

    #[test]
    fn per_class_body_returns_deterministic_bytes() {
        assert_eq!(per_class_body(301), b"moved\n".as_slice());
        assert_eq!(per_class_body(404), b"not found\n".as_slice());
        assert_eq!(per_class_body(500), b"server error\n".as_slice());
        assert_eq!(per_class_body(503), b"service unavailable\n".as_slice());
        // Other codes fall through to the empty body (deterministic; tests rely on this).
        assert_eq!(per_class_body(200), b"".as_slice());
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --bin health-aware-http1-backend`
Expected: FAIL — `parse_per_path` + `per_class_body` don't exist.

- [ ] **Step 3: Implement the extension**

Add to `tests/helpers/health-aware-http1-backend/src/main.rs`:

```rust
use std::collections::HashMap;

/// 13.1 D8: parse `--per-path` flag value: `PATH=STATUS[,PATH=STATUS,...]`.
/// Returns a map; on malformed input returns Err.
fn parse_per_path(s: &str) -> Result<HashMap<String, u16>> {
    let mut out = HashMap::new();
    for entry in s.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let (path, status) = entry
            .split_once('=')
            .with_context(|| format!("per-path entry missing '=': {entry:?}"))?;
        let status: u16 = status
            .parse()
            .with_context(|| format!("per-path status not numeric: {status:?}"))?;
        out.insert(path.to_string(), status);
    }
    Ok(out)
}

/// 13.1 D8: deterministic per-class body bytes per PLAN-time lock-in #11.
/// 2xx → empty body (preserves existing `--data-body` semantics; per-path 2xx is unusual);
/// 3xx → `"moved\n"`; 4xx-404 → `"not found\n"`; 5xx-500 → `"server error\n"`; 5xx-503 → `"service unavailable\n"`.
/// Other codes → empty body (defensive default).
fn per_class_body(status: u16) -> &'static [u8] {
    match status {
        301 => b"moved\n",
        404 => b"not found\n",
        500 => b"server error\n",
        503 => b"service unavailable\n",
        _ => b"",
    }
}
```

Extend `Config`:

```rust
#[derive(Debug, Clone)]
struct Config {
    port: u16,
    healthz_status: u16,
    data_status: u16,
    data_body: Vec<u8>,
    /// 13.1 D8: per-path status overrides.
    per_path: HashMap<String, u16>,
}
```

Extend `parse_args` (handle `--per-path PATH=STATUS,...` flag):

```rust
            "--per-path" => {
                per_path = parse_per_path(&args[i + 1])
                    .context("parsing --per-path")?;
                i += 2;
            }
```

(Initialize `let mut per_path: HashMap<String, u16> = HashMap::new();` near the other defaults.)

Modify `serve` so the per-path lookup runs BEFORE the `/healthz` special-case:

```rust
    let path = req.path.unwrap_or("/").to_string();
    let (status, body): (u16, Vec<u8>) = if let Some(&s) = cfg.per_path.get(&path) {
        // 13.1 D8: per-path mapping wins.
        (s, per_class_body(s).to_vec())
    } else if path == "/healthz" {
        (cfg.healthz_status, Vec::new())
    } else {
        (cfg.data_status, cfg.data_body.clone())
    };
```

Extend `status_reason` with the new status codes:

```rust
fn status_reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        301 => "Moved Permanently",
        404 => "Not Found",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "OK",
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --bin health-aware-http1-backend` + `cargo test --workspace`
Expected: PASS (3 new tests + 0 regressions; the 12.2 fixture 0019 still consumes `--healthz-status` etc. unchanged).

- [ ] **Step 5: Verify gates clean**

Run 5 stable-toolchain gates. Expected: clean.

- [ ] **Step 6: PROGRESS append + commit**

Commit:
```
phase 13.1: Task 6 — configurable-status backend --per-path flag (D8)
```

---

## Task 7: Fixture 0020 + `Driver::Http1KeepAlive` + Docker wrapper (D9.1 + D10)

**Goal:** Land fixture `0020-upstream-connection-pooling-and-per-class-counters` + the new harness driver variant + the Docker-gated wrapper. Per lock-in #4, the driver + the fixture land together — neither makes sense without the other.

**Files:**
- Create: `tests/fixtures/0020-upstream-connection-pooling-and-per-class-counters/envoy.yaml`
- Create: `tests/fixtures/0020-upstream-connection-pooling-and-per-class-counters/envoy-rust.yaml`
- Create: `tests/fixtures/0020-upstream-connection-pooling-and-per-class-counters/expectations.yaml`
- Create: `tests/differential/tests/upstream_connection_pooling_and_per_class_counters.rs`
- Modify: `tests/differential/src/lib.rs` (add `Driver::Http1KeepAlive` variant + dispatch arm)

- [ ] **Step 1: Add `Driver::Http1KeepAlive` variant + a failing unit test**

In `tests/differential/src/lib.rs`, near the existing `Driver::Http1AfterSettle` variant (around `:140`):

```rust
    /// 13.1 D10: drive N sequential HTTP/1.1 requests over a SINGLE downstream
    /// keep-alive conn. The discriminating-observable shape per parent-13 §2
    /// item-iv: with separate per-request conns, upstream_cx_total: N (the
    /// pool returns the conn AFTER downstream close, by which time the next
    /// downstream conn has triggered a fresh upstream connect). With this
    /// driver, upstream_cx_total: 1 (full pool reuse). The driver also fires
    /// a post-settle admin-stats scrape so the expectations.yaml asserts the
    /// stats snapshot bilaterally.
    Http1KeepAlive {
        /// Sequential requests over the single downstream conn.
        requests: Vec<Http1KeepAliveRequest>,
        /// After all requests + a `settle_ms` sleep, scrape the named admin
        /// stats and assert bilaterally per `expected_stats`.
        #[serde(default)]
        settle_ms: u64,
        /// Bilaterally-asserted stat values after settle.
        #[serde(default)]
        expected_stats: Vec<KeepAliveExpectedStat>,
    },
```

Add the supporting structs alongside the existing `PreRequest`/`AdminScrapeCase`:

```rust
/// 13.1 D10: one HTTP/1.1 request in a `Driver::Http1KeepAlive` sequence.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Http1KeepAliveRequest {
    pub method: String,        // "GET" only at 13.1
    pub path: String,
    pub host: String,
    pub expected_status: u16,
}

/// 13.1 D10: bilateral stat assertion for `Driver::Http1KeepAlive::expected_stats`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct KeepAliveExpectedStat {
    pub name: String,
    pub value: u64,
}
```

Add a unit test in the same file:

```rust
#[test]
fn driver_http1_keep_alive_round_trips_through_serde() {
    let yaml = r#"
driver:
  Http1KeepAlive:
    requests:
      - method: GET
        path: /
        host: example
        expected_status: 200
    settle_ms: 100
    expected_stats:
      - name: cluster.backend.upstream_cx_total
        value: 1
"#;
    let _exp: Expectations = serde_yaml::from_str(yaml).expect("yaml parses");
}
```

(`Expectations` is the existing struct enclosing `Driver`; locate via grep.)

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p differential -- driver_http1_keep_alive_round_trips`
Expected: FAIL — variant doesn't exist.

- [ ] **Step 3: Implement the driver dispatch arm**

Locate the existing dispatch in `tests/differential/src/lib.rs` (search for `Driver::Http1AfterSettle` arm in the match; around `:2199` per the grep output). Add a sibling arm:

```rust
        Driver::Http1KeepAlive { requests, settle_ms, expected_stats } => {
            // Open ONE TCP connection to the proxy + drive N requests sequentially
            // over keep-alive H1; assert per-request status; sleep `settle_ms`;
            // then scrape admin for each `expected_stats` entry and assert byte-equal.
            let mut stream = tokio::net::TcpStream::connect(proxy_addr).await
                .with_context(|| format!("connecting to proxy {proxy_addr}"))?;
            for req in requests {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let wire = format!(
                    "{} {} HTTP/1.1\r\nhost: {}\r\nconnection: keep-alive\r\n\r\n",
                    req.method, req.path, req.host,
                );
                stream.write_all(wire.as_bytes()).await
                    .context("writing request")?;
                stream.flush().await.context("flushing")?;
                // Read response (status + headers + CL body — simple read loop).
                let resp_status = read_h1_response_status(&mut stream).await
                    .context("reading response")?;
                anyhow::ensure!(
                    resp_status == req.expected_status,
                    "expected status {} for {} {}, got {}",
                    req.expected_status, req.method, req.path, resp_status
                );
            }
            // Close downstream conn cleanly.
            drop(stream);
            // Settle.
            tokio::time::sleep(std::time::Duration::from_millis(*settle_ms)).await;
            // Scrape admin /stats and assert.
            for stat in expected_stats {
                let actual = scrape_admin_stat(admin_addr, &stat.name).await
                    .with_context(|| format!("scraping {}", stat.name))?;
                anyhow::ensure!(
                    actual == stat.value,
                    "stat {} expected {} got {}",
                    stat.name, stat.value, actual
                );
            }
            Ok(())
        }
```

The helpers (`read_h1_response_status`, `scrape_admin_stat`) are discovered or written at task time. The existing harness already has admin-stats-scrape code (via `Driver::AdminScrape`); reuse that pattern.

- [ ] **Step 4: Write fixture YAML files**

Create `tests/fixtures/0020-upstream-connection-pooling-and-per-class-counters/envoy.yaml`:

```yaml
node:
  id: node
  cluster: cluster
static_resources:
  listeners:
    - name: l
      address: { socket_address: { address: 0.0.0.0, port_value: {{PORT}} } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend_cluster }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend_cluster
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      dns_lookup_family: V4_ONLY
      circuit_breakers:
        thresholds:
          - priority: DEFAULT
            max_connections: 4
      load_assignment:
        cluster_name: backend_cluster
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: backend, port_value: 8080 } }
admin:
  address: { socket_address: { address: 0.0.0.0, port_value: {{ADMIN_PORT}} } }
```

Create `tests/fixtures/0020-upstream-connection-pooling-and-per-class-counters/envoy-rust.yaml` — IDENTICAL to `envoy.yaml`.

Create `tests/fixtures/0020-upstream-connection-pooling-and-per-class-counters/expectations.yaml`:

```yaml
driver:
  Http1KeepAlive:
    requests:
      - { method: GET, path: /,    host: backend, expected_status: 200 }
      - { method: GET, path: /,    host: backend, expected_status: 200 }
      - { method: GET, path: /,    host: backend, expected_status: 200 }
      - { method: GET, path: /,    host: backend, expected_status: 200 }
      - { method: GET, path: /301, host: backend, expected_status: 301 }
      - { method: GET, path: /404, host: backend, expected_status: 404 }
      - { method: GET, path: /404, host: backend, expected_status: 404 }
      - { method: GET, path: /500, host: backend, expected_status: 500 }
      - { method: GET, path: /500, host: backend, expected_status: 500 }
      - { method: GET, path: /500, host: backend, expected_status: 500 }
    settle_ms: 500
    expected_stats:
      - { name: http.ingress_http.downstream_rq_2xx,            value: 4 }
      - { name: http.ingress_http.downstream_rq_3xx,            value: 1 }
      - { name: http.ingress_http.downstream_rq_4xx,            value: 2 }
      - { name: http.ingress_http.downstream_rq_5xx,            value: 3 }
      - { name: http.ingress_http.downstream_rq_total,          value: 10 }
      - { name: cluster.backend_cluster.upstream_rq_2xx,        value: 4 }
      - { name: cluster.backend_cluster.upstream_rq_3xx,        value: 1 }
      - { name: cluster.backend_cluster.upstream_rq_4xx,        value: 2 }
      - { name: cluster.backend_cluster.upstream_rq_5xx,        value: 3 }
      - { name: cluster.backend_cluster.upstream_rq_total,      value: 10 }
      - { name: cluster.backend_cluster.upstream_cx_total,      value: 1 }
      - { name: cluster.backend_cluster.upstream_cx_http1_total, value: 1 }
```

- [ ] **Step 5: Write Docker-gated wrapper**

Create `tests/differential/tests/upstream_connection_pooling_and_per_class_counters.rs`:

```rust
//! 13.1 D9.1.b: Docker-gated wrapper for fixture
//! 0020-upstream-connection-pooling-and-per-class-counters. Mirrors the 12.2
//! `upstream_active_health_check.rs` shape: boots both proxies + the
//! configurable-status backend on a shared bridge network, drives the
//! `Driver::Http1KeepAlive` pre_requests over a single downstream H1 conn,
//! scrapes admin /stats, asserts byte-equal counters bilaterally.

#![cfg(any())]  // gated #[ignore]; see body
```

The wrapper structure mirrors `tests/differential/tests/upstream_active_health_check.rs` line-for-line — substitute the fixture name + the backend invocation flags (`--per-path /301=301,/404=404,/500=500`). Discoverable at task time via `cat tests/differential/tests/upstream_active_health_check.rs`.

- [ ] **Step 6: Run locally with Docker**

Run: `cargo test -p differential -- upstream_connection_pooling_and_per_class_counters --include-ignored --nocapture`
Expected: PASS bilaterally. If RED, the divergence is the §6.2 item-iv shape OR a counter-value off-by-one — diagnose, fix, re-run.

- [ ] **Step 7: Verify gates clean + commit**

Run 5 stable-toolchain gates. Expected: clean.

Commit:
```
phase 13.1: Task 7 — fixture 0020 + Driver::Http1KeepAlive harness extension (D9.1 + D10; closes 06.3 REVIEW I2 (a))
```

---

## Task 8: In-process H1 backstop (D9.3)

**Goal:** Create `crates/envoy-bin/tests/upstream_connection_pooling.rs` — an in-process subprocess-driven test exercising pool reuse + per-class counter math + the 5-standard-header presence assertion per 10 REVIEW M1.

**Files:**
- Create: `crates/envoy-bin/tests/upstream_connection_pooling.rs`

- [ ] **Step 1: Implement the backstop**

Mirror the 12.2 `crates/envoy-bin/tests/upstream_active_health_check.rs` shape (which the implementer reads first via `cat`). The test should:
1. Spawn the `health-aware-http1-backend` subprocess with `--per-path /301=301,/404=404,/500=500` (use `tokio::process::Command` + `.kill_on_drop(true)` per 09 REVIEW M3).
2. Wait until the backend is ready (`/healthz`-poll loop with deadline).
3. Spawn `envoy-bin` subprocess with a synthesized bootstrap (HCM listener routing `/`, `/301`, `/404`, `/500` to a `circuit_breakers`-configured cluster pointing at the backend; admin listener exposing `/stats`).
4. Drive a per-class workload over a single downstream H1 keep-alive conn.
5. Scrape admin `/stats` and assert:
   - `cluster.backend.upstream_cx_total: 1` (pool reuse)
   - `cluster.backend.upstream_cx_http1_total: 1`
   - per-class downstream + upstream counters match the workload distribution
6. Assert the 5 standard HTTP/1.1 headers (`server`, `date`, `content-length`, `content-type`, `connection`) are present on every non-2xx response (per 10 REVIEW M1 lesson).

- [ ] **Step 2: Run the backstop**

Run: `cargo test -p envoy-bin --test upstream_connection_pooling -- --nocapture`
Expected: PASS.

- [ ] **Step 3: Verify gates clean + commit**

Commit:
```
phase 13.1: Task 8 — in-process H1 backstop (D9.3)
```

---

## Task 9: `parse_bootstrap` fuzz seed (D11)

**Goal:** Add `cluster_circuit_breakers.yaml` seed to the parse_bootstrap fuzz corpus (20 → 21 entries in the SUCCESS array per lock-in #12). Three sibling edits.

**Files:**
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_circuit_breakers.yaml`
- Modify: `crates/envoy-config/fuzz/.gitignore` (one new allow-list line)
- Modify: `crates/envoy-config/src/bootstrap.rs::tests::fuzz_corpus_seeds_parse_or_reject_cleanly` (extend SUCCESS array)

- [ ] **Step 1: Write the failing test (compile error from the new test array entry)**

Modify `crates/envoy-config/src/bootstrap.rs::tests::fuzz_corpus_seeds_parse_or_reject_cleanly` (line `:3518`). Add ONE new entry to the SUCCESS array:

```rust
            "fuzz/corpus/parse_bootstrap/hcm_upstream_active_health_check.yaml",
            "fuzz/corpus/parse_bootstrap/cluster_circuit_breakers.yaml",    // 13.1 D11
```

- [ ] **Step 2: Run to verify it fails (file doesn't exist)**

Run: `cargo test -p envoy-config -- fuzz_corpus_seeds`
Expected: FAIL with `panic!("read .../cluster_circuit_breakers.yaml: ...")`.

- [ ] **Step 3: Create the seed file**

Create `crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_circuit_breakers.yaml`:

```yaml
admin:
  address:
    socket_address:
      address: 0.0.0.0
      port_value: 9901
static_resources:
  listeners: []
  clusters:
    - name: pooled_cluster
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      dns_lookup_family: V4_ONLY
      circuit_breakers:
        thresholds:
          - priority: DEFAULT
            max_connections: 4
      load_assignment:
        cluster_name: pooled_cluster
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 8080
```

- [ ] **Step 4: Extend the fuzz `.gitignore` allow-list**

Modify `crates/envoy-config/fuzz/.gitignore`. Add ONE new line at the end of the allow-list block:

```
!corpus/parse_bootstrap/cluster_circuit_breakers.yaml
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p envoy-config -- fuzz_corpus_seeds`
Expected: PASS.

- [ ] **Step 6: (Optional) Run the fuzz target for ~60s**

Run: `cargo +nightly fuzz run parse_bootstrap -- -runs=200000 -max_total_time=60`
Expected: 0 crashes (continues past previous-run cov ~13414).

- [ ] **Step 7: Verify gates clean + commit**

Commit:
```
phase 13.1: Task 9 — parse_bootstrap fuzz seed cluster_circuit_breakers.yaml (D11)
```

---

## Task 10: State-4 phase-done verification + STATE advance

**Goal:** Run the full state-4 verification per `BOOTSTRAP_PROMPT.md` §7.5 (a)-(e) gates; quote evidence into PROGRESS; advance STATE.md from `13.1` state-3-complete → state-4-complete / state-5-next.

**Files:**
- Modify: `docs/envoy-rust/phases/13.1-h1-pool-and-fixture/PROGRESS.md` (append `### Task 10 — state-4 phase-done verification + STATE advance` subsection with per-gate evidence)
- Modify: `docs/envoy-rust/STATE.md` (advance 4 top pointers + append `### Phase-13.1 state-3 execution arc` subsection)

- [ ] **Step 1: Run all 5 stable-toolchain gates locally**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace
cargo deny check
```

Expected: all clean. Quote outputs (test count, clippy clean, etc.) into PROGRESS.

- [ ] **Step 2: Run the full Docker-gated differential suite**

```bash
cargo test -p differential -- --include-ignored
```

Expected: **20 Docker-gated fixtures green simultaneously** (0001-0020) per lock-in #17.

- [ ] **Step 3: Confirm h2spec ≥95%**

Run: `cargo test -p h2spec -- --include-ignored` (or whatever the conformance suite invocation is — see existing PROGRESS in 12.2 for the exact command).
Expected: parent-05 baseline 99.31% held.

- [ ] **Step 4: Run `parse_bootstrap` fuzz for the short-budget CI duration**

Run: `cargo +nightly fuzz run parse_bootstrap -- -runs=200000`
Expected: 0 crashes.

- [ ] **Step 5: Push HEAD + wait for CI green**

```bash
git push
gh run watch
```

Expected: CI green at HEAD. Quote the run URL + completion timestamp into PROGRESS.

- [ ] **Step 6: Append state-4 PROGRESS subsection**

Mirror the 12.2 Task 8 state-4 PROGRESS subsection shape. Per-gate evidence:
- (a) fixture 0020 line from the `cargo test -p differential` output
- (b) all 20 fixtures green simultaneously
- (c) h2spec line + pass-rate
- (d) fuzz `Done N runs` line + 0 crashes
- (e) 5 stable-toolchain gates per-line outputs
- CI run URL + HEAD SHA + duration

- [ ] **Step 7: Advance STATE.md**

Modify `docs/envoy-rust/STATE.md`'s top 4 pointers:
- Active phase: `13.1` state-2-complete / state-3-next → state-3-complete / state-4-complete / state-5-next
- Next expected skill: `superpowers:subagent-driven-development` → `superpowers:requesting-code-review` scoped to the state-3 commit range
- Last commit / Last updated rewrites

Append a `### Phase-13.1 state-3 execution arc` subsection at end of Notes; preserve all prior subsections verbatim per D-3.5 / D-3.4.

- [ ] **Step 8: Commit + push + confirm CI**

Commit:
```
phase 13.1: Task 10 — state-4 phase-done verification + STATE advance to state-5-next
```

Push + gh run watch. Expected: GREEN.

---

## Self-Review

**1. Spec coverage:** 13.1 SPEC deliverables D1, D2, D3, D4, D7-H1, D8, D9.1-H1, D9.3-H1, D10, D11 each map to a task:
- D1 → Task 1
- D2 → Task 2
- D3 → Task 3
- D4 → Task 4
- D7-H1 → Task 5
- D8 → Task 6
- D9.1 + D10 → Task 7 (atomic per lock-in #4)
- D9.3 → Task 8
- D11 → Task 9
- State-4 verification → Task 10

Gap check: SPEC §1 acceptance signal (a)–(f) all addressed at Task 10. SPEC §2 9 locked findings are baked into lock-ins #2 + #4 + #11. SPEC §5 invariants are reflected in lock-ins #1, #7, #8, #9, #10. SPEC §6 signposts addressed in Tasks 3-4 (cycle-resolution) + Task 7 (discriminating-observable). SPEC §7 no-ADR projection reflected in lock-in #16. SPEC §8 commit format reflected in lock-in #13.

**2. Placeholder scan:** No `TBD`/`TODO`/`fill in later` outside scoped signposts (Task 4 Step 1 + Task 8 Step 1 explicitly signpost the in-crate harness wiring as task-time-discoverable from cited precedents — these are clear pointers, not placeholders).

**3. Type consistency:** `H1Pool`/`H1PoolManager`/`PoolGuard`/`PoolError`/`Http1KeepAliveRequest`/`KeepAliveExpectedStat` consistent across Tasks 3-7. `CircuitBreakers`/`Thresholds`/`RoutingPriority` consistent across Tasks 1-2 + Task 9. `ConfigError::{UnsupportedMultipleCircuitBreakerThresholds, UnsupportedCircuitBreakerPriority, InvalidMaxConnections}` consistent at Task 2.

---

*End of 13.1 PLAN. Subagent-driven execution begins at the next session per `feedback_execution_style`.*
