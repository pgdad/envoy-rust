# Phase 12.2 (`12.2-active-http-probe-and-fixture`) — SPEC

- **Phase id:** `12.2`
- **Slug:** `12.2-active-http-probe-and-fixture`
- **Parent:** `12` (`12-upstream-active-health-check`). The **second + closing sub-phase** of the phase-12 split (parent-12 state-2 PLAN-write; ADR-0036). 12.2's state-6 commit flips parent ROADMAP row `12` `in-progress → done` per the closing-sub-phase invariant (the 07.2 / 08.2 precedent). The full feature narrative lives in `docs/envoy-rust/phases/12-upstream-active-health-check/SPEC.md`; the foundation slice it builds on lives in `docs/envoy-rust/phases/12.1-endpoint-health-and-lb-integration/SPEC.md`.
- **depends-on:** `12.1` (strict). 12.2's probe task mutates the `EndpointHealth` state machine + drives the `pick()` unhealthy-exclusion seam that 12.1 landed.
- **Status before this SPEC lands:** `planned` (added as a sub-phase row at the parent-12 state-2 split commit).

---

## 1. Goal and acceptance signal

Phase 12.2 lands the **observable behavior + the differential fixture** of active HTTP health checking — the slice 12.1's foundation made possible. After 12.2:

- A new **`envoy-health`** workspace crate owns a background tokio task, spawned per (cluster, endpoint) when `Cluster.health_checks` is configured, that loops every `interval`, issues an HTTP `GET <path>` probe (reusing `envoy-http1::Client`), evaluates the response status against `expected_statuses`, and calls `EndpointHealth::record_success`/`record_failure` (the 12.1 state machine). The 3 `cluster.<name>.health_check.{attempt,success,failure}` counters increment in the task.
- envoy-rust's no-healthy-upstream synth-503 emits the **`no healthy upstream`** body (19 bytes) per **ADR-0037** (reconciling the empty-body divergence the parent §6.2 verification found).
- A new differential fixture **`0019-upstream-active-health-check`** + the project's first **synthetic health-aware backend harness primitive** (the 06.3 REVIEW I2 down-payment) + a **settle-then-probe** driver bilaterally assert that both proxies, after a settle window, eject an unhealthy endpoint and return the synth-503 `no healthy upstream` to a data-plane request.
- An in-process backstop exercises both convergence directions (healthy → 200 through; unhealthy → 503 no-healthy) on the H1 path.

**12.2 engages and makes the first down-payment on carryforward 06.3 REVIEW I2** (the synthetic-backend harness infrastructure). **12.2 does NOT fully close I2** — I2's residual (per-class `downstream_rq_3xx/4xx/5xx` + `cluster.<name>.upstream_rq_5xx` wire coverage + the `cluster.<name>.upstream_cx_total` `value-exact` tightening) stays tied to connection pooling per the REVIEW §3 disposition. PROGRESS attributes the down-payment honestly (D-3.4); do NOT over-claim full closure.

**Acceptance signal (a)–(f), per `BOOTSTRAP_PROMPT.md` §7.5:**

