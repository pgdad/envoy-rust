# Phase 13.2 (`13.2-h2-pool-and-cx-total-tightening`) — SPEC

- **Phase id:** `13.2`
- **Slug:** `13.2-h2-pool-and-cx-total-tightening`
- **Parent:** `13` (`13-connection-pooling`). The **second + closing sub-phase** of the phase-13 split (parent-13 state-2 PLAN-write; ADR-0038). 13.2's state-6 commit flips parent ROADMAP row `13` `in-progress → done` per the closing-sub-phase invariant (the 02.2 / 03.2 / 07.2 / 08.2 / 12.2 precedent). The full feature narrative lives in `docs/envoy-rust/phases/13-connection-pooling/SPEC.md`; the H1 foundation it builds atop lives in `docs/envoy-rust/phases/13.1-h1-pool-and-fixture/SPEC.md`.
- **depends-on:** `13.1` (strict). 13.2's H2 pool mirrors the architectural shape that 13.1 lands for H1 (the cycle-resolution pattern, the `PoolGuard`/`ConnGaugeGuard` RAII discipline, the `cx_*` stats wiring conventions). 13.2's BEHAVIOR_CONTRACT row tightening can only fire AFTER 13.1's H1 pool is in place — otherwise the row would tighten globally while only one protocol pools (the H2 cluster fixture 0010 would still emit per-call accounting, falsifying the tightened row).
- **Status before this SPEC lands:** `planned` (added as a sub-phase row at the parent-13 state-2 split commit).

---

## 1. Goal and acceptance signal

Phase 13.2 lands the **H2 connection-pool primitive + the H2 router-proxy-arm pool integration + the `cluster.<name>.upstream_cx_total` BEHAVIOR_CONTRACT row tightening to `value-exact` + the new H2-specific pool stat rows + the H2-surface fixture + the in-process H2 backstop + the parent-13 close** — the second half + the contract-surface deliverable of phase-13 connection pooling. After 13.2:

- `envoy-http2` carries a per-cluster, per-endpoint **`H2Pool`** primitive (a pool of TCP connections each multiplexing many concurrent H2 streams) with a `PoolGuard` RAII handle owning one `ConnGaugeGuard` per acquire. The H2 router-proxy arm dispatches through `H2Pool::acquire()` rather than per-call `Client::connect()`.
- envoy-rust's `BEHAVIOR_CONTRACT.md` `cluster.<name>.upstream_cx_total` row tightens from `name-required, value-may-differ` to **`value-exact`** under the harness's single-downstream-keep-alive-conn driver (carving out TCP-proxy clusters, which stay at `name-required, value-may-differ` until TCP pooling lands in a follow-up phase). **This is the named full-closure site for 06.3 REVIEW I2 (b).**
- A new differential observable lands at fixture 0020 (extended) OR a sibling fixture 0021: an H2 cluster with a `circuit_breakers`-configured `max_connections` cap + a multi-request workload over a single downstream H1 keep-alive conn → `cluster.<name>.upstream_cx_total: 1` bilaterally (one H2 upstream connection multiplexing N streams).
- An in-process H2 backstop exercises H2 pool reuse + the multiplexed-streams accounting on the H2 path.
- **Parent-13 ROADMAP row flips `done`** at the 13.2 state-6 close-out commit (the closing-sub-phase invariant).

**13.2 ENGAGES and FULLY CLOSES 06.3 REVIEW I2 (b)** (the `cluster.<name>.upstream_cx_total` value-exact tightening; the I2 residual paired with the I2 (a) per-class wire coverage that 13.1 closes). With both (a) + (b) closed at the phase-13 lifecycle, **06.3 REVIEW I2 is FULLY CLOSED** at the close of parent-13.

**Acceptance signal (a)–(f), per `BOOTSTRAP_PROMPT.md` §7.5:**

