# Phase 14 (`14-outlier-detection`) — SPEC

- **Phase id:** `14`
- **Slug:** `14-outlier-detection`
- **Status before this SPEC lands:** _not yet in ROADMAP.md_ (per `docs/envoy-rust/ROADMAP.md` at HEAD `96630f9`, the phase-13.2 state-6 CLOSING-sub-phase close-out commit that simultaneously flipped row `13.2` AND parent row `13` `in-progress → done`; the "Upstream robustness family" §9 table at that HEAD carries the parent-12 row + sub-rows `12.1` + `12.2` + the parent-13 row + sub-rows `13.1` + `13.2`, all `status: done` — no row exists yet for outlier detection). **This SPEC's landing commit adds the THIRD concrete row beneath the "Upstream robustness family" heading**, with `status: planned`.
- **Charter source:** `BOOTSTRAP_PROMPT.md` §9 — *"Upstream robustness family — active health checks (HTTP/TCP/gRPC/custom), **outlier detection variants**, circuit breakers, retries + hedging, per-protocol connection pooling."* This phase lands **passive health checking (outlier detection)** — per-cluster `outlier_detection` config that observes data-plane response statuses and ejects endpoints that produce too many consecutive 5xx (or consecutive gateway-failure) responses. The minimum-viable variants are `consecutive_5xx` + `consecutive_gateway_failure`; success-rate-based, failure-percentage-based, and `consecutive_local_origin_failure` defer per §4.
- **Position in the project:** the **sixth post-MVP-trunk feature-family phase** and the **third concrete Upstream-robustness-family phase** (after parent-12 active HTTP health checking + parent-13 per-protocol upstream connection pooling). The MVP trunk 00→08 + the three HTTP-filter-family phases (09 `local_ratelimit`, 10 `rbac`, 11 `fault`) + parent-12 (sub-phases 12.1 + 12.2) + parent-13 (sub-phases 13.1 + 13.2) all stand `done`. The **21-Docker-gated-fixture regression baseline** established at phase-13.2 close (`0001-tcp-echo` through `0021-upstream-h2-connection-pooling`) carries forward unchanged per `BOOTSTRAP_PROMPT.md` §7.5 (b).
- **depends-on:** `04 06 12 13` — phase `04` (the H1 router-proxy arm at `crates/envoy-http1/src/hcm.rs` proxy-arm site + `crates/envoy-http1/src/router.rs::write_proxied_response`) and the H2 router-proxy arm landed at parent-05 (the H2 HCM proxy-arm site at `crates/envoy-http2/src/hcm.rs`) are the response-receipt sites where the new outlier-detection hook fires. Phase `06` (the `envoy-stats` foundation: `StatsRegistry` + `Counter`/`Gauge` primitives) is load-bearing for the new outlier-detection stat namespace. Phase `12` (the per-endpoint `EndpointHealth` state machine at `envoy-cluster` + the `Cluster::pick()` unhealthy-exclusion + panic-threshold seam + the 12.2 no-healthy-upstream synth-503 path) is the structural sibling: outlier-detection ejection is independent of active-HC unhealth but reuses the same `pick()`-side exclusion seam. Phase `13` (the H1+H2 connection pools + the 13.x configurable-status synthetic backend at `tests/helpers/health-aware-http1-backend/` with `--per-path PATH=STATUS,...`) is the structural precedent: phase-14's fixture reuses the configurable-status backend, and phase-14's per-cluster periodic-background ejection sweeper is the **fourth periodic-background primitive** (after 12.2's active-HC scheduler + 13.1's H1 pool idle sweeper + 13.2's H2 pool idle sweeper) sharing identical `tokio_util::sync::CancellationToken` cancellation discipline.
- **Brainstorm narrative:** see the "Phase-14 state-1 brainstorm" subsection of `docs/envoy-rust/STATE.md` for the family-pick + feature-pick rationale.

---

## 1. Goal and acceptance signal

Phase 14 lands **passive health checking via outlier detection** as the third concrete Upstream-robustness-family feature. When a cluster's `outlier_detection` block is configured, envoy-rust observes every upstream response status the router proxy-arm sees, counts consecutive 5xx (and consecutive gateway-failure 502/503/504) responses per-endpoint, and **ejects** an endpoint that crosses the configured threshold. Ejected endpoints are excluded from `Cluster::pick()` (alongside active-HC-unhealthy endpoints) for `base_ejection_time`; a per-cluster periodic sweeper un-ejects past-deadline endpoints subject to a `max_ejection_percent` safety cap.

The phase reuses the load-bearing seams from parent-12 + parent-13:

- The `pick()` exclusion seam from 12.1 (`Cluster::pick()` filters by `EndpointHealth::is_healthy()`) gains a second filter on a new `EndpointEjection::is_ejected()`. Both signals must pass for an endpoint to be picked — active-HC unhealth (12.x) and outlier-detection ejection (14) compose independently.
- The no-healthy-upstream synth-503 path from 12.2 (`crates/envoy-http1/src/hcm.rs:582` + the H2 sibling) fires unchanged when ejection (alone or combined with active HC) drives `pick() -> None`.
- The configurable-status synthetic backend from 13.x is reused as fixture 0022's data-plane backend (no harness extension required).
- The periodic-background-primitive cadence from 12.2 + 13.1 + 13.2 (`tokio_util::sync::CancellationToken` + `pub async fn shutdown(self)` on the manager) is the architectural template for the new ejection sweeper.

**Differential surface added by phase 14:**

- **Fixture `0022-upstream-outlier-detection-consecutive-5xx`** — bilateral assertion that both proxies, given identical bootstraps configuring an H1 cluster with `outlier_detection: {consecutive_5xx: 3, base_ejection_time: 60s, max_ejection_percent: 100, interval: 1s}` pointing at a single configurable-status backend serving 5xx on `/fail` and `common_lb_config.healthy_panic_threshold: {value: 0}` (panic disabled), drive a sequence of 4+ requests against `/fail` over a single downstream keep-alive conn. **Discriminating differential observable:** the first 3 requests return the backend's 5xx; the 4th (and subsequent) requests return the **synthetic 503 `no healthy upstream`** body (the 12.2 path; ejection is the only signal that drives `pick() -> None` because active HC is unconfigured + the cluster has one endpoint). The bilateral status sequence + the response body bytes + the `cluster.<name>.outlier_detection.ejections_*` counter values are the value-exact bilateral assertions.

**Acceptance signal (a)–(f), per `BOOTSTRAP_PROMPT.md` §7.5:**

- **(a)** Fixture `0022-upstream-outlier-detection-consecutive-5xx` green at Docker-gated CI.
- **(b)** All **21 pre-existing differential fixtures** (`0001-tcp-echo` through `0021-upstream-h2-connection-pooling`) **remain green simultaneously** at the same CI run (regression-equivalence per §7.5 (b)). The existing fixtures configure NO `outlier_detection`; phase-14's machinery must be inert when `outlier_detection` is absent (no behavior change on the 21 existing fixtures) — the 05.1 / 07.1 / 12.1 inert-when-unconfigured pattern.
- **(c)** `h2spec` continues at ≥95% (parent-05 baseline 99.31%). Phase 14 does NOT touch the H2 framing/codec path (the H2 response-receipt hook is at the HCM proxy-arm site, not the codec); the state-4 verification re-confirms the gate held.
- **(d)** `parse_bootstrap` fuzz target clean for the short-budget CI run on the extended corpus (one new seed for the `outlier_detection` bootstrap shape; corpus extends from 21 to 22 seeds).
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check` all clean.
- **(f)** `REVIEW.md` approved.

A **single CI run** must light up gates (a) through (e) **simultaneously** (continues the project precedent — fixture inheritance is a regression vector).

> **NOTE — likely phase split (see §6.1).** Phase 14's surface (a per-endpoint `EndpointEjection` state machine + a response-receipt hook on H1+H2 router arms + a periodic ejection sweeper + envoy-config schema + validator + LB integration + the 4 stat rows + the configurable-backend reuse + fixture 0022 + the in-process backstop + the fuzz seed) is projected at **~1500–2100 LoC**, near or over the `BOOTSTRAP_PROMPT.md` §6.1 ~1500-LoC split gate. **The state-2 PLAN-writer evaluates the split gate against the locked §6.2 findings; SPLIT into `14.1` + `14.2` is the recommended posture** per §6.1 (recommended seam: foundation slice 14.1 = schema + state machine + LB integration + stats; observable-behavior slice 14.2 = response-receipt hooks + ejection sweeper + fixture + parent-14 close). This SPEC covers the whole feature; if the state-2 LoC estimate confirms >~1500, the PLAN-write executes the split (creating `14.1`/`14.2` SPECs + the split ADR), mirroring the parent-12 / parent-13 split cadence.

---

## 2. Behavior-contract scope for phase 14

Phase 14 extends `docs/envoy-rust/BEHAVIOR_CONTRACT.md` with authored additions, landed at the tasks where each is first empirically exercised (per the established 06.x → 13.2 doctrine — contract extensions land at empirical-engagement task time, NOT at PLAN-write time and NOT at state-1 SPEC time).

### 2.1 "Stat-name mapping" extension — outlier-detection stats (projected; §6.2-verified)

New rows under the cluster outlier-detection stat namespace, mirroring upstream Envoy v1.33's documented stat tree. Upstream emits (under `cluster.<name>.outlier_detection.*`): `ejections_active`, `ejections_enforced_total`, `ejections_overflow`, `ejections_consecutive_5xx`, `ejections_consecutive_gateway_failure`, `ejections_success_rate`, `ejections_failure_percentage`, `ejections_detected_*` sibling variants, `ejections_total` (legacy). At phase-14 minimum-viable scope, the projected wired subset (the rest defer with their detection types per §4):

| Stat name | Equivalence (projected; §6.2-verified) | Rationale |
|---|---|---|
| `cluster.<name>.outlier_detection.ejections_active` | value-exact (steady state; counts currently-ejected endpoints) | Gauge; updates inline at each ejection-state transition (one source of truth, NOT polled — mirrors 08.2 `server.live` + 12.1 `membership_healthy` pattern). Under the fixture's post-settle steady state both proxies emit the same converged value. |
| `cluster.<name>.outlier_detection.ejections_enforced_total` | value-exact | Counter; one increment per actual ejection (the consecutive-threshold crossed AND the max-ejection-percent cap honored). Both proxies emit at the ejection-decision site; under deterministic load both increment the same number of times. |
| `cluster.<name>.outlier_detection.ejections_consecutive_5xx` | value-exact | Counter; one increment per ejection triggered by the `consecutive_5xx` detector. Sibling of `ejections_consecutive_gateway_failure`. |
| `cluster.<name>.outlier_detection.ejections_consecutive_gateway_failure` | value-exact | Counter; one increment per ejection triggered by the `consecutive_gateway_failure` detector. |
| `cluster.<name>.outlier_detection.ejections_overflow` | value-exact (0-case; potentially non-zero on max-ejection-percent enforcement) | Counter; incremented when a would-eject endpoint is held un-ejected because `max_ejection_percent` is already at cap. Fixture's `max_ejection_percent: 100` keeps this at 0; future fixtures could exercise overflow deterministically. |

**Namespace empirical-verification signpost:** the `cluster.<name>.outlier_detection.*` shape is the recommended state-1 projection per the 12.1 cluster-stats convention + the 13.x precedent. **The state-2 PLAN-writer empirically verifies the exact stat names + which counters/gauges fire at what site + the per-stat semantics** against `envoyproxy/envoy:v1.33.0` + admin `/stats` scrape before locking (per §6.2). Per-detector-type counters (`ejections_consecutive_5xx` vs `ejections_consecutive_gateway_failure`) are projected as separate counters mirroring upstream's per-type breakdown; §6.2 confirms.

### 2.2 No-healthy-upstream synth-503 path: REUSED unchanged

Phase 14's ejection-driven `pick() -> None` reuses the 12.2-landed no-healthy-upstream synth-503 path (`crates/envoy-http1/src/hcm.rs:582`) verbatim. The BEHAVIOR_CONTRACT.md `Response body — no-healthy-upstream synth-503` row (12.2-landed at `BEHAVIOR_CONTRACT.md:27-36`) covers the wire shape; phase 14 does NOT amend the row. The fixture asserts the 19-byte `no healthy upstream` body bilaterally as the discriminating observable when the ejection forces `pick() -> None`.

### 2.3 H2 ejection-driven synth-503 path

The H2 sibling of the no-healthy-upstream synth-503 path lives in `crates/envoy-http2/src/hcm.rs`. The state-2 PLAN-writer empirically verifies that the existing H2 synth-503 path emits the same 19-byte body under the same circumstances (or extends the H2 path to match if a gap exists). **Recommended state-1 projection:** the H2 path already emits the 19-byte body at parent-12.2's reconciliation (the BEHAVIOR_CONTRACT row is codec-agnostic). If the §6.2 verification surfaces an H2-side gap, an inline ADR records the H2 reconciliation at the state-2 PLAN-write commit (mirrors ADR-0037's H1 reconciliation pattern). The fixture covers the H1 path; an H2 sibling fixture defers unless the §6.2 verification surfaces a wire-observable H2 gap.

### 2.4 No DECISIONS.md amendment required at SPEC time

Phase 14 lands no carryforward whose close shape is a *documentation* amendment. The outlier-detection-driven `pick() -> None` path reuses the 12.2 BEHAVIOR_CONTRACT row verbatim; the PROGRESS narrative attributes the reuse. **No new ADR is required at SPEC time.** Conditional ADRs (the likely split ADR; the §6.2 empirical-verification revision) are enumerated in §7.

---

## 3. Deliverables

Phase 14's scope is enumerated as deliverables `D1`–`D9` below. **The state-2 PLAN-writer organizes deliverables into tasks AND evaluates the §6.1 split gate** (which may fire — see §6.1). These deliverables are LISTED roughly in execution order. If the phase splits, the recommended seam (§6.1) assigns D1/D2/D3/D5/D6 to `14.1` and D4/D7/D8/D9 to `14.2`.

### D1 — `envoy-config` schema extension (`Cluster.outlier_detection`)

At `crates/envoy-config/src/bootstrap.rs`, extend the existing `Cluster` struct (the 12.1 + 13.1 extensions sit at the same site) with an `outlier_detection` field:

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
    pub consecutive_5xx: Option<u32>,                 // Envoy default 5
    #[serde(default)]
    pub consecutive_gateway_failure: Option<u32>,     // Envoy default 5
    #[serde(default)]
    pub interval: Option<String>,                     // parse_duration; Envoy default 10s
    #[serde(default)]
    pub base_ejection_time: Option<String>,           // parse_duration; Envoy default 30s
    #[serde(default)]
    pub max_ejection_percent: Option<u32>,            // Envoy default 10 (range 0..=100)
    // OPTIONAL — all defer per §4 (deny_unknown_fields rejects):
    //   success_rate_minimum_hosts / success_rate_request_volume / success_rate_stdev_factor
    //   failure_percentage_threshold / failure_percentage_minimum_hosts / failure_percentage_request_volume
    //   consecutive_local_origin_failure / split_external_local_origin_errors
    //   enforcing_consecutive_5xx / enforcing_consecutive_gateway_failure / enforcing_success_rate / ...
    //   max_ejection_time / interval_jitter / max_ejection_time_jitter
    //   event_log_path / always_eject / no_traffic_eject
}
```

`parse_duration` (`bootstrap.rs:2289`, accepts `1s`/`250ms`/`500us`) is reused directly. All struct shapes carry `#[serde(deny_unknown_fields)]` per the established envoy-config discipline. The phase-14-deferred fields enumerated in §4 are each rejected by `deny_unknown_fields`.

### D2 — `envoy-config` validator extension

At the cluster-validation site (the `from_bootstrap` / validator path producing `ConfigError`), add a `validate_outlier_detection(cluster) -> Result<(), ConfigError>` sub-validator. The validator checks:

- `consecutive_5xx >= 1` (when present) → `ConfigError::InvalidOutlierDetectionThreshold { cluster, field, value }`.
- `consecutive_gateway_failure >= 1` (when present) → same error.
- `interval` parses via `parse_duration` and is `> 0` → `ConfigError::InvalidOutlierDetectionTiming { cluster, field }` (reuses `parse_duration` error surface).
- `base_ejection_time` parses via `parse_duration` and is `> 0` → same error.
- `max_ejection_percent` (when present) is in `0..=100` → `ConfigError::InvalidMaxEjectionPercent { cluster, value }`.
- At least one of `consecutive_5xx` / `consecutive_gateway_failure` is present (otherwise the block is no-op) → recommended posture (state-2-verified): allow no-op blocks (matches Envoy parse-time tolerance) — verify against `envoyproxy/envoy:v1.33.0`'s acceptance.

Roughly **4–6 new `ConfigError` variants** land at this site. Each carries `cluster: String` per the established envoy-config error-context discipline. Each has positive + negative parse-path unit tests. The validator is exercised by the existing `parse_bootstrap` fuzz target (the new fixture's bootstrap seeds the corpus per D9.2).

### D3 — Per-endpoint ejection state machine (`envoy-cluster`)

In `crates/envoy-cluster/`, add a per-endpoint `EndpointEjection` type carrying the runtime ejection state, owned by the `Cluster` alongside the 12.1 `EndpointHealth`:

```rust
/// Per-endpoint outlier-detection ejection state. Shared (Arc) so the
/// response-receipt hook (D4) + the ejection sweeper (D7) can mutate it
/// while pick() (D5) reads it.
pub struct EndpointEjection {
    /// Monotonic ns since process start of the ejection instant; 0 = not ejected.
    ejected_at_ns: AtomicU64,
    /// Consecutive 5xx response count (any 5xx status — 500-599).
    consecutive_5xx: AtomicU32,
    /// Consecutive gateway-failure count (502/503/504 subset).
    consecutive_gateway_failure: AtomicU32,
    /// Read-only thresholds (shared via Arc<OutlierDetectionConfig> at cluster construct).
    thresholds: Arc<OutlierDetectionConfig>,
}

impl EndpointEjection {
    /// Record a response status. Increments / resets counters; transitions to
    /// ejected when a threshold crosses (delegated to the cluster-level
    /// max_ejection_percent gate).
    pub fn record_response(&self, status: u16) -> EjectionDecision;

    pub fn is_ejected(&self) -> bool;

    /// Called by the sweeper at past-deadline; returns true if un-eject fired.
    pub fn try_unEject(&self, now_ns: u64) -> bool;
}

pub enum EjectionDecision {
    NoChange,
    Eject { detector: Detector },
}

pub enum Detector {
    Consecutive5xx,
    ConsecutiveGatewayFailure,
}
```

The state machine lives in `envoy-cluster` (NOT a new crate) next to the 12.1 `EndpointHealth` — `pick()` reads both with no cross-crate dependency. The response-receipt hook (D4) and the ejection sweeper (D7) mutate it from their respective sites. Per-cluster `OutlierDetectionConfig` (thresholds + interval + base_ejection_time + max_ejection_percent) is shared via `Arc` across all per-endpoint `EndpointEjection` instances + the sweeper.

**`max_ejection_percent` enforcement (§6.2-verified):** the ejection decision is **per-cluster-gated** — when an endpoint crosses a threshold, the cluster checks whether ejecting it would push the cluster past `max_ejection_percent` of endpoints currently ejected; if so, the ejection is held (overflow counter increments). The check happens at `record_response`'s ejection-decision site, NOT at sweep time. **The state-2 PLAN-writer empirically verifies the exact comparison semantics** (strictly-below vs at-or-below; rounding direction for fractional caps; per-cluster vs per-locality).

### D4 — Response-receipt hooks (H1 + H2 router-proxy arms)

Modify the H1 router-proxy-arm response-receipt site at `crates/envoy-http1/src/router.rs::write_proxied_response` (and the H2 sibling at `crates/envoy-http2/src/hcm.rs` post-dispatch site) to call `cluster.record_response(endpoint, status)` after the existing `upstream_rq_total` + `upstream_rq_5xx` increments fire. The hook fires AFTER counter increments + BEFORE the response is written downstream — the ordering is load-bearing (the counter increments are unconditional; the ejection-decision side-effect must not race with the response write but must complete before downstream observers see the response).

The `Cluster::record_response` method dispatches: looks up the endpoint's `EndpointEjection`, calls `EndpointEjection::record_response(status)`, applies the `max_ejection_percent` gate, transitions the endpoint to ejected if appropriate, and emits the per-detector + enforced counter increment. **Synth-status bypass**: envoy-rust-side synth-502 (connect-fail) + synth-503 (no-healthy-upstream) paths do NOT call `record_response` — these are not upstream responses and must not feed the ejection counters (matches the existing 06.3 `upstream_rq_total` synth-bypass semantic).

**Important crate-boundary note:** `envoy-cluster` already exposes `Cluster::record_response`-style mutation methods (the 13.x `H1PoolManager`/`H2PoolManager` accessors live as external sibling registries, not on `Cluster` directly — same pattern). The hook adds one new method on `Cluster` (`record_response(endpoint, status)`) — does NOT require a new crate or cycle resolution.

### D5 — Load-balancer integration (exclude ejected + sweeper interaction)

Modify `crates/envoy-cluster/src/cluster.rs::Cluster::pick()` to consult `EndpointEjection` alongside the existing 12.1 `EndpointHealth`:

- Build the candidate set as endpoints where **BOTH** `EndpointHealth::is_healthy()` AND `!EndpointEjection::is_ejected()` hold.
- The 12.1 panic threshold continues to apply: when the **non-panic** filtered set (healthy AND not-ejected) is empty AND panic-routing is enabled (`healthy_fraction < panic_threshold`), `pick()` returns over the unfiltered set (mirrors 12.1's panic semantics — when everything is failing, prefer attempting over hard-failing).
- When the filtered set is empty AND panic is disabled (or panic doesn't fire), `pick()` returns `None` → the existing no-healthy-upstream synth-503 path fires unchanged.

Clusters with NO `outlier_detection` configured: all endpoints are implicitly never-ejected (the `EndpointEjection` is initialized with thresholds disabled OR is absent from the cluster's per-endpoint metadata); `pick()` behaves exactly as today — **the existing 21 fixtures see no behavior change** (acceptance gate (b)).

### D6 — Outlier-detection stats wiring + BEHAVIOR_CONTRACT extension

At cluster construction (when `outlier_detection` is configured), register the outlier-detection counters + the active-ejections gauge against the `Arc<StatsRegistry>` (the 06.x convention; `register_counter`/`register_gauge` idempotent for same-name re-registration):

- `cluster.<name>.outlier_detection.ejections_consecutive_5xx` (counter; incremented at `record_response`'s ejection-decision site when the 5xx detector fires).
- `cluster.<name>.outlier_detection.ejections_consecutive_gateway_failure` (counter; incremented when the gateway-failure detector fires).
- `cluster.<name>.outlier_detection.ejections_enforced_total` (counter; incremented at each actual ejection — sum of per-detector counters, modulo overflow).
- `cluster.<name>.outlier_detection.ejections_overflow` (counter; incremented when a would-eject is held by `max_ejection_percent`).
- `cluster.<name>.outlier_detection.ejections_active` (gauge; updated inline at each ejection / un-ejection — one source of truth, NOT polled).

**D6.1 — `Stat-name mapping` rows** (§2.1) land at the task where each stat is first registered + exercised (per the 06.x → 13.x cadence).

### D7 — Ejection sweeper (the fourth periodic-background primitive)

A per-cluster background `tokio::time::interval`-driven task spawned at cluster construct time when `outlier_detection` is configured. Mirrors the 12.2 `envoy-health::Scheduler` topology + the 13.1/13.2 H1+H2 pool idle sweepers verbatim:

```rust
// crates/envoy-cluster/src/outlier.rs (new module)

pub struct OutlierEjectionSweeper {
    cluster_name: String,
    endpoints: Arc<Vec<Arc<EndpointEjection>>>,
    config: Arc<OutlierDetectionConfig>,
    cancel: CancellationToken,    // tokio_util::sync::CancellationToken
    join: tokio::task::JoinHandle<()>,
}

impl OutlierEjectionSweeper {
    pub fn spawn(cluster_name: String,
                 endpoints: Arc<Vec<Arc<EndpointEjection>>>,
                 config: Arc<OutlierDetectionConfig>) -> Self {
        let cancel = CancellationToken::new();
        let cancel_inner = cancel.clone();
        let join = tokio::spawn(async move {
            let mut tick = tokio::time::interval(config.interval);
            loop {
                tokio::select! {
                    _ = cancel_inner.cancelled() => return,
                    _ = tick.tick() => {
                        let now_ns = monotonic_ns();
                        for ep in endpoints.iter() {
                            if ep.is_ejected() {
                                // ejected_at + base_ejection_time >= now → un-eject
                                if ep.is_past_deadline(now_ns) {
                                    ep.try_unEject(now_ns);
                                }
                            }
                        }
                    }
                }
            }
        });
        Self { /* ... */ }
    }

    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.join.await;
    }
}

pub struct OutlierManager {
    // Per-cluster sweepers; mirrors H1PoolManager / H2PoolManager pattern.
    sweepers: HashMap<String, OutlierEjectionSweeper>,
}
```

The sweeper is the **fourth periodic-background primitive**. The cancellation discipline + shutdown semantics MIRROR 12.2 + 13.1 + 13.2 verbatim per the standing post-MVP-trunk discipline (one source of truth for the pattern; phase-14 reuses the conventions verbatim).

**Crate-placement decision:** the sweeper lives inside `envoy-cluster` as a new module `outlier.rs` (~200-300 LoC). `envoy-cluster` already uses tokio (STRICT_DNS via `tokio::net::lookup_host`); adding a background task is consistent with `envoy-http1::pool` (13.1) + `envoy-http2::pool` (13.2) living inside their own crates. **No new crate; no cycle.** The cluster manager (`envoy-cluster::ClusterManager`) gains an `OutlierManager` field; `envoy-bin` wires it at startup (mirrors `H1PoolManager`/`H2PoolManager`/`envoy-health::Scheduler` external-sibling-registry wiring). Recommended posture: NO new ADR for the crate placement (ordinary structure — mirrors the established 12.2 + 13.x patterns).

### D8 — Fixture + Docker wrapper + in-process backstop

- **D8.1 — Fixture `tests/fixtures/0022-upstream-outlier-detection-consecutive-5xx/`** — configures: a cluster with **one endpoint** pointing at the 13.x configurable-status synthetic backend serving 5xx on `/fail` and 200 on `/`; `outlier_detection: {consecutive_5xx: 3, base_ejection_time: 60s, max_ejection_percent: 100, interval: 1s}`; `common_lb_config.healthy_panic_threshold: {value: 0}` (panic disabled); an H1 HCM listener routing `/` and `/fail` to the cluster. The fixture's pre_requests sequence drives ≥4 sequential GET `/fail` over a single downstream keep-alive conn (reusing 13.1's `Driver::Http1KeepAlive`). After ejection: requests 1-3 return the backend's 5xx; requests 4+ return the synthetic 503 + 19-byte `no healthy upstream` body. The bilateral expectations.yaml asserts per-request status + body + the post-settle `cluster.<name>.outlier_detection.ejections_*` counter values + the `ejections_active` gauge.

  **Driver choice:** reuses 13.1's `Driver::Http1KeepAlive` verbatim — no new harness primitive required (the fixture's discriminating observable is a per-request status sequence, not a multi-protocol pivot).

  Docker-gated wrapper at `tests/differential/tests/upstream_outlier_detection.rs` mirroring the 12.2 / 13.x wrapper shape.

- **D8.2 — Fuzz corpus seed.** New file `crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_outlier_detection.yaml` containing the `outlier_detection` bootstrap shape. Mirrors the 12.2 / 13.x corpus-seed precedent. Extends the seed coverage 21 → 22, with the `crates/envoy-config/fuzz/.gitignore` allow-list extension AND the `bootstrap.rs::tests::fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS-array extension (both files edited together, per the 09-13.2 corpus-seed convention).

- **D8.3 — In-process backstop.** New file `crates/envoy-bin/tests/upstream_outlier_detection.rs` mirroring the 12.2 / 13.x backstop shape — with the standing `tokio::process::Command` + `.kill_on_drop(true)` + `Stdio::null()`/`piped()` discipline. Boots `envoy-bin` with a synthesized bootstrap + an in-process configurable-status backend; exercises the eviction sequence in-process (status flips 5xx-5xx-5xx-synth503) + the post-`base_ejection_time` un-eject convergence direction. Includes the 5-standard-header presence assertion on the synth-503 response (the 10/11/12.2 lesson + the 13.x in-process-backstop discipline carry forward).

### D9 — Documentation deliverables (rolled in with prior tasks)

- **D9.1 — BEHAVIOR_CONTRACT.md extension** (§2.1) — landed at D6's stats-wiring task.
- **D9.2 — PROGRESS.md attribution** — outlier-detection-driven `pick() -> None` reuses the 12.2 BEHAVIOR_CONTRACT row verbatim; PROGRESS narrates the reuse honestly (D-3.4).

---

## 4. Out of scope (deferred non-goals)

Phase 14 explicitly does NOT land:

- **Success-rate-based ejection** (`success_rate_minimum_hosts`, `success_rate_request_volume`, `success_rate_stdev_factor`). Requires a sliding-window aggregate per endpoint; distinct subsystem. Defers to a follow-up phase. Rejected by `deny_unknown_fields`.
- **Failure-percentage-based ejection** (`failure_percentage_threshold`, `failure_percentage_minimum_hosts`, `failure_percentage_request_volume`). Same family as success-rate-based. Defers.
- **`consecutive_local_origin_failure`** (connect-level / transport failures, not 5xx responses). Requires new error-class accounting plus a new code-path through the H1 + H2 router-arm connect-fail sites. Defers.
- **`split_external_local_origin_errors`** — toggles the local-origin classification. Defers with `consecutive_local_origin_failure`.
- **`enforcing_consecutive_5xx` / `enforcing_consecutive_gateway_failure` / `enforcing_*` knobs.** Phase-14 assumes 100% enforcement (every threshold crossing actually ejects, modulo `max_ejection_percent`). The runtime-fractional-percent enforcement defers; `enforcing_*` fields rejected by `deny_unknown_fields`.
- **`max_ejection_time` / `max_ejection_time_jitter` / `interval_jitter`** — ejection-time refinements (cap on ejection duration; jitter on the sweeper interval). Phase-14 uses a fixed `base_ejection_time`. Defers.
- **Ejection event logging** (`event_log_path`, `always_eject`, `no_traffic_eject`). Observability surface; defers.
- **TCP / gRPC / custom outlier detection.** Phase-14 supports HTTP-only outlier detection. TCP-side outlier detection (connect failures on TCP-proxy clusters) defers with the TCP pool follow-up.
- **`success_rate_balance_minimum_hosts` / `enforcement_minimum_health_percent`** — locality-aware variants. Defers.
- **Outlier detection over H2 / H3 upstream clusters.** Phase-14's fixture exercises H1; the H2 router-arm hook fires symmetrically (D4) but no dedicated H2 fixture lands at phase-14 scope (the 13.x H2-pool fixture infrastructure is reused if the §6.2 verification surfaces an H2-specific divergence). Defers a dedicated H2 outlier-detection fixture.

---

## 5. Architectural invariants

Phase 14 honors and extends the established cross-crate invariants:

### 5.1 Crate boundaries + the dependency-cycle constraint (load-bearing)

- **`EndpointEjection` state lives in `envoy-cluster`** so `Cluster::pick()` reads it with no cross-crate dependency. Mirrors the 12.1 `EndpointHealth` precedent verbatim.
- **The response-receipt hook fires from `envoy-http1` and `envoy-http2`** (the router-proxy-arm response-receipt sites) — these crates already depend on `envoy-cluster` (the router holds `Arc<Cluster>`), so the new `Cluster::record_response` method is reachable without cycle inversion.
- **The ejection sweeper lives inside `envoy-cluster`** as a new module `outlier.rs` (mirrors the 13.1 H1 pool sweeper living inside `envoy-http1::pool`). The `OutlierManager` is an external sibling registry to `ClusterManager` (mirrors `H1PoolManager`/`H2PoolManager`/`envoy-health::Scheduler` — the cycle-resolution pattern stable since 12.2).
- **No new top-level Cargo dep; no new workspace member; no cycle.** The state-3 implementer verifies the dependency graph stays acyclic.

### 5.2 Hand-rolled per D-3.2

Outlier detection is hand-rolled per **D-3.2**'s scratch-mandate (the upstream-robustness family is on the must-be-written-from-scratch list — *"Active health checking, outlier detection, circuit breakers ... Must be written from scratch"*). The implementation uses only **std-lib + tokio + tokio-util + bytes + envoy-cluster + envoy-config + envoy-stats + envoy-http1/envoy-http2 internal types** — all D-3.2-permitted; all already pulled. **No new top-level Cargo dep.**

**Explicit non-grants:** no `tower`-style passive-health-check library. No `circuit-breaker` crate. No `rand` (no jitter at phase-14 scope per §4). None on D-3.2's permitted-foundations list beyond what is already pulled.

### 5.3 No new top-level Cargo deps

The recommended no-foundations-grants posture carries forward. The new `outlier.rs` module is an internal addition to `envoy-cluster`. **If the state-3 implementer surfaces a genuine external-crate need, a foundations-grant ADR lands per D-3.5 — see §7.**

### 5.4 Outlier detection is inert when unconfigured (regression-equivalence)

When `Cluster.outlier_detection` is absent, NO sweeper spawns, all `EndpointEjection` instances are never-ejected (or absent from the cluster's per-endpoint state), the response-receipt hook short-circuits with a no-op, `Cluster::pick()` behaves exactly as 12.1, and the 21 existing fixtures see ZERO behavior change. This is the regression-equivalence invariant (acceptance gate (b)) and the load-bearing safety property of the phase — the 05.1 / 07.1 / 12.1 / 13.1-via-default-config pattern. **The state-3 implementer verifies the inert path is the hot path** (no per-response branch cost on unconfigured-OD clusters beyond a cheap `Option::is_some()` check).

### 5.5 The ejection sweeper is the fourth periodic-background primitive

Phase 14 introduces the project's **fourth periodic timer-driven background task** (after 12.2's `envoy-health::Scheduler` + 13.1's H1 pool idle sweeper + 13.2's H2 pool idle sweeper). All four share identical `tokio_util::sync::CancellationToken` cancellation discipline + `pub async fn shutdown(self)` on the manager. The state-3 implementer ensures graceful cancellation (no leaked tasks on cluster destroy; tests assert task shutdown).

### 5.6 The 12.1 `pick()` exclusion seam is reused

Phase 14 plugs into the same seam phase 12 deliberately reserved. The 12.1 `Cluster::pick()` already filters by `EndpointHealth::is_healthy()`; phase-14 adds a sibling filter on `!EndpointEjection::is_ejected()`. Both signals AND together. The 12.2 no-healthy-upstream synth-503 path fires unchanged when ejection drives `pick() -> None`. **This is the primary reason the architectural risk is LOW-MEDIUM despite the new subsystem** — the dispatch + synth-503 seams are already wired and BEHAVIOR_CONTRACT-locked.

### 5.7 Synth-status bypass on ejection counters (mirrors 06.3 / 12.2 / 13.x convention)

The response-receipt hook fires ONLY on real upstream responses, NOT on envoy-rust-side synth-502/503 paths. This mirrors the 06.3 BEHAVIOR_CONTRACT note ("`upstream_rq_total` + `upstream_rq_5xx` bypass synth-paths") and is load-bearing: if synth-503 fed the ejection counters, the no-healthy-upstream path would feed itself (a feedback loop). The state-3 implementer enforces the bypass at the router-arm dispatch site.

### 5.8 Composition with active HC (orthogonal signals)

Active HC ejection (12.x) and outlier-detection ejection (14) are independent signals — both must pass for an endpoint to be picked. A cluster can configure both (`health_checks` + `outlier_detection`); they compose orthogonally. The state-2 PLAN-writer empirically verifies the composition (`envoyproxy/envoy:v1.33.0` with both configured) does not surface unexpected interaction — recommended state-1 projection: independent signals AND together cleanly; verify at §6.2.

---

## 6. Implementation signposts for the planner

The state-2 PLAN-writer reads this section to drive PLAN structure.

### 6.1 Split-gate evaluation

Per `BOOTSTRAP_PROMPT.md` §6.1, the state-2 PLAN-write evaluates whether the PLAN exceeds ~25 numbered tasks OR ~1500 LoC. Phase 14's surface estimate at SPEC time:

- D1 — envoy-config schema (`OutlierDetection`) (~80 LoC + ~120 LoC tests).
- D2 — envoy-config validator (4–6 ConfigError variants) (~80 LoC + ~140 LoC tests).
- D3 — `EndpointEjection` state machine + `OutlierDetectionConfig` (~180 LoC + ~200 LoC tests).
- D4 — response-receipt hooks (H1 + H2 router-arm modifications) (~90 LoC modify + ~150 LoC tests).
- D5 — `Cluster::pick()` ejection-filter integration (~50 LoC modify + ~110 LoC tests).
- D6 — stats wiring (4 counters + 1 gauge) + BEHAVIOR_CONTRACT rows (~80 LoC + ~80 LoC tests).
- D7 — ejection sweeper (`outlier.rs` module + `OutlierManager` + envoy-bin wiring) (~220 LoC + ~200 LoC tests).
- D8.1 — fixture 0022 + Docker wrapper (~100 LoC YAML/wrapper).
- D8.2 — fuzz seed (~30 LoC + 2 file edits).
- D8.3 — in-process backstop (~240 LoC).
- State-4 verification + STATE-advance (~docs).

**SPEC-time projection: ~12–16 tasks; ~1500–2100 LoC** (production ~700, tests ~880, fixture/backstop ~370). **The LoC estimate sits near or over the §6.1 ~1500-LoC gate** (task count is under ~25). **Recommended posture: state-2 PLAN-writer evaluates the gate against the locked §6.2 findings; SPLIT into `14.1` + `14.2`** is the recommended posture if the estimate confirms >~1500 (the standing 12/13 split cadence):

- **`14.1` — schema + state machine + LB integration + stats foundation slice (no new fixture).** D1 (schema) + D2 (validator) + D3 (`EndpointEjection` state machine + `OutlierDetectionConfig`) + D5 (LB integration: `pick()` ejection-filter) + D6 (stats wiring). Inert when `outlier_detection` is unconfigured → 21 existing fixtures stay green (the 05.1 / 07.1 / 12.1 foundation-slice pattern). **No new fixture; no response-receipt hooks yet; no sweeper yet.** ~700–900 LoC.
- **`14.2` — response-receipt hooks + ejection sweeper + fixture 0022 + parent-14 close.** D4 (response-receipt hooks on H1+H2) + D7 (ejection sweeper) + D8.1 (fixture 0022) + D8.2 (fuzz seed) + D8.3 (in-process backstop) + parent-14 close. ~900–1200 LoC. This is where observable behavior + the differential fixture land.

**Split ADR:** if the split fires, an ADR (`ADR-0040`) lands at the state-2 PLAN-write commit per `BOOTSTRAP_PROMPT.md` §6.2 step 6, mirroring ADR-0036 / ADR-0038 (parent-12 / parent-13 splits). The parent-14 ROADMAP row flips `planned → in-progress` with `sub-phases: 14.1, 14.2`; each sub-phase gets its own row (`planned`) + its own `SPEC.md`.

**If the state-2 LoC estimate comes in at-or-under ~1500** (e.g. the PLAN-writer finds a leaner harness path), single-phase is permitted. **Standing posture: split.**

### 6.2 Empirical verification at state-2 PLAN-write

Per the phase-10/11/12/13-ratified verify-at-PLAN-write process improvement: **the state-2 PLAN-writer empirically verifies the upstream wire/behavior shapes BEFORE locking PLAN lock-ins.** Phase 14's §6.2 surface (against `envoyproxy/envoy:v1.33.0` Docker + the 13.x configurable-status backend + admin `/stats`):

1. **Exact `outlier_detection` config shape** — field hierarchy, default values for each field (`consecutive_5xx` default 5?, `consecutive_gateway_failure` default 5?, `interval` default 10s?, `base_ejection_time` default 30s?, `max_ejection_percent` default 10? range), and any deny_unknown_fields-relevant phase-14-deferred field names (the `enforcing_*` knobs in particular).
2. **`outlier_detection` stat namespace** — exact stat names, which counters exist (the §2.1 projection vs Envoy's actual emit), per-detector-type counter breakdown, the `ejections_active` gauge name, semantics on the `ejections_overflow` counter.
3. **Initial endpoint ejection state** — when an `outlier_detection`-configured cluster starts, are endpoints initially never-ejected (the projection) OR is there a warmup window? The state-2 PLAN-writer captures the initial state empirically.
4. **`max_ejection_percent` enforcement semantics** — strictly-below vs at-or-below the cap; rounding for fractional caps; per-cluster vs per-locality scope; what happens when the cap is hit (overflow counts; is the would-eject endpoint held un-ejected indefinitely or re-checked).
5. **Ejection-time semantics** — un-eject at exactly `ejected_at + base_ejection_time`, or with a stagger? Does the per-endpoint counter reset on un-eject? On subsequent ejection?
6. **Discriminating observable for fixture 0022** — drive the 4-request workload against `envoyproxy/envoy:v1.33.0`; capture the per-request status sequence + the per-counter values + the body bytes (verify the 19-byte `no healthy upstream` body fires on the post-ejection requests — re-confirm ADR-0037 bilaterally).
7. **Composition with active HC** — boot Envoy with both `health_checks` AND `outlier_detection` configured; verify the orthogonal-AND composition holds.
8. **H1 vs H2 sibling consistency** — verify that an H2 cluster with `outlier_detection` configured emits the same stat namespace + behaves identically (the projection holds; if a divergence surfaces, document it).
9. **Synth-status bypass** — confirm Envoy's synth-503 paths do NOT feed the ejection counters (the projection per §5.7).

Each finding lands as a PLAN lock-in. **If any finding differs materially from the SPEC projection, the lock-in records the divergence + the SPEC §X.Y revision via an inline ADR at the state-2 PLAN-write commit** (mirrors ADR-0034 / ADR-0037 precedents). Given the split is also projected (§6.1), the empirical-revision ADR (if it fires) would be `ADR-0041` (the split ADR takes `ADR-0040`); at most one ADR per commit per D-3.5 — if both fire at the PLAN-write commit, they take consecutive numbers across the lock-in narrative.

### 6.3 The 12.x + 13.x seam reuse (NOT a new carryforward closure)

Phase 14 reuses 12.2's no-healthy-upstream synth-503 path (`BEHAVIOR_CONTRACT.md:27-36`) + 12.1's `EndpointHealth` / `pick()` seam + 13.x's configurable-status synthetic backend + 13.1's `Driver::Http1KeepAlive` driver. **No carryforward closes at phase 14** — the 06.3 REVIEW I2 closed at parent-13. PROGRESS attributes the seam reuses explicitly (the reuse is positive — it minimizes new harness/code surface).

### 6.4 In-process backstop assertions (heeds the 10/11/12.2/13.x lesson)

D8.3 SHOULD exercise BOTH the ejection direction (post-3-5xx → synth-503 fires) AND the un-eject direction (post-`base_ejection_time` → endpoint re-picks; backend now serving 200 → 200 returns). Include the per-probe 5-standard-header presence assertion on the synth-503 response. Recommended: include all three checkpoints (eject + un-eject + header presence).

### 6.5 The 06.x stats convention

StatsRegistry registration at cluster-construct time when `outlier_detection` is configured; per-cluster ownership of the Counter/Gauge handles; the `ejections_active` gauge updated inline at each ejection/un-ejection state transition (one source of truth, NOT polled — the 08.2 `server.live` / 12.1 `membership_healthy` pattern). Namespace `cluster.<name>.outlier_detection.*` (§6.2-verified).

### 6.6 The BEHAVIOR_CONTRACT extension cadence

Contract extensions land at the TASK where each is first empirically exercised, NOT at PLAN-write and NOT at SPEC time. Stat rows at D6; the no-healthy-body row is REUSED unchanged (no new row required).

### 6.7 Pre-state-4 fmt discipline (continues per 06.1 R-9)

Per-task PROGRESS sections quote `cargo fmt --all -- --check` at every PROGRESS-task close, NOT just at state-4.

### 6.8 State-4 evidence-discipline (continues per 05.3 → … → 13.2 chain)

Per-gate quoted evidence in PROGRESS at the state-4 verification task: real CI run URL + HEAD SHA + completion timestamp + per-gate quoted output (5 stable-toolchain gates + each Docker-gated fixture + h2spec_pass_rate_gate + parse_bootstrap fuzz iteration count). Phase 14 does NOT touch the H2 framing/codec — the state-4 verification confirms h2spec ≥95% held (no regression).

### 6.9 Cargo.lock cadence

The phase-04.1 REVIEW M5/M9 Cargo.lock-cadence ADR carries forward. Phase 14 adds zero new top-level Cargo deps; the new `outlier.rs` module is internal to `envoy-cluster` (no new workspace member; no external-advisory surface change).

### 6.10 PLAN.md + PROGRESS.md skeleton + Task 1 preamble land alongside at state-2

Per the 06.2 → 13.2 cadence. State-2 PLAN-write lands `PLAN.md` (or, on split, the `14.1` PLAN) + `PROGRESS.md` skeleton + Task 1 preamble in a single standalone pre-Task-1 commit.

### 6.11 Subagent-driven execution at state 3 (per `feedback_execution_style`)

The user's standing preference auto-memory `feedback_execution_style` ("default to subagent-driven-development; skip the two-option fork") applies at state 3. The state-2 PLAN-write organizes tasks for subagent-driven execution per the 06.x → 13.2 cadence.

### 6.12 Known-deferred small follow-up from 13.x (opportunistic close candidate)

The 13.1-surfaced **cluster per-class `upstream_rq_{2,3,4}xx` counter family extension** (envoy-rust's `Cluster` carries only `upstream_rq_total + upstream_rq_5xx` Arc<Counter> fields today at `crates/envoy-cluster/src/cluster.rs:60-76`; the 4 per-class siblings deferred at 13.x). Phase-14's fixture exercises 5xx responses heavily and could opportunistically light up the per-class extension IF the state-2 PLAN-writer judges the extension scope is small enough to fold in (~30-50 LoC + 1 BEHAVIOR_CONTRACT row). **Recommended posture: defer unless cheap** — the extension is observability work, not outlier-detection work; folding it in inflates 14's scope. If deferred, carries forward unchanged; named owner: a future observability-family phase.

---

## 7. ADR projection

**Recommended posture at state-1: NO new ADRs land at THIS (state-1 brainstorm) commit.** The DECISIONS.md ledger head stays at **ADR-0039** through phase 14's state-1; the next-available number is **ADR-0040**.

Conditional ADR slots, reserved for state-2 / state-3 landing:

- **Conditional ADR-0040 (option A — phase split).** Per §6.1, the LoC projection (~1500–2100) is near or over the §6.1 gate; the state-2 PLAN-write may split phase 14 into `14.1` + `14.2`, landing `ADR-0040: split phase 14 into 14.1–14.2 because plan exceeded ~1500 LoC` per `BOOTSTRAP_PROMPT.md` §6.2 step 6. **Recommended posture: SPLIT** (the standing 12/13 cadence; the foundation-slice / observable-behavior seam is the recommended shape).

- **Conditional ADR-0041 (option B — §6.2 empirical-verification revision).** If any of the 9 §6.2 items (esp. the stat namespace exact names + the `max_ejection_percent` enforcement semantics + the initial ejection state) differs materially from this SPEC's projection, an inline ADR lands at the state-2 PLAN-write commit recording the divergence + the SPEC revision. Numbered `ADR-0041` if the split ADR took `ADR-0040`. **Recommended posture: verify all 9; land the revision ADR if any differ.**

- **Conditional ADR (option C — outlier sweeper architecture / cycle resolution).** Only if the PLAN-writer judges the new-module / external-`OutlierManager`-registry choice (§5.1) warrants append-only recording. **Recommended posture: NO ADR** — the external-sibling-registry pattern is established (mirrors 12.2 `envoy-health::Scheduler` + 13.1 `H1PoolManager` + 13.2 `H2PoolManager` verbatim); record the choice as a PLAN lock-in, not an ADR.

- **Conditional ADR (option D — foundations grant).** No external-crate grant projected (§5.2/§5.3). If state-3 surfaces a genuine need, an ADR lands at the surfacing task. **Recommended: no grant.**

- **Conditional ADR (option E — H2 no-healthy-upstream body reconciliation).** Only if the §6.2 item-8 verification surfaces an H2-side gap on the 12.2-reconciled 19-byte body (per §2.3). **Recommended posture: NO ADR projected** — the projection holds the H2 path emits the 19-byte body. Verify at §6.2.

At most ONE ADR lands per commit (per D-3.5 sequential numbering). If none fire, the ledger stays at ADR-0039 through phase 14 (unlikely, given the split projection).

---

## 8. State-machine signposts for the phase-14 state-2 session

The next session (state 2) reads this section and acts.

- **Lifecycle state at session start:** State 2 (SPEC.md exists; PLAN.md does not).
- **Skill:** `superpowers:writing-plans` per `BOOTSTRAP_PROMPT.md` §5 state 2.
- **Output:** `PLAN.md` + `PROGRESS.md` skeleton + Task 1 preamble (standalone pre-Task-1 commit per the 04.3 → 13.2 cadence). **If the split fires (§6.1, recommended): create `docs/envoy-rust/phases/14.1-<subtitle>/SPEC.md` + `14.2-<subtitle>/SPEC.md`, update ROADMAP (parent 14 → `in-progress` + `sub-phases: 14.1, 14.2`; two new sub-phase rows `planned`), update STATE → `14.1`, land `ADR-0040` (split), and STOP** — the next session starts `14.1` at state 1/2 per §6.2 step 7.
- **Empirical verification at state 2 (per §6.2 — HEAVY 9 items):** verify all 9 items against `envoyproxy/envoy:v1.33.0` before locking. Land any empirical-revision ADR inline.
- **Split-gate evaluation:** §6.1 above. **Recommended: SPLIT into 14.1 + 14.2.**
- **The 12.x + 13.x seam reuse (§6.3):** no carryforward closes; PROGRESS attributes the seam reuses honestly.
- **Crate-placement + cycle decision (§5.1 / D7):** recommended `outlier.rs` module inside `envoy-cluster` + external `OutlierManager` sibling registry to `ClusterManager` (mirrors 12.2/13.x patterns verbatim); record as a PLAN lock-in.
- **PLAN-time SPEC corrections:** the PLAN-writer reads this SPEC against HEAD `<state-1-commit-SHA>` and flags any drift (the exact `Cluster` struct fields + line, the exact `pick()` signature in `cluster.rs`, the exact 12.1 `EndpointHealth` shape, the exact H1 / H2 router-arm response-receipt sites, the exact `parse_duration` signature, the existing 13.x `health-aware-http1-backend` `--per-path` flag at `tests/helpers/`, the existing `Driver::Http1KeepAlive` shape in `tests/differential/src/lib.rs`). Per the 06.2 → 13.2 "N PLAN-write SPEC corrections" pattern, corrections land in the PROGRESS Task 1 preamble.

---

## 9. Commit message format (for state 6 of the phase-14 lifecycle)

If phase 14 stays single (NOT recommended; see §6.1):

```
phase 14: outlier detection (consecutive_5xx + consecutive_gateway_failure) + fixture 0022 [ADR-0040, ...]

<1-3 sentence summary>

Differential surface: fixture 0022-upstream-outlier-detection-consecutive-5xx; all 22 Docker-gated fixtures (0001-0022) green simultaneously at CI run <ID> HEAD <SHA>.
Conformance: h2spec ≥95% gate held at parent-05 baseline (H2 framing path untouched).
```

If phase 14 splits (recommended), the closing sub-phase (`14.2`) commit carries `[parent 14 done]` per the 07.2 / 08.2 / 12.2 / 13.2 closing-sub-phase precedents, and each sub-phase commits per the §5.3 format with its own ADR bracket as applicable.

---

## 10. State-machine commit (this commit — phase-14 state-1 close-out)

This SPEC is the state-1 output. The state-1 close-out commit is **docs-only** and touches:

- **CREATE** `docs/envoy-rust/phases/14-outlier-detection/SPEC.md` (this file).
- **MODIFY** `docs/envoy-rust/ROADMAP.md` — **adds a new row to the existing "Upstream robustness family" §9 table**. Row content:
  ```
  | 14 | outlier detection (consecutive_5xx + consecutive_gateway_failure) + ejection sweeper + fixture 0022 | 04 06 12 13 | planned | — | fixture 0022-upstream-outlier-detection-consecutive-5xx green; envoy-cluster gains per-endpoint EndpointEjection state machine + pick() ejection-exclusion + outlier.rs module + OutlierEjectionSweeper (fourth periodic-background primitive) + OutlierManager external sibling registry; envoy-http1 + envoy-http2 router-arms gain Cluster::record_response hook at the response-receipt site (synth-status bypass per §5.7); envoy-config gains OutlierDetection schema + ~4-6 ConfigError variants (success-rate-based + failure-percentage-based + consecutive_local_origin_failure + enforcing_* knobs rejected per minimum-viable scope); cluster.<name>.outlier_detection.{ejections_consecutive_5xx, ejections_consecutive_gateway_failure, ejections_enforced_total, ejections_overflow, ejections_active} stats; reuses 12.x EndpointHealth pick() seam + 12.2 no-healthy-upstream synth-503 path + 13.x configurable-status synthetic backend + 13.1 Driver::Http1KeepAlive; LIKELY SPLIT into 14.1 + 14.2 at state-2 PLAN-write per ~1500-2100 LoC estimate |
  ```
  The "Upstream robustness family" heading + its descriptive line stay unchanged; the new row joins the existing table beneath them per `BOOTSTRAP_PROMPT.md` §4.1 invariant 2 (append-only; never delete rows). All other ROADMAP rows + family headings untouched; the schema header untouched.
- **MODIFY** `docs/envoy-rust/STATE.md` — advances "Active phase" pointer from `_none — awaiting next planning_` to:
  - `id: 14`
  - `slug: 14-outlier-detection`
  - `directory: docs/envoy-rust/phases/14-outlier-detection/`
  - `status: phase 14 lifecycle state 1-complete / state-2-next (SPEC.md landed; PLAN.md does not exist; SPLIT into 14.1 + 14.2 projected at state-2)`

  Rewrites "Next expected skill" to `superpowers:writing-plans` scoped to this SPEC. Rewrites "Last commit" + "Last updated" with prior demoted to `_Historical_`. Appends a new "Phase-14 state-1 brainstorm" subsection in Notes recording the family-pick + feature-pick rationale + the split projection + the ADR projection. Preserves all prior subsections verbatim per D-3.5 (append-only) + D-3.4 (context isolation) — including the `### Phase-13.2 rollovers` + all `### Phase-13.* state-*` subsections.

No code changes, no fixture changes, no Cargo.toml changes, no DECISIONS.md changes (ledger head stays **ADR-0039**), no BEHAVIOR_CONTRACT.md changes. ENVOY_TARGET.md + rust-toolchain.toml untouched (D-3.7 / D-3.9 unchanged).

**Commit message:**

```
phase 14: state-1 brainstorm — outlier-detection SPEC.md (Upstream robustness family third phase; passive health checking via consecutive 5xx + consecutive gateway failure)
```

Per the project precedent (phase-12 / phase-13 state-1 brainstorm commit title shape — descriptive title with a parenthesized scope summary). No `[ADR-NNNN]` brackets — no ADR lands at this commit.

**Predecessor:** `96630f9` — phase-13.2 state-6 CLOSING-sub-phase close-out (the most-recent commit; docs-only; flipped row `13.2` AND parent-13 `13` `in-progress → done` simultaneously).

**Origin/main:** `96630f9`. Local + origin are in sync as of THIS state-1 brainstorm commit's prologue. After landing, the docs-only edits push to origin and the next CI run re-validates the docs-only edits compile cleanly through the 5 stable-toolchain gates + the parse_bootstrap fuzz target on the unchanged 21-seed corpus (predecessor docs-only CI runs took ~2-3m).

---

*End of SPEC. Phase 14 state-1 lifecycle complete on landing. The next session enters state 2 — writes PLAN.md per `superpowers:writing-plans`, performs the §6.2 empirical verification at PLAN-write (outlier-detection config shape + stat namespace + initial ejection state + max_ejection_percent semantics + ejection-time semantics + fixture 0022 discriminating observable + composition with active HC + H1/H2 sibling consistency + synth-status bypass), and evaluates the §6.1 split gate (SPLIT into 14.1 + 14.2 recommended; land ADR-0040).*
