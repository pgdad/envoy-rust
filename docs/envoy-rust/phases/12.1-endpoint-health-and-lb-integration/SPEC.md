# Phase 12.1 (`12.1-endpoint-health-and-lb-integration`) — SPEC

- **Phase id:** `12.1`
- **Slug:** `12.1-endpoint-health-and-lb-integration`
- **Parent:** `12` (`12-upstream-active-health-check`). This is the **first of two sub-phases** the parent-12 state-2 PLAN-write split phase 12 into, per `BOOTSTRAP_PROMPT.md` §6.1 (the parent-12 SPEC-time estimate ~1800–2200 LoC is materially over the ~1500-LoC split gate). The split decision + the seam rationale are recorded in **ADR-0036**; the full feature narrative + the carved-from parent scope live in `docs/envoy-rust/phases/12-upstream-active-health-check/SPEC.md`.
- **depends-on:** `02 04` (inherited from parent-12; `02` = the cluster manager + round-robin LB seam being extended; `04` = the `envoy-http1` codec/types foundation; `06` `envoy-stats` is the implicit stats foundation). 12.1 depends only on the MVP trunk being `done` — no dependency on 12.2.
- **Sub-phase ordering:** `12.1 → 12.2`, strict. 12.2 (the active-HC probe task + fixture) cannot land before 12.1 because 12.2's probe task mutates the `EndpointHealth` state machine that 12.1 introduces and the `pick()` unhealthy-exclusion seam that 12.1 wires.
- **Status before this SPEC lands:** `planned` (added as a sub-phase row at the parent-12 state-2 split commit).

---

## 1. Goal and acceptance signal

Phase 12.1 lands the **config + per-endpoint health-state + load-balancer-integration + stats foundation slice** of active HTTP health checking. After 12.1:

- `envoy-config` parses + validates a cluster `health_checks` block (HTTP-only, 0-or-1) + a `common_lb_config.healthy_panic_threshold` `Percent`.
- `envoy-cluster` carries a per-endpoint `EndpointHealth` state machine (consecutive-success/failure threshold transitions) and `Cluster::pick()` excludes unhealthy endpoints + honors the panic threshold.
- `envoy-stats` registers the `cluster.<name>.health_check.{attempt,success,failure}` counters + the `cluster.<name>.membership_healthy` gauge.

**12.1 lands NO new differential fixture, and NO probe task.** This is the **foundation-slice pattern** established at phase 05.1 (the `STRICT_DNS` config preamble) and phase 07.1 (the `envoy-filter` framework foundation): the seam is wired + unit-tested, and regression-equivalence is the differential proof — all **18 existing Docker-gated fixtures** (`0001-tcp-echo` through `0018-http-filter-fault`) stay green simultaneously, proving the new machinery is **inert when `health_checks` is unconfigured** (§5.4 of the parent SPEC). The active-HC probe task that *drives* `EndpointHealth` + the differential fixture that exercises the no-healthy-upstream path land in **12.2**.

> **Inert-and-unexercised seam (read carefully — the §6.2 item-1 finding makes this load-bearing).** Empirical verification (parent SPEC §6.2; findings reproduced in §2 below) confirmed that **upstream Envoy starts each endpoint of an active-HC cluster in an UNHEALTHY/pending state** — a host is not marked healthy until the first health check passes (`healthy_threshold` consecutive successes). 12.1 lands the same initial-state semantics. **Consequence:** in the 12.1-only world, a cluster *configured* with `health_checks` but with no probe task yet (the task lands in 12.2) would have all endpoints stuck at the initial unhealthy state → `pick()` → `None` → synth-503 for all of its traffic. **12.1 therefore ships NO fixture or test that configures `health_checks` on a traffic-serving cluster** — the configured-HC path stays unexercised until 12.2 wires the probe task + the fixture together. The 18 existing fixtures configure NO `health_checks`, so their `EndpointHealth` is absent and `pick()` behaves exactly as phase-02 round-robin (§5.4). This is the same "land the seam; the consumer lands in the next slice" discipline as 07.1 (which shipped the `Decision::StopAndSend` enum that no 07.1 filter emitted).