- **(a)** Fixture `0019-upstream-active-health-check` green at Docker-gated CI.
- **(b)** All **18 pre-existing fixtures** (`0001`–`0018`) remain green simultaneously at the same CI run (19 total green). 12.1's machinery + 12.2's task stay inert when `health_checks` is unconfigured.
- **(c)** `h2spec` ≥95% (parent-05 baseline 99.31%). 12.2 touches no H2 codec/framing path; the no-healthy-body change is on the H1 synth-503 writer path. The state-4 verification re-confirms the gate held.
- **(d)** `parse_bootstrap` fuzz clean for the short-budget CI run on the 19-seed corpus (the health-check seed landed at 12.1; 12.2 adds no new corpus seed unless the fixture surfaces a new bootstrap shape worth seeding — PLAN-writer's call).
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check` all clean (the new `envoy-health` crate included).
- **(f)** `REVIEW.md` approved.

A **single CI run** lights gates (a)–(e) simultaneously. **12.2's state-6 commit closes parent-12** (flips ROADMAP row `12` → `done`).

---

## 2. Empirical findings inherited from the parent-12 state-2 verification (locked facts)

The parent-12 state-2 PLAN-write performed the parent SPEC §6.2 HEAVY 6-item verification against `envoyproxy/envoy:v1.33.0` (full table in STATE.md `### Phase-12 state-2 split decision`). The findings that bind **12.2**:

- **(item 2 — THE byte-precision item; ADR-0037)** **No-healthy-upstream synth response:** status **503**; body **`no healthy upstream`** = **19 bytes**, hex `6e 6f 20 68 65 61 6c 74 68 79 20 75 70 73 74 72 65 61 6d`, **NO trailing newline**; headers `{ content-length: 19, content-type: text/plain, date, server: envoy, connection: close }`; **NO wire-observable `x-envoy-*` response-flag header** (the `UH` flag is an access-log surface, not a data-plane response header). **This DIVERGES from envoy-rust's existing synth-503** (`crates/envoy-http1/src/hcm.rs:918` `synth_status` emits an EMPTY body, `content-length: 0`). **ADR-0037** reconciles: envoy-rust's health-driven no-healthy-upstream path emits the 19-byte `no healthy upstream` body. The fixture asserts the body byte-exact.
- **(item 1)** **Initial endpoint health state = Unhealthy/pending-until-first-pass** — drives the convergence direction in the fixture/backstop: a cluster with an unhealthy backend starts unhealthy and *stays* unhealthy (never passes a check) → converges to the synth-503; a healthy backend converges (after `healthy_threshold` passes) to 200-through. The settle window must exceed `interval × max(healthy_threshold, unhealthy_threshold) + timeout + margin`.
- **(item 5)** **HTTP probe shape:** `GET <path>`, `:authority`/`Host: <cluster-name>` (the default when `http_health_check.host` is unset), `user-agent: Envoy/HC`. Default `expected_statuses` = exactly 200. The probe is a fresh connection per probe (no `reuse_connection` at phase-12 scope). envoy-rust's probe (reusing `envoy-http1::Client`) sets `Host: <cluster-name>` unless `http_health_check.host` overrides. *(The probe `user-agent` string is NOT a differential-asserted surface — the fixture asserts the data-plane response, not the probe's wire bytes; envoy-rust may use its own UA. The synthetic backend keys only on path + method.)*
- **(item 6 — duration divergence)** **The fixture uses integer-second durations** (`interval: 1s`, `timeout: 1s`) on **both** proxy sides (identical YAMLs): upstream Envoy rejects `500ms` and envoy-rust's `parse_duration` rejects `0.5s`, so integer seconds is the only shared form. The parent SPEC's fixture sketch (`interval: 0.5s`) is corrected to `1s`. *(With `interval: 1s` + `unhealthy_threshold: 1` + `timeout: 1s`, the settle window is ≥ ~3 s + margin.)*

Items 3 (panic threshold) + 4 (stat names) bind 12.1 (the schema + LB-integration + stats); 12.2 inherits them via the 12.1 foundation and exercises `healthy_panic_threshold: { value: 0 }` (panic disabled) in the fixture so an all-unhealthy cluster reaches `pick() -> None` (rather than panic-routing to the unhealthy endpoint).

---

## 3. Deliverables

12.2 carries parent-12 deliverables **D4, D6.2 (the body reconciliation), D7**. The 12.2 state-2 PLAN-writer organizes these into TDD tasks for subagent-driven execution.

### D4 — Active HTTP health-check task + the `envoy-health` crate (the headline primitive)

