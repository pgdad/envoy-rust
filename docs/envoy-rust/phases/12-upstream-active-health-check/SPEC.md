# Phase 12 (`12-upstream-active-health-check`) — SPEC

- **Phase id:** `12`
- **Slug:** `12-upstream-active-health-check`
- **Status before this SPEC lands:** _not yet in ROADMAP.md_ (per `docs/envoy-rust/ROADMAP.md` at HEAD `7e27c1e`, the phase-11 state-6 close-out commit; the "Upstream robustness family" §9 heading exists as a heading-only entry with no concrete rows beneath it). **This SPEC's landing commit adds the FIRST concrete row beneath the "Upstream robustness family" heading**, with `status: planned`.
- **Charter source:** `BOOTSTRAP_PROMPT.md` §9 — *"Upstream robustness family — active health checks (HTTP/TCP/gRPC/custom), outlier detection variants, circuit breakers, retries + hedging, per-protocol connection pooling."* This phase lands **active HTTP health checking** narrowed to the minimum-viable surface: per-cluster active HTTP health checks that periodically probe each endpoint, drive a per-endpoint health state machine (consecutive-success/failure thresholds), and exclude unhealthy endpoints from load-balancer selection — closing the loop with the pre-built `Cluster::pick() -> Option` seam and the existing no-healthy-endpoint → synth-503 writer path. TCP/gRPC/custom health checkers, outlier detection, circuit breakers, retries, and connection pooling all defer per §4 below.
- **Position in the project:** the **fourth post-MVP-trunk feature-family phase** and the **first concrete Upstream-robustness-family phase** (after three consecutive HTTP-filter-family phases: 09 `local_ratelimit`, 10 `rbac`, 11 `fault`). The MVP trunk 00→08 stands `done` as of commit `304ce98`; phases 09/10/11 stand `done` as of `518140c`/`e24053e`/`7e27c1e`. **This is the first feature-family phase to leave the HTTP-filter family** — the 09 REVIEW M2 H2-decoration carryforward that decisively drove the 09 → 10 → 11 HTTP-filter picks was CLOSED at phase 11 (the H1 + H2 HCM filter-synth writer paths are now at parity), freeing the next pick to diversify per the brainstorm scoring (§ STATE.md "Phase-12 state-1 brainstorm" subsection).
- **depends-on:** `02 04` — phase `02` (the cluster manager + round-robin LB seam: `envoy-cluster::Cluster::pick()`, `ClusterHandle::pick_endpoint() -> Option<SocketAddr>`, `ClusterManager`) is the foundation being extended; phase `04` (specifically 04.3's `envoy-http1::Client`) is load-bearing because the active HTTP health-check probe reuses the existing H1 client. An **implicit dependency on phase `06`** (the `envoy-stats` foundation: `StatsRegistry` + `Counter`/`Gauge` primitives) is load-bearing for the health-check stats but is not added to the depends-on field per ROADMAP schema conventions (the schema captures the primary direct foundations; cross-deliverable reuse — `envoy-stats`, `envoy-config::parse_duration`, `envoy-config::Int64Range` — is implicit). The 18-Docker-gated-fixture regression baseline established at phase-11 close (`0001-tcp-echo` through `0018-http-filter-fault`) carries forward unchanged per `BOOTSTRAP_PROMPT.md` §7.5 (b).
- **Brainstorm narrative:** see the "Phase-12 state-1 brainstorm" subsection of `docs/envoy-rust/STATE.md` for the family-pick + feature-pick rationale with alternatives considered along the 5-dimension scoring framework (carryforward closure value / foundation pressure / architectural risk / contract-surface maturity / first-phase scope tractability).

---

## 1. Goal and acceptance signal

Phase 12 lands **active HTTP health checking** as the first concrete Upstream-robustness-family feature. When a cluster is configured with an active HTTP health check (`Cluster.health_checks` with an `http_health_check` block), envoy-rust spawns a background periodic-probe task per endpoint that, every `interval`, issues an HTTP request to the endpoint's configured health-check `path` and evaluates the response status against the configured `expected_statuses` (default: 200). The result drives a per-endpoint health state machine with `healthy_threshold` / `unhealthy_threshold` consecutive-result transitions. The load balancer's `pick()` excludes unhealthy endpoints; when a cluster has zero healthy endpoints (and panic routing is not engaged), the existing no-healthy-endpoint writer path returns the synthetic 503.

The phase **engages and makes the first down-payment on the named carryforward, 06.3 REVIEW I2**:

- **06.3 REVIEW I2** (`docs/envoy-rust/phases/06.3-stats-wiring-and-close/REVIEW.md` §3) deferred the *synthetic-backend harness infrastructure* (a backend cluster that returns operator-controlled non-2xx responses + the `{{SYNTHETIC_5XX_PORT}}`-style harness template) plus the wire-level bilateral coverage that infrastructure unlocks. The REVIEW named the close site verbatim: *"whichever phase first surfaces the synthetic backend (... could be folded into the upstream-robustness family ...)."* **Phase 12 IS the first upstream-robustness-family phase**, and its fixture (D7.1) requires exactly a synthetic backend that returns a non-2xx status on the health-check path. Phase 12 therefore lands the FIRST synthetic-health-aware backend harness primitive — the I2 down-payment. **Phase 12 does NOT fully close I2**: I2's residual (the per-class `downstream_rq_3xx/4xx/5xx` + `cluster.<name>.upstream_rq_5xx` wire-level coverage, and the `cluster.<name>.upstream_cx_total` tightening to `value-exact`) remains tied to connection pooling per the REVIEW disposition. Phase 12 advances I2; full closure remains a later upstream-robustness phase. The PROGRESS narrative attributes the down-payment honestly and does NOT over-claim full I2 closure.

**Differential surface added by phase 12:**

- **Fixture `0019-upstream-active-health-check`** — bilateral assertion that both proxies, given an identical bootstrap configuring an active HTTP health check against a synthetic backend whose health-check path returns a non-2xx status, converge (after a settle window) to ejecting the unhealthy endpoint and returning the synthetic no-healthy-upstream 503 to a downstream data-plane request. The discriminating differential observable is **status 503 + the no-healthy-upstream synth body** on the data-plane request (vs the 200 the backend's data path would return if health checking were absent / the endpoint were not ejected). The exact no-healthy-upstream body bytes + response flags + the health-check stat names + the initial-endpoint-health-state semantics + the panic-threshold semantics are **empirically verified at state-2 PLAN-write per §6.2** (this phase has an unusually large empirically-discoverable surface — see §6.2).

**Acceptance signal (a)–(f), per `BOOTSTRAP_PROMPT.md` §7.5:**

- **(a)** Fixture `0019-upstream-active-health-check` green at Docker-gated CI.
- **(b)** All **18 pre-existing differential fixtures** (`0001-tcp-echo` through `0018-http-filter-fault`) **remain green simultaneously** at the same CI run (regression-equivalence per `BOOTSTRAP_PROMPT.md` §7.5 (b)). The existing fixtures configure NO health checks; phase 12's health-check machinery must be inert when `health_checks` is absent (no behavior change on the existing fixtures).
- **(c)** `h2spec` continues at ≥95% (parent-05 baseline 99.31%). Phase 12 does NOT touch the H2 codec/framing path (health checking is a cluster/LB-layer feature, codec-agnostic on the downstream side); the state-4 verification re-confirms the gate held.
- **(d)** `parse_bootstrap` fuzz target clean for the short-budget CI run on the extended corpus (one new seed for the health-check bootstrap shape; corpus extends from 18 to 19 seeds).
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check` all clean.
- **(f)** `REVIEW.md` approved.

A **single CI run** must light up gates (a) through (e) **simultaneously** (continues the project precedent established at 06.1 / 07.x / 08.x / 09 / 10 / 11 — fixture inheritance is a regression vector).

> **NOTE — likely phase split (see §6.1).** Phase 12's surface (a new periodic-task primitive + an endpoint health state machine + config schema + validator + LB integration + panic-threshold honoring + a new synthetic-backend harness primitive + a settle-then-probe differential driver + in-process backstop + fuzz seed) is projected at **~1800–2200 LoC**, materially over the `BOOTSTRAP_PROMPT.md` §6.1 ~1500-LoC split gate. **The state-2 PLAN-writer is expected to split phase 12 into `12.1` + `12.2`** per §6.1 / §6.2 (recommended seam below). This SPEC covers the whole feature; if the state-2 LoC estimate confirms >~1500, the PLAN-write executes the split (creating `12.1`/`12.2` SPECs + the split ADR), mirroring the parent-phase split cadence used for phases 02–08.

---

## 2. Behavior-contract scope for phase 12

Phase 12 extends `docs/envoy-rust/BEHAVIOR_CONTRACT.md` with authored additions, landed at the tasks where each is first empirically exercised (per the established 06.x / 07.x / 08.x / 09 / 10 / 11 doctrine — contract extensions land at empirical-engagement task time, NOT at PLAN-write time and NOT at state-1 SPEC time).

### 2.1 "Stat-name mapping" extension — health-check stats (projected; §6.2-verified)

New rows under the cluster health-check stat namespace, mirroring upstream Envoy v1.33's documented stat tree. Upstream's active health checking emits (under `cluster.<name>.health_check.*`): `attempt`, `success`, `failure`, `passive_failure`, `network_failure`, `verify_cluster`; plus the cluster membership gauges `cluster.<name>.membership_healthy`, `membership_total`, `membership_degraded`, `membership_excluded`. At phase-12 minimum-viable scope, the wired subset (the rest defer with their features per §4):

| Stat name | Equivalence (projected; §6.2-verified) | Rationale |
|---|---|---|
| `cluster.<name>.health_check.attempt` | name-required, value-may-differ | Counter; one increment per health-check probe issued. The count is **timing-dependent** (it equals the number of probe intervals elapsed during the test window, which differs across the two proxies' independent schedulers/process-start instants). Both proxies emit the name; values diverge by probe count. |
| `cluster.<name>.health_check.success` | name-required, value-may-differ | Counter; one increment per probe whose response status is in `expected_statuses`. Timing-dependent like `attempt`. |
| `cluster.<name>.health_check.failure` | name-required, value-may-differ | Counter; one increment per probe whose response status is NOT in `expected_statuses` (or that fails to connect/times out). Timing-dependent. |
| `cluster.<name>.membership_healthy` | value-exact (steady state) | Gauge; the count of currently-healthy endpoints in the cluster. Converges to a deterministic STEADY-STATE value after health-check convergence (e.g. `0` when the sole endpoint is ejected; `1` when healthy). The fixture asserts the post-settle steady-state value, NOT a transient. |

**Namespace empirical-verification signpost:** the `cluster.<name>.health_check.*` + `cluster.<name>.membership_healthy` shapes are the recommended state-1 projections per the 06.1 cluster-stats convention. **The state-2 PLAN-writer empirically verifies the exact stat names + which counters exist + the gauge semantics against `envoyproxy/envoy:v1.33.0` + admin `/stats` scrape** before locking (per §6.2). The `attempt/success/failure` timing-dependence means the differential fixture does NOT assert their values (it asserts only the data-plane 503 + optionally the `membership_healthy` steady-state gauge); the counters are exercised by the in-process backstop (D7.3) + unit tests (D4/D6).

### 2.2 "No-healthy-upstream synth response" — body + flags (projected; §6.2-verified)

When a cluster has zero healthy endpoints (all ejected by health checking, and panic routing not engaged), `Cluster::pick()` returns `None`, and the existing HCM writer path emits a synthetic 503 (`crates/envoy-http1/src/hcm.rs:580-582` — *"no healthy endpoint for cluster — returning 503"*; the `RouterError::NoHealthyEndpoint` at `crates/envoy-http1/src/router.rs:18-20` *"covers the case where `pick_endpoint()` returns `None` for any reason"*). Phase 12 must verify this synth-503's wire shape matches upstream Envoy v1.33's no-healthy-upstream response:

- **Status:** 503 (already emitted by the existing path).
- **Body bytes:** upstream Envoy v1.33 emits a `no healthy upstream` body on the no-healthy-upstream path. **The exact body bytes are §6.2-verified at state-2 PLAN-write.** If envoy-rust's existing synth-503 body differs from upstream's no-healthy-upstream body, phase 12 reconciles (either the implementation is adjusted, or a BEHAVIOR_CONTRACT row + ADR documents the divergence — never both silently, per D-3.3). **The phase-10/11 experience (RBAC body off by 1 byte; verify-don't-assume) makes this load-bearing.**
- **Response flags:** upstream emits the `UH` (`no healthy upstream`) response flag in access logs / `x-envoy-...`; phase 12's differential fixture does NOT assert response flags unless the §6.2 verification shows a wire-observable flag header (response-flags in the access log are an observability surface; the fixture asserts the data-plane status + body).
- **Headers:** the synth-503 standard headers (`server`, `date`, `content-length`, `content-type`) are covered by the existing 04.1 `server` + `date` Header allow-list rows; `content-length`/`content-type` are value-exact under the deterministic fixture. **No new Header allow-list row is projected** (§6.2 confirms).

### 2.3 No DECISIONS.md amendment required at SPEC time

Phase 12 lands no carryforward whose close shape is a *documentation* amendment (unlike phases 10/11, which amended ADR-0033). The 06.3 REVIEW I2 down-payment is ordinary deliverable work (building the synthetic-backend harness + the fixture), not a decision; the PROGRESS narrative attributes it. **No new ADR is required at SPEC time.** Conditional ADRs (the likely split ADR; the §6.2 empirical-verification revision) are enumerated in §7.

---

## 3. Deliverables

Phase 12's scope is enumerated as deliverables `D1`–`D8` below. **The state-2 PLAN-writer organizes deliverables into tasks AND evaluates the §6.1 split gate** (which is projected to fire — see §6.1). These deliverables are LISTED roughly in execution order but the SPEC is not prescriptive about task organization; only about the surface. If the phase splits, the recommended seam (§6.1) assigns D1/D2/D3/D5/D6 to `12.1` and D4/D7/D8 to `12.2`.

### D1 — `envoy-config` schema extension (cluster health-check config + panic threshold)

At `crates/envoy-config/src/bootstrap.rs`, extend the existing `Cluster` struct (`bootstrap.rs:56`) with a `health_checks` field and a `common_lb_config` field:

```rust
pub struct Cluster {
    // ... existing fields (name, type, lb_policy, load_assignment, upstream_protocol, ...) ...
    #[serde(default)]
    pub health_checks: Vec<HealthCheck>,        // OPTIONAL; phase-12 supports exactly 0 or 1, HTTP-only
    #[serde(default)]
    pub common_lb_config: Option<CommonLbConfig>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HealthCheck {
    pub timeout: String,                        // parse_duration; per-probe response timeout
    pub interval: String,                       // parse_duration; between probes
    pub healthy_threshold: u32,                 // consecutive successes to mark healthy
    pub unhealthy_threshold: u32,               // consecutive failures to mark unhealthy
    pub http_health_check: HttpHealthCheck,     // REQUIRED at phase 12 (the only supported checker type)
    // OPTIONAL — all defer per §4:
    //   tcp_health_check / grpc_health_check / custom_health_check  (validator rejects)
    //   no_traffic_interval / unhealthy_interval / interval_jitter / reuse_connection
    //   no_traffic_healthy_interval / initial_jitter / event_log_path / always_log_health_check_failures
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HttpHealthCheck {
    pub path: String,                           // REQUIRED at phase 12
    #[serde(default)]
    pub host: Option<String>,                   // OPTIONAL :authority / Host on the probe (defaults to cluster name per upstream)
    #[serde(default)]
    pub expected_statuses: Vec<Int64Range>,     // OPTIONAL; default = [200,201) i.e. exactly 200 (reuses 04.x Int64Range)
    // codec_client_type / service_name_matcher / request_headers_to_add defer per §4
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CommonLbConfig {
    #[serde(default)]
    pub healthy_panic_threshold: Option<Percent>,  // default 50% per upstream; 0 disables panic
}
```

The `healthy_panic_threshold` uses a `Percent` type: **the PLAN-writer decides between (a) adding a new `Percent { value: f64 }` envoy-config type (matches upstream Envoy's `type.v3.Percent` `{ value: double }` shape) or (b) reusing the phase-11 `FractionalPercent`.** Recommended: (a) a small new `Percent` type — upstream's `common_lb_config.healthy_panic_threshold` is a `Percent` (`{value: 50.0}`), structurally distinct from `FractionalPercent` (numerator/denominator). §6.2 verifies the exact config shape.

`Int64Range` (`bootstrap.rs:1080`, with `InvalidInt64Range` validation) and `parse_duration` (`bootstrap.rs:2289`, accepts `60s`/`250ms`/`500us`) are **reused directly** (confirmed present at SPEC time) — no duration/range primitive is duplicated. All struct shapes carry `#[serde(deny_unknown_fields)]` per the established envoy-config discipline (rejects the phase-12-deferred upstream fields enumerated in §4). The phase-12-deferred fields are each enumerated in §4; each is rejected by `deny_unknown_fields`.

### D2 — `envoy-config` validator extension

At `crates/envoy-config/src/bootstrap.rs` cluster-validation site (the existing `from_bootstrap`/validator path that produces `ClusterError`/`ConfigError`), add a `validate_health_checks(cluster) -> Result<(), ConfigError>` sub-validator. The validator checks:

- **At most one** `health_checks` entry at phase-12 scope (upstream allows multiple; phase 12 supports 0 or 1). More than one → `ConfigError::UnsupportedMultipleHealthChecks { cluster }`.
- The single health check **is HTTP** (`http_health_check` present; TCP/gRPC/custom rejected) → `ConfigError::UnsupportedHealthCheckType { cluster }`.
- `healthy_threshold >= 1` and `unhealthy_threshold >= 1` → `ConfigError::InvalidHealthCheckThreshold { cluster, field }`.
- `timeout` and `interval` parse via `parse_duration` and are `> 0` → `ConfigError::InvalidHealthCheckTiming { cluster, field }` (reuses the `parse_duration` error surface).
- `http_health_check.path` is non-empty → `ConfigError::EmptyHealthCheckPath { cluster }`.
- each `expected_statuses` range is a valid `Int64Range` (delegates to the existing `Int64Range` validation).
- if `common_lb_config.healthy_panic_threshold` present, its `value` is in `[0.0, 100.0]` → `ConfigError::InvalidPanicThreshold { cluster, value }`.

Roughly **4–6 new `ConfigError` variants** land at this site (the PLAN-writer may consolidate). Each carries `cluster: String` per the established envoy-config error-context discipline. Each has positive + negative parse-path unit tests. The validator is exercised by the existing `parse_bootstrap` fuzz target (the new fixture's bootstrap seeds the corpus per D8.2).

### D3 — Per-endpoint health state machine (`envoy-cluster`)

In `crates/envoy-cluster/`, add a per-endpoint `EndpointHealth` type carrying the runtime health state, owned by the `Cluster` and consulted by `pick()`:

```rust
/// Per-endpoint active-health-check state. Shared (Arc) so the health-check
/// task (D4) can mutate it while pick() (D5) reads it.
pub struct EndpointHealth {
    state: AtomicU8,            // Healthy | Unhealthy (discriminant)
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

**Initial state (§6.2-verified — load-bearing):** upstream Envoy's initial endpoint health state when active HC is configured (healthy-until-first-failure vs unhealthy-until-first-success/pending) is **the single most-important §6.2 verification item** — it determines whether the convergence in the fixture is "starts healthy then ejected" or "starts pending then never passes." The PLAN-writer pins this empirically before locking D3's initial-state semantics. (Recommended state-1 projection, to be verified: hosts with active HC configured begin in a not-yet-checked state that is treated as healthy=false until the first check resolves, with `no_traffic_interval` nuances — DO NOT ASSUME; verify.)

The state machine lives in `envoy-cluster` (NOT a new crate) so `pick()` can read it without a cross-crate dependency. The health-check TASK that mutates it lives elsewhere (D4) to avoid a dependency cycle — see §5.1.

### D4 — Active HTTP health-check task (the new periodic-probe primitive)

The headline new architectural primitive: a background tokio task, spawned per (cluster, endpoint), that loops every `interval`, issues an HTTP health-check probe, evaluates the result, and calls `EndpointHealth::record_success`/`record_failure`.

**Crate-placement / dependency-cycle decision (§5.1; PLAN-write lock-in):** the probe needs the `envoy-http1::Client`. But `envoy-http1` already depends on `envoy-cluster` (`router` → `ClusterHandle`), so `envoy-cluster` MUST NOT depend on `envoy-http1` (cycle). Two clean options — **the PLAN-writer picks and records the choice as a lock-in (and an ADR only if a cycle-resolution choice needs recording, per the ADR-0031 precedent; a plain new crate forming a clean DAG needs no ADR):**

- **(A) New `envoy-health` crate (recommended).** Depends on `envoy-cluster` (to read endpoints + write `EndpointHealth`) + `envoy-http1` (the probe `Client`) + `envoy-config` + `envoy-stats` + `tokio`. Owns the health-check task + scheduler. Wired at `envoy-bin` startup. Forms a clean DAG (`envoy-health → {envoy-http1 → envoy-cluster}`); no cycle. Matches the project's one-crate-per-from-scratch-component pattern (`envoy-stats`, `envoy-accesslog`, `envoy-filter`). The natural long-term home for the rest of the upstream-robustness family (outlier detection, circuit breakers).
- **(C) `HealthProber` trait injection.** A `HealthProber` trait declared in `envoy-cluster`; the task lives in `envoy-cluster` and calls `dyn HealthProber`; the concrete HTTP prober (using `envoy-http1::Client`) is impl'd in `envoy-bin` (or `envoy-http1`) and injected at startup. No new crate; dependency-inverts the cycle. Lighter, but places a periodic-task scheduler inside the currently-pure `envoy-cluster`.

Recommended: **(A)**. The task is hand-rolled per **D-3.2** (*"Active health checking ... Must be written from scratch"*). Probe shape (§6.2-verified): `GET <path>` with the `:authority`/`Host` set per upstream (defaults to the cluster name unless `http_health_check.host` overrides); response status checked against `expected_statuses` (default exactly 200); connection-failure / timeout counts as a failure. Tasks are tied to cluster/process lifetime and cancelled on shutdown (tokio task handles held by the scheduler; `kill_on_drop`-equivalent cancellation). **No async-runtime change** — reuses the existing tokio runtime.

### D5 — Load-balancer integration (exclude unhealthy + panic threshold)

Modify `crates/envoy-cluster/src/cluster.rs::Cluster::pick()` (`cluster.rs:129`) to consult `EndpointHealth`:

- Build the candidate set as the **healthy** endpoints (round-robin over healthy endpoints only).
- **Panic threshold:** compute `healthy_fraction = healthy_count / total_count`. If `healthy_fraction < panic_threshold` (default 50%; `common_lb_config.healthy_panic_threshold`), enter **panic mode** — round-robin over **ALL** endpoints (healthy and unhealthy), matching upstream Envoy's panic-routing safety valve. A configured `healthy_panic_threshold: 0` disables panic (0% healthy never triggers panic → `pick()` returns `None` → the existing no-healthy-503 path fires). **The exact panic semantics (strictly-below vs at-or-below; the default value) are §6.2-verified.**
- When the (non-panic) healthy set is empty, `pick()` returns `None` — the **pre-built** no-healthy-endpoint → synth-503 path (`hcm.rs:580-582`) fires unchanged. This is the integration seam the existing code annotated *"`Option<_>` is preserved for phase-06+ health checking"* (`cluster.rs:152`).

Clusters with NO `health_checks` configured: all endpoints are implicitly healthy (the `EndpointHealth` is either absent or initialized-healthy), so `pick()` behaves exactly as today — **the existing 18 fixtures see no behavior change** (acceptance gate (b)).

### D6 — Health-check stats wiring + BEHAVIOR_CONTRACT extension

At cluster construction (or health-check task spawn), register the health-check counters + the membership gauge against the `Arc<StatsRegistry>` (the 06.x convention; `register_counter`/`register_gauge` idempotent for same-name re-registration):

- `cluster.<name>.health_check.attempt` / `.success` / `.failure` (counters; incremented in the D4 task).
- `cluster.<name>.membership_healthy` (gauge; updated on every `EndpointHealth` state transition — one source of truth, NOT polled, mirroring the 08.2 `server.live`/`server.state` inline-CAS-site pattern).

**D6.1 — `Stat-name mapping` rows** (§2.1) land at the D6 stats-wiring task commit. **D6.2 — the no-healthy-upstream body/contract reconciliation** (§2.2) lands at the task where the synth-503 path is first exercised bilaterally (the fixture task). Per the 06.x → 11 cadence: contract extensions land at the empirical-engagement task, NOT at PLAN-write and NOT at SPEC time.

### D7 — Fixture + synthetic-backend harness + Docker wrapper + fuzz seed + in-process backstop

- **D7.1 — Fixture `tests/fixtures/0019-upstream-active-health-check/` + the synthetic-backend harness primitive (the 06.3 REVIEW I2 down-payment).** The fixture configures a cluster with one endpoint pointing at a **synthetic backend** that returns a non-2xx status on the health-check path (`/healthz`) and 200 on the data path (`/`), an active HTTP health check (`path: /healthz`, `expected_statuses: [200]`, `unhealthy_threshold: 1`, short `interval`/`timeout`), and `common_lb_config.healthy_panic_threshold: { value: 0 }` (panic disabled). After a settle window (≥ `interval × unhealthy_threshold + timeout` + margin), both proxies eject the endpoint → a downstream `GET /` returns the **synthetic 503 no-healthy-upstream** (NOT the 200 the backend's data path would serve absent health checking). This is the discriminating differential observable. Bootstrap sketch:

  ```yaml
  static_resources:
    clusters:
    - name: hc_backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      common_lb_config:
        healthy_panic_threshold: { value: 0 }
      health_checks:
      - timeout: 1s
        interval: 0.5s
        healthy_threshold: 1
        unhealthy_threshold: 1
        http_health_check:
          path: /healthz
          expected_statuses: [ { start: 200, end: 201 } ]
      load_assignment: { ... single endpoint -> the synthetic backend ... }
    listeners:
    - name: ingress_http
      # HCM with a router routing "/" to cluster hc_backend
  ```

  **Synthetic-backend harness primitive (the I2 down-payment):** a small test backend (extend the existing echo-server helper, or a new `tests/helpers` health-aware backend) that serves a configurable status per path — 200 on `/`, a configured non-2xx (e.g. 503) on `/healthz`. This is the synthetic-backend infrastructure 06.3 REVIEW I2 named. The PLAN-writer locates the existing echo-server helpers + decides extend-vs-new.

  **Harness signpost (PLAN-write decision):** the fixture needs a **settle-then-probe** capability (wait for HC convergence, THEN drive the data-plane request + assert). Precedents: `Driver::AdminScrape`'s `within_ms` polling + the 08.2 drain fixture's connection-refused polling. **Recommended: a new `Driver::Http1AfterSettle`-style variant** (or a `settle_ms` field on a probe driver) that sleeps/polls past convergence, then drives one `Http1` request and asserts status + body. The state-2 PLAN-writer confirms the existing `Driver` internals and picks the minimal extension. Phase 12 does NOT opt into Timing tolerances (§7.2) — it asserts the post-convergence STEADY STATE; the settle window is a harness mechanic, not a compared latency bound.

  Docker-gated wrapper at `tests/differential/tests/upstream_active_health_check.rs` mirroring the 10/11 `http_filter_*.rs` wrapper shape.

- **D7.2 — Fuzz corpus seed.** New file `crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_health_check.yaml` containing the health-check bootstrap shape. Mirrors the 07.2/09/10/11 corpus-seed precedent. Extends the seed coverage 18 → 19, with the `crates/envoy-config/fuzz/.gitignore` allow-list extension AND the `bootstrap.rs::tests::fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS-array extension (both files edited together, per the 09/10/11 Task-6 lesson).

- **D7.3 — In-process backstop.** New file `crates/envoy-bin/tests/upstream_active_health_check.rs` mirroring the 10/11 backstop shape — **with the 09 REVIEW M3 subprocess discipline baked in** (`tokio::process::Command` + `.kill_on_drop(true)` + `Stdio::null()`/`piped()`; the standing pattern since phase 10). Boots `envoy-bin` with a synthesized bootstrap + an in-process synthetic backend; exercises BOTH the healthy path (backend healthy → after settle, `GET /` → 200 through) AND the unhealthy path (backend unhealthy → after settle, `GET /` → 503 no-healthy body), giving cheap H1 coverage of both convergence directions to complement the Docker differential fixture. Per the 10 REVIEW M1 / 11 §6.4 lesson, include the per-probe standard-header presence assertion on the 503 probe OR disclose its omission.

### D8 — (reserved / folded)

D8 is intentionally folded into D7 above (fixture + harness + fuzz + backstop are one cohesive deliverable cluster for this phase). Numbering kept D1–D7 + the implicit state-4 verification task. (The §6.1 split, if it fires, assigns D1/D2/D3/D5/D6 → `12.1`; D4/D7 → `12.2`.)

---

## 4. Out of scope (deferred non-goals)

Phase 12 explicitly does NOT land:

- **TCP / gRPC / custom health checkers.** Only `http_health_check` is supported; the validator (D2) rejects `tcp_health_check`/`grpc_health_check`/`custom_health_check` with `ConfigError::UnsupportedHealthCheckType`. The HTTP checker is the most common + the one reusing the existing H1 client. TCP/gRPC/custom defer to follow-up upstream-robustness phases.
- **Multiple health checks per cluster.** Phase 12 supports 0 or 1; the validator rejects >1. Defers.
- **Outlier detection (passive health).** Ejecting endpoints based on data-plane 5xx/timeout/consecutive-gateway-failure is a distinct subsystem (passive vs active). Defers to a follow-up upstream-robustness phase. (It would reuse the same `EndpointHealth` ejection seam landed here.)
- **Circuit breakers** (`max_connections`/`max_pending_requests`/`max_requests`/`max_retries`). The overflow observable is concurrency/timing-dependent (hard to make deterministic cross-proxy). Defers.
- **Retries + hedging** (`retry_policy`, `num_retries`, `retry_on`, `per_try_timeout`). An inline request-path enhancement; differential observability is largely stats-based or needs a stateful backend. Defers to a follow-up.
- **Per-protocol connection pooling.** The named full-closure site for 06.3 REVIEW I2's `cluster.<name>.upstream_cx_total` `value-exact` tightening. Phase 12 advances I2 (synthetic-backend infra) but does NOT land pooling. Defers.
- **`no_traffic_interval` / `unhealthy_interval` / `interval_jitter` / `initial_jitter` / `reuse_connection`.** Probe-cadence refinements. Phase 12 uses a single fixed `interval` + a fresh connection per probe (simplest). Rejected by `deny_unknown_fields`; defer.
- **Health-check event logging (`event_log_path`, `always_log_health_check_failures`).** Observability surface; defers.
- **`degraded` / `excluded` host states + `membership_degraded`/`membership_excluded` gauges.** Phase 12 has a binary Healthy/Unhealthy state machine. The degraded state defers.
- **Health-check request headers (`request_headers_to_add`) + `service_name_matcher` + `codec_client_type`.** The probe is a plain `GET <path>`. Defer.
- **Active HC over an H2 upstream.** The phase-12 probe uses the H1 client (the existing, simplest path). H2-upstream health checking defers (the `codec_client_type` knob).

---

## 5. Architectural invariants

Phase 12 honors and extends the established cross-crate invariants:

### 5.1 Crate boundaries + the dependency-cycle constraint (load-bearing)

- **`EndpointHealth` state lives in `envoy-cluster`** so `Cluster::pick()` reads it with no cross-crate dependency. `envoy-cluster` stays a leaf-ish crate (no dependency on `envoy-http1`).
- **The health-check TASK lives outside `envoy-cluster`** — recommended in a **new `envoy-health` crate** (option A, §D4) that depends on `envoy-cluster` + `envoy-http1` + `envoy-config` + `envoy-stats` + `tokio`, forming a clean DAG (no cycle, because `envoy-http1 → envoy-cluster` already exists and `envoy-health` sits above both). The alternative (option C: a `HealthProber` trait in `envoy-cluster` + injection from `envoy-bin`) is acceptable if the PLAN-writer prefers no new crate. **This cycle-avoidance is the central architectural decision of the phase** (mirrors the ADR-0028 / ADR-0031 cycle-resolution lineage). A plain new crate forming a clean DAG needs no ADR; an ADR lands only if a non-obvious cycle-resolution choice is recorded.
- **No new path-dep introduces a cycle.** The state-3 implementer verifies the dependency graph stays acyclic (`cargo` will reject a cycle at build time; the design must not require one).
- New workspace member (`envoy-health`, if option A) is added to the root `Cargo.toml` `members` list per the phase-N crate-introduction cadence.

### 5.2 Hand-rolled per D-3.2

Active health checking is hand-rolled per **D-3.2**'s *"Active health checking, outlier detection, circuit breakers ... Must be written from scratch"* doctrine. The implementation uses only **std-lib + tokio + bytes + envoy-http1 (Client) + envoy-config + envoy-stats** — all D-3.2-permitted; all already pulled. The periodic-probe scheduler + the health state machine + the threshold transitions + the panic-threshold check are all written from scratch.

**Explicit non-grants:** no new top-level crate. No `tower`/health-check library. No `rand` (no jitter at phase-12 scope per §4). None on D-3.2's permitted-foundations list beyond what is already pulled. The state-3 implementer must NOT pull any new top-level crate (any pull forces a foundations-grant ADR per D-3.5).

### 5.3 No new top-level Cargo deps

The recommended no-foundations-grants posture carries forward. A new *workspace-internal* crate (`envoy-health`) is NOT a foundations grant (it pulls only already-permitted deps). **If the state-3 implementer surfaces a genuine external-crate need, a foundations-grant ADR lands per D-3.5 — see §7.**

### 5.4 Health checking is inert when unconfigured (regression-equivalence)

When `Cluster.health_checks` is empty/absent, NO health-check task spawns, all endpoints are healthy, `pick()` behaves exactly as phase-02 round-robin, and the 18 existing fixtures see ZERO behavior change. This is the regression-equivalence invariant (acceptance gate (b)) and the load-bearing safety property of the phase.

### 5.5 The active-HC task is the first periodic-background primitive

Phase 12 introduces the project's first **periodic timer-driven background task** (prior tasks are connection-scoped or request-scoped). The task is tied to cluster/process lifetime and cancelled cleanly on shutdown. This primitive is the foundation the rest of the upstream-robustness family (outlier-detection ejection windows, circuit-breaker accounting) will reuse. The state-3 implementer ensures graceful cancellation (no leaked tasks; tests assert task shutdown).

### 5.6 Health decision semantic (cross-proxy deterministic in steady state)

The cross-proxy invariant is the **steady-state** health decision, NOT the convergence timing:

- **Probe:** every `interval`, `GET <path>`; success iff response status ∈ `expected_statuses`.
- **Transition:** `unhealthy_threshold` consecutive failures → Unhealthy; `healthy_threshold` consecutive successes → Healthy.
- **Pick:** round-robin over healthy endpoints; if `healthy_fraction < panic_threshold`, panic (round-robin over all); if the (non-panic) healthy set is empty, `pick()` → `None` → synth-503.

**Determinism across both proxies** holds in STEADY STATE because (a) the synthetic backend's per-path status is deterministic (both proxies' probes get the same status), and (b) given enough settle time, both proxies' state machines converge to the same Healthy/Unhealthy verdict regardless of their independent probe schedules. The fixture asserts the post-settle steady state. **Timing is NOT compared** (per §7.2); the convergence WINDOW differs across proxies but the converged STATE does not. Phase 12 does NOT opt into Timing tolerances.

### 5.7 The pre-built `pick() -> Option` + no-healthy-503 seam

Phase 12 plugs into a seam the codebase deliberately reserved: `ClusterHandle::pick_endpoint() -> Option<SocketAddr>` (`cluster.rs:152`, annotated *"preserved for phase-06+ health checking"*) + `RouterError::NoHealthyEndpoint` (`router.rs:18-20`, *"covers the case where `pick_endpoint()` returns `None` for any reason"*) + the HCM synth-503 arm (`hcm.rs:580-582`). The health-driven `None` reuses this path unchanged — no new writer-arm wiring is needed (contrast phase 11's new H2 writer arm). This is the primary reason the architectural risk is LOW-MEDIUM despite the new subsystem.

---

## 6. Implementation signposts for the planner

The state-2 PLAN-writer reads this section to drive PLAN structure.

### 6.1 Split-gate evaluation (READ FIRST — split projected to fire)

Per `BOOTSTRAP_PROMPT.md` §6.1, the state-2 PLAN-write evaluates whether the PLAN exceeds ~25 numbered tasks OR ~1500 LoC. Phase 12's surface estimate at SPEC time:

- D1 — envoy-config schema (HealthCheck + HttpHealthCheck + CommonLbConfig + Percent) (~160 LoC + ~160 LoC tests).
- D2 — envoy-config validator (4–6 ConfigError variants) (~90 LoC + ~140 LoC tests).
- D3 — `EndpointHealth` state machine (~120 LoC + ~150 LoC tests).
- D4 — active-HC task + `envoy-health` crate/scheduler (~260 LoC + ~200 LoC tests).
- D5 — LB integration (pick filters unhealthy + panic threshold) (~90 LoC + ~130 LoC tests).
- D6 — stats wiring (3 counters + 1 gauge) + BEHAVIOR_CONTRACT rows (~70 LoC + ~70 LoC tests).
- D7.1 — fixture 0019 + synthetic-backend harness + settle-driver + Docker wrapper (~120 LoC YAML/wrapper + ~150 LoC harness + ~70 LoC backend).
- D7.2 — fuzz seed (~30 LoC + 2 file edits).
- D7.3 — in-process backstop (~210 LoC).
- State-4 verification + STATE-advance (~docs).

**SPEC-time projection: ~14–16 tasks; ~1800–2200 LoC** (production ~850, tests ~900, fixture/harness/backend ~470). **This is materially OVER the §6.1 ~1500-LoC gate** (task count is under ~25). **Recommended posture: SPLIT into `12.1` + `12.2`** at state-2 PLAN-write per §6.2:

- **`12.1` — config + health-state + LB-integration foundation (no new fixture).** D1 (schema) + D2 (validator) + D3 (`EndpointHealth` state machine) + D5 (LB integration: `pick()` filters unhealthy + panic threshold) + D6 (stats). **No new differential fixture** — regression-equivalence proven via the 18 existing fixtures staying green (the machinery is inert when unconfigured per §5.4), exactly the 05.1 / 07.1 "foundation slice, no new fixture" pattern. ~900–1000 LoC. The health state is wired + LB-integrated + stat-registered but nothing yet DRIVES probes (so all endpoints stay at their initial state) — the slice lands the seam.
- **`12.2` — active-HC task + fixture + parent-12 close.** D4 (the `envoy-health` crate + the periodic-probe task) + D7.1 (fixture 0019 + synthetic backend + settle-driver) + D7.2 (fuzz seed) + D7.3 (in-process backstop) + the §6.2 contract reconciliation + parent-12 close. ~1000–1200 LoC. This is where observable behavior + the differential fixture + the I2 down-payment land.

**Split ADR:** if the split fires, an ADR (`ADR-0036`) lands at the state-2 PLAN-write commit per `BOOTSTRAP_PROMPT.md` §6.2 step 6 ("ADR explaining the split"), mirroring ADR-0013/0017/0020/0022/0029/0030/0032. The parent-12 ROADMAP row flips `planned → in-progress` with `sub-phases: 12.1, 12.2`; each sub-phase gets its own row (`planned`) + its own `SPEC.md`.

**If the state-2 LoC estimate comes in at-or-under ~1500** (e.g. the PLAN-writer finds a leaner harness path), single-phase is permitted — but the SPEC-time projection strongly expects the split. **Recommended: split.**

### 6.2 Empirical verification at state-2 PLAN-write (HEAVY for this phase)

Per the phase-10/11-ratified verify-at-PLAN-write process improvement: **the state-2 PLAN-writer empirically verifies the upstream wire/behavior shapes BEFORE locking PLAN lock-ins.** Phase 12 has an unusually LARGE empirically-discoverable surface — run `envoyproxy/envoy:v1.33.0` Docker with an active-HC cluster + a synthetic backend + admin `/stats`, and verify:

1. **Initial endpoint health state** when active HC is configured (healthy-until-first-fail vs unhealthy/pending-until-first-pass), and the convergence behavior. **The single most load-bearing item** — it determines the fixture's settle/assert logic. (D3.)
2. **No-healthy-upstream synth response** body bytes (hex-dump) + status (503) + any wire-observable response flag. Compare to envoy-rust's existing synth-503 body; reconcile per §2.2 / D6.2. (Phase-10/11 byte-precision lesson.)
3. **Panic threshold**: the default value (50%?), the comparison semantics (strictly-below vs at-or-below), and the exact `common_lb_config.healthy_panic_threshold` config shape (`{value: 0}` Percent). (D1/D5.)
4. **Health-check stat names** present under `cluster.<name>.health_check.*` + the `membership_healthy` gauge name + semantics. (D6 / §2.1.)
5. **HTTP probe shape**: method (GET?), the `:authority`/`Host` Envoy sends (cluster-name default?), the `expected_statuses` default (exactly 200, or 200–299?). (D4.)
6. **Duration config shape**: confirm Envoy accepts the string durations `parse_duration` handles (`1s`/`0.5s`/`250ms`) for `interval`/`timeout`, vs requiring `{seconds, nanos}`. (D1.)

Each finding lands as a PLAN lock-in. **If any finding differs materially from the SPEC projection, the lock-in records the divergence + the SPEC §X.Y revision via an inline ADR at the state-2 PLAN-write commit** (mirrors the phase-10 ADR-0034 / phase-11-verification precedent). Given the split is also projected (§6.1), the empirical-revision ADR (if it fires) would be `ADR-0037` (the split ADR takes `ADR-0036`); at most one ADR per commit per D-3.5 — if both fire at the PLAN-write commit, they take consecutive numbers across the lock-in narrative. **Recommended: verify all 6; land the split ADR (`ADR-0036`) + any empirical-revision ADR (`ADR-0037`) at state-2.**

### 6.3 The 06.3 REVIEW I2 down-payment (NOT full closure)

D7.1 lands the first synthetic-backend harness primitive (the infrastructure 06.3 REVIEW I2 named). The PROGRESS narrative attributes the down-payment + explicitly states that I2's residual (per-class counter wire coverage + `upstream_cx_total` `value-exact` tightening via connection pooling) remains open with connection pooling as the named full-closure site. **Do NOT over-claim full I2 closure** (D-3.4 honesty). The PLAN-writer reads the 06.3 REVIEW.md §3 I2 entry + §8 R-track item 4 via direct spot-check before writing D7.1.

### 6.4 In-process backstop assertions (heeds the phase-10 M1 / phase-11 §6.4 lesson)

D7.3 SHOULD exercise BOTH convergence directions (healthy → 200, unhealthy → 503) AND include the per-probe standard-header presence assertion on the 503 probe, OR explicitly disclose any omission in PROGRESS. Recommended: include it.

### 6.5 The 06.x stats convention

StatsRegistry registration at cluster-construction / task-spawn time; per-cluster ownership of the Counter/Gauge handles; the `membership_healthy` gauge updated inline at the state-transition site (one source of truth, NOT polled — the 08.2 `server.live` pattern). Namespace `cluster.<name>.health_check.*` (§6.2-verified).

### 6.6 The BEHAVIOR_CONTRACT extension cadence

Contract extensions land at the TASK where each is first empirically exercised, NOT at PLAN-write and NOT at SPEC time. Stat rows at D6; the no-healthy-body reconciliation at the fixture task (D6.2).

### 6.7 Pre-state-4 fmt discipline (continues per 06.1 R-9)

Per-task PROGRESS sections quote `cargo fmt --all -- --check` at every PROGRESS-task close, NOT just at state-4.

### 6.8 State-4 evidence-discipline (continues per 05.3 → … → 11 chain)

Per-gate quoted evidence in PROGRESS at the state-4 verification task: real CI run URL + HEAD SHA + completion timestamp + per-gate quoted output (5 stable-toolchain gates + each Docker-gated fixture + h2spec_pass_rate_gate + parse_bootstrap fuzz iteration count). Phase 12 does NOT touch the H2 framing path — the state-4 verification confirms h2spec ≥95% held (no regression).

### 6.9 Cargo.lock cadence

The phase-04.1 REVIEW M5/M9 Cargo.lock-cadence ADR carries forward. Phase 12 adds zero new top-level Cargo deps; a new workspace-internal `envoy-health` crate (if option A) changes `Cargo.lock` only by adding the internal path-dep — no external-advisory surface change.

### 6.10 PLAN.md + PROGRESS.md skeleton + Task 1 preamble land alongside at state-2

Per the 06.2 / … / 11 cadence. State-2 PLAN-write lands `PLAN.md` (or, on split, the `12.1` PLAN) + `PROGRESS.md` skeleton + Task 1 preamble in a single standalone pre-Task-1 commit.

### 6.11 Subagent-driven execution at state 3 (per `feedback_execution_style`)

The user's standing preference auto-memory `feedback_execution_style` ("default to subagent-driven-development; skip the two-option fork") applies at state 3. The state-2 PLAN-write organizes tasks for subagent-driven execution per the 06.x / … / 11 cadence (each task independent enough to dispatch in isolation; PROGRESS attestation per-task; in-phase recovery cadence if any task surfaces a code-quality-review-blocking finding). Subagents claiming "same pattern as previous phase" verify the precedent shape via direct code-spot-check before the claim lands in PROGRESS.

---

## 7. ADR projection

**Recommended posture at state-1: NO new ADRs land at THIS (state-1 brainstorm) commit.** The DECISIONS.md ledger head stays at **ADR-0035** through phase 12's state-1; the next-available number is **ADR-0036**.

Conditional ADR slots, reserved for state-2 / state-3 landing:

- **Conditional ADR-0036 (option A — phase split). LIKELY TO FIRE.** Per §6.1, the LoC projection (~1800–2200) is over the §6.1 gate; the state-2 PLAN-write is expected to split phase 12 into `12.1` + `12.2`, landing `ADR-0036: split phase 12 into 12.1–12.2 because plan exceeded ~1500 LoC` per `BOOTSTRAP_PROMPT.md` §6.2 step 6. **Recommended posture: split; land ADR-0036 at the PLAN-write commit.**

- **Conditional ADR-0037 (option B — §6.2 empirical-verification revision). PLAUSIBLE.** If any of the 6 §6.2 items (esp. the initial-health-state semantics or the no-healthy-upstream body bytes) differs materially from this SPEC's projection, an inline ADR lands at the state-2 PLAN-write commit recording the divergence + the SPEC revision. Numbered `ADR-0037` if the split ADR took `ADR-0036`. **Recommended posture: verify all 6; land the revision ADR if any differ.**

- **Conditional ADR (option C — `envoy-health` crate cycle-resolution).** Only if the PLAN-writer judges the new-crate / cycle-avoidance choice (§5.1) warrants append-only recording (mirroring ADR-0028/0031). **Recommended posture: NO ADR** — a new crate forming a clean DAG is ordinary structure (like `envoy-stats`/`envoy-accesslog`/`envoy-filter`, none of which needed a creation ADR); record the choice as a PLAN lock-in, not an ADR. An ADR lands only if the chosen resolution is non-obvious.

- **Conditional ADR (option D — foundations grant).** No external-crate grant projected (§5.2/§5.3). If state-3 surfaces a genuine need, an ADR lands at the surfacing task. **Recommended: no grant.**

At most ONE ADR lands per commit (per D-3.5 sequential numbering). If none fire, the ledger stays at ADR-0035 through phase 12 (unlikely, given the split projection).

---

## 8. State-machine signposts for the phase-12 state-2 session

The next session (state 2) reads this section and acts.

- **Lifecycle state at session start:** State 2 (SPEC.md exists; PLAN.md does not).
- **Skill:** `superpowers:writing-plans` per `BOOTSTRAP_PROMPT.md` §5 state 2.
- **Output:** `PLAN.md` + `PROGRESS.md` skeleton + Task 1 preamble (standalone pre-Task-1 commit per the 04.3 / 05.1 / 06.x / 07.x / 08.x / 09 / 10 / 11 cadence). **If the split fires (§6.1, recommended): create `docs/envoy-rust/phases/12.1-<subtitle>/SPEC.md` + `12.2-<subtitle>/SPEC.md`, update ROADMAP (parent 12 → `in-progress` + `sub-phases: 12.1, 12.2`; two new sub-phase rows `planned`), update STATE → `12.1`, land `ADR-0036` (split), and STOP** — the next session starts `12.1` at state 1/2 per §6.2 step 7.
- **Empirical verification at state 2 (per §6.2 — HEAVY):** verify all 6 items against `envoyproxy/envoy:v1.33.0` before locking. Land any empirical-revision ADR inline.
- **Split-gate evaluation:** §6.1 above. **Recommended: SPLIT into 12.1 + 12.2.**
- **The 06.3 REVIEW I2 down-payment (D7.1):** §6.3 above — down-payment, NOT full closure; read the 06.3 REVIEW.md §3 I2 + §8 R-track item 4 via direct spot-check.
- **The dependency-cycle decision (§5.1 / D4):** recommended new `envoy-health` crate (option A); record as a PLAN lock-in.
- **PLAN-time SPEC corrections:** the PLAN-writer reads this SPEC against HEAD `<state-1-commit-SHA>` and flags any drift (the exact `Cluster` struct fields + line, the exact `pick()` signature at `cluster.rs:129`, the exact `ClusterHandle::pick_endpoint` seam at `:152`, the exact `RouterError::NoHealthyEndpoint` + the synth-503 arm at `hcm.rs:580-582`, the exact `parse_duration`/`Int64Range` signatures, the exact `envoy-http1::Client` probe API, the existing echo-server helper locations). Per the 06.2 → … → 11 "N PLAN-write SPEC corrections" pattern, corrections land in the PROGRESS Task 1 preamble.

---

## 9. Commit message format (for state 6 of the phase-12 lifecycle)

If phase 12 stays single (NOT recommended; see §6.1):

```
phase 12: active HTTP health checking + endpoint ejection + fixture 0019 [ADR-0036, ...]

<1-3 sentence summary>

Differential surface: fixture 0019-upstream-active-health-check; all 19 Docker-gated fixtures (0001-0019) green simultaneously at CI run <ID> HEAD <SHA>.
Conformance: h2spec ≥95% gate held at parent-05 baseline (H2 framing path untouched).
```

If phase 12 splits (recommended), the closing sub-phase (`12.2`) commit carries `[parent 12 done]` per the 07.2 / 08.2 closing-sub-phase precedent, and each sub-phase commits per the §5.3 format with its own ADR bracket as applicable.

---

## 10. State-machine commit (this commit — phase-12 state-1 close-out)

This SPEC is the state-1 output. The state-1 close-out commit is **docs-only** and touches:

- **CREATE** `docs/envoy-rust/phases/12-upstream-active-health-check/SPEC.md` (this file).
- **MODIFY** `docs/envoy-rust/ROADMAP.md` — **adds a table + first concrete row** beneath the existing "Upstream robustness family" §9 heading. Row content:
  ```
  | 12 | active HTTP health checking (cluster health_checks) + endpoint ejection + fixture 0019 (engages 06.3 REVIEW I2 synthetic-backend) | 02 04 | planned | — | fixture 0019-upstream-active-health-check green; envoy-cluster gains per-endpoint EndpointHealth state machine + pick() unhealthy-exclusion + panic-threshold honoring; new active HTTP health-check task (recommended new envoy-health crate; reuses envoy-http1::Client) probes Cluster.health_checks per interval and drives healthy/unhealthy thresholds; envoy-config gains HealthCheck + HttpHealthCheck + CommonLbConfig + Percent schema + ~4-6 ConfigError variants (TCP/gRPC/custom + multiple health checks rejected per minimum-viable scope); cluster.<name>.health_check.* + membership_healthy stats; first synthetic-backend harness primitive (06.3 REVIEW I2 down-payment; full I2 closure remains with connection pooling); LIKELY SPLIT into 12.1 + 12.2 at state-2 PLAN-write per ~1800-2200 LoC estimate |
  ```
  The "Upstream robustness family" heading + its descriptive line stay unchanged; the table + row join beneath them per `BOOTSTRAP_PROMPT.md` §4.1 invariant 2 (append-only; never delete rows). All other ROADMAP rows + family headings untouched; the schema header untouched.
- **MODIFY** `docs/envoy-rust/STATE.md` — advances "Active phase" pointer from `_none_ — awaiting next planning` to:
  - `id: 12`
  - `slug: 12-upstream-active-health-check`
  - `directory: docs/envoy-rust/phases/12-upstream-active-health-check/`
  - `status: phase 12 lifecycle state 1-complete / state-2-next (SPEC.md landed; PLAN.md does not exist; SPLIT into 12.1 + 12.2 projected at state-2)`

  Rewrites "Next expected skill" to `superpowers:writing-plans` scoped to this SPEC. Rewrites "Last commit" + "Last updated". Appends a new "Phase-12 state-1 brainstorm" subsection in Notes recording the family-pick + feature-pick rationale + alternatives along the 5-dimension scoring + the split projection + the I2 down-payment projection + the ADR projection. Preserves all prior subsections verbatim per D-3.5 (append-only) + D-3.4 (context isolation) — including the `### Phase-11 rollovers` + all `### Phase-11 state-*` subsections.

No code changes, no fixture changes, no Cargo.toml changes, no DECISIONS.md changes (ledger head stays **ADR-0035**), no BEHAVIOR_CONTRACT.md changes. ENVOY_TARGET.md + rust-toolchain.toml untouched (D-3.7 / D-3.9 unchanged).

**Commit message:**

```
phase 12: state-1 brainstorm — upstream-active-health-check SPEC.md (Upstream robustness family first phase; active HTTP health checking; engages 06.3 REVIEW I2)
```

Per the project precedent (phase-11 state-1 brainstorm commit `1370aaa` title shape — descriptive title with a parenthesized scope summary). No `[ADR-NNNN]` brackets — no ADR lands at this commit.

**Predecessor:** `7e27c1e` — phase 11 state-6 close-out (the most-recent commit; docs-only standalone-phase close-out).

**Origin/main:** `7e27c1e`. Local + origin are in sync as of THIS state-1 brainstorm commit's prologue. After landing, the docs-only edits push to origin and the next CI run re-validates the docs-only edits compile cleanly through the 5 stable-toolchain gates + the parse_bootstrap fuzz target on the unchanged 18-seed corpus (predecessor docs-only CI runs took ~2-3m).

---

*End of SPEC. Phase 12 state-1 lifecycle complete on landing. The next session enters state 2 — writes PLAN.md per `superpowers:writing-plans`, performs the §6.2 empirical verification at PLAN-write (active-HC cluster vs `envoyproxy/envoy:v1.33.0`: initial-health-state + no-healthy-body + panic-threshold + stat names + probe shape + duration shape), and evaluates the §6.1 split gate (SPLIT into 12.1 + 12.2 recommended; land ADR-0036).*