**Acceptance signal (a)–(f), per `BOOTSTRAP_PROMPT.md` §7.5:**

- **(a)** No new fixture. *(12.1 has no new differential fixture; gate (a) is satisfied vacuously — the 05.1/07.1 precedent.)*
- **(b)** All **18 pre-existing differential fixtures** (`0001` through `0018`) remain green simultaneously at the same Docker-gated CI run (regression-equivalence; the load-bearing 12.1 differential proof — the new health-check machinery must be inert when unconfigured).
- **(c)** `h2spec` continues at ≥95% (parent-05 baseline 99.31%). 12.1 touches no H2 codec/framing path.
- **(d)** `parse_bootstrap` fuzz target clean for the short-budget CI run. **12.1 extends the seed corpus 18 → 19** with the health-check bootstrap shape (the seed exercises the new D1 schema + D2 validator parse path). *(Per the 07.2/09/10/11 corpus-seed precedent — the new schema is the natural fuzz surface; landing the seed in 12.1 where the schema lands is cleaner than deferring it to 12.2.)*
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check` all clean.
- **(f)** `REVIEW.md` approved.

A **single CI run** must light gates (a)–(e) simultaneously (the project precedent).

---

## 2. Empirical findings inherited from the parent-12 state-2 verification (locked facts)

The parent-12 state-2 PLAN-write performed the parent SPEC §6.2 HEAVY 6-item empirical verification against `envoyproxy/envoy:v1.33.0` (Docker; an active-HC cluster + a synthetic health-aware backend + admin `/stats`; methodology + the full findings table are in the STATE.md `### Phase-12 state-2 split decision` subsection). The findings that bind **12.1**:

1. **Initial endpoint health state = UNHEALTHY/pending-until-first-pass** (the single most load-bearing item). With an unhealthy backend + a 3 s probe delay, the data plane returned `no healthy upstream` (503) from t=0 — *before* the first probe could resolve. Hosts with active HC configured begin not-healthy; the first passing check (`healthy_threshold` consecutive successes) marks them healthy. **MATCHES the parent SPEC's recommended projection.** → D2 (`EndpointHealth` initial state).
3. **Panic threshold:** default **50 %**; comparison **strictly-below** (`healthy_percent < panic_threshold`: at `value: 0`, `0 % < 0 %` is false → no panic → synth-503; at default 50 %, `0 % < 50 %` is true → panic → routes to all); `Percent` config shape is **`{ value: <double> }`** (e.g. `{ value: 0 }`); panic mode round-robins over **ALL** endpoints (healthy + unhealthy); upstream also emits a `cluster.<name>.lb_healthy_panic` counter (NOT wired at 12.1 scope — defers). **MATCHES projection.** → D1 (`CommonLbConfig`/`Percent`) + D5 (panic logic).
4. **Health-check stat names** under `cluster.<name>.health_check.*`: `attempt`, `success`, `failure`, `passive_failure`, `network_failure`, `verify_cluster`, `healthy`, `degraded`; membership gauges `membership_{healthy,total,degraded,excluded,change}`. `membership_healthy` = count of currently-healthy endpoints (1 when healthy, 0 when ejected; converges to a deterministic steady state). **MATCHES projection.** 12.1 wires the minimum-viable subset: the `attempt`/`success`/`failure` counters + the `membership_healthy` gauge (the rest defer with their features per parent SPEC §4). → D4 (stats).
5. **HTTP probe shape (the parts that bind 12.1's schema):** default `expected_statuses` = **exactly 200** (a `/healthz` returning 201 marked the endpoint unhealthy); `Int64Range` is **half-open `[start, end)`** (`[{start:200,end:201}]` excludes 201; `[{start:200,end:202}]` includes 201) — confirming the existing 04.2 `Int64Range` (`crates/envoy-config/src/bootstrap.rs:1080`) is reusable directly for `expected_statuses`. **MATCHES projection.** → D1 (`HttpHealthCheck.expected_statuses`).
6. **Duration config shape (MATERIAL DIVERGENCE — affects D1/D2 + the 12.2 fixture):** upstream Envoy accepts decimal-second protobuf-JSON durations (`1s`, `0.5s`, `0.25s`) but **REJECTS `500ms`**; envoy-rust's `parse_duration` (`crates/envoy-config/src/bootstrap.rs:2289`) accepts `1s`/`500ms`/`500us` (integer value + `s`/`ms`/`us` suffix) but **REJECTS `0.5s`** (it parses the numeric part as `u64`, so `"0.5"` fails). **The only duration form both parsers accept is integer seconds (`1s`, `2s`, …).** → **PLAN-time correction:** 12.1's D1 reuses `parse_duration` as-is (no `0.5s` support is added — that is scope creep); the validator (D2) parses `timeout`/`interval` via `parse_duration`, so any sub-second `0.5s` in an envoy-rust config is *rejected at validate time*. The 12.2 fixture (which 12.2 owns) must use integer-second durations (`interval: 1s`, `timeout: 1s`) on both proxy sides so the two YAMLs stay identical. No ADR (this is a fixture-shape / parser-reuse decision, not a wire-level contract change).

> Item **2** (the no-healthy-upstream synth body bytes) is a 12.2-binding finding (it is exercised by the 12.2 fixture + the synth-503 reconciliation) and is recorded in the 12.2 SPEC + **ADR-0037**. 12.1 does not touch the synth-503 writer path.

---

## 3. Deliverables

12.1 carries parent-12 deliverables **D1, D2, D3, D5, D6**. The state-2 PLAN-writer for 12.1 organizes these into TDD tasks for subagent-driven execution.

### D1 — `envoy-config` schema extension

At `crates/envoy-config/src/bootstrap.rs`, extend the `Cluster` struct (`bootstrap.rs:56`; confirmed at HEAD to carry `name`, `cluster_type`, `lb_policy`, `load_assignment`, `transport_socket`, `dns_lookup_family`, `typed_extension_protocol_options` — all with `#[serde(deny_unknown_fields)]`) with two new optional fields:

```rust
pub struct Cluster {
    // ... existing fields ...
    #[serde(default)]
    pub health_checks: Vec<HealthCheck>,          // OPTIONAL; phase-12 supports exactly 0 or 1, HTTP-only
    #[serde(default)]
    pub common_lb_config: Option<CommonLbConfig>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HealthCheck {
    pub timeout: String,                          // parse_duration (integer s/ms/us); per-probe response timeout
    pub interval: String,                         // parse_duration; between probes
    pub healthy_threshold: u32,                   // consecutive successes to mark healthy
    pub unhealthy_threshold: u32,                 // consecutive failures to mark unhealthy
    pub http_health_check: HttpHealthCheck,       // REQUIRED at phase 12 (the only supported checker type)
    // OPTIONAL upstream fields that defer per parent SPEC §4 are REJECTED by deny_unknown_fields:
    //   tcp_health_check / grpc_health_check / custom_health_check
    //   no_traffic_interval / unhealthy_interval / interval_jitter / reuse_connection
    //   no_traffic_healthy_interval / initial_jitter / event_log_path / always_log_health_check_failures
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HttpHealthCheck {
    pub path: String,                             // REQUIRED at phase 12
    #[serde(default)]
    pub host: Option<String>,                     // OPTIONAL :authority / Host on the probe; defaults to the
                                                  // cluster name per upstream (§6.2 item-5 confirmed)
    #[serde(default)]
    pub expected_statuses: Vec<Int64Range>,       // OPTIONAL; default = exactly 200 (§6.2 item-5 confirmed);
                                                  // reuses the 04.2 Int64Range (half-open [start, end))
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CommonLbConfig {
    #[serde(default)]
    pub healthy_panic_threshold: Option<Percent>, // default 50% per upstream; { value: 0 } disables panic
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Percent {
    pub value: f64,                               // upstream type.v3.Percent { value: double } (§6.2 item-3 confirmed)
}
```

- **`Int64Range`** (`bootstrap.rs:1080`, with `ConfigError::InvalidInt64Range` validation) and **`parse_duration`** (`bootstrap.rs:2289`) are **reused directly** — no duration/range primitive is duplicated.
- **`Percent` is a new envoy-config type** (recommended path (a) in parent SPEC §D1): `{ value: f64 }`, matching upstream's `type.v3.Percent` (§6.2 item-3 confirmed `{ value: 0 }` accepted). NOT the phase-11 `FractionalPercent` (numerator/denominator — structurally distinct).
- `Cluster` derives `Serialize` (for `/config_dump`); the new fields/structs must derive `Serialize` too (the 08.1 `Bootstrap` Serialize cascade carries forward — the new structs feed `/config_dump`). Confirm the existing `#[derive(Debug, Serialize, Deserialize, PartialEq)]` cascade and match it.

### D2 — `envoy-config` validator extension

At the cluster-validation site (the existing `from_bootstrap`/validator path producing `ConfigError`; `ConfigError` lives in `crates/envoy-config/src/lib.rs` per the phase-11 SPEC-correction precedent), add a `validate_health_checks(cluster) -> Result<(), ConfigError>` sub-validator. Checks:

- **At most one** `health_checks` entry → `ConfigError::UnsupportedMultipleHealthChecks { cluster }` on `len > 1`.
- The single check **is HTTP** (`http_health_check` present; TCP/gRPC/custom rejected). *(In the chosen schema only `http_health_check` exists, so a non-HTTP checker is rejected by `deny_unknown_fields` at parse time. The PLAN-writer decides whether a distinct `UnsupportedHealthCheckType` variant is still warranted for forward-compat clarity, mirroring the phase-09/10/11 reject-variant discipline — recommended: yes, a stub-friendly variant the validator can grow.)*
- `healthy_threshold >= 1` AND `unhealthy_threshold >= 1` → `ConfigError::InvalidHealthCheckThreshold { cluster, field }`.
- `timeout` and `interval` parse via `parse_duration` AND are `> 0` → `ConfigError::InvalidHealthCheckTiming { cluster, field }`. *(§6.2 item-6: a sub-second `0.5s` fails `parse_duration` and surfaces here as an `InvalidHealthCheckTiming`.)*
- `http_health_check.path` non-empty → `ConfigError::EmptyHealthCheckPath { cluster }`.
- each `expected_statuses` range is a valid `Int64Range` (delegates to the existing `Int64Range` validation; half-open `[start, end)` per §6.2 item-5).
- if `common_lb_config.healthy_panic_threshold` present, `value` in `[0.0, 100.0]` → `ConfigError::InvalidPanicThreshold { cluster, value }`.

Roughly **4–6 new `ConfigError` variants** (the PLAN-writer may consolidate; each carries `cluster: String` per the established error-context discipline). Each has positive + negative parse-path unit tests. The validator is exercised by the `parse_bootstrap` fuzz target (the D-corpus seed below seeds it).

### D3 — Per-endpoint health state machine (`envoy-cluster`)

In `crates/envoy-cluster/`, add a per-endpoint `EndpointHealth` type carrying runtime health state, owned by the `Cluster` and consulted by `pick()`:

```rust
/// Per-endpoint active-health-check state. Shared (Arc) so the 12.2 health-check
/// task can mutate it while pick() (D5) reads it.
pub struct EndpointHealth {
    state: AtomicU8,                 // Healthy | Unhealthy discriminant
    consecutive_success: AtomicU32,
    consecutive_failure: AtomicU32,
    healthy_threshold: u32,
    unhealthy_threshold: u32,
}

impl EndpointHealth {
    /// Record a probe success; transition Unhealthy -> Healthy after
    /// `healthy_threshold` consecutive successes. Resets the failure counter.
    pub fn record_success(&self);
    /// Record a probe failure; transition Healthy -> Unhealthy after
    /// `unhealthy_threshold` consecutive failures. Resets the success counter.
    pub fn record_failure(&self);
    pub fn is_healthy(&self) -> bool;
}
```

- **Initial state = Unhealthy** (§6.2 item-1, the load-bearing finding): a freshly-constructed `EndpointHealth` for an active-HC-configured endpoint starts Unhealthy and requires `healthy_threshold` consecutive successes (driven by the 12.2 task) to become Healthy. Unit tests cover: initial-unhealthy; `healthy_threshold` successes flip to Healthy; `unhealthy_threshold` failures flip to Healthy→Unhealthy; the counter-reset-on-opposite-result semantics; threshold > 1 edge cases.
- The state machine lives in `envoy-cluster` (NOT a new crate) so `pick()` reads it with no cross-crate dependency. The task that *mutates* it lives in 12.2's `envoy-health` crate (parent SPEC §5.1 dependency-cycle constraint).
- **Membership wiring (§5.4 inert-when-unconfigured):** `EndpointHealth` is constructed **only when the cluster has a `health_checks` entry**. A cluster with no `health_checks` carries no `EndpointHealth` (or an always-healthy sentinel) → `pick()` behaves exactly as phase-02 round-robin. This is the regression-equivalence safety property; unit tests assert a no-health-check cluster's `pick()` is unchanged.

### D5 — Load-balancer integration (exclude unhealthy + panic threshold)

Modify `Cluster::pick()` (`crates/envoy-cluster/src/cluster.rs:129`; confirmed at HEAD as a round-robin over `self.endpoints` returning `Option<SocketAddr>`, delegated from `ClusterHandle::pick_endpoint()` at `:152`, annotated *"`Option<_>` is preserved for phase-06+ health checking"*):

- Build the candidate set as the **healthy** endpoints; round-robin over healthy endpoints only.
- **Panic threshold (§6.2 item-3):** compute `healthy_percent = 100.0 * healthy_count / total_count`. If `healthy_percent < panic_threshold` (default 50.0; from `common_lb_config.healthy_panic_threshold.value`), enter **panic mode** — round-robin over **ALL** endpoints (healthy + unhealthy). `healthy_panic_threshold: { value: 0 }` disables panic (`0.0 < 0.0` is false → never panics → a 0-healthy cluster returns `None`). **Strictly-below comparison** (`<`, not `<=`) per §6.2 item-3.
- When the (non-panic) healthy set is empty, `pick()` returns `None` — the **pre-built** no-healthy-endpoint → synth-503 path fires unchanged (`crates/envoy-http1/src/hcm.rs:582` `synth_status(503, close)`; `RouterError::NoHealthyEndpoint`). **12.1 does NOT touch the synth-503 writer path** (the body reconciliation per ADR-0037 lands in 12.2 with the fixture that exercises it). 12.1's `pick()` change makes `None` *reachable* for configured-HC clusters; 12.2's task + fixture *exercise* it.
- **Clusters with NO `health_checks`:** all endpoints implicitly healthy → `pick()` unchanged → the 18 existing fixtures see zero behavior change (acceptance gate (b)).

Unit tests: round-robin over a mixed healthy/unhealthy set excludes the unhealthy; panic engages at `healthy_percent < threshold` and routes to all; `value: 0` + 0-healthy → `None`; default 50 % + 0-healthy → panic-routes-to-all; a no-health-check cluster is unchanged.

### D6 — Health-check stats wiring + BEHAVIOR_CONTRACT extension

At cluster construction (or `EndpointHealth` construction), register against the `Arc<StatsRegistry>` (the 06.x convention; idempotent same-name re-registration):

- `cluster.<name>.health_check.attempt` / `.success` / `.failure` (counters). At 12.1 these are **registered but not yet incremented by a probe** (the 12.2 task increments them) — they exist + read 0. *(This is the 07.1 `Decision::StopAndSend`-style forward-wiring: the stat handles land with the seam; the increment site lands with the consumer in 12.2. The PLAN-writer confirms registering a counter that stays 0 until 12.2 is clean under clippy — no dead-code lint, mirroring the way 07.1 landed the unused enum variant. If clippy flags an unused handle, the PLAN-writer either threads the handle into `EndpointHealth` for 12.2 to increment, or defers the 3 counter registrations to 12.2 and lands only the `membership_healthy` gauge at 12.1. Recommended: land the gauge at 12.1; defer the 3 counters to 12.2 where they are incremented — cleaner than a 0-forever registered counter. The 12.1 PLAN-writer makes the call and records it as a lock-in.)*
- `cluster.<name>.membership_healthy` (gauge; the count of currently-healthy endpoints). Updated inline at every `EndpointHealth` state transition (one source of truth, NOT polled — the 08.2 `server.live`/`server.state` inline-CAS pattern). At 12.1, with no probe task, a configured-HC cluster's gauge reads its initial value (0, all endpoints start unhealthy); 12.2's task drives it. Since 12.1 ships no configured-HC fixture, the gauge's steady-state values are exercised by 12.2.

**BEHAVIOR_CONTRACT.md** — the `Stat-name mapping` rows for `cluster.<name>.health_check.{attempt,success,failure}` (name-required, value-may-differ — timing-dependent) + `cluster.<name>.membership_healthy` (value-exact in steady state) land at the D6 task **only for stats 12.1 actually wires** (the gauge if the PLAN-writer lands counters in 12.2). Per the 06.x → 11 cadence, a stat row lands at the task where the stat is first wired; rows for stats deferred to 12.2 land in 12.2.

### D-corpus — fuzz seed (acceptance gate (d))

New seed `crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_health_check.yaml` carrying the health-check bootstrap shape (an active-HC cluster with `common_lb_config.healthy_panic_threshold`, integer-second durations per §6.2 item-6). Extends the corpus 18 → 19, with the `crates/envoy-config/fuzz/.gitignore` allow-list extension AND the `bootstrap.rs::tests::fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS-array extension (both files edited together — the 09/10/11 Task-6 lesson).

---

## 4. Out of scope for 12.1 (lands in 12.2 or defers per parent SPEC §4)

- **The active-HC periodic-probe task + the `envoy-health` crate** (parent D4) — 12.2.
- **The differential fixture `0019` + the synthetic-backend harness primitive (the 06.3 REVIEW I2 down-payment) + the settle-then-probe driver + the Docker wrapper** (parent D7.1) — 12.2.
- **The no-healthy-upstream synth-503 body reconciliation** (parent D6.2 / §2.2; the empty → `no healthy upstream` 19-byte change per ADR-0037) — 12.2 (co-located with the fixture that exercises it bilaterally).
- **The in-process backstop** (parent D7.3) — 12.2.
- Everything in the parent SPEC §4 deferral list (TCP/gRPC/custom checkers; multiple health checks; outlier detection; circuit breakers; retries; connection pooling; jitter/`reuse_connection`/`no_traffic_interval`; degraded/excluded host states; HC over H2 upstream; `lb_healthy_panic` + the non-wired `health_check.*` counters).

---

## 5. Architectural invariants (inherited from parent SPEC §5)

- **§5.1 crate boundaries:** `EndpointHealth` STATE lives in `envoy-cluster` (so `pick()` reads it cycle-free). No new crate at 12.1 (`envoy-health` is a 12.2 deliverable). No new path-dep.
- **§5.2 hand-rolled per D-3.2:** the state machine + threshold transitions + panic check are written from scratch atop std + the existing `AtomicU8`/`AtomicU32` + `envoy-stats`. No new top-level Cargo dep. No `unsafe` (every crate root keeps `#![forbid(unsafe_code)]`).
- **§5.4 inert-when-unconfigured (the load-bearing 12.1 safety property):** no `health_checks` ⇒ no `EndpointHealth` ⇒ `pick()` is phase-02 round-robin ⇒ the 18 existing fixtures unchanged. This is acceptance gate (b) and the entire reason 12.1 needs no new fixture.
- **§5.6 steady-state health decision** + **§5.7 the pre-built `pick() -> Option` + no-healthy-503 seam** carry forward; 12.1 plugs `pick()` into the seam without touching the writer arm.

---

## 6. Signposts for the 12.1 state-2 PLAN-writer

- The empirical §6.2 verification is **already done** (parent-12 state-2; findings in §2 above + STATE.md `### Phase-12 state-2 split decision`). **The 12.1 PLAN-writer does NOT re-run Docker** — the findings are locked facts. (If a 12.1 implementation detail surfaces a new empirical question, verify it then.)
- **PLAN-time SPEC corrections** (read this SPEC against HEAD; the parent verified these at split time): `Cluster` struct fields at `bootstrap.rs:56` (no `upstream_protocol` field — protocol is via `typed_extension_protocol_options`); `pick()` signature + `pick_endpoint()` seam at `cluster.rs:129`/`:152`; `parse_duration` at `bootstrap.rs:2289` (integer-only; rejects `0.5s`); `Int64Range` at `bootstrap.rs:1080` (half-open); `ConfigError` lives in `lib.rs`; `synth_status` at `hcm.rs:918` (the current empty-body 503 — NOT touched at 12.1). Corrections land in the PROGRESS Task 1 preamble.
- **Stats wiring decision (D6):** decide whether the 3 `health_check.*` counters land at 12.1 (registered-but-0) or defer to 12.2 (registered-and-incremented). Recommended: gauge at 12.1, counters at 12.2 (avoids a 0-forever registered counter / clippy dead-handle friction). Record as a lock-in.
- **`UnsupportedHealthCheckType` variant:** recommended to land it (forward-compat clarity), even though `deny_unknown_fields` already rejects non-HTTP checkers structurally.
- **Subagent-driven execution at state 3** per `feedback_execution_style`. Organize tasks: D1 schema (+ Percent) → D2 validator (+ ConfigError variants) → D3 EndpointHealth → D5 pick() integration → D6 stats + contract row → D-corpus fuzz seed → state-4 verification (18 fixtures green + 5 gates + h2spec ≥95% + fuzz on the 19-seed corpus).
- **Carryforward:** 12.1 engages no carryforward (the 06.3 REVIEW I2 down-payment is a 12.2 deliverable — the synthetic backend). The inherited inventory (parent SPEC standing list) carries forward unchanged.

---

## 7. ADR projection for 12.1

**Recommended posture: NO new ADR lands during the 12.1 lifecycle.** ADR-0036 (split) + ADR-0037 (no-healthy-body empirical reconciliation) both land at the parent-12 state-2 split commit (before 12.1 begins). 12.1 introduces no new crate, no foundations grant, no wire-level contract revision (the schema + state machine + LB-integration are ordinary hand-rolled deliverable work). DECISIONS.md ledger head is **ADR-0037** at 12.1 start; next available **ADR-0038**. A 12.1 ADR lands only if execution surfaces a genuine ambiguity (e.g., a non-obvious `EndpointHealth` memory-ordering decision warranting durable record — unlikely; the `Relaxed`-ordering precedent at `cluster.rs` pick() covers it).

---

## 8. Commit message format (for state 6 of the 12.1 lifecycle)

```
phase 12.1: cluster health_checks schema + EndpointHealth state machine + pick() unhealthy-exclusion + panic threshold + health_check stats

<1-3 sentence summary>

Differential surface: no new fixture; all 18 Docker-gated fixtures (0001-0018) green simultaneously at CI run <ID> HEAD <SHA> (regression-equivalence — health-check machinery inert when unconfigured).
Conformance: h2spec ≥95% gate held at parent-05 baseline (H2 framing path untouched).
```

(No `[ADR-NNNN]` bracket unless a 12.1 ADR lands per §7.)

---

*End of 12.1 SPEC. The seam (config + EndpointHealth + LB-integration + stats) lands here; the probe task that drives it + the differential fixture land in 12.2.*