The project's **first periodic timer-driven background task**. A new workspace crate **`crates/envoy-health/`** (parent SPEC §5.1 option A — the recommended cycle-free placement) added to the root `Cargo.toml` `members` list, depending on `envoy-cluster` (read endpoints + write `EndpointHealth`) + `envoy-http1` (the probe `Client`) + `envoy-config` + `envoy-stats` + `tokio`. This forms a clean DAG — `envoy-health → { envoy-http1 → envoy-cluster }` — with **no cycle** (because `envoy-http1` already depends on `envoy-cluster`, and `envoy-health` sits above both). `#![forbid(unsafe_code)]` per D-3.8. **No new top-level Cargo dep** (a workspace-internal crate is not a foundations grant; parent SPEC §5.3). A clean-DAG new crate needs **no ADR** (ordinary structure like `envoy-stats`/`envoy-accesslog`/`envoy-filter`); the PLAN-writer records the crate creation as a lock-in.

The task, per (cluster, endpoint):

- Loops every `interval` (parsed via `parse_duration`; integer-second form per §2 item-6).
- Issues `GET <http_health_check.path>` to the endpoint via `envoy-http1::Client::connect(...)` + `ClientStream::send_request(...)` (confirmed API at `crates/envoy-http1/src/client.rs`). Sets `Host: <http_health_check.host or cluster-name>` per §2 item-5.
- Applies the per-probe `timeout` (via `tokio::time::timeout` around the probe).
- **Success** iff the response status ∈ `expected_statuses` (default exactly 200; half-open `Int64Range`). Connection-failure / timeout / malformed-response counts as a **failure** (a `network_failure`-class result; phase-12 folds it into `failure` — the `network_failure` sub-counter defers per parent §4).
- Calls `EndpointHealth::record_success` / `record_failure` (12.1) → drives the Healthy/Unhealthy transition + the inline `membership_healthy` gauge update.
- Increments the `cluster.<name>.health_check.attempt` (every probe) / `.success` / `.failure` counters (the registration site per the 12.1 D6 lock-in — if 12.1 deferred the 3 counters to 12.2, they register + increment here).
- Tasks are tied to cluster/process lifetime and **cancelled cleanly on shutdown** (task handles held by a scheduler; `kill_on_drop`-equivalent cancellation). Tests assert no leaked task after shutdown (parent SPEC §5.5).

Wired at `envoy-bin` startup: after the `ClusterManager` is built, `envoy-health` spawns the probe tasks for every cluster carrying `health_checks`. **No async-runtime change** — reuses the existing tokio runtime. Unit/integration tests in `envoy-health` exercise: a success-sequence drives Unhealthy→Healthy after `healthy_threshold`; a failure-sequence drives Healthy→Unhealthy after `unhealthy_threshold`; a timeout counts as failure; clean cancellation.

### D6.2 — No-healthy-upstream synth-503 body reconciliation (ADR-0037)

Reconcile envoy-rust's no-healthy-upstream synth-503 to emit the **`no healthy upstream`** body (19 bytes) per **ADR-0037** + §2 item-2. The current `synth_status(503, close)` (`crates/envoy-http1/src/hcm.rs:918`) emits an **empty** body. The reconciliation must change the body **only on the no-healthy-upstream path** (`hcm.rs:582`, the `else` arm where `pick_endpoint()` returned `None`), NOT globally on `synth_status` (other 503/502 synth paths — e.g. the connect-fail 502 at `:524`/`:568` — keep their existing bodies). Recommended shape: a dedicated helper (e.g. `synth_no_healthy_upstream(close) -> Response`) that mirrors `synth_status` but sets `body: Bytes::from_static(b"no healthy upstream")` + `content-length: 19`, with the same 5-header set `{ server, date, content-length, content-type, connection }`. The PLAN-writer confirms `synth_status`'s exact construction + the writer-arm at `hcm.rs:582` and decides helper-vs-inline. Unit test asserts the 19-byte body + status 503 + the 5 headers.

> **Reachability note:** the no-healthy-upstream path is currently defense-in-depth (the validator rejects empty clusters, so phase-02 round-robin never returns `None`). 12.1 made `None` *reachable* for configured-HC clusters (all endpoints unhealthy + panic disabled). 12.2's task drives a cluster to all-unhealthy, and the fixture exercises the path bilaterally — so the body change is exercised by a real differential fixture (not an untested stub; satisfies `BOOTSTRAP_PROMPT.md` §6.3).

