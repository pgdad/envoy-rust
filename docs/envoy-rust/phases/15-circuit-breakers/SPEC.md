# Phase 15 (`15-circuit-breakers`) — SPEC

- **Phase id:** `15`
- **Slug:** `15-circuit-breakers`
- **Status before this SPEC lands:** _not yet in ROADMAP.md_ (per `docs/envoy-rust/ROADMAP.md` at HEAD `b575bdc35`, the phase-14.2 state-6 closing-sub-phase close-out commit; the "Upstream robustness family" §9 table at that HEAD carries parent rows `12`/`13`/`14` + sub-rows `12.1`/`12.2`/`13.1`/`13.2`/`14.1`/`14.2`, all `status: done` — no row exists yet for circuit breakers). **This SPEC's landing commit adds the FOURTH concrete row beneath the "Upstream robustness family" heading**, with `status: planned`.
- **Charter source:** `BOOTSTRAP_PROMPT.md` §9 — *"Upstream robustness family — active health checks (HTTP/TCP/gRPC/custom), outlier detection variants, **circuit breakers**, retries + hedging, per-protocol connection pooling."* This phase lands the **observable behavior of the `max_connections` circuit breaker** — the overflow-rejection wire shape + the circuit-breaker observability stat subset + the first differential fixture that actually trips the cap. **The `max_connections` ENFORCEMENT itself already landed at phase 13** (see the critical scoping finding below + ADR-0042); phase 15 closes the OBSERVABILITY + DIFFERENTIAL-COVERAGE gap around it. Other circuit-breaker thresholds (`max_pending_requests > 0` request queueing, `max_requests`, `max_retries`, `max_connection_pools`, `track_remaining`, `retry_budget`), non-DEFAULT priority, and retries+hedging all defer per §4.
- **Position in the project:** the **seventh post-MVP-trunk feature-family phase** and the **fourth concrete Upstream-robustness-family phase** (after parent-12 active HTTP health checking closed at `3ec7fb9`, parent-13 connection pooling closed at `96630f9`, and parent-14 outlier detection closed at `b575bdc35`). The MVP trunk 00→08 + the three HTTP-filter-family phases (09 `local_ratelimit`, 10 `rbac`, 11 `fault`) + parent-12/13/14 all stand `done`. The **22-Docker-gated-fixture regression baseline** established at phase-14.2 close (`0001-tcp-echo` through `0022-upstream-outlier-detection-consecutive-5xx`) carries forward unchanged per `BOOTSTRAP_PROMPT.md` §7.5 (b).
- **depends-on:** `04 05 06 13` — phase `04` (the `envoy-http1` H1 router-proxy arm at `crates/envoy-http1/src/hcm.rs` whose `PoolError::Overflow` arm at `:542` renders the synth-503) and phase `05` (the `envoy-http2` H2 router-proxy arm at `crates/envoy-http2/src/hcm.rs:368-380` whose `PoolError::Overflow` arm renders a local 503) are the overflow-rejection seams being made observable + differentially tested. Phase `06` (the `envoy-stats` foundation: `StatsRegistry` + `Counter`/`Gauge` primitives) is load-bearing for the new circuit-breaker stat subset. Phase `13` is the **direct foundation**: it landed (a) the `Cluster.circuit_breakers` schema (`crates/envoy-config/src/bootstrap.rs:1170-1196`), (b) the per-endpoint `established`-count cap enforcement in `H1Pool::acquire` / `H2Pool::acquire` (`crates/envoy-http1/src/pool.rs:200-211`; `crates/envoy-http2/src/pool.rs:304-314`), (c) the `max_connections`-sourcing in `H1PoolManager::for_bootstrap` / `H2PoolManager::for_bootstrap` (`pool.rs:358-363` / `:507-512`), and (d) the `PoolError::Overflow` → synth-503 router-arm mappings. Phase 15 wires observability + a differential fixture ON TOP of these existing seams.
- **Brainstorm narrative:** see the "Phase-15 state-1 brainstorm" subsection of `docs/envoy-rust/STATE.md` for the family-pick + feature-pick rationale, the critical "enforcement-already-landed-at-13" finding that reshaped the phase from *implement-enforcement* to *observability + differential-coverage*, and the alternatives weighed (TCP active health check; TCP-proxy `upstream_cx_total` carve-out closure; retries; outlier-detection success-rate variant). The scoping decision is ratified in **ADR-0042** (landed at this brainstorm commit).

---

## 0. Critical scoping finding (READ FIRST) — `max_connections` enforcement already landed at phase 13

The phase-15 charter line in `BOOTSTRAP_PROMPT.md` §9 ("circuit breakers") and the prior planning-boundary note (STATE.md at HEAD `b575bdc35`) framed circuit breakers as *"max_connections enforcement — currently parsed but NOT enforced."* **This framing is FALSE at HEAD `b575bdc35`.** The state-1 brainstorm verified the code directly:

