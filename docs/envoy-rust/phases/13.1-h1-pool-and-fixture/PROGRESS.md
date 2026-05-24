# Phase 13.1 (`13.1-h1-pool-and-fixture`) — PROGRESS

> Running log of state-3 execution. Each task closes with a `### Task N — <title>` subsection quoting test names + per-gate clean outputs + commit SHA. State-4 (Task 10) quotes per-gate evidence per the 12.2 / 12.1 / 06.3 precedent.

---

## Task 1 preamble — PLAN-write context for state-3 controller

This section is the controller's read-on-start brief; it lands at the state-2 PLAN-write commit (not Task 1's own commit) per the 12.1 / 12.2 state-2 PLAN-write precedent.

### Locked §2 facts (DO NOT re-run Docker)

The parent-13 state-2 PLAN-write performed the parent SPEC §6.2 HEAVY 9-item empirical verification against `envoyproxy/envoy:v1.33.0`. All 9 items MATCHED the parent SPEC's projections; the findings are LOCKED into the 13.1 SPEC §2 + the STATE.md `### Phase-13 state-2 split decision` subsection. The 13.1 state-3 controller MUST NOT re-run Docker for §6.2 verification; if a 13.1 implementation detail surfaces a new empirical question, verify at task time. The 9 findings (recap; full details in 13.1 SPEC §2):

