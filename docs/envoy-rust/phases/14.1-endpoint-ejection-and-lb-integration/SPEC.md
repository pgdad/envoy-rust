# Phase 14.1 (`14.1-endpoint-ejection-and-lb-integration`) — SPEC

- **Phase id:** `14.1`
- **Slug:** `14.1-endpoint-ejection-and-lb-integration`
- **Status before this SPEC lands:** _not yet in ROADMAP.md_ (created at the parent-14 state-2 SPLIT commit; alongside this SPEC, ROADMAP gains a new row `14.1 status: planned` under the existing Upstream-robustness-family §9 table — see ADR-0040).
- **Parent SPEC:** `docs/envoy-rust/phases/14-outlier-detection/SPEC.md` (parent-14 state-1 brainstorm, committed at the predecessor of THIS commit; the parent SPEC §6.1 + §7 explicitly recommend this split shape).
- **Charter source:** `BOOTSTRAP_PROMPT.md` §9 — *"Upstream robustness family — outlier detection variants"* — projected onto a **foundation slice** that lands the schema + validator + per-endpoint state machine + load-balancer integration + stats wiring **without** the response-receipt hook + ejection sweeper + fixture (which defer to `14.2`). This slice is **inert when `outlier_detection` is unconfigured** — the 21 pre-existing Docker-gated fixtures stay green at simultaneous CI under the `BOOTSTRAP_PROMPT.md` §7.5 (b) regression-equivalence invariant.
- **Position in the project:** the **foundation slice** of the third concrete Upstream-robustness-family phase (parent-14 outlier detection). After 12.1 (active-HC `EndpointHealth` foundation) + 13.1 (H1 pool foundation), this is the **third foundation-slice sub-phase in the Upstream-robustness family** — the established post-MVP-trunk cadence (05.1 / 07.1 / 12.1) for landing inert machinery before the observable-behavior sibling sub-phase.
- **depends-on:** `04 06 12` — the H1 router-proxy arm at `crates/envoy-http1/src/router.rs::write_proxied_response` (phase 04) is where the response-receipt hook fires AT 14.2 (this slice declares the `Cluster::record_response` method seam but does NOT wire it from H1/H2). Phase 06 (`envoy-stats` foundation: `StatsRegistry` + Counter/Gauge primitives) is load-bearing for the new outlier-detection stat namespace. Phase 12 (the 12.1 per-endpoint `EndpointHealth` state machine + `Cluster::pick()` unhealthy-exclusion seam + 12.1 panic-threshold) is the structural sibling: outlier-detection ejection is independent of active-HC unhealth but reuses the same `pick()`-side exclusion seam — extends `pick()` with a sibling `!EndpointEjection::is_ejected()` filter ANDed with the existing `EndpointHealth::is_healthy()` filter. 14.1 does NOT depend on `13` (no connection-pool integration; the response-receipt hook in 14.2 fires at the router-arm site, not at the pool site).

---

## 1. Goal and acceptance signal

Phase 14.1 lands the **inert-when-unconfigured foundation** for passive outlier-detection ejection. When a cluster's `outlier_detection` block is configured, the parser accepts the minimum-viable schema (D1) and the validator enforces threshold + timing ranges (D2); the per-endpoint `EndpointEjection` state machine (D3) is constructed alongside the existing 12.1 `EndpointHealth` and registered with the cluster's per-endpoint metadata; `Cluster::pick()` (D5) gains a sibling `!is_ejected()` filter ANDed with the existing `!is_unhealthy()` filter; and the outlier-detection stat namespace (D6) is registered against the `Arc<StatsRegistry>`. **No response-receipt hook fires** (deferred to 14.2 D4) — the `EndpointEjection` state machine has its mutation surface defined but no caller; `is_ejected()` always returns `false` at 14.1 because no caller transitions the state. **No ejection sweeper spawns** (deferred to 14.2 D7) — the state machine has a `try_unEject` method defined but no caller. **No fixture lands** (deferred to 14.2 D8.1) — 14.1's acceptance is regression-equivalence on the 21 existing Docker-gated fixtures (the 05.1 / 07.1 / 12.1 foundation-slice pattern).