- **The cap is enforced.** `H1Pool::acquire` (`crates/envoy-http1/src/pool.rs:200-211`) checks `if *n >= self.max_connections { return Err(PoolError::Overflow { cluster, max }) }` on the connect-on-miss branch (per-endpoint `established: HashMap<SocketAddr, u32>`); `H2Pool::acquire` (`crates/envoy-http2/src/pool.rs:304-314`) does the same after exhausting per-connection stream-slot capacity.
- **The cap is config-sourced.** `H1PoolManager::for_bootstrap` (`pool.rs:358-363`) extracts `max_connections` via `cfg.circuit_breakers.as_ref().and_then(|cb| cb.thresholds.first()).and_then(|t| t.max_connections).unwrap_or(DEFAULT_MAX_CONNECTIONS)` (`DEFAULT_MAX_CONNECTIONS = 1024`, `pool.rs:22`); `H2PoolManager::for_bootstrap` (`pool.rs:507-512`) mirrors it.
- **The overflow is already rejected.** Both router arms map `PoolError::Overflow` → a synthetic 503: H1 at `crates/envoy-http1/src/hcm.rs:542` (with an explicit comment that `cx_total` does NOT increment on this arm — no connect was attempted), H2 at `crates/envoy-http2/src/hcm.rs:368-380`.

**What is therefore ALREADY DONE and NOT re-implemented by phase 15:** the schema, the validator (`validate_circuit_breakers` at `bootstrap.rs:2583-2613` with `ConfigError::{UnsupportedMultipleCircuitBreakerThresholds, UnsupportedCircuitBreakerPriority, InvalidMaxConnections}`), the per-endpoint cap enforcement, the config-sourcing, and the overflow→503 rejection.

**What is MISSING (and IS the phase-15 deliverable surface):**

1. **Circuit-breaker observability stats.** envoy-rust emits NONE of upstream Envoy's circuit-breaker stat tree. The minimum-viable subset phase 15 adds: `cluster.<name>.upstream_cx_overflow` (counter — +1 per rejected acquire) + `cluster.<name>.circuit_breakers.default.cx_open` (gauge 0/1 — `1` while `established == max_connections`). (Mirrors the 14.1 minimum-viable stat-subset precedent: emit the names Envoy emits for the breaker envoy-rust enforces; `allowlist_envoy_only` the rest.)
2. **Differential coverage of the overflow path.** NO existing fixture trips the cap. Fixture 0020 sets `max_connections: 4` but its `Driver::Http1KeepAlive` workload is **sequential over a single downstream keep-alive connection**, so it never needs >1 concurrent upstream connection and never overflows. Phase 15 adds a **new concurrent harness driver** that opens K simultaneous downstream connections, forcing K concurrent upstream connections, and a fixture (`0023`) where K > `max_connections` so the cap trips bilaterally.
3. **A `max_pending_requests: 0` schema carve-out** so the fixture's Envoy side does NOT queue (see §2.3 + §3 D1) — without it the two proxies diverge on overflow (Envoy queues; envoy-rust 503s immediately).

This finding is ratified in **ADR-0042** (landed at this brainstorm commit) and corrects the durable planning record.

---

## 1. Goal and acceptance signal

Phase 15 makes the **`max_connections` circuit breaker observable and differentially verified**. When a cluster configures `circuit_breakers.thresholds[0].max_connections: M` and the live in-flight upstream-connection demand exceeds `M`, both upstream Envoy and envoy-rust:

- reject the excess connection demand with a **503** (envoy-rust's existing synth-503 overflow arm; Envoy's `UO`-flagged local reply under `max_pending_requests: 0` — see §2.3), and
- emit `cluster.<name>.upstream_cx_overflow` (counter, value-exact under deterministic concurrent load) and `cluster.<name>.circuit_breakers.default.cx_open` (gauge; `1` while at the cap).

**Differential surface added by phase 15:**

- **Fixture `0023-upstream-circuit-breaker-max-connections`** — bilateral assertion that both proxies, given identical bootstraps configuring a pooled H1 upstream cluster with `circuit_breakers.thresholds[0].max_connections: 1` + `max_pending_requests: 0` (the no-queue carve-out), and a **concurrent** downstream workload of K=2 simultaneous slow requests (the synthetic backend holds each request open long enough that both are in-flight at once), produce: **one 200** (the request that acquired the single connection) **and one 503** (the request that overflowed the cap), with `cluster.<name>.upstream_cx_overflow = 1` and `cluster.<name>.circuit_breakers.default.cx_open` observed at `1` while saturated (settling to `0` after drain). The discriminating differential observable is the **overflow 503 + the `upstream_cx_overflow` counter tick** — without the cap both proxies would serve two 200s.

**Acceptance signal (a)–(f), per `BOOTSTRAP_PROMPT.md` §7.5:**