- **(i)** `circuit_breakers.thresholds[<i>].{priority?, max_connections?}` shape; priority defaults DEFAULT (omitted in /config_dump when DEFAULT); max_connections default 1024.
- **(ii)** Envoy ALWAYS pools regardless of `circuit_breakers` config; default `max_connections: 1024` (never hit under fixture load). → Default-enabled pool with hardcoded defaults when `circuit_breakers` absent (PLAN lock-in #2).
- **(iii)** Idle_timeout knob lives at `typed_extension_protocol_options.envoy.extensions.upstreams.http.v3.HttpProtocolOptions.common_http_protocol_options.idle_timeout` (default 3600s). → Phase-13 hardcodes 60s; config-side knob defers.
- **(iv)** **CRITICAL nuance:** `upstream_cx_total: 1` requires a SINGLE downstream keep-alive conn issuing N sequential requests. With separate downstream conns: `upstream_cx_total: N`. → PLAN lock-in #4: fixture 0020 driver MUST use `Driver::Http1KeepAlive` (the D10 driver extension landing at Task 7 atomically with the fixture).
- **(v)** `upstream_cx_destroy` + 5 sub-siblings (local/remote/with_active_rq variants). All per-cluster. → Phase-13 wires the parent `upstream_cx_destroy` + `upstream_cx_http1_total` only at 13.1; sub-siblings defer.
- **(vi)** H2 stats namespace registration (defers to 13.2).
- **(vii)** Per-class HCM counters bilateral byte-equality (verified). → 13.1 D9.1 fixture 0020 = 06.3 REVIEW I2 (a) full-closure site (Task 7).
- **(viii)** Cluster `upstream_rq_{2,3,4,5}xx` per-class distribution byte-equal.
- **(ix)** **CONFIRMED:** HCM `downstream_rq_5xx` INCLUDES synth-503; cluster `upstream_rq_5xx` does NOT (synth bypasses upstream). Body verified byte-exact = ADR-0037's 19 bytes `no healthy upstream`.

### PLAN-time SPEC corrections (verified at state-2 PLAN-write against HEAD `a88fe26`)

The 13.1 PLAN-writer read every named seam directly and confirms:

- **`Cluster` struct at `crates/envoy-cluster/src/cluster.rs:32-76`** — confirmed `pub(crate)` fields including `name`, `endpoints`, `cursor`, `upstream_protocol`, `cx_total`, `cx_active`, `upstream_rq_total`, `upstream_rq_5xx`, `endpoint_health`, `panic_threshold`. **No `h1_pool` field yet — 13.1 D3 does NOT add one** (lock-in #1 — external `H1PoolManager` instead, per the bin-wired-injection precedent at 12.2's `envoy-health::Scheduler`).
- **`ConnGaugeGuard` at `crates/envoy-cluster/src/cluster.rs:18-26`** — confirmed. Field is private; Task 3 Step 2 adds a `ConnGaugeGuard::from_gauge(Arc<Gauge>) -> Self` pub constructor so the H1 pool can construct guards against the shared cluster gauge handle.
- **`envoy-http1::Client::connect` at `crates/envoy-http1/src/client.rs:33`** — confirmed `pub async fn connect(addr: SocketAddr, host: &str) -> Result<ClientStream, Http1Error>`.
- **`ClientStream` at `crates/envoy-http1/src/client.rs:56`** — confirmed `pub struct` with `pub(crate)` fields (`stream`, `host`, `buf`). Sibling `pool.rs` module accesses directly — **no visibility widening needed** (lock-in #10).
- **H1 `upstream_cx_total` increment site is at `crates/envoy-http1/src/hcm.rs:514`** (within the connect-on-miss `Ok(s) => { cluster.cx_total().inc(); Ok(s) }` arm immediately after `Client::connect(endpoint, &host_header)`). The surrounding `tier-1 cached_upstream` micro-cache at `hcm.rs:502-527` becomes dead code at Task 4 per lock-in #5 and is removed. **The parent-13 SPEC §8's claimed site `router.rs:85-90` was INCORRECT** — verified by grep; this PLAN supersedes that claim per D-3.4.
- **`envoy-tcp/src/lib.rs:108`** carries a `cluster.cx_total().inc()` TCP-proxy site — **UNTOUCHED at 13.1** (TCP pool defers per parent SPEC §4).
- **`Cluster` struct in envoy-config at `crates/envoy-config/src/bootstrap.rs:56`** — confirmed carries `health_checks` (12.1) + `common_lb_config` (12.1) — **no `circuit_breakers` field yet**; 13.1 D1 adds it. The defense-in-depth `Cluster`-by-hand test constructors at `crates/envoy-cluster/src/cluster.rs:803, :825` carry `common_lb_config: None` at their tail; Task 1 Step 4 extends each with `circuit_breakers: None`.
- **12.2 helper at `tests/helpers/health-aware-http1-backend/src/main.rs`** carries `--port` + `--healthz-status` + `--data-status` + `--data-body` flags. Task 6 D8 adds `--per-path PATH=STATUS,...` additively. The helper has NO existing `#[cfg(test)] mod tests` block — Task 6 adds the first.
- **`Driver` enum at `tests/differential/src/lib.rs:39`** — confirmed; variants `Http1`/`Http2`/`Http1ProbeList`/`Http2ProbeList`/`Http1AfterSettle`/`AdminScrape`. Task 7 D10 adds `Http1KeepAlive` as a sibling variant.
- **`envoy-bin/src/main.rs`** — `from_bootstrap` called at `:124`; `envoy-health::Scheduler::spawn` invoked at `:134`. Task 4 Step 4 wires `H1PoolManager::for_bootstrap` between these two calls (cluster_mgr exists; pool needs it).
- **`parse_bootstrap` fuzz corpus on disk has 25 *.yaml files**; the test SUCCESS array has 20 entries; REJECT array has 3 entries; `minimal.yaml` is asserted separately. The "20→21" framing in the SPEC refers to SUCCESS-array count. Task 9 D11 extends SUCCESS to 21 entries + adds the seed file + extends `.gitignore` (3 sibling edits). (The pre-existing `cluster_http2_protocol_options.yaml` seed on disk that is in neither test array is an unrelated pre-existing condition and is not 13.1's concern.)

### Cycle-resolution decision narrative (lock-in #1)

The SPEC §5.1 left the seam open between (a) a new trait declared in `envoy-cluster` (e.g. `pub trait H1ClientPool`) implemented by `envoy-http1::H1Pool` + held as `Cluster.h1_pool: Option<Arc<dyn H1ClientPool>>`, vs (b) bin-wired injection via an external `H1PoolManager` mirroring 12.2's `envoy-health::Scheduler` pattern (sibling-to-`ClusterManager`, NOT a field on Cluster).

**13.1 PLAN-time pick: option (b) — external `H1PoolManager`.** Rationale (per `feedback_pick_recommendation` — pick the obvious recommendation; no fork):

1. Option (a) requires interior mutability on `Cluster` (the `Arc<Cluster>` returned by `from_bootstrap` has no setter), OR widening `from_bootstrap`'s signature to take a `pool_builder` callback, OR pre-building pools before from_bootstrap (chicken-and-egg with `Cluster.cx_total` registration). All three are intrusive.
2. Option (b) parallels the 12.2 `envoy-health::Scheduler` precedent verbatim. The bin constructs `cluster_mgr` first, then `H1PoolManager` second (it walks the bootstrap's clusters, looks up each one in cluster_mgr to grab the shared `Arc<Counter>`/`Arc<Gauge>` stat handles, builds one `H1Pool` per H1 cluster). The HCM proxy arm gets the pool manager plumbed alongside `cluster_mgr`.
3. NO new trait declared in `envoy-cluster`. NO new top-level Cargo dep. NO modification to `envoy-cluster::Cluster`'s struct shape (only `ConnGaugeGuard` gains a public `from_gauge` constructor — a 4-line addition).
4. NO ADR fires (lock-in #16). The cycle resolution is ordinary structure — the same shape 12.2 used.

If state-3 execution surfaces a non-obvious lifecycle complication (e.g. the HCM-config-construction site doesn't have ergonomic reach to `Arc<H1PoolManager>` and the plumbing is uglier than expected), the controller documents the surfacing finding in PROGRESS + lands an inline ADR resolving it. NOT projected.

### Carryforward dispositions entering 13.1

**Closures attributed at 13.1 (lock-in #15):**
- **06.3 REVIEW I2 (a)** (per-class downstream_rq_3xx/4xx/5xx + cluster.upstream_rq_5xx wire-level bilateral coverage) — **FULLY CLOSED at Task 7** (fixture 0020's per-class assertions). PROGRESS at Task 7 attributes the closure honestly.
- **06.3 REVIEW I2 (b)** (`cluster.<name>.upstream_cx_total` value-exact BEHAVIOR_CONTRACT row tightening) — primitive landed at Task 3 (the H1 pool itself), but the **contract row tightening DEFERS to 13.2** (where both H1 + H2 pool uniformly; the row at `BEHAVIOR_CONTRACT.md:89` mentions no protocol carve-out, so tightening at 13.1 would falsify the H2 surface). PROGRESS at Task 5 names the 13.2 D7.1 site.

**Carryforwards entering 13.1 (none gates state-2 or state-3):**
- **12.2 REVIEW Minor carryforwards: 11 active** (A-M2 `Scheduler::task_count` visibility; A-M4 `scheduler.rs:91` `.expect` un-tested negative; B-M1..B-M6 various backend/fixture/backstop nits; C-M1 + C-M2 + C-M4 narrative-finishing-touch). Close opportunistically when 13.1 touches the named seam (Task 3-4 touch envoy-http1 — opportunistic close on A-* not applicable since A-* are envoy-health; B-* on backend touched at Task 6 — opportunistic close on B-M1..M6 case by case).
- **12.1 REVIEW M1 + M3** (no-action style nits — carry forward).
- **Phase-11 REVIEW M1–M8** (next HTTP-filter-family phase — 13.1 does NOT touch HTTP-filter files).
- **10 M2/M3/M4/D1/D2/T1; 09 M1/D1/D2/T1/T2/T3; 08.2 M1-M8 + T1-T3; 08.1 M3; 07.2 M2/M3; 06.2 M1/M2/M4/M5; 06.1 M2/M3/M5/M6; 05.3 I2; 05.2 I1/I2/I3; 04.1 M5/M9/M-claim/M1/M2/M4/M7; 02.2 M1; Phase-00 I3** — all carry forward indefinitely per their existing named-owner dispositions. **Phase 13.1 touches** `envoy-http1` (new `pool.rs` + `hcm.rs:514` modify) + `envoy-config` (Cluster.circuit_breakers schema + validator) + `tests/helpers/health-aware-http1-backend/` (D8 additive extension) + `tests/fixtures/0020-*` (new) + `tests/differential/src/lib.rs` (D10 driver) + `crates/envoy-bin/tests/upstream_connection_pooling.rs` (new) + `crates/envoy-bin/src/main.rs` (H1PoolManager wire) + `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (2 new rows) — close opportunistically at named-seam tasks.

### Task ordering rationale

The 10-task organization (PLAN §Tasks 1-10) reflects a foundation-first cadence:
- **Tasks 1-2** — envoy-config schema + validator (no runtime impact; sets up the corpus seed at Task 9).
- **Task 3** — the architecturally headline H1Pool primitive + manager + idle sweeper, unit-tested in isolation (no HCM coupling — provable correctness before integration).
- **Task 4** — H1 HCM proxy-arm migration + `H1PoolManager` plumbing into HCM config + envoy-bin wire-up + tier-1 micro-cache removal + cx_total increment-site migration (the load-bearing integration task; 19 existing fixtures must regress-equivalence here per gate (b)).
- **Task 5** — D7 stats wiring + BEHAVIOR_CONTRACT rows (largely docs; the registrations themselves land at Task 3).
- **Task 6** — D8 backend extension (test-helper-only; no production touch).
- **Task 7** — fixture 0020 + D10 driver + Docker wrapper (the only fixture-adding task; lands the I2 (a) closure surface).
- **Task 8** — D9.3 in-process H1 backstop (envoy-bin/tests/-resident; subprocess discipline per 09 REVIEW M3).
- **Task 9** — D11 fuzz corpus seed (mechanical 3-sibling-file edit).
- **Task 10** — state-4 verification + STATE advance + push + CI confirm.

This is a PLAN-writer recommendation; the state-3 controller may reorganize within the constraints (Task 3 before Task 4; Tasks 1+2 before Task 9; Task 7 atomic per lock-in #4; Task 10 last).

---

*(State-3 task subsections append below as each task closes — `### Task 1 — ...` through `### Task 10 — state-4 phase-done verification + STATE advance`.)*
