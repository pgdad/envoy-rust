# Phase 13 (`13-connection-pooling`) — SPEC

- **Phase id:** `13`
- **Slug:** `13-connection-pooling`
- **Status before this SPEC lands:** _not yet in ROADMAP.md_ (per `docs/envoy-rust/ROADMAP.md` at HEAD `3ec7fb9`, the phase-12.2 state-6 close-out commit; the "Upstream robustness family" §9 table at that HEAD carries the parent-12 row + sub-rows `12.1` + `12.2`, all `status: done` — no row exists yet for connection pooling). **This SPEC's landing commit adds the SECOND concrete row beneath the "Upstream robustness family" heading**, with `status: planned`.
- **Charter source:** `BOOTSTRAP_PROMPT.md` §9 — *"Upstream robustness family — active health checks (HTTP/TCP/gRPC/custom), outlier detection variants, circuit breakers, retries + hedging, **per-protocol connection pooling**."* This phase lands **per-protocol connection pooling** — upstream connection pools for HTTP/1.1 (one TCP connection per in-flight request; idle reuse across requests) and HTTP/2 (one TCP connection multiplexing many concurrent streams) — driven by a per-cluster `circuit_breakers.max_connections` upper bound. Outlier detection, circuit breakers' other thresholds (`max_pending_requests`/`max_requests`/`max_retries`), retries + hedging, and TCP/gRPC/custom active HC checkers all defer per §4 below.
- **Position in the project:** the **fifth post-MVP-trunk feature-family phase** and the **second concrete Upstream-robustness-family phase** (after parent-12 closed with sub-phases `12.1` + `12.2` at commit `3ec7fb9`). The MVP trunk 00→08 + the three HTTP-filter-family phases (09 `local_ratelimit`, 10 `rbac`, 11 `fault`) + parent-12 active HTTP health checking all stand `done`. The 19-Docker-gated-fixture regression baseline established at phase-12.2 close (`0001-tcp-echo` through `0019-upstream-active-health-check`) carries forward unchanged per `BOOTSTRAP_PROMPT.md` §7.5 (b).
- **depends-on:** `04 05 06 12` — phase `04` (the `envoy-http1::Client` upstream H1 origination seam at `crates/envoy-http1/src/client.rs:24,56` + the H1 router-proxy arm at `crates/envoy-http1/src/hcm.rs`) and phase `05` (the `envoy-http2::Client` upstream H2 origination seam at `crates/envoy-http2/src/client.rs:13,75` + the H2 router-proxy arm at `crates/envoy-http2/src/hcm.rs`) are the per-protocol upstream-dispatch seams being pooled. Phase `06` (the `envoy-stats` foundation: `StatsRegistry` + `Counter`/`Gauge` primitives + the `ConnGaugeGuard` RAII pattern at `crates/envoy-cluster/src/cluster.rs:18-26`) is load-bearing for the new pool stats + the `cluster.<name>.upstream_cx_total` semantic tightening. Phase `12` (the new `envoy-health` periodic-background primitive + the `HealthAwareHttp1Backend` synthetic-backend harness primitive at `tests/helpers/health-aware-http1-backend/`) is the structural precedent: the synthetic 5xx backend harness reuses the same shape, and the new pool primitive lives next to the new `envoy-health` crate (same upstream-robustness-family naming + crate-DAG conventions).
- **Brainstorm narrative:** see the "Phase-13 state-1 brainstorm" subsection of `docs/envoy-rust/STATE.md` for the family-pick + feature-pick rationale with alternatives considered along the 5-dimension scoring framework (carryforward closure value / foundation pressure / architectural risk / contract-surface maturity / first-phase scope tractability).

---

## 1. Goal and acceptance signal

Phase 13 lands **per-protocol upstream connection pooling** as the second concrete Upstream-robustness-family feature. When a cluster's `circuit_breakers.thresholds[0].max_connections` field is configured (or under the default upper bound when unconfigured), envoy-rust maintains a **per-cluster, per-endpoint upstream connection pool**:

- For **HTTP/1.1 upstream clusters** (`upstream_protocol = Http1`): a pool of idle keep-alive TCP connections, one in-flight request per connection. The H1 router-proxy arm acquires a connection from the pool on dispatch (creating a new TCP connection if the pool has none idle AND the pool size is under `max_connections`); returns the connection to the pool on response completion (keep-alive); evicts on connection close or idle timeout.
- For **HTTP/2 upstream clusters** (`upstream_protocol = Http2`): a pool of TCP connections each multiplexing many concurrent streams. The H2 router-proxy arm dispatches each request as a new stream on the most-recently-used connection that has remaining stream capacity (`SETTINGS_MAX_CONCURRENT_STREAMS` cap from the upstream's SETTINGS frame); creates a new TCP connection if all existing connections are at stream capacity AND the pool size is under `max_connections`.

The pool **changes the semantics of `cluster.<name>.upstream_cx_total`**: at phase 13 close, the counter increments **once per established upstream TCP connection** (matching upstream Envoy's documented "per-established-connection-from-the-pool" semantic) rather than once per upstream call. `BEHAVIOR_CONTRACT.md`'s disposition for this stat tightens from `name-required, value-may-differ` to **`value-exact`** on the deterministic harness load — the named full-closure site for the 06.3 REVIEW I2 residual.

The phase **also engages and FULLY CLOSES the named carryforward, 06.3 REVIEW I2** (the residual not closed by the 12.2 down-payment):

- **06.3 REVIEW I2** (`docs/envoy-rust/phases/06.3-stats-wiring-and-close/REVIEW.md` §3) named two closure pieces: **(a) wire-level bilateral coverage for per-class `downstream_rq_3xx/4xx/5xx` + `cluster.<name>.upstream_rq_5xx`** (requires a synthetic 5xx backend), and **(b) `cluster.<name>.upstream_cx_total` disposition tightening to `value-exact`** (requires deterministic connection accounting — i.e. pooling). The 12.2 down-payment landed the FIRST synthetic-backend harness primitive (`HealthAwareHttp1Backend`); residual (a) + (b) both stayed open with **connection pooling** as the named owner. **Phase 13 closes (a)** by extending the 12.2 helper into a `Configurable5xxHttp1Backend`-style primitive that emits operator-configured non-2xx statuses per path, and landing a new fixture that exercises 3xx/4xx/5xx + `upstream_rq_5xx` bilaterally. **Phase 13 closes (b)** by introducing pooling + tightening the BEHAVIOR_CONTRACT row. PROGRESS attributes full I2 closure honestly at the task where the contract row tightens.

**Differential surface added by phase 13:**

- **Fixture `0020-upstream-connection-pooling-and-5xx-classes`** (or `0020` + `0021` if the state-2 PLAN-write picks a 2-fixture split) — bilateral assertion that both proxies, given identical bootstraps configuring (a) a pooled H1 upstream cluster + multiple downstream requests + assertion that `cluster.<name>.upstream_cx_total` ends at a deterministic small value (e.g. 1 or 2 connections for N requests), and (b) a synthetic backend that returns 3xx/4xx/5xx on operator-controlled paths + downstream traffic exercising each path + assertion that `downstream_rq_3xx/4xx/5xx` and `cluster.<name>.upstream_rq_5xx` match byte-for-byte across proxies. The discriminating differential observable is the **deterministic small `upstream_cx_total`** (without pooling envoy-rust would emit `N`; with pooling it emits the same small number as upstream Envoy) + the **per-class counter byte-equality**. The exact pool stat names, the H2 SETTINGS interaction, the idle-eviction semantics, and the precise `circuit_breakers.thresholds` shape are **empirically verified at state-2 PLAN-write per §6.2**.

**Acceptance signal (a)–(f), per `BOOTSTRAP_PROMPT.md` §7.5:**

- **(a)** Fixture `0020-upstream-connection-pooling-and-5xx-classes` (or 0020 + 0021) green at Docker-gated CI.
- **(b)** All **19 pre-existing differential fixtures** (`0001-tcp-echo` through `0019-upstream-active-health-check`) **remain green simultaneously** at the same CI run (regression-equivalence per §7.5 (b)). The existing fixtures all assert `upstream_cx_total` under `name-required, value-may-differ`; tightening the disposition does not regress any existing assertion (the assertion-side gets stricter — under deterministic harness load, both proxies still emit the same value). If any existing fixture's deterministic value differs from upstream Envoy's pooled value, EITHER the fixture's expectations are updated to assert the deterministic small value bilaterally OR a per-fixture allow-list exception lands with a rationale (D-3.3 honesty).
- **(c)** `h2spec` continues at ≥95% (parent-05 baseline 99.31%). Phase 13 touches the H2 upstream-client surface (pool integration on the upstream side) but NOT the H2 downstream framing — the state-4 verification re-confirms the gate held; if the H2 pool integration introduces a regression on the upstream-codec side that h2spec catches (e.g. via a re-baselined run that exercises a previously-untouched code path), the regression is addressed in-phase or the gate holds via the codec being unchanged.
- **(d)** `parse_bootstrap` fuzz target clean for the short-budget CI run on the extended corpus (one new seed for the `circuit_breakers` bootstrap shape; corpus extends from 20 to 21 seeds).
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check` all clean.
- **(f)** `REVIEW.md` approved.

A **single CI run** must light up gates (a) through (e) **simultaneously** (continues the project precedent — fixture inheritance is a regression vector).

> **NOTE — likely phase split (see §6.1).** Phase 13's surface (envoy-config `circuit_breakers` schema + validator + H1 pool primitive + H1 router-arm pool integration + H2 pool primitive + H2 router-arm pool integration + pool stats wiring + `cluster.<name>.upstream_cx_total` contract tightening + synthetic-5xx-backend harness primitive + settle-and-scrape differential driver + fixture + fuzz seed + in-process backstop) is projected at **~1800–2400 LoC**, materially over the `BOOTSTRAP_PROMPT.md` §6.1 ~1500-LoC split gate. **The state-2 PLAN-writer is expected to split phase 13 into `13.1` + `13.2`** per §6.1 / §6.2 (recommended seam below). This SPEC covers the whole feature; if the state-2 LoC estimate confirms >~1500, the PLAN-write executes the split (creating `13.1`/`13.2` SPECs + the split ADR), mirroring the parent-12 / parent-08 / parent-07 split cadence.

---

## 2. Behavior-contract scope for phase 13

Phase 13 extends `docs/envoy-rust/BEHAVIOR_CONTRACT.md` with authored additions, landed at the tasks where each is first empirically exercised (per the established 06.x / 07.x / 08.x / 09 / 10 / 11 / 12 doctrine — contract extensions land at empirical-engagement task time, NOT at PLAN-write time and NOT at state-1 SPEC time).

### 2.1 "Stat-name mapping" tightening — `upstream_cx_total` (the 06.3 REVIEW I2 (b) closure)

**Existing row at `BEHAVIOR_CONTRACT.md:89`** (06.1 initial entries):

> `cluster.<name>.upstream_cx_total` — *name-required, value-may-differ.* Counter; one increment per established upstream TCP connection. Envoy's stat semantics are "per-established-connection-from-the-pool" with default connection pooling enabled; envoy-rust under the no-pooling regime (per phase-04.3 / 05.3 posture) increments once per upstream call. Both are correct under their respective contracts. **When connection pooling lands (upstream-robustness family), the disposition tightens to value-exact.**

Phase 13 closes the conditional: the row tightens to **`value-exact`**, with rationale narrating the pool's deterministic per-connection accounting under the harness load. The increment site moves from "once per upstream call" (the existing `crates/envoy-http1/src/router.rs:85-90`-style site + the H2 inline site at `crates/envoy-http2/src/hcm.rs:286-289`) to **"once per established upstream TCP connection at pool create time"** (the new pool primitive). The `5xx` sibling `cluster.<name>.upstream_rq_5xx` stays `value-exact` (already 06.3-tightened on the response-receipt site, not the connect site).

### 2.2 "Stat-name mapping" extension — new pool stats (projected; §6.2-verified)

New rows under the cluster upstream-connection namespace, mirroring upstream Envoy v1.33's documented stat tree:

| Stat name | Equivalence (projected; §6.2-verified) | Rationale |
|---|---|---|
| `cluster.<name>.upstream_cx_destroy` | value-exact | Counter; one increment per pool connection destroyed (peer close, idle timeout, error close). Symmetric to `upstream_cx_total` — under deterministic harness load both proxies destroy the same set of connections at end of test (after the harness's settle window). |
| `cluster.<name>.upstream_cx_destroy_local_with_active_rq` | value-exact (0-case) OR name-required, value-may-differ | Counter; envoy-rust-side connection destroys while a request was outstanding. Under the harness's deterministic flow this stays 0; the dimension hardens once a fixture deliberately exercises a forced-close path. State-2 PLAN-writer empirically confirms exact name + 0-case behavior. |
| `cluster.<name>.upstream_cx_overflow` | value-exact (0-case) | Counter; incremented when an upstream connect attempt is rejected because the pool is at `max_connections`. Default harness load never hits the cap; the dimension hardens when a fixture deliberately exercises overflow (deferred to a follow-up phase per §4 unless the state-2 PLAN-writer folds it in). |
| `cluster.<name>.upstream_cx_http1_total` | value-exact | Counter; one increment per pooled H1 connection established. Sibling of `upstream_cx_total` partitioned by protocol. Both proxies emit. |
| `cluster.<name>.upstream_cx_http2_total` | value-exact | Counter; sibling for H2. |
| `cluster.<name>.upstream_cx_idle_timeout` | name-required, value-may-differ | Counter; incremented when an idle pool connection times out and is destroyed. Timing-dependent (the idle window differs across proxies' wall clocks); value-exact under explicit Timing-tolerances opt-in (NOT taken at phase 13). |
| `cluster.<name>.upstream_rq_pending_overflow` | value-exact (0-case) | Counter; incremented when a request would block waiting for a pool connection but `max_pending_requests` is exceeded — defers with the `max_pending_requests` field per §4 unless the state-2 PLAN-writer pulls it in. Listed here for forward-mapping completeness. |

**Namespace empirical-verification signpost:** the upstream Envoy v1.33 admin `/stats` scrape on a `circuit_breakers`-configured cluster under harness-deterministic load is the authoritative source. **The state-2 PLAN-writer empirically verifies the exact stat names + which counters fire at what site + the per-stat semantics** before locking (per §6.2). The recommended state-1 projection above is provisional — DO NOT ASSUME.

### 2.3 "Per-class downstream_rq stats" — fixture-level bilateral coverage (the 06.3 REVIEW I2 (a) closure)

The existing rows at `BEHAVIOR_CONTRACT.md:96-100` (06.3 entries) define `http.<stat_prefix>.downstream_rq_{2,3,4,5}xx` as `value-exact`. Phase 13 lands fixture 0020 (or 0020 + 0021) that exercises bilateral wire-level coverage for the 3xx / 4xx / 5xx classes — the I2 (a) closure. The fixture pre_requests drive an N-request workload against the synthetic 5xx backend with N divided into per-class buckets (e.g. 1 3xx + 1 4xx + 2 5xx + the rest 2xx, totals chosen for value-exact bilateral assertion). The `expectations.yaml` `value_exact` block populates accordingly.

The `cluster.<name>.upstream_rq_5xx` row at `BEHAVIOR_CONTRACT.md:106` (`value-exact`) is exercised under the new fixture for the first time at the wire level (12.2's fixture 0019 hits the no-healthy-upstream synth-503 path, which doesn't count toward `upstream_rq_5xx` per the synth-vs-proxy bypass).

### 2.4 No DECISIONS.md amendment required at SPEC time

Phase 13 lands no carryforward whose close shape is a *documentation* amendment. The 06.3 REVIEW I2 closure is ordinary deliverable work (building the pool primitives + the synthetic-5xx-backend harness + the fixture + tightening the contract row); the PROGRESS narrative attributes it. **No new ADR is required at SPEC time.** Conditional ADRs (the likely split ADR; the §6.2 empirical-verification revision) are enumerated in §7.

---

## 3. Deliverables

Phase 13's scope is enumerated as deliverables `D1`–`D11` below. **The state-2 PLAN-writer organizes deliverables into tasks AND evaluates the §6.1 split gate** (which is projected to fire — see §6.1). These deliverables are LISTED roughly in execution order but the SPEC is not prescriptive about task organization; only about the surface. If the phase splits, the recommended seam (§6.1) assigns D1/D2/D3/D4/D7-H1/D9-H1 to `13.1` and D5/D6/D7-H2/D7-tighten/D8/D9-5xx/D10 to `13.2`.

### D1 — `envoy-config` schema extension (`Cluster.circuit_breakers.thresholds`)

At `crates/envoy-config/src/bootstrap.rs`, extend the existing `Cluster` struct with a `circuit_breakers` field. The minimum-viable phase-13 surface is `max_connections`; the other circuit-breaker thresholds (`max_pending_requests`/`max_requests`/`max_retries`) defer per §4 (validator rejects via `deny_unknown_fields` OR via an explicit field-presence check, recommended state-1 projection — to be §6.2-verified):

```rust
pub struct Cluster {
    // ... existing fields (name, type, lb_policy, load_assignment, upstream_protocol,
    // health_checks, common_lb_config) ...
    #[serde(default)]
    pub circuit_breakers: Option<CircuitBreakers>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CircuitBreakers {
    #[serde(default)]
    pub thresholds: Vec<Thresholds>,           // OPTIONAL; phase-13 supports exactly 0 or 1
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Thresholds {
    #[serde(default)]
    pub priority: Option<RoutingPriority>,     // OPTIONAL; phase-13 supports DEFAULT only
    #[serde(default)]
    pub max_connections: Option<u32>,          // OPTIONAL; default `1024` per upstream Envoy
    // OPTIONAL — all defer per §4 (validator rejects):
    //   max_pending_requests / max_requests / max_retries / max_connection_pools
    //   track_remaining / retry_budget
}
```

**Important §6.2-verifiable item:** upstream Envoy's `circuit_breakers` field uses a `Thresholds` list keyed by `priority`; the DEFAULT priority is the only one phase 13 supports. Per upstream the `max_connections` default is **1024** (large; not commonly hit under fixture load). The exact shape (`Thresholds` vs flat field; whether `priority` is required) is §6.2-verified.

All struct shapes carry `#[serde(deny_unknown_fields)]` per the established envoy-config discipline (rejects the phase-13-deferred upstream fields enumerated in §4). The phase-13-deferred fields are each enumerated in §4; each is rejected by `deny_unknown_fields`.

### D2 — `envoy-config` validator extension

At the cluster-validation site, add a `validate_circuit_breakers(cluster) -> Result<(), ConfigError>` sub-validator. The validator checks:

- **At most one** `circuit_breakers.thresholds` entry at phase-13 scope. More than one → `ConfigError::UnsupportedMultipleCircuitBreakerThresholds { cluster }`.
- The `priority` (if present) is `DEFAULT` (or `priority` absent — both map to DEFAULT) → `ConfigError::UnsupportedCircuitBreakerPriority { cluster, priority }`.
- `max_connections >= 1` (a 0 value is structurally meaningless — would prevent any upstream connection; reject as `ConfigError::InvalidMaxConnections { cluster, value }`).
- Phase-13-deferred threshold fields (`max_pending_requests`/`max_requests`/`max_retries`) absent — handled by `deny_unknown_fields` automatically OR via an explicit `ConfigError::UnsupportedCircuitBreakerField { cluster, field }` variant if `deny_unknown_fields` is removed (NOT recommended — keep `deny_unknown_fields`).

Roughly **3–4 new `ConfigError` variants** land at this site (the PLAN-writer may consolidate). Each carries `cluster: String` per the established envoy-config error-context discipline. Each has positive + negative parse-path unit tests. The validator is exercised by the existing `parse_bootstrap` fuzz target (the new fixture's bootstrap seeds the corpus per D11).

### D3 — H1 connection pool primitive (the new pool architecture)

The headline new architectural primitive: a per-cluster, per-endpoint H1 connection pool. **Crate-placement / dependency-cycle decision (§5.1; PLAN-write lock-in):**

The pool primitive needs to be reachable from the H1 router-proxy arm (`crates/envoy-http1/src/hcm.rs` proxy arm at the upstream-call site). The natural placement is **inside `envoy-http1`** (the pool owns `Client`/`ClientStream` lifetime; the pool is H1-specific):

```rust
// crates/envoy-http1/src/pool.rs (new file)

pub struct H1Pool {
    cluster_name: String,
    max_connections: u32,
    // Per-endpoint idle-connection lists, protected by a tokio::sync::Mutex
    // (NOT std::sync::Mutex — the pool may be held across .await in acquire())
    idle: tokio::sync::Mutex<HashMap<SocketAddr, Vec<PooledConnection>>>,
    // Total established (idle + in-flight) per endpoint, for max_connections enforcement
    established: tokio::sync::Mutex<HashMap<SocketAddr, u32>>,
    // Stat handles owned by the pool (cluster.<name>.upstream_cx_total etc.)
    cx_total: Arc<envoy_stats::Counter>,
    cx_destroy: Arc<envoy_stats::Counter>,
    cx_http1_total: Arc<envoy_stats::Counter>,
    cx_active: Arc<envoy_stats::Gauge>,
}

pub struct PooledConnection {
    stream: ClientStream,
    // ... last-used timestamp, etc.
}

pub struct PoolGuard<'a> {
    pool: &'a H1Pool,
    endpoint: SocketAddr,
    stream: Option<ClientStream>,   // None after take(), preventing return on Drop
    cx_active_guard: ConnGaugeGuard,
}

impl Drop for PoolGuard<'_> {
    fn drop(&mut self) {
        // Return the stream to the pool's idle list if it's still good (no
        // protocol error AND keep-alive negotiated AND not aborted by caller).
        // The ConnGaugeGuard inside us also fires its Drop -> decrements
        // upstream_cx_active.
    }
}

impl H1Pool {
    /// Acquire a connection to `endpoint`. Returns an existing idle one if any;
    /// otherwise creates a new one if the pool is under max_connections. Returns
    /// a pool-overflow error if the pool is at cap (overflow stat increments).
    pub async fn acquire(&self, endpoint: SocketAddr, host: &str) -> Result<PoolGuard<'_>, PoolError>;

    /// Idle-timeout sweeper (tokio task spawned at pool create). Periodically
    /// scans the idle list and evicts connections past their idle window.
    fn spawn_idle_sweeper(self: Arc<Self>);
}
```

**Important §6.2-verifiable items at PLAN-write:**

- The default upstream Envoy `max_connections` per cluster.
- The default upstream Envoy H1 idle timeout (the wall-clock window an idle pool connection sits before eviction).
- Whether the pool is per-endpoint or per-cluster (Envoy: per-endpoint, as endpoints differ by `SocketAddr`).
- The keep-alive semantics: under what response shapes does envoy-rust return the H1 stream to the pool vs destroy it (Connection: close; Connection: keep-alive; HTTP/1.0 fallback; CL: 0 vs chunked; etc.).

The pool is hand-rolled per **D-3.2** (*"per-protocol connection pooling ... Must be written from scratch"* — implied by *"Filter chain engine ... All load balancing algorithms ... Active health checking, outlier detection, circuit breakers"* doctrine). The implementation uses only **std-lib + tokio + tokio-util + bytes + envoy-cluster + envoy-config + envoy-stats + envoy-http1 internal types** — all D-3.2-permitted; all already pulled. **No new top-level Cargo dep.**

### D4 — H1 router-proxy-arm pool integration

Modify `crates/envoy-http1/src/hcm.rs` proxy-arm dispatch site: replace the current `Client::connect(endpoint, host).await + ClientStream::send_request(req).await` pattern with `H1Pool::acquire(endpoint, host).await + PoolGuard::send_request(req).await`. The `PoolGuard` is held for the request's lifetime; Drop returns the stream to the pool on success or destroys it on protocol error.

`crates/envoy-http1/src/router.rs::write_proxied_response`'s existing `upstream_rq_total` + `upstream_rq_5xx` increment sites are UNCHANGED — they fire on response-receipt regardless of pool / no-pool dispatch path. The `cluster.<name>.upstream_cx_total` increment site MOVES from `router.rs` (which no longer connects) to the pool's `acquire()` connect-on-miss branch.

**Important:** the existing `ConnGaugeGuard` (`crates/envoy-cluster/src/cluster.rs:18-26`) is REUSED: the pool's `PoolGuard` owns one `ConnGaugeGuard` per pool acquire, so the gauge correctly tracks **per-active-request** active connections (matches upstream Envoy's `upstream_cx_active` semantic). On pool return, the `ConnGaugeGuard` drops → gauge decrements; the actual TCP connection sits in the pool's idle list (no `cx_active` count attributed to idle pool members) until next acquire OR eviction.

### D5 — H2 connection pool primitive

Parallel structure to D3, but the H2 pool's semantics differ:

- A pooled H2 connection multiplexes many concurrent streams. The acquire path picks the most-recently-used connection that has remaining stream capacity (under the upstream's `SETTINGS_MAX_CONCURRENT_STREAMS`); if all connections are at capacity AND the pool is under `max_connections`, create a new connection.
- A pooled H2 connection's lifetime ENDS on peer GOAWAY or local error; streams already opened on a GOAWAY-receiving connection finish, but new streams go to a different connection.
- Idle timeout applies to connections with zero active streams (the `h2` crate's `is_open()` / connection-state semantics).

```rust
// crates/envoy-http2/src/pool.rs (new file)

pub struct H2Pool {
    cluster_name: String,
    max_connections: u32,
    // Per-endpoint connection lists with stream-capacity tracking.
    connections: tokio::sync::Mutex<HashMap<SocketAddr, Vec<Arc<H2PoolEntry>>>>,
    // Stat handles
    cx_total: Arc<envoy_stats::Counter>,
    cx_destroy: Arc<envoy_stats::Counter>,
    cx_http2_total: Arc<envoy_stats::Counter>,
    cx_active: Arc<envoy_stats::Gauge>,
}

struct H2PoolEntry {
    client: ClientStream,        // h2-codec connection
    max_streams: u32,            // from peer SETTINGS
    active_streams: AtomicU32,
}
```

**Important §6.2-verifiable items at PLAN-write:**

- Exact upstream Envoy H2 pool behavior on GOAWAY (drain remaining streams vs immediate close).
- Whether per-endpoint H2 pools allow >1 connection by default (Envoy: yes, when SETTINGS_MAX_CONCURRENT_STREAMS is reached).
- The H2 pool's per-stream stat naming (some H2-specific counters live at `cluster.<name>.upstream_cx_http2_total` and similar — §2.2 lists projections).

### D6 — H2 router-proxy-arm pool integration

Modify `crates/envoy-http2/src/hcm.rs` proxy-arm dispatch site analogous to D4. The H2-arm's `send_request` is a per-stream operation that returns when the upstream sends response headers; the `PoolGuard` is held across the response body read; on completion the stream is closed and the connection stays in the pool (decrementing `active_streams`).

The existing inline `upstream_rq_total.inc()` + 5xx-conditional at `crates/envoy-http2/src/hcm.rs:286-289` is UNCHANGED.

### D7 — Pool stat wiring + `cluster.<name>.upstream_cx_total` contract tightening

At pool construct time (the per-cluster pool is built when the cluster is built — `from_bootstrap` is the natural site), register the new counters/gauges against the `Arc<StatsRegistry>`:

- `cluster.<name>.upstream_cx_destroy` (counter; incremented on pool eviction)
- `cluster.<name>.upstream_cx_http1_total` (counter; H1 connects only)
- `cluster.<name>.upstream_cx_http2_total` (counter; H2 connects only)
- `cluster.<name>.upstream_cx_idle_timeout` (counter; idle-sweep evictions)

The existing `cluster.<name>.upstream_cx_total` continues to register at cluster-construct time; the INCREMENT-SITE moves into the pool's `acquire()` connect-on-miss branch.

**D7.1 — BEHAVIOR_CONTRACT row tightening (the I2 (b) closure):** the existing row at `BEHAVIOR_CONTRACT.md:89` for `cluster.<name>.upstream_cx_total` is rewritten from `name-required, value-may-differ` to **`value-exact`**, with rationale: *"Under connection pooling (phase 13), both proxies emit one increment per established upstream TCP connection at pool-create time. Under deterministic harness load (fixed N downstream requests + max_connections cap that allows pool reuse) both proxies establish the same small number of connections."* This landing is the **named full-closure site for 06.3 REVIEW I2 (b)** — PROGRESS attributes it explicitly.

**D7.2 — BEHAVIOR_CONTRACT new rows (the §2.2 projections):** the 6 new pool stat rows land at the task where each is first registered + exercised, per the 06.x→11→12 cadence.

### D8 — Synthetic 5xx backend harness primitive (extends 12.2's HealthAwareHttp1Backend pattern)

A new (or extended) helper crate at `tests/helpers/configurable-http1-backend/` (or extending `tests/helpers/health-aware-http1-backend/`'s pattern). The backend serves operator-configured responses per path: 200 on `/`, configurable non-2xx on `/3xx-path`, `/4xx-path`, `/5xx-path`, etc. Mirrors the 12.2 `HealthAwareHttp1Backend` shape (spawn-based subprocess; settle-then-probe driver; bind-on-port-0 + report-back). **This primitive fully realizes the synthetic-backend infrastructure 06.3 REVIEW I2 named** — the 12.2 down-payment landed the SHAPE; phase 13 lands the configurable per-path-status capability.

The state-2 PLAN-writer decides:
- (a) Extend `HealthAwareHttp1Backend` with a `per_path_status: HashMap<String, u16>` field; OR
- (b) Create a sibling `ConfigurableHttp1Backend` next to it, sharing the spawn/wait infrastructure.

Recommended: (a) — extend, since the 12.2 helper's name `HealthAwareHttp1Backend` already captures the "per-path-status" capability conceptually (the health-aware aspect IS per-path-status; the per-class extension is its natural generalization). If the rename complicates the 12.2 fixture, (b) keeps backwards-compat.

### D9 — Fixture(s) + Docker wrapper(s) + in-process backstop

- **D9.1 — Fixture `tests/fixtures/0020-upstream-connection-pooling-and-5xx-classes/`** (state-2 PLAN-writer may split into two: `0020-upstream-connection-pooling` + `0021-per-class-counters`). The fixture configures: a cluster with `circuit_breakers.thresholds[0].max_connections: 4` (or smaller — empirically tuned at PLAN-write); the synthetic 5xx backend as the cluster's endpoint; a downstream HCM listener routing `/` → 200-backend-path, `/3xx` → 3xx, `/4xx` → 4xx, `/5xx` → 5xx; a pre_requests sequence driving N requests with a per-class distribution. After settle, the assertions check `cluster.<name>.upstream_cx_total = <small N>` (the I2 (b) closure surface; the small-N bilateral value is the discriminating differential observable) + `http.<stat_prefix>.downstream_rq_{2,3,4,5}xx` per the per-class distribution + `cluster.<name>.upstream_rq_5xx` (the I2 (a) closure surface).
- **D9.2 — `tests/differential/tests/upstream_connection_pooling.rs`** (and possibly `per_class_counters.rs`) Docker-gated wrapper mirroring the 12.2 shape.
- **D9.3 — In-process backstop at `crates/envoy-bin/tests/upstream_connection_pooling.rs`**, mirroring the 12.2 `upstream_active_health_check.rs` shape. Boots `envoy-bin` with a synthesized bootstrap + in-process `ConfigurableHttp1Backend`; drives the per-class load; asserts the deterministic pool stats + per-class counter values. Per the 10/11/12.2 lesson, include the 5-standard-header presence assertion on any synth response (none expected in this fixture, but the discipline carries forward).

### D10 — Settle-and-scrape differential driver extension

The fixture's pre_requests + post-request settle + admin `/stats` scrape pattern was first established at 06.1 (fixture 0011's Prometheus exposition). Phase 13's fixture extends it with a **multi-request pre_requests sequence** (the per-class distribution requires multiple distinct downstream requests). The harness's existing `pre_requests` + `Driver::AdminScrape` patterns are reused; no new harness primitive is projected unless the state-2 PLAN-writer surfaces a need.

### D11 — Fuzz corpus seed

New file `crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_circuit_breakers.yaml` containing the `circuit_breakers` bootstrap shape. Mirrors the 12.2 corpus-seed precedent. Extends the seed coverage 20 → 21, with the `crates/envoy-config/fuzz/.gitignore` allow-list extension AND the `bootstrap.rs::tests::fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS-array extension (both files edited together, per the 09/10/11/12.2 Task-6/Task-7 lesson).

---

## 4. Out of scope (deferred non-goals)

Phase 13 explicitly does NOT land:

- **`max_pending_requests` / `max_requests` / `max_retries` / `max_connection_pools` circuit-breaker thresholds.** Phase 13 supports only `max_connections`. Validator rejects via `deny_unknown_fields`. Defer.
- **Non-DEFAULT `RoutingPriority` thresholds.** Phase 13 supports the single DEFAULT priority. Multi-priority circuit breaking defers.
- **Retries + hedging** (`retry_policy`, `num_retries`, `retry_on`, `per_try_timeout`). Inline request-path enhancement; differential observability is largely stats-based. Defers.
- **Outlier detection** (passive health). Distinct subsystem (passive vs the 12-active health). Defers to a follow-up upstream-robustness phase. (It would reuse the same `EndpointHealth` ejection seam landed at 12.1.)
- **TCP / gRPC / custom active HC checkers.** Stays deferred per phase 12.
- **`max_requests_per_connection` (Envoy field).** Phase-13 H1 pools follow keep-alive indefinitely until idle-timeout / peer-close. Defer.
- **Pool-overflow request queueing.** When the pool is at `max_connections` AND no idle connections, phase 13 returns an error (the `RouterError` path renders synth-503 or similar; exact behavior is §6.2-verified). Defers the queueing-until-spare-arrives semantic.
- **Pool warm-up / pre-connect on cluster create.** Phase 13 lazily creates connections on first request. Defer.
- **Pool over TLS upstream.** Already supported transparently because the H1/H2 client's `connect()` accepts a `transport_socket` configuration; the pool acquires-then-returns `ClientStream` (already TLS-aware). The TLS handshake cost is amortized across pooled requests — this is a natural emergent benefit, not a new feature.
- **Per-stream H2 graceful drain.** The H2 pool's `Drop` on a connection waits for active streams to complete via the existing `h2` codec's `is_open()` API. The fine-grained GOAWAY handling (sending GOAWAY on local pool close) defers.

---

## 5. Architectural invariants

Phase 13 honors and extends the established cross-crate invariants:

### 5.1 Crate boundaries + the dependency-cycle constraint (load-bearing)

- **H1 pool lives in `envoy-http1`** (it owns `ClientStream` lifetime; H1-specific).
- **H2 pool lives in `envoy-http2`** (it owns `h2::client::SendRequest` / equivalent lifetime; H2-specific).
- **The cluster's pool handles** are owned by the cluster (built at `ClusterManager::from_bootstrap`); they hold `Arc<H1Pool>` and `Arc<H2Pool>` per-cluster. This means `envoy-cluster` MUST get a typed reference to the pool primitives — but `envoy-cluster` cannot depend on `envoy-http1`/`envoy-http2` (today there's no such dep; adding one would invert the existing dep direction). **The cycle-avoidance pattern (§5.1 in the parent-12 SPEC; ADR-0028 / ADR-0031 lineage) MUST be honored.**

**Recommended pattern (PLAN-write lock-in):** a `ClusterHandle::set_h1_pool(Arc<dyn ClientPool>)` / `set_h2_pool` injection at `envoy-bin` startup, OR each pool primitive lives behind a trait declared in `envoy-cluster`. The state-2 PLAN-writer picks. The current pool-related stat counters already living on `Cluster` (`cx_total`, `cx_active`, `upstream_rq_total`, `upstream_rq_5xx`) stay there; the pool primitives stay in their respective protocol crates. **No new top-level Cargo dep; no cycle.**

### 5.2 Hand-rolled per D-3.2

Connection pooling is hand-rolled per **D-3.2**'s scratch-mandate (the upstream-robustness family is on the must-be-written-from-scratch list). The implementation uses only **std-lib + tokio + tokio-util + bytes + envoy-cluster + envoy-config + envoy-stats + the existing envoy-http1/envoy-http2 internal types** — all D-3.2-permitted; all already pulled. **No new top-level Cargo dep.**

**Explicit non-grants:** no `tower`-style pool abstraction. No `deadpool` / `bb8` / `mobc` connection-pool crate. No `dashmap` (use `tokio::sync::Mutex` over `HashMap` — the locking cost is acceptable; the harness load is low). None on D-3.2's permitted-foundations list beyond what is already pulled.

### 5.3 No new top-level Cargo deps

The recommended no-foundations-grants posture carries forward. Pool primitives reuse existing internal types. **If the state-3 implementer surfaces a genuine external-crate need (e.g. `dashmap` for performance reasons), a foundations-grant ADR lands per D-3.5 — see §7.**

### 5.4 Pooling is opt-in but default-enabled (regression-equivalence with care)

The pool runs **whether or not `circuit_breakers` is configured** — the default `max_connections` is large (`1024` per upstream Envoy projection; §6.2-verified). When `circuit_breakers` is absent, the pool uses defaults; the resulting `upstream_cx_total` count is the deterministic small N (pool reuse), NOT the prior-art "once per call" N.

**Regression-equivalence (acceptance gate (b)) requires checking each existing fixture's `upstream_cx_total` expectation:** the 19 existing fixtures should each tolerate the tightened value-exact disposition. Per the BEHAVIOR_CONTRACT.md row 89 (the `name-required, value-may-differ` disposition), no existing fixture asserts a specific `upstream_cx_total` value — they assert presence only (which still holds). **State-2 PLAN-writer empirically confirms** by inspecting each existing fixture's `expectations.yaml` for `upstream_cx_total` references; if any fixture's existing assertion shape requires update, the update lands inside phase 13.

**Alternative posture (state-2 PLAN-writer decision):** the pool could be guarded by `circuit_breakers` presence (no `circuit_breakers` configured → no pool; one-connection-per-call as today). This preserves bit-for-bit existing fixture behavior. **Recommended: default-pool-enabled** (matches upstream Envoy and is the eventual destination); but the PLAN-writer may pick guarded-pool if §6.2 verification surfaces specific upstream-Envoy nuances that complicate default-enabled.

### 5.5 The pool is the second periodic-background primitive

Phase 13 introduces the project's **second periodic timer-driven background task** (after 12.2's `envoy-health` probe loop): the per-pool idle-connection sweeper. Mirrors the `envoy-health` `Scheduler::spawn` topology — one tokio task per pool, holding a `tokio::time::interval` over the idle-timeout window; on each tick, walk the idle list + evict past-deadline entries. The state-3 implementer ensures graceful cancellation (no leaked tasks on cluster destroy; tests assert task shutdown).

### 5.6 The pre-built `ConnGaugeGuard` RAII pattern is reused

The existing `crates/envoy-cluster/src/cluster.rs:18-26` `ConnGaugeGuard` (06.3 D15.3.b RAII pattern) is reused: each `PoolGuard` (H1 or H2 pool's per-acquire handle) owns one `ConnGaugeGuard`. The gauge correctly counts active **borrowed** connections; the pool's idle list does NOT hold guards (idle connections don't count toward `upstream_cx_active`). Drop ordering: when `PoolGuard` drops, the `ConnGaugeGuard` inside it drops → `cx_active.dec()`. The actual TCP stream MAY return to the pool's idle list (the `PoolGuard::Drop` impl orchestrates this).

### 5.7 The harness extends — no new driver type needed (most likely)

Phase 13's fixture pre_requests + admin /stats scrape pattern reuses the 06.1 driver shape. If the state-2 PLAN-writer surfaces a need for a "multi-request with bookkeeping" driver, it lands as a small extension to `Driver` rather than a new type. **No new differential driver is projected.**

---

## 6. Implementation signposts for the planner

The state-2 PLAN-writer reads this section to drive PLAN structure.

### 6.1 Split-gate evaluation (READ FIRST — split projected to fire)

Per `BOOTSTRAP_PROMPT.md` §6.1, the state-2 PLAN-write evaluates whether the PLAN exceeds ~25 numbered tasks OR ~1500 LoC. Phase 13's surface estimate at SPEC time:

- D1 — envoy-config schema (`CircuitBreakers` + `Thresholds`) (~80 LoC + ~100 LoC tests).
- D2 — envoy-config validator (3–4 ConfigError variants) (~70 LoC + ~110 LoC tests).
- D3 — H1 pool primitive + idle sweeper (~280 LoC + ~250 LoC tests).
- D4 — H1 router-arm pool integration (~80 LoC modify + ~140 LoC tests).
- D5 — H2 pool primitive + idle sweeper (~250 LoC + ~220 LoC tests).
- D6 — H2 router-arm pool integration (~80 LoC modify + ~120 LoC tests).
- D7 — pool stats wiring + BEHAVIOR_CONTRACT row tightening + new rows (~80 LoC + ~80 LoC tests + ~40 LoC docs).
- D8 — synthetic 5xx backend harness primitive (~150 LoC + ~100 LoC tests).
- D9.1 — fixture 0020 (or 0020 + 0021) + harness wrapper (~180 LoC YAML/wrapper).
- D9.2 — Docker-gated `differential` test wrapper(s) (~40 LoC).
- D9.3 — in-process backstop (~250 LoC).
- D10 — settle-and-scrape driver extension (~30 LoC if any).
- D11 — fuzz seed (~30 LoC + 2 file edits).
- State-4 verification + STATE-advance (~docs).

**SPEC-time projection: ~16–20 tasks; ~1800–2400 LoC** (production ~970, tests ~1020, fixture/harness/backend ~470). **This is materially OVER the §6.1 ~1500-LoC gate** (task count is approaching ~25). **Recommended posture: SPLIT into `13.1` + `13.2`** at state-2 PLAN-write per §6.2:

- **`13.1` — H1 pool + envoy-config schema + H1 router wiring + fixture 0020 (H1-only).** D1 (schema) + D2 (validator) + D3 (H1 pool primitive) + D4 (H1 router integration) + D7-H1 (H1 pool stats wiring; `upstream_cx_http1_total` etc.) + D8 (configurable backend extension) + D9.1-H1 (fixture 0020 H1-only) + D9.3-H1 (in-process backstop H1-only) + D11 (fuzz seed). **No H2 pool yet.** Existing H2 cluster fixture `0010` stays under the existing "one-conn-per-call" non-pooled path; the BEHAVIOR_CONTRACT row 89 tightening defers to 13.2 (where both protocols pool). ~1000–1200 LoC. Single new fixture; per-class counter bilateral coverage lands here.
- **`13.2` — H2 pool + I2 (b) closure + BEHAVIOR_CONTRACT tightening + parent-13 close.** D5 (H2 pool primitive) + D6 (H2 router integration) + D7.1 (the `upstream_cx_total` row tightening to `value-exact` — fires NOW that both protocols pool) + D7.2 (any remaining new pool stat rows) + D9.1-H2 (fixture 0020 extension OR fixture 0021 for the H2 pool surface) + D9.3-H2 (in-process backstop H2-only) + parent-13 close. ~900–1100 LoC. The headline I2 (b) closure surface (the `upstream_cx_total` row tightening) lands here, as both H1 and H2 must pool before the contract row can tighten across all clusters.

**Split ADR:** if the split fires, an ADR (`ADR-0038`) lands at the state-2 PLAN-write commit per `BOOTSTRAP_PROMPT.md` §6.2 step 6, mirroring ADR-0036 (parent-12 split). The parent-13 ROADMAP row flips `planned → in-progress` with `sub-phases: 13.1, 13.2`; each sub-phase gets its own row (`planned`) + its own `SPEC.md`.

**Alternative split (state-2 PLAN-writer may pick):** `13.1` lands H1+H2 pool foundations + envoy-config schema + NO new fixture (regression-equivalence via existing 19 fixtures staying green — the 05.1 / 07.1 / 12.1 foundation-slice pattern); `13.2` lands router-arm pool integration + new fixture(s) + per-class counter coverage + parent-13 close. This alternative has the architectural benefit that the foundation is fully built before being wired (mirrors 12.1 → 12.2 cleanly), but the cost that the H1 router and the H2 router wire INSIDE 13.2 — a busier sub-phase. The H1-only/H2-only split (recommended above) keeps each sub-phase narrower per-protocol; the foundation-slice split keeps the foundation-first cadence.

**Recommended: split (H1-only / H2-only seam).** State-2 PLAN-writer picks the exact seam.

**If the state-2 LoC estimate comes in at-or-under ~1500** (e.g. the PLAN-writer finds a leaner H2-pool path), single-phase is permitted — but the SPEC-time projection strongly expects the split.

### 6.2 Empirical verification at state-2 PLAN-write (HEAVY for this phase)

Per the phase-10/11/12-ratified verify-at-PLAN-write process improvement: **the state-2 PLAN-writer empirically verifies the upstream wire/behavior shapes BEFORE locking PLAN lock-ins.** Phase 13 has an unusually LARGE empirically-discoverable surface — run `envoyproxy/envoy:v1.33.0` Docker with a `circuit_breakers`-configured cluster + the configurable backend + admin `/stats`, and verify:

1. **Exact `circuit_breakers.thresholds` shape:** field hierarchy, `priority` requirement (default DEFAULT? required field?), the `max_connections` default value (1024? 1000?), and any deny_unknown_fields-relevant phase-13-deferred fields.
2. **H1 pool default behavior:** under no `circuit_breakers` config, does upstream Envoy pool by default? The default H1 `max_connections` per cluster.
3. **H1 idle timeout:** the upstream-default keep-alive idle window. Whether this knob is `idle_timeout` (HCM) vs a cluster-level field.
4. **`upstream_cx_total` increment semantics:** is the count per pool-create OR per established connection (which might be the same)? Confirm under fixture: drive 5 requests through a pooled cluster, scrape, value should be `1` or `2` (small), not `5`.
5. **`upstream_cx_destroy` + sibling counters:** name + when they fire + whether they're per-cluster or per-endpoint.
6. **H2 pool default behavior:** default `max_concurrent_streams` + multi-connection-per-endpoint thresholds.
7. **Per-class HCM counters under the synthetic-5xx-backend fixture:** byte-equal values for `http.<stat_prefix>.downstream_rq_{2,3,4,5}xx` after driving N requests with per-class distribution.
8. **`cluster.<name>.upstream_rq_5xx` under the synthetic-5xx-backend fixture:** byte-equal value matching the number of 5xx-receiving downstream requests.
9. **HCM filter-synth bypass on per-class counters:** the 06.3 BEHAVIOR_CONTRACT note about "symmetric on response_status_for_log, agnostic to synth-vs-proxy origin" is preserved.

Each finding lands as a PLAN lock-in. **If any finding differs materially from the SPEC projection, the lock-in records the divergence + the SPEC §X.Y revision via an inline ADR at the state-2 PLAN-write commit** (mirrors the phase-10 ADR-0034 / phase-12 ADR-0037 precedents). Given the split is also projected (§6.1), the empirical-revision ADR (if it fires) would be `ADR-0039` (the split ADR takes `ADR-0038`); at most one ADR per commit per D-3.5 — if both fire at the PLAN-write commit, they take consecutive numbers across the lock-in narrative.

### 6.3 The 06.3 REVIEW I2 closure (the headline carryforward closure)

Phase 13 fully closes 06.3 REVIEW I2 — both (a) per-class wire coverage AND (b) `upstream_cx_total` tightening. The PROGRESS narrative explicitly states full closure at the task where the BEHAVIOR_CONTRACT row tightens AND at the task where the per-class fixture lands; the closure ends the open 06.3 I2 carryforward in the standing inventory. The PLAN-writer reads the 06.3 REVIEW.md §3 I2 entry + §8 R-track item 4 via direct spot-check before writing D7.1 + D9.1.

### 6.4 In-process backstop assertions (heeds the phase-10 M1 / phase-12 §6.4 lesson)

D9.3 SHOULD exercise the deterministic pool stat values (small `upstream_cx_total` after N requests) AND the per-class counter values, AND include the per-probe standard-header presence assertion on any synth response, OR explicitly disclose any omission in PROGRESS. Recommended: include both convergence directions where applicable.

### 6.5 The 06.x stats convention

StatsRegistry registration at pool-construct time (which is cluster-construct time); per-pool ownership of the Counter/Gauge handles; the `upstream_cx_total` counter incremented at pool's `acquire()` connect-on-miss branch (one source of truth — the per-call site at `router.rs:85-90` is REMOVED for the H1 path, INCREMENT moves to pool; symmetric for H2). The PLAN-writer ensures the increment-site migration is atomic per protocol (no double-counting).

### 6.6 The BEHAVIOR_CONTRACT extension cadence

Contract extensions land at the TASK where each is first empirically exercised, NOT at PLAN-write and NOT at SPEC time. The `upstream_cx_total` row tightening at D7.1 fires at the task where the pool's `acquire()` connect-on-miss branch is in place + the fixture's deterministic value-exact bilateral assertion lands.

### 6.7 Pre-state-4 fmt discipline (continues per 06.1 R-9)

Per-task PROGRESS sections quote `cargo fmt --all -- --check` at every PROGRESS-task close, NOT just at state-4.

### 6.8 State-4 evidence-discipline (continues per 05.3 → … → 12.2 chain)

Per-gate quoted evidence in PROGRESS at the state-4 verification task: real CI run URL + HEAD SHA + completion timestamp + per-gate quoted output (5 stable-toolchain gates + each Docker-gated fixture + h2spec_pass_rate_gate + parse_bootstrap fuzz iteration count). Phase 13 touches the H2 upstream-client surface — the state-4 verification confirms h2spec ≥95% held (no upstream-codec regression).

### 6.9 Cargo.lock cadence

The phase-04.1 REVIEW M5/M9 Cargo.lock-cadence ADR carries forward. Phase 13 adds zero new top-level Cargo deps; the new pool modules are internal to existing crates (no new workspace member unless the state-2 PLAN-writer pulls one out — recommended `pool.rs` modules INSIDE `envoy-http1`/`envoy-http2`).

### 6.10 PLAN.md + PROGRESS.md skeleton + Task 1 preamble land alongside at state-2

Per the 06.2 / … / 12.2 cadence. State-2 PLAN-write lands `PLAN.md` (or, on split, the `13.1` PLAN) + `PROGRESS.md` skeleton + Task 1 preamble in a single standalone pre-Task-1 commit.

### 6.11 Subagent-driven execution at state 3 (per `feedback_execution_style`)

The user's standing preference auto-memory `feedback_execution_style` ("default to subagent-driven-development; skip the two-option fork") applies at state 3. The state-2 PLAN-write organizes tasks for subagent-driven execution per the 06.x / … / 12.2 cadence (each task independent enough to dispatch in isolation; PROGRESS attestation per-task; in-phase recovery cadence if any task surfaces a code-quality-review-blocking finding). Subagents claiming "same pattern as previous phase" verify the precedent shape via direct code-spot-check before the claim lands in PROGRESS.

### 6.12 12.2 Minor carryforward opportunistic closures

The 12.2 REVIEW landed 11 active Minor carryforwards (A-M2, A-M4, B-M1..B-M6, C-M1, C-M2, C-M4). Phase 13 touches `envoy-http1` (the H1 pool primitive + H1 router-arm integration) and `envoy-http2` (the H2 pool primitive + H2 router-arm integration) and `tests/helpers/` (the configurable-backend extension or sibling); any of these surface areas may close 12.2 Minor items opportunistically. State-2 PLAN-writer reads the 12.2 REVIEW.md §4 carryforward table + assigns close-opportunities per task. None gates phase 13; all are awareness-only.

---

## 7. ADR projection

**Recommended posture at state-1: NO new ADRs land at THIS (state-1 brainstorm) commit.** The DECISIONS.md ledger head stays at **ADR-0037** through phase 13's state-1; the next-available number is **ADR-0038**.

Conditional ADR slots, reserved for state-2 / state-3 landing:

- **Conditional ADR-0038 (option A — phase split). LIKELY TO FIRE.** Per §6.1, the LoC projection (~1800–2400) is over the §6.1 gate; the state-2 PLAN-write is expected to split phase 13 into `13.1` + `13.2`, landing `ADR-0038: split phase 13 into 13.1–13.2 because plan exceeded ~1500 LoC` per `BOOTSTRAP_PROMPT.md` §6.2 step 6. **Recommended posture: split; land ADR-0038 at the PLAN-write commit.**

- **Conditional ADR-0039 (option B — §6.2 empirical-verification revision). PLAUSIBLE.** If any of the 9 §6.2 items (esp. the `circuit_breakers` shape or the `upstream_cx_total` default-pool behavior) differs materially from this SPEC's projection, an inline ADR lands at the state-2 PLAN-write commit recording the divergence + the SPEC revision. Numbered `ADR-0039` if the split ADR took `ADR-0038`. **Recommended posture: verify all 9; land the revision ADR if any differ.**

- **Conditional ADR (option C — pool cycle-resolution). PLAUSIBLE.** If §5.1's pool-handle injection pattern (`ClusterHandle::set_h1_pool` etc.) ends up taking a non-obvious shape (e.g. requires a new trait declared in `envoy-cluster` for cycle-avoidance), an ADR records the choice mirroring ADR-0028 / ADR-0031. **Recommended posture: NO ADR unless the resolution is non-obvious** — a straight injection through an `Option<Arc<dyn ClientPool>>` field is ordinary structure.

- **Conditional ADR (option D — foundations grant).** No external-crate grant projected (§5.2/§5.3). If state-3 surfaces a genuine need (e.g. `dashmap` for performance), an ADR lands at the surfacing task. **Recommended: no grant.**

At most ONE ADR lands per commit (per D-3.5 sequential numbering). If none fire, the ledger stays at ADR-0037 through phase 13 (unlikely, given the split projection).

---

## 8. State-machine signposts for the phase-13 state-2 session

The next session (state 2) reads this section and acts.

- **Lifecycle state at session start:** State 2 (SPEC.md exists; PLAN.md does not).
- **Skill:** `superpowers:writing-plans` per `BOOTSTRAP_PROMPT.md` §5 state 2.
- **Output:** `PLAN.md` + `PROGRESS.md` skeleton + Task 1 preamble (standalone pre-Task-1 commit per the 04.3 / 05.1 / 06.x / 07.x / 08.x / 09 / 10 / 11 / 12.x cadence). **If the split fires (§6.1, recommended): create `docs/envoy-rust/phases/13.1-<subtitle>/SPEC.md` + `13.2-<subtitle>/SPEC.md`, update ROADMAP (parent 13 → `in-progress` + `sub-phases: 13.1, 13.2`; two new sub-phase rows `planned`), update STATE → `13.1`, land `ADR-0038` (split), and STOP** — the next session starts `13.1` at state 1/2 per §6.2 step 7.
- **Empirical verification at state 2 (per §6.2 — HEAVY):** verify all 9 items against `envoyproxy/envoy:v1.33.0` before locking. Land any empirical-revision ADR inline.
- **Split-gate evaluation:** §6.1 above. **Recommended: SPLIT into 13.1 + 13.2** (H1-only / H2-only seam recommended; foundation-slice alternative documented).
- **The 06.3 REVIEW I2 closure (D7.1 + D9.1):** §6.3 above — full closure (both (a) per-class wire coverage AND (b) `upstream_cx_total` tightening). Read the 06.3 REVIEW.md §3 I2 + §8 R-track item 4 via direct spot-check.
- **The pool cycle-resolution decision (§5.1):** recommended trait-or-Arc injection through `ClusterHandle`; record as a PLAN lock-in.
- **PLAN-time SPEC corrections:** the PLAN-writer reads this SPEC against HEAD `<state-1-commit-SHA>` and flags any drift (the exact `Cluster` struct fields + line; the exact `envoy-http1::Client::connect` signature at `client.rs:33`; the exact `envoy-http2::Client::connect` at `client.rs:19`; the exact `ConnGaugeGuard` at `cluster.rs:18-26`; the existing `upstream_cx_total` increment-site call paths at `router.rs:85-90` (H1) + `hcm.rs:286-289` (H2); the 12.2 helper at `tests/helpers/health-aware-http1-backend/`). Per the 06.2 → … → 12.2 "N PLAN-write SPEC corrections" pattern, corrections land in the PROGRESS Task 1 preamble.

---

## 9. Commit message format (for state 6 of the phase-13 lifecycle)

If phase 13 stays single (NOT recommended; see §6.1):

```
phase 13: per-protocol connection pooling (H1 + H2) + 06.3 REVIEW I2 full closure + fixture 0020 [ADR-0038, ...]

<1-3 sentence summary>

Differential surface: fixture 0020-upstream-connection-pooling-and-5xx-classes; all 20 Docker-gated fixtures (0001-0020) green simultaneously at CI run <ID> HEAD <SHA>.
Conformance: h2spec ≥95% gate held at parent-05 baseline (H2 upstream-client pool integration verified to not regress the codec gate).
```

If phase 13 splits (recommended), the closing sub-phase (`13.2`) commit carries `[parent 13 done]` per the 02.2 / 03.2 / 07.2 / 08.2 / 12.2 closing-sub-phase precedent, and each sub-phase commits per the §5.3 format with its own ADR bracket as applicable.

---

## 10. State-machine commit (this commit — phase-13 state-1 close-out)

This SPEC is the state-1 output. The state-1 close-out commit is **docs-only** and touches:

- **CREATE** `docs/envoy-rust/phases/13-connection-pooling/SPEC.md` (this file).
- **MODIFY** `docs/envoy-rust/ROADMAP.md` — adds a new row to the existing "Upstream robustness family" §9 table beneath the existing parent-12 + 12.1 + 12.2 rows. Row content:
  ```
  | 13 | per-protocol upstream connection pooling (H1 + H2) + `cluster.<name>.upstream_cx_total` tightening to value-exact + synthetic 5xx backend + fixture 0020 (fully closes 06.3 REVIEW I2) | 04 05 06 12 | planned | — | fixture 0020-upstream-connection-pooling-and-5xx-classes green; envoy-http1 + envoy-http2 each gain a per-cluster/per-endpoint connection pool (H1 idle-keep-alive list; H2 multiplexed-streams pool); router-arm dispatch routes through pool acquire/release rather than per-call connect; cluster.<name>.upstream_cx_total disposition tightens to value-exact (fully closes 06.3 REVIEW I2 (b)); per-class downstream_rq_3xx/4xx/5xx + cluster.<name>.upstream_rq_5xx wire bilateral coverage via fixture 0020 (fully closes 06.3 REVIEW I2 (a)); envoy-config gains Cluster.circuit_breakers schema (max_connections; phase-13 supports DEFAULT priority only) + ~3-4 ConfigError variants; new pool stats cluster.<name>.upstream_cx_destroy + upstream_cx_http1_total + upstream_cx_http2_total + upstream_cx_idle_timeout; configurable-status synthetic-backend harness primitive extends the 12.2 HealthAwareHttp1Backend pattern; LIKELY SPLIT into 13.1 + 13.2 at state-2 PLAN-write per ~1800-2400 LoC estimate |
  ```
  The "Upstream robustness family" heading + the parent-12 + 12.1 + 12.2 rows stay unchanged; the new row joins beneath them per `BOOTSTRAP_PROMPT.md` §4.1 invariant 2 (append-only; never delete rows). All other ROADMAP rows + family headings untouched; the schema header untouched.
- **MODIFY** `docs/envoy-rust/STATE.md` — advances "Active phase" pointer from `_none_ — awaiting next planning` to:
  - `id: 13`
  - `slug: 13-connection-pooling`
  - `directory: docs/envoy-rust/phases/13-connection-pooling/`
  - `status: phase 13 lifecycle state 1-complete / state-2-next (SPEC.md landed; PLAN.md does not exist; SPLIT into 13.1 + 13.2 projected at state-2)`

  Rewrites "Next expected skill" to `superpowers:writing-plans` scoped to this SPEC. Rewrites "Last commit" + "Last updated". Appends a new `### Phase-13 state-1 brainstorm` subsection in Notes recording the family-pick + feature-pick rationale + alternatives along the 5-dimension scoring + the split projection + the I2 full-closure projection + the ADR projection. Preserves all prior subsections verbatim per D-3.5 (append-only) + D-3.4 (context isolation) — including the `### Phase-12.2 rollovers` + all `### Phase-12.x state-*` + `### Phase-12 state-*` subsections.

No code changes, no fixture changes, no Cargo.toml changes, no DECISIONS.md changes (ledger head stays **ADR-0037**), no BEHAVIOR_CONTRACT.md changes. ENVOY_TARGET.md + rust-toolchain.toml untouched (D-3.7 / D-3.9 unchanged).

**Commit message:**

```
phase 13: state-1 brainstorm — connection-pooling SPEC.md (Upstream robustness family second phase; H1+H2 connection pooling; fully closes 06.3 REVIEW I2)
```

Per the project precedent (phase-12 state-1 brainstorm commit `edd654c` title shape — descriptive title with a parenthesized scope summary). No `[ADR-NNNN]` brackets — no ADR lands at this commit.

**Predecessor:** `3ec7fb9` — phase 12.2 state-6 close-out (the most-recent commit; docs-only closing-sub-phase close-out that flipped parent-12 + 12.2 rows to done).

**Origin/main:** `3ec7fb9`. Local + origin are in sync as of THIS state-1 brainstorm commit's prologue. After landing, the docs-only edits push to origin and the next CI run re-validates the docs-only edits compile cleanly through the 5 stable-toolchain gates + the `parse_bootstrap` fuzz target on the unchanged 20-seed corpus (predecessor docs-only CI runs took ~2-3m).

---

*End of SPEC. Phase 13 state-1 lifecycle complete on landing. The next session enters state 2 — writes PLAN.md per `superpowers:writing-plans`, performs the §6.2 empirical verification at PLAN-write (`circuit_breakers` shape + `upstream_cx_total` pooled semantics + H1+H2 default pool behavior + per-class HCM counter byte-equality under synthetic 5xx backend + 9 items total against `envoyproxy/envoy:v1.33.0`), and evaluates the §6.1 split gate (SPLIT into 13.1 + 13.2 recommended; land ADR-0038).*