- **(a)** Fixture 0020 (H2 pool extension OR sibling fixture 0021) green at Docker-gated CI. (State-2 PLAN-writer decides: extend fixture 0020 with an H2 cluster + an H2-pool assertion, OR create a sibling fixture 0021-h2-upstream-connection-pooling that mirrors 0020's shape for the H2 case. **Recommended: sibling fixture 0021** — separates the H1-pool surface from the H2-pool surface cleanly + keeps each fixture's expectations focused.)
- **(b)** All **20 pre-existing differential fixtures** (`0001-tcp-echo` through `0020-upstream-connection-pooling-and-per-class-counters` from 13.1) remain green simultaneously at the same Docker-gated CI run. The H2 pool's default-enabled posture must not regress the H2 cluster fixture `0010` (and any other H2-touching fixture). Per the same §5.4 reasoning as 13.1, no existing fixture asserts `upstream_cx_total` value-exact (all use the pre-13.1 presence-only disposition; 13.1 didn't tighten the row; 13.2's tightening lands here + the new fixture is the only one exercising the tightened disposition value-exact).
- **(c)** `h2spec` continues at ≥95% (parent-05 baseline 99.31%). 13.2 touches the H2 upstream-client surface (pool integration on the upstream side) but NOT the H2 downstream framing — the state-4 verification re-confirms the gate held; the h2spec runs against envoy-rust's H2 listener (downstream), which is independent of the H2 upstream-client pool.
- **(d)** `parse_bootstrap` fuzz target clean for the short-budget CI run on the existing 21-seed corpus (the `circuit_breakers` seed landed at 13.1; 13.2 adds no new corpus seed unless the H2 fixture surfaces a new bootstrap shape worth seeding — PLAN-writer's call).
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check` all clean.
- **(f)** `REVIEW.md` approved.

A **single CI run** lights gates (a)–(e) simultaneously. **13.2's state-6 commit closes parent-13** (flips ROADMAP rows `13.2` AND `13` `in-progress → done` SIMULTANEOUSLY per the closing-sub-phase invariant).

---

## 2. Empirical findings inherited from the parent-13 state-2 verification (locked facts)

The parent-13 state-2 PLAN-write performed the parent SPEC §6.2 HEAVY 9-item verification against `envoyproxy/envoy:v1.33.0` (full table in STATE.md `### Phase-13 state-2 split decision` + 13.1 SPEC §2). The findings that bind **13.2**:

- **(item vi — the H2 finding)** **H2 pool default behavior:** default `max_concurrent_streams` honors peer SETTINGS frame (no envoy-side limit by default; Envoy-as-client uses the server's SETTINGS_MAX_CONCURRENT_STREAMS cap, default 100 per RFC 7540 §6.5.2 if peer doesn't send a SETTINGS frame). Per-endpoint multi-connection threshold: Envoy spawns a 2nd upstream H2 connection only when the 1st is at peer's MAX_CONCURRENT_STREAMS cap (or at the cluster's `circuit_breakers.max_connections` cap, whichever is lower). Stat namespace includes `cluster.<name>.upstream_cx_http2_total` (the H2-specific connect counter). The legacy cluster-field `http2_protocol_options` stays absent in /config_dump when `typed_extension_protocol_options` is used (Envoy's modern path). **MATCHES the parent SPEC's recommended projection.** → D5 (H2 pool primitive design).

- **(items i, iv inherited from 13.1)** the `circuit_breakers.thresholds[0].max_connections` schema + the discriminating differential observable shape (single-downstream-keep-alive-conn driver → `upstream_cx_total: 1` for N requests) apply identically to the H2 pool case. The harness's `Driver::Http1KeepAlive` from 13.1 D10 drives an H1 downstream conn against an H2 upstream cluster — the downstream is H1 (the harness's standard mode), but upstream is H2 (the cluster's `typed_extension_protocol_options.http2_protocol_options`). The H2 pool reuses one upstream H2 connection across the N HCM dispatches, each as a separate H2 stream.

- **(items vii, viii inherited from 13.1)** the per-class HCM + cluster counter bilateral byte-equality applies symmetrically to the H2 upstream case (cluster `upstream_rq_{2,3,4,5}xx` increments per upstream H2 response regardless of protocol). 13.1 fixture 0020 exercises this for H1 upstream; 13.2 fixture 0021 (or extension) exercises it for H2 upstream — providing bilateral coverage across both upstream protocols.

- **(item ix inherited from 13.1)** HCM filter-synth bypass on per-class counters preserved unchanged on the H2 path: synth-503 INCREMENTS HCM `downstream_rq_5xx` but does NOT increment cluster `upstream_rq_5xx` (synth bypasses upstream regardless of upstream protocol).

**§6.2 synthesis: ALL 9 items MATCHED the parent-13 SPEC's projections** (confirmed at the parent-13 state-2 split commit; the 13.2 PLAN-writer does NOT re-run Docker).

---

## 3. Deliverables

13.2 carries parent-13 deliverables **D5, D6, D7.1 (the BEHAVIOR_CONTRACT row tightening), D7.2 (new H2 pool stat rows), D9.1-H2 (the new H2-pool fixture), D9.3-H2 (in-process H2 backstop), parent-13 close**. The state-2 PLAN-writer for 13.2 organizes these into TDD tasks for subagent-driven execution.

### D5 — H2 connection-pool primitive (`crates/envoy-http2/src/pool.rs`)

A new file at `crates/envoy-http2/src/pool.rs` mirroring the 13.1 D3 H1 pool's architectural shape (the cycle-resolution pattern, the `PoolGuard`/`ConnGaugeGuard` RAII discipline, the stats wiring conventions), with H2-specific semantics:

```rust
//! 13.2 D5: per-cluster H2 connection pool. Holds TCP connections each
//! multiplexing many concurrent H2 streams; `acquire()` returns a handle
//! to the most-recently-used connection with remaining stream capacity
//! (subject to peer's SETTINGS_MAX_CONCURRENT_STREAMS and the cluster's
//! max_connections cap).

pub struct H2Pool {
    cluster_name: String,
    max_connections: u32,
    idle_timeout: std::time::Duration,
    // Per-endpoint connection list with stream-capacity tracking.
    connections: tokio::sync::Mutex<HashMap<SocketAddr, Vec<Arc<H2PoolEntry>>>>,
    // Stat handles owned by the pool.
    cx_total: Arc<envoy_stats::Counter>,        // shared with cluster cx_total
    cx_destroy: Arc<envoy_stats::Counter>,
    cx_http2_total: Arc<envoy_stats::Counter>,
    cx_active: Arc<envoy_stats::Gauge>,         // shared with cluster cx_active
}

struct H2PoolEntry {
    // The h2-codec connection handle; cloneable per h2's SendRequest API.
    client_stream: ClientStream,
    // Peer's SETTINGS_MAX_CONCURRENT_STREAMS (or RFC 7540 §6.5.2 default 100
    // if peer hasn't sent SETTINGS yet). Read once at handshake completion.
    max_streams: u32,
    // Active streams (live H2 stream count). Atomic for concurrent acquire().
    active_streams: AtomicU32,
    // Last-stream-completion timestamp (for idle eviction).
    last_idle: parking_lot::Mutex<Option<std::time::Instant>>,
}

pub struct PoolGuard {
    pool: Arc<H2Pool>,
    endpoint: SocketAddr,
    entry: Arc<H2PoolEntry>,
    cx_active_guard: ConnGaugeGuard,  // gauge decrements on drop
    // Stream stayed-good vs. errored — set by send_request consumer.
    invalidated: bool,
}

impl PoolGuard {
    pub fn client_stream_mut(&mut self) -> &mut ClientStream { /* via Arc */ }
    /// Mark the connection as un-pooled (e.g., on GOAWAY or transport error).
    /// Drop will destroy rather than return-to-pool.
    pub fn invalidate(&mut self) { self.invalidated = true; }
}

impl Drop for PoolGuard {
    fn drop(&mut self) {
        // Decrement active_streams; if invalidated, also evict the entry from
        // the pool's connections map (cx_destroy increments).
        self.entry.active_streams.fetch_sub(1, Ordering::Relaxed);
        if self.invalidated {
            let pool = Arc::clone(&self.pool);
            let endpoint = self.endpoint;
            let entry = Arc::clone(&self.entry);
            tokio::spawn(async move {
                let mut conns = pool.connections.lock().await;
                if let Some(list) = conns.get_mut(&endpoint) {
                    list.retain(|e| !Arc::ptr_eq(e, &entry));
                }
                pool.cx_destroy.inc();
            });
        } else if self.entry.active_streams.load(Ordering::Relaxed) == 0 {
            // Last stream completed — record idle timestamp for the sweeper.
            *self.entry.last_idle.lock() = Some(std::time::Instant::now());
        }
        // ConnGaugeGuard's Drop fires → upstream_cx_active decrements.
    }
}

impl H2Pool {
    pub fn new(cluster_name: String, max_connections: u32, /* stat handles */) -> Arc<Self>;

    /// Acquire a stream slot on an existing connection if any has capacity;
    /// otherwise creates a new H2 connection (subject to max_connections cap).
    pub async fn acquire(self: &Arc<Self>, endpoint: SocketAddr, host: &str)
        -> Result<PoolGuard, PoolError>;

    /// Idle-timeout sweeper (mirrors 13.1 D3).
    pub fn spawn_idle_sweeper(self: &Arc<Self>) -> tokio::task::JoinHandle<()>;
}
```

**ClientStream visibility:** the existing `ClientStream` at `crates/envoy-http2/src/client.rs:75` has **PRIVATE** fields (`send_request: h2::client::SendRequest<Bytes>` and `host: String` — NOT `pub(crate)`). The new sibling `pool.rs` module cannot access these directly. **13.2 PLAN-writer's call (recommended):** widen the two fields to `pub(crate)` (a 1-line visibility change; mirrors the H1 ClientStream's existing `pub(crate)` posture). Alternative: add `pub(crate) fn clone_send_request(&self) -> SendRequest<Bytes>` accessor (`h2::client::SendRequest` is `Clone`). **Recommended: visibility widening** (uniform with H1; no new accessor to maintain).

