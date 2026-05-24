# Phase 13.1 (`13.1-h1-pool-and-fixture`) — SPEC

- **Phase id:** `13.1`
- **Slug:** `13.1-h1-pool-and-fixture`
- **Parent:** `13` (`13-connection-pooling`). This is the **first of two sub-phases** the parent-13 state-2 PLAN-write split phase 13 into, per `BOOTSTRAP_PROMPT.md` §6.1 (the parent-13 SPEC-time estimate ~1800–2400 LoC is materially over the ~1500-LoC split gate; LoC re-confirmed at this state-2 PLAN-write). The split decision + the seam rationale are recorded in **ADR-0038**; the full feature narrative + the carved-from parent scope live in `docs/envoy-rust/phases/13-connection-pooling/SPEC.md`.
- **depends-on:** `04 05 06 12` (inherited from parent-13; `04` = the `envoy-http1` codec + the H1 router-proxy arm being pooled; `05` = the depended-on H2 surface in case any pooled-path adjacent code needs awareness — H2 pool itself defers to 13.2; `06` = `envoy-stats` + the `ConnGaugeGuard` RAII pattern; `12` = the `HealthAwareHttp1Backend` synthetic-backend harness primitive that 13.1's configurable-5xx-backend extension builds atop). 13.1 depends only on parent-12 having `done` — no dependency on 13.2.
- **Sub-phase ordering:** `13.1 → 13.2`, strict. 13.2 (the H2 pool + the `cluster.<name>.upstream_cx_total` BEHAVIOR_CONTRACT row tightening + the parent-13 close) cannot land before 13.1 because (a) 13.2's H2 pool mirrors the architectural shape that 13.1 lands for H1; (b) 13.2's BEHAVIOR_CONTRACT row tightening can only fire AFTER both H1 + H2 pool (otherwise H2 cluster fixtures see "per call" semantics while H1 sees "pool reuse" — the row cannot tighten until the contract is uniform across protocols); (c) 13.2 closes parent-13.
- **Status before this SPEC lands:** `planned` (added as a sub-phase row at the parent-13 state-2 split commit).

---

## 1. Goal and acceptance signal

Phase 13.1 lands the **H1 connection-pool primitive + the `circuit_breakers` schema + the H1 router-proxy-arm pool integration + the configurable-status synthetic backend + the fixture/backstop for the H1 surface + the per-class HCM counter bilateral wire coverage** — the first half of phase-13 connection pooling. After 13.1:

- `envoy-config` parses + validates a cluster `circuit_breakers` block (`thresholds[0].max_connections`; phase-13 supports DEFAULT priority only + the `max_connections` field only).
- `envoy-http1` carries a per-cluster, per-endpoint **`H1Pool`** primitive (an idle keep-alive list of `ClientStream`s) with a `PoolGuard` RAII handle owning one `ConnGaugeGuard` per acquire. The H1 router-proxy arm dispatches through `H1Pool::acquire()` rather than per-call `Client::connect()`.
- The 12.2-landed `health-aware-http1-backend` helper crate extends to support **per-path operator-configured status** (a `--per-path` flag mapping path → status), enabling 3xx/4xx/5xx response shaping for fixture 0020.
- A new differential fixture **`0020-upstream-connection-pooling-and-per-class-counters`** bilaterally asserts: (1) `cluster.<name>.upstream_cx_total` deterministic small value under a single-downstream-keep-alive-conn driver issuing N sequential requests through the pooled H1 upstream (the I2 (b) primitive surface — but the BEHAVIOR_CONTRACT row tightening DEFERS to 13.2 where both protocols pool); (2) per-class `http.<stat_prefix>.downstream_rq_{2,3,4,5}xx` + `cluster.<name>.upstream_rq_{2,3,4,5}xx` byte-equal values across proxies under a fixed per-class workload (the **named full-closure site for 06.3 REVIEW I2 (a)**).
- An in-process H1 backstop exercises pool reuse + per-class counter math + the deterministic-pool-count assertion on the H1 path.

**13.1 lands NO H2 pool, NO `cluster.<name>.upstream_cx_total` BEHAVIOR_CONTRACT row tightening, and does NOT close parent-13.** This is the **H1-only seam**; 13.2 lands H2 + the contract tightening + parent-13 close.

> **Inert-when-unconfigured property carry-forward:** the H1 pool is **default-enabled** (matches upstream Envoy v1.33's posture per §2 item-2: Envoy ALWAYS pools regardless of whether `circuit_breakers` is configured; the default `max_connections` is 1024 — never hit under fixture load). When `circuit_breakers` is absent from a cluster's YAML, envoy-rust's H1 pool runs with hardcoded defaults (`max_connections: 1024`; idle timeout: 60 s — the 13.1 deferral per §4). The 19 existing fixtures (`0001`-`0019`) configure NO `circuit_breakers`, so their H1 traffic now goes through the pool with defaults. **Regression-equivalence requires checking that each existing fixture's `upstream_cx_total` expectation tolerates pool-based accounting** — every existing fixture asserts `upstream_cx_total` under the pre-existing `name-required, value-may-differ` BEHAVIOR_CONTRACT disposition (presence only, not value), so pool-based accounting cannot regress any existing fixture's assertion (per the SPEC §5.4 reading + verified at parent-13 SPEC-time).

**Acceptance signal (a)–(f), per `BOOTSTRAP_PROMPT.md` §7.5:**

- **(a)** Fixture `0020-upstream-connection-pooling-and-per-class-counters` green at Docker-gated CI.
- **(b)** All **19 pre-existing differential fixtures** (`0001-tcp-echo` through `0019-upstream-active-health-check`) remain green simultaneously at the same Docker-gated CI run (regression-equivalence; the H1 pool's default-enabled posture must not regress the 13 H1-touching fixtures: `0007/0008/0011/0012/0013/0014/0015/0016/0017/0018/0019` + the H1-on-listener admin paths in `0011/0014/0015`). The 6 H2/TCP/TLS-only fixtures (`0001/0002/0003/0004/0005/0006/0009/0010`) are inert vis-à-vis H1 pooling.
- **(c)** `h2spec` continues at ≥95% (parent-05 baseline 99.31%). 13.1 touches no H2 codec/framing path (the H2 pool defers to 13.2; the H2 cluster fixture 0010 stays under the existing non-pooled path).
- **(d)** `parse_bootstrap` fuzz target clean for the short-budget CI run. **13.1 extends the seed corpus 20 → 21** with the `circuit_breakers` bootstrap shape (the seed exercises the new D1 schema + D2 validator parse path).
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check` all clean.
- **(f)** `REVIEW.md` approved.

A **single CI run** must light gates (a)–(e) simultaneously (the project precedent).

---

## 2. Empirical findings inherited from the parent-13 state-2 verification (locked facts)

The parent-13 state-2 PLAN-write performed the parent SPEC §6.2 HEAVY 9-item empirical verification against `envoyproxy/envoy:v1.33.0` (Docker; a `circuit_breakers`-configured cluster + a configurable-status backend + admin `/stats`; methodology + the full findings table are in the STATE.md `### Phase-13 state-2 split decision` subsection). The findings that bind **13.1**:

1. **(item i)** **`circuit_breakers.thresholds` shape:** `circuit_breakers: { thresholds: [{ priority?: <RoutingPriority>, max_connections?: u32, max_pending_requests?: u32, max_requests?: u32, max_retries?: u32, ... }] }`. The `priority` field defaults to `DEFAULT` (and is omitted from `/config_dump` when DEFAULT). The `max_connections` default is **1024** per upstream Envoy docs (large; never hit under fixture load). `thresholds` is a list keyed by `priority`; at phase-13 scope envoy-rust supports exactly 0 or 1 entry with DEFAULT priority (the validator rejects 2+ entries + non-DEFAULT priorities). The phase-13-deferred threshold fields (`max_pending_requests`/`max_requests`/`max_retries`/...) are rejected by envoy-config's `deny_unknown_fields` discipline. **MATCHES the parent SPEC's recommended projection.** → D1 (`CircuitBreakers`/`Thresholds`) + D2 (validator).

2. **(item ii)** **H1 default pool behavior under NO `circuit_breakers`:** upstream Envoy **ALWAYS pools** regardless of whether `circuit_breakers` is configured. A 5-request workload over a single downstream keep-alive conn against a `circuit_breakers`-absent cluster produces `cluster.<name>.upstream_cx_total: 1` (full pool reuse). The `circuit_breakers` block is OPTIONAL at config-time; absence means defaults (large `max_connections` cap). **MATCHES projection.** → §5.4 default-enabled-pool decision: 13.1's H1 pool runs **with defaults when `circuit_breakers` is absent** (NOT pool-gated). The 19 existing fixtures (none configures `circuit_breakers`) now pool transparently.

3. **(item iv)** **`upstream_cx_total` increment semantics + the discriminating differential observable:** one increment per established upstream TCP connection. **Discriminating observable shape:** with a **SINGLE downstream keep-alive conn** issuing 5 sequential requests through the pool → `upstream_cx_total: 1` (full pool reuse). With **separate downstream conns** (curl invocations) → `upstream_cx_total: N` (one upstream connect per downstream-side conn, because Envoy's H1 pool returns the connection to the idle list AFTER the downstream conn closes, by which point the next downstream conn has already triggered a fresh upstream connect). **MATCHES projection but with nuance.** **PLAN-time fixture-shape lock-in:** the fixture 0020 driver MUST issue multiple requests over a SINGLE downstream keep-alive conn to make `upstream_cx_total` the discriminating differential observable (1 vs. N). The differential harness's existing `Driver::Http1` opens one conn per request — **13.1 must extend the driver to support `Driver::Http1MultiRequest` (or equivalent: a single downstream conn with N sequential requests)** OR fixture 0020 uses a different observable (e.g., a single-request fixture where pool's effect isn't observable, but the per-class counter coverage stands). **Recommended: extend the driver** (~30 LoC; mirrors `Driver::Http1AfterSettle` 12.2 precedent) so the fixture asserts both per-class counters AND deterministic small `upstream_cx_total`.

4. **(item iii)** **H1 idle_timeout knob + default:** lives at `typed_extension_protocol_options.envoy.extensions.upstreams.http.v3.HttpProtocolOptions.common_http_protocol_options.idle_timeout` (a Duration). NOT a cluster top-level field. Default: 1 hour (3600 s) per upstream docs. **MATCHES projection.** → §4 deferral: phase-13 (13.1 + 13.2) does NOT add the config-side `idle_timeout` knob; the pool uses a hardcoded sensible default (60 s — short enough to test idle eviction in a deterministic fuzz/test, long enough to never fire under the 19 fixture's settle windows). The contract-row equivalence dimension for `cluster.<name>.upstream_cx_idle_timeout` (name-required, value-may-differ — timing-dependent) defers to 13.2 if 13.1 doesn't fire that counter.

5. **(item v)** **`upstream_cx_destroy` + sibling counters:** the full set: `upstream_cx_destroy`, `_destroy_local`, `_destroy_remote`, `_destroy_with_active_rq`, `_destroy_local_with_active_rq`, `_destroy_remote_with_active_rq`, `upstream_cx_close_notify`. All per-cluster (NOT per-endpoint). All fire when a pool connection is destroyed (different sub-counters for local-initiated vs remote-initiated; the per-`_with_active_rq` variants fire only when destruction happens while a request was outstanding). **MATCHES projection.** → D7 (stats): phase-13 wires the parent `upstream_cx_destroy` (always-fire sibling); other 5 siblings defer to follow-up phases. The `upstream_cx_close_notify` (peer-initiated Connection: close count) lands at 13.1 if pool-eviction-on-peer-close is implemented; defers otherwise. **13.1 PLAN-writer's call:** recommended — wire `upstream_cx_destroy` AND `upstream_cx_http1_total` at 13.1 (the H1-pool-create counter); defer the 5 sub-siblings.

6. **(item vii)** **Per-class HCM counters under the synthetic 5xx backend fixture:** `http.<stat_prefix>.downstream_rq_{2xx,3xx,4xx,5xx,total}` increments per request per status class — bilateral byte-exact under deterministic harness load (verified empirically: 2× 2xx + 1× 3xx + 1× 4xx + 2× 5xx → matches across both proxies). Listener namespace also mirrors: `listener.<address>.http.<stat_prefix>.downstream_rq_*`. **MATCHES projection.** → D9.1 fixture 0020 lands the per-class workload; the contract rows at `BEHAVIOR_CONTRACT.md:96-100` (06.3-landed value-exact) are EXERCISED bilaterally at the wire level for the FIRST time at this fixture (the I2 (a) closure site — the 06.3 REVIEW §3 named-owner site).

7. **(item viii)** **`cluster.<name>.upstream_rq_5xx` + siblings:** `upstream_rq_{2xx,3xx,4xx,5xx,total}` increments per upstream response per status class. Bilateral byte-exact under deterministic load (verified: 2× 5xx upstream → `cluster.backend_cluster.upstream_rq_5xx: 2`). **MATCHES projection.** → fixture 0020 also asserts the cluster-namespace per-class values (a second I2 (a) closure surface).

8. **(item ix)** **HCM filter-synth bypass on per-class counters:** **CONFIRMED.** Synth-503 (the `no healthy upstream` path; verified body matches ADR-0037's 19 bytes exactly during the empirical run) INCREMENTS `http.<stat_prefix>.downstream_rq_5xx` AND `http.<stat_prefix>.downstream_rq_total` (HCM sees it), but does NOT increment `cluster.<name>.upstream_rq_5xx` NOR `upstream_rq_total` (the synth bypasses the upstream call, so upstream counters don't fire). The bypass is at the cluster layer (no upstream attempt → no upstream counter increment), NOT at the HCM layer. **MATCHES the existing 06.3 BEHAVIOR_CONTRACT note exactly** (lines 96-106 already document this asymmetry). Phase-13 preserves it.

> Items (vi) (H2 pool default behavior + `max_concurrent_streams`) bind 13.2 (the H2 pool); 13.1 inherits the finding via the parent SPEC but does NOT wire any H2-specific stat.

**§6.2 synthesis: ALL 9 items MATCHED the parent-13 SPEC's projections.** No empirical-revision ADR fires at the state-2 split commit (the only ADR is ADR-0038 — the split). The item-(iv) discriminating-observable nuance is a PLAN-time fixture-shape lock-in (single-downstream-keep-alive-conn driver), NOT a wire-contract change.

---

## 3. Deliverables

13.1 carries parent-13 deliverables **D1, D2, D3, D4, D7-H1, D8, D9.1-H1, D9.3-H1, D11**. The state-2 PLAN-writer for 13.1 organizes these into TDD tasks for subagent-driven execution.

### D1 — `envoy-config` schema extension (`Cluster.circuit_breakers.thresholds`)

At `crates/envoy-config/src/bootstrap.rs`, extend the `Cluster` struct (`bootstrap.rs:56`; confirmed at HEAD `221b0fd` to carry `name`, `cluster_type`, `lb_policy`, `load_assignment`, `transport_socket`, `dns_lookup_family`, `typed_extension_protocol_options`, `health_checks` (12.1), `common_lb_config` (12.1) — all with `#[serde(deny_unknown_fields)]`) with one new optional field:

```rust
pub struct Cluster {
    // ... existing fields ...
    #[serde(default)]
    pub circuit_breakers: Option<CircuitBreakers>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CircuitBreakers {
    #[serde(default)]
    pub thresholds: Vec<Thresholds>,           // OPTIONAL; phase-13 supports exactly 0 or 1
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Thresholds {
    #[serde(default)]
    pub priority: Option<RoutingPriority>,     // OPTIONAL; phase-13 supports DEFAULT only;
                                                // absent + DEFAULT both map to DEFAULT (§6.2 item-i)
    #[serde(default)]
    pub max_connections: Option<u32>,          // OPTIONAL; default 1024 per upstream Envoy (§6.2 item-i)
    // OPTIONAL upstream fields that DEFER per §4 are REJECTED by deny_unknown_fields:
    //   max_pending_requests / max_requests / max_retries / max_connection_pools
    //   track_remaining / retry_budget
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RoutingPriority {
    Default,      // serializes/deserializes as "DEFAULT"
    High,         // PHASE-13 REJECTS at validator
}
```

- `Cluster` derives `Serialize` (for `/config_dump`); the new structs must derive it too (the 08.1 Bootstrap Serialize cascade carries forward).
- `RoutingPriority` is a new envoy-config enum mirroring upstream `RoutingPriority` (DEFAULT / HIGH). At phase-13 scope only DEFAULT is supported; the validator rejects HIGH explicitly so the rejection error surfaces a named variant (instead of just a serde error).

### D2 — `envoy-config` validator extension

At the cluster-validation site (the existing `from_bootstrap`/validator path producing `ConfigError`; lives in `crates/envoy-config/src/lib.rs` per the phase-11 SPEC-correction precedent), add a `validate_circuit_breakers(cluster) -> Result<(), ConfigError>` sub-validator. Checks:

- **At most one** `thresholds` entry → `ConfigError::UnsupportedMultipleCircuitBreakerThresholds { cluster }` on `len > 1`.
- The single entry's `priority` is `DEFAULT` (or absent) → `ConfigError::UnsupportedCircuitBreakerPriority { cluster, priority }` on HIGH.
- `max_connections` (if present) is `>= 1` → `ConfigError::InvalidMaxConnections { cluster, value }` on 0 (structurally meaningless — would prevent any upstream connection).
- Phase-13-deferred threshold fields (`max_pending_requests`/`max_requests`/`max_retries`/...) absent — handled automatically by `deny_unknown_fields`. NO dedicated variant for these (the serde rejection at parse time is sufficient + cleaner than a per-field rejection variant; mirrors how `health_checks`'s deferred-checker-type fields are rejected by `deny_unknown_fields`).

Roughly **3 new `ConfigError` variants** (the PLAN-writer may consolidate; each carries `cluster: String` per the established error-context discipline). Each has positive + negative parse-path unit tests. The validator is exercised by the `parse_bootstrap` fuzz target (the D11 seed below seeds it).

### D3 — H1 connection-pool primitive (`crates/envoy-http1/src/pool.rs`)

The headline new architectural primitive. A new file at `crates/envoy-http1/src/pool.rs` declares:

```rust
//! 13.1 D3: per-cluster H1 connection pool. Holds an idle keep-alive list
//! of ClientStream per endpoint; `acquire()` reuses an idle stream or
//! connects a new one (subject to max_connections cap).

pub struct H1Pool {
    cluster_name: String,
    max_connections: u32,
    idle_timeout: std::time::Duration,
    // Per-endpoint idle-connection list (tokio::sync::Mutex because the pool
    // may be held across an .await in acquire()'s connect-on-miss branch).
    idle: tokio::sync::Mutex<HashMap<SocketAddr, Vec<IdleEntry>>>,
    // Per-endpoint total established count (idle + in-flight), for the
    // max_connections cap enforcement.
    established: tokio::sync::Mutex<HashMap<SocketAddr, u32>>,
    // Stat handles registered at pool construct time (clusters own them).
    cx_total: Arc<envoy_stats::Counter>,         // shared with the existing cluster cx_total
    cx_destroy: Arc<envoy_stats::Counter>,
    cx_http1_total: Arc<envoy_stats::Counter>,
    cx_active: Arc<envoy_stats::Gauge>,          // shared with the existing cluster cx_active
}

struct IdleEntry {
    stream: ClientStream,
    last_returned: std::time::Instant,
}

pub struct PoolGuard {
    pool: Arc<H1Pool>,
    endpoint: SocketAddr,
    stream: Option<ClientStream>,   // None after take(), preventing pool-return on Drop
    cx_active_guard: ConnGaugeGuard,  // RAII: gauge decrements on drop
}

impl PoolGuard {
    pub fn stream_mut(&mut self) -> &mut ClientStream { self.stream.as_mut().expect("PoolGuard::stream_mut after take") }
    /// Mark the stream as un-returnable (e.g., on protocol error). Drop will
    /// destroy rather than return-to-pool.
    pub fn invalidate(&mut self) { self.stream = None; }
}

impl Drop for PoolGuard {
    fn drop(&mut self) {
        if let Some(stream) = self.stream.take() {
            // Stream still healthy → return to pool's idle list AT SCOPE EXIT
            // (synchronous; no .await). The pool's next acquire() picks it up.
            // If pool is being dropped concurrently the stream just drops here.
            let pool = Arc::clone(&self.pool);
            let endpoint = self.endpoint;
            tokio::spawn(async move {
                let mut idle = pool.idle.lock().await;
                idle.entry(endpoint).or_default().push(IdleEntry {
                    stream,
                    last_returned: std::time::Instant::now(),
                });
                // NOTE: cx_destroy NOT incremented on return-to-pool; only
                // on actual eviction (peer close, idle timeout, max_conn evict).
            });
        }
        // ConnGaugeGuard's Drop fires AFTER the take() above → upstream_cx_active
        // decrements regardless of return-vs-destroy.
    }
}

impl H1Pool {
    pub fn new(cluster_name: String, max_connections: u32, /* stat handles */) -> Arc<Self>;

    /// Acquire a connection to `endpoint`. Returns an existing idle one if any;
    /// otherwise creates a new connection (subject to max_connections cap).
    /// Returns Err(PoolError::Overflow) if at cap AND no idle stream available.
    pub async fn acquire(self: &Arc<Self>, endpoint: SocketAddr, host: &str)
        -> Result<PoolGuard, PoolError>;

    /// Spawn the idle-timeout sweeper task. Returns the JoinHandle so callers
    /// can cancel on shutdown.
    pub fn spawn_idle_sweeper(self: &Arc<Self>) -> tokio::task::JoinHandle<()>;
}

#[derive(Debug, thiserror::Error)]
pub enum PoolError {
    #[error("upstream pool overflow: cluster={cluster}, max_connections={max}")]
    Overflow { cluster: String, max: u32 },
    #[error("upstream connect failed")]
    Connect(#[from] Http1Error),
}
```

**Cycle-resolution decision (§5.1 lock-in, recommended posture):** the pool is owned by **`envoy-cluster`'s `Cluster`** via `Arc<H1Pool>` injected at `from_bootstrap` time. `envoy-cluster` cannot depend on `envoy-http1` directly (today `envoy-http1` depends on `envoy-cluster`, NOT vice versa — adding a back-edge would create a cycle). The cycle-avoidance pattern: **declare an `H1ClientPool` trait in `envoy-cluster` + implement it in `envoy-http1`**, OR **inject `Arc<H1Pool>` from `envoy-bin` at startup** (the bin-wires-the-pool pattern; mirrors how `envoy-bin` wires the `envoy-health` scheduler at parent-12). **Recommended: bin-wired injection** (no new trait; simpler; same shape as 12.2's `envoy-health::Scheduler` wiring). The 13.1 PLAN-writer reads the existing `from_bootstrap` flow at `crates/envoy-cluster/src/cluster.rs` + the `envoy-bin/src/main.rs` startup path to confirm the seam and decide. **No new top-level Cargo dep; no cycle.** A non-obvious cycle-resolution decision → ADR; a straightforward bin-wired injection → PLAN lock-in only.

**ClientStream visibility:** the existing `ClientStream` at `crates/envoy-http1/src/client.rs:56` has `pub(crate)` fields (visible to sibling modules in `envoy-http1`). The new `pool.rs` is a sibling module → can access `ClientStream`'s internals freely. **No visibility widening needed.**

**Hand-rolled per D-3.2** (*"per-protocol connection pooling ... Must be written from scratch"*). Implementation uses std + tokio + tokio-util + bytes + envoy-stats + the existing envoy-http1 internal types. **No new top-level Cargo dep.** No `dashmap` / `deadpool` / `bb8` / `mobc`. No `unsafe` (the `envoy-http1` crate keeps `#![forbid(unsafe_code)]`).

**Idle sweeper:** a single tokio task spawned per H1Pool, holding a `tokio::time::interval(idle_timeout / 4)`. Each tick walks the per-endpoint idle lists, evicts entries past `idle_timeout`. Per parent-13 §5.5 (second periodic-background primitive), the task is cleanly cancellable on shutdown.

**Unit tests:** acquire-from-empty creates connection; acquire-from-non-empty reuses; return-to-pool puts the stream back; max_connections cap enforced; idle sweeper evicts past-deadline; PoolGuard::invalidate prevents return on Drop; cx_total/cx_destroy/cx_http1_total/cx_active fire at the right sites; the established + idle counts stay consistent through acquire-release cycles.

### D4 — H1 router-proxy-arm pool integration

Modify `crates/envoy-http1/src/hcm.rs` proxy-arm dispatch site (the existing `Client::connect(...)` + `ClientStream::send_request(...)` pattern at `hcm.rs:514` — confirmed at HEAD `221b0fd` via direct read; the SPEC §8's claimed site `router.rs:85-90` is INCORRECT — PLAN-time correction). Replace the per-call connect with `cluster.h1_pool().acquire(endpoint, host).await?` + `pool_guard.stream_mut().send_request(req).await?`. The `PoolGuard` is held for the request's lifetime; Drop returns the stream to the pool on success OR destroys it on protocol error (via `pool_guard.invalidate()` in the error-handling arm).

The `cluster.<name>.upstream_cx_total` increment site **MIGRATES from `hcm.rs:514` into the pool's `acquire()` connect-on-miss branch** (one source of truth — the pool fires `cx_total.inc()` exactly when a new TCP connect succeeds, instead of every call). The router-proxy-arm's `upstream_rq_total` + `upstream_rq_5xx` increment sites are UNCHANGED (they fire on response-receipt regardless of pool/no-pool dispatch).

**TCP-proxy increment site at `crates/envoy-tcp/src/lib.rs:108` is UNTOUCHED** (TCP pooling defers to a follow-up phase). The new H1 pool's `cx_total` increment site is the H1-only path; the TCP proxy continues its per-call increment. This means the existing TCP fixtures (`0001/0003/0004/0005/0006`) keep their per-call `upstream_cx_total` semantics — no regression vs. the existing presence-only assertions. The 13.2-landed BEHAVIOR_CONTRACT row tightening (D7.1) qualifies its scope: "H1 and H2 cluster pools tighten to value-exact under the harness's single-downstream-keep-alive-conn driver; TCP proxy (no pool) stays at name-required, value-may-differ until TCP pooling lands in a follow-up phase."

**The `ConnGaugeGuard` (`crates/envoy-cluster/src/cluster.rs:18-26`) is REUSED:** each `PoolGuard` owns one `ConnGaugeGuard`, so `upstream_cx_active` correctly tracks borrowed connections (idle pool members don't count). Drop ordering: `PoolGuard::Drop` takes the stream → fires `ConnGaugeGuard::Drop` → gauge decrements → the spawned task returns the stream to the pool's idle list asynchronously.

### D7 — H1 pool stats wiring + BEHAVIOR_CONTRACT extensions (NO `upstream_cx_total` row tightening at 13.1)

At pool construct time (per-cluster, in `from_bootstrap`), register against the `Arc<StatsRegistry>`:

- `cluster.<name>.upstream_cx_destroy` (counter; incremented at every pool eviction — peer close, idle timeout, max_connections evict)
- `cluster.<name>.upstream_cx_http1_total` (counter; one increment per H1 pool connect-on-miss)

The existing `cluster.<name>.upstream_cx_total` continues to register at cluster-construct time; the INCREMENT-SITE moves into the pool's `acquire()` connect-on-miss branch (D4 above).

**BEHAVIOR_CONTRACT.md** — at the task where each stat is first wired (the 06.x → 11 → 12 cadence):
- `cluster.<name>.upstream_cx_destroy` row lands at the D7 task. Disposition: `value-exact (0-failures case)` — under deterministic harness load with no forced-close, the destroy counter increments only via idle sweep (which doesn't fire under 13.1 fixture's ~5 s settle window vs. the hardcoded 60 s idle timeout) OR via downstream-close-driven pool destruction. **PLAN-writer empirically pins** the exact 0-vs-N-case at fixture-build time.
- `cluster.<name>.upstream_cx_http1_total` row lands at the D7 task. Disposition: `value-exact` (one increment per H1 pool connect; under the fixture's single-downstream-keep-alive-conn driver issuing N sequential requests → both proxies emit 1).

**The `cluster.<name>.upstream_cx_total` row at `BEHAVIOR_CONTRACT.md:89` is NOT TIGHTENED AT 13.1.** The row stays `name-required, value-may-differ` because the H2 cluster's pool defers to 13.2 — tightening at 13.1 would tighten globally (the row mentions no protocol carve-out), but the H2 cluster fixture 0010 still emits per-call accounting until 13.2. **The contract row tightening lands at 13.2 D7.1** when both protocols pool uniformly. Phase-13 LIFECYCLE-wise: the row tightening is parent-13's headline contract-surface deliverable; it lands at 13.2 (which closes parent-13). PROGRESS at 13.1 D7 explicitly states the row is NOT yet tightened + names the 13.2 site.

### D8 — Configurable-status synthetic HTTP/1.1 backend (extend the 12.2 `health-aware-http1-backend`)

The 12.2-landed helper at `tests/helpers/health-aware-http1-backend/src/main.rs` already supports `--healthz-status` (default 503) + `--data-status` (default 200) + `--data-body` (default `ok\n`). It serves:
- `GET /healthz` → `--healthz-status`
- `GET /<anything-else>` → `--data-status`

Phase-13 extends this with **per-path operator-configured status routing** for the 3xx/4xx/5xx workload:

- Add a `--per-path PATH=STATUS[,PATH=STATUS,...]` flag (e.g. `--per-path /301=301,/404=404,/500=500,/503=503`) that maps explicit paths to status codes. Unknown paths fall through to the existing `--data-status` (default 200).
- For each per-path-mapped non-2xx response, the body is a deterministic short string (`"moved\n"` for 3xx, `"not found\n"` for 4xx, `"server error\n"` for 5xx, etc. — exact bodies pinned at PLAN-write per the bilateral fixture's `value_exact` block).
- The existing `--healthz-status` + `--data-status` + `--data-body` flags remain UNCHANGED (12.2 fixture 0019 continues to consume them); the new `--per-path` is purely additive.

**Recommended: extend in-place** (a small additive ~50-LoC edit to `main.rs` + a `parse_per_path()` helper). The crate rename from `health-aware-http1-backend` to e.g. `configurable-http1-backend` is NOT recommended (would churn the 12.2 fixture's harness wrapper); the name's "health-aware" framing is the special case + "per-path-status" is the general case it now supports.

**Architectural footnote:** this primitive fully realizes the synthetic-backend infrastructure 06.3 REVIEW I2 named verbatim ("whichever phase first surfaces the synthetic backend ... the upstream-robustness family"). The 12.2 down-payment landed the SHAPE (a configurable per-path-status backend, with the special case of `/healthz` being the active-HC probe path); 13.1 lands the operator-configurable per-path-status capability the I2 (a) closure requires.

### D9.1 — Fixture `0020-upstream-connection-pooling-and-per-class-counters` + Docker wrapper

- **D9.1.a — `tests/fixtures/0020-upstream-connection-pooling-and-per-class-counters/`** with `envoy.yaml` + `envoy-rust.yaml` (identical: a STRICT_DNS cluster pointing at the configurable backend container; `circuit_breakers: { thresholds: [{ max_connections: 4 }] }`; HCM routes `/` + `/301` + `/404` + `/500` + `/503` through `backend_cluster`; admin `/stats` exposed). The backend container starts with `--per-path /301=301,/404=404,/500=500,/503=503`.

  **Pre_requests workload (~10 requests; tuned at PLAN-write):**
  - 4× `GET /` → 2xx (200)
  - 1× `GET /301` → 3xx (301)
  - 2× `GET /404` → 4xx (404)
  - 3× `GET /500` → 5xx (500)
  
  All requests over a **SINGLE downstream H1 keep-alive connection** (the discriminating-observable lock-in per §2 item-4).
  
  After settle (~500 ms), admin `/stats` scrape asserts:
  - `http.ingress_http.downstream_rq_2xx: 4` (value-exact, bilateral)
  - `http.ingress_http.downstream_rq_3xx: 1` (value-exact, bilateral)
  - `http.ingress_http.downstream_rq_4xx: 2` (value-exact, bilateral)
  - `http.ingress_http.downstream_rq_5xx: 3` (value-exact, bilateral)
  - `http.ingress_http.downstream_rq_total: 10` (value-exact, bilateral)
  - `cluster.backend_cluster.upstream_rq_2xx: 4` (value-exact, bilateral)
  - `cluster.backend_cluster.upstream_rq_3xx: 1` (value-exact, bilateral)
  - `cluster.backend_cluster.upstream_rq_4xx: 2` (value-exact, bilateral)
  - `cluster.backend_cluster.upstream_rq_5xx: 3` (value-exact, bilateral)
  - `cluster.backend_cluster.upstream_rq_total: 10` (value-exact, bilateral)
  - `cluster.backend_cluster.upstream_cx_total: 1` (value-exact, bilateral — the small N via pool reuse; both proxies emit 1 because the single downstream keep-alive conn drives all 10 requests through 1 upstream pool connection)
  - `cluster.backend_cluster.upstream_cx_http1_total: 1` (value-exact, bilateral)
  
  `expectations.yaml` populates the `value_exact` block accordingly. `envoy.yaml` + `envoy-rust.yaml` are IDENTICAL (same schema; both parsers accept the field).

- **D9.1.b — Docker-gated wrapper at `tests/differential/tests/upstream_connection_pooling_and_per_class_counters.rs`** mirroring the 12.2 `upstream_active_health_check.rs` shape: boots both proxies + the backend on a shared bridge network, drives `pre_requests` via the new `Driver::Http1KeepAlive` (D10 below) over a single downstream conn, scrapes admin `/stats`, asserts byte-equal counter values.

### D9.3 — In-process H1 backstop

New file at `crates/envoy-bin/tests/upstream_connection_pooling.rs` mirroring the 12.2 `upstream_active_health_check.rs` shape, with the 09 REVIEW M3 subprocess discipline (`tokio::process::Command` + `.kill_on_drop(true)` + `Stdio::null()`/`piped()`). Boots `envoy-bin` with a synthesized bootstrap + an in-process `configurable-http1-backend` instance (or in-process Python equivalent — PLAN-writer's call); exercises:
- The pool-reuse direction: 5 sequential GET / over a single downstream H1 keep-alive conn → `upstream_cx_total: 1` + `upstream_rq_total: 5` (deterministic small N).
- Per-class counter math: a mix of 2xx/3xx/4xx/5xx requests → `downstream_rq_{2,3,4,5}xx` + `cluster.upstream_rq_{2,3,4,5}xx` assertions match the per-class distribution.
- Per the 10 REVIEW M1 / 12.2 D9.3 lesson, **include the 5-standard-header presence assertion** on any non-2xx response (server, date, content-length, content-type, connection) OR explicitly disclose any omission in PROGRESS. Recommended: include it.

### D10 — Differential-harness driver extension (`Driver::Http1KeepAlive` or equivalent)

The existing harness `Driver::Http1` opens one downstream conn per `pre_requests` entry — which would make `upstream_cx_total` NON-discriminating (N requests → N upstream conns; see §2 item-4 nuance). **13.1 extends the driver** with a single-conn variant:

```rust
// In tests/differential/src/lib.rs (or wherever Driver enum lives):
pub enum Driver {
    // ... existing variants ...
    /// 13.1 D10: drive N sequential requests over a single downstream H1
    /// keep-alive conn. Required for fixture 0020's discriminating
    /// upstream_cx_total observable (§6.2 item-iv finding).
    Http1KeepAlive {
        requests: Vec<HttpRequest>,
    },
}
```

The driver impl opens one TCP conn to the proxy + sends N HTTP/1.1 requests sequentially over that conn (Connection: keep-alive default; reads N responses; closes). ~30 LoC + a small unit test. Mirrors the 12.2 `Driver::Http1AfterSettle` precedent.

**Alternative posture:** the PLAN-writer may extend the existing `Driver::Http1` to accept a `keep_alive: bool` field on each pre_request entry. Recommended: distinct variant (clearer intent + easier to read in `expectations.yaml`).

### D11 — Fuzz corpus seed

New file `crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_circuit_breakers.yaml` containing the `circuit_breakers` bootstrap shape:

```yaml
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 9901 } } }
static_resources:
  listeners: []
  clusters:
  - name: pooled_cluster
    connect_timeout: 1s
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
        - endpoint: { address: { socket_address: { address: 127.0.0.1, port_value: 8080 } } }
```

Extends the corpus 20 → 21, with the `crates/envoy-config/fuzz/.gitignore` allow-list extension AND the `bootstrap.rs::tests::fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS-array extension (both files edited together per the 09/10/11/12 Task-7 lesson).

---

## 4. Out of scope for 13.1 (lands in 13.2 or defers per parent SPEC §4)

- **The H2 connection pool primitive + the H2 router-arm pool integration** (parent D5 + D6) — 13.2.
- **The `cluster.<name>.upstream_cx_total` BEHAVIOR_CONTRACT row tightening from `name-required, value-may-differ` to `value-exact`** (parent D7.1; the 06.3 REVIEW I2 (b) full-closure site) — 13.2 (fires only when both H1 + H2 pool uniformly).
- **The `cluster.<name>.upstream_cx_http2_total` stat row + remaining H2-specific pool stat rows** (parent D7.2) — 13.2.
- **The parent-13 close** (closing-sub-phase invariant: 13.2's state-6 commit flips both 13.2 + parent-13 ROADMAP rows `in-progress → done` simultaneously) — 13.2.
- All of the parent-13 SPEC §4 deferral list: `max_pending_requests` / `max_requests` / `max_retries` / `max_connection_pools` thresholds; non-DEFAULT routing priorities; retries + hedging; outlier detection (passive health); TCP/gRPC/custom active HC checkers; `max_requests_per_connection`; pool-overflow request queueing (phase-13 returns an error on cap reached, no queue); pool warm-up / pre-connect; per-stream H2 graceful drain.
- **TCP-proxy connection pooling** (TCP pool defers — 13.1's pool is H1-specific; envoy-tcp's per-call cx_total increment at `lib.rs:108` stays untouched). The contract row tightening at 13.2 D7.1 carves out the TCP case explicitly.
- **The `idle_timeout` envoy-config knob** (per §2 item-iii: lives under `typed_extension_protocol_options.http.v3.HttpProtocolOptions.common_http_protocol_options.idle_timeout`; phase-13 uses a hardcoded sensible default like 60 s; the schema knob defers to a follow-up phase).

---

## 5. Architectural invariants (inherited from parent SPEC §5)

- **§5.1 the dependency-cycle constraint (the central architectural decision):** the H1 pool lives in `envoy-http1` (owns ClientStream lifetime; H1-specific). `envoy-cluster`'s `Cluster` holds `Arc<H1Pool>` injected at `from_bootstrap` time via the bin-wired injection pattern (`envoy-bin/src/main.rs` constructs the pool + attaches it to the `Cluster`). **No new trait declared in `envoy-cluster` is required** (the injection is a typed `Arc<H1Pool>` field on `Cluster`, conditionally `None` when no H1 cluster is configured OR `Some(default-pool)` always). The 13.1 PLAN-writer reads `from_bootstrap` + `envoy-bin/src/main.rs` to confirm the seam shape; if the bin-wired pattern surfaces a non-obvious complication (e.g. `Cluster`'s lifecycle vs. the pool's), an ADR records the resolution.
- **§5.2 hand-rolled per D-3.2** (*"per-protocol connection pooling ... Must be written from scratch"*): the pool primitive + the idle sweeper are written from scratch atop std + tokio + tokio-util + bytes + envoy-stats. **No new top-level Cargo dep; no `dashmap`/`deadpool`/`bb8`/`mobc`; no `unsafe`.**
- **§5.3 no new top-level Cargo dep:** verified at PLAN-write (the new pool primitive uses only existing-pulled crates). The 13.1 PLAN-writer confirms before locking.
- **§5.4 default-enabled pool (per §2 item-2 finding):** the H1 pool runs with hardcoded defaults (`max_connections: 1024`; `idle_timeout: 60 s`) when `circuit_breakers` is absent. The 19 existing fixtures (none configures `circuit_breakers`) pool transparently. Regression-equivalence guaranteed by the existing presence-only `upstream_cx_total` assertions (no value-exact assertion to regress).
- **§5.5 the pool is the second periodic-background primitive** (after 12.2's `envoy-health` probe loop): the idle-sweeper task. Cleanly cancellable on shutdown; tests assert no leaked task. The 13.1 PLAN-writer confirms graceful-cancellation discipline matches 12.2's `Scheduler::spawn` pattern.
- **§5.6 the pre-built `ConnGaugeGuard` RAII pattern is REUSED:** each `PoolGuard` owns one `ConnGaugeGuard`; the gauge counts borrowed connections; idle pool members don't count toward `upstream_cx_active`.
- **§5.7 the harness extends — no new driver TYPE (just a new variant):** `Driver::Http1KeepAlive` is a new variant on the existing `Driver` enum, not a new harness primitive.

---

## 6. Signposts for the 13.1 state-2 PLAN-writer

- The empirical §6.2 verification is **already done** (parent-13 state-2; findings in §2 above + STATE.md `### Phase-13 state-2 split decision`). **The 13.1 PLAN-writer does NOT re-run Docker** — the 9 findings are locked facts. (If a 13.1 implementation detail surfaces a new empirical question — e.g. the exact upstream-Envoy behavior when pool overflow + max_pending_requests both fire — verify at task time.)
- **PLAN-time SPEC corrections** (read this SPEC against HEAD `<state-2-split-SHA>`; the parent SPEC's claimed seam citations have already been verified at this state-2 PLAN-write):
  - `Cluster` struct at `crates/envoy-cluster/src/cluster.rs:32-76` (confirmed `pub(crate)` fields including `name`, `endpoints`, `cursor`, `upstream_protocol`, `cx_total`, `cx_active`, `upstream_rq_total`, `upstream_rq_5xx`, `endpoint_health`, `panic_threshold`; **no `h1_pool` field yet** — 13.1 D3 adds it).
  - `ConnGaugeGuard` at `cluster.rs:18-26` — confirmed.
  - `envoy-http1::Client::connect` at `client.rs:33` — confirmed `pub async fn connect(addr: SocketAddr, host: &str) -> Result<ClientStream, Http1Error>`.
  - `ClientStream` at `client.rs:56` — confirmed `pub struct` with `pub(crate)` fields (`stream`, `host`, `buf`) — the sibling `pool.rs` module can access these directly without visibility widening.
  - **H1 `upstream_cx_total` increment site is at `crates/envoy-http1/src/hcm.rs:514`, NOT `router.rs:85-90` as the parent SPEC §8 claimed** (parent-13 SPEC §8 was incorrect; PLAN-time lock-in). The migration target is the new pool's `acquire()` connect-on-miss branch.
  - `envoy-tcp/src/lib.rs:108` carries a `cluster.cx_total().inc()` site for TCP-proxy — **UNTOUCHED at 13.1** (TCP pool defers).
  - `Cluster` struct in `envoy-config` at `crates/envoy-config/src/bootstrap.rs:56` carries `health_checks` (12.1) + `common_lb_config` (12.1) — **no `circuit_breakers` field yet**; 13.1 D1 adds it.
  - The 12.2 helper at `tests/helpers/health-aware-http1-backend/src/main.rs` carries `--healthz-status` + `--data-status` + `--data-body` flags — phase-13 D8 adds `--per-path` additively.
  - The `Driver` enum lives at `tests/differential/src/lib.rs:~140` — 13.1 D10 adds `Driver::Http1KeepAlive`.

  Corrections land in the PROGRESS Task 1 preamble.
- **The cycle-resolution decision (§5.1):** the recommended bin-wired injection pattern is straightforward (no new trait; the `Cluster`'s `h1_pool: Option<Arc<H1Pool>>` field is populated by `envoy-bin` at startup after constructing each `Cluster`). The 13.1 PLAN-writer verifies by reading `envoy-bin/src/main.rs` at the `ClusterManager::from_bootstrap` call site; if the verification surfaces a non-obvious lifecycle complication, an inline ADR records the resolution. Recommended posture: **NO ADR** (the injection is ordinary structure).
- **The discriminating-observable fixture-shape lock-in (§2 item-iv):** the fixture 0020 driver MUST use the new `Driver::Http1KeepAlive` over a single downstream conn; this is the load-bearing PLAN lock-in. The 13.1 PLAN-writer organizes Task 5 (fixture 0020 + driver extension) to land both atomically.
- **Subagent-driven execution at state 3** per `feedback_execution_style`. Suggested task organization: D1 schema → D2 validator → D3 H1Pool primitive + idle sweeper + unit tests → D4 H1 router-arm pool integration → D7 pool stats wiring (+ BEHAVIOR_CONTRACT new rows for cx_destroy + cx_http1_total; NO upstream_cx_total row tightening at 13.1) → D8 configurable backend extension → D10 + D9.1 fixture 0020 + Driver::Http1KeepAlive + Docker wrapper → D9.3 in-process backstop → D11 fuzz seed → state-4 verification (19+1 fixtures green + 5 gates + h2spec ≥95% + fuzz on the 21-seed corpus).
- **Carryforward closures engaged at 13.1:** **06.3 REVIEW I2 (a)** (per-class downstream_rq_3xx/4xx/5xx + cluster.<name>.upstream_rq_5xx wire-level bilateral coverage) — **FULLY CLOSED at the D9.1 task** (fixture 0020's per-class assertions). **06.3 REVIEW I2 (b)** (`cluster.<name>.upstream_cx_total` value-exact tightening) — primitive landed at 13.1's H1 pool but the BEHAVIOR_CONTRACT row tightening DEFERS to 13.2 (where both protocols pool uniformly). PROGRESS attributes the I2 (a) closure honestly at 13.1; the I2 (b) closure attribution is 13.2's. Other inventory items (12.2 REVIEW Minor carryforwards A-M2/A-M4/B-M1..B-M6/C-M1/C-M2/C-M4; 12.1 REVIEW M1+M3; phase-11 REVIEW M1-M8; phases 10/09/08.x/07.2/06.x/05.x/04.1/02.2/00 residuals) carry forward unchanged.

---

## 7. ADR projection for 13.1

**Recommended posture: NO new ADR lands during the 13.1 lifecycle.** ADR-0038 (split) lands at the parent-13 state-2 split commit (before 13.1 begins). 13.1 introduces no new crate (the H1 pool is a new module INSIDE `envoy-http1`), no foundations grant, no wire-level contract revision (the new pool stats are ordinary 06.x → 12.x cadence; the `upstream_cx_total` row tightening defers to 13.2). DECISIONS.md ledger head is **ADR-0038** at 13.1 start; next available **ADR-0039**. A 13.1 ADR lands only if execution surfaces a genuine ambiguity (e.g., a non-obvious cycle-resolution OR a non-obvious pool-return-on-protocol-error decision warranting durable record — neither projected).

---

## 8. Commit message format (for state 6 of the 13.1 lifecycle)

```
phase 13.1: H1 connection pool + circuit_breakers schema + H1 router pool integration + fixture 0020 + per-class counter bilateral coverage (06.3 REVIEW I2 (a) closed)

<1-3 sentence summary; names the I2 (a) full-closure site; notes I2 (b) defers to 13.2>

Differential surface: fixture 0020-upstream-connection-pooling-and-per-class-counters; all 20 Docker-gated fixtures (0001-0020) green simultaneously at CI run <ID> HEAD <SHA>.
Conformance: h2spec ≥95% gate held at parent-05 baseline (H2 upstream-client surface untouched at 13.1).
```

(No `[ADR-NNNN]` bracket unless a 13.1 ADR lands per §7.)

---

*End of 13.1 SPEC. The H1 connection pool + envoy-config circuit_breakers schema + per-class counter wire coverage land here; the H2 pool + the `upstream_cx_total` BEHAVIOR_CONTRACT row tightening (the 06.3 REVIEW I2 (b) full-closure site) + parent-13 close all land in 13.2.*