**Differential surface added by phase 14.1:** none. No new fixture. No behavior change on the 21 pre-existing fixtures (they configure no `outlier_detection`; the foundation machinery is inert).

**Acceptance signal (a)–(f), per `BOOTSTRAP_PROMPT.md` §7.5:**

- **(a)** No new fixture at 14.1; this gate is satisfied by the regression-equivalence assertion below.
- **(b)** All **21 pre-existing differential fixtures** (`0001-tcp-echo` through `0021-upstream-h2-connection-pooling`) **remain green simultaneously** at a single CI run vs `envoyproxy/envoy:v1.33.0` (the regression-equivalence invariant per §7.5 (b); the foundation machinery is inert when `outlier_detection` is unconfigured). The state-3 implementer verifies via a direct CI run at the state-4 verification.
- **(c)** `h2spec` continues at ≥95% (parent-05 baseline 99.31%). 14.1 does NOT touch the H2 framing/codec path. State-4 verification re-confirms the gate held.
- **(d)** `parse_bootstrap` fuzz target clean for the short-budget CI run on the **extended corpus** (one new seed for the `outlier_detection` bootstrap shape — corpus extends from 21 to 22 seeds per parent-14 SPEC D8.2). The 14.1 PLAN-writer's call on whether to land the corpus seed at 14.1 (foundation; the seed exercises ONLY the new envoy-config schema + validator) or 14.2 (alongside the fixture); **recommended posture: land at 14.1** (the seed exercises the new envoy-config surface that 14.1 introduces).
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check` all clean.
- **(f)** `REVIEW.md` approved.

A **single CI run** must light up gates (a)–(e) **simultaneously** (continues the project precedent — fixture inheritance is a regression vector even on no-new-fixture sub-phases).

---

## 2. Behavior-contract scope for phase 14.1

Phase 14.1 extends `docs/envoy-rust/BEHAVIOR_CONTRACT.md` with authored additions, landed at the tasks where each is first empirically exercised (per the established 06.x → 13.2 doctrine — contract extensions land at empirical-engagement task time, NOT at PLAN-write time and NOT at state-1 SPEC time).

### 2.1 "Stat-name mapping" extension — outlier-detection stats (§6.2-revised per ADR-0041)

Per the parent-14 SPEC §6.2 9-item empirical verification ratified by ADR-0041, the upstream Envoy v1.33.0 emission set under `cluster.<name>.outlier_detection.*` is **21 names** (NOT the 5 the parent SPEC §2.1 originally projected). The phase-14 minimum-viable subset envoy-rust emits is **7 names**:

| Stat name | Equivalence | Rationale |
|---|---|---|
| `cluster.<name>.outlier_detection.ejections_active` | value-exact (steady state; counts currently-ejected endpoints) | Gauge. The single source of truth for the active-ejection count; updates inline at each `record_response`-driven ejection AND each sweeper-driven un-ejection. Under fixture-0022's post-settle steady state both proxies converge to the same value. **Only gauge in the namespace.** |
| `cluster.<name>.outlier_detection.ejections_enforced_total` | value-exact | Counter. One increment per actual ejection (the consecutive-threshold crossed AND the max-ejection-percent cap honored). Sum across detector types modulo overflow. |
| `cluster.<name>.outlier_detection.ejections_overflow` | value-exact (0-case at fixture-0022's `max_ejection_percent: 100`) | Counter. Per the §6.2 item-4 finding: increments **per detection-tick** when a would-eject endpoint is held un-ejected because the cap is met, NOT once-per-host (overflow is a re-fire counter). Fixture-0022's `max_ejection_percent: 100` keeps this at 0. |
| `cluster.<name>.outlier_detection.ejections_detected_consecutive_5xx` | value-exact | Counter. Per-detector-type tick fired at every threshold-crossing, regardless of whether the cap permits enforcement. Sibling of `ejections_enforced_consecutive_5xx`. |
| `cluster.<name>.outlier_detection.ejections_enforced_consecutive_5xx` | value-exact | Counter. Per-detector-type tick fired only when the threshold-crossing actually drives an ejection. Equal to `ejections_detected_consecutive_5xx` minus the per-detector overflow share. At `enforcing_consecutive_5xx: 100` (the fixture-0022 setting and envoy-rust's only supported value at phase-14 scope per parent SPEC §4 deferral of `enforcing_*` knobs), `enforced` == `detected` modulo the cap. |
| `cluster.<name>.outlier_detection.ejections_detected_consecutive_gateway_failure` | value-exact (0-case at fixture-0022) | Counter. Same shape as the `_consecutive_5xx` sibling. The fixture-0022 backend serves 500 (NOT 502/503/504), so the gateway-failure detector never fires; both proxies emit 0. |
| `cluster.<name>.outlier_detection.ejections_enforced_consecutive_gateway_failure` | value-exact (0-case at fixture-0022) | Counter. Sibling of `_detected_consecutive_gateway_failure`. 0-case at fixture-0022. |

Envoy emits an additional **14 names** envoy-rust does NOT emit at phase 14 minimum-viable scope (deferred per parent SPEC §4): the `_detected_/_enforced_` pairs for `consecutive_local_origin_failure`, `success_rate`, `local_origin_success_rate`, `failure_percentage`, `local_origin_failure_percentage`; the legacy aliases `ejections_total` + `ejections_consecutive_5xx` + `ejections_success_rate`. The fixture-0022 expectations.yaml uses `allowlist_envoy_only` for these per the established differential-harness pattern (12.2 / 13.x precedent).

**Rows land at D6 (stats wiring) — Task TBD per the 14.1 PLAN-writer's call.**

### 2.2 No-healthy-upstream synth-503 path: UNCHANGED at 14.1

Phase 14.1's `Cluster::pick()` extension (D5) routes through the existing 12.2-landed no-healthy-upstream synth-503 path at `crates/envoy-http1/src/hcm.rs:582` + the H2 sibling at `crates/envoy-http2/src/hcm.rs` when filtering yields the empty set + panic doesn't fire. **No code change at 14.1 to the synth-503 path itself.** The path is reached only when ejection drives `pick() -> None`, which **cannot happen at 14.1** because no caller transitions `EndpointEjection` to ejected (the response-receipt hook lands at 14.2 D4). Effectively the synth-503 path is dormant at 14.1 wrt outlier-detection (still hot for the existing 12.x active-HC paths).

### 2.3 No DECISIONS.md amendment required at 14.1 state-2 PLAN-write

Phase 14.1's foundation surface is mechanical extension of the 12.1 / 13.x patterns. The split ADR (`ADR-0040`) and the §6.2 empirical-revision ADR (`ADR-0041`) BOTH land at the parent-14 state-2 SPLIT commit (THIS commit, the predecessor of 14.1's PLAN-write); 14.1's own state-2 PLAN-write commit lands no ADR projected per §7 below.

---

## 3. Deliverables

Phase 14.1's deliverables are **D1, D2, D3, D5, D6** from the parent-14 SPEC §3 (`D4` response-receipt hooks, `D7` sweeper, `D8` fixture/backstop/fuzz-seed defer to 14.2 per the split seam). The 14.1 PLAN-writer organizes these into tasks per `BOOTSTRAP_PROMPT.md` §5 state 2.

### D1 — `envoy-config` schema extension (`Cluster.outlier_detection`)

At `crates/envoy-config/src/bootstrap.rs`, extend the existing `Cluster` struct (the 12.1 + 13.1 extensions sit at the same site) with an `outlier_detection` field carrying the minimum-viable schema. Field hierarchy locked per §6.2 item-1 (defaults match the parent-14 SPEC's projection exactly):

```rust
pub struct Cluster {
    // ... existing fields (name, type, lb_policy, load_assignment, upstream_protocol,
    // health_checks, common_lb_config, circuit_breakers) ...
    #[serde(default)]
    pub outlier_detection: Option<OutlierDetection>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OutlierDetection {
    #[serde(default)]
    pub consecutive_5xx: Option<u32>,                 // Envoy default 5 (§6.2 item-1 confirmed)
    #[serde(default)]
    pub consecutive_gateway_failure: Option<u32>,     // Envoy default 5 (§6.2 item-1 confirmed)
    #[serde(default)]
    pub interval: Option<String>,                     // parse_duration; Envoy default 10s
    #[serde(default)]
    pub base_ejection_time: Option<String>,           // parse_duration; Envoy default 30s
    #[serde(default)]
    pub max_ejection_percent: Option<u32>,            // Envoy default 10 (range 0..=100)
    // Phase-14 DEFERRED — all rejected by deny_unknown_fields per §6.2 item-1 (Envoy accepts; envoy-rust does not):
    //   success_rate_*, failure_percentage_*, consecutive_local_origin_failure,
    //   split_external_local_origin_errors, enforcing_*, max_ejection_time,
    //   max_ejection_time_jitter
}
```

`parse_duration` (`crates/envoy-config/src/bootstrap.rs:2289`, accepts `1s`/`250ms`/`500us`) is reused directly. The 14.1 PLAN-writer verifies the exact line number at PLAN-time.

### D2 — `envoy-config` validator extension

At the cluster-validation site (the `from_bootstrap` / validator path producing `ConfigError`), add a `validate_outlier_detection(cluster) -> Result<(), ConfigError>` sub-validator. The validator checks per the parent-14 SPEC §3 D2 lock-ins:

- `consecutive_5xx >= 1` (when present) → `ConfigError::InvalidOutlierDetectionThreshold { cluster, field, value }`.
- `consecutive_gateway_failure >= 1` (when present) → same error.
- `interval` parses via `parse_duration` AND result is `> 0` → `ConfigError::InvalidOutlierDetectionTiming { cluster, field }` (reuses `parse_duration`'s error surface for parse failures).
- `base_ejection_time` parses via `parse_duration` AND result is `> 0` → same error.
- `max_ejection_percent` (when present) in `0..=100` → `ConfigError::InvalidMaxEjectionPercent { cluster, value }`.
- No-op `outlier_detection: {}` (both detector thresholds absent) accepted per §6.2 item-1 (Envoy v1.33.0 accepts; envoy-rust matches).

Roughly **3-4 new `ConfigError` variants** land at this site. Each carries `cluster: String` per the established envoy-config error-context discipline. Each has positive + negative parse-path unit tests.

### D3 — Per-endpoint ejection state machine (`envoy-cluster`)

In `crates/envoy-cluster/`, add a per-endpoint `EndpointEjection` type owned by `Cluster` alongside the 12.1 `EndpointHealth`. Per the parent-14 SPEC §3 D3 + the §6.2 item-3 + item-4 + item-5 lock-ins:

- **Initial state at construct:** `ejected_at_ns: 0` (NOT ejected); `consecutive_5xx: 0`; `consecutive_gateway_failure: 0`. Endpoints are implicitly never-ejected at boot — NO warmup window (§6.2 item-3 confirmed).
- **Mutation surface declared but no caller:** `record_response(status: u16) -> EjectionDecision` is declared on `EndpointEjection` and `Cluster::record_response(endpoint, status)` is declared on `Cluster`. Neither method has a CALLER at 14.1 — the response-receipt hook lands at 14.2 D4. Tests at 14.1 exercise the state-machine methods directly via test-only harness paths.
- **`max_ejection_percent` cap (§6.2 item-4 lock-in):** the cap formula at the ejection-decision site is `cap_count = floor(host_count * max_ejection_percent / 100)`. The overflow counter increments per detection-tick on cap-blocked enforcement (NOT one-shot per host). The cap check happens at `record_response`'s ejection-decision site (when present); at 14.1 it's exercised only via direct unit tests.
- **Per-detector counter reset semantics (§6.2 item-5 lock-in):** the per-endpoint `consecutive_5xx` and `consecutive_gateway_failure` counters reset to 0 on (a) any 2xx/3xx/4xx response, (b) un-ejection at sweep time. At 14.1 the un-eject side is exercised via direct unit tests; the response-receipt side defers to 14.2.
- **Synth-status classification (§6.2 item-9 lock-in):** the parent-14 SPEC §5.7 projection is NUANCED by the §6.2 item-9 finding — connect-failure is classified as BOTH `consecutive_5xx` AND `consecutive_gateway_failure` for the picked endpoint (mirrors Envoy); the `pick() -> None` synth-503 does NOT call `record_response` (no endpoint to attribute to). The classifier resides in `Cluster::record_response`'s prologue; the response-receipt hook (14.2 D4) supplies the status. At 14.1 the classifier's logic is exercised via direct unit tests on `EndpointEjection::record_response`.

The state machine lives in `envoy-cluster` (NOT a new crate) next to the 12.1 `EndpointHealth` — `pick()` reads both with no cross-crate dependency.

### D5 — Load-balancer integration (exclude ejected; preserve panic threshold)

Modify `crates/envoy-cluster/src/cluster.rs::Cluster::pick()` to consult `EndpointEjection` alongside the 12.1 `EndpointHealth` per parent-14 SPEC §3 D5:

- Build the candidate set as endpoints where **BOTH** `EndpointHealth::is_healthy()` AND `!EndpointEjection::is_ejected()` hold (the AND-composition per §6.2 item-7 lock-in).
- The 12.1 panic threshold continues to apply: when the **non-panic** filtered set is empty AND panic-routing is enabled (`healthy_fraction < panic_threshold`), `pick()` returns over the unfiltered set.
- When the filtered set is empty AND panic doesn't fire, `pick()` returns `None` → the existing 12.2-landed no-healthy-upstream synth-503 path fires unchanged.
- Clusters with NO `outlier_detection`: all endpoints implicitly never-ejected; `pick()` behaves exactly as 12.1.

### D6 — Outlier-detection stats wiring

At cluster construction (when `outlier_detection` is configured), register the 7 minimum-viable outlier-detection metrics (per §2.1) against the `Arc<StatsRegistry>`. The 06.x `register_counter`/`register_gauge` idempotent re-registration discipline applies.

**D6.1 — `Stat-name mapping` rows** (§2.1) land at the task where each stat is first registered.

---

## 4. Out of scope (deferred to 14.2 or out per parent §4)

- **D4 response-receipt hooks (H1+H2 router arms)** → 14.2.
- **D7 ejection sweeper + `OutlierManager`** → 14.2.
- **D8.1 fixture 0022 + Docker wrapper** → 14.2.
- **D8.3 in-process backstop** → 14.2.
- **D8.2 fuzz seed** → **recommended at 14.1** (the seed exercises the new envoy-config surface 14.1 introduces); 14.1 PLAN-writer's call. If deferred, 14.2 picks it up.
- **All success-rate-based / failure-percentage-based / local-origin-failure / `enforcing_*` / `max_ejection_time*` / event-log / TCP-side / H3-side detectors** — defer per parent-14 SPEC §4 (full list unchanged). Rejected by `deny_unknown_fields` per §6.2 item-1.
- **The `* num_ejections` ejection-time multiplier** — out per parent SPEC §4 (the §6.2 item-5 finding confirms it exists in Envoy but is not material at minimum-viable scope).

---

## 5. Architectural invariants

Phase 14.1 honors the parent-14 SPEC §5 invariants verbatim. The slice-specific signposts:

### 5.1 Crate boundaries

- **`EndpointEjection` state lives in `envoy-cluster`** so `Cluster::pick()` reads it with no cross-crate dependency. Mirrors the 12.1 `EndpointHealth` placement verbatim.
- **`Cluster::record_response` method declared but uncalled at 14.1.** The H1/H2 router-arm wiring lands at 14.2 D4. No cycle inversion required (the existing `envoy-http1` and `envoy-http2` already depend on `envoy-cluster` for `Arc<Cluster>`).
- **No new top-level Cargo dep; no new workspace member; no cycle.** All implementation uses existing pulled crates (std-lib + tokio + bytes + envoy-cluster + envoy-config + envoy-stats internal types).

### 5.2 Hand-rolled per D-3.2

Outlier detection's state machine + LB integration is hand-rolled per D-3.2 (parent §5.2). No `tower`-style passive-health-check library; no `rand` (no jitter at phase-14 scope).

### 5.3 Outlier-detection is inert when unconfigured (regression-equivalence — the load-bearing safety property at 14.1)

When `Cluster.outlier_detection` is absent, the per-endpoint `EndpointEjection` is constructed with default never-ejected state (a no-op construct cost); the response-receipt hook (14.2) short-circuits at the cluster-level `outlier_detection.is_none()` check; `Cluster::pick()` filters by `EndpointHealth::is_healthy()` AND `!EndpointEjection::is_ejected()` — and the latter ALWAYS returns `false` on never-ejected endpoints, so the effective filter is unchanged from 12.1. **The 21 existing fixtures see ZERO behavior change.** This is the load-bearing safety property at 14.1 (acceptance gate (b)).

The state-3 implementer verifies the inert path is the hot path (no per-pick or per-response branch cost on unconfigured-OD clusters beyond a cheap `Option::is_some()` check or an `AtomicU64::load(Ordering::Relaxed)` read).

### 5.4 The 12.1 `pick()` exclusion seam is reused (no new mechanism)

Phase 14.1 plugs into the same seam phase 12.1 deliberately reserved. The `Cluster::pick()` already filters by `EndpointHealth::is_healthy()`; 14.1 adds a sibling filter on `!EndpointEjection::is_ejected()`. Both signals AND together via boolean composition at the candidate-build site. **The architectural risk is LOW** — the dispatch + synth-503 seams are already wired and BEHAVIOR_CONTRACT-locked.

### 5.5 Stat-namespace lock-in per ADR-0041

The 7-name minimum-viable subset (§2.1) is locked by ADR-0041's §6.2 revision; envoy-rust does NOT emit the additional 14 Envoy-side names at phase-14 scope (deferred per parent §4). The fixture-0022 expectations.yaml at 14.2 D8.1 uses `allowlist_envoy_only` for the deferred names.

---

## 6. Implementation signposts for the planner

The 14.1 state-2 PLAN-writer reads this section to drive PLAN structure.

### 6.1 Split-gate evaluation at 14.1

Per `BOOTSTRAP_PROMPT.md` §6.1, the 14.1 state-2 PLAN-write evaluates whether the foundation-slice PLAN exceeds ~25 numbered tasks OR ~1500 LoC. Phase 14.1's surface estimate at THIS SPEC's time:

- D1 — envoy-config schema (`OutlierDetection`) (~80 LoC + ~120 LoC tests).
- D2 — envoy-config validator (3-4 ConfigError variants) (~80 LoC + ~140 LoC tests).
- D3 — `EndpointEjection` state machine + `OutlierDetectionConfig` (~180 LoC + ~220 LoC tests).
- D5 — `Cluster::pick()` ejection-filter integration (~50 LoC modify + ~110 LoC tests).
- D6 — stats wiring (4 counters + 2 counters + 1 gauge) + BEHAVIOR_CONTRACT rows (~90 LoC + ~90 LoC tests).
- D8.2 fuzz seed (optional at 14.1; ~30 LoC + 2 file edits).
- State-4 verification + STATE-advance (~docs).

**14.1 SPEC-time projection: ~10-13 tasks; ~700-900 LoC** (production ~380, tests ~480, fuzz seed ~30, docs). **Comfortably under the §6.1 gates.** No nested split projected.

### 6.2 Empirical verification — ratified by ADR-0041 at parent-14 state-2 SPLIT

The 14.1 state-2 PLAN-write does NOT re-run the §6.2 9-item empirical verification (the parent-14 state-2 split commit already ratified the findings via ADR-0041). The 14.1 PLAN-writer pulls forward the relevant §6.2 lock-ins:

- **Item 1 (config defaults):** `consecutive_5xx=5, consecutive_gateway_failure=5, interval=10s, base_ejection_time=30s, max_ejection_percent=10`. D1 + D2 lock these. The deferred-field list rejected by `deny_unknown_fields` is enumerated in §4 + the D1 doc comment.
- **Item 2 (stat namespace):** 7-name minimum-viable subset (§2.1). D6 locks. ADR-0041 ratifies the divergence from the parent SPEC's original 5-name projection.
- **Item 3 (initial state):** endpoints implicitly never-ejected at boot. D3 + D5 lock.
- **Item 4 (max_ejection_percent):** `floor(N*pct/100)` cap; overflow re-fires per detection-tick. D3 locks (state-machine logic at the ejection-decision site; exercised at 14.1 via direct unit tests).
- **Item 7 (composition with HC):** AND via independent health-flag bits at the `pick()` candidate-build site. D5 locks.

§6.2 items 5 (ejection-time), 6 (fixture observable), 8 (H1 vs H2 sibling), 9 (synth-status bypass + connect-failure classification) defer their PLAN lock-ins to **14.2** where the response-receipt hook (D4) + sweeper (D7) + fixture (D8.1) engage them.

### 6.3 PLAN-time SPEC corrections (per the 06.2 → 13.2 cadence)

The 14.1 state-2 PLAN-writer reads this SPEC against the PLAN-time HEAD and verifies the exact code surfaces:

- The exact `Cluster` struct fields + line in `crates/envoy-cluster/src/cluster.rs:60-76` (the 12.1 `Arc<EndpointHealth>` placement; 14.1 adds an `Arc<EndpointEjection>` sibling).
- The exact `pick()` signature in `crates/envoy-cluster/src/cluster.rs` (12.1 + 13.1 stable shape).
- The exact 12.1 `EndpointHealth` shape (the state-machine pattern 14.1 mirrors).
- The exact `parse_duration` signature in `crates/envoy-config/src/bootstrap.rs:2289` (reuse target).
- The exact 12.1 `validate_health_checks` site (the validator-pattern 14.1 mirrors at D2).

Corrections land in the 14.1 PROGRESS Task 1 preamble per the 06.2 → 13.2 cadence.

### 6.4 Subagent-driven execution at state 3 (per `feedback_execution_style`)

The user's standing preference auto-memory `feedback_execution_style` applies at 14.1 state 3. The 14.1 state-2 PLAN-writer organizes tasks for subagent-driven execution per the 06.x → 13.2 cadence.

### 6.5 The 06.x stats convention

StatsRegistry registration at cluster-construct time when `outlier_detection` is configured; per-cluster ownership of the Counter/Gauge handles; the `ejections_active` gauge updated inline at each ejection / un-ejection state transition (one source of truth, NOT polled — the 08.2 `server.live` / 12.1 `membership_healthy` pattern).

### 6.6 The BEHAVIOR_CONTRACT extension cadence

Contract extensions land at the TASK where each is first empirically exercised, NOT at PLAN-write and NOT at SPEC time. Stat rows (§2.1) at D6.

### 6.7 Cargo.lock cadence

Phase 04.1 REVIEW M5/M9 carries forward. Phase 14.1 adds zero new top-level Cargo deps.

### 6.8 Known-deferred small follow-up from 13.x (opportunistic close candidate)

The 13.1-surfaced **cluster per-class `upstream_rq_{2,3,4}xx` counter family extension** (carried from 13.1 / 13.2) is an opportunistic close candidate at 14.1 IF the PLAN-writer judges the extension cost is small (~30-50 LoC + 1 BEHAVIOR_CONTRACT row). Phase 14.1's surface touches cluster stats wiring (D6) but does NOT touch the HCM/router stats wiring sites where the per-class HCM counters live. **Recommended posture: defer** — 14.1's surface is unconnected to the per-class HCM counter site.

### 6.9 Carryforward inventory entering 14.1 (all carry forward unchanged per parent §6.3)

- 13.2 REVIEW 13 new Minors + 13.1 REVIEW 9 active Minors + 12.2 REVIEW 11 active Minors + 12.1 REVIEW M1/M3 + phase-11 M1-M8 + 10/09/08.x/07.2/06.x/05.x/04.1/02.2/00 residuals + 04.1 REVIEW M5/M9 Cargo.lock cadence — all carry forward unchanged. Phase 14.1 closes NO named carryforward.
- **A-M2** (13.2 stale `tokio::sync::Mutex` comment at `crates/envoy-http1/src/pool.rs:322`) — 14.1 does NOT touch `envoy-http1`; no opportunistic close at 14.1.
- **ADR-0028** (H1-listener × H2-cluster dispatch deferral) — carries forward per ADR-0039 Consequences.

---

## 7. ADR projection

**Recommended posture at 14.1 state-2 PLAN-write: NO new ADRs.** The DECISIONS.md ledger head after the parent-14 state-2 SPLIT commit is **ADR-0041**; the next-available number at 14.1 state-2 PLAN-write is **ADR-0042**.

Conditional ADR slots, reserved for 14.1 state-2 / state-3 landing:

- **Conditional ADR-0042 (option A — additional §6.2 nuance surfaced at PLAN-write).** Only if the PLAN-writer's verification-against-HEAD surfaces a new architectural constraint not covered by ADR-0041 (e.g., the exact `EndpointHealth` shape diverges materially from the SPEC's projection; the `pick()` signature requires a refactor that warrants an ADR). **Recommended posture: NO ADR projected** — the parent-14 + 12.1 + 13.x patterns are stable; ordinary corrections land as PLAN lock-ins in the PROGRESS Task 1 preamble.
- **Conditional ADR (foundations grant).** NOT PROJECTED — no external-crate dependency projected; uses only existing pulled crates.
- **Conditional ADR (cycle resolution).** NOT PROJECTED — the `EndpointEjection` lives in `envoy-cluster`; the `Cluster::record_response` method is declared on `Cluster` (no new trait, no new crate). The 14.2 D4 wiring will fire from `envoy-http1` and `envoy-http2` (both already depend on `envoy-cluster`).

At most ONE ADR lands per commit. **Recommended: no ADR fires at 14.1.**

---

## 8. State-machine signposts for the 14.1 state-2 session

The next session (14.1 state 2) reads this section and acts.

- **Lifecycle state at session start:** State 2 (SPEC.md exists; PLAN.md does not).
- **Skill:** `superpowers:writing-plans` per `BOOTSTRAP_PROMPT.md` §5 state 2.
- **Output:** `PLAN.md` (~10-13 tasks; ~700-900 LoC; under the §6.1 gates) + `PROGRESS.md` skeleton + Task 1 preamble (standalone pre-Task-1 commit per the 04.3 → 13.2 cadence). NO further split projected.
- **Empirical verification at 14.1 state 2:** does NOT re-run (parent §6.2 ratified by ADR-0041). The 14.1 PLAN-writer pulls forward the relevant lock-ins per §6.2 above.
- **PLAN-time SPEC corrections:** per the 06.2 → 13.2 cadence; corrections land in the PROGRESS Task 1 preamble.

---

## 9. Commit message format (for state 6 of the 14.1 lifecycle)

```
phase 14.1: outlier-detection schema + EndpointEjection state machine + pick() ejection-exclusion + stats foundation slice [ADR-NNNN, ...]

<1-3 sentence summary>

Differential surface: NO new fixture; all 21 Docker-gated fixtures (0001-0021) green simultaneously at CI run <ID> HEAD <SHA> (regression-equivalence per §7.5 (b); foundation machinery is inert when outlier_detection is unconfigured).
Conformance: h2spec ≥95% gate held at parent-05 baseline (H2 framing path untouched).
```

Per the 12.1 / 13.1 foundation-slice precedent. No `[parent 14 done]` tag (14.1 is NOT the closing sub-phase).

---

## 10. State-machine commit (THIS commit — parent-14 state-2 SPLIT closeout)

This SPEC is one of TWO sub-phase SPECs landing at the parent-14 state-2 SPLIT commit (alongside `14.2-response-receipt-hook-and-fixture/SPEC.md` + ADR-0040 (split) + ADR-0041 (§6.2 revision) + ROADMAP parent-row flip + 2 sub-phase rows + STATE.md advance to `14.1` state-2-next).

**Predecessor:** the parent-14 state-1 brainstorm commit (the immediate predecessor of THIS commit; SHA `542e8b5`).

**Origin/main:** `542e8b5` at THIS commit's prologue. After landing, the docs-only edits push to origin and the next CI run re-validates through the 5 stable-toolchain gates + the parse_bootstrap fuzz target on the unchanged 21-seed corpus.

---

*End of SPEC. Phase 14.1 lifecycle state 1 complete on landing of THIS parent-14 state-2 SPLIT commit. The next session enters 14.1 state 2 — writes PLAN.md per `superpowers:writing-plans`, performs the PLAN-time SPEC corrections per §6.3 against the PLAN-time HEAD, and lands `PLAN.md` + `PROGRESS.md` skeleton + Task 1 preamble in a single standalone pre-Task-1 commit.*