**Cycle-resolution decision (§5.1):** mirrors 13.1's bin-wired injection pattern. `envoy-cluster`'s `Cluster` holds `Arc<H2Pool>` injected at `envoy-bin` startup. **No new trait in `envoy-cluster`; no new top-level Cargo dep; no cycle.**

**Hand-rolled per D-3.2:** same as 13.1 — std + tokio + tokio-util + bytes + envoy-stats + the existing envoy-http2 internal types (including `h2::client::SendRequest` access). No new top-level Cargo dep.

**Multi-connection semantics:** when `acquire()` is called and (a) all existing connections are at `min(peer SETTINGS_MAX_CONCURRENT_STREAMS, cluster's max_connections-derived cap)` AND (b) `established_count < max_connections`, the pool spawns a new H2 connection via `Client::connect()` and adds it to the per-endpoint list. The PLAN-writer empirically pins (at fixture-build time, against the configurable backend) the exact peer-SETTINGS-vs-cluster-cap interaction.

**GOAWAY handling:** on receipt of GOAWAY from peer, the connection is marked `invalidated` for future acquires (existing streams finish; new streams go to a different connection). At phase-13 scope, the fine-grained "send GOAWAY on local pool close" defers per parent §4.

**Idle sweeper:** mirrors 13.1's. A single tokio task per H2Pool, holding `tokio::time::interval(idle_timeout / 4)`; each tick walks the per-endpoint connection lists, evicts entries where `last_idle.elapsed() > idle_timeout && active_streams == 0`.

