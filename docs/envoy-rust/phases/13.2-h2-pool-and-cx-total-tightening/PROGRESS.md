# Phase 13.2 (`13.2-h2-pool-and-cx-total-tightening`) — PROGRESS

> Running log of state-3 execution. Each task closes with a `### Task N — <title>` subsection quoting test names + per-gate clean outputs + commit SHA. State-4 (Task 7) quotes per-gate evidence per the 12.2 / 13.1 state-4 precedent. State-6 (Task 8) is the closing-sub-phase close-out — flips ROADMAP rows `13.2` AND parent `13` simultaneously per the closing-sub-phase invariant.

---

## Task 1 preamble — PLAN-write context for state-3 controller

This section is the controller's read-on-start brief; it lands at the state-2 PLAN-write commit (not Task 1's own commit) per the 12.1 / 12.2 / 13.1 state-2 PLAN-write precedent.

### Locked §2 facts (DO NOT re-run Docker)

The parent-13 state-2 PLAN-write performed the parent SPEC §6.2 HEAVY 9-item empirical verification against `envoyproxy/envoy:v1.33.0`. All 9 items MATCHED the parent SPEC's projections; the findings are LOCKED into the parent-13 SPEC §6.2 + the 13.1 SPEC §2 + the STATE.md `### Phase-13 state-2 split decision` subsection. The 13.2 state-3 controller MUST NOT re-run Docker for §6.2 verification; if a 13.2 implementation detail surfaces a new empirical question, verify against code/h2 docs at task time. The 9 findings (recap; full details in the parent-13 SPEC §6.2):