- **(a)** Fixture `0023-upstream-circuit-breaker-max-connections` green at Docker-gated CI.
- **(b)** All **22 pre-existing differential fixtures** (`0001` through `0022`) **remain green simultaneously** at the same CI run (regression-equivalence per §7.5 (b)). The new observability stats are inert/zero on every existing fixture (none configures a trip-able cap under concurrent load), and the new schema field (`max_pending_requests`) is `Option`-absent on every existing fixture, so the existing 22 stay byte-identical. **State-2 PLAN-writer empirically confirms** no existing fixture's `expectations.yaml` asserts the new stat names (they would read 0/absent) — the inert-when-unconfigured discipline (05.1/07.1/12.1/14.1 foundation-slice pattern applied to the stat subset).
- **(c)** `h2spec` continues at ≥95% (parent-05 baseline). Phase 15 does NOT touch the H2 downstream framing nor the H2 codec; the gate holds trivially (the state-4 verification re-confirms).
- **(d)** `parse_bootstrap` fuzz target clean for the short-budget CI run on the extended corpus (the existing `cluster_circuit_breakers.yaml` seed is extended with the `max_pending_requests: 0` shape; corpus count unchanged at 22 unless the PLAN-writer adds a distinct seed — recommended: extend the existing seed in place, OR add `cluster_circuit_breaker_overflow.yaml` 22 → 23, PLAN-writer's call).
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check` all clean.
- **(f)** `REVIEW.md` approved.

A **single CI run** must light up gates (a) through (e) **simultaneously** (continues the project precedent — fixture inheritance is a regression vector).

> **NOTE — single phase projected (see §6.1).** Phase 15's surface (schema `max_pending_requests:0` carve-out + validator + 2 observability stats wired at the pool cap-check sites for H1+H2 + concurrent harness driver + fixture 0023 + in-process backstop + fuzz-seed extension + BEHAVIOR_CONTRACT rows) is projected at **~950–1300 LoC / ~12–16 tasks**, UNDER the `BOOTSTRAP_PROMPT.md` §6.1 ~1500-LoC / ~25-task split gate. **Phase 15 is projected to ship as a SINGLE un-split phase.** The split valve (§6.1) is held in reserve: if the state-2 LoC estimate exceeds ~1500 (most likely driver — the concurrent harness driver + `cx_open` gauge wiring across both pools blowing up), the recommended seam is `15.1` (schema + observability stats foundation, no new fixture) / `15.2` (concurrent driver + fixture + parent close).

---

## 2. Behavior-contract scope for phase 15

Phase 15 extends `docs/envoy-rust/BEHAVIOR_CONTRACT.md` with authored additions, landed at the tasks where each is first empirically exercised (per the established 06.x→14.x doctrine — contract extensions land at empirical-engagement task time, NOT at PLAN-write time and NOT at state-1 SPEC time).

### 2.1 "Stat-name mapping" extension — circuit-breaker observability subset (projected; §6.2-verified)

New rows under the cluster circuit-breaker namespace, mirroring upstream Envoy v1.33's documented stat tree. **Minimum-viable subset** (the 14.1 namespace-subset precedent — emit the names Envoy emits for the breaker envoy-rust ENFORCES; the rest go on `allowlist_envoy_only`):

| Stat name | Equivalence (projected; §6.2-verified) | Rationale |
|---|---|---|
| `cluster.<name>.upstream_cx_overflow` | value-exact | Counter; one increment per upstream-connection demand rejected because the pool is at `max_connections`. Fires at the pool's cap-check site (`H1Pool::acquire` `pool.rs:204` / `H2Pool::acquire` `pool.rs:308`) — the SAME site that returns `PoolError::Overflow`. Under the fixture-0023 deterministic concurrent K=2 / cap=1 load, both proxies emit exactly `1`. The increment is single-source-of-truth (the cap-check branch), symmetric to the existing `cx_total`-on-connect-on-miss discipline. |
| `cluster.<name>.circuit_breakers.default.cx_open` | value-exact (steady-state; 0/1 gauge) | Gauge; `1` while the connection circuit breaker is "open" (the cluster is AT `max_connections` so further connections are denied), `0` otherwise. Updated inline at the per-endpoint `established`-count edges that reach/leave `max_connections` (one source of truth, NOT polled — the 08.2 `server.live` / 14.1 `ejections_active` pattern). The `default` segment is the DEFAULT `RoutingPriority` (the only priority phase 13/15 supports). **§6.2-verifiable nuance:** Envoy's `cx_open` is a CLUSTER-level gauge (per-priority, summed across hosts); envoy-rust tracks `established` PER-ENDPOINT (a 13.x design choice). For the **single-endpoint** fixture-0023 cluster they coincide; the multi-endpoint reconciliation defers (§4). |

**Deferred sibling `*_open` gauges** (`circuit_breakers.default.rq_pending_open`, `rq_open`, `rq_retry_open`, `cx_pool_open`) and the `remaining_*` gauges (which Envoy emits only under `track_remaining: true`) are NOT emitted by envoy-rust at phase-15 minimum-viable scope — they correspond to thresholds envoy-rust does not enforce (deferred per §4). They land on the fixture's `allowlist_envoy_only` (the 14.1 `allowlist_envoy_only`-for-deferred-names precedent). **§6.2 PLAN-writer empirically enumerates the exact Envoy-side set** so the allow-list is complete.

**Namespace empirical-verification signpost:** the upstream Envoy v1.33 admin `/stats?filter=circuit_breakers` + `/stats?filter=cx_overflow` scrape on a `max_connections`-tripped cluster under harness-deterministic concurrent load is the authoritative source. **The state-2 PLAN-writer empirically verifies the exact stat names + which fire at what cap-edge + the `cx_open` gauge's at-cap vs below-cap value before locking** (per §6.2). The projection above is provisional — DO NOT ASSUME.

### 2.2 "Response body / response flag" — overflow 503 wire shape (projected; §6.2-verified)

Phase 15 adds a BEHAVIOR_CONTRACT row for the **overflow-rejection wire shape**. envoy-rust's existing overflow arm emits `synth_status(503, close)` (empty body, the 04.3 synth wire shape — 5 standard HTTP/1.1 headers, `content-length: 0`). Upstream Envoy's `max_connections` overflow under `max_pending_requests: 0` emits a 503 local reply with the `UO` (UpstreamOverflow) response flag and a **non-empty body** (the §6.2-verifiable exact bytes — Envoy's circuit-breaker local-reply body). **This is a candidate §6.2 reconciliation** (mirrors ADR-0037's no-healthy-upstream-body reconciliation + ADR-0034's RBAC-body reconciliation): if Envoy's overflow body is non-empty, the phase reconciles envoy-rust's overflow arm to emit the matching bytes (a dedicated `synth_cx_overflow` helper adjacent to `synth_status` / `synth_no_healthy_upstream`), and the BEHAVIOR_CONTRACT row records the byte-exact body. **§6.2 PLAN-writer captures the exact body + header set + whether the `UO` flag surfaces in any header (it is primarily an access-log flag, not a response header — §6.2 confirms).** The conditional reconciliation ADR is reserved as ADR-0043 (§7).

### 2.3 The `max_pending_requests: 0` no-queue carve-out (the bilateral-equivalence enabler)

**The load-bearing differential-equivalence decision.** Upstream Envoy's `max_connections` circuit breaker does NOT immediately reject on cx-saturation by default: requests that need a connection but find the pool at `max_connections` are **queued as pending** (up to `max_pending_requests`, default 1024), then served as connections free. envoy-rust has NO pending-request queue — `H1Pool::acquire` returns `PoolError::Overflow` → immediate 503. **Under default `max_pending_requests` the two proxies would DIVERGE** (Envoy queues + eventually serves 200; envoy-rust 503s). To obtain a bilaterally-green fixture WITHOUT implementing the pending-request queue (deferred per §4), fixture 0023 sets `max_pending_requests: 0` on BOTH sides — disabling Envoy's queue so Envoy ALSO rejects the overflow immediately (`upstream_rq_pending_overflow` + 503 `UO`). This requires the schema to **accept `max_pending_requests: 0`** (the no-queue config) while **rejecting `max_pending_requests > 0`** (the queue, deferred). §6.2 confirms `max_pending_requests: 0` produces Envoy's immediate-reject behavior. The carve-out is ratified in ADR-0042 (the minimum-viable scope boundary) + empirically confirmed at §6.2 (projected ADR-0043 if the exact wire shape diverges).

### 2.4 DECISIONS.md amendment at SPEC time — ADR-0042 (the scoping ADR)

Unlike phases 12/13/14 (whose brainstorm SPECs landed NO ADR — the split + §6.2 ADRs landed at PLAN-write), phase 15's brainstorm DOES land an ADR: **ADR-0042** records (a) the critical finding that `max_connections` enforcement already landed at phase 13 (correcting the "parsed but not enforced" durable record), and (b) the minimum-viable scope boundary — deliver observability + the overflow differential + the `max_pending_requests: 0` carve-out; defer the pending-request queue + `max_requests`/`max_retries`/`track_remaining`/multi-priority + multi-endpoint `cx_open` reconciliation. The ADR is justified because the scope is non-obvious (it contradicts the charter framing) and cold-readability (D-3.4) demands a future session understand WHY phase 15 is observability-not-enforcement. Conditional §6.2-reconciliation + split ADRs are enumerated in §7.

---

## 3. Deliverables

Phase 15's scope is enumerated as deliverables `D1`–`D9` below. **The state-2 PLAN-writer organizes deliverables into tasks AND evaluates the §6.1 split gate** (projected NOT to fire — single phase). Deliverables are LISTED roughly in execution order; the SPEC constrains the surface, not the task organization.

### D1 — `envoy-config` schema extension (`Thresholds.max_pending_requests`, accept-0-only)

At `crates/envoy-config/src/bootstrap.rs`, extend the existing `Thresholds` struct (currently `priority` + `max_connections`, `bootstrap.rs:1177-1185`) with a `max_pending_requests` field:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Thresholds {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<RoutingPriority>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_connections: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_pending_requests: Option<u32>,   // phase-15: accept 0 ONLY (no-queue); reject > 0
}
```

`deny_unknown_fields` continues to reject the still-deferred threshold fields (`max_requests`, `max_retries`, `max_connection_pools`, `track_remaining`, `retry_budget`). The `/clusters` admin output already SHOWS Envoy emits `max_pending_requests::1024` (fixture 0014 expectations) — the field name is confirmed; phase 15 accepts only the `0` value.

### D2 — `envoy-config` validator extension (`max_pending_requests` accept-0 / reject->0)

Extend `validate_circuit_breakers` (`bootstrap.rs:2583-2613`) to reject `max_pending_requests > 0` with a new `ConfigError::UnsupportedNonZeroMaxPendingRequests { cluster, value }` (or a consolidated variant — PLAN-writer's call), carrying `cluster: String` per the established error-context discipline. `max_pending_requests: 0` and `max_pending_requests` absent both pass. Positive + negative parse-path unit tests land per the established 13.1 validator-test cadence. The validator is exercised by the existing `parse_bootstrap` fuzz target (D8 extends the seed).

### D3 — `upstream_cx_overflow` counter wiring (H1 + H2 pools)

Register `cluster.<name>.upstream_cx_overflow` at pool-construct time in `H1PoolManager::for_bootstrap` / `H2PoolManager::for_bootstrap` (next to the existing `cx_destroy` / `cx_http{1,2}_total` registrations, `pool.rs:364-367` / `:513-516`), gated on the cluster configuring `circuit_breakers` (inert when unconfigured — the 14.1 stat-registration discipline). Increment at the cap-check site in `H1Pool::acquire` (`pool.rs:204`, the `*n >= self.max_connections` branch, BEFORE the `return Err(PoolError::Overflow)`) and `H2Pool::acquire` (`pool.rs:308`). The counter handle is owned by the pool (mirrors `cx_total`/`cx_destroy`). One source of truth per protocol.

### D4 — `circuit_breakers.default.cx_open` gauge wiring (H1 + H2 pools)

Register `cluster.<name>.circuit_breakers.default.cx_open` at pool-construct time (gated on `circuit_breakers` configured). The gauge tracks "the cluster is at the connection cap." **Recommended wiring (PLAN-write lock-in):** update the gauge inline at the two `established`-count mutation edges per pool — set to `1` when an `established` increment reaches `max_connections`; set to `0` when an `established` decrement (connect-failure rollback `pool.rs:217-220`; pool eviction in `PoolGuard::Drop` `pool.rs:135-143`; idle-sweeper eviction) drops below `max_connections`. **§6.2-verifiable:** whether Envoy's `cx_open` flips on the connection that REACHES the cap (so cx_open=1 means "next connection would overflow") vs the connection that EXCEEDS it. The PLAN-writer confirms the exact edge semantic + whether `cx_open` is per-cluster (Envoy) vs per-endpoint (envoy-rust's `established` map) — for the single-endpoint fixture they coincide (§2.1 nuance + §4 carve-out). Consider a small `CxOpenGauge` RAII or an inline helper on the pool; the PLAN-writer picks (avoid a sweeper — this is edge-driven, not polled).

### D5 — Overflow 503 wire-shape reconciliation (conditional on §6.2)

If §6.2 confirms Envoy's `max_connections` overflow 503 (under `max_pending_requests: 0`) carries a non-empty body (§2.2), reconcile envoy-rust's overflow arm: add a `synth_cx_overflow(close)` helper adjacent to `synth_status` / `synth_no_healthy_upstream` emitting the byte-exact Envoy body + the 5 standard headers, and call it from the H1 `hcm.rs:542` + H2 `hcm.rs:368-380` overflow arms (replacing the current empty-body `synth_status(503, close)` on the overflow path ONLY — the connect-fail 502 + send-fail 502 paths keep `synth_status`). BEHAVIOR_CONTRACT §2.2 row records the body. **If §6.2 finds the overflow body is empty (matching envoy-rust today), D5 is a no-op** and the BEHAVIOR_CONTRACT row records the empty-body equivalence. Reserved ADR-0043 fires only if a material divergence is reconciled.

### D6 — Concurrent harness driver (`Driver::Http1Concurrent` — the new harness primitive)

The headline new test primitive. Existing drivers (`Http1`, `Http1KeepAlive`, `Http2`, `Http2KeepAlive`, `AdminScrape`) are all **sequential** — none issues concurrent in-flight requests, so none can force concurrent upstream-connection demand > `max_connections`. Phase 15 adds `Driver::Http1Concurrent` to `tests/differential/src/lib.rs`:

- Configurable `concurrency: usize` (K simultaneous downstream connections), each issuing one request, all launched together (e.g. `tokio::join!` / `FuturesUnordered`).
- Per-request expected-status assertion that tolerates **set-equality** (e.g. "expect exactly one 200 and one 503" — the ASSIGNMENT of which concurrent request wins the connection is non-deterministic, so the driver asserts the MULTISET of statuses, not a positional sequence).
- A post-settle admin `/stats` scrape asserting `upstream_cx_overflow` + (optionally) the `cx_open` at-saturation observation.
- The synthetic backend must HOLD each request open long enough for K requests to be simultaneously in-flight (so the cap actually trips) — reuse/extend the `tests/helpers/health-aware-http1-backend/` helper with a `--hold-ms <MS>` per-request delay knob (the §6.2-tuned hold window guarantees overlap under both proxies).

**§6.2-verifiable:** the hold-window + concurrency tuning that deterministically trips the cap on BOTH proxies (too-short a hold → the first request finishes before the second connects → no overflow → flaky). The PLAN-writer tunes against the live Envoy.

### D7 — Fixture 0023 + Docker wrapper + in-process backstop

- **D7.1 — Fixture `tests/fixtures/0023-upstream-circuit-breaker-max-connections/`.** Configures: an H1 upstream cluster with `circuit_breakers.thresholds[0]: { max_connections: 1, max_pending_requests: 0 }`; the hold-capable synthetic backend as the endpoint; an HCM listener routing `/` → backend; a `Driver::Http1Concurrent { concurrency: 2, hold_ms: <tuned> }` workload. Assertions: the status multiset `{200, 503}`; `cluster.<name>.upstream_cx_overflow: 1`; the `cx_open` saturation observation (§6.2 decides whether this is asserted at-saturation via a mid-flight scrape or only the terminal `0` — the terminal-0 settle is the safe bilateral assertion; the at-1 mid-flight observation may be backstop-only if timing-fragile across proxies). `allowlist_envoy_only` for the deferred Envoy-side `circuit_breakers.default.*` names per §2.1.
- **D7.2 — `tests/differential/tests/upstream_circuit_breaker.rs`** Docker-gated wrapper mirroring the 13.1/14.2 shape.
- **D7.3 — In-process backstop at `crates/envoy-bin/tests/upstream_circuit_breaker.rs`**, mirroring the 13.1/14.2 backstop shape. Boots `envoy-bin` with a synthesized bootstrap + in-process hold-capable backend; drives the concurrent K=2/cap=1 load; asserts the overflow 503 + `upstream_cx_overflow` + the `cx_open` gauge BOTH edges (rises to 1 at saturation, returns to 0 after drain — the backstop can observe the gauge directly without cross-proxy timing fragility, per the 14.2 both-convergence-directions backstop discipline). Includes the 5-standard-header presence assertion on the overflow 503 synth response (the 10/11/12.2/14.2 synth-header discipline).

### D8 — Fuzz corpus seed extension

Extend `crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_circuit_breakers.yaml` (the existing 13.1 seed) with the `max_pending_requests: 0` shape, OR add a sibling `cluster_circuit_breaker_overflow.yaml` (corpus 22 → 23) — PLAN-writer's call. If a new seed file: edit `crates/envoy-config/fuzz/.gitignore` allow-list AND the `bootstrap.rs::tests::fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS-array together (the 09/10/11/12.2/13.1 Task-N lesson — both files edited atomically).

### D9 — BEHAVIOR_CONTRACT extensions

Land the §2.1 stat rows (`upstream_cx_overflow` + `circuit_breakers.default.cx_open`) + the §2.2 overflow-body row at the task where each is first empirically exercised (the 06.x→14.x contract-extension cadence — at engagement task time, NOT at PLAN-write, NOT at SPEC time).

---

## 4. Out of scope (deferred non-goals)

Phase 15 explicitly does NOT land:

- **`max_pending_requests > 0` (the pending-request queue).** Phase 15 accepts ONLY `max_pending_requests: 0` (no-queue). The queue-until-a-connection-frees semantic (+ `upstream_rq_pending_active` / `upstream_rq_pending_total` / `rq_pending_open` stats) defers to a follow-up phase.
- **`max_requests` (active-request circuit breaker).** The total-active-request cap + `rq_open` gauge + `upstream_rq_pending_overflow`-adjacent `upstream_rq_total`-cap semantics defer.
- **`max_retries` / `retry_budget` (retry circuit breaker).** Tied to retries+hedging (a distinct phase). Defers.
- **`max_connection_pools`.** Defers.
- **`track_remaining: true` + the `remaining_*` gauges** (`remaining_cx`, `remaining_pending`, `remaining_rq`, `remaining_retries`). Envoy emits these only under `track_remaining`; phase 15 rejects `track_remaining` (deny_unknown_fields) and does not emit the `remaining_*` gauges. Defers.
- **Non-DEFAULT `RoutingPriority` thresholds.** Phase 15 (like 13) supports the single DEFAULT priority. Multi-priority circuit breaking defers (the `circuit_breakers.high.*` namespace stays Envoy-only / allow-listed).
- **Per-cluster (vs per-endpoint) `max_connections` semantics + the multi-endpoint `cx_open` reconciliation.** envoy-rust's 13.x cap is per-endpoint (`established: HashMap<SocketAddr, u32>`); Envoy's is per-cluster (summed across hosts/priorities). Fixture 0023 uses a single-endpoint cluster so they coincide. The multi-endpoint reconciliation (sum the per-endpoint `established` for a cluster-level cap) defers — flagged as a known divergence in §5.4 + the new SPEC's carryforward.
- **Retries + hedging.** Distinct phase.
- **Outlier-detection variants** (success-rate / failure-percentage). Distinct phase (extends 14.x).
- **TCP / gRPC active HC checkers; TCP-proxy connection pooling** (the `upstream_cx_total` TCP carve-out). Stay deferred per phases 12/13.

---

## 5. Architectural invariants

Phase 15 honors and extends the established cross-crate invariants:

### 5.1 No new crate, no new top-level Cargo dep

All work lands inside existing crates: `envoy-config` (schema + validator), `envoy-http1` + `envoy-http2` (pool stat wiring at the existing `pool.rs` cap-check sites + the overflow-arm reconciliation at `hcm.rs`), `tests/differential` (the new `Driver::Http1Concurrent`), `tests/helpers/health-aware-http1-backend` (the `--hold-ms` knob), `tests/fixtures` (0023), `crates/envoy-bin/tests` (backstop). **No new workspace member; no new top-level Cargo dep.** The concurrent driver uses tokio primitives already pulled.

### 5.2 Inert-when-unconfigured (the foundation-slice discipline applied to stats)

The two new stats (`upstream_cx_overflow`, `circuit_breakers.default.cx_open`) register ONLY for clusters that configure `circuit_breakers` (the 14.1 conditional-registration pattern). The 22 existing fixtures (none of which trips a cap under concurrent load) see the counters at 0 / the gauge at 0, OR — for clusters without `circuit_breakers` — see no such stat registered at all. Regression-equivalence (acceptance gate (b)) holds because no existing fixture asserts these names.

### 5.3 One-source-of-truth stat sites (the 06.x→14.x discipline)

`upstream_cx_overflow` increments at exactly ONE site per protocol (the pool cap-check branch). `cx_open` updates at exactly the `established`-count mutation edges (no polling — the 08.2 `server.live` / 14.1 `ejections_active` inline-edge-update pattern). The PLAN-writer ensures no double-counting and that the gauge is a terminal-0 gauge (returns to 0 after drain, like the 06.3 `cx_active` / 14.1 `ejections_active` gauges) so the fixture's post-settle terminal scrape is deterministic.

### 5.4 Known divergence: per-endpoint vs per-cluster cap (documented, single-endpoint-fixture-sidestepped)

envoy-rust's `max_connections` enforcement is per-endpoint (a 13.x design choice — `established: HashMap<SocketAddr, u32>`); Envoy's is per-cluster. Phase 15 does NOT reconcile this (per §4); it uses a single-endpoint STRICT_DNS cluster (single-A resolution, the 04.3/05.3/13.x fixture posture) so per-endpoint == per-cluster and the fixture is bilateral. The divergence is recorded in the new SPEC's §5.4 + the STATE.md `Phase-15 rollovers` carryforward so a future multi-endpoint circuit-breaker phase owns it.

### 5.5 Concurrent driver determinism (heeds the flakiness risk)

The new `Driver::Http1Concurrent` introduces the project's first CONCURRENT differential driver — a flakiness vector if the hold-window is mis-tuned. The §6.2 PLAN-writer tunes the `--hold-ms` window + concurrency against the live Envoy so the cap deterministically trips on BOTH proxies (the first request must still be in-flight when the second connects). The driver asserts the status MULTISET (not a positional sequence) because connection-acquisition order is non-deterministic. The in-process backstop (D7.3) provides a timing-robust second signal that observes the gauge edges directly.

---

## 6. Implementation signposts for the planner

The state-2 PLAN-writer reads this section to drive PLAN structure.

### 6.1 Split-gate evaluation (READ FIRST — split projected NOT to fire)

Per `BOOTSTRAP_PROMPT.md` §6.1, the state-2 PLAN-write evaluates whether the PLAN exceeds ~25 numbered tasks OR ~1500 LoC. Phase 15's surface estimate at SPEC time:

- D1 — schema (`max_pending_requests` field) (~25 LoC + ~50 LoC tests).
- D2 — validator (1 ConfigError variant + accept-0/reject->0) (~40 LoC + ~80 LoC tests).
- D3 — `upstream_cx_overflow` wiring (H1 + H2) (~60 LoC + ~80 LoC tests).
- D4 — `cx_open` gauge wiring (H1 + H2, edge-driven) (~120 LoC + ~120 LoC tests).
- D5 — overflow-body reconciliation (conditional; 0–~60 LoC + ~60 LoC tests).
- D6 — `Driver::Http1Concurrent` + `--hold-ms` backend knob (~150 LoC + ~80 LoC tests).
- D7.1 — fixture 0023 (YAML + expectations) (~120 LoC).
- D7.2 — Docker-gated wrapper (~40 LoC).
- D7.3 — in-process backstop (~220 LoC).
- D8 — fuzz seed (~25 LoC + ≤2 file edits).
- D9 — BEHAVIOR_CONTRACT rows (~50 LoC docs).
- State-4 verification + STATE-advance (~docs).

**SPEC-time projection: ~12–16 tasks; ~950–1300 LoC** (production ~330, tests ~500, fixture/harness/backstop ~380, docs ~90). **This is UNDER the §6.1 ~1500-LoC / ~25-task gate → single un-split phase.** If the state-2 LoC estimate exceeds ~1500 (most likely driver: the concurrent driver + `cx_open` edge-wiring across both pools), the recommended seam is **`15.1`** (D1+D2+D3+D4+D9 — schema + observability stats foundation; NO new fixture; regression-equivalence on the 22 existing fixtures via the inert-when-unconfigured pattern — the 14.1 foundation-slice precedent) / **`15.2`** (D5+D6+D7+D8 — concurrent driver + fixture 0023 + overflow reconciliation + parent-15 close). The split ADR would be ADR-0044 (after ADR-0042 scoping + the conditional ADR-0043 §6.2 reconciliation). **Projected single — the split is held in reserve.**

### 6.2 Empirical verification at state-2 PLAN-write (HEAVY for this phase)

Per the phase-10/11/12/13/14-ratified verify-at-PLAN-write process: **the state-2 PLAN-writer empirically verifies the upstream wire/behavior shapes BEFORE locking PLAN lock-ins.** Run `envoyproxy/envoy:v1.33.0` Docker with a `max_connections: 1` + `max_pending_requests: 0` cluster + the hold-capable backend + a concurrent K=2 driver + admin `/stats`, and verify:

1. **`max_pending_requests: 0` behavior:** does Envoy reject the overflow IMMEDIATELY (no queue) under `max_pending_requests: 0`? (The bilateral-equivalence premise — §2.3. If Envoy still queues at `0`, the carve-out fails and the phase re-scopes — UNLIKELY but verify.)
2. **Exact circuit-breaker stat names + values:** `cluster.<name>.upstream_cx_overflow` (value after K=2/cap=1 → expect `1`); `cluster.<name>.circuit_breakers.default.cx_open` (gauge value at saturation → expect `1`; at drain → `0`); enumerate the FULL `circuit_breakers.default.*` + `circuit_breakers.high.*` Envoy-side set for the `allowlist_envoy_only`.
3. **Overflow 503 wire shape:** exact status (503?), the `UO` response flag surfacing (access-log only? any response header?), and the body bytes (empty? non-empty Envoy local-reply text?) — §2.2 / D5.
4. **`cx_open` edge semantic:** flips at the connection that REACHES `max_connections` (cx_open=1 means "at cap") vs EXCEEDS it — D4.
5. **Per-endpoint vs per-cluster:** confirm Envoy's `max_connections` is per-cluster (so the single-endpoint fixture is the bilateral-safe topology) — §5.4.
6. **Hold-window tuning:** the `--hold-ms` + concurrency that deterministically trips the cap on BOTH proxies (§5.5).
7. **`upstream_cx_total` / `upstream_cx_active` interaction:** confirm the overflow-rejected request does NOT increment `cx_total` (no connect attempted) — matches the existing `hcm.rs:542` comment — on BOTH proxies.

Each finding lands as a PLAN lock-in. **If finding 1 or 3 differs materially from the SPEC projection, the lock-in records the divergence + the SPEC §2.x revision via an inline ADR at the state-2 PLAN-write commit** (mirrors phase-12 ADR-0037 / phase-14 ADR-0041). The reserved number is **ADR-0043** (§7).

### 6.3 In-process backstop assertions (heeds the 14.2 both-directions lesson)

D7.3 SHOULD exercise the `cx_open` gauge BOTH edges (rises to 1 at saturation; returns to 0 after drain) — the backstop observes the gauge directly without cross-proxy timing fragility (the 14.2 eject/un-eject both-convergence-directions discipline). Include the 5-standard-header presence assertion on the overflow 503 synth response.

### 6.4 The 06.x stats convention + the inert-when-unconfigured discipline

StatsRegistry registration at pool-construct time, gated on `circuit_breakers` configured (the 14.1 conditional-registration pattern); per-pool ownership of the Counter/Gauge handles; the increment/edge-update sites are single-source-of-truth (§5.3).

### 6.5 Pre-state-4 fmt discipline (continues per 06.1 R-9)

Per-task PROGRESS sections quote `cargo fmt --all -- --check` at every PROGRESS-task close, NOT just at state-4.

### 6.6 State-4 evidence-discipline (continues per 05.3 → … → 14.2 chain)

Per-gate quoted evidence in PROGRESS at the state-4 verification task: real CI run URL + HEAD SHA + completion timestamp + per-gate quoted output (5 stable-toolchain gates + each Docker-gated fixture + h2spec_pass_rate_gate + parse_bootstrap fuzz iteration count).

### 6.7 Isolated-crate build discipline (heeds `project_isolated_crate_build_blindspot` / the 14.2 state-5 I-1 finding)

`cargo build --workspace` (the §7.5 gate) can be GREEN while `cargo build -p <crate>` FAILS — feature unification across the workspace masks a missing per-crate feature. Phase 15 touches `envoy-config`, `envoy-http1`, `envoy-http2`. **The state-4 verification MUST run `cargo build -p envoy-config`, `cargo build -p envoy-http1`, `cargo build -p envoy-http2` STANDALONE** (in addition to the workspace build) to catch a feature-unification blind spot. Quote each standalone build in PROGRESS.

### 6.8 Cargo.lock cadence

The phase-04.1 REVIEW M5/M9 Cargo.lock-cadence ADR carries forward. Phase 15 adds zero new top-level Cargo deps.

### 6.9 PLAN.md + PROGRESS.md skeleton + Task 1 preamble land alongside at state-2

Per the 06.2 → … → 14.2 cadence. State-2 PLAN-write lands `PLAN.md` + `PROGRESS.md` skeleton + Task 1 preamble in a single standalone pre-Task-1 commit.

### 6.10 Subagent-driven execution at state 3 (per `feedback_execution_style`)

State 3 implementation is subagent-driven (`superpowers:subagent-driven-development`), implementers dispatched SERIALLY (`feedback_serial_subagent_dispatch`) — not parallel (they race on `main`). Not engaged at this state-1 brainstorm.

---

## 7. Conditional ADRs (projected; land at PLAN-write or in-execution if they fire)

- **ADR-0042 (LANDED at this brainstorm commit) — phase-15 minimum-viable scope.** Records: `max_connections` enforcement already landed at phase 13 (correcting "parsed but not enforced"); phase 15 delivers observability (`upstream_cx_overflow` + `circuit_breakers.default.cx_open`) + the overflow differential fixture + the `max_pending_requests: 0` no-queue carve-out; defers the pending-request queue + `max_requests`/`max_retries`/`track_remaining`/multi-priority + the multi-endpoint `cx_open` reconciliation. (This is the ONLY ADR landed at the brainstorm; the cadence-departure from phases 12/13/14 is justified by the non-obvious scope finding.)
- **Conditional ADR-0043 (PLAUSIBLE) — §6.2 empirical-verification revision.** Fires if §6.2 finding 1 (`max_pending_requests: 0` behavior) or finding 3 (overflow 503 body bytes / wire shape) diverges materially from the §2.2/§2.3 projection. Mirrors ADR-0037 (phase-12 no-healthy-upstream-body reconciliation) + ADR-0034 (phase-10 RBAC-body). Lands at the state-2 PLAN-write commit if it fires.
- **Conditional ADR-0044 (UNLIKELY) — phase split.** Fires ONLY if the state-2 LoC estimate exceeds ~1500 (§6.1). Projected NOT to fire (single phase). If it fires, the seam is `15.1` (schema + observability foundation, no fixture) / `15.2` (concurrent driver + fixture + parent close).

**ADR ledger at SPEC time:** DECISIONS.md head is ADR-0041 (count 42); this SPEC's commit lands **ADR-0042** (count 43; next available ADR-0043).

---

## 8. Summary

Phase 15 is the fourth Upstream-robustness-family phase. It makes the **already-enforced** `max_connections` circuit breaker **observable and differentially verified**: two minimum-viable circuit-breaker stats (`upstream_cx_overflow` counter + `circuit_breakers.default.cx_open` gauge), the project's first CONCURRENT differential driver (`Driver::Http1Concurrent`) to trip the cap, a `max_pending_requests: 0` no-queue schema carve-out enabling bilateral equivalence without the (deferred) pending-request queue, and fixture 0023 proving the overflow→503 wire shape + the overflow counter bilaterally. Projected single un-split phase (~950–1300 LoC). The scope finding (enforcement-already-landed) + the minimum-viable boundary are ratified in ADR-0042 at this brainstorm commit; the §6.2 wire-shape verification + the conditional reconciliation/split ADRs are reserved for the next session's PLAN-write.