**BEHAVIOR_CONTRACT.md** — the `Response body` "no-healthy-upstream" disposition (the 19-byte `no healthy upstream` body, byte-exact) lands at the D6.2 task. If 12.1 deferred any `health_check.*` `Stat-name mapping` rows to 12.2 (per the 12.1 D6 lock-in), they land at the D4 task where the counters first increment.

### D7 — Fixture + synthetic-backend harness + Docker wrapper + in-process backstop

- **D7.1 — Fixture `tests/fixtures/0019-upstream-active-health-check/` + the synthetic health-aware backend harness primitive (the 06.3 REVIEW I2 down-payment).** The fixture configures a STRICT_DNS cluster with one endpoint pointing at a **synthetic backend** that returns a non-2xx status on the health-check path (`/healthz`) and 200 on the data path (`/`), an active HTTP health check (`path: /healthz`, `expected_statuses: [{start:200,end:201}]`, `healthy_threshold: 1`, `unhealthy_threshold: 1`, `interval: 1s`, `timeout: 1s` — integer-second durations per §2 item-6), and `common_lb_config.healthy_panic_threshold: { value: 0 }` (panic disabled). After a settle window (≥ `interval × unhealthy_threshold + timeout` + margin, ~3 s), both proxies eject the endpoint → a downstream `GET /` returns the **synth-503 `no healthy upstream`** (NOT the 200 the backend's data path serves absent ejection). The discriminating differential observable: **status 503 + body `no healthy upstream`** (byte-exact). `envoy.yaml` and `envoy-rust.yaml` are **identical** (integer-second durations + the schema both parsers accept) — no per-side divergence, no fixture-README ADR note.

  **The synthetic-backend harness primitive (the I2 down-payment):** a small test backend that serves a configurable status per path — 200 on `/`, a configured non-2xx (503) on `/healthz`. The PLAN-writer locates the existing echo-server helpers (`tests/helpers/http1-echo-server` + `tests/differential/src/backend.rs`) and decides extend-vs-new (recommended: a new health-aware backend helper, or a path-keyed extension of `backend.rs`, so the existing echo backends stay simple). This is the synthetic-backend infrastructure 06.3 REVIEW I2 named verbatim ("whichever phase first surfaces the synthetic backend … the upstream-robustness family").

  **The settle-then-probe driver:** the fixture needs a capability to wait past HC convergence, THEN drive the data-plane request + assert. Precedents: `Driver::AdminScrape`'s `within_ms` polling + the 08.2 drain fixture's connection-refused polling. Recommended: a new `Driver::Http1AfterSettle`-style variant (or a `settle_ms` field on a probe driver) that sleeps/polls past convergence, then drives one `Http1` request and asserts status + body. The PLAN-writer confirms the existing `Driver` internals (`tests/differential/src/lib.rs`) + picks the minimal extension. **Phase 12 does NOT opt into Timing tolerances (BEHAVIOR_CONTRACT.md §Timing)** — the assertion is the post-convergence STEADY STATE; the settle window is a harness mechanic, not a compared latency bound.

  Docker-gated wrapper at `tests/differential/tests/upstream_active_health_check.rs` mirroring the 10/11 `http_filter_*.rs` wrapper shape.

- **D7.3 — In-process backstop.** New file `crates/envoy-bin/tests/upstream_active_health_check.rs` mirroring the 10/11 backstop shape — **with the 09 REVIEW M3 subprocess discipline baked in** (`tokio::process::Command` + `.kill_on_drop(true)` + `Stdio::null()`/`piped()`; the standing pattern since phase 10). Boots `envoy-bin` with a synthesized bootstrap + an in-process synthetic backend; exercises BOTH directions: the **healthy** path (backend `/healthz` → 200 ⇒ after settle, `GET /` → 200 through to the backend) AND the **unhealthy** path (backend `/healthz` → 503 ⇒ after settle, `GET /` → 503 `no healthy upstream`), giving cheap H1 coverage of both convergence directions to complement the Docker differential fixture. Per the 10 REVIEW M1 / parent §6.4 lesson, **include the per-probe standard-header presence assertion on the 503 probe** (the 5 standard headers `{server, date, content-length, content-type, connection}`) OR explicitly disclose any omission in PROGRESS. Recommended: include it. *(The fuzz corpus seed landed at 12.1 D-corpus; 12.2 adds none unless a new bootstrap shape surfaces.)*

---

## 4. Out of scope for 12.2 (defers per parent SPEC §4)

All of the parent SPEC §4 deferral list: TCP/gRPC/custom checkers; multiple health checks per cluster; outlier detection; circuit breakers; retries + hedging; per-protocol connection pooling (the named full-closure site for 06.3 REVIEW I2's residual); `no_traffic_interval`/`unhealthy_interval`/`interval_jitter`/`initial_jitter`/`reuse_connection`; health-check event logging; degraded/excluded host states + `membership_degraded`/`membership_excluded`; the `lb_healthy_panic` + `health_check.{passive_failure,network_failure,verify_cluster,healthy,degraded}` counters; HC request headers / `service_name_matcher` / `codec_client_type`; active HC over an H2 upstream. **Full 06.3 REVIEW I2 closure** (per-class counter wire coverage + `upstream_cx_total` `value-exact`) remains with connection pooling — 12.2 makes the synthetic-backend down-payment only.

---

## 5. Architectural invariants (inherited from parent SPEC §5)

- **§5.1 the dependency-cycle constraint (the central architectural decision):** the `EndpointHealth` STATE lives in `envoy-cluster` (12.1); the health-check TASK lives in the new `envoy-health` crate (12.2), which depends on `envoy-cluster` + `envoy-http1` + `envoy-config` + `envoy-stats` + `tokio` — a clean DAG, no cycle (mirrors the ADR-0028/0031 cycle-resolution lineage; a clean-DAG new crate needs no ADR). The state-3 implementer verifies the dep graph stays acyclic (`cargo` rejects a cycle at build time).
- **§5.2 hand-rolled per D-3.2** (*"Active health checking … Must be written from scratch"*): the periodic-probe scheduler + the probe loop + the timeout handling are written from scratch atop tokio + `envoy-http1::Client`. No new top-level Cargo dep; no `rand` (no jitter at phase-12 scope).
- **§5.4 inert-when-unconfigured:** no `health_checks` ⇒ no probe task spawned ⇒ the 18 existing fixtures unchanged (acceptance gate (b)).
- **§5.5 the active-HC task is the first periodic-background primitive** — graceful cancellation on shutdown; tests assert no leaked task. This primitive is the foundation the rest of the upstream-robustness family (outlier-detection windows, circuit-breaker accounting) reuses.
- **§5.6 steady-state health decision** (cross-proxy deterministic in steady state; convergence WINDOW differs, converged STATE does not) + **§5.7 the pre-built `pick() -> Option` + no-healthy-503 seam** (12.2 reuses the seam; the only writer-arm change is the D6.2 body reconciliation).

---

## 6. Signposts for the 12.2 state-2 PLAN-writer

- The empirical §6.2 verification is **done** (parent-12 state-2; §2 above + STATE.md `### Phase-12 state-2 split decision` + ADR-0037 for the body bytes). **The 12.2 PLAN-writer does NOT re-run Docker** for the 6 items — they are locked facts. (A 12.2-specific implementation question — e.g. the exact `envoy-http1::Client` probe-request construction — is verified against the code at PLAN-write, not against Docker.)
- **PLAN-time SPEC corrections** (read this SPEC against HEAD): the `envoy-http1::Client` API (`client.rs`: `Client::connect` + `ClientStream::send_request`); `synth_status` at `hcm.rs:918` (empty body) + the no-healthy writer-arm at `hcm.rs:582`; the `Driver` internals at `tests/differential/src/lib.rs`; the echo-server helper locations (`tests/helpers/*-echo-server`, `tests/differential/src/backend.rs`); the 12.1-landed `EndpointHealth` API + the D6 stats-wiring lock-in (whether the 3 counters were registered at 12.1 or deferred to 12.2). Corrections land in the PROGRESS Task 1 preamble.
- **The 06.3 REVIEW I2 down-payment (D7.1):** read the `docs/envoy-rust/phases/06.3-stats-wiring-and-close/REVIEW.md` §3 I2 + §8 R-track item 4 via direct spot-check; attribute the down-payment honestly (NOT full closure — connection pooling owns the residual). *(Already spot-checked at parent-12 state-2: I2 deferred the synthetic-backend infra + the per-class counter wire coverage + `upstream_cx_total` value-exact tightening to "whichever phase first surfaces the synthetic backend … the upstream-robustness family"; 12.2 surfaces the synthetic backend = the down-payment; the counter/cx coverage stays with pooling.)*
- **Subagent-driven execution at state 3** per `feedback_execution_style`. Organize tasks: D4 envoy-health crate + probe task + scheduler + envoy-bin wiring → D4 stats increment (+ any deferred 12.1 counter registration + contract rows) → D6.2 no-healthy body reconciliation (ADR-0037) + BEHAVIOR_CONTRACT row → D7.1 synthetic backend helper + fixture 0019 + settle-driver + Docker wrapper → D7.3 in-process backstop (both directions; 503-probe header assertion) → state-4 verification (19 fixtures green + 5 gates + h2spec ≥95% + fuzz on the 19-seed corpus) → state-6 parent-12 close.
- **Carryforward:** 12.2 ENGAGES 06.3 REVIEW I2 (down-payment, NOT full closure). No other carryforward engaged (phase 12 touches no HTTP-filter file). The inherited inventory carries forward unchanged.

---

## 7. ADR projection for 12.2

**Recommended posture: NO new ADR lands during the 12.2 lifecycle.** ADR-0036 (split) + ADR-0037 (no-healthy-body empirical reconciliation) both landed at the parent-12 state-2 split commit (before 12.1/12.2 began). The `envoy-health` crate creation is recommended as a **PLAN lock-in, NOT an ADR** (a clean-DAG new crate is ordinary structure, like `envoy-stats`/`envoy-accesslog`/`envoy-filter`, none of which needed a creation ADR). No foundations grant projected (D-3.2 permits the hand-rolled task atop tokio + the existing crates). DECISIONS.md ledger head is **ADR-0037** at 12.2 start; next available **ADR-0038**. A 12.2 ADR lands only if state-3 surfaces a genuine ambiguity (e.g., a non-obvious cancellation-protocol decision warranting durable record, OR a new external-crate need — neither projected).

---

## 8. Commit message format (for state 6 of the 12.2 lifecycle — parent-12 close)

```
phase 12.2: active HTTP health-check probe task (envoy-health crate) + no-healthy-upstream body + fixture 0019 [parent 12 done] [ADR-0037]

<1-3 sentence summary; names the 06.3 REVIEW I2 synthetic-backend down-payment (NOT full closure)>

Differential surface: fixture 0019-upstream-active-health-check; all 19 Docker-gated fixtures (0001-0019) green simultaneously at CI run <ID> HEAD <SHA>.
Conformance: h2spec ≥95% gate held at parent-05 baseline (H2 framing path untouched).
```

*(The `[ADR-0037]` bracket appears because 12.2 IMPLEMENTS the ADR-0037 reconciliation; ADR-0037 itself landed at the parent-12 state-2 commit. The `[parent 12 done]` tag flips ROADMAP rows `12.2` AND `12` to `done` simultaneously — the closing-sub-phase invariant per the 07.2 / 08.2 precedent.)*

---

*End of 12.2 SPEC. 12.2 lands the observable behavior + the differential fixture + the I2 synthetic-backend down-payment, and closes parent-12 + the first Upstream-robustness-family phase.*