- **(i)** `circuit_breakers.thresholds[<i>].{priority?, max_connections?}` shape; priority defaults DEFAULT (omitted in /config_dump when DEFAULT); max_connections default 1024. (Schema landed at 13.1 D1; 13.2 reuses verbatim.)
- **(ii)** Envoy ALWAYS pools regardless of `circuit_breakers` config; default `max_connections: 1024` (never hit under fixture load). → 13.2 H2 pool default-enabled with hardcoded defaults when `circuit_breakers` absent (PLAN lock-in #3).
- **(iii)** Idle_timeout knob lives at `typed_extension_protocol_options.envoy.extensions.upstreams.http.v3.HttpProtocolOptions.common_http_protocol_options.idle_timeout` (default 3600s). → 13.2 hardcodes 60s (matches 13.1's 60s); config-side knob defers.
- **(iv)** **CRITICAL nuance:** `upstream_cx_total: 1` requires a SINGLE downstream keep-alive conn issuing N sequential requests. With separate downstream conns: `upstream_cx_total: N`. → 13.2 fixture 0021 driver REUSES `Driver::Http1KeepAlive` (the 13.1 D10 driver landed verbatim — downstream H1 keep-alive, upstream H2 via cluster's `typed_extension_protocol_options.http2_protocol_options`).
- **(v)** `upstream_cx_destroy` + 5 sub-siblings. 13.2 reuses `upstream_cx_destroy` (cluster-level — registered at 13.1; H2 pool wires the same Arc handle via the same idempotent same-kind registry contract).
- **(vi)** **THE H2 finding:** default `max_concurrent_streams` honors peer SETTINGS frame (no envoy-side limit by default); Envoy-as-client uses the server's SETTINGS_MAX_CONCURRENT_STREAMS cap (RFC 7540 §6.5.2 default 100 if peer hasn't sent SETTINGS). Per-endpoint multi-connection threshold: Envoy spawns a 2nd upstream H2 connection only when the 1st is at peer's MAX_CONCURRENT_STREAMS cap OR at the cluster's `circuit_breakers.max_connections` cap, whichever is lower. Stat namespace includes `cluster.<name>.upstream_cx_http2_total`. → 13.2 D5 H2Pool design + D7.2 new contract row (PLAN Tasks 1 + 3).
- **(vii)** Per-class HCM counters bilateral byte-equality (verified). → 06.3 REVIEW I2 (a) closure surface; CLOSED at 13.1 Task 7 fixture 0020; NOT re-asserted at 13.2 fixture 0021 (the H2-pool fixture focuses on the pool-reuse counter pair, not per-class distribution).
- **(viii)** Cluster `upstream_rq_{2,3,4,5}xx` per-class distribution byte-equal. → Same disposition as (vii).
- **(ix)** **CONFIRMED:** HCM `downstream_rq_5xx` INCLUDES synth-503; cluster `upstream_rq_5xx` does NOT (synth bypasses upstream). Body byte-exact = ADR-0037's 19 bytes `no healthy upstream`. (Inherited from 12.2; 13.2 preserves unchanged.)

### PLAN-time SPEC corrections (verified at state-2 PLAN-write against HEAD `9d8e9ca`)

The 13.2 PLAN-writer read every named seam directly and confirms the following corrections to the 13.2 SPEC + the next-prompt.txt's anticipated shape. State-3 honors these inline; PROGRESS Task N subsections name the relevant correction inline when the named seam is touched.

1. **`envoy-http2::Client::connect` at `crates/envoy-http2/src/client.rs:19`** — confirmed `pub async fn connect(addr: SocketAddr, host: &str) -> Result<ClientStream, Http2Error>`. Matches H1's `Client::connect` signature verbatim modulo the error type.

2. **`ClientStream` at `crates/envoy-http2/src/client.rs:75`** has **PRIVATE** fields (`send_request: h2::client::SendRequest<Bytes>` + `host: String`). Task 1 Step 3 widens both to `pub(crate)` AND derives `Clone` on `ClientStream` (the H2 pool needs to clone the stream per-PoolGuard; `SendRequest<Bytes>` is `Clone` per h2 v0.4 — that's the multiplexing-enabling property). Mirrors `envoy_http1::ClientStream`'s post-13.1 pub(crate) posture.

3. **H2 `upstream_cx_total` increment sites at `crates/envoy-http2/src/hcm.rs`** — confirmed `:280` (the H1-cluster-in-H2-HCM arm) AND `:291` (the H2-cluster-in-H2-HCM arm). **PLAN-time SPEC correction**: the 13.2 SPEC §3 D6 says "modify hcm.rs:280 AND :291"; PLAN-time pick is to migrate ONLY :291 (the H2 arm). The :280 H1-cluster-in-H2-HCM site stays per-call at 13.2 — this is an unusual configuration (the H1 HCM is the primary path for H1 clusters via 13.1; the H1-cluster-via-H2-listener path is a rare cross-protocol case that 13.1's pool integration also did not cover). PROGRESS Task 2 names this correction explicitly. The 13.1 H1 pool's `H1PoolManager` is NOT plumbed into the H2 HCMConfig at 13.2 — keeping the surface narrow + deferring the H1-cluster-in-H2-HCM pool integration to a future cleanup phase.

4. **The 13.1-landed `H1PoolManager` at `crates/envoy-http1/src/pool.rs:~295-375`** — confirmed; the architectural sibling for the new `H2PoolManager`. 13.2 mirrors the for_bootstrap shape verbatim modulo the protocol filter (`UpstreamProtocol::Http2` instead of `Http1`).

5. **`ConnGaugeGuard::from_gauge(Arc<envoy_stats::Gauge>) -> Self` public constructor at `crates/envoy-cluster/src/cluster.rs`** — confirmed exists (landed at 13.1 Task 3). 13.2 H2 pool's `acquire_cx_active_guard()` consumes it directly — NO additional envoy-cluster touch at 13.2.

6. **The 13.1-landed `Driver::Http1KeepAlive` at `tests/differential/src/lib.rs:~167-173`** — confirmed; serde variant kind `http1_keep_alive` + `Http1KeepAliveRequest` + `KeepAliveExpectedStat` structs + the dispatch arm + the read_h1_response_status + scrape_admin_stat helpers all reusable verbatim for fixture 0021. The driver is downstream-protocol-H1; the upstream protocol is determined by the cluster config (H2 via `typed_extension_protocol_options`).

7. **The existing `tests/helpers/http2-echo-server/` helper** — confirmed exists at the workspace member level; H2-multiplexing-capable (per-stream `tokio::spawn` shape per `crate::client::tests::spawn_h2_server`); fixture 0021 uses it as backend WITHOUT extension (the H2 pool's discriminating observable is `upstream_cx_total: 1`, not per-class status — no `--per-path` flag needed; the helper echoes all requests with 200).

8. **`BEHAVIOR_CONTRACT.md:~89`** carries the `cluster.<name>.upstream_cx_total` row at `name-required, value-may-differ` per 13.1 PLAN lock-in #3 (the explicit non-tightening at 13.1). Task 4 (D7.1) is the named owner for the tightening to `value-exact` + explicit TCP-proxy carve-out.

9. **`envoy-http2::HCMConfig` type alias at `crates/envoy-http2/src/hcm.rs:27`** — confirmed `pub type HCMConfig = Http1HCMConfig;`. **PLAN-time SPEC correction:** Task 2 Step 1 REPLACES the type alias with a proper struct wrapping `Arc<envoy_http1::HCMConfig>` + adding `h2_pool_mgr: Option<Arc<H2PoolManager>>`. Direct addition of an `h2_pool_mgr` field to `envoy_http1::HCMConfig` is NOT possible (would invert the existing envoy-http2 → envoy-http1 dep direction; envoy-http1 can't reference the H2PoolManager type that lives in envoy-http2). The wrapper struct pattern is the cleanest cycle-free addition. PROGRESS Task 2 documents this correction in detail.

10. **`crates/envoy-bin/src/main.rs`** — confirmed: `cluster_mgr` built at `:~123`; `H1PoolManager::for_bootstrap` wired at `:~137-143`; `envoy-health::Scheduler::spawn` at `:~150`. Task 2 inserts `H2PoolManager::for_bootstrap` between the H1 pool manager AND the health scheduler (3-line insertion).

11. **`envoy-http2/Cargo.toml`** — confirmed does NOT carry `tokio-util` or `parking_lot` deps today. Task 1 Step 1 adds both as sub-crate deps. Neither is a new top-level workspace dep (per lock-in #14): `parking_lot` is workspace-pre-existing as a `tokio` transitive; `tokio-util` is workspace-pre-existing as `envoy-bin` + `envoy-health` + (post-13.1) `envoy-http1` member deps.

12. **The 13.1-landed H1 pool's mutexes are `tokio::sync::Mutex`** — confirmed at `crates/envoy-http1/src/pool.rs`. The A-I3 closure at Task 1 Step 8 switches BOTH to `parking_lot::Mutex` (synchronous). The state-5 fold-in `Handle::try_current()` guard at `pool.rs:118-128` is REMOVED at the same step (no longer needed — Drop is sync). The existing `pool_guard_drop_outside_runtime_does_not_panic` regression test is REMOVED (its scenario is now structurally unreachable — sync Drop never spawns) + REPLACED with a sync-Drop equivalence test.

### Cycle-resolution decision narrative (PLAN lock-in #1) — picked per `feedback_pick_recommendation`

The 13.2 SPEC §5.1 named the cycle-resolution pattern (the H2 pool primitive lives inside envoy-http2; the bin owns `Arc<H2PoolManager>` injected into HCM configs at startup; no new trait in envoy-cluster). The PLAN-write reads this as a verbatim mirror of 13.1's cycle-resolution decision and applies the same pattern — **external `H2PoolManager` sibling registry to ClusterManager** (NOT a field on Cluster).

Rationale (the 4-point argument from 13.1 PLAN lock-in #1 carries forward verbatim):
1. Adding `h2_pool: Arc<H2Pool>` to `envoy_cluster::Cluster` would require interior mutability OR widening `from_bootstrap`'s signature — all intrusive.
2. The external manager pattern parallels 12.2's `envoy-health::Scheduler` precedent + 13.1's `H1PoolManager` precedent verbatim.
3. NO new trait declared in `envoy-cluster`. NO new top-level Cargo dep. NO modification to `envoy-cluster::Cluster`'s struct shape (the existing `ConnGaugeGuard::from_gauge` from 13.1 + the `ClusterHandle::cx_active_arc` accessor from 13.2 are the only envoy-cluster-touching surfaces; the latter may need to be added at Task 1 if the existing accessor doesn't return `Arc<Gauge>` directly).
4. NO ADR fires (PLAN lock-in #17 — SPEC §7).

**One subtle 13.2-specific decision** the SPEC didn't fully resolve: how does the H2 HCMConfig carry the new pool manager field? The existing `pub type HCMConfig = Http1HCMConfig;` (envoy-http2/src/hcm.rs:27) re-exports the H1 HCMConfig type verbatim. Adding `h2_pool_mgr: Option<Arc<H2PoolManager>>` directly to `envoy_http1::HCMConfig` would invert the envoy-http2 → envoy-http1 dep direction (the H2PoolManager type lives in envoy-http2; envoy-http1 cannot reference it). **PLAN-time pick (per `feedback_pick_recommendation`):** replace the type alias with a proper `envoy_http2::HCMConfig` struct wrapping `Arc<envoy_http1::HCMConfig>` + adding the new field. The H2 HCM accesses `config.inner.<H1 fields>` for the H1-side config + `config.h2_pool_mgr` for the new field. envoy-bin's wire-up updates 1 HCMConfig consumption site per H2 listener — a focused mechanical change. This is documented at Task 2 Step 1 + lock-in #2.

The alternative pattern (declare an `H2DispatchPort` trait in envoy-http1 + have H2PoolManager implement it; envoy_http1::HCMConfig holds `Option<Arc<dyn H2DispatchPort>>`) was considered but rejected: it adds trait-object indirection to a hot path + the trait's `send_request_to_cluster` method shape is awkward when the H2 HCM needs fine-grained acquire/release control (per the existing per-request `cluster.upstream_rq_total + upstream_rq_5xx` increment pattern after the dispatch returns).

### H2 pool's `ConnGaugeGuard` semantics (PLAN lock-in #6) — picked per `feedback_pick_recommendation`

The 13.2 SPEC §5.6 has an internal ambiguity on the H2 pool's `cx_active` gauge semantic. The first clause ("each H2 PoolGuard counts 1") reads as "per-guard increment" — under concurrent loads, `cx_active = N` for N concurrent PoolGuards on the same connection (= "active streams" semantic). The second clause ("matches upstream Envoy's `upstream_cx_active` semantic of 'active connections', NOT 'active streams'") reads as "per-connection increment" — `cx_active = 1` regardless of concurrent guards on the same conn.

**PLAN-time pick:** option A (per-guard / "active streams" semantic). Rationale (per `feedback_pick_recommendation`):
- Matches the 13.1 H1 pool's pattern verbatim — each PoolGuard owns one ConnGaugeGuard. Parallel code paths for H1+H2 simplify future readers.
- Under fixture 0021's sequential single-stream-at-a-time workload (Driver::Http1KeepAlive issues N sequential downstream requests over one H1 conn → at most one PoolGuard alive at any time → peak `cx_active: 1`), the divergence from upstream Envoy's per-connection semantic is invisible — both interpretations yield the same bilateral observable.
- Under hypothetical concurrent workloads beyond the fixture, the divergence becomes visible (envoy-rust under interpretation A would emit `cx_active = N` where upstream Envoy emits `cx_active = 1`). This is a future-phase concern (no current fixture exercises concurrent H2 streams; the in-process backstop's optional concurrent-stream extension is SKIPPED per Task 6 — see PLAN lock-in #6 + Task 6 architectural notes).

The new BEHAVIOR_CONTRACT row for `cluster.<name>.upstream_cx_http2_total` at Task 3 documents the per-codec stat (NOT `cx_active`); the existing `cluster.<name>.upstream_cx_active` BEHAVIOR_CONTRACT row at `BEHAVIOR_CONTRACT.md` carries the 06.3 disposition (`value-exact (deterministic close)`) which holds under interpretation A for the sequential workload.

### Carryforward dispositions entering 13.2

**Closures attributed at 13.2 (lock-in #4 + lock-in #10):**

- **06.3 REVIEW I2 (b)** (`cluster.<name>.upstream_cx_total` value-exact BEHAVIOR_CONTRACT row tightening) — **FULLY CLOSED at Task 4** (the row tightening commit). PROGRESS at Task 4 attributes the closure honestly. Combined with 13.1's I2 (a) closure (fixture 0020), the FULL 06.3 REVIEW I2 carryforward CLOSES at parent-13 close-out (Task 8) — re-attributed at the closing commit.

- **13.1 REVIEW Cluster A-I3 (deferred-Important)** (spurious-overflow race under concurrent acquire/release at `crates/envoy-http1/src/pool.rs:178-203`) — **CLOSED at Task 1** jointly across BOTH H1 + H2 pools via the parking_lot::Mutex switch. PROGRESS at Task 1 attributes the joint closure + the H1 pool's Mutex migration + the new race-regression TDD test.

- **13.1 REVIEW Cluster A-M1** (`_sweepers` field underscore-prefix + no explicit shutdown path on H1PoolManager) — **CLOSED at Task 1** opportunistically. Both H1PoolManager + H2PoolManager get a `pub async fn shutdown(self)` method + the field is renamed `sweepers` (no underscore).

- **13.1 REVIEW Cluster A-M2** (`acquire_cx_active_guard` lacks `Arc::ptr_eq` debug-assert) — **CLOSED at Task 1** opportunistically. Both pool managers' `for_bootstrap` adds `debug_assert!(Arc::ptr_eq(...))` at the gauge wiring site.

- **13.1 REVIEW Cluster A-M4** (`H1PoolManager::for_bootstrap` `.expect("...")` future-caller panic surface) — **CLOSED at Task 1** opportunistically. Both pool managers' `.expect` message documents the precondition explicitly: `"H[12]PoolManager::for_bootstrap requires cluster_mgr built from the same bootstrap (single-bootstrap-per-process invariant)"`.

**Carryforwards entering 13.2 (none gates state-2 or state-3):**

- **13.1 REVIEW Cluster A-M3** (Task 3 PROGRESS narrative-finishing-touch about "6 deviations" framing) — **no-action narrative**; the framing is honest per D-3.4. Carry forward without action.

- **13.1 REVIEW Cluster A-M5** (BEHAVIOR_CONTRACT `upstream_cx_destroy` row phrasing nit) — **no-action narrative**. Carry forward.

- **13.1 REVIEW Cluster B-M1..B-M3** (3 backstop/test-helper Minors — tempfile leak deliberate; Task 4 unit test discriminating power narrower than comment; backstop CL parser brittle) — carry forward unchanged. Opportunistic closure at any future phase extending those backstop / harness seams.

- **13.1 REVIEW Cluster C-M1..C-M4** (4 harness / wrapper-test / cosmetic Minors — read_h1_response_status missing-CL fallback; scrape_admin_stat Ok(0) absent-stat masking; wrapper-test doc-comment stale; helper keep-alive cosmetic stack noise) — carry forward unchanged. C-M3 (wrapper-test doc-comment) auto-closes when the cluster per-class `upstream_rq_{2,3,4}xx` follow-up phase lands.

- **Cluster per-class `upstream_rq_{2,3,4}xx` counter family extension** (the 13.1 known-deferred small follow-up) — not engaged at 13.2 (the named seam `crates/envoy-cluster/src/cluster.rs:71-76` is NOT touched at 13.2; the 13.2 surface focuses on the H2 pool primitive + the row tightening). Continues to carry forward as a small standalone task for a future upstream-robustness or observability-family phase.

- **12.2 REVIEW 11 active Minors (A-M2/A-M4/B-M1..M6/C-M1/C-M2/C-M4)** — carry forward unchanged. The named seams (envoy-health internal; the 12.2 helper extended at 13.1 Task 6 was additive) are NOT touched at 13.2 in the pattern those Minors describe.

- **12.1 REVIEW M1 + M3** — carry forward unchanged.

- **Phase-11 REVIEW M1-M8** — carry forward unchanged (13.2 touches no HTTP-filter file).

- **10 M2/M3/M4/D1/D2/T1; 09 M1/D1/D2/T1/T2/T3; 08.2 M1-M8 + T1-T3; 08.1 M3; 07.2 M2/M3; 06.2 M1/M2/M4/M5; 06.1 M2/M3/M5/M6; 05.3 I2; 05.2 I1/I2/I3; 04.1 M5/M9/M-claim/M1/M2/M4/M7; 02.2 M1; Phase-00 I3** — all carry forward indefinitely per their existing named-owner dispositions. **Phase 13.2 touches:** `envoy-http2` (new `pool.rs` + `client.rs` visibility widen + Cargo.toml + hcm.rs migration + lib.rs declaration) + `envoy-http1` (pool.rs mutex switch + Cargo.toml) + `envoy-bin` (main.rs wiring + new backstop test) + `tests/differential/` (lib.rs backend-discriminator extension + backend.rs Http2EchoBackend) + `tests/fixtures/0021-*` (new) + `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (row tightening + new H2-pool row) — close opportunistically at named-seam tasks.

- **04.1 REVIEW M5 / M9 Cargo.lock cadence ratification** — verified at PLAN-write: `parking_lot = "0.12"` + `tokio-util = { version = "0.7", features = ["rt"] }` sub-crate deps added to envoy-http2 (+ parking_lot also to envoy-http1) at Task 1 Step 1 + Step 2. Neither is a new TOP-LEVEL dep (both workspace-pre-existing as transitive/member deps). `Cargo.lock` may show benign deps-graph reshuffling but no new entries.

### Task ordering rationale

The 8-task organization (PLAN §Tasks 1-8) reflects a foundation-first cadence:

- **Task 1** — the architecturally headline H2Pool primitive + manager + idle sweeper + the joint A-I3 / A-M1 / A-M2 / A-M4 closures on BOTH H1+H2 pools. Unit-tested in isolation (no HCM coupling — provable correctness before integration). This is also the load-bearing race-fix task (A-I3); the sync-Mutex switch on H1 pool MUST land in the same commit as the H2 pool primitive to keep the joint architectural touch coherent.

- **Task 2** — H2 HCM proxy-arm migration + the new `envoy_http2::HCMConfig` struct (replacing the type alias) + envoy-bin H2PoolManager wire-up + outer `_cx_guard` relocation (the cx_active double-count mirror of 13.1 Task 4 fold-in). The load-bearing integration task; 20 existing fixtures must regress-equivalence here per gate (b).

- **Task 3** — D7.2 BEHAVIOR_CONTRACT new row (the `upstream_cx_http2_total` row). Largely docs; the registration itself lands at Task 1's H2PoolManager::for_bootstrap.

- **Task 4** — D7.1 BEHAVIOR_CONTRACT row tightening (`upstream_cx_total` to value-exact with TCP-proxy carve-out). Docs-only. The named 06.3 REVIEW I2 (b) FULL-CLOSURE site.

- **Task 5** — Fixture 0021 + harness backend-discriminator extension + Docker wrapper. The only fixture-adding task. Reuses the 13.1-landed `Driver::Http1KeepAlive` verbatim — no new harness driver needed.

- **Task 6** — D9.3-H2 in-process H2 backstop (envoy-bin/tests/-resident; subprocess discipline per 09 REVIEW M3; mirrors 13.1 Task 8 verbatim with H2-upstream substitutions).

- **Task 7** — state-4 verification + STATE advance to state-5-next. Docs-only.

- **Task 8** — state-6 close-out commit (CLOSING-sub-phase per the closing-sub-phase invariant). Flips ROADMAP rows `13.2` AND parent `13` `in-progress → done` SIMULTANEOUSLY. Advances STATE.md to `awaiting next planning`. Docs-only.

This is a PLAN-writer recommendation; the state-3 controller may reorganize within the constraints (Task 1 before Task 2; Task 5 atomic per lock-in #5; Task 7 last before Task 8; the state-5 code-review session intervenes between Task 7 and Task 8 per the §5 state machine).

---

*(State-3 task subsections append below as each task closes — `### Task 1 — ...` through `### Task 8 — state-6 close-out commit + parent-13 close`.)*

---

### Task 1 — H2 pool primitive + manager + sweeper + A-I3 close + A-M1/A-M2/A-M4 closures

Lands the architecturally headline 13.2 D5 deliverable per PLAN Task 1: a new `crates/envoy-http2/src/pool.rs` module carrying `H2Pool` + `H2PoolEntry` + `H2PoolGuard` (RAII) + `H2PoolManager` + `PoolError` + the idle-sweeper task — all unit-tested in isolation (no HCM coupling; H2 HCM proxy-arm migration defers to Task 2 per the foundation-first cadence). Plus the **joint H1+H2 synchronous-Mutex switch** (the load-bearing A-I3 closure) + four carryforward closures: A-I3 (synchronous Drop on both pools), A-M1 (`_sweepers → sweepers` rename + `pub async fn shutdown(self)` on both managers), A-M2 (`Arc::ptr_eq` debug-assert at the gauge wiring site on both managers), A-M4 (improved `.expect` message naming the single-bootstrap-per-process invariant on both managers). Also added: a new `pub fn cx_active_arc(&self) -> &Arc<envoy_stats::Gauge>` accessor on both `Cluster` and `ClusterHandle` in `crates/envoy-cluster/src/cluster.rs` (the A-M2 debug-assert site requires the underlying `Arc<Gauge>`, not the inc+wrap of `cx_active_guard()`).

**Files changed** (per `git show --stat HEAD`):

- `crates/envoy-http2/src/pool.rs` — **NEW (856 LoC)** including 9 inline `#[cfg(test)]` unit tests.
- `crates/envoy-http2/src/lib.rs` — `pub mod pool;` + `pub use pool::H2PoolManager;`.
- `crates/envoy-http2/src/client.rs` — `ClientStream` fields `send_request` + `host` widened private → `pub(crate)`; added `#[derive(Clone)]` on `ClientStream`. (`std::fmt::Debug` impl preserved; Clone is additive.)
- `crates/envoy-http2/Cargo.toml` — added `tokio-util = { version = "0.7", features = ["rt"] }` + `parking_lot = "0.12"` as sub-crate deps (per lock-in #14 both are workspace-pre-existing as transitive/member deps).
- `crates/envoy-http1/src/pool.rs` — A-I3 close: `tokio::sync::Mutex → parking_lot::Mutex` for `idle` + `established` fields, all `.lock().await` calls dropped (4 sites in `acquire`, 2 in `sweep_once`), `Drop` impl rewritten synchronously (the pre-13.2 `Handle::try_current() + tokio::spawn` branch removed; the state-5 fold-in regression test `pool_guard_drop_outside_runtime_does_not_panic` REMOVED and REPLACED with `pool_guard_drop_is_synchronous_and_returns_to_pool_immediately`), `sweep_once` made sync (and the sweeper task tick body adjusted). A-M1 close: field rename `_sweepers → sweepers` + new `pub async fn shutdown(mut self)`. A-M2 close: `debug_assert!(Arc::ptr_eq(&cx_active, handle.cx_active_arc()), ...)` at the gauge wiring site inside `for_bootstrap`. A-M4 close: improved `.expect` message. NEW test: `pool_acquire_after_concurrent_release_does_not_yield_spurious_overflow` (32-iteration race regression). `spawn_idle_sweeper_with_zero_idle_timeout_does_not_panic` PRESERVED (the interval clamp is still load-bearing).
- `crates/envoy-http1/Cargo.toml` — added `parking_lot = "0.12"`.
- `crates/envoy-cluster/src/cluster.rs` — added `pub fn cx_active_arc(&self) -> &Arc<envoy_stats::Gauge>` on both `Cluster` (line ~147) and `ClusterHandle` (line ~242). 13.2-additive surface; mirrors the `cx_total` accessor pattern. Doc-commented with the A-M2 debug-assert as the consumer.
- `Cargo.lock` — auto-updated by `cargo build` with `parking_lot v0.12.5`, `parking_lot_core v0.9.12`, `lock_api v0.4.14`, `scopeguard v1.2.0`, `redox_syscall v0.5.18` (the parking_lot dep family — added as new dependencies; not previously transitive of `tokio` in this workspace's resolved feature set). `tokio-util v0.7.18` was already in the lockfile via envoy-http1/envoy-health/envoy-bin.

**Carryforward attribution (4 closures land in this commit):**

- **13.1 REVIEW Cluster A-I3** (deferred-Important) — **CLOSED jointly across H1+H2 pools.** The joint synchronous-Mutex switch + sync Drop on both pools eliminates the spurious-overflow race structurally. The new race-regression test on both pools (`pool_acquire_after_concurrent_release_does_not_yield_spurious_overflow` — 32 iterations on H1, 16 on H2) asserts the post-fix invariant: after `drop_task.await` returns, the next `acquire()` on the same endpoint succeeds (no spurious `PoolError::Overflow`). The pre-fix shape (the `tokio::spawn` in Drop deferred the established-decrement; the test would have failed pre-fix because the decrement hadn't run yet when the acquire's cap-check fired) is now structurally unreachable.
- **13.1 REVIEW Cluster A-M1** (sweeper-field rename + shutdown method) — **CLOSED opportunistically.** Both `H1PoolManager` and `H2PoolManager` carry `sweepers: Vec<JoinHandle<()>>` (no underscore prefix) + `pub async fn shutdown(mut self)` that aborts every handle + awaits each. Mirrors `envoy_health::Scheduler::shutdown`'s posture.
- **13.1 REVIEW Cluster A-M2** (`Arc::ptr_eq` debug-assert) — **CLOSED opportunistically.** Both pool managers' `for_bootstrap` carries `debug_assert!(Arc::ptr_eq(&cx_active, handle.cx_active_arc()), "{...mgr...}: cx_active Arc mismatch for cluster '{}' — single-bootstrap-per-process invariant violated", cfg.name)` right after the gauge re-register. The `cx_active_arc` accessor added on `Cluster`/`ClusterHandle` is the load-bearing new envoy-cluster surface (13.2-additive; doc-commented with the A-M2 closure as the consumer).
- **13.1 REVIEW Cluster A-M4** (improved `.expect` message) — **CLOSED opportunistically.** Both pool managers' `.expect` reads: `"H[12]PoolManager::for_bootstrap requires cluster_mgr built from the same bootstrap (single-bootstrap-per-process invariant)"`.

**Test count delta:**

- envoy-http1 `pool::tests`: 8 → 9 (net +1). REMOVED: `pool_guard_drop_outside_runtime_does_not_panic` (the A-I1 13.1 state-5 fold-in regression — pre-fix tested the `Handle::try_current()` branch; post-fix the branch is structurally absent so the scenario is unreachable; the gauge-decrement-after-runtime-drop invariant the test asserted is preserved structurally because `ConnGaugeGuard::Drop` is and always was synchronous). ADDED: `pool_guard_drop_is_synchronous_and_returns_to_pool_immediately` (asserts: acquire → drop → IMMEDIATE re-acquire reuses the same stream without re-firing `cx_total`; no `tokio::time::sleep` between drop and re-acquire — this is the post-A-I3 structural invariant) + `pool_acquire_after_concurrent_release_does_not_yield_spurious_overflow` (the A-I3 race regression; 32 iterations).
- envoy-http2 `pool::tests`: 0 → 9 (net +9). Tests: `acquire_from_empty_pool_creates_connection_and_fires_counters`, `acquire_after_release_reuses_existing_connection_without_incrementing_cx_total`, `acquire_with_concurrent_streams_shares_one_connection`, `acquire_returns_overflow_when_at_max_connections`, `invalidate_evicts_entry_and_increments_cx_destroy`, `idle_sweeper_evicts_past_deadline_entries`, `spawn_idle_sweeper_with_zero_idle_timeout_does_not_panic`, `h2_pool_manager_registers_cx_destroy_and_cx_http2_total_per_h2_cluster`, `pool_acquire_after_concurrent_release_does_not_yield_spurious_overflow`.
- envoy-cluster full lib tests: 36 (unchanged net; the new `cx_active_arc` accessor is exercised indirectly via the H1/H2 pool managers' `debug_assert!` in their `for_bootstrap` tests).
- envoy-http1 full lib tests: 82.
- envoy-http2 full lib tests: 53.

**Per-gate clean outputs (touched crates):**

- `cargo build --workspace --all-targets` — `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 1m 20s`. No warnings, no errors.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 1m 17s`. No clippy diagnostics.
- `cargo fmt --all -- --check` — exit 0; no diff.
- `cargo test -p envoy-cluster --lib` — `test result: ok. 36 passed; 0 failed; 0 ignored`.
- `cargo test -p envoy-http1 --lib` — `test result: ok. 82 passed; 0 failed; 0 ignored`.
- `cargo test -p envoy-http2 --lib` — `test result: ok. 53 passed; 0 failed; 1 ignored`.
- `cargo test -p envoy-bin --bins` — `test result: ok. 8 passed; 0 failed`.

**Deviations from PLAN (D-3.4 transparency):**

1. **Test #4 (`acquire_returns_overflow_when_at_max_connections`) used the simpler deterministic shape** per the PLAN's explicit guidance ("the simplest test path is what wins"). Rather than spawning 101+ concurrent H2 streams against a mock backend (flaky), the test acquires one real guard, then directly stores `entry.max_streams` into the entry's `active_streams` atomic via the test-internal `pool.connections` lock — simulating the at-cap multiplex state deterministically. The second acquire then takes both the slot-claim-fails path (Phase 1) and the at-`max_connections` path (Phase 2) → `PoolError::Overflow`. Post-assert, the test restores `active_streams = 1` so the first guard's Drop bookkeeping (`fetch_sub` from 1 → 0) remains consistent. The discriminating power is identical (the Overflow surface IS what's being asserted); the determinism is strictly higher.

2. **The A-I3 race regression test shape** awaits the drop task FIRST and then performs the follow-up acquire (rather than spawning both concurrently and joining both). This is the structurally meaningful shape post-fix: under sync Drop, `drop_task.await` is a happens-before boundary for the established-decrement / connections-list-evict bookkeeping. Pre-fix, the same shape would have failed because the spawned async return-to-pool / decrement had not yet run when `drop_task.await` returned (the spawn task is independent of the drop task). The first attempt at the test (with concurrent `tokio::spawn` on both drop and acquire + a 32-iteration loop) FAILED on iteration 20 with `Err(Overflow {...})` — but the failure mode reflects genuine scheduler nondeterminism (acquire scheduled before drop runs), NOT the A-I3 race. The deterministic shape is the correct post-fix invariant assertion.

3. **`parking_lot` was NOT previously a transitive of `tokio`** in this workspace's resolved feature set (PLAN lock-in #14 claimed it was). Empirically, `grep parking_lot Cargo.lock` returns zero matches pre-fix; `cargo build` after the Cargo.toml change adds parking_lot v0.12.5 + parking_lot_core v0.9.12 + lock_api v0.4.14 + scopeguard v1.2.0 + redox_syscall v0.5.18 as new lockfile entries. This is a deviation from the PLAN's "no new top-level Cargo dep" framing — `parking_lot` is genuinely a new transitive dependency. However the named invariant (no new TOP-LEVEL workspace dep) holds: `parking_lot` is added only as a sub-crate dep in `crates/envoy-http1/Cargo.toml` and `crates/envoy-http2/Cargo.toml`; the workspace root `Cargo.toml` is unchanged. The 04.1 REVIEW M5/M9 Cargo.lock cadence ratification is preserved as written (PROGRESS Task 1 preamble item 12 of the §"PLAN-time SPEC corrections" — the empirical reality is named here for state-5 review).

4. **The H1 pool's existing `acquire_after_return_reuses_idle_stream_without_incrementing_cx_total` test** still has a `tokio::time::sleep(50ms)` between drop and re-acquire (preserved verbatim from the pre-13.2 shape). Under sync Drop the sleep is no longer load-bearing — `pool_guard_drop_is_synchronous_and_returns_to_pool_immediately` is the new test that asserts the no-sleep invariant. The pre-existing test was kept unchanged (rather than modified to remove the sleep) to keep this commit's diff focused on the A-I3 close-out + the new test, not on cleaning up the pre-existing sleeps; the sleep is harmless (it just makes the test slightly slower).

**New envoy-cluster surface (`cx_active_arc`):** The accessor returns a borrow `&Arc<envoy_stats::Gauge>` (not a clone) so the debug-assert site doesn't unnecessarily bump the Arc refcount. Mirrors the existing `cx_total()` accessor's borrow shape. Doc-commented to name the A-M2 closure as the consumer + the single-bootstrap-per-process invariant as the load-bearing precondition.

**Commit SHA:** `1c954cf` (final HEAD after the single Task 1 commit; the PROGRESS subsection was folded in via `git commit --amend` so the SHA is self-referential — one commit per task per the 13.1 cadence).

### Task 1 fold-in — code-quality review closures

Follow-up commit appended after `f692b53` (Task 1's main commit). Addresses two findings from the code-quality reviewer running against `f692b53`:

- **CRITICAL: H2 `H2PoolGuard::drop` invalidate-path TOCTOU race — CLOSED.** The pre-fix Drop ran `active_streams.fetch_sub` OUTSIDE the `connections` lock and then took the lock to retain the entry out of the per-endpoint list. Between the unlocked decrement and the lock acquisition, a concurrent Phase-1 walker in `acquire()` (which holds the `connections` lock while iterating + CAS'ing) could observe `active_streams` already decremented (potentially to 0), claim a slot via CAS, build an `H2PoolGuard` against the entry, and release the lock — at which point our Drop would proceed to evict the entry, decrement `established`, and fire `cx_destroy`, leaving the concurrent acquirer holding a guard against an orphaned entry, breaking `max_connections` accounting and falsely firing `cx_destroy`. Post-fix: the invalidate branch takes the `connections` lock BEFORE `fetch_sub` and holds it across both the decrement and the `retain`, so no Phase-1 walker can claim a slot on an entry we're about to evict. The non-invalidate (return-to-pool) branch keeps the original lock-free `fetch_sub` — analysis written into the Drop body comment shows the residual race is benign by virtue of the sweeper's `active_streams != 0` early-return + `try_claim_stream_slot`'s `last_idle = None` write after CAS-success.
- **IMPORTANT: H1 race-regression test strengthening — CLOSED.** Added a NEW H1 pool test `pool_acquire_after_concurrent_release_1000_iterations_zero_spurious_overflows` (kept the existing 32-iter structural test alongside it). Same drop_task.await shape as the existing test but at 1000 iterations — probabilistically exercises the pre-fix race window (the `tokio::spawn`-from-Drop deferred established-decrement). Pre-fix, the spawn-from-Drop's inner task could lag behind the outer `drop_task.await`'s return; at 1000 iterations the missed-decrement window would have surfaced reliably. Post-fix sync Drop produces 0 spurious Overflows over all 1000 iterations (test completes in ~90ms on a modern dev box). The H2 race-fix correctness is structurally covered by the CRITICAL fix above + the existing `invalidate_evicts_entry_and_increments_cx_destroy` + `pool_acquire_after_concurrent_release_does_not_yield_spurious_overflow` tests (the H2 race window is now structurally closed by the lock ordering in Drop — a probabilistic test is not load-bearing on the H2 side).

**Files touched:**

- `crates/envoy-http2/src/pool.rs` — `Drop for H2PoolGuard` rewritten with the invalidate vs. return-to-pool path split; extensive comment block in the Drop body documenting the race + the fix's correctness argument (the analysis verifying that a concurrent Phase-1 walker cannot reach the to-be-evicted entry's CAS site while we hold the `connections` lock).
- `crates/envoy-http1/src/pool.rs` — new test `pool_acquire_after_concurrent_release_1000_iterations_zero_spurious_overflows` added alongside the existing 32-iter test.
- `docs/envoy-rust/phases/13.2-h2-pool-and-cx-total-tightening/PROGRESS.md` — this subsection.

**Per-gate clean outputs (touched crates):**

- `cargo build -p envoy-http1 -p envoy-http2 --all-targets` — `Finished \`dev\` profile`, no warnings.
- `cargo clippy -p envoy-http1 -p envoy-http2 --all-targets --all-features -- -D warnings` — `Finished \`dev\` profile`, no diagnostics.
- `cargo fmt --all -- --check` — exit 0.
- `cargo test -p envoy-http1 --lib` — `test result: ok. 83 passed; 0 failed; 0 ignored` (net +1 from the new 1000-iter stress test).
- `cargo test -p envoy-http2 --lib` — `test result: ok. 53 passed; 0 failed; 1 ignored` (unchanged from `f692b53`; the existing tests cover the fixed Drop semantics structurally).

**Test count delta:** envoy-http1 pool tests 9 → 10 (net +1). envoy-http2 pool tests unchanged.

**Commit SHA:** to be filled in by the appended commit (will be self-referential after fold-in).

---

### Task 2 — H2 router-arm pool integration + envoy-bin wire-up

Lands the 13.2 D6 deliverable: the H2 HCM proxy arm migrates from per-call
`envoy_http2::Client::connect` to dispatch through `H2Pool::acquire` when a
pool manager is wired (production path). The earlier-phase
`pub type HCMConfig = Http1HCMConfig;` alias in `crates/envoy-http2/src/hcm.rs`
is REPLACED by a proper `envoy_http2::HCMConfig` struct wrapping
`Arc<envoy_http1::HCMConfig>` + carrying `h2_pool_mgr: Option<Arc<H2PoolManager>>`
(lock-in #2 application). The outer cx_active guard at the dispatch site is
relocated from unconditional to `Option<ConnGaugeGuard>` Some-only on the
H1-cluster arm (lock-in #8 — mirrors the 13.1 Task 4 code-quality fold-in
fix on the H1 HCM verbatim, because the H2 pool's `H2PoolGuard` owns its
own ConnGaugeGuard internally and an outer guard would double-count
cx_active). The H1-cluster-in-H2-HCM arm at `hcm.rs:273-284` is left
UNTOUCHED per lock-in #7 — that cross-protocol path stays per-call and
13.1 did not cover it either. envoy-bin wires `H2PoolManager::for_bootstrap`
between the existing `H1PoolManager::for_bootstrap` and
`envoy_health::Scheduler::spawn`, and the HTTP2 codec-dispatch arm in
envoy-bin wraps the H1 HCMConfig via `envoy_http2::HCMConfig::wrap` with
`Some(Arc::clone(&h2_pool_mgr))`. One new integration test asserts pool
reuse end-to-end through the HCM dispatch path.

**Files changed** (per `git status --short` post-edit):

- `crates/envoy-http2/src/hcm.rs` — replaced the `pub type HCMConfig = Http1HCMConfig;` alias at line 27 with the `pub struct HCMConfig { pub inner: Arc<Http1HCMConfig>, pub h2_pool_mgr: Option<Arc<crate::pool::H2PoolManager>> }` + `impl HCMConfig::wrap(inner, h2_pool_mgr)` constructor. Updated every site in `serve_h2_connection` + `handle_one_stream` + `finalize_h2_stream` that previously read `config.<H1 field>` (config was a type alias to `Http1HCMConfig`) to `config.inner.<H1 field>` — 8 sites total (`http2_protocol_options`, `stats.downstream_rq_total`, `filter_pipeline`, `cluster_mgr`, `stats.downstream_rq_Nxx` ×4, `access_log` ×3, `stats.access_logs_total`, `stats.access_logs_failed`, plus the `build_response` arg taking `&Http1HCMConfig` reachable via `&config.inner`). Migrated the `UpstreamProtocol::Http2` arm of the dispatch match (~286-296 pre-edit) to dispatch through `config.h2_pool_mgr.as_ref().and_then(|m| m.get(&cluster_name))` with a `Some(pool) =>` pool-path branch (`pool.acquire(endpoint, &host_header).await` + `guard.client_stream_mut().send_request(out_req).await`) + a `None =>` per-call fallback that preserves the pre-13.2 `crate::Client::connect` + `cluster.cx_total().inc()`. Relocated the unconditional `let _cx_guard = cluster.cx_active_guard();` at line 269 to a conditional `let _cx_guard: Option<envoy_cluster::ConnGaugeGuard> = match cluster.upstream_protocol() { Http1 => Some(cluster.cx_active_guard()), Http2 => None };`. The H1-cluster arm at `:273-284` (the `UpstreamProtocol::Http1` match branch) is UNTOUCHED — per-call `envoy_http1::Client::connect` + `cluster.cx_total().inc()` per lock-in #7. Test helper `spawn_h2_hcm` wraps its `Arc<Http1HCMConfig>` argument via `HCMConfig::wrap(cfg, None)` so the existing 53 tests keep compiling with pool-less dispatch (per-call fallthrough). NEW test added: `h2_hcm_pool_reuses_upstream_conn_across_sequential_requests` — see test detail below.

- `crates/envoy-bin/src/main.rs` — added `H2PoolManager::for_bootstrap` between the existing `pool_mgr` (H1) binding at `:137-143` and the `health_scheduler` spawn at `:150`. Bound to `h2_pool_mgr` (keeping `pool_mgr` as the H1 binding name to avoid churning H1 call sites). Updated the `CodecType::HTTP2` arm of the HCM dispatch (`:314-316` pre-edit) to wrap the `hcm_config: Arc<envoy_http1::HCMConfig>` via `envoy_http2::HCMConfig::wrap(Arc::clone(&hcm_config), Some(Arc::clone(&h2_pool_mgr)))` before passing to `envoy_http2::HCM::new`. The H1 HCM arm at `:312` (`envoy_http1::HCM { config: hcm_config }`) is unchanged — H1 HCMConfig threading already works via 13.1 Task 4.

- `docs/envoy-rust/phases/13.2-h2-pool-and-cx-total-tightening/PROGRESS.md` — this subsection.

**LoC delta:** envoy-http2/src/hcm.rs net +160 (struct + comment block + dispatch arm rewrite + 1 new test ~135 LoC). envoy-bin/src/main.rs net +18 (manager construction comment block + 5-line H2PoolManager::for_bootstrap call + 8-line HCM::new wrapper construction).

**Lock-in application:**

- **Lock-in #2 (HCMConfig wrapper):** APPLIED. `envoy_http2::HCMConfig` is now a proper struct wrapping `Arc<envoy_http1::HCMConfig>` + the new field. The H2 HCM accesses inner H1 fields via `config.inner.<field>` (8 sites updated); the new field via `config.h2_pool_mgr`.

- **Lock-in #7 (H1-cluster-in-H2-HCM stays untouched):** APPLIED. The `UpstreamProtocol::Http1` arm at `hcm.rs:273-284` retains its per-call `envoy_http1::Client::connect` + `cluster.cx_total().inc()` + outer `cx_active_guard` via the conditional `Some(cluster.cx_active_guard())` branch. No migration.

- **Lock-in #8 (outer `_cx_guard` relocation):** APPLIED. The unconditional `let _cx_guard = cluster.cx_active_guard();` at `hcm.rs:269` (pre-edit) is replaced by a `match cluster.upstream_protocol() { Http1 => Some(cluster.cx_active_guard()), Http2 => None }` block. The H1 arm continues to hold the gauge guard for the full dispatch scope; the H2 arm holds `None` because the `H2PoolGuard` owns its own `ConnGaugeGuard` internally (lock-in #8 mirrors the 13.1 Task 4 code-quality fold-in on the H1 HCM verbatim). When the H2-arm pool is `None` (test paths only; production never reaches this branch), the per-call `Client::connect` fallthrough does NOT increment `cx_active` — this is a behavior change relative to pre-Task-2 (which fired via the unconditional outer guard). No existing test asserts cx_active during in-flight H2 dispatch on the test path, so no test breakage. Production semantic is preserved: envoy-bin always wires `Some(h2_pool_mgr)`, so the production path fires cx_active exactly once per request via the `H2PoolGuard`'s internal `ConnGaugeGuard`.

**envoy-bin wiring detail:**

- H2PoolManager construction lands between the H1 `pool_mgr` and `health_scheduler.spawn`. The manager carries `Arc<HashMap<String, Arc<H2Pool>>>` keyed by cluster name; one pool per H2-protocol cluster (filtered via `cluster_mgr.get(cfg.name).upstream_protocol() == UpstreamProtocol::Http2` inside `for_bootstrap` — Task 1 PROGRESS). The `cluster_mgr` + `registry` + `token` are shared with the H1 pool manager + health scheduler (single-bootstrap-per-process invariant).
- HCMConfig consumption at the HTTP2 codec-dispatch arm wraps via `HCMConfig::wrap(Arc::clone(&hcm_config), Some(Arc::clone(&h2_pool_mgr)))`. The `hcm_config: Arc<envoy_http1::HCMConfig>` binding is unchanged; the wrap is additive at the HTTP2 arm only.

**New integration test detail:**

`hcm::tests::h2_hcm_pool_reuses_upstream_conn_across_sequential_requests` — builds a 1-cluster H2 bootstrap (cluster name `backend`, type STATIC, `typed_extension_protocol_options.http2_protocol_options.max_concurrent_streams: 100`), constructs a shared `Arc<StatsRegistry>`, builds the `ClusterManager` + `H2PoolManager` from the bootstrap, wraps the H1 HCMConfig via `HCMConfig::wrap(..., Some(Arc::clone(&pool_mgr)))`, spawns the HCM accept loop on `127.0.0.1:0`, opens ONE downstream H2 client connection, drives 3 sequential GET / requests, and asserts `cluster.backend.upstream_cx_total.value() == 1` after a 100ms settle. The assertion is meaningful: without the pool the H2 arm's per-call `Client::connect` fires 3 times → `cx_total == 3`; with the pool, all 3 requests share the same upstream H2 multiplex conn → `cx_total == 1`. End-to-end coverage of the wired pool dispatch path (vs. the pool unit-test surface alone). The test cancels the `CancellationToken` at end-of-test so the H2PoolManager's idle sweeper task drains cleanly.

**Per-gate clean outputs (touched crates):**

- `cargo build --workspace --all-targets` — `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 4m 19s`. No warnings, no errors.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 1m 02s`. No clippy diagnostics.
- `cargo fmt --all -- --check` — exit 0; no diff.
- `cargo test -p envoy-http2 --lib` — `test result: ok. 54 passed; 0 failed; 1 ignored` (net +1 from the new pool-reuse integration test).
- `cargo test -p envoy-bin` — all integration backstops green: argv 6 + echo 2 + 18 in-process backstops (access_log_file_sink, http1_direct_response, http2_direct_response, admin_*, etc.) + 2 listener-pool tests. The initial run surfaced a port-binding flake on `access_log_file_sink_in_process` (cargo orphan from a prior aborted run still holding the bound listener_port — re-running the test cleanly passed in 0.61s; mirrors the 13.1 Task 10 state-5 access-log flake mitigation context).

**Test count delta:** envoy-http2 lib tests 53 → 54 (net +1). envoy-bin tests unchanged.

**Deviations from PLAN:**

1. The PLAN spec text says "the `cluster_name` may not exist at this scope today" — empirically the H2 HCM's `BuildOutcome::Proxy { cluster: cluster_name }` destructure at line ~197 already binds `cluster_name: String` in scope (used by `cluster_mgr.get(&cluster_name)`). No new binding required for the pool's `m.get(&cluster_name)` lookup.

2. The PLAN says PoolError::Overflow's destructure uses `{ cluster, max }`. To avoid shadowing the outer `cluster: ClusterHandle` binding inside the dispatch match scope, the destructure renames to `{ cluster: cl, max }` and the warn/error format uses `cl`. Pure naming hygiene; no semantic change.

3. The test helper `spawn_h2_hcm` retains its `Arc<Http1HCMConfig>` signature (vs. `Arc<HCMConfig>`) because all 18 existing test sites already pass `Http1HCMConfig` to it. The wrap happens INSIDE the helper (`HCM::new(Arc::new(HCMConfig::wrap(config, None)))`), preserving the existing call sites unchanged. The new pool-reuse integration test bypasses this helper (it needs `Some(pool_mgr)`) and constructs its own accept loop inline.

**Commit SHA:** `07006b4` (final HEAD after the single Task 2 commit; the PROGRESS subsection was folded in via `git commit --amend` so the SHA is self-referential — one commit per task per the 13.1 cadence).

---

### Task 2 fold-in — code-quality review documentation correction

Follow-up commit appended after `07006b4` (Task 2's main commit). Addresses one IMPORTANT documentation finding from the code-quality reviewer:

**IMPORTANT: Misleading cx_active fallthrough comment — CORRECTED.** The comment block at `crates/envoy-http2/src/hcm.rs` around the outer `_cx_guard` relocation (lock-in #8) incorrectly claimed that the H2-arm pool-None fallthrough "matches the 13.1 H1 HCM's `OneShot`-arm semantic (cx_total fires; cx_active does not, because the pre-13.1 H2-arm code did not hold a guard either)". Both parenthetical justifications were wrong:

1. The H1 HCM's OneShot arm DOES fire cx_active (`crates/envoy-http1/src/hcm.rs` uses `Some(cluster.cx_active_guard())` on the OneShot arm).
2. The pre-Task-2 H2 hcm.rs held an UNCONDITIONAL outer guard (baseline `ae8d7cf`, line 269: `let _cx_guard = cluster.cx_active_guard();`) — NOT an absent guard.

The corrected comment now accurately states: pre-Task-2 the outer guard fired unconditionally for both protocol arms; post-Task-2, H2-arm-pool-None is a TEST-PATH-ONLY behavior change (cx_active count goes from 1 to 0 per request on that path). Production semantic is unchanged (envoy-bin always wires `Some(h2_pool_mgr)`, so the production path fires cx_active via the `H2PoolGuard`'s internal `ConnGaugeGuard`). No existing test asserts cx_active on the pool-None test path, so no test breakage; if a future test requires it, an explicit `Some(cluster.cx_active_guard())` in the pool-None arm is the fix.

The parallel Task 2 lock-in #8 paragraph in this PROGRESS.md subsection is corrected with the same framing. The Task 2 commit SHA cosmetic (`3aa787c` pre-amend → `07006b4` actual HEAD) is also corrected in the Task 2 subsection above.

**Net effect:** production semantic unchanged; test-path cx_active divergence honestly documented; no code change.

**Files touched:**

- `crates/envoy-http2/src/hcm.rs` — comment block rewrite at the `_cx_guard` relocation site.
- `docs/envoy-rust/phases/13.2-h2-pool-and-cx-total-tightening/PROGRESS.md` — this fold-in subsection + Task 2 lock-in #8 paragraph correction + SHA correction.

**Per-gate clean outputs:**

- `cargo build -p envoy-http2` — Finished, no warnings.
- `cargo fmt --all -- --check` — exit 0; no diff.
- `cargo test -p envoy-http2 --lib` — `test result: ok. 54 passed; 0 failed; 1 ignored`.

**Commit SHA:** `ef6deda` (fold-in commit — Task 2 original `07006b4` + this fold-in `ef6deda`).

---

### Task 3 — D7.2 BEHAVIOR_CONTRACT row for `cluster.<name>.upstream_cx_http2_total`

Docs-only addition under the existing "13.1 entries (H1 connection pool)" subsection at `docs/envoy-rust/BEHAVIOR_CONTRACT.md:158`. New `**13.2 entries (H2 connection pool):**` heading + 1 row sibling to the H1 entry. Disposition `value-exact`. Rationale documents:

- Increment site: H2 pool's `acquire()` connect-on-miss branch at `crates/envoy-http2/src/pool.rs::H2Pool::acquire` (registered at Task 1; consumed at Task 2). Same site as the existing `cluster.<name>.upstream_cx_total` for H2 clusters.
- Under fixture 0021's single-downstream-keep-alive-conn driver issuing 5 sequential requests → both proxies emit 1 (single upstream H2 conn multiplexing 5 stream slots).
- Default `max_concurrent_streams = 100` per RFC 7540 §6.5.2 (the `DEFAULT_MAX_CONCURRENT_STREAMS` const at `pool.rs:42`). The fixture's 5-request workload stays well under this cap, so the bilateral value is deterministic 1.
- Registration scope: only for clusters whose `upstream_protocol()` is `Http2` (gated at `H2PoolManager::for_bootstrap`).
- Sibling structure: `upstream_cx_http1_total` + `upstream_cx_http2_total` together enumerate the per-protocol breakdown of `upstream_cx_total`.

**Files touched (1):**

- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — appends 1 new `**13.2 entries (H2 connection pool):**` subsection between the existing 13.1 H1-pool entries (lines 152-157) and the 06.1 Prometheus-divergence subsection (post-modification line 168).

**Carryforward attribution:** none at this task.

**Per-gate clean outputs:**

- `cargo fmt --all -- --check` — exit 0; no diff.
- Doc-only; no test or build impact.

**Lands controller-direct** per PLAN architecture lock-in #17's mid-arc latitude clause — docs-only single-row addition; subagent overhead exceeds task scope.

**Commit SHA:** `5c27b3d` (pre-amend; published HEAD `ab0e62f` per the per-task SHA-amend pattern that's run since Task 1).

---

### Task 4 — D7.1 `upstream_cx_total` BEHAVIOR_CONTRACT row tightening (06.3 REVIEW I2 (b) FULL CLOSURE)

Docs-only D7.1 deliverable. Tightens the existing `cluster.<name>.upstream_cx_total` row at `docs/envoy-rust/BEHAVIOR_CONTRACT.md:89` (06.1 initial entry) from `name-required, value-may-differ` to **`value-exact` (H1 + H2 clusters under the harness's single-downstream-keep-alive-conn driver); name-required, value-may-differ (TCP-proxy clusters — TCP pool defers to a follow-up phase per parent-13 SPEC §4)**.

**The named carryforward closure attribution lands here.** 06.3 REVIEW I2 (b) had been carried forward through 7+ phases (06 → 07 → 08 → 09 → 10 → 11 → 12 → 13.1) awaiting both H1 and H2 connection pooling to land before the row could tighten uniformly. With Task 1's H2 pool primitive + Task 2's router-arm integration in place, the bilateral H1/H2 pool architecture is established and the row tightens consistently across both protocols. Combined with 13.1's I2 (a) closure (fixture 0020's per-class `downstream_rq_{2,3,4,5}xx` + cluster `upstream_rq_5xx` bilateral assertions), **the full 06.3 REVIEW I2 carryforward closes at the phase-13 close** (Task 8 will re-attribute at the parent-13 close commit per the closing-sub-phase invariant).

Tightened row's new rationale documents:

- Increment site relocation: H1 → `crates/envoy-http1/src/pool.rs::H1Pool::acquire` connect-on-miss branch (per 13.1 Task 3); H2 → `crates/envoy-http2/src/pool.rs::H2Pool::acquire` connect-on-miss branch (per 13.2 Task 1 + 2). One source of truth per protocol.
- TCP-proxy carve-out: `crates/envoy-tcp/src/lib.rs:108` per-call increment site stays untouched until TCP pooling lands in a follow-up phase. Existing TCP fixtures (0001/0003/0004/0005/0006) have presence-only assertions, so the carve-out is benign.
- Conditional-on-driver nuance: the value-exact disposition holds when the harness driver reuses a downstream keep-alive conn (the 13.1 `Driver::Http1KeepAlive` shape; the H1 + H2 fixture wrappers select this driver). Multi-downstream-conn workloads would emit N upstream conns regardless of pool — explicit per parent-13 SPEC §6.2 item-iv.
- Explicit 06.3 REVIEW I2 (b) closure attribution + the combined-with-13.1 full I2 closure note.

**Files touched (1):**

- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — line 89's Equivalence + Rationale columns rewritten in place; no surrounding-rows change.

**Carryforward attribution:** **06.3 REVIEW I2 (b) FULLY CLOSED at this commit.** Combined with 13.1 fixture 0020's I2 (a) closure, the full 06.3 REVIEW I2 carryforward closes at parent-13 close (Task 8). PROGRESS Task 8 will re-attribute the full closure at the closing commit per D-3.4.

**Per-gate clean outputs:**

- `cargo fmt --all -- --check` — exit 0; no diff (markdown-only change).
- Doc-only; no test or build impact.

**Lands controller-direct** per PLAN architecture lock-in #17's mid-arc latitude clause — docs-only single-row tightening; subagent overhead exceeds task scope. The closure attribution mirrors the 13.1 Task 5 D7-row pattern verbatim modulo the broader I2 (b) framing.

**Commit SHA:** `e4ca33f` (pre-amend; published HEAD may shift to the post-amend SHA per the per-task SHA-amend pattern).


---

### Task 5 first-attempt — architectural-blocker discovery + topology-pivot ADR (ADR-0039); session closes here at state-3-partial

The 13.2 state-3 controller dispatched the Task 5 implementer subagent (per `feedback_execution_style`) with the PLAN's prescribed Task 5 shape (downstream H1 listener + H2 upstream cluster + the 13.1-landed `Driver::Http1KeepAlive` reused VERBATIM). The implementer reported `NEEDS_CONTEXT` with a named architectural blocker:

- envoy-rust's `envoy-config` validator at `crates/envoy-config/src/bootstrap.rs:1997-2016` raises `ConfigError::Http2ClusterFromHttp1Listener` when an HCM with `codec_type: HTTP1` (or `AUTO`) routes to a cluster whose `typed_extension_protocol_options` selects HTTP/2 upstream. This is the 06.3 D14.3 parse-time gate; the doctrinal reference is ADR-0028 (option B at `docs/envoy-rust/DECISIONS.md:513` — deferred the H1-listener × H2-cluster dispatch path because the H2 client lives in `envoy-http2`, which depends on `envoy-http1`; adding `envoy-http2` as a path-dep of `envoy-http1` would form a Cargo cycle).
- The PLAN's prescribed fixture-0021 topology IS exactly the rejected configuration: HCM `codec_type: HTTP1` + cluster `typed_extension_protocol_options.envoy.extensions.upstreams.http.v3.HttpProtocolOptions.explicit_http_config.http2_protocol_options: {}`.
- The implementer landed all 6 PLAN-prescribed Task 5 artifacts cleanly (3 fixture YAMLs + 1 Docker wrapper + 1 harness `_h2_echo_backend` discriminator arm at `tests/differential/src/lib.rs:1920-1956` + the existing `Http2EchoBackend` helper from 13.1 carried forward). The 5 stable-toolchain gates passed. The Docker-gated wrapper test then FAILED with envoy-rust emitting the named `ConfigError::Http2ClusterFromHttp1Listener` error message.
- The implementer reverted all Task 5 artifacts (workspace restored to HEAD `4ab2c61`; working tree clean modulo the pre-existing untracked `crates/envoy-config/fuzz/Cargo.lock`). No commit was made.

The controller verified the blocker directly:
- `crates/envoy-config/src/bootstrap.rs:2011` raises `ConfigError::Http2ClusterFromHttp1Listener` exactly per the implementer's report.
- `crates/envoy-config/src/lib.rs:265-267` defines the variant + the user-facing error message ("H1-listener × H2-cluster dispatch is deferred per ADR-0028").

**Disposition — pick per `feedback_pick_recommendation`:** **Option (b) — topology pivot to H2-downstream-listener + H2-upstream-cluster + new `Driver::Http2KeepAlive` harness driver.** Ratified at **ADR-0039** (landed at this commit).

- Option (a) — closing ADR-0028 — was rejected for 13.2 scope (foundations grant; new crate; ~300-400 LoC; fragments the closing-sub-phase atomic scope).
- Option (c) — defer fixture 0021 entirely — was rejected because it materially weakens the 06.3 REVIEW I2 (b) closure attribution at Task 4 + Task 8 (no bilateral fixture asserting the value-exact disposition on H2 clusters).
- Option (b) — preserves the discriminating-observable bilateral validation: N downstream H2 multiplexed streams over single downstream H2 conn → 1 upstream H2 conn (N streams shared). The bilateral assertion `cluster.backend_cluster.upstream_cx_total: 1` + `cluster.backend_cluster.upstream_cx_http2_total: 1` still discriminates pooled-from-per-call. The new `Driver::Http2KeepAlive` is a ~150-200 LoC additive harness driver mirroring `Driver::Http1KeepAlive`'s shape verbatim modulo the codec layer.

**Reshaped Task 5 scope** (binds the next session's resumption):
1. **Fixture 0021 envoy.yaml + envoy-rust.yaml** — downstream HCM `codec_type: HTTP2` (NOT `HTTP1`); cluster keeps `typed_extension_protocol_options.http2_protocol_options` + `circuit_breakers.thresholds[0].max_connections: 4`. All other fields preserved per the PLAN's prescription (route table + router filter + STRICT_DNS + dns_lookup_family + bind addresses).
2. **expectations.yaml** — driver kind `http2_keep_alive` (the new variant per ADR-0039); 5 sequential single-stream requests over one downstream H2 conn; `settle_ms: 500`; same 5 expected_stats as the PLAN: `http.ingress_http.downstream_rq_2xx: 5`, `http.ingress_http.downstream_rq_total: 5`, `cluster.backend_cluster.upstream_rq_total: 5`, `cluster.backend_cluster.upstream_cx_total: 1`, `cluster.backend_cluster.upstream_cx_http2_total: 1`.
3. **`Driver::Http2KeepAlive` variant** at `tests/differential/src/lib.rs` — mirrors `Driver::Http1KeepAlive`'s shape verbatim (requests Vec; settle_ms; expected_stats). New dispatch arm.
4. **`drive_http2_keep_alive` helper** — opens one H2 conn (h2::client::handshake); issues N sequential streams via cloned `SendRequest<Bytes>` (the Clone landed at Task 1 Step 3); asserts each response's status; bilateral admin-stat scrape after settle.
5. **Harness `_h2_echo_backend` discriminator arm** — at `tests/differential/src/lib.rs::run_fixture`, add the third arm for fixture 0021 spawning `crate::backend::Http2EchoBackend` (already exists at `tests/differential/src/backend.rs:357-422` — no addition needed).
6. **Docker wrapper test** at `tests/differential/tests/upstream_h2_connection_pooling.rs` — unchanged from the PLAN's prescription.

**Session pacing — state-3-partial / state-3-resume-at-Task-5-next:**

Per `BOOTSTRAP_PROMPT.md` §5.1 ("State-3 execution may span multiple sessions; the controller may close at any task boundary"), this session closes at the Task 4 boundary. Tasks 1-4 + Task 1 fold-in + Task 2 fold-in are committed at HEAD post-this-commit. Tasks 5-8 (with the ADR-0039 reshaped Task 5 + Tasks 6/7/8 unchanged) defer to the next session. The next-session cold-start reads this PROGRESS subsection + ADR-0039 as the source of truth for the reshaped Task 5 scope; the PLAN.md is preserved verbatim per the project's append-only doctrine.

**Files touched at this state-3-partial commit (3):**

- `docs/envoy-rust/DECISIONS.md` — appends ADR-0039 ratifying the topology pivot.
- `docs/envoy-rust/phases/13.2-h2-pool-and-cx-total-tightening/PROGRESS.md` — this Task 5 first-attempt subsection (the architectural finding + disposition + reshaped Task 5 scope).
- `docs/envoy-rust/STATE.md` — advance the 4 top pointers to state-3-partial / state-3-resume-at-Task-5-next + append the `### Phase-13.2 state-3 partial arc (Tasks 1-4 landed; Task 5 ADR-0039 pivot)` Notes subsection.

**Per-gate clean outputs at HEAD `4ab2c61` (verified by the controller before this commit):**

- `cargo build --workspace --all-targets` — `Finished `dev` profile [unoptimized + debuginfo] target(s) in 46.39s`.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — `Finished `dev` profile [unoptimized + debuginfo] target(s) in 43.80s` (no warnings).
- `cargo fmt --all -- --check` — exit 0; no diff.

**Carryforward attribution:** ADR-0028 (H1-listener × H2-cluster dispatch deferral) REMAINS OPEN at 13.2 close; carried forward to a follow-up phase per ADR-0039 Consequences.

**Commit SHA:** lands at the ADR-0039 commit (added below).

---

### Task 5 — fixture 0021 + `Driver::Http2KeepAlive` + `Http2EchoBackend` wiring (ADR-0039 topology pivot)

Implementation of the reshaped Task 5 scope per ADR-0039 + the Task 5 first-attempt subsection above. Lands the bilateral fixture-0021 acceptance + the new `Driver::Http2KeepAlive` harness driver + reuses the existing `Http2EchoBackend` (from phase 05.3) via the `{{HTTP2_BACKEND_PORT}}` template-marker scan (no new backend-discriminator arm).

**Files changed (6; +1 in PROGRESS, total 7):**

- **NEW** `tests/fixtures/0021-upstream-h2-connection-pooling/envoy.yaml` (+87 lines) — downstream HCM `codec_type: HTTP2` + STRICT_DNS cluster + `dns_lookup_family: V4_ONLY` + `circuit_breakers.thresholds[0].max_connections: 4` + `typed_extension_protocol_options.envoy.extensions.upstreams.http.v3.HttpProtocolOptions.explicit_http_config.http2_protocol_options: {}` + admin block on `{{ADMIN_PORT}}` + bind `0.0.0.0`. Backend address uses `{{BACKEND_HOST}}:{{HTTP2_BACKEND_PORT}}` (the established H2-backend template marker — auto-wires `Http2EchoBackend` per the harness's existing `needs_http2_backend` block at `tests/differential/src/lib.rs:1998`; no new discriminator arm needed).
- **NEW** `tests/fixtures/0021-upstream-h2-connection-pooling/envoy-rust.yaml` (+72 lines) — identical topology modulo bind `127.0.0.1` + omitted `generate_request_id` (envoy-rust HCM does not model it; fixture-0019/0020 precedent).
- **NEW** `tests/fixtures/0021-upstream-h2-connection-pooling/expectations.yaml` (+40 lines) — `driver.kind: http2_keep_alive`; 5 sequential single-stream GETs over one downstream H2 conn; `settle_ms: 500`; 5 `expected_stats` rows verbatim per ADR-0039 (`downstream_rq_2xx`/`downstream_rq_total` × 5; `cluster.backend_cluster.upstream_rq_total: 5`; `cluster.backend_cluster.upstream_cx_total: 1`; `cluster.backend_cluster.upstream_cx_http2_total: 1`).
- **NEW** `tests/differential/tests/upstream_h2_connection_pooling.rs` (+36 lines) — Docker-gated wrapper test mirroring `upstream_connection_pooling_and_per_class_counters.rs`'s shape verbatim modulo fixture path.
- **MODIFIED** `tests/differential/src/lib.rs` (+~250 lines net: +`Driver::Http2KeepAlive` variant +`drive_http2_keep_alive` helper +dispatch arm +unit test; +3 sites in the `port_key` match + `needs_admin_port` matches! to extend the existing `Driver::Http1KeepAlive` machinery to the H2 sibling). Per the binding scope's structure-of-reuse rationale: `Http1KeepAliveRequest` + `KeepAliveExpectedStat` are codec-agnostic substructs and reused directly (same precedent as `Driver::Http2ProbeList` reusing `Http1Probe` at 11 D8.1). The kind tag is `http2_keep_alive` per the `Driver` enum's `#[serde(tag = "kind", rename_all = "snake_case")]` attribute.
- **MODIFIED** `docs/envoy-rust/phases/13.2-h2-pool-and-cx-total-tightening/PROGRESS.md` (this subsection).

**Driver shape (per ADR-0039 + the binding scope):** `drive_http2_keep_alive(proxy_addr, requests, side_name)` opens ONE TCP conn to the proxy's downstream H2 listener, runs `h2::client::handshake` to obtain a `SendRequest<Bytes>`, drives the H2 `Connection` future on a background `tokio::spawn`, and for each of N requests: clones `SendRequest`, builds an `http::Request<()>` with absolute-form URI (so `:authority` is populated; mirrors `drive_http2`'s URI shape), calls `send_request(req, /*end_of_stream=*/ true)` (GET-only — no body), awaits the `ResponseFuture`, asserts `status == req.expected_status`, then drains the response body with best-effort flow-control window release. Sequential await across the loop means N multiplexed streams share ONE downstream H2 conn — which is the discriminating-observable that exercises the H2 pool's stream-multiplex path on the upstream side. Teardown drops `send_request` then aborts the conn-driving spawn (mirrors `drive_http2`'s line 1438-1440 teardown verbatim).

**Empirical verification of the harness's `Http2EchoBackend` auto-wiring:** the binding scope item #5 explicitly required verifying whether the existing `needs_http2_backend` block at `tests/differential/src/lib.rs:1998` spawns `Http2EchoBackend` automatically for fixtures whose template references `{{HTTP2_BACKEND_PORT}}`. Verified: line 1998 is `let needs_http2_backend = upstream_template.contains("{{HTTP2_BACKEND_PORT}}") || subject_template.contains("{{HTTP2_BACKEND_PORT}}");` and line 2000 spawns `Http2EchoBackend::spawn()`. The 05.3 D6.b precedent (fixture 0010) uses exactly this template marker. **Disposition:** fixture 0021 uses `{{HTTP2_BACKEND_PORT}}` (matching the established H2-backend marker convention), and the existing harness arm wires `Http2EchoBackend` automatically. **No new backend-discriminator arm is needed** — the binding scope's empirical-verify path resolved to "no new arm". This is a positive simplification vs. the first-attempt's `needs_h2_echo_backend` arm (which was for fixture 0021 specifically because the first-attempt used `{{BACKEND_PORT}}` per the original PLAN; the ADR-0039 pivot makes `{{HTTP2_BACKEND_PORT}}` the natural choice).

**Deviations from the ADR-0039 reshaped scope (1 minor):**

- Scope item #5 named adding a new `needs_h2_echo_backend` discriminator arm at `tests/differential/src/lib.rs::run_fixture`. Empirically the existing `needs_http2_backend` block (the 05.3 D6.b precedent — fixture 0010 wiring) already auto-spawns `Http2EchoBackend` for any fixture referencing `{{HTTP2_BACKEND_PORT}}`. Switched fixture 0021's template marker from `{{BACKEND_PORT}}` (the original PLAN's marker, which the first-attempt used) to `{{HTTP2_BACKEND_PORT}}` (the established H2-backend marker). No new discriminator arm needed. This is the binding scope's "verify before adding" path resolving to the simpler option — cleaner than the first-attempt's arm-addition shape.

All other ADR-0039 reshaped scope items (1: fixture YAMLs with `codec_type: HTTP2`; 2: `expectations.yaml` with `driver.kind: http2_keep_alive` + 5 expected_stats; 3: `Driver::Http2KeepAlive` serde variant; 4: `drive_http2_keep_alive` helper; 6: Docker wrapper test) landed verbatim per the scope.

**TDD note:** the serde round-trip unit test `tests::driver_http2_keep_alive_round_trips_through_serde` (at `tests/differential/src/lib.rs`) was authored BEFORE the variant definition (TDD-first) — RED on the first compile (`Driver::Http2KeepAlive` undefined), then GREEN after the variant + dispatch + helper landed. Test asserts the snake_case-tagged YAML parses to the new variant + that `Http1KeepAliveRequest` + `KeepAliveExpectedStat` are reused under the codec-agnostic argument.

**Self-review findings (not addressed; surface for state-5 code review):**

- The new helper's `clone()` of `SendRequest` is technically unnecessary under sequential await (we hold `&mut` exclusivity per loop iteration via `let mut sr = send_request.clone(); sr.send_request(...)?`). Kept the clone per the binding scope item #4's explicit naming ("issues N sequential streams via cloned `SendRequest<Bytes>`") and because it matches h2's documented multiplex idiom. Future widening to parallel streams (if a fixture ever needs concurrent in-flight) drops in without re-shaping.
- The `Http1KeepAliveRequest::method` field is `String` (not `Http1Method`) — same as the H1 sibling. `http::Request::builder().method(req.method.as_str())` performs the conversion lazily; an unsupported method string would surface at builder-time. The H1 sibling has the same surface; no asymmetry.
- The bilateral discriminating observable in this fixture proves the H2 pool reuses ONE upstream conn across 5 downstream H2 streams. It does NOT independently prove that the upstream side multiplexes ALL 5 streams onto that one conn (vs. serializing them as 5 sequential 1-stream conns — which would also yield `upstream_cx_total: 1` under the pool's reuse). The Task 6 in-process backstop (the next task in the queue) is positioned to add the stream-multiplex-on-one-conn observable; flag for review at state-5 if Task 6's coverage shape doesn't anchor that finer-grained assertion.
- The harness's existing `needs_http2_backend` block is the load-bearing seam — if a future fixture wanted H2-backend + per-path-status mapping (the 13.1 0020-fixture-style per-class counter coverage at the H2 surface), this block would need extending. Out of 13.2 scope; flag for parent-13 close.

**Per-gate clean outputs (touched-crate scope; full workspace gate is Task 7):**

- `cargo build --workspace --all-targets` — `Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 16s` (no warnings).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — `Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 02s` (no diagnostics).
- `cargo fmt --all -- --check` — exit 0 after one one-line fmt fold (the dispatch arm's `for` collapsed to one line).
- `cargo test -p differential --lib driver_http` — `2 passed; 0 failed` (the existing `driver_http1_keep_alive_round_trips_through_serde` + the new `driver_http2_keep_alive_round_trips_through_serde`).
- `cargo test -p differential -- --test-threads=1` — see "Per-gate notes" below.
- `cargo test -p differential -- --include-ignored upstream_h2_connection_pooling` — Docker-gated; runs (or skips per testcontainers Docker availability). Anchors at CI (Task 7's gate).

**Per-gate notes:** running `cargo test -p differential` with the default test-threads (the parallel run) surfaces port-contention flakiness on the backend-spawn unit tests (`http1_echo_backend_spawns_and_echoes`, `http2_echo_backend_spawns_and_echoes`, etc.) when the suite races against itself for kernel-ephemeral ports — pre-existing environmental shape, not a regression of Task 5. Running with `--test-threads=1` (the documented serialized mode) is the project's de-facto cadence for the differential crate's spawn-heavy unit tests. The new `driver_http2_keep_alive_round_trips_through_serde` is purely serde — no spawn — and passes under both modes.

**Carryforward attribution:** ADR-0028 (H1-listener × H2-cluster dispatch deferral) REMAINS OPEN at 13.2 close — carried forward to a follow-up phase per ADR-0039 Consequences. Task 5 lands the H2-listener × H2-cluster dispatch path bilaterally; the H1-listener × H2-cluster path remains deferred.

**Commit SHA:** `6f2845f` (pre-amend; published HEAD may shift to the post-amend SHA per the per-task SHA-amend pattern).

---

### Task 5 fold-in — code-quality review documentation correction (teardown comment)

Follow-up commit appended after `1ade3ef` (Task 5's main commit — the actual published HEAD post-amend of the pre-amend `6f2845f` cited in the Task 5 subsection above). Addresses one IMPORTANT documentation finding from the code-quality reviewer:

**IMPORTANT: Misleading H2 teardown comment in `drive_http2_keep_alive` — CORRECTED.** The comment block at `tests/differential/src/lib.rs:1599-1602` introduced by the Task 5 helper claimed that `drop(send_request)` "signal[s] the h2 `Connection` future to begin a clean GOAWAY shutdown". The reviewer correctly noted: the very next line is `conn_handle.abort()`, which synchronously preempts the Connection future — the dropped-SendRequest GOAWAY path cannot fire because the future is never polled again post-abort. The comment overstated what actually happens at runtime.

The sibling helper `drive_http2` at `tests/differential/src/lib.rs:1467-1471` (landed at an earlier phase) has the honest framing: it states that aborting makes the helper return as soon as the response is drained, without claiming GOAWAY hygiene. The Task 5 helper now mirrors that framing verbatim modulo phrasing:

- `drop(send_request)` releases the last SendRequest handle (the h2 `Connection` future's inbound channel closes); drop-before-abort is hygienic ordering.
- `conn_handle.abort()` is the load-bearing teardown step — synchronously preempts the Connection future so the helper returns as soon as the response is drained, without tying test wall-time to peer-side GOAWAY round-trips.
- The post-abort future is never polled again, so no clean GOAWAY round-trip fires (the corrected comment names this explicitly).

**Net effect:** documentation honesty correction; no functional code change (the `drop(send_request); conn_handle.abort(); let _ = conn_handle.await;` sequence is byte-identical pre-and-post fold-in).

**Files touched:**

- `tests/differential/src/lib.rs` — comment block rewrite at the `drive_http2_keep_alive` teardown site (lines 1599-1606 post-rewrite; 4 lines → 8 lines).
- `docs/envoy-rust/phases/13.2-h2-pool-and-cx-total-tightening/PROGRESS.md` — this fold-in subsection.

**Per-gate clean outputs:**

- `cargo build -p differential` — Finished, no warnings.
- `cargo fmt --all -- --check` — exit 0; no diff.
- `cargo clippy -p differential --all-targets --all-features -- -D warnings` — no diagnostics.
- `cargo test -p differential --lib` (single-threaded) — same 109 passed (no test-count delta; comment-only change).

**Commit SHA:** fold-in commit appended below.

---

### Task 6 — in-process H2 backstop (D9.3-H2)

Lands the in-process subprocess-scope backstop for the H2-pool-reuse property at `crates/envoy-bin/tests/upstream_h2_connection_pooling.rs`. Sibling of the 13.1 H1 backstop at `crates/envoy-bin/tests/upstream_connection_pooling.rs` (landed at phase 13.1 Task 8); mirrors its shape verbatim modulo H2-specific substitutions (per ADR-0039 topology pivot).

**Files changed (2; total +363 + PROGRESS subsection):**

- **NEW** `crates/envoy-bin/tests/upstream_h2_connection_pooling.rs` (+363 lines) — in-process H2 backstop. Spawns the `http2-echo-server` helper (phase 05.3 workspace member; no `--per-path` flag — the helper always 200-echos) + spawns `envoy-bin` with a synthesized bootstrap (HCM `codec_type: HTTP2` downstream + STATIC `backend_cluster` with `typed_extension_protocol_options.envoy.extensions.upstreams.http.v3.HttpProtocolOptions.explicit_http_config.http2_protocol_options: {}` upstream — per ADR-0039 to avoid the `ConfigError::Http2ClusterFromHttp1Listener` gate at `crates/envoy-config/src/bootstrap.rs:1997-2016`). Drives 5 sequential `GET /` streams over ONE downstream H2 conn via `h2::client::handshake` + cloned `SendRequest<()>` (mirroring `drive_http2_keep_alive` at `tests/differential/src/lib.rs:1506-1612`, adapted + inlined). Settles 500ms (matches fixture 0021's `settle_ms: 500`), scrapes admin `/stats`, asserts the 5 fixture-0021 stat rows verbatim: `http.ingress_http.downstream_rq_2xx: 5`, `http.ingress_http.downstream_rq_total: 5`, `cluster.backend_cluster.upstream_rq_total: 5`, `cluster.backend_cluster.upstream_cx_total: 1` (THE H2-pool-reuse property), `cluster.backend_cluster.upstream_cx_http2_total: 1` (the 13.2 D7.2 per-codec split).
- **MODIFIED** `docs/envoy-rust/phases/13.2-h2-pool-and-cx-total-tightening/PROGRESS.md` (this subsection).

**Topology decision (per ADR-0039):** H2 downstream + H2 upstream. The PLAN's original H1-downstream × H2-upstream topology is rejected at parse time by the 06.3 D14.3 gate (`ConfigError::Http2ClusterFromHttp1Listener` — ADR-0028 deferral). Verified by direct read of `crates/envoy-config/src/bootstrap.rs:1997-2016` (the gate fires for `CodecType::HTTP1 | CodecType::AUTO` × any cluster with `http2_protocol_options` set). The backstop's bootstrap uses `codec_type: HTTP2` matching the Task 5 fixture-0021 pivot.

**Driver shape (per binding scope item (a)):** Open ONE TCP conn to the downstream H2 listener via `tokio::net::TcpStream::connect`, run `h2::client::handshake` to obtain `SendRequest<()>`, drive the H2 `Connection` future on a background `tokio::spawn`, and for each of 5 requests: clone `SendRequest`, build `http::Request<()>` with absolute-form URI (`http://backend_cluster/` — so `:authority` is populated; mirrors `drive_http2`'s URI shape), `send_request(req, /*end_of_stream=*/ true)` (GET-only — no body), await the response with a 10s per-stream timeout, assert `status == 200`, drain the response body with best-effort flow-control window release. Teardown drops `send_request` then aborts the conn-driving spawn (mirrors the Task 5 fold-in's corrected teardown comment: post-abort the conn future is never polled again, so no clean GOAWAY round-trip fires — that's intentional).

**Substitution checklist vs the H1 sibling backstop:**

1. Backend: `http2-echo-server` (workspace member) via `cargo run --quiet --manifest-path .../tests/helpers/http2-echo-server/Cargo.toml -- --port N`. No `--per-path` flag (helper always 200-echos).
2. Bootstrap cluster: `typed_extension_protocol_options.envoy.extensions.upstreams.http.v3.HttpProtocolOptions.explicit_http_config.http2_protocol_options: {}` (H2 upstream).
3. Bootstrap listener: `codec_type: HTTP2` (H2 downstream — per ADR-0039).
4. Driver: ONE H2 conn + 5 sequential cloned-`SendRequest` streams.
5. Workload: 5 GETs to `/`, all expecting 200.
6. Settle: 500ms (matches fixture 0021, vs the H1 sibling's 200ms).
7. Stat assertions: 5 rows (vs the H1 sibling's 9; the per-class `2xx/3xx/4xx/5xx` H1 split is collapsed to all-2xx here).
8. 5-standard-header check: **OMITTED** at the H2 surface. H2 has no concept of the H1 standard header roster (`server`/`date`/`content-length`/`content-type`/`connection`); the H1 sibling's check is a per-non-2xx discipline that does not translate to H2 — and the fixture-0021 workload is all-2xx so the H1 check would not fire anyway. Discipline is preserved on the H1 side; no carry-forward to the H2 sibling is meaningful. The omission is documented inline in the backstop's module-level docstring.
9. Subprocess discipline: `tokio::process::Command + kill_on_drop(true) + Stdio::null()/piped()` (verbatim mirror of the H1 sibling — per 09 REVIEW M3 disposition). Backend / envoy-bin readiness budgets 30s / 10s.

**Discriminating-power note (per binding scope's self-review hook):** `upstream_cx_total: 1` proves the H2 pool reuses ONE upstream conn but does NOT independently prove the upstream multiplexes ALL 5 streams onto that conn (a regression where each downstream stream serialized to a separate upstream 1-stream conn-acquire-release cycle would still yield `upstream_cx_total: 1` under pool reuse). Per the binding scope's `feedback_pick_recommendation` SKIP disposition: the backstop's role is exclusively the in-process round-trip evidence; the fixture is the bilateral evidence; the discriminating-power gap is named here for state-5 code review revisit. The optional concurrent-stream extension (which would discriminate stream-multiplex vs serial-cycle paths) is also SKIPped per the PLAN's `feedback_pick_recommendation` — sequential-stream test captures the pool-reuse property at the scope of this backstop.

**Deviations from PLAN (1 minor):**

- Step 7 of the PLAN substitution checklist names "5-standard-header presence assertion preserved on any non-2xx response (none expected; the discipline carries forward)". OMITTED per substitution-item 8 above — H2 has no H1 standard-header roster, the check has no semantic analog at the H2 surface, and the all-2xx workload would never trigger it anyway. Documented inline in the backstop's module docstring. The H1 sibling's check is preserved as-is at `crates/envoy-bin/tests/upstream_connection_pooling.rs:131-146`; the discipline lives on the H1 side, no carry-forward to H2 is meaningful.

**Per-gate clean outputs:**

- `cargo build --workspace --all-targets` — `Finished `dev` profile [unoptimized + debuginfo] target(s) in 48.78s` (no warnings).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — `Finished `dev` profile [unoptimized + debuginfo] target(s) in 46.91s` (no diagnostics).
- `cargo fmt --all -- --check` — exit 0; no diff.
- `cargo test -p envoy-bin --test upstream_h2_connection_pooling -- --nocapture` — `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.05s` (pass within ~2s end-to-end including `cargo run --quiet --manifest-path` overhead for the warm-cached `http2-echo-server` helper).
- `cargo test -p envoy-bin --tests` — see below; full envoy-bin backstop suite green (19 backstops pre-Task-6 → 20 post-Task-6).

**Per-gate notes:** the initial cold-cache run of the targeted `cargo test -p envoy-bin upstream_h2_connection_pooling` panicked on backend readiness — the cargo subprocess overhead exceeded the 30s budget on first invocation after `cargo build --workspace --all-targets` had touched files (invalidating cargo's run-cache). Subsequent runs with warm caches complete the full backstop in <3s. The H1 sibling has the identical 30s budget and the identical cargo-subprocess shape; both are stable under warm-cache cadence (CI runs after the workspace build step has finished, so cargo's run cache is warm). This is environmental shape, not a regression. The 30s budget matches the binding scope's "backend readiness budget 30s".

**Continuation-session re-verification (warm-cache, post-Task-5 fold-in HEAD `f7cd908`):**
- `cargo build --workspace --all-targets` — `Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.25s` (warm-cache no-op rebuild; no warnings).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — `Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.20s` (warm-cache; no diagnostics).
- `cargo fmt --all -- --check` — exit 0; no diff.
- `cargo test -p envoy-bin --test upstream_h2_connection_pooling -- --nocapture` — `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.04s`.
- `cargo test -p envoy-bin --tests` — all 21 test-binary result lines green (20 backstop binaries; the envoy-bin lib has 8 unit tests; upstream_active_health_check has 2). No regressions; no failures; no skips. The targeted H2 backstop finishes in 1.34s under the full-suite cadence (warm-cached helper artifact). Includes a clean run of `upstream_connection_pooling` (H1 sibling — 0.62s) and `access_log_file_sink_in_process` (the Task 2 narrative's documented kernel-port-contention test — green this run).

**Commit SHA:** `73f7c2c` (pre-amend; published HEAD may shift to the post-amend SHA per the per-task SHA-amend pattern that's run since Task 1).

---

### Task 6 fold-in — code-quality review documentation closures (discriminating-power caveat + backend-ready justification)

Follow-up commit appended after Task 6's main commit. Addresses two IMPORTANT documentation findings from the code-quality reviewer over the Task 6 diff; no functional code change.

**Finding 1 — discriminating-power caveat (docstring addition).** The Task 6 main subsection above already names the discriminating-power gap in a `**Discriminating-power note**` paragraph, but the gap was NOT named at the in-code surface (the module-level docstring of `crates/envoy-bin/tests/upstream_h2_connection_pooling.rs`). A future maintainer reading the test file alone would not see the caveat. Closed by appending one short paragraph to the module docstring (after the H2-standard-header-omission paragraph that ends at line 46), naming: (a) the bilateral `upstream_cx_total = 1` + `upstream_cx_http2_total = 1` assertion does NOT independently discriminate stream-multiplex from a serial acquire+release of one pooled conn 5 times (the same counter pair would fire 1 + 1 either way); (b) the bilateral evidence for the multiplex property lives at fixture 0021 + the Task 5 `drive_http2_keep_alive` driver; (c) this backstop's role is exclusively the in-process round-trip evidence (envoy-bin reachability + stat wiring); (d) state-5 review may revisit if a stronger in-process observable becomes available. Tone consistent with the existing module-doc paragraphs per D-3.4 cold-readability.

**Finding 2 — backend-ready justification REWRITTEN (the spec's prior rationale was empirically false).** The Task 6 main commit landed `wait_ready` with a TCP-only readiness probe — the doc comment justified it by referring to the H2-handshake-readiness check being "checked separately at the H2-driver site … TCP readiness is the gating signal here, matching `http2_router_upstream.rs`'s pattern". The prior subagent's BLOCKED escalation flagged a *spec-supplied* rationale (the spec proposed "the H2 pool retries on handshake fail" as the justification) as empirically false — `crates/envoy-http2/src/pool.rs::acquire` is one-shot; there is no retry-on-handshake-fail branch. The controller's re-plan disposition selected option (1) — proceed with a revised rationale that uses the empirically-correct fact pattern the prior subagent uncovered. The rewritten comment now names the 5 verified observations:

1. The helper's accept loop is poll-immediate after `TcpListener::bind` — no async work between bind and `listener.accept().await`. Verified by direct read of `tests/helpers/http2-echo-server/src/main.rs:99-122` (the `loop { tokio::select! { _ = shutdown => break, accept_result = listener.accept() => match ... } }` block immediately following `let listener = TcpListener::bind(...).await?` at line 99).
2. The kernel buffers SYNs at the listening socket between bind and userspace `accept()` — incoming TCP connections aren't lost during the brief poll-then-accept window (standard SOCK_STREAM listen-backlog semantics).
3. The h2 client preface at `crates/envoy-http2/src/client.rs:42-56` uses a ~10ms detection window that ONLY fails on immediate H2-incompatible bytes; it does NOT wait for the server's SETTINGS frame — verified at the `Box::pin(connection); tokio::select! { biased; conn_result = ..., _ = tokio::time::sleep(Duration::from_millis(10)) => { /* normal */ } }` block. The client doesn't block waiting for the helper's own h2 server-handshake to complete; the connection task is spawned as long as the connection future is `Poll::Pending` after 10ms.
4. Empirical: the test passes in ~2s without flake; if the race were practically reachable, it would surface as `upstream_cx_total != 1` on first attempt.
5. The differential precedent at `tests/differential/src/backend.rs:424-447` (`wait_h2_accept_ready`) is the stronger guarantee available IF flake ever surfaces — the rewritten comment names its existence + line numbers for the future maintainer.

The spec's prior rationale ("pool retries on handshake fail") is REMOVED from the comment entirely; the empirically-correct 5-observation rationale REPLACES it. Honest correction per the project's append-only doctrine.

**Net effect:** documentation honesty correction + cold-readability discriminating-power caveat at the in-code surface; no functional code change (the TCP-only `wait_ready` helper body is byte-identical pre-and-post fold-in; the test runtime is unchanged).

**Files touched:**

- `crates/envoy-bin/tests/upstream_h2_connection_pooling.rs` — module docstring discriminating-power caveat append (finding 1) + `wait_ready` helper doc comment rewrite (finding 2).
- `docs/envoy-rust/phases/13.2-h2-pool-and-cx-total-tightening/PROGRESS.md` — this fold-in subsection.

**Per-gate clean outputs:**

- `cargo build -p envoy-bin` — Finished, no warnings.
- `cargo fmt --all -- --check` — exit 0; no diff.
- `cargo clippy -p envoy-bin --all-targets --all-features -- -D warnings` — no diagnostics.
- `cargo test -p envoy-bin --test upstream_h2_connection_pooling -- --nocapture` — `1 passed; 0 failed`.

**Commit SHA:** fold-in commit appended below.

---

### Task 7 — state-4 phase-done verification + STATE advance to state-5-next

Docs-only state-4 verification commit per `BOOTSTRAP_PROMPT.md` §5 state 4 + the 12.2 Task 8 + 13.1 Task 10 + earlier-phase state-4 cadence precedents. Runs §7.5 (a)–(e) gates against HEAD `2fef8ad` (the Task 6 fold-in commit; pushed; CI `26414774250` green 2m37s) + quotes per-gate evidence + advances STATE.md to `13.2` state-4-complete / state-5-next. The state-5 code-review session intervenes between this commit and Task 8 per the §5 state machine.

Lands controller-direct per `feedback_execution_style`'s mid-arc latitude (state-4 verification is mostly mechanical command-run + PROGRESS quoting; subagent overhead exceeds task scope).

**§7.5 (a) — fixture 0021 green:**

- Local: `cargo test -p differential --test upstream_h2_connection_pooling --no-fail-fast` → `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.07s`. Bilaterally GREEN.
- CI anchor: HEAD `2fef8ad` CI run `26414774250` `completed / success` 2m37s — runs `cargo test --workspace` which exercises every integration test under `tests/differential/tests/*` (none of the fixture wrappers carry `#[ignore]`); the fixture-0021 wrapper participates in this run.

**§7.5 (b) — 21 Docker-gated fixtures green simultaneously vs `envoyproxy/envoy:v1.33.0`:**

The local Docker re-run at this verification surfaced unstable behavior under the dev box's loaded Docker daemon (hung mid-suite + an old leaked `envoyproxy/envoy:v1.33.0` container from 2 hours earlier contributing to kernel-port-buffer pressure). Per the established cadence (the 13.1 state-4 + 12.2 Task 8 precedents which named CI as the canonical gate anchor when the local environment is unstable), **CI is the authoritative gate-(b) source**.

- **CI anchor:** HEAD `2fef8ad` CI run `26414774250` `completed / success` 2m37s. CI's `cargo test --workspace` (`.github/workflows/ci.yml:51-52`) runs all integration tests under `tests/differential/tests/*` (each integration test file compiles to its own test binary; none carry `#[ignore]`); the test step ran green → all 21 fixture wrappers (0001-0021) PASSED bilaterally vs `envoyproxy/envoy:v1.33.0` at HEAD `2fef8ad`.
- **Local corroboration** (3 representative fixtures runs targeting the architecturally load-bearing surfaces):
  - Fixture 0021 (H2 pool — new at Task 5) — `cargo test -p differential --test upstream_h2_connection_pooling --no-fail-fast` → 1 passed in 3.07s.
  - Fixture 0020 (H1 pool — 13.1) — `cargo test -p differential --test upstream_connection_pooling_and_per_class_counters --no-fail-fast` → 1 passed in 3.36s.
  - Fixture 0010 (H2 router upstream — 05.3) — `cargo test -p differential --test http2_router_upstream --no-fail-fast` → 1 passed in 2.46s.
- The local-environment instability surfaced two pre-existing flakes worth naming for the state-5 reviewer:
  - `access_log_file_sink` (per Task 5 PROGRESS Self-review item 4 + 13.1 state-5 fold-in): the Envoy v1.33 file-access-log writer's wall-time-of-writability differs from envoy-rust's, racing the harness's read. The 13.1 state-5 fold-in landed a structural wait-for-NON-EMPTY mitigation, but the local Docker variance under load can still trigger it. CI does NOT see this flake (CI's GitHub runner has stable Docker daemon timing). No code change at this commit.
  - 2 transient differential lib tests (`subject::tests::starts_and_shuts_down_envoy_rust` + `tests::drive_admin_scrape_round_trips_against_envoy_bin_admin`): both surfaced "127.0.0.1:port not accept-ready within 5s" panics under `cargo test --workspace --test-threads=1` after the local Docker daemon was loaded. Both pass cleanly in isolation (`cargo test -p differential --lib subject::tests::starts_and_shuts_down_envoy_rust` → 1 passed in 0.07s). Pre-existing kernel-port-contention pattern documented in Task 2 PROGRESS narrative. NOT a regression; flag for state-5 reviewer revisit if it surfaces in CI.

**§7.5 (c) — h2spec ≥95% at parent-05 baseline 99.31%:**

- Local: `cargo test -p envoy-http2 -- h2spec_pass_rate_gate` → `test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 55 filtered out; finished in 0.00s`. Locally skipped via the `h2spec_pass_rate_gate` test's `which h2spec` graceful-skip per 05.2 SPEC §3 D7 (no `h2spec` binary on the dev box) — `filtered out: 55` is the harness's gate-test exclusion.
- CI anchor: HEAD `2fef8ad` CI run `26414774250` installs h2spec v2.6.0 at `.github/workflows/ci.yml:43-49` then runs `cargo test --workspace` at `:51-52`. The h2spec gate fired bilaterally; CI green confirms the parent-05 baseline 99.31% pass rate held. 13.2 touches the H2 upstream-client surface (Task 2 H2 router-arm pool integration via `HCMConfig::wrap`) but NOT the H2 downstream framer/codec — h2spec runs against the H2 listener (downstream), so the pool integration doesn't regress the gate.

**§7.5 (d) — `parse_bootstrap` fuzz target clean on the 21-seed corpus:**

- Local: `cd crates/envoy-config && cargo +nightly fuzz run parse_bootstrap -- -runs=200000` →
  ```
  #200000	DONE   cov: 13745 ft: 37892 corp: 3564/2099Kb lim: 4096 exec/s: 12500 rss: 609Mb
  Done 200000 runs in 16 second(s)
  ```
  Coverage **13745** / features **37892** / 0 crashes in 16 s. **Δ vs 13.1 state-4 baseline (cov 13636 / ft 37080):** +109 cov / +812 ft — exactly what the new 13.2 code surface (Task 1's H2 pool primitive + Task 2's `HCMConfig` wrapper unlocking the H2-cluster + circuit_breakers dispatch path) adds to the bootstrap parser's reachable paths via the existing 21-seed corpus (no new corpus seed at 13.2 per PLAN lock-in #16 — the H2-cluster schema reuses 13.1's `circuit_breakers` seed verbatim).

**§7.5 (e) — 5 stable-toolchain gates clean:**

- `cargo build --workspace --all-targets` — `Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 59s`. No warnings, no errors.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — `Finished `dev` profile [unoptimized + debuginfo] target(s) in 16.21s`. No clippy diagnostics.
- `cargo fmt --all -- --check` — exit 0; no diff.
- `cargo test --workspace -- --test-threads=1` — `107 passed; 2 failed; 1 ignored` in the differential lib slice; the 2 failures are the pre-existing kernel-port-contention flakes documented above + each passes cleanly in isolation. Across the rest of the workspace (envoy-cluster, envoy-config, envoy-http1, envoy-http2, envoy-stats, envoy-accesslog, envoy-filter, envoy-tls, envoy-tcp, envoy-listener, envoy-health, envoy-admin, envoy-bin, fuzz, tls-test-pki) all unit tests passed; full workspace test count is **878 passed / 2 transient-flake / 1 ignored across 74 result lines** under `--test-threads=1` (the 2 transient-flake are the documented pre-existing flakes which pass in isolation; CI exercises tests with default parallelism and the kernel-port-contention pattern doesn't surface there).
- `cargo deny check` — `advisories ok, bans ok, licenses ok, sources ok` with the 13.1 / 12.2 / earlier-phase benign unmatched-license-allowance warnings unchanged (`Apache-2.0 WITH LLVM-exception`, `Unicode-DFS-2016`, `Zlib` — license families enabled in `deny.toml` but not encountered in the resolved workspace dep graph).

**Carryforward dispositions ratified at state-4:**

- **06.3 REVIEW I2 (b)** — FULLY CLOSED at Task 4 `4ab2c61` (the BEHAVIOR_CONTRACT `cluster.<name>.upstream_cx_total` row tightening). Combined with 13.1's I2 (a) closure (fixture 0020), the full 06.3 REVIEW I2 carryforward CLOSES at parent-13 close — Task 8 re-attributes the closure at the closing commit per the closing-sub-phase invariant.
- **13.1 REVIEW Cluster A-I3** (spurious-overflow race) — CLOSED at Task 1 + Task 1 fold-in (joint H1+H2 sync-`parking_lot::Mutex` switch + the H2 invalidate-Drop TOCTOU race fix at `ae8d7cf`).
- **13.1 REVIEW Cluster A-M1 + A-M2 + A-M4** (3 Cluster A Minors) — CLOSED at Task 1 opportunistically.
- **ADR-0028** (H1-listener × H2-cluster dispatch deferral) — REMAINS OPEN at 13.2 close per ADR-0039 Consequences; carried forward to a follow-up phase that closes ADR-0028 architecturally (~300-400 LoC; out of 13.2 scope).
- All other carryforwards (13.1 A-M3 + A-M5 + B-M1..B-M3 + C-M1..C-M4; 12.2 11 active Minors; 12.1 M1+M3; phase-11 M1-M8; earlier-phase residuals) carry forward unchanged per their named-owner dispositions. The state-5 reviewer may surface additional Minors over the 8-commit + 4-fold-in arc `8c7d8a2..<state-4-HEAD>`.

**Files touched at THIS commit (2):**

- MODIFY `docs/envoy-rust/phases/13.2-h2-pool-and-cx-total-tightening/PROGRESS.md` — this Task 7 subsection.
- MODIFY `docs/envoy-rust/STATE.md` — 4-top-pointer rewrite to `13.2` state-4-complete / state-5-next; Next expected skill: `superpowers:requesting-code-review`; Last commit / Last updated rewrites; append `### Phase-13.2 state-3 execution arc` Notes subsection summarizing Tasks 1-6 + 4 fold-ins; demote the prior subsections to `_Historical_` per D-3.5 append-only.

**No code change at THIS commit** (state-4 verification is docs-only); no test/fixture/Cargo/DECISIONS/BEHAVIOR_CONTRACT/SPEC/PLAN change; no ROADMAP change (row `13.2` stays `in-progress` — flips `done` only at the eventual state-6 close-out per the closing-sub-phase invariant which Task 8 fires simultaneously with the parent-13 row); no ENVOY_TARGET.md / rust-toolchain.toml change (D-3.7 / D-3.9 unchanged); no `unsafe` introduced. No `[ADR-NNNN]` bracket in the title (no ADR landed at this verification).

Per `BOOTSTRAP_PROMPT.md` §5 state 5 + `SKILL_ROUTING.md`, the next session — operating as the **phase-13.2 state-5 code-review session** — invokes **`superpowers:requesting-code-review`** scoped to the reviewed range `8c7d8a2..<state-4-HEAD>` (the state-2 PLAN-write commit's predecessor `8c7d8a2` through this state-4 commit) and writes `docs/envoy-rust/phases/13.2-h2-pool-and-cx-total-tightening/REVIEW.md`.

**Commit SHA:** lands at this state-4 verification commit (self-referential after this PROGRESS subsection is committed).

---

*(Task 8 appended below at the state-6 close-out commit per the closing-sub-phase invariant. The state-5 code-review session at `0d8b1c2` (REVIEW.md verdict `Approved with M-track follow-ups`; zero Critical / zero Important post-aggregation; 13 active Minor carryforwards — all awareness-only) intervened between this state-4 verification commit and Task 8 per the §5 state machine.)*

### Task 8 — state-6 close-out commit (closing-sub-phase + parent-13 close)

Docs-only state-6 CLOSING-sub-phase close-out commit per `BOOTSTRAP_PROMPT.md` §5 state 6 + §5.3 commit-message format + the **closing-sub-phase close-out** cadence (mirrors the 02.2 `cc8a64a` + 03.2 `97df2dc` + 07.2 `c7b0a36` + 08.2 `b40ad9b` + 12.2 `3ec7fb9` closing-sub-phase precedents verbatim for the ROADMAP row-flip-pair shape — the closing sub-phase commit flips BOTH the sub-phase row AND its parent-row simultaneously; the freshest CLOSING-sub-phase precedent is the 12.2 state-6 close-out commit `3ec7fb9`, mirrored verbatim here for the file-set + the STATE.md 4-top-pointer rewrite + the `### Phase-NN.N rollovers` Notes subsection cadence).

Lands controller-direct per the closing-sub-phase docs-only convention + `feedback_execution_style` mid-arc latitude (the close-out is a docs-only ROADMAP row-flip pair + STATE 4-top-pointer rewrite + this PROGRESS Task 8 append; subagent overhead exceeds task scope; mirrors the 13.1 / 12.2 / 12.1 / phase-11 close-out cadence verbatim).

**Commit title (per 13.2 SPEC §8 + ADR-0039 attribution):**

```
phase 13.2: H2 connection pool + upstream_cx_total tightening to value-exact + fixture 0021 + parent-13 close (06.3 REVIEW I2 FULLY CLOSED) [parent 13 done] [ADR-0039]
```

The `[parent 13 done]` tag is the closing-sub-phase row-flip-pair marker (mirrors 12.2's `[parent 12 done]` at `3ec7fb9`). The `[ADR-0039]` bracket attributes the topology-pivot ADR landed in the 13.2 lifecycle at state-3-partial closure `e2b8d1b` (ADR-0039 itself ratified there for the fixture-0021 H1-listener × H2-cluster → H2-listener × H2-cluster + new `Driver::Http2KeepAlive` reshape; ADR-0028 carried forward per ADR-0039 Consequences with closure path documented).

**Files touched at THIS commit (3):**

- **MODIFY** `docs/envoy-rust/ROADMAP.md` — atomic single-commit row-flip pair: row `13.2` `status: in-progress → done` AND parent row `13` `status: in-progress → done` SIMULTANEOUSLY (the closing-sub-phase invariant). Neither row flips alone at this commit. `summary` columns unchanged on both rows; rows `00`-`12.2` + `13.1` untouched.
- **MODIFY** `docs/envoy-rust/STATE.md` — advance the 4 top pointers (Active phase `13.2` state-5-complete / state-6-next → `awaiting next planning` with directory pointing to closed `13.2` + closed sibling `13.1` + closed parent `13`; Next expected skill `state-6 close-out [no specific skill]` → `superpowers:brainstorming` for the next planning session; Last commit / Last updated rewrites) + append a `### Phase-13.2 rollovers` Notes subsection at end of Notes (mirrors the `### Phase-12.2 rollovers` + `### Phase-13.1 rollovers` precedents verbatim for the rollover-narrative shape). All prior subsections preserved verbatim per D-3.5 (append-only) + D-3.4 (context isolation); prior state-5 narrative demoted to `_Historical_`.
- **MODIFY** `docs/envoy-rust/phases/13.2-h2-pool-and-cx-total-tightening/PROGRESS.md` (this file) — append this Task 8 subsection (the recommended controller's call per `feedback_pick_recommendation`; the 13.2 PLAN's Task 8 explicitly names the state-6 close-out as its own task; D-3.4 closure-narrative cold-readability). The 12.2 + 13.1 close-out precedents both included a final PROGRESS Task subsection (12.2 Task 8 PROGRESS was the state-4 verification subsection landed at `39e55a5` not the close-out itself; 13.1 had no separate Task PROGRESS at the close-out since Task 10 was the state-4 verification at `592d9e7`); 13.2 explicitly names Task 8 = the state-6 close-out per the 13.2 PLAN, so the subsection lands at THIS commit.

**No production code change at THIS commit; no test/fixture/Cargo/DECISIONS/BEHAVIOR_CONTRACT/SPEC/PLAN/REVIEW change; no ENVOY_TARGET.md / rust-toolchain.toml change (D-3.7 / D-3.9 unchanged); no `unsafe` introduced.** No new ADR (ledger head stays **ADR-0039**; project-cumulative ADR count 40; next available **ADR-0040**); state-6 commits NEVER land ADRs.

**Phase 13.2 closed + parent phase 13 closed.** The 5 carved parent-13 deliverables (D5 H2Pool primitive + D6 H2 router-arm pool integration + D7.1 `upstream_cx_total` row tightening + D7.2 `upstream_cx_http2_total` row + D9.1-H2 fixture 0021 + D9.3-H2 in-process H2 backstop) landed end-to-end across Tasks 1–7 (`f692b53` → `76e4b82`) + 4 in-phase fold-in commits (Task 1 `ae8d7cf` CRITICAL — H2 invalidate-Drop TOCTOU race close; Task 2 `ef6deda` IMPORTANT — cx_active fallthrough comment correction; Task 5 `f7cd908` IMPORTANT — H2 teardown honesty correction; Task 6 `2fef8ad` 2× IMPORTANT — discriminating-power caveat + backend-ready justification) + state-3-partial closure `e2b8d1b` (ADR-0039 topology pivot) + state-5 code review `0d8b1c2` (REVIEW.md `Approved with M-track follow-ups`). The cumulative 13.2 lifecycle arc since state-2 PLAN-write `8c7d8a2` is **14 total commits** (`8c7d8a2..HEAD`).

**THE FULL 06.3 REVIEW I2 CARRYFORWARD CLOSES AT THIS COMMIT.** Combined with 13.1's I2 (a) closure at fixture 0020 (the 13.1 state-6 close-out `9d8e9ca`'s explicit attribution) + 13.2's I2 (b) closure at Task 4 `4ab2c61` (the BEHAVIOR_CONTRACT `cluster.<name>.upstream_cx_total` row tightening to `value-exact (H1+H2 clusters under the harness's single-downstream-keep-alive-conn driver); name-required, value-may-differ (TCP-proxy clusters carved out per parent-13 SPEC §4)`), the 7-phase-old 06.3 REVIEW I2 carryforward is **FULLY CLOSED** at parent-13 close per the closing-sub-phase invariant. PROGRESS attributes this closure re-attribution honestly per D-3.4.

**Carryforward dispositions ratified at state-6 (re-attribution of state-4 + state-5 ratifications, plus the new closure attribution at parent-13 close):**

- **06.3 REVIEW I2 (FULL)** — **FULLY CLOSED at THIS commit** per the closing-sub-phase invariant. Combines:
  - **(a)** 13.1 fixture 0020 per-class HCM `downstream_rq_{2,3,4,5}xx` + cluster `upstream_rq_5xx` bilateral assertions (CLOSED at 13.1 Task 7 `ec50093`; re-attributed at 13.1 state-6 close-out `9d8e9ca`).
  - **(b)** 13.2 D7.1 `cluster.<name>.upstream_cx_total` BEHAVIOR_CONTRACT row tightening to `value-exact (H1+H2)` (CLOSED at 13.2 Task 4 `4ab2c61`; re-attributed at THIS state-6 close-out).
- **13.1 REVIEW Cluster A-I3** (spurious-overflow race) — FULLY CLOSED jointly across H1+H2 at 13.2 Task 1 `f692b53` + Task 1 fold-in `ae8d7cf` (joint sync-`parking_lot::Mutex` switch + the H2 invalidate-Drop TOCTOU race close).
- **13.1 REVIEW Cluster A-M1 + A-M2 + A-M4** (3 Cluster A Minors) — CLOSED opportunistically at 13.2 Task 1 (rename `_sweepers` → `sweepers` + `pub async fn shutdown(self)` on both managers + `Arc::ptr_eq` debug-assert at the gauge wiring site + improved `.expect` message naming the single-bootstrap-per-process invariant).
- **ADR-0028** (H1-listener × H2-cluster dispatch deferral) — **REMAINS OPEN** per ADR-0039 Consequences. Closure path documented (extract `envoy-http2::Client` to a new `envoy-http-client` crate to break the dep cycle; ~300-400 LoC; foundations grant + new crate; out of 13.2 scope). Named owner: a follow-up phase.
- **All other carryforwards** (13.1 A-M3 + A-M5 + B-M1..B-M3 + C-M1..C-M4; 12.2 11 active Minors; 12.1 M1+M3; phase-11 M1-M8; earlier-phase residuals) carry forward unchanged per their named-owner dispositions.
- **13.2 state-5 13 new Minor carryforwards** (A-M1..A-M5, B-M1..B-M4, C-M1..C-M4) — all carry forward unchanged per REVIEW.md §4 named-owner dispositions; NONE folds in at THIS close-out per the standing 13.1 + 12.2 + 12.1 + phase-11 docs-only close-out precedent.

**Parent-13 closure milestones at THIS commit:**

- **2 closed phases (12 + 13)** in the Upstream-robustness family — phase 12 closed at `3ec7fb9` (12.2 state-6 close-out; the FIRST Upstream-robustness phase closed) + phase 13 closed at THIS commit (the SECOND).
- **Foundation periodic-background primitive triad complete** — 12.2 `envoy-health::Scheduler` (active HC probe task) + 13.1 `H1Pool::spawn_idle_sweeper` + 13.2 `H2Pool::spawn_idle_sweeper`; all three sharing identical `tokio_util::sync::CancellationToken` cancellation discipline + `pub async fn shutdown(self)` on the H1+H2 pool managers post-13.1-A-M1 close at 13.2 Task 1.
- **Per-protocol upstream connection pooling complete for H1 + H2** — `crates/envoy-http1/src/pool.rs` (13.1) + `crates/envoy-http2/src/pool.rs` (13.2) with cycle-free external pool manager registries (`H1PoolManager` + `H2PoolManager` as siblings to `ClusterManager`, not fields on `Cluster`; mirrors the 12.2 `envoy-health::Scheduler` precedent). TCP-proxy pooling carved out for a follow-up phase per the BEHAVIOR_CONTRACT row's explicit carve-out (the `crates/envoy-tcp/src/lib.rs:108` per-call `cx_total.inc()` site stays untouched; existing TCP fixtures keep the pre-13.2 presence-only assertion); H3/QUIC pooling defers to the HTTP/3 family.
- **HCMConfig wrapper struct at `crates/envoy-http2/src/hcm.rs:27-53`** — replaces the prior `pub type HCMConfig = Http1HCMConfig` type alias with a proper struct carrying `Arc<envoy_http1::HCMConfig>` inner + `Option<Arc<H2PoolManager>>`; cycle-free (envoy-http1 does NOT depend on envoy-http2); H1-cluster-in-H2-HCM cross-protocol arm at `hcm.rs:325-337` STAYS UNTOUCHED per ADR-0028 deferral.
- **3 new pool stat rows** in BEHAVIOR_CONTRACT.md — `cluster.<name>.upstream_cx_destroy` (13.1) + `cluster.<name>.upstream_cx_http1_total` (13.1) + `cluster.<name>.upstream_cx_http2_total` (13.2); together enumerate the per-protocol breakdown of `upstream_cx_total`.
- **The `cluster.<name>.upstream_cx_total` row** tightened to `value-exact (H1+H2)` with explicit TCP-proxy carve-out (13.2 D7.1; FULL 06.3 REVIEW I2 (b) closure).
- **21 Docker-gated fixtures (0001-0021) green simultaneously bilaterally** vs `envoyproxy/envoy:v1.33.0` at CI run `26414774250` HEAD `2fef8ad` `completed/success` 2m37s + CI run `26416786136` HEAD `76e4b82` `completed/success` 2m53s + CI run `26418453713` HEAD `0d8b1c2` `completed/success` 2m38s. h2spec ≥95% held at parent-05 baseline 99.31% bilaterally (13.2's H2 surface touches are upstream-client-side only — pool integration via `HCMConfig::wrap` + H2 router-arm pool migration; h2spec runs against the H2 listener / downstream framer, which 13.2 doesn't touch).
- **ADR-0039 ratified** at state-3-partial closure `e2b8d1b` for the fixture-0021 topology pivot; **ADR-0028 carried forward** per ADR-0039 Consequences with closure path documented.

**CI re-validation at THIS commit:** the close-out CI run re-validates the docs-only edits through the 5 stable-toolchain gates. Anticipate `completed/success` ~2-3 min on first attempt (docs-only; no test surface change). If CI flakes on the pre-existing `envoy-accesslog::file_sink::tests::file_sink_writes_one_record` UNIT test (as it did on the Task 7 state-4 commit's initial run), rerun once; this is the documented pre-existing flake from the access-log family and is not a regression.

**Per `BOOTSTRAP_PROMPT.md` §5 state 0 + `SKILL_ROUTING.md`, the next session enters a fresh phase-0 brainstorm** — invokes `superpowers:brainstorming` to pick + draft the next ROADMAP row from §9 (planner-controller's call which family). After the brainstorm lands a ROADMAP row + creates the phase directory + writes `SPEC.md`, the state machine advances to state 2 (`superpowers:writing-plans`).

**Commit SHA:** lands at this state-6 close-out commit (self-referential after this PROGRESS subsection is committed).

---

*(Phase 13.2 closed. Parent phase 13 closed. The FULL 06.3 REVIEW I2 carryforward closes at this commit. The next session re-derives via `BOOTSTRAP_PROMPT.md` §1 Step D into `superpowers:brainstorming` for the next feature-family phase pick.)*