**Unit tests:** acquire-from-empty creates connection; acquire-from-non-empty-with-capacity reuses; acquire-when-all-at-cap creates 2nd connection (if under max_connections) OR returns Overflow (if at cap); active_streams tracking through concurrent acquires; invalidate prevents reuse; idle sweeper evicts only when active_streams == 0; cx_total/cx_destroy/cx_http2_total/cx_active fire at the right sites.

### D6 — H2 router-proxy-arm pool integration

Modify `crates/envoy-http2/src/hcm.rs` proxy-arm dispatch sites (`hcm.rs:280` AND `hcm.rs:291` — confirmed at parent-13 SPEC's §8 + verified at 13.1 state-2 SPEC corrections via direct grep). The two existing `Client::connect(...)` + per-call `cx_total.inc()` sites become `cluster.h2_pool().acquire(endpoint, host).await?` + `pool_guard.client_stream_mut().send_request(req).await?`. The `cx_total` increment-site MIGRATES from `hcm.rs:280` + `:291` into the pool's `acquire()` connect-on-miss branch (one source of truth — mirrors 13.1's H1 migration from `hcm.rs:514`).

The router-proxy-arm's existing inline `upstream_rq_total` + `upstream_rq_5xx` increments stay UNCHANGED (response-receipt-driven; pool-agnostic).

**The `ConnGaugeGuard` (`crates/envoy-cluster/src/cluster.rs:18-26`) is REUSED:** each `PoolGuard` owns one `ConnGaugeGuard`; the gauge counts borrowed-for-active-stream connections.

### D7.1 — `cluster.<name>.upstream_cx_total` BEHAVIOR_CONTRACT row tightening (the 06.3 REVIEW I2 (b) closure)

The existing row at `BEHAVIOR_CONTRACT.md:89` (06.1 initial entry):

> `cluster.<name>.upstream_cx_total` — *name-required, value-may-differ.* Counter; one increment per established upstream TCP connection. Envoy's stat semantics are "per-established-connection-from-the-pool" with default connection pooling enabled; envoy-rust under the no-pooling regime (per phase-04.3 / 05.3 posture) increments once per upstream call. Both are correct under their respective contracts. **When connection pooling lands (upstream-robustness family), the disposition tightens to value-exact.**

**13.2 D7.1 closes the conditional**, rewriting the row to:

> `cluster.<name>.upstream_cx_total` — *value-exact (H1 + H2 clusters under the harness's single-downstream-keep-alive-conn driver); name-required, value-may-differ (TCP-proxy clusters — TCP pool defers to a follow-up phase).* Counter; one increment per established upstream TCP connection at pool-create time. Under H1/H2 pooling (phase 13), both proxies emit the same small N under deterministic load (1 if the workload fits in one pooled connection; more if the harness exceeds max_concurrent_streams or max_connections, in which case both proxies still emit identical N because the cap is bilaterally configured). The increment site lives in the H1/H2 pool's `acquire()` connect-on-miss branch (one source of truth per protocol). The TCP-proxy increment at `crates/envoy-tcp/src/lib.rs:108` remains per-call until TCP pooling lands; existing TCP fixtures (`0001/0003/0004/0005/0006`) carry the pre-13.2 name-required, value-may-differ disposition (their `expectations.yaml` assertion is presence-only, so the tightened value-exact disposition is satisfied trivially on the H1/H2 side under the same presence-only assertion).

This is the **named full-closure site for 06.3 REVIEW I2 (b)** — PROGRESS attributes it explicitly at the D7.1 task.

The contract row narrative also documents the §6.2 item-(iv) discriminating-observable nuance: the value-exact disposition is **conditional on the harness driver issuing multiple requests over a single downstream keep-alive conn** (else N upstream conns per N downstream conns, regardless of pool — see 13.1 §2 item-iv). Fixture 0021 (or extension to 0020) uses `Driver::Http1KeepAlive` (the 13.1-landed driver variant) for this reason.

### D7.2 — New H2 pool stat rows (BEHAVIOR_CONTRACT extensions)

At pool construct time (per-cluster, in `from_bootstrap`), register against the `Arc<StatsRegistry>`:

- `cluster.<name>.upstream_cx_http2_total` (counter; one increment per H2 pool connect-on-miss)
- Optionally: the H2 pool's sibling counters reuse the H1 pool's `cluster.<name>.upstream_cx_destroy` (a generic destroy counter at the cluster level), so this counter doesn't need re-registration at 13.2.

**BEHAVIOR_CONTRACT.md** — at the D7.2 task:
- `cluster.<name>.upstream_cx_http2_total` row lands. Disposition: `value-exact` (one increment per H2 pool connect; under the fixture's single-downstream-keep-alive-conn driver issuing N HCM-dispatches → both proxies emit 1, mirroring the H1 `upstream_cx_http1_total`).

### D9.1-H2 — Fixture 0021 (recommended: sibling fixture) OR fixture 0020 H2 extension

**Recommended posture: sibling fixture `tests/fixtures/0021-upstream-h2-connection-pooling/`** mirroring 0020's shape exactly with two differences: (1) the cluster carries `typed_extension_protocol_options.envoy.extensions.upstreams.http.v3.HttpProtocolOptions.explicit_http_config.http2_protocol_options: {}` (H2 upstream); (2) the backend is an H2C backend (envoy-rust's existing `http2-echo-server` helper at `tests/helpers/http2-echo-server/` is the natural candidate — but the H2C backend's per-path-status capability would need to mirror the 13.1 D8 `health-aware-http1-backend`'s extension OR the fixture uses the existing echo backend with a simpler workload that doesn't exercise per-class counters).

**13.2 PLAN-writer's call (PLAN-time lock-in):** decide between:
- **(a) sibling fixture 0021 (recommended)** — clean separation of H1-pool and H2-pool surfaces; simpler `expectations.yaml`; the workload can be simpler (just upstream_cx_total assertion + upstream_cx_http2_total assertion); the existing `http2-echo-server` works as backend without per-path-status extension (just upstream_cx_total + a small request workload + reach assertion).
- **(b) extend fixture 0020** — keeps both protocols in one fixture; complex YAML (two clusters? one with H1 + one with H2? + per-protocol assertions); harder to read.

**Recommended: (a) sibling fixture 0021** + **the existing `http2-echo-server` helper** (no per-path-status extension needed for 13.2's narrower scope). Workload: N (e.g. 5) sequential requests over a single downstream H1 keep-alive conn (the 13.1-landed `Driver::Http1KeepAlive`); assertions: `cluster.<name>.upstream_cx_total: 1`, `cluster.<name>.upstream_cx_http2_total: 1`, `cluster.<name>.upstream_rq_total: 5`, `http.ingress_http.downstream_rq_2xx: 5`, `http.ingress_http.downstream_rq_total: 5`. (The per-class counter coverage is already exhaustively asserted at fixture 0020; 13.2's fixture focuses on the H2 pool surface specifically.)

**Docker-gated wrapper:** `tests/differential/tests/upstream_h2_connection_pooling.rs` mirroring the 13.1 shape.

### D9.3-H2 — In-process H2 backstop

New file at `crates/envoy-bin/tests/upstream_h2_connection_pooling.rs` mirroring the 13.1 D9.3 shape, with the 09 REVIEW M3 subprocess discipline. Boots `envoy-bin` with a synthesized H2-cluster bootstrap + an in-process H2C backend (the existing `http2-echo-server` helper); exercises:
- The pool-reuse direction: 5 sequential GET / over a single downstream H1 keep-alive conn through an H2 upstream cluster → `upstream_cx_total: 1`, `upstream_cx_http2_total: 1`, `upstream_rq_total: 5` (deterministic small N).
- The active-streams-tracking direction: optionally drive 2 concurrent requests over the same downstream conn (HCM dispatches concurrently as two H2 streams on the same upstream conn; `active_streams` peak = 2; `upstream_cx_total` stays 1).
- Per the 10 REVIEW M1 / 12.2 / 13.1 D9.3 lesson, **include the 5-standard-header presence assertion** on any non-2xx response (none expected in this fixture, but discipline carries forward).

### parent-13 close

13.2's state-6 commit MUST flip ROADMAP rows `13.2` AND parent `13` `in-progress → done` SIMULTANEOUSLY (the closing-sub-phase invariant — mirrors the 02.2 / 03.2 / 07.2 / 08.2 / 12.2 closing-sub-phase precedent). The commit title carries the `[parent 13 done]` tag per the precedent. PROGRESS attributes the full 06.3 REVIEW I2 closure (both (a) from 13.1 + (b) from 13.2 D7.1).

---

## 4. Out of scope for 13.2 (defers per parent SPEC §4)

All of the parent-13 SPEC §4 deferral list carries forward unchanged: `max_pending_requests` / `max_requests` / `max_retries` / `max_connection_pools` circuit-breaker thresholds; non-DEFAULT routing priorities; retries + hedging; outlier detection; TCP/gRPC/custom active HC checkers; `max_requests_per_connection`; pool-overflow request queueing; pool warm-up; per-stream H2 graceful drain (the fine-grained GOAWAY-on-local-close); pool over TLS upstream (transparent — the H1/H2 client's `connect()` accepts TLS configuration already); the `idle_timeout` envoy-config knob.

Additionally:
- **TCP-proxy connection pooling** continues to defer — the `crates/envoy-tcp/src/lib.rs:108` per-call `cx_total.inc()` site stays untouched. The 13.2 D7.1 BEHAVIOR_CONTRACT row tightening explicitly carves out TCP clusters from `value-exact`.
- **H3/QUIC connection pooling** defers per the parent-13 SPEC's HTTP/3 family scoping (the `upstream_cx_http3_total` stat namespace exists in upstream Envoy v1.33 but is not wired at envoy-rust until the HTTP/3 family phase).

---

## 5. Architectural invariants (inherited from parent SPEC §5)

- **§5.1 the dependency-cycle constraint:** mirrors 13.1's resolution. The H2 pool lives in `envoy-http2` (owns `ClientStream`/`SendRequest` lifetime; H2-specific). `envoy-cluster`'s `Cluster` holds `Arc<H2Pool>` injected at `envoy-bin` startup via the bin-wired pattern (the 13.1 cycle-resolution pattern carries forward). No new trait declared in `envoy-cluster`; no new top-level Cargo dep.
- **§5.2 hand-rolled per D-3.2:** the H2 pool primitive + the multi-connection-per-endpoint logic + the active_streams tracking + the idle sweeper are written from scratch atop std + tokio + tokio-util + bytes + envoy-stats + the existing envoy-http2 internal types (including `h2::client::SendRequest` direct access). No new top-level Cargo dep.
- **§5.3 no new top-level Cargo dep:** verified at PLAN-write. 13.2 PLAN-writer confirms before locking.
- **§5.4 default-enabled pool:** the H2 pool runs with hardcoded defaults (`max_connections: 1024`; `idle_timeout: 60 s`; `max_concurrent_streams`: peer's SETTINGS or RFC default 100) when `circuit_breakers` is absent. The 1 existing H2-touching fixture `0010-http2-router-upstream` (`circuit_breakers` not configured) pools transparently. Regression-equivalence guaranteed by `0010`'s presence-only `upstream_cx_total` assertion (and any other counter that becomes affected).
- **§5.5 the H2 pool's idle sweeper is the THIRD periodic-background primitive** (after 12.2's `envoy-health` probe loop + 13.1's H1 pool's idle sweeper). Same cancellation discipline.
- **§5.6 the `ConnGaugeGuard` RAII pattern is REUSED uniformly across H1 + H2.** The gauge counts the SUM of borrowed H1 connections + borrowed H2 connections (each H2 PoolGuard counts 1, regardless of how many streams the connection multiplexes — matches upstream Envoy's `upstream_cx_active` semantic of "active connections", NOT "active streams"). The 13.2 PLAN-writer empirically pins this against the §6.2 item-(vi) verification at fixture-build time.

---

## 6. Signposts for the 13.2 state-2 PLAN-writer

- The empirical §6.2 verification is **done** (parent-13 state-2; §2 above + STATE.md `### Phase-13 state-2 split decision`). **The 13.2 PLAN-writer does NOT re-run Docker** for the 9 items — they are locked facts. (A 13.2-specific implementation question — e.g. the exact upstream-Envoy GOAWAY-during-pool-acquire behavior — is verified against the code/docs at task time.)
- **PLAN-time SPEC corrections** (read this SPEC against HEAD at 13.2-state-2-start; the parent SPEC + 13.1 state-2 SPEC corrections have already verified):
  - `envoy-http2::Client::connect` at `crates/envoy-http2/src/client.rs:19` — confirmed signature.
  - `ClientStream` at `client.rs:75` — fields are **PRIVATE** (not `pub(crate)` like H1); 13.2 D5 widens to `pub(crate)` (recommended) OR adds a `pub(crate)` accessor.
  - H2 `upstream_cx_total` increment sites at `crates/envoy-http2/src/hcm.rs:280` + `:291` — confirmed at 13.1 state-2 via direct grep.
  - The 13.1-landed `H1Pool` at `crates/envoy-http1/src/pool.rs` is the architectural sibling — 13.2 mirrors its shape.
  - The 13.1-landed `Driver::Http1KeepAlive` at `tests/differential/src/lib.rs` — reused by fixture 0021.
  - The 13.1-landed `Cluster.h1_pool` field at `crates/envoy-cluster/src/cluster.rs` (or wherever 13.1 placed it) — 13.2 adds the parallel `Cluster.h2_pool` field.
  - The existing `http2-echo-server` helper at `tests/helpers/http2-echo-server/` — confirmed exists; usable as the fixture 0021 backend.
  - Corrections land in the PROGRESS Task 1 preamble.
- **The fixture-vs-extension decision (D9.1-H2):** recommended posture is sibling fixture 0021. 13.2 PLAN-writer confirms before locking; if the recommendation surfaces an unexpected complication (e.g. the `http2-echo-server` helper lacks a needed capability for the workload), the alternative (extend 0020) lands.
- **Subagent-driven execution at state 3** per `feedback_execution_style`. Suggested task organization: Task 1 — H2 pool primitive + idle sweeper + unit tests (D5); Task 2 — H2 router-arm pool integration (D6); Task 3 — H2 pool stats wiring + `upstream_cx_http2_total` BEHAVIOR_CONTRACT row (D7.2); Task 4 — `upstream_cx_total` BEHAVIOR_CONTRACT row tightening (D7.1) + PROGRESS attribution of the 06.3 REVIEW I2 (b) full-closure; Task 5 — fixture 0021 + Docker wrapper (D9.1-H2); Task 6 — in-process H2 backstop (D9.3-H2); Task 7 — state-4 verification (20+1 fixtures green + 5 gates + h2spec ≥95% + fuzz on 21-seed corpus); Task 8 — state-6 close-out commit (parent-13 close).
- **Carryforward closure attributed at 13.2:** **06.3 REVIEW I2 (b)** — FULLY CLOSED at Task 4 (the BEHAVIOR_CONTRACT row tightening). Combined with 13.1's **06.3 REVIEW I2 (a)** closure, **the full 06.3 REVIEW I2 carryforward is CLOSED at the parent-13 close.** PROGRESS at 13.2 Task 4 + Task 8 attributes the full closure honestly; the standing carryforward inventory drops I2.
- **Other carryforward dispositions:** other inventory items (12.2 REVIEW Minor carryforwards; 12.1 / phase-11 / earlier-phase residuals) carry forward unchanged; 13.2 may close opportunistically when its named seams overlap.

---

## 7. ADR projection for 13.2

**Recommended posture: NO new ADR lands during the 13.2 lifecycle.** ADR-0038 (split) lands at the parent-13 state-2 split commit (before 13.1 + 13.2 begin). 13.2 introduces no new crate (the H2 pool is a new module INSIDE `envoy-http2`), no foundations grant, no wire-level contract revision (the contract row tightening at D7.1 is the projected I2 (b) closure — ordinary deliverable work, NOT a new ADR-worthy decision since the conditional was already documented in the row's original 06.1 narrative and the parent-13 SPEC §2). DECISIONS.md ledger head is **ADR-0038** at 13.2 start; next available **ADR-0039**. A 13.2 ADR lands only if execution surfaces a genuine ambiguity (e.g., a non-obvious GOAWAY-handling decision OR a multi-connection-threshold decision warranting durable record — neither projected).

---

## 8. Commit message format (for state 6 of the 13.2 lifecycle — parent-13 close)

```
phase 13.2: H2 connection pool + upstream_cx_total tightening to value-exact + fixture 0021 + parent-13 close (06.3 REVIEW I2 FULLY CLOSED) [parent 13 done]

<1-3 sentence summary; names the I2 (b) full-closure + the full 06.3 REVIEW I2 closure at parent-13 close>

Differential surface: fixture 0021-upstream-h2-connection-pooling; all 21 Docker-gated fixtures (0001-0021) green simultaneously at CI run <ID> HEAD <SHA>.
Conformance: h2spec ≥95% gate held at parent-05 baseline (H2 upstream-client surface pool-integrated without codec regression).
```

*(No `[ADR-NNNN]` bracket unless a 13.2 ADR lands per §7. The `[parent 13 done]` tag flips ROADMAP rows `13.2` AND `13` to `done` simultaneously — the closing-sub-phase invariant per the 02.2 / 03.2 / 07.2 / 08.2 / 12.2 precedent.)*

---

*End of 13.2 SPEC. The H2 connection pool + the `cluster.<name>.upstream_cx_total` BEHAVIOR_CONTRACT row tightening (the 06.3 REVIEW I2 (b) full-closure site) + the parent-13 close all land here. With this commit, parent-13 closes + the 06.3 REVIEW I2 carryforward is fully closed + the project's per-protocol connection pooling primitive is in place for both H1 and H2.*
