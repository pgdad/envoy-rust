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
