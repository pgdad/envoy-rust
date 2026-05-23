# Phase 12.2 (`12.2-active-http-probe-and-fixture`) — PLAN

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development`
> per `feedback_execution_style` auto-memory and per the established 06.x / 07.x / 08.x /
> 09 / 10 / 11 / 12.1 cadence. Tasks 1-8 implement the phase per `SPEC.md`. Steps use `- [ ]`
> checkbox syntax for tracking.

**Goal.** Land the **observable behavior + the differential fixture** of active HTTP health
checking — the closing slice the 12.1 foundation made possible (parent-12 deliverables D4 /
D6.2 / D7 carved into 12.2 by ADR-0036): a new **`envoy-health`** workspace crate owns
periodic-background probe tasks (one per (cluster, endpoint) when `Cluster.health_checks` is
configured) that reuse `envoy-http1::Client` to probe `GET <http_health_check.path>` per
`interval`, evaluate the response status against `expected_statuses`, and drive the 12.1
`EndpointHealth` Healthy/Unhealthy transitions; the 3 `cluster.<name>.health_check.{attempt,
success,failure}` counters increment in the task; envoy-rust's no-healthy-upstream synth-503
emits the **`no healthy upstream`** body (19 bytes per ADR-0037) ONLY on the `hcm.rs:582`
arm; a new differential fixture **`0019-upstream-active-health-check`** + the project's
first synthetic health-aware backend harness primitive (the 06.3 REVIEW I2 **down-payment**,
NOT full closure) + a settle-then-probe driver bilaterally assert post-convergence steady
state on an H1 listener; an in-process backstop exercises BOTH convergence directions
(healthy → 200 through; unhealthy → 503 `no healthy upstream`). The state-6 commit **closes
parent-12** (flips ROADMAP row `12` `in-progress → done` — the 07.2 / 08.2 closing-sub-phase
precedent).

**Architecture.** The probe TASK lives in a new `crates/envoy-health/` crate (parent SPEC
§5.1 option A — recommended cycle-free placement; clean DAG over `envoy-cluster +
envoy-http1 + envoy-config + envoy-stats + tokio`; no cycle, because `envoy-http1` already
depends on `envoy-cluster` and `envoy-health` sits above both). The crate hosts the
project's FIRST periodic-background primitive — `tokio::spawn`ed task per (cluster,
endpoint) with `tokio::time::interval(interval)` + `tokio::time::timeout(timeout, ...)` per
probe + a `CancellationToken`-style shutdown handle. The new `envoy-health::Scheduler`
(returned by `Scheduler::spawn(&bootstrap, cluster_mgr, registry)`) is wired into `envoy-bin`
startup right after `cluster_mgr = from_bootstrap(...)`. The `ClusterHandle` accessor surface
widens minimally so `envoy-health` can iterate (`SocketAddr`, `Arc<EndpointHealth>`) pairs
for each cluster that configured active HC; the bootstrap's `HealthCheck` + `HttpHealthCheck`
config is read directly from `&bootstrap` (not duplicated onto the runtime `Cluster`) — keeps
`envoy-cluster` as the structural seam and isolates HC-config interpretation in `envoy-health`.

The no-healthy-body reconciliation (ADR-0037) adds a NEW helper
`synth_no_healthy_upstream(close) -> Response` adjacent to `synth_status` in
`crates/envoy-http1/src/hcm.rs`. It mirrors `synth_status`'s 5-standard-header shape but
substitutes `body: Bytes::from_static(b"no healthy upstream")` + `content-length: "19"`. The
**single** call-site change is at `hcm.rs:582` (currently `outgoing = synth_status(503,
close);`) → `outgoing = synth_no_healthy_upstream(close);`. **Global `synth_status` at
`hcm.rs:918` is NOT touched** — the connect-fail 502 path at `:524` + the send-fail 502 path
at `~:570` keep their existing empty bodies. The reconciliation surface is exactly one
writer-arm + one new helper.

Fixture `0019-upstream-active-health-check` configures an H1 listener (HCM `codec_type:
HTTP1`) + a STRICT_DNS cluster with one endpoint pointing at a synthetic health-aware
backend (serves 200 on `/`, 503 on `/healthz`) + an HTTP health check (`path: /healthz`,
`expected_statuses: [{start: 200, end: 201}]`, `healthy_threshold: 1`, `unhealthy_threshold:
1`, `interval: 1s`, `timeout: 1s` — integer-second durations per §6.2 item-6) +
`common_lb_config.healthy_panic_threshold: { value: 0 }` (panic disabled so 0-healthy → no
panic-route → `pick() -> None` → the new `synth_no_healthy_upstream` body). After a settle
window (~3s + margin), both proxies eject the endpoint → downstream `GET /` returns **status
503 + body `no healthy upstream`** (NOT the 200 the backend's data path serves absent
ejection). The discriminating differential observable is the **status 503 + 19-byte body
byte-exact**. The harness gains a new `Driver::Http1AfterSettle` variant (Driver enum
extension; mirrors `Driver::Http1` + adds a `settle_ms: u64` field) that sleeps
`settle_ms` then drives one `Http1` request and applies the existing 5-axis equivalence
cascade. The synthetic-backend harness primitive lands as a new
`tests/differential/src/backend.rs` `HealthAwareHttp1Backend` (extends the existing pattern;
serves a configurable per-path status; runs in a Docker container alongside the proxy via
the existing testcontainers bridge-network plumbing).

The in-process backstop at `crates/envoy-bin/tests/upstream_active_health_check.rs` mirrors
the phase-10/11 backstop shape (subprocess discipline: `tokio::process::Command` +
`kill_on_drop(true)` + `Stdio::null()`/`piped()` per 09 REVIEW M3) and exercises BOTH
convergence directions — the healthy path (in-process backend `/healthz` → 200 ⇒ after
settle, `GET /` → 200 through to the backend body) AND the unhealthy path (backend
`/healthz` → 503 ⇒ after settle, `GET /` → 503 `no healthy upstream` + 5 standard
HTTP/1.1 response headers per the 10 REVIEW M1 lesson).

**Tech Stack.** Zero new top-level Cargo deps (D-3.2 + D-3.5: the recommended `envoy-health`
crate uses ONLY already-permitted foundations — `tokio` + `bytes` + `envoy-cluster` +
`envoy-http1` + `envoy-config` + `envoy-stats` + `tracing` + `thiserror`; no `rand` — no
jitter at phase-12 scope per parent §4). One new workspace-internal crate (`envoy-health`)
added to root `Cargo.toml` `members` — ordinary structure per the precedent of
`envoy-stats`/`envoy-accesslog`/`envoy-filter` (no creation ADR). No `unsafe` (every crate
root keeps `#![forbid(unsafe_code)]` per D-3.8). No new path-dep cycle (verified at PLAN-write
against HEAD `67ae869`: `envoy-cluster` depends on `envoy-config` + `envoy-stats` + `tokio`
[net, rt, macros]; `envoy-http1` depends on `envoy-cluster` + others; `envoy-health` sits
ABOVE both — clean DAG). No H2 framing-path touch (h2spec ≥95% holds vacuously at the
parent-05 baseline 99.31% — the fixture runs on an H1 listener; the backstop is H1; matches
the 12.1/phase-11 H1 backstop discipline). The `parse_bootstrap` fuzz corpus extends 19 → 20
with one new seed exercising the fixture-0019 bootstrap shape (HCM + router + HC-configured
cluster + panic-disabled + route gating — a different combination than 12.1's
header-only-HC seed).

---

## 0. Architecture lock-ins

These decisions are settled at PLAN-write; subagents implement them as written and do NOT
re-litigate. Numbered for cross-reference from PROGRESS.

1. **No nest-split, no nest-SPLIT.** 12.2 is projected at ~1000-1200 LoC (production ~600
   `envoy-health` + ~30 `hcm.rs:582` arm + new helper + ~60 counter wiring; tests ~250 unit
   + ~150 integration; fixture/harness/backend ~150) — well under the `BOOTSTRAP_PROMPT.md`
   §6.1 ~1500-LoC / ~25-task gate. The parent-12 split (ADR-0036) already absorbed the
   over-gate scope into 12.1 + 12.2; 12.2 does NOT nest-split. 8 tasks (~10-12 LoC/task
   average through tasks 7+8 for docs-only / fuzz; ~120-180 LoC/task average for tasks 1-6).

2. **No ADR lands in the 12.2 lifecycle** (SPEC §7). DECISIONS.md ledger head is **ADR-0037**
   at 12.2 start; next available **ADR-0038**. The `envoy-health` crate creation is ordinary
   structure (clean DAG; matches `envoy-stats`/`envoy-accesslog`/`envoy-filter` precedent;
   no creation ADR per the parent-12 state-1 brainstorm + state-2 split lock-in). No
   foundations grant projected (D-3.2 permits the hand-rolled task atop tokio + the existing
   crates). No wire-level contract revision needed at the PLAN-write (ADR-0037 already
   landed at the parent-12 state-2 split + is IMPLEMENTED at D6.2 here, not re-decided). A
   12.2 ADR lands ONLY if state-3 surfaces a genuine unforeseen ambiguity (unlikely).

3. **The §6.2 empirical verification is DONE** (parent-12 state-2 commit `4f9ba04`;
   findings in SPEC §2 + STATE.md `### Phase-12 state-2 split decision` + ADR-0037). **Do
   NOT re-run Docker.** The locked facts 12.2 bakes in: initial endpoint state Unhealthy
   already landed at 12.1 D3 (item 1); no-healthy-upstream body **`no healthy upstream` = 19
   bytes**, hex `6e 6f 20 68 65 61 6c 74 68 79 20 75 70 73 74 72 65 61 6d`, NO trailing
   newline (item 2 → D6.2 lands HERE per ADR-0037); panic-threshold already landed at 12.1
   D5 (item 3); stat names `cluster.<name>.health_check.{attempt,success,failure}` (item 4
   → D7 lands HERE; the `membership_healthy` gauge already landed at 12.1 D6); HTTP probe
   shape `GET /healthz` + `:authority`/`Host: <cluster-name>` default + default
   `expected_statuses` = exactly 200 + half-open `Int64Range` (item 5 → D4 drives HERE);
   **integer-second durations only** (item 6 — the fixture uses `1s`/`1s`).

4. **The new `envoy-health` crate (recommended option A) lands at Task 1 with the
   following minimum-viable shape:**
   - **Path:** `crates/envoy-health/`. New workspace member added to root `Cargo.toml`
     `members` list (alphabetical adjacent to `envoy-filter`).
   - **Deps:** `envoy-cluster = { path = "../envoy-cluster" }`, `envoy-http1 = { path =
     "../envoy-http1" }`, `envoy-config = { path = "../envoy-config" }`, `envoy-stats = {
     path = "../envoy-stats" }`, `tokio = { version = "1", features = ["rt", "macros",
     "time", "sync"] }`, `bytes = "1"`, `tracing = "0.1"`, `thiserror = "2"`. **No new
     top-level Cargo dep.** Dev-deps: `tokio = { features = ["rt-multi-thread"] }` +
     `tokio-util` (already in workspace) for `CancellationToken` if used.
   - **Module layout:** `src/lib.rs` (`#![forbid(unsafe_code)]` + `pub use scheduler::*;`
     + `pub use probe::*;`), `src/scheduler.rs` (the `Scheduler` + `spawn(&bootstrap,
     cluster_mgr, registry)` entrypoint that walks the bootstrap clusters, registers the 3
     counters per configured-HC cluster, builds one probe task per (cluster, endpoint), and
     returns a handle to the running tasks), `src/probe.rs` (the per-(cluster, endpoint)
     `probe_loop` async fn — the periodic primitive).
   - **Clean DAG verified:** `envoy-health → envoy-http1 → envoy-cluster` + `envoy-health →
     envoy-cluster` (no cycle); `cargo build --workspace` is the build-time enforcement.

5. **The `ClusterHandle` accessor surface widens minimally at Task 1** to expose the
   (`endpoints`, `endpoint_health`) pairs `envoy-health` needs. Recommended new method:

   ```rust
   /// 12.2: per-endpoint health-probe targets when this cluster configures
   /// active health checks. Yields `(endpoint addr, EndpointHealth handle)`
   /// pairs that the `envoy-health` probe task drives. Returns `None` when
   /// no `health_checks` configured (the §5.4 inert-when-unconfigured
   /// invariant — no probe task should spawn for the cluster).
   pub fn health_probe_targets(&self) -> Option<Vec<(SocketAddr, Arc<EndpointHealth>)>>;
   ```

   Implementation: zip `self.inner.endpoints` with `self.inner.endpoint_health.as_ref()?`
   (the `as_ref` pre-flight returns `None` cleanly when no HC). Collects into a `Vec` so
   the caller does not need to hold an internal borrow. **NOT a public-API revision worth
   an ADR** — it is the minimum accessor needed by the new `envoy-health` consumer (the
   06.1 cluster-accessor cadence, where new consumers got new pub accessors at the
   surfacing site).

6. **The probe task topology preserves the M2 single-writer-per-endpoint contract**
   (12.1 REVIEW M2). The `Scheduler::spawn` builds EXACTLY ONE `tokio::spawn`-ed
   `probe_loop` per (cluster, endpoint) pair. The `Arc<EndpointHealth>` for an endpoint is
   moved into exactly that task; no other code path calls `record_success`/`record_failure`
   on it. The probe loop is the SOLE writer to that endpoint's `EndpointHealth`. **Task 1
   folds in a ~1-LoC API-boundary single-writer-contract comment on `EndpointHealth`** —
   appending a `/// SAFETY (12.2):` note adjacent to the existing doc comment in
   `crates/envoy-cluster/src/health.rs` stating the live-writer contract the production
   code now relies on. This closes the M2 forward-correctness verification at the API
   boundary; the 12.2 review verifies the topology against this contract.

7. **The M4 `membership_healthy` BEHAVIOR_CONTRACT row Equivalence-column fold-in lands at
   Task 2** (the D7 counter-wiring task, where the gauge becomes naturally driven by the
   live probe task — the natural revisit site per the 12.1 REVIEW §3 M4 disposition).
   Exactly one cell edit: `value-exact (steady state)` → `value-exact (12.2 steady state;
   reads 0 at 12.1)`. No other contract-row change at Task 2 beyond the 3 new counter rows.

8. **The 3 `cluster.<name>.health_check.{attempt,success,failure}` counters land at Task 2
   inside `envoy-health`** (not inside `envoy-cluster`) per the parent SPEC §6.2 item-4
   wiring + the 12.1 D6 lock-in deferral. The 3 counters are registered at
   `Scheduler::spawn` time (once per configured-HC cluster) via
   `StatsRegistry::register_counter("cluster.<name>.health_check.<kind>")` and the
   resulting `Arc<Counter>` handles are MOVED into the per-endpoint probe task (each task
   holds 3 `Arc<Counter>` clones — every probe `attempt.inc()`, then `success.inc()` or
   `failure.inc()`).

9. **The probe-shape contract** (§6.2 item-5): per-probe → `GET <http_health_check.path>` +
   `Host: <http_health_check.host or cluster-name>` (default cluster name unless override).
   The probe HTTP request is constructed via `envoy_http1::codec::Request {
       method: "GET", path: <hc.path>, host: <hc.host or cluster_name>, headers: vec![], body: None }`
   (the existing `Request` struct shape at `crates/envoy-http1/src/codec.rs`); the
   `user-agent` is unset (envoy-rust does not differentially assert the probe wire bytes —
   the synthetic backend keys on method + path only). The probe is a fresh `Client::connect`
   per probe (no `reuse_connection` at phase-12 scope per parent §4).

10. **The probe success criterion** (§6.2 item-5): success iff response status ∈
    `expected_statuses`. Decision algorithm: `expected_statuses` defaults to `vec![]` at
    parse time; the probe loop interprets empty as "exactly 200" (the upstream default).
    With any explicit `expected_statuses` entries (each a half-open `Int64Range { start,
    end }`), success iff `expected_statuses.iter().any(|r| (r.start..r.end).contains(&(status
    as i64)))`. Failure includes: status NOT in `expected_statuses`; connection failure
    (`Http1Error::UpstreamConnect`); read timeout (`tokio::time::timeout` `Err`); malformed
    response (any other `Http1Error`). All failure classes increment the `failure` counter
    + call `EndpointHealth::record_failure` (the network_failure sub-counter defers per
    parent §4).

11. **Per-probe timeout** (parent SPEC §D4): the probe `Client::connect` + `send_request`
    are wrapped in `tokio::time::timeout(hc.timeout_duration, async { ... })`. A timeout
    `Err` counts as failure (per lock-in #10). The default-30s `READ_TIMEOUT` baked into
    `crates/envoy-http1/src/client.rs:21` is the outer ceiling; the per-probe `timeout`
    config (parsed via `parse_duration`) is the inner bound.

12. **The periodic-loop primitive shape** (project's FIRST periodic-background task per
    parent SPEC §5.5):

    ```rust
    async fn probe_loop(
        addr: SocketAddr,
        host: String,
        path: String,
        timeout: Duration,
        interval: Duration,
        expected_statuses: Vec<Int64Range>,
        endpoint_health: Arc<EndpointHealth>,
        attempt: Arc<Counter>,
        success: Arc<Counter>,
        failure: Arc<Counter>,
        cancel: CancellationToken,
    ) {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = cancel.cancelled() => return,
                _ = ticker.tick() => {
                    attempt.inc();
                    match probe_once(addr, &host, &path, timeout, &expected_statuses).await {
                        Ok(()) => { success.inc(); endpoint_health.record_success(); }
                        Err(_) => { failure.inc(); endpoint_health.record_failure(); }
                    }
                }
            }
        }
    }
    ```

    `MissedTickBehavior::Delay` (the default) lets the loop catch up after backpressure
    without firing a burst; `tokio::select!` over the cancellation token + ticker is the
    standard cancellable-loop shape (matches the 08.2 listener-side `tokio::select!` over
    `cancel.cancelled()` + accept).

13. **The `Scheduler` shutdown contract** (parent SPEC §5.5): on `Scheduler::shutdown` the
    `CancellationToken` is cancelled, then ALL `JoinHandle`s are `await`-joined with a
    bounded budget (~1s). Tasks honor cancellation at the `tokio::select!` boundary; no
    background task outlives `Scheduler::shutdown`. Unit tests assert: (a) `Scheduler::spawn`
    spawns N tasks for N (cluster, endpoint) pairs; (b) `Scheduler::shutdown` returns after
    all tasks exit; (c) after shutdown, no `record_*` calls fire (the
    `Arc<EndpointHealth>::strong_count` returns to the expected baseline).

14. **The ADR-0037 implementation at D6.2 (Task 3) adds a new helper
    `synth_no_healthy_upstream(close: bool) -> Response`** adjacent to the existing
    `synth_status` at `crates/envoy-http1/src/hcm.rs` (~`:918`). The helper mirrors
    `synth_status` exactly EXCEPT the body + content-length:

    ```rust
    /// 12.2 (parent-12 D6.2 per ADR-0037): no-healthy-upstream synth-503
    /// response. Mirrors `synth_status` 5-header shape but emits the 19-byte
    /// `no healthy upstream` body (hex `6e 6f 20 68 65 61 6c 74 68 79 20 75
    /// 70 73 74 72 65 61 6d`; no trailing newline) matching upstream Envoy
    /// v1.33.0's no-healthy-upstream wire shape (§6.2 item-2, locked at
    /// parent-12 split `4f9ba04`). Used ONLY at the `pick() -> None` arm
    /// (`hcm.rs:582`); the connect-fail 502 and other synth paths keep
    /// `synth_status`'s empty body.
    fn synth_no_healthy_upstream(close: bool) -> Response {
        let body = Bytes::from_static(b"no healthy upstream");
        Response {
            status: 503,
            reason: None,
            headers: vec![
                (headers::SERVER.to_string(), DEFAULT_SERVER_NAME.to_string()),
                (headers::DATE.to_string(), now_imf_fixdate()),
                (headers::CONTENT_LENGTH.to_string(), body.len().to_string()),
                (headers::CONTENT_TYPE.to_string(), DEFAULT_CONTENT_TYPE.to_string()),
                (headers::CONNECTION.to_string(), connection_value(close).to_string()),
            ],
            body,
        }
    }
    ```

    `body.len()` evaluates to `19` at construction (compile-time-constant
    `Bytes::from_static` length); we use `body.len().to_string()` not `"19".to_string()`
    so future `b"..."` edits cannot drift the content-length out of sync. A unit test in
    `hcm.rs` `#[cfg(test)] mod tests` asserts: status 503; body bytes; the 5 standard
    headers with exact names + content-length `"19"`.

15. **The D6.2 call-site change is EXACTLY ONE line** at `crates/envoy-http1/src/hcm.rs:582`
    — `outgoing = synth_status(503, close);` → `outgoing = synth_no_healthy_upstream(close);`.
    The connect-fail 502 path at `~:524` (`outgoing = synth_status(502, close);`) +
    send-fail 502 path (`outgoing = synth_status(502, close);` in the `RouteAction::Route`
    error arm) KEEP `synth_status` (their empty-body shape is unchanged). The PLAN-writer
    confirmed via direct read of `hcm.rs:560-600`: only the no-healthy-upstream `else` arm
    of the `pick_endpoint().is_some()` match calls `synth_status(503, close)` — that is the
    sole site to change.

16. **The `Response body` BEHAVIOR_CONTRACT row for the no-healthy-upstream path lands at
    Task 3.** New disposition entry in the existing "Response body" matrix dimension (the
    `## Equivalence matrix` already covers `Response body` as "Byte-exact for deterministic
    handlers; semantically equal for filter-modified bodies"). The new authored entry
    extends the `## Stat-name mapping` section's `**12.1 entries (active health checking):**`
    block (now `**12.1/12.2 entries...**` or a new `**12.2 entries...**` block — see lock-in
    #18 for the contract-organization decision).

17. **Fixture 0019 lands at Task 5 on an H1 listener** (matching 12.1/phase-11's H1
    backstop discipline; H2 path untouched so h2spec ≥95% holds vacuously). Directory
    `tests/fixtures/0019-upstream-active-health-check/` contains 4 files mirroring the
    fixture-0008 H1 shape (`envoy.yaml`, `envoy-rust.yaml`, `expectations.yaml`, `README.md`)
    — no `inputs/` directory needed (the driver carries the request shape). The two YAMLs
    are **identical** modulo per-side surface (admin block + bind address + node
    block + `generate_request_id: false` on the upstream side; no admin + 127.0.0.1 +
    no generate_request_id on the envoy-rust side — the fixture-0008/0018 pattern). The
    fixture YAML duration shape uses INTEGER seconds (`1s`/`1s`) per §6.2 item-6.

18. **BEHAVIOR_CONTRACT organization at Tasks 2 + 3.** Two append edits:
    - **Task 2 — `## Stat-name mapping`** — append a new `**12.2 entries (active health checking
      counters):**` block with the 3 new counter rows; ALSO edit the existing 12.1 entry row
      `membership_healthy` Equivalence cell per lock-in #7 (the M4 fold-in). Adjacent block
      preserves the 12.1 entries block verbatim.
    - **Task 3 — a new `## Response body — no-healthy-upstream synth-503` subsection** appended
      after `## Equivalence matrix` (or extending the existing `## Equivalence matrix` with a
      footnote table). Per the 08.1/08.2 "## Admin endpoint body shapes" precedent, a discrete
      named subsection is cleaner than embedding in the matrix table. The subsection
      documents: status 503; body 19 bytes `no healthy upstream`; the 5 standard headers; the
      `pick() -> None` reachability path (the H1 `hcm.rs:582` arm); the helper name.

19. **The synthetic-backend harness primitive (Task 4)** lands as a new
    `tests/differential/src/backend.rs` `HealthAwareHttp1Backend` struct + `spawn` method.
    Shape mirrors the existing `Http1EchoBackend` (~`tests/differential/src/backend.rs:179`):
    constructs a testcontainers-based HTTP/1 backend that serves per-path responses, here
    extended to serve **200 on `/` + 503 on `/healthz`** (the discriminating health-check
    signal). The PLAN-writer locates the existing `Http1EchoBackend` implementation;
    `HealthAwareHttp1Backend` reuses the same container image / driver if the existing
    helper image supports per-path scripting, OR uses a new minimal image (the simplest path
    is a small ad-hoc helper backend container — recommended). **This is the 06.3 REVIEW I2
    DOWN-PAYMENT** (the synthetic-backend harness infrastructure the REVIEW named for the
    "upstream-robustness family" close site); 12.2 does NOT fully close I2 — the per-class
    `downstream_rq_3xx/4xx/5xx` + `cluster.<name>.upstream_rq_5xx` wire coverage + the
    `upstream_cx_total` `value-exact` tightening remain tied to connection pooling per the
    REVIEW §3 disposition. PROGRESS attributes the down-payment honestly.

20. **The `Driver::Http1AfterSettle` settle-then-probe variant (Task 5)** extends the
    Driver enum at `tests/differential/src/lib.rs` (after the existing `Http2ProbeList`
    variant ~`:140`). Shape:

    ```rust
    /// 12.2 NEW: settle-then-drive variant — sleeps `settle_ms` past
    /// active-HC convergence, then drives ONE Http1 request and applies the
    /// existing 5-axis equivalence cascade. The fixture asserts the
    /// post-convergence STEADY STATE, not a transient. Phase 12 does NOT
    /// opt into Timing tolerances (the settle_ms is a harness mechanic,
    /// not a compared latency bound — BEHAVIOR_CONTRACT.md §Timing).
    Http1AfterSettle {
        settle_ms: u64,
        method: Http1Method,
        path: String,
        host: String,
        #[serde(default)]
        expected_status: Option<u16>,
        #[serde(default)]
        expected_body: Option<Http1BodyRule>,
        #[serde(default)]
        expected_headers: Option<Http1HeaderRule>,
    },
    ```

    Dispatch arm in the driver loop (the existing match on `Driver::*` near `~:1655`):
    `Driver::Http1AfterSettle { settle_ms, .. } => { tokio::time::sleep(Duration::from_millis(*settle_ms)).await; drive_http1(...).await }`.
    The fixture sets `settle_ms: 3500` (≥ `interval × unhealthy_threshold + timeout +
    margin` = `1000 × 1 + 1000 + 1500 = 3500`ms).

21. **The fixture's port-template tag stays `PORT`** (the H1-listener default per the
    Driver-enum match at `~:1655`: `Driver::Http1 { .. } => "PORT"`; `Http1AfterSettle`
    falls under the same arm). No new tag needed.

22. **Docker-gated wrapper at `tests/differential/tests/upstream_active_health_check.rs`
    (Task 5)** mirrors the 10/11/12.1-precedent `http_filter_*.rs` shape: a single
    `#[tokio::test] async fn upstream_active_health_check_fixture()` that calls
    `differential::run_fixture(&dir)`. The harness skips when `DOCKER_HOST` is unavailable
    (cluster-level skip — no per-test cfg gate).

23. **In-process backstop (Task 6) exercises BOTH convergence directions on H1.** Two
    `#[tokio::test]` test fns in `crates/envoy-bin/tests/upstream_active_health_check.rs`:
    `unhealthy_endpoint_returns_synth_503` (backend `/healthz` → 503 ⇒ after settle, `GET
    /` → 503 + `no healthy upstream` body + 5 standard HTTP/1.1 headers) +
    `healthy_endpoint_passes_through` (backend `/healthz` → 200 ⇒ after settle, `GET /` →
    200 + backend's data-path body). The 503-probe header-presence assertion includes all 5
    standard HTTP/1.1 headers (`server`, `date`, `content-length`, `content-type`,
    `connection`) — closes the 10 REVIEW M1 lesson for the new fixture file at the same
    standard the phase-11 H1 backstop applied. **Subprocess discipline**:
    `tokio::process::Command` + `.kill_on_drop(true)` + `Stdio::null()`/`piped()` per the
    standing 09 REVIEW M3 pattern (matches the phase-10/11 backstop shape verbatim).

24. **Fuzz corpus seed (Task 7)** extends `parse_bootstrap` corpus 19 → 20.
    New file `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_upstream_active_health_check.yaml`
    + `.gitignore` allow-list entry + `fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS-array
    extension — all three in **ONE commit** (the 09/10/11/12.1 Task-6 lesson). The seed exercises
    a NEW combination: a full HCM + router + active-HC-configured cluster + panic-disabled +
    route gating (whereas 12.1's `cluster_health_check.yaml` seed is HCM-less header-only) —
    genuinely non-redundant.

25. **Subagent-driven execution at state 3** per `feedback_execution_style`: each task below
    is dispatched to a fresh subagent with two-stage review (spec-compliance + code-quality)
    per the 06.x → 12.1 cadence. The state-2 PLAN-write (this commit) is the controller's
    authoring pass — NOT a subagent dispatch. **TDD on every task** per
    `superpowers:test-driven-development`: write the failing test, run it red, implement
    minimally, run it green, commit. One commit per task per the 06.x → 12.1 one-commit-per-task
    cadence.

---

## 1. PLAN-write SPEC corrections (read against HEAD `67ae869`)

Per the 06.2 → 12.1 "N PLAN-write SPEC corrections" pattern, the PLAN-writer read the 12.2
SPEC §3 surfaces against HEAD (the 12.1 state-6 close-out) and flagged mechanical drift.
These corrections land in the PROGRESS Task 1 preamble + are reflected in the task code below.

1. **`Cluster.endpoint_health` + `Cluster.panic_threshold` are `pub(crate)`** (verified
   `crates/envoy-cluster/src/cluster.rs:72,75`) — not accessible from `envoy-health`. **Task
   1 lands a new public accessor `ClusterHandle::health_probe_targets() -> Option<Vec<(SocketAddr,
   Arc<EndpointHealth>)>>`** (lock-in #5). The internal field stays `pub(crate)`.

2. **The runtime `Cluster` does NOT carry the parsed `HealthCheck` config** (verified — only
   `endpoint_health` thresholds-via-`EndpointHealth` + `panic_threshold` are runtime fields;
   `path`, `interval`, `timeout`, `host`, `expected_statuses` are NOT). `envoy-health` reads
   the HC config DIRECTLY from `&bootstrap.static_resources.clusters[i].health_checks[0]` —
   the envoy-config schema. **`Scheduler::spawn` takes BOTH `&bootstrap` AND `Arc<ClusterManager>`**:
   the former for the HC config (parsing path/interval/timeout/host/expected_statuses), the
   latter for the resolved (SocketAddr, EndpointHealth) pairs via `health_probe_targets()`.
   This keeps `envoy-cluster` as the structural seam (no HC-config interpretation leaking
   into the cluster crate).

3. **`envoy_http1::client::Client::connect` signature is `pub async fn connect(addr:
   SocketAddr, host: &str) -> Result<ClientStream, Http1Error>`** (verified
   `crates/envoy-http1/src/client.rs:33-47`). `ClientStream::send_request(&mut self, request:
   Request) -> Result<Response, Http1Error>` (verified `:73`). `Request` is the existing
   `crates/envoy-http1/src/codec.rs::Request` struct (the same one HCM dispatches; carries
   `method`, `path`, `host`, `headers`, `body`).

4. **`envoy_http1::Client` + `ClientStream` are already PUBLIC** (verified — `pub struct
   Client;` at `client.rs:24`; `pub struct ClientStream` at `:56`). No envoy-http1 surface
   widening needed. The probe imports `envoy_http1::client::{Client, ClientStream}` directly.

5. **`Http1Error::UpstreamConnect { addr, source }`** is the connect-failure variant
   (verified — see `crates/envoy-http1/src/error.rs`). The probe's connect-failure arm
   matches on `Err(Http1Error::UpstreamConnect { .. })` plus the timeout-arm wraps
   `Client::connect` in `tokio::time::timeout`.

6. **`Response.status: u16`, `Response.headers: Vec<(String, String)>`, `Response.body:
   Bytes`** (verified `crates/envoy-http1/src/response.rs`). The probe's success check is on
   `Response.status`.

7. **`hcm.rs:582` arm currently reads `outgoing = synth_status(503, close);`** (verified —
   the `else` branch of `if cluster.pick_endpoint().is_some() { ... } else { ... }` at
   `hcm.rs:580-582`; the comment is *"No healthy endpoint available for this cluster."*).
   Task 3 changes this single line to `outgoing = synth_no_healthy_upstream(close);` + adds
   the new helper adjacent to `synth_status` (`~:918`). **No global `synth_status` edit; no
   call-site fan-out.**

8. **`synth_status` at `hcm.rs:918`** (verified) returns a `Response { status, reason: None,
   headers: vec![(server, default), (date, now), (content-length, "0"), (content-type,
   default), (connection, value)], body: Bytes::new() }`. The new `synth_no_healthy_upstream`
   helper (lock-in #14) mirrors this shape modulo body bytes + content-length.

9. **`envoy-bin/src/main.rs` `from_bootstrap` call-site is at `~:124`** (verified —
   `envoy_cluster::from_bootstrap(&bootstrap, std::sync::Arc::clone(&registry)).await`).
   Task 1 inserts `let scheduler = envoy_health::Scheduler::spawn(&bootstrap,
   std::sync::Arc::clone(&cluster_mgr), std::sync::Arc::clone(&registry))?;` immediately
   after the `cluster_mgr` construction (or in the late-startup section near the listener
   spawn, alongside the existing `JoinSet`). The `scheduler` handle is held for the
   lifetime of the runtime and cancelled at shutdown via the same `CancellationToken`
   already in scope (`signal_token` at `~:97`). PLAN-writer recommendation: hook scheduler
   shutdown into the existing `signal_token` cancellation — when `shutdown_signal().await`
   fires, the scheduler observes the cancellation and exits its loops. Implementation:
   `Scheduler::new()` takes a `CancellationToken`; `Scheduler::spawn(&bootstrap, cluster_mgr,
   registry, signal_token.clone())`.

10. **`StatsRegistry::register_counter(&str) -> Result<Arc<Counter>, StatsError>`**
    (verified `crates/envoy-stats/src/registry.rs:45`); `Counter::inc()` increments by 1.
    `register_counter` is idempotent (same name re-register returns the existing handle).

11. **`Int64Range { start: i64, end: i64 }` half-open `[start, end)`** is the existing
    envoy-config primitive (`crates/envoy-config/src/bootstrap.rs:1080`) — the probe's
    success check uses `(r.start..r.end).contains(&(status as i64))`.

12. **`parse_duration` is `pub fn parse_duration(s: &str) -> Result<Duration, String>`**
    (verified `crates/envoy-config/src/bootstrap.rs:2289`; integer-only; rejects `0.5s`).
    `envoy-health` calls `parse_duration(&hc.interval)` + `parse_duration(&hc.timeout)`
    once at `Scheduler::spawn` time + caches the `Duration` per (cluster, endpoint) probe
    task. The 12.1 D2 validator already guaranteed both parse cleanly + are non-zero.

13. **The existing `Http1EchoBackend` at `tests/differential/src/backend.rs:179`** uses
    testcontainers + a small Docker image to serve an HTTP/1 echo. `HealthAwareHttp1Backend`
    (Task 4) follows the same shape: `.spawn() -> Result<Self>` + `.port() -> u16` +
    `.container_host() -> &'static str`. The PLAN-writer recommends a small ad-hoc helper
    backend container (the simplest path; precedent: `Http1EchoBackend` already runs an
    ad-hoc image). The new backend serves 200 on `/`, 503 on `/healthz`, with a small
    `tokio` + `httparse` implementation in `tests/helpers/health-aware-http1-backend/` (or
    extended onto the existing `tests/helpers/http1-echo-server` — PLAN-writer recommends
    a new helper since the echo backend's semantics are simpler than path-keyed responses).

14. **`tests/differential/src/lib.rs::Driver` enum dispatch arm** (verified `~:1655-1667`):
    the match returns the port-template tag for each variant. `Http1AfterSettle` falls under
    `"PORT"` (same as `Http1` since both drive an H1 data-plane request). The actual
    dispatch site (the `match driver { Driver::Http1 { .. } => drive_http1(...).await, ... }`
    pattern in `run_fixture`) extends with one arm for `Http1AfterSettle` that sleeps
    `settle_ms` first.

---

## File Structure

- **Create** `crates/envoy-health/Cargo.toml` — new workspace member (lock-in #4).
- **Create** `crates/envoy-health/src/lib.rs` — `#![forbid(unsafe_code)]` + re-exports
  (Scheduler + Probe + Error types).
- **Create** `crates/envoy-health/src/scheduler.rs` — `Scheduler::spawn(&bootstrap,
  cluster_mgr, registry, signal_token)`; walks bootstrap clusters with HC configured,
  registers counters, spawns one probe task per (cluster, endpoint); holds JoinHandles +
  exposes `shutdown()`.
- **Create** `crates/envoy-health/src/probe.rs` — `probe_loop` + `probe_once` async fns
  (the periodic-background primitive per lock-in #12).
- **Create** `crates/envoy-health/src/error.rs` — `thiserror`-derived `HealthError`
  (registration failure surfaced from `Scheduler::spawn`).
- **Modify** `Cargo.toml` (workspace root) — append `crates/envoy-health` to `members`
  (alphabetical; adjacent to `envoy-filter`).
- **Modify** `crates/envoy-cluster/src/cluster.rs` — add public accessor
  `ClusterHandle::health_probe_targets() -> Option<Vec<(SocketAddr, Arc<EndpointHealth>)>>`
  (lock-in #5).
- **Modify** `crates/envoy-cluster/src/health.rs` — append a `///` API-boundary
  single-writer-contract comment on `EndpointHealth` (the M2 fold-in; lock-in #6).
- **Modify** `crates/envoy-bin/Cargo.toml` — add `envoy-health = { path = "../envoy-health" }`
  dependency.
- **Modify** `crates/envoy-bin/src/main.rs` — construct + hold the `Scheduler` after the
  `cluster_mgr` build site at `~:124`.
- **Modify** `crates/envoy-http1/src/hcm.rs` — add the `synth_no_healthy_upstream` helper
  adjacent to `synth_status` at `~:918`; change the single call-site at `:582`.
- **Modify** `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — Task 2 appends 3 `health_check.*`
  counter rows + edits the existing 12.1 `membership_healthy` Equivalence cell (the M4
  fold-in); Task 3 appends the no-healthy-upstream `## Response body` subsection.
- **Create** `tests/helpers/health-aware-http1-backend/` (workspace member or in-tree binary
  inside the differential harness — PLAN-writer decides at Task 4 between a new workspace
  member helper or an ad-hoc backend launched from `backend.rs`).
- **Modify** `tests/differential/src/backend.rs` — add `HealthAwareHttp1Backend` struct +
  `spawn()` (mirrors `Http1EchoBackend`).
- **Modify** `tests/differential/src/lib.rs` — add `Driver::Http1AfterSettle` variant
  (lock-in #20) + its dispatch arm + the port-template `"PORT"` mapping.
- **Create** `tests/fixtures/0019-upstream-active-health-check/{envoy.yaml,envoy-rust.yaml,
  expectations.yaml,README.md}` (Task 5).
- **Create** `tests/differential/tests/upstream_active_health_check.rs` — Docker-gated
  wrapper (Task 5).
- **Create** `crates/envoy-bin/tests/upstream_active_health_check.rs` — in-process
  backstop, both directions (Task 6).
- **Create** `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_upstream_active_health_check.yaml`
  (Task 7).
- **Modify** `crates/envoy-config/fuzz/.gitignore` (allow-list the new seed; Task 7).
- **Modify** `crates/envoy-config/src/bootstrap.rs` (`fuzz_corpus_seeds_parse_or_reject_cleanly`
  SUCCESS array extension; Task 7).
- **Modify** `docs/envoy-rust/phases/12.2-active-http-probe-and-fixture/PROGRESS.md` (every task).
- **Modify** `docs/envoy-rust/STATE.md` (Task 8).

---

## Task 1: envoy-health crate scaffold + periodic probe task + ClusterHandle accessor + envoy-bin wiring + M2 contract comment

**Files:**
- Create: `crates/envoy-health/Cargo.toml`
- Create: `crates/envoy-health/src/{lib.rs, scheduler.rs, probe.rs, error.rs}`
- Modify: `Cargo.toml` (workspace `members`)
- Modify: `crates/envoy-cluster/src/cluster.rs` (add `health_probe_targets` accessor)
- Modify: `crates/envoy-cluster/src/health.rs` (M2 single-writer contract comment)
- Modify: `crates/envoy-bin/Cargo.toml` (add `envoy-health` path-dep)
- Modify: `crates/envoy-bin/src/main.rs` (wire `Scheduler::spawn` after `cluster_mgr`)

- [ ] **Step 1: Add `envoy-health` to workspace members** in root `Cargo.toml` (alphabetical
  position; adjacent to `envoy-filter`):

```toml
    "crates/envoy-filter",
    "crates/envoy-health",
    "crates/envoy-http1",
```

- [ ] **Step 2: Create `crates/envoy-health/Cargo.toml`**

```toml
[package]
name = "envoy-health"
version = "0.0.0"
edition = "2024"
publish = false
license = "Apache-2.0"

[lib]
name = "envoy_health"
path = "src/lib.rs"

[dependencies]
envoy-cluster = { path = "../envoy-cluster" }
envoy-config = { path = "../envoy-config" }
envoy-http1 = { path = "../envoy-http1" }
envoy-stats = { path = "../envoy-stats" }
bytes = "1"
thiserror = "2"
tokio = { version = "1", features = ["rt", "macros", "time", "sync"] }
tokio-util = { version = "0.7", features = ["rt"] }
tracing = "0.1"

[dev-dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "test-util"] }
```

> Note on `tokio-util`: already a workspace transitive (envoy-bin uses
> `tokio_util::sync::CancellationToken`); no new top-level crate (lock-in #4 verified).

- [ ] **Step 3: Create `crates/envoy-health/src/lib.rs`**

```rust
#![forbid(unsafe_code)]

//! Phase 12.2 (parent-12 D4): active HTTP health-check probe tasks.
//!
//! The first periodic-background primitive in the project. `Scheduler::spawn`
//! walks every cluster carrying `health_checks` and spawns one
//! `tokio::spawn`ed `probe_loop` per (cluster, endpoint) pair. Each loop
//! ticks every `interval`, issues a `GET <path>` via `envoy_http1::Client`
//! to the endpoint, evaluates the response status against `expected_statuses`,
//! and calls `EndpointHealth::record_success` / `record_failure` (the 12.1
//! state machine) — driving the `membership_healthy` gauge + the 3
//! `cluster.<n>.health_check.{attempt,success,failure}` counters this crate
//! registers.
//!
//! Single-writer contract per (cluster, endpoint): `Scheduler::spawn`
//! produces EXACTLY ONE `probe_loop` task per pair, and the `Arc<EndpointHealth>`
//! is MOVED into that task. No other code path in envoy-rust calls `record_*`
//! on it. The 12.1 `EndpointHealth` Relaxed-ordering soundness rests on this
//! contract (12.1 REVIEW M2; closed at this crate's API boundary).
//!
//! Cycle-free dependency graph: `envoy-health → envoy-http1 → envoy-cluster`
//! + `envoy-health → envoy-cluster` (clean DAG; verified at PLAN-write).
//! `envoy-cluster` stays a leaf for `pick()`; `envoy-http1` stays a router-side
//! consumer; `envoy-health` sits above both as the active-HC driver.

pub mod error;
mod probe;
mod scheduler;

pub use error::HealthError;
pub use scheduler::Scheduler;
```

- [ ] **Step 4: Create `crates/envoy-health/src/error.rs`**

```rust
//! `HealthError` — surfaced by `Scheduler::spawn` when stats registration or
//! duration parsing fails. Per-cluster context (`cluster: String`) matches
//! the envoy-cluster / envoy-config error discipline.

use thiserror::Error;

/// Phase-12.2 health-scheduler error surface.
#[derive(Debug, Error)]
pub enum HealthError {
    /// Stats registration failed for one of the 3 per-cluster counters.
    #[error("registering health_check stats for cluster '{cluster}': {message}")]
    StatsRegistration { cluster: String, message: String },
    /// `parse_duration` rejected `interval` or `timeout` (the 12.1 D2
    /// validator already rejects these at parse, so this is defense-in-depth).
    #[error("parsing {field} for cluster '{cluster}': {message}")]
    InvalidDuration {
        cluster: String,
        field: &'static str,
        message: String,
    },
}
```

- [ ] **Step 5: Create `crates/envoy-health/src/probe.rs`** (the periodic-background primitive)

```rust
//! Per-(cluster, endpoint) probe loop and per-probe `probe_once` helper.
//!
//! `probe_loop` is the body of every spawned task: a `tokio::time::interval`
//! ticker + `tokio::select!` on a `CancellationToken` (graceful shutdown).
//! `probe_once` performs ONE HTTP probe — `Client::connect` + `send_request`
//! wrapped in `tokio::time::timeout(timeout, ...)`. Connection failures,
//! timeouts, and out-of-range statuses all count as failure.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use envoy_cluster::EndpointHealth;
use envoy_config::Int64Range;
use envoy_http1::client::{Client, ClientStream};
use envoy_http1::codec::Request;
use envoy_stats::Counter;
use tokio::time::{MissedTickBehavior, interval, timeout};
use tokio_util::sync::CancellationToken;

/// 12.2: the periodic probe loop, one tokio task per (cluster, endpoint).
/// Single-writer to `endpoint_health` per the M2 contract (PLAN lock-in #6).
/// Graceful cancellation via the `tokio::select!` cancel branch.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn probe_loop(
    addr: SocketAddr,
    host: String,
    path: String,
    probe_timeout: Duration,
    interval_dur: Duration,
    expected_statuses: Vec<Int64Range>,
    endpoint_health: Arc<EndpointHealth>,
    attempt: Arc<Counter>,
    success: Arc<Counter>,
    failure: Arc<Counter>,
    cancel: CancellationToken,
) {
    let mut ticker = interval(interval_dur);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                tracing::debug!(addr=%addr, "active-HC probe task shutting down");
                return;
            }
            _ = ticker.tick() => {
                attempt.inc();
                match probe_once(addr, &host, &path, probe_timeout, &expected_statuses).await {
                    Ok(()) => {
                        success.inc();
                        endpoint_health.record_success();
                    }
                    Err(e) => {
                        tracing::debug!(addr=%addr, error=?e, "active-HC probe failed");
                        failure.inc();
                        endpoint_health.record_failure();
                    }
                }
            }
        }
    }
}

/// Outcome of a single probe — Ok = healthy contribution; Err = failure
/// contribution.
#[derive(Debug)]
pub(crate) enum ProbeError {
    /// `tokio::time::timeout(probe_timeout, ...)` elapsed.
    Timeout,
    /// `Client::connect` returned an error (typically `UpstreamConnect`).
    Connect(String),
    /// `send_request` returned an error.
    Send(String),
    /// Response status not in `expected_statuses`.
    UnexpectedStatus(u16),
}

/// 12.2: one probe — connect + send_request + status check, all under one
/// per-probe `tokio::time::timeout`. Fresh connection (no `reuse_connection`
/// at phase-12 scope per parent §4).
pub(crate) async fn probe_once(
    addr: SocketAddr,
    host: &str,
    path: &str,
    probe_timeout: Duration,
    expected_statuses: &[Int64Range],
) -> Result<(), ProbeError> {
    let probe = async move {
        let mut stream: ClientStream = Client::connect(addr, host)
            .await
            .map_err(|e| ProbeError::Connect(e.to_string()))?;
        let req = Request {
            method: "GET".to_string(),
            path: path.to_string(),
            host: host.to_string(),
            headers: Vec::new(),
            body: None,
        };
        let resp = stream
            .send_request(req)
            .await
            .map_err(|e| ProbeError::Send(e.to_string()))?;
        if status_acceptable(resp.status, expected_statuses) {
            Ok(())
        } else {
            Err(ProbeError::UnexpectedStatus(resp.status))
        }
    };
    match timeout(probe_timeout, probe).await {
        Ok(r) => r,
        Err(_) => Err(ProbeError::Timeout),
    }
}

/// 12.2: success criterion per §6.2 item-5 + PLAN lock-in #10.
/// Empty `expected_statuses` = the upstream default (exactly 200).
fn status_acceptable(status: u16, expected: &[Int64Range]) -> bool {
    if expected.is_empty() {
        return status == 200;
    }
    let s = status as i64;
    expected.iter().any(|r| s >= r.start && s < r.end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_expected_statuses_accepts_only_200() {
        assert!(status_acceptable(200, &[]));
        assert!(!status_acceptable(201, &[]));
        assert!(!status_acceptable(503, &[]));
    }

    #[test]
    fn half_open_range_excludes_end() {
        let r = vec![Int64Range { start: 200, end: 201 }];
        assert!(status_acceptable(200, &r));
        assert!(!status_acceptable(201, &r));
    }

    #[test]
    fn multi_range_union() {
        let r = vec![
            Int64Range { start: 200, end: 300 },
            Int64Range { start: 418, end: 419 },
        ];
        assert!(status_acceptable(204, &r));
        assert!(status_acceptable(418, &r));
        assert!(!status_acceptable(419, &r));
        assert!(!status_acceptable(503, &r));
    }
}
```

- [ ] **Step 6: Create `crates/envoy-health/src/scheduler.rs`**

```rust
//! `Scheduler::spawn` — the entry point envoy-bin calls after building the
//! `ClusterManager`. Walks every cluster carrying `health_checks` (12.1 D2
//! validator guarantees 0 or 1 per cluster, HTTP-only); registers the 3
//! `cluster.<n>.health_check.{attempt,success,failure}` counters; spawns
//! one `probe_loop` per (cluster, endpoint) pair (the single-writer-per-endpoint
//! topology that the 12.1 M2 contract requires).
//!
//! `Scheduler::shutdown` cancels every running probe task via a shared
//! `CancellationToken` and awaits the JoinHandles. The envoy-bin runtime
//! wires the scheduler's cancellation to the existing `signal_token` so
//! SIGTERM/SIGINT triggers a clean drain.

use std::sync::Arc;

use envoy_cluster::{ClusterManager, EndpointHealth};
use envoy_config::Bootstrap;
use envoy_stats::{Counter, StatsRegistry};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::error::HealthError;
use crate::probe::probe_loop;

/// 12.2: the active-HC scheduler. Holds the JoinHandles of every spawned
/// probe task. Drop without `shutdown()` is safe — the tasks observe the
/// runtime shutdown — but `shutdown()` is preferred for clean drain.
#[derive(Debug)]
pub struct Scheduler {
    handles: Vec<JoinHandle<()>>,
    cancel: CancellationToken,
}

impl Scheduler {
    /// 12.2: walk the bootstrap clusters with `health_checks` configured,
    /// register the 3 per-cluster counters, and spawn one `probe_loop` per
    /// (cluster, endpoint) pair. Returns a `Scheduler` holding the task
    /// handles. `cancel` is the shared shutdown token — `Scheduler::shutdown`
    /// or the caller cancelling `cancel` (via the envoy-bin signal token)
    /// terminates every loop at its next `tokio::select!` boundary.
    pub fn spawn(
        bootstrap: &Bootstrap,
        cluster_mgr: Arc<ClusterManager>,
        registry: Arc<StatsRegistry>,
        cancel: CancellationToken,
    ) -> Result<Self, HealthError> {
        let mut handles = Vec::new();
        for cfg in &bootstrap.static_resources.clusters {
            // 12.1 D2 validator guarantees 0 or 1 HC entry, HTTP-only.
            let hc = match cfg.health_checks.first() {
                Some(h) => h,
                None => continue,
            };
            let http = hc
                .http_health_check
                .as_ref()
                .expect("validator-guaranteed http_health_check present");

            // Register the 3 counters (one set per cluster).
            let attempt = register_counter(&registry, &cfg.name, "attempt")?;
            let success = register_counter(&registry, &cfg.name, "success")?;
            let failure = register_counter(&registry, &cfg.name, "failure")?;

            // Re-parse durations (12.1 D2 validator already accepted them
            // — defense-in-depth, identical-result on the success path).
            let interval_dur = envoy_config::parse_duration(&hc.interval)
                .map_err(|message| HealthError::InvalidDuration {
                    cluster: cfg.name.clone(),
                    field: "interval",
                    message,
                })?;
            let probe_timeout = envoy_config::parse_duration(&hc.timeout)
                .map_err(|message| HealthError::InvalidDuration {
                    cluster: cfg.name.clone(),
                    field: "timeout",
                    message,
                })?;

            let host_default = http
                .host
                .clone()
                .unwrap_or_else(|| cfg.name.clone());
            let path = http.path.clone();
            let expected = http.expected_statuses.clone();

            // Walk the resolved (addr, EndpointHealth) pairs from the
            // ClusterManager (the 12.2 `health_probe_targets` accessor).
            let handle = match cluster_mgr.get(&cfg.name) {
                Some(h) => h,
                None => continue, // defense-in-depth: validator+manager align
            };
            let targets = handle
                .health_probe_targets()
                .expect("HC-configured cluster has health_probe_targets");
            for (addr, endpoint_health) in targets {
                let cancel = cancel.clone();
                let host_str = host_default.clone();
                let path_str = path.clone();
                let exp = expected.clone();
                let a = Arc::clone(&attempt);
                let s = Arc::clone(&success);
                let f = Arc::clone(&failure);
                let eh: Arc<EndpointHealth> = endpoint_health;
                let h = tokio::spawn(async move {
                    probe_loop(
                        addr,
                        host_str,
                        path_str,
                        probe_timeout,
                        interval_dur,
                        exp,
                        eh,
                        a,
                        s,
                        f,
                        cancel,
                    )
                    .await;
                });
                handles.push(h);
            }
        }
        Ok(Scheduler { handles, cancel })
    }

    /// 12.2: cancel every probe task and await their JoinHandles. Returns
    /// once every task has exited at its next `tokio::select!` boundary.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        for h in self.handles {
            let _ = h.await;
        }
    }

    /// 12.2: test helper — count of spawned probe tasks.
    pub fn task_count(&self) -> usize {
        self.handles.len()
    }
}

fn register_counter(
    registry: &StatsRegistry,
    cluster: &str,
    kind: &'static str,
) -> Result<Arc<Counter>, HealthError> {
    registry
        .register_counter(&format!("cluster.{cluster}.health_check.{kind}"))
        .map_err(|e| HealthError::StatsRegistration {
            cluster: cluster.to_string(),
            message: e.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use envoy_cluster::from_bootstrap;
    use envoy_config::parse_bootstrap;

    const HC_BOOTSTRAP: &str = r#"
static_resources:
  listeners: []
  clusters:
    - name: hc_backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      common_lb_config:
        healthy_panic_threshold: { value: 0 }
      health_checks:
        - timeout: 1s
          interval: 1s
          healthy_threshold: 1
          unhealthy_threshold: 1
          http_health_check:
            path: /healthz
            expected_statuses:
              - { start: 200, end: 201 }
      load_assignment:
        cluster_name: hc_backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 60001 } }
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 60002 } }
admin:
  address:
    socket_address: { address: 127.0.0.1, port_value: 9901 }
"#;

    const NO_HC_BOOTSTRAP: &str = r#"
static_resources:
  listeners: []
  clusters:
    - name: plain
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: plain
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 60003 } }
admin:
  address:
    socket_address: { address: 127.0.0.1, port_value: 9901 }
"#;

    #[tokio::test]
    async fn spawns_one_task_per_hc_endpoint() {
        let bootstrap = parse_bootstrap(HC_BOOTSTRAP).expect("parse");
        let registry = Arc::new(StatsRegistry::new());
        let cluster_mgr = Arc::new(
            from_bootstrap(&bootstrap, Arc::clone(&registry))
                .await
                .expect("build"),
        );
        let cancel = CancellationToken::new();
        let scheduler = Scheduler::spawn(&bootstrap, cluster_mgr, registry, cancel.clone())
            .expect("scheduler");
        assert_eq!(scheduler.task_count(), 2, "one task per (cluster, endpoint)");
        scheduler.shutdown().await;
    }

    #[tokio::test]
    async fn spawns_zero_tasks_when_no_hc_configured() {
        let bootstrap = parse_bootstrap(NO_HC_BOOTSTRAP).expect("parse");
        let registry = Arc::new(StatsRegistry::new());
        let cluster_mgr = Arc::new(
            from_bootstrap(&bootstrap, Arc::clone(&registry))
                .await
                .expect("build"),
        );
        let cancel = CancellationToken::new();
        let scheduler = Scheduler::spawn(&bootstrap, cluster_mgr, registry, cancel.clone())
            .expect("scheduler");
        assert_eq!(scheduler.task_count(), 0, "no probe task for no-HC cluster");
        scheduler.shutdown().await;
    }

    #[tokio::test]
    async fn shutdown_terminates_all_tasks() {
        let bootstrap = parse_bootstrap(HC_BOOTSTRAP).expect("parse");
        let registry = Arc::new(StatsRegistry::new());
        let cluster_mgr = Arc::new(
            from_bootstrap(&bootstrap, Arc::clone(&registry))
                .await
                .expect("build"),
        );
        let cancel = CancellationToken::new();
        let scheduler = Scheduler::spawn(&bootstrap, cluster_mgr, registry, cancel.clone())
            .expect("scheduler");
        // Tasks loop on a 1s interval; shutdown via cancel must return promptly
        // (not wait for the next tick — `tokio::select!` exits cancel branch).
        let dur = tokio::time::timeout(std::time::Duration::from_secs(3), scheduler.shutdown())
            .await;
        assert!(dur.is_ok(), "shutdown returned within 3s");
    }
}
```

- [ ] **Step 7: Add the `health_probe_targets` accessor** in
  `crates/envoy-cluster/src/cluster.rs` `impl ClusterHandle` (after `upstream_rq_5xx` at
  `~:232`):

```rust
    /// 12.2 (parent-12 D4): per-endpoint health-probe targets when this
    /// cluster configures active health checks. Yields one (addr,
    /// EndpointHealth) pair per resolved endpoint that the `envoy-health`
    /// probe task drives (one task per pair; single-writer-per-endpoint
    /// per the 12.1 REVIEW M2 forward-correctness contract closed at
    /// `envoy-health`'s API boundary). Returns `None` when the cluster
    /// has no `health_checks` configured (the §5.4 inert-when-unconfigured
    /// invariant — no probe task should spawn).
    pub fn health_probe_targets(
        &self,
    ) -> Option<Vec<(SocketAddr, Arc<crate::EndpointHealth>)>> {
        let health = self.inner.endpoint_health.as_ref()?;
        Some(
            self.inner
                .endpoints
                .iter()
                .copied()
                .zip(health.iter().map(Arc::clone))
                .collect(),
        )
    }
```

- [ ] **Step 8: Append the M2 single-writer-contract comment** on `EndpointHealth` in
  `crates/envoy-cluster/src/health.rs` (extend the existing doc comment on the struct at
  `~:23-30`):

```rust
/// Per-endpoint active-health-check state. Shared (`Arc`) so the 12.2 probe
/// task can mutate it while `pick()` (D5) reads it. Single-writer per endpoint
/// (one probe task per (cluster, endpoint)), so `record_*` never race each
/// other for a given endpoint; `pick()` reads `is_healthy()` concurrently with
/// `Relaxed` loads (no happens-before dependency — the `cluster.rs` `pick()`
/// cursor `Relaxed` precedent).
///
/// **API-boundary contract (12.1 REVIEW M2; closed at 12.2):** the live
/// production writer of every `EndpointHealth` is the `envoy-health::Scheduler`
/// probe task spawned per (cluster, endpoint); callers obtaining an
/// `Arc<EndpointHealth>` from `ClusterHandle::health_probe_targets()` (12.2)
/// MUST NOT call `record_success`/`record_failure` themselves and MUST NOT
/// hand the `Arc` to additional writer tasks. Violating this contract makes
/// the `Relaxed`-ordering soundness assumption invalid (concurrent
/// load-modify-store races on `state` may double-increment/decrement the
/// membership gauge). Tests + the 12.2 review verify the contract at the
/// scheduler boundary.
```

- [ ] **Step 9: Wire `Scheduler` into envoy-bin.** Add to `crates/envoy-bin/Cargo.toml`:

```toml
envoy-health = { path = "../envoy-health" }
```

  Then in `crates/envoy-bin/src/main.rs`, after the `cluster_mgr` construction (~`:128`,
  immediately after the `let cluster_mgr = Arc::new(envoy_cluster::from_bootstrap(...)?);`
  block):

```rust
    // 12.2 (parent-12 D4): spawn active-HC probe tasks for every cluster
    // carrying `health_checks`. Cancellation wired to the existing signal
    // token so SIGTERM/SIGINT triggers clean shutdown.
    let health_scheduler = envoy_health::Scheduler::spawn(
        &bootstrap,
        std::sync::Arc::clone(&cluster_mgr),
        std::sync::Arc::clone(&registry),
        token.clone(),
    )
    .context("building active-HC scheduler")?;
```

  Hold `health_scheduler` for the lifetime of the runtime; on the existing shutdown path
  (after `set.join_next().await` returns), call `health_scheduler.shutdown().await;` before
  returning. PLAN-writer note: the existing shutdown sequence is JoinSet-based; the
  scheduler shutdown is `await`ed at the end of `run()` (after the JoinSet drains) to
  ensure clean tokio task drain.

- [ ] **Step 10: Run the new tests + workspace build/test**

Run: `cargo test -p envoy-health`
Expected: PASS (probe::tests 3 + scheduler::tests 3 = 6).

Run: `cargo build --workspace --all-targets`
Expected: clean (the new `envoy-health` member compiles + envoy-bin pulls it).

Run: `cargo test --workspace`
Expected: clean (no regression in envoy-cluster or envoy-bin tests; the new
`ClusterHandle::health_probe_targets` accessor is additive).

- [ ] **Step 11: clippy + fmt + commit**

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Expected: clean.
Run: `cargo fmt --all -- --check`
Expected: clean.

```bash
git add Cargo.toml crates/envoy-health crates/envoy-cluster/src/cluster.rs \
        crates/envoy-cluster/src/health.rs crates/envoy-bin/Cargo.toml \
        crates/envoy-bin/src/main.rs
git commit -m "phase 12.2: task 1 — D4 envoy-health crate + periodic probe task + ClusterHandle::health_probe_targets + envoy-bin wiring + M2 single-writer-contract comment"
```

---

## Task 2: D7 health_check.{attempt,success,failure} counters + BEHAVIOR_CONTRACT row + M4 fold-in

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (append 12.2 entries block + edit 12.1
  `membership_healthy` Equivalence cell)
- Test: `crates/envoy-health/src/scheduler.rs` (registration-assertion tests)

> Per the 06.x → 12.1 cadence: contract extensions land at the task where the surface is
> first wired. The 3 counters are registered at `Scheduler::spawn` time (Task 1); this task
> adds the test attestations that the registrations land + extends the contract.

- [ ] **Step 1: Write the failing registration tests** (append to `scheduler.rs` tests
  module)

```rust
    #[tokio::test]
    async fn registers_three_counters_per_hc_cluster() {
        let bootstrap = parse_bootstrap(HC_BOOTSTRAP).expect("parse");
        let registry = Arc::new(StatsRegistry::new());
        let cluster_mgr = Arc::new(
            from_bootstrap(&bootstrap, Arc::clone(&registry))
                .await
                .expect("build"),
        );
        let cancel = CancellationToken::new();
        let _scheduler = Scheduler::spawn(&bootstrap, cluster_mgr, registry.clone(), cancel)
            .expect("scheduler");
        let snapshot = registry.snapshot();
        for kind in ["attempt", "success", "failure"] {
            let name = format!("cluster.hc_backend.health_check.{kind}");
            assert!(
                snapshot.iter().any(|(n, _)| n == &name),
                "registry must contain {name}; snapshot = {snapshot:?}"
            );
        }
    }

    #[tokio::test]
    async fn registers_no_counters_when_no_hc_configured() {
        let bootstrap = parse_bootstrap(NO_HC_BOOTSTRAP).expect("parse");
        let registry = Arc::new(StatsRegistry::new());
        let cluster_mgr = Arc::new(
            from_bootstrap(&bootstrap, Arc::clone(&registry))
                .await
                .expect("build"),
        );
        let cancel = CancellationToken::new();
        let _scheduler = Scheduler::spawn(&bootstrap, cluster_mgr, registry.clone(), cancel)
            .expect("scheduler");
        let snapshot = registry.snapshot();
        for kind in ["attempt", "success", "failure"] {
            let name = format!("cluster.plain.health_check.{kind}");
            assert!(
                !snapshot.iter().any(|(n, _)| n == &name),
                "registry must NOT contain {name} (no HC configured)"
            );
        }
    }
```

- [ ] **Step 2: Run red.**

Run: `cargo test -p envoy-health registers_three_counters registers_no_counters`
Expected: BOTH PASS (Task 1 already registers the 3 counters in `Scheduler::spawn`). This
task's substance is the contract extension; the tests are attestations. If either FAILS,
Task 1's `Scheduler::spawn` registration logic is wrong — fix in `scheduler.rs`, not here.

- [ ] **Step 3: Append the 12.2 contract entries** in
  `docs/envoy-rust/BEHAVIOR_CONTRACT.md`, in the `## Stat-name mapping` section, after the
  existing `**12.1 entries (active health checking):**` block (preserving the 12.1 block
  verbatim modulo the single Equivalence-cell edit in Step 4):

```markdown
**12.2 entries (active health checking — counters):**

| Stat name | Equivalence | Rationale |
|---|---|---|
| `cluster.<name>.health_check.attempt` | name-required, value-may-differ | Counter; one increment per health-check probe issued by the `envoy-health` scheduler. The count is **timing-dependent** — both proxies tick on their own independent `tokio::time::interval` schedules from independent process-start instants, so the elapsed-probe count over a fixed test window differs across proxies. Both proxies emit the name; the equivalence dimension is name-required only (value-exact is not feasible without timing-tolerance opt-in per §Timing tolerances, which phase 12 does NOT take). Registered at `Scheduler::spawn` time only when the cluster configures `health_checks`. |
| `cluster.<name>.health_check.success` | name-required, value-may-differ | Counter; one increment per probe whose response status ∈ `expected_statuses` (default exactly 200, half-open `Int64Range`). Same timing-dependence rationale as `.attempt`. |
| `cluster.<name>.health_check.failure` | name-required, value-may-differ | Counter; one increment per probe whose response status is NOT in `expected_statuses`, OR connect failure, OR per-probe `tokio::time::timeout` elapsed, OR malformed response (the network-failure-class results fold into `failure` at phase-12 scope; the dedicated `network_failure` sub-counter defers per parent SPEC §4). Same timing-dependence rationale as `.attempt`. |
```

- [ ] **Step 4: M4 fold-in.** In the same `## Stat-name mapping` section, in the existing
  `**12.1 entries (active health checking):**` block, edit ONLY the `cluster.<name>.membership_healthy`
  row's Equivalence cell:

```markdown
| `cluster.<name>.membership_healthy` | value-exact (12.2 steady state; reads 0 at 12.1) | Gauge; the count of currently-healthy endpoints in the cluster. Registered at `from_bootstrap` time only when the cluster configures `health_checks`; updated inline at each `EndpointHealth` Healthy/Unhealthy flip (one source of truth, NOT polled — the 08.2 `server.live` pattern). At 12.1, with no probe task, a configured-HC cluster's gauge reads its initial value 0 (all endpoints start Unhealthy per §6.2 item-1); 12.2's probe task drives it to the converged steady state. Inert when `health_checks` is unconfigured (no such gauge registered). The 3 `cluster.<name>.health_check.{attempt,success,failure}` counters defer to 12.2 where the probe task increments them (12.1 D6 lock-in). |
```

  Only the Equivalence column text `value-exact (steady state)` → `value-exact (12.2 steady
  state; reads 0 at 12.1)` changes. The Rationale column is unchanged.

- [ ] **Step 5: Run tests green + workspace test**

Run: `cargo test -p envoy-health`
Expected: PASS (8 tests now: 3 probe + 5 scheduler).

- [ ] **Step 6: fmt + commit**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md crates/envoy-health/src/scheduler.rs
git commit -m "phase 12.2: task 2 — D7 health_check.{attempt,success,failure} counters + BEHAVIOR_CONTRACT 12.2 block + M4 12.1 membership_healthy Equivalence-cell fold-in"
```

---

## Task 3: D6.2 hcm.rs:582-arm no-healthy-upstream body reconciliation (ADR-0037)

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` (add `synth_no_healthy_upstream` helper; change
  the single `:582` call-site)
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (append `## Response body — no-healthy-upstream
  synth-503` subsection)

- [ ] **Step 1: Write the failing test** (append to `hcm.rs` `#[cfg(test)] mod tests`)

```rust
    #[test]
    fn synth_no_healthy_upstream_emits_19_byte_body_and_5_headers() {
        // 12.2 D6.2 / ADR-0037: the no-healthy-upstream synth-503 emits the
        // 19-byte body `no healthy upstream` (matching upstream Envoy v1.33.0
        // per parent-12 §6.2 item-2). Mirrors `synth_status` 5-standard-header
        // shape modulo body + content-length.
        let resp = super::synth_no_healthy_upstream(true);
        assert_eq!(resp.status, 503);
        assert_eq!(resp.body.as_ref(), b"no healthy upstream");
        assert_eq!(resp.body.len(), 19, "exact byte count per ADR-0037");
        let header_names: Vec<&str> =
            resp.headers.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            header_names,
            vec![
                headers::SERVER,
                headers::DATE,
                headers::CONTENT_LENGTH,
                headers::CONTENT_TYPE,
                headers::CONNECTION,
            ],
            "5 standard HTTP/1.1 headers in canonical order"
        );
        let cl = resp
            .headers
            .iter()
            .find(|(n, _)| n == headers::CONTENT_LENGTH)
            .map(|(_, v)| v.as_str())
            .expect("content-length present");
        assert_eq!(cl, "19", "content-length matches body length");
    }
```

- [ ] **Step 2: Run red**

Run: `cargo test -p envoy-http1 synth_no_healthy_upstream`
Expected: FAIL — `cannot find function synth_no_healthy_upstream`.

- [ ] **Step 3: Add the helper** in `crates/envoy-http1/src/hcm.rs`, adjacent to
  `synth_status` (immediately after the `synth_status` function body at `~:935`)

```rust
/// 12.2 (parent-12 D6.2 per ADR-0037): no-healthy-upstream synth-503 response.
/// Mirrors `synth_status`'s 5-header shape but emits the 19-byte body
/// `no healthy upstream` (hex `6e 6f 20 68 65 61 6c 74 68 79 20 75 70 73 74
/// 72 65 61 6d`; no trailing newline) matching upstream Envoy v1.33.0's
/// no-healthy-upstream wire shape (§6.2 item-2; locked at parent-12 split
/// `4f9ba04`; ADR-0037). Used ONLY at the `pick() -> None` arm of HCM's
/// per-request dispatch (`hcm.rs:582` in this file); the connect-fail 502
/// and other synth paths keep `synth_status`'s empty body.
fn synth_no_healthy_upstream(close: bool) -> Response {
    let body = Bytes::from_static(b"no healthy upstream");
    Response {
        status: 503,
        reason: None,
        headers: vec![
            (headers::SERVER.to_string(), DEFAULT_SERVER_NAME.to_string()),
            (headers::DATE.to_string(), now_imf_fixdate()),
            (headers::CONTENT_LENGTH.to_string(), body.len().to_string()),
            (
                headers::CONTENT_TYPE.to_string(),
                DEFAULT_CONTENT_TYPE.to_string(),
            ),
            (
                headers::CONNECTION.to_string(),
                connection_value(close).to_string(),
            ),
        ],
        body,
    }
}
```

- [ ] **Step 4: Change the single call-site** at `crates/envoy-http1/src/hcm.rs:582`. The
  current arm reads:

```rust
                    } else {
                        // No healthy endpoint available for this cluster.
                        tracing::warn!(
                            cluster = %cluster.name(),
                            "no healthy endpoint for cluster — returning 503",
                        );
                        outgoing = synth_status(503, close);
                    }
```

  Change ONLY the last line:

```rust
                    } else {
                        // No healthy endpoint available for this cluster.
                        // 12.2 (parent-12 D6.2 / ADR-0037): emit the 19-byte
                        // `no healthy upstream` body to match upstream Envoy
                        // v1.33.0's wire shape on the same path.
                        tracing::warn!(
                            cluster = %cluster.name(),
                            "no healthy endpoint for cluster — returning 503",
                        );
                        outgoing = synth_no_healthy_upstream(close);
                    }
```

  **No other `synth_status` call is touched** (the connect-fail 502 + send-fail 502 paths
  keep their empty `synth_status` body per lock-in #15; verified via `grep -n "synth_status"
  crates/envoy-http1/src/hcm.rs` — the 4 call sites are: the helper definition itself; the
  connect-fail 502 arm; the send-fail 502 arm; and the `:582` arm; only the last changes).

- [ ] **Step 5: Append the BEHAVIOR_CONTRACT subsection.** In
  `docs/envoy-rust/BEHAVIOR_CONTRACT.md`, after the `## Equivalence matrix` section's first
  empty line + horizontal rule (and before `## Header allow-list`), append a new section:

```markdown
## Response body — no-healthy-upstream synth-503

> Authored per phase 12.2 SPEC §2.2 + ADR-0037. The H1 HCM per-request
> dispatch path returns a synthetic 503 when `Cluster::pick()` yields
> `None` — both proxies emit it with identical wire shape on the same
> active-HC eviction.

| Reachability path | Equivalence disposition |
|---|---|
| `pick() -> None` (HCM H1 `hcm.rs:582` arm; cluster has `health_checks` configured AND all endpoints unhealthy AND panic not engaged) | Status 503; body byte-exact `no healthy upstream` (19 bytes, hex `6e 6f 20 68 65 61 6c 74 68 79 20 75 70 73 74 72 65 61 6d`, NO trailing newline); 5 standard HTTP/1.1 response headers `{server, date, content-length: 19, content-type, connection}`. Emitted via the dedicated `synth_no_healthy_upstream` helper adjacent to `synth_status` — the helper is used ONLY on this path. The connect-fail 502 + send-fail 502 paths keep `synth_status`'s empty body (phase-04.3 wire shape). |

---
```

  (Insert the new section between `## Equivalence matrix` and `## Header allow-list`,
  preserving both around it.)

- [ ] **Step 6: Run tests green + workspace test**

Run: `cargo test -p envoy-http1 synth_no_healthy_upstream`
Expected: PASS.
Run: `cargo test --workspace`
Expected: PASS (no regression in H1/HCM unit tests; the existing HCM tests that exercise
the connect-fail 502 + other synth paths are untouched).

- [ ] **Step 7: clippy + fmt + commit**

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Expected: clean.
Run: `cargo fmt --all -- --check`
Expected: clean.

```bash
git add crates/envoy-http1/src/hcm.rs docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 12.2: task 3 — D6.2 synth_no_healthy_upstream helper + hcm.rs:582 arm reconciles to 19-byte body per ADR-0037 + BEHAVIOR_CONTRACT Response-body subsection"
```

---

## Task 4: D7.1 synthetic health-aware backend harness primitive (06.3 REVIEW I2 down-payment)

**Files:**
- Create: `tests/helpers/health-aware-http1-backend/Cargo.toml`
- Create: `tests/helpers/health-aware-http1-backend/src/main.rs`
- Modify: `Cargo.toml` (workspace `members`)
- Modify: `tests/differential/src/backend.rs` (add `HealthAwareHttp1Backend`)

> This task lands the synthetic-backend harness primitive 06.3 REVIEW I2 named. Phase 12.2
> makes the down-payment (the synthetic-backend infrastructure); full I2 closure (per-class
> counter wire coverage + `upstream_cx_total` value-exact tightening) remains tied to
> connection pooling per the 06.3 REVIEW §3 disposition. PROGRESS attributes the
> down-payment honestly — do NOT over-claim full closure.

- [ ] **Step 1: Add the helper to workspace members** in root `Cargo.toml` (alphabetical
  position; adjacent to existing `tests/helpers/http1-echo-server`):

```toml
    "tests/helpers/health-aware-http1-backend",
    "tests/helpers/http1-echo-server",
```

- [ ] **Step 2: Create `tests/helpers/health-aware-http1-backend/Cargo.toml`**

```toml
[package]
name = "health-aware-http1-backend"
version = "0.0.0"
edition = "2024"
publish = false
license = "Apache-2.0"

[[bin]]
name = "health-aware-http1-backend"
path = "src/main.rs"

[dependencies]
anyhow = "1"
bytes = "1"
httparse = "1"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "net", "io-util"] }
tracing = "0.1"
tracing-subscriber = "0.3"
```

- [ ] **Step 3: Create `tests/helpers/health-aware-http1-backend/src/main.rs`**

```rust
#![forbid(unsafe_code)]

//! 12.2 D7.1: synthetic health-aware HTTP/1.1 backend for the active-HC
//! differential fixture. Serves a configurable per-path status — by default,
//! 200 on `/` and 503 on `/healthz` (the discriminating health-check signal).
//! This is the project's first synthetic-backend harness primitive (the
//! 06.3 REVIEW I2 down-payment).
//!
//! CLI:
//!   health-aware-http1-backend --port <PORT> [--healthz-status 503]
//!     [--data-status 200] [--data-body "ok\n"]
//!
//! All response shaping is hand-rolled (no framework) so the backend stays
//! transparent — no hidden header behavior. Connection: close per response
//! (the active-HC probe uses a fresh connection per probe).

use std::env;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[derive(Debug, Clone)]
struct Config {
    port: u16,
    healthz_status: u16,
    data_status: u16,
    data_body: Vec<u8>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_target(false).init();
    let cfg = parse_args()?;
    let listener = TcpListener::bind(("0.0.0.0", cfg.port))
        .await
        .with_context(|| format!("binding 0.0.0.0:{}", cfg.port))?;
    tracing::info!(port = cfg.port, "health-aware-http1-backend listening");
    let cfg = Arc::new(cfg);
    loop {
        let (stream, peer) = listener.accept().await.context("accept")?;
        let cfg = Arc::clone(&cfg);
        tokio::spawn(async move {
            if let Err(e) = serve(stream, cfg).await {
                tracing::debug!(error=?e, %peer, "connection ended");
            }
        });
    }
}

fn parse_args() -> Result<Config> {
    let mut port: Option<u16> = None;
    let mut healthz_status: u16 = 503;
    let mut data_status: u16 = 200;
    let mut data_body: Vec<u8> = b"ok\n".to_vec();
    let args: Vec<String> = env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--port" => {
                port = Some(args[i + 1].parse().context("parsing --port")?);
                i += 2;
            }
            "--healthz-status" => {
                healthz_status = args[i + 1].parse().context("parsing --healthz-status")?;
                i += 2;
            }
            "--data-status" => {
                data_status = args[i + 1].parse().context("parsing --data-status")?;
                i += 2;
            }
            "--data-body" => {
                data_body = args[i + 1].as_bytes().to_vec();
                i += 2;
            }
            other => bail!("unknown arg: {other}"),
        }
    }
    Ok(Config {
        port: port.context("--port is required")?,
        healthz_status,
        data_status,
        data_body,
    })
}

async fn serve(mut stream: TcpStream, cfg: Arc<Config>) -> Result<()> {
    let mut buf = vec![0u8; 8192];
    let mut filled = 0;
    let head_end = loop {
        let n = stream.read(&mut buf[filled..]).await?;
        if n == 0 {
            bail!("EOF before headers complete");
        }
        filled += n;
        if let Some(pos) = buf[..filled].windows(4).position(|w| w == b"\r\n\r\n") {
            break pos + 4;
        }
        if filled == buf.len() {
            bail!("request headers too large");
        }
    };
    let mut headers_storage = [httparse::EMPTY_HEADER; 32];
    let mut req = httparse::Request::new(&mut headers_storage);
    req.parse(&buf[..head_end])?;
    let path = req.path.unwrap_or("/").to_string();
    let (status, body): (u16, Vec<u8>) = if path == "/healthz" {
        (cfg.healthz_status, Vec::new())
    } else {
        (cfg.data_status, cfg.data_body.clone())
    };
    let resp = format!(
        "HTTP/1.1 {status} {reason}\r\nserver: health-aware-http1-backend\r\ncontent-length: {len}\r\ncontent-type: text/plain\r\nconnection: close\r\n\r\n",
        status = status,
        reason = status_reason(status),
        len = body.len(),
    );
    stream.write_all(resp.as_bytes()).await?;
    stream.write_all(&body).await?;
    let _ = stream.shutdown().await;
    Ok(())
}

fn status_reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        503 => "Service Unavailable",
        _ => "OK",
    }
}
```

- [ ] **Step 4: Add `HealthAwareHttp1Backend` to `tests/differential/src/backend.rs`** —
  append after the existing `Http1EchoBackend` block (~`:255`):

```rust
/// 12.2 D7.1 (06.3 REVIEW I2 down-payment): synthetic health-aware HTTP/1.1
/// backend. Serves 200 on `/` and 503 on `/healthz` by default — the
/// discriminating signal for the active-HC differential fixture. Runs on
/// the host bridge network via testcontainers' `cargo run`-equivalent
/// pattern (the existing helper-binary lifecycle in this module).
pub struct HealthAwareHttp1Backend {
    child: tokio::process::Child,
    port: u16,
}

impl HealthAwareHttp1Backend {
    /// 12.2: spawn the helper backend binary as a tokio subprocess (NOT a
    /// Docker container — the backend runs on the host alongside the
    /// differential harness; the Docker-running envoy + envoy-rust dial
    /// `host.docker.internal:port` per the existing 04.3 / 05.3 helper
    /// pattern). `kill_on_drop(true)` per 09 REVIEW M3 standing discipline.
    pub async fn spawn() -> Result<Self> {
        let port = crate::reserve_port()?;
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .ok_or_else(|| anyhow::anyhow!("locating workspace root"))?;
        let helper_manifest = manifest.join("tests/helpers/health-aware-http1-backend/Cargo.toml");
        let child = tokio::process::Command::new(env!("CARGO"))
            .arg("run")
            .arg("--quiet")
            .arg("--manifest-path")
            .arg(&helper_manifest)
            .arg("--")
            .arg("--port")
            .arg(port.to_string())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context("spawning health-aware-http1-backend")?;
        // Brief readiness poll: connect to 127.0.0.1:port with retry up to ~3s.
        let addr: std::net::SocketAddr = ("127.0.0.1", port).into();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            if tokio::net::TcpStream::connect(addr).await.is_ok() {
                break;
            }
            if std::time::Instant::now() >= deadline {
                anyhow::bail!("health-aware-http1-backend did not become ready");
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        Ok(Self { child, port })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Address from inside a Docker container running on the host bridge
    /// network. Matches the existing `Http1EchoBackend` convention.
    pub fn container_host(&self) -> &'static str {
        "host.docker.internal"
    }
}

impl Drop for HealthAwareHttp1Backend {
    fn drop(&mut self) {
        // kill_on_drop(true) handles the SIGKILL; this Drop is a no-op
        // anchor for the lifecycle contract (matches Http1EchoBackend).
        let _ = self.child.start_kill();
    }
}
```

- [ ] **Step 5: Run the workspace build to verify the new helper compiles**

Run: `cargo build --workspace --all-targets`
Expected: clean (the new helper binary builds; the `backend.rs` extension compiles against
the existing `tokio::process::Command` + `reserve_port` + `Result` re-exports).

- [ ] **Step 6: clippy + fmt + commit**

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Expected: clean.
Run: `cargo fmt --all -- --check`
Expected: clean.

```bash
git add Cargo.toml tests/helpers/health-aware-http1-backend tests/differential/src/backend.rs
git commit -m "phase 12.2: task 4 — D7.1 synthetic health-aware HTTP/1.1 backend (06.3 REVIEW I2 down-payment) + HealthAwareHttp1Backend harness primitive"
```

---

## Task 5: D7.2 fixture 0019 + Driver::Http1AfterSettle + Docker wrapper

**Files:**
- Modify: `tests/differential/src/lib.rs` (`Driver::Http1AfterSettle` variant + dispatch)
- Create: `tests/fixtures/0019-upstream-active-health-check/envoy.yaml`
- Create: `tests/fixtures/0019-upstream-active-health-check/envoy-rust.yaml`
- Create: `tests/fixtures/0019-upstream-active-health-check/expectations.yaml`
- Create: `tests/fixtures/0019-upstream-active-health-check/README.md`
- Create: `tests/differential/tests/upstream_active_health_check.rs`

- [ ] **Step 1: Add `Driver::Http1AfterSettle` variant** in
  `tests/differential/src/lib.rs`, after `Driver::Http2ProbeList` (~`:140`):

```rust
    /// 12.2 NEW: settle-then-drive H1 variant. Sleeps `settle_ms`
    /// past active-HC convergence (≥ `interval × unhealthy_threshold +
    /// timeout + margin`), then drives ONE Http1 request and applies the
    /// existing 5-axis equivalence cascade. The fixture asserts the
    /// post-convergence STEADY STATE, not a transient. Phase 12 does NOT
    /// opt into Timing tolerances (the settle_ms is a harness mechanic,
    /// not a compared latency bound — BEHAVIOR_CONTRACT.md §Timing).
    Http1AfterSettle {
        settle_ms: u64,
        method: Http1Method,
        path: String,
        host: String,
        #[serde(default)]
        expected_status: Option<u16>,
        #[serde(default)]
        expected_body: Option<Http1BodyRule>,
        #[serde(default)]
        expected_headers: Option<Http1HeaderRule>,
    },
```

- [ ] **Step 2: Extend the port-template arm** (~`:1655`) and the dispatch arm. The
  port-template arm extension (add `Driver::Http1AfterSettle { .. }` alongside `Http1` in
  the `"PORT"` group):

```rust
        Driver::TcpEcho
        | Driver::TlsTcp { .. }
        | Driver::TlsTcpProbeList { .. }
        | Driver::Http1 { .. }
        | Driver::Http1ProbeList { .. }
        | Driver::Http1WithAccessLog { .. }
        | Driver::Http1AfterSettle { .. }
        | Driver::Http2 { .. }
        | Driver::Http2ProbeList { .. }
```

  Then in the existing `match driver { ... }` dispatch (the one that calls `drive_http1`,
  `drive_http2`, etc.), add:

```rust
        Driver::Http1AfterSettle {
            settle_ms,
            method,
            path,
            host,
            expected_status,
            expected_body,
            expected_headers,
        } => {
            tracing::debug!(settle_ms, "Http1AfterSettle: sleeping for active-HC settle");
            tokio::time::sleep(std::time::Duration::from_millis(*settle_ms)).await;
            drive_http1(
                addr,
                *method,
                path.clone(),
                host.clone(),
                *expected_status,
                expected_body.clone(),
                expected_headers.clone(),
            )
            .await
        }
```

  (PLAN-writer note: the exact signature of `drive_http1` is what the existing
  `Driver::Http1` arm uses verbatim; copy that arm's parameter passing for parity.)

- [ ] **Step 3: Create `tests/fixtures/0019-upstream-active-health-check/envoy.yaml`**

```yaml
# Phase 12.2 differential acceptance fixture: assert post-convergence
# active-HC steady state on an H1 listener.
#
# Topology:
#   - downstream: HTTP/1.1 listener on {{PORT}} → HCM → router → cluster `hc_backend`
#   - upstream  : a synthetic health-aware backend at {{BACKEND_HOST}}:{{BACKEND_PORT}}
#                 returning 200 on `/` and 503 on `/healthz`
#   - cluster   : active HTTP HC every 1s, `unhealthy_threshold: 1`, path `/healthz`,
#                 panic disabled (`healthy_panic_threshold: { value: 0 }`)
#
# After ~3 s + margin both proxies converge: the sole endpoint is ejected
# (probes hit `/healthz` → 503 → Unhealthy) → `pick() -> None` →
# synth-503 with body `no healthy upstream` (19 bytes, ADR-0037).
# Driver: `Driver::Http1AfterSettle` (settle 3500 ms, then GET /).
#
# This is the FIRST Upstream-robustness-family fixture and the FIRST
# fixture exercising synth-503 from the no-healthy-upstream arm (the
# `hcm.rs:582` arm; vs synth_status (502 connect-fail) at the connect-fail
# arm). The bilateral assertion is status 503 + body byte-exact + the 5
# standard HTTP/1.1 headers via set-equal-modulo-allow-list.
#
# Integer-second durations per parent §6.2 item-6 (both proxies' duration
# parsers accept `1s` only — Envoy rejects `500ms`; envoy-rust rejects
# `0.5s`).
admin:
  address:
    socket_address:
      address: 0.0.0.0
      port_value: 0
node:
  cluster: phase-12-cluster
  id: phase-12-envoy
static_resources:
  listeners:
    - name: ingress_http
      address:
        socket_address:
          address: 0.0.0.0
          port_value: {{PORT}}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                codec_type: HTTP1
                stat_prefix: ingress_http
                generate_request_id: false
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: local
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: hc_backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: hc_backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      dns_lookup_family: V4_ONLY
      common_lb_config:
        healthy_panic_threshold: { value: 0 }
      health_checks:
        - timeout: 1s
          interval: 1s
          healthy_threshold: 1
          unhealthy_threshold: 1
          http_health_check:
            path: /healthz
            expected_statuses:
              - { start: 200, end: 201 }
      load_assignment:
        cluster_name: hc_backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: {{BACKEND_HOST}}
                      port_value: {{BACKEND_PORT}}
```

- [ ] **Step 4: Create `tests/fixtures/0019-upstream-active-health-check/envoy-rust.yaml`**

```yaml
# envoy-rust side. No admin block; bind 127.0.0.1; no generate_request_id
# (envoy-rust's HCM config does not model it).
node:
  cluster: phase-12-cluster
  id: phase-12-envoy-rust
static_resources:
  listeners:
    - name: ingress_http
      address:
        socket_address:
          address: 127.0.0.1
          port_value: {{PORT}}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                codec_type: HTTP1
                stat_prefix: ingress_http
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: local
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: hc_backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: hc_backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      dns_lookup_family: V4_ONLY
      common_lb_config:
        healthy_panic_threshold: { value: 0 }
      health_checks:
        - timeout: 1s
          interval: 1s
          healthy_threshold: 1
          unhealthy_threshold: 1
          http_health_check:
            path: /healthz
            expected_statuses:
              - { start: 200, end: 201 }
      load_assignment:
        cluster_name: hc_backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: {{BACKEND_HOST}}
                      port_value: {{BACKEND_PORT}}
```

- [ ] **Step 5: Create `tests/fixtures/0019-upstream-active-health-check/expectations.yaml`**

```yaml
driver:
  Http1AfterSettle:
    settle_ms: 3500
    method: GET
    path: /
    host: hc_backend
    expected_status: 503
    expected_body:
      ByteExact: "no healthy upstream"
    expected_headers:
      SetEqualModuloAllowList:
        required:
          - server
          - date
          - content-length
          - content-type
          - connection
```

- [ ] **Step 6: Create `tests/fixtures/0019-upstream-active-health-check/README.md`**

```markdown
# Fixture 0019 — `upstream-active-health-check`

**Phase:** 12.2 (parent-12 D7).
**Differential surface:** post-convergence active-HC steady state on an H1 listener.

After ~3.5s settle, both proxies have probed the synthetic backend at `/healthz` ≥1
time, observed the 503, transitioned the sole endpoint to Unhealthy, and (with
`healthy_panic_threshold: { value: 0 }` disabling panic) make `pick()` return
`None`. The H1 HCM `hcm.rs:582` arm fires synth-503 with body `no healthy upstream`
(19 bytes per ADR-0037 / `synth_no_healthy_upstream`).

The discriminating bilateral observable: **status 503 + body byte-exact + the 5
standard HTTP/1.1 headers** via `set-equal-modulo-allow-list`. The `server` +
`date` header values diverge per the existing 04.1 allow-list rows.

Integer-second durations (`1s`/`1s`) per §6.2 item-6 — the only duration form
both proxy parsers accept.

The synthetic backend is launched by the harness (`HealthAwareHttp1Backend`;
12.2 D7.1 / the 06.3 REVIEW I2 down-payment).
```

- [ ] **Step 7: Create `tests/differential/tests/upstream_active_health_check.rs`** —
  Docker wrapper mirroring fixture-0018's shape:

```rust
//! Phase 12.2 differential acceptance test for fixture
//! 0019-upstream-active-health-check. Drives a single `GET /` on an HTTP/1.1
//! listener AFTER a 3.5s settle window past active-HC convergence. Both
//! proxies must converge to ejecting the sole endpoint (active HC probes
//! `/healthz` → 503 → Unhealthy; `healthy_panic_threshold: { value: 0 }`
//! disables panic) and return synth-503 with body `no healthy upstream`
//! (19 bytes per ADR-0037).
//!
//! This is the FIRST Upstream-robustness-family differential fixture and
//! the FIRST one to drive synth-503 from the no-healthy-upstream arm
//! bilaterally (the `hcm.rs:582` arm). The 06.3 REVIEW I2 synthetic-backend
//! harness primitive (`HealthAwareHttp1Backend`) lands at Task 4 / D7.1
//! and is exercised end-to-end here.
//!
//! Docker-gated by the differential harness at the cluster level (no per-test
//! cfg gate; the harness skips when `DOCKER_HOST` is unavailable).

use std::path::PathBuf;

#[tokio::test]
async fn upstream_active_health_check_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0019-upstream-active-health-check");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
```

- [ ] **Step 8: Hook fixture 0019's backend setup into the harness backend dispatcher.**
  The differential harness's `run_fixture` dispatches `BACKEND_HOST`/`BACKEND_PORT` template
  substitution from the per-fixture backend. The PLAN-writer follows the existing
  `Http1EchoBackend` precedent at `tests/differential/src/lib.rs` (the `match` on fixture
  directory name → backend selection). Add a `0019-upstream-active-health-check` arm that
  spawns `HealthAwareHttp1Backend` and exposes `BACKEND_HOST` + `BACKEND_PORT` template
  values. Implementation matches the existing fixture-0008 / fixture-0010 backend
  selection — see `lib.rs::run_fixture` for the canonical pattern (PLAN-writer leaves the
  exact edit site to be located at task time since the harness's backend dispatcher has
  evolved across phases; the principle is "follow `0008-http1-router-upstream`'s
  `Http1EchoBackend` arm verbatim, substituting `HealthAwareHttp1Backend`").

- [ ] **Step 9: Run the fixture locally if Docker available, else build-only**

Run: `cargo build --workspace --all-targets`
Expected: clean.
Run: `cargo test -p differential upstream_active_health_check_fixture -- --include-ignored`
(if Docker available)
Expected: PASS bilaterally; envoy + envoy-rust both emit the 503 + `no healthy upstream`
body after the 3.5s settle.

  > If Docker is NOT available locally, this task's verification defers to CI (the
  > `build + test + lint` job runs the Docker step). The build must still be clean.

- [ ] **Step 10: clippy + fmt + commit**

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Expected: clean.
Run: `cargo fmt --all -- --check`
Expected: clean.

```bash
git add tests/differential/src/lib.rs tests/fixtures/0019-upstream-active-health-check \
        tests/differential/tests/upstream_active_health_check.rs
git commit -m "phase 12.2: task 5 — D7.2 fixture 0019-upstream-active-health-check + Driver::Http1AfterSettle + Docker wrapper"
```

---

## Task 6: D7.3 in-process backstop (H1 — both convergence directions)

**Files:**
- Create: `crates/envoy-bin/tests/upstream_active_health_check.rs`

> Mirrors the phase-10/11 backstop shape: subprocess discipline
> (`tokio::process::Command` + `kill_on_drop(true)` + `Stdio::null()`/`piped()` per 09
> REVIEW M3); exercises BOTH directions (healthy → 200 through; unhealthy → 503 + `no
> healthy upstream`); the 503-probe asserts the 5 standard HTTP/1.1 headers per 10 REVIEW
> M1. The in-process synthetic backend reuses the `health-aware-http1-backend` helper
> binary (Task 4).

- [ ] **Step 1: Create `crates/envoy-bin/tests/upstream_active_health_check.rs`**

```rust
//! In-process backstop for active HTTP health checking, exercised over an
//! HTTP/1.1 listener. Complements the H1 differential fixture 0019 with
//! cheap H1-codec coverage of BOTH convergence directions:
//!   - healthy:   in-process backend `/healthz` → 200 ⇒ after settle, GET / → 200
//!     through to the backend body
//!   - unhealthy: in-process backend `/healthz` → 503 ⇒ after settle, GET / → 503
//!     with body `no healthy upstream` + 5 standard HTTP/1.1 headers
//!
//! Per phase-09 REVIEW M3 disposition + SPEC §6.4: uses
//! `tokio::process::Command` with `.kill_on_drop(true)`, `stdout: Stdio::null()`,
//! and `stderr: Stdio::piped()` for diagnostics. Discipline copied verbatim from
//! the phase-10/11 `http_filter_*.rs` backstop precedents.
//!
//! On the 503 probe the backstop asserts the per-probe standard HTTP/1.1
//! header presence (10 REVIEW M1 lesson; the 5 headers `{server, date,
//! content-length, content-type, connection}`).

#![forbid(unsafe_code)]

use std::io::Write;
use std::net::{SocketAddr, TcpListener as StdListener};
use std::process::Stdio;
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const SETTLE_MS: u64 = 3500;

fn reserve_port() -> u16 {
    let l = StdListener::bind(("127.0.0.1", 0)).unwrap();
    let p = l.local_addr().unwrap().port();
    drop(l);
    p
}

async fn wait_ready(addr: SocketAddr, budget: Duration) -> std::io::Result<()> {
    let deadline = Instant::now() + budget;
    let mut delay = Duration::from_millis(50);
    loop {
        match TcpStream::connect(addr).await {
            Ok(_) => return Ok(()),
            Err(_) if Instant::now() < deadline => {
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_millis(500));
            }
            Err(e) => return Err(e),
        }
    }
}

async fn http1_get(addr: SocketAddr, path: &str) -> (u16, Vec<(String, String)>, Vec<u8>) {
    let mut stream = tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(addr))
        .await
        .expect("connect timeout")
        .expect("connect");
    let req = format!("GET {path} HTTP/1.1\r\nHost: hc_backend\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes()).await.expect("write");
    let mut buf = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut buf))
        .await
        .expect("read timeout")
        .expect("read");
    let head_end = buf
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("\\r\\n\\r\\n");
    let head = std::str::from_utf8(&buf[..head_end]).expect("utf8");
    let mut lines = head.split("\r\n");
    let status_line = lines.next().expect("status");
    let status: u16 = status_line.split_whitespace().nth(1).unwrap().parse().unwrap();
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| {
            let (n, v) = l.split_once(": ")?;
            Some((n.to_ascii_lowercase(), v.to_string()))
        })
        .collect();
    let body = buf[head_end + 4..].to_vec();
    (status, headers, body)
}

/// Boot envoy-bin with a synthesized bootstrap pointing at `backend_port`.
async fn spawn_envoy_bin(listener_port: u16, backend_port: u16) -> tokio::process::Child {
    let bootstrap = format!(
        r#"
static_resources:
  listeners:
    - name: ingress_http
      address:
        socket_address:
          address: 127.0.0.1
          port_value: {listener_port}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                codec_type: HTTP1
                stat_prefix: ingress_http
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: local
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          route: {{ cluster: hc_backend }}
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: hc_backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      common_lb_config:
        healthy_panic_threshold: {{ value: 0 }}
      health_checks:
        - timeout: 1s
          interval: 1s
          healthy_threshold: 1
          unhealthy_threshold: 1
          http_health_check:
            path: /healthz
            expected_statuses:
              - {{ start: 200, end: 201 }}
      load_assignment:
        cluster_name: hc_backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: {{ socket_address: {{ address: 127.0.0.1, port_value: {backend_port} }} }}
admin:
  address:
    socket_address: {{ address: 127.0.0.1, port_value: 0 }}
"#
    );
    let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
    tmp.write_all(bootstrap.as_bytes()).expect("write bootstrap");
    let path = tmp.path().to_path_buf();
    // Persist tempfile by leaking it; the test process exits shortly.
    std::mem::forget(tmp);
    tokio::process::Command::new(env!("CARGO_BIN_EXE_envoy-bin"))
        .arg("-c")
        .arg(&path)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn envoy-bin")
}

async fn spawn_backend(port: u16, healthz_status: u16) -> tokio::process::Child {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .join("tests/helpers/health-aware-http1-backend/Cargo.toml");
    tokio::process::Command::new(env!("CARGO"))
        .arg("run")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(&manifest)
        .arg("--")
        .arg("--port")
        .arg(port.to_string())
        .arg("--healthz-status")
        .arg(healthz_status.to_string())
        .arg("--data-status")
        .arg("200")
        .arg("--data-body")
        .arg("ok\n")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn helper backend")
}

#[tokio::test]
async fn unhealthy_endpoint_returns_synth_503_no_healthy_upstream() {
    let listener_port = reserve_port();
    let backend_port = reserve_port();
    let _backend = spawn_backend(backend_port, 503).await;
    wait_ready(
        ("127.0.0.1", backend_port).into(),
        Duration::from_secs(10),
    )
    .await
    .expect("backend ready");
    let _envoy = spawn_envoy_bin(listener_port, backend_port).await;
    wait_ready(
        ("127.0.0.1", listener_port).into(),
        Duration::from_secs(10),
    )
    .await
    .expect("envoy-bin ready");

    // Settle past active-HC convergence (≥ interval + timeout + margin).
    tokio::time::sleep(Duration::from_millis(SETTLE_MS)).await;

    let (status, headers, body) = http1_get(("127.0.0.1", listener_port).into(), "/").await;
    assert_eq!(status, 503, "no-healthy-upstream synth 503");
    assert_eq!(body, b"no healthy upstream", "ADR-0037 body bytes");
    // 10 REVIEW M1: 5 standard HTTP/1.1 header presence assertion.
    for required in ["server", "date", "content-length", "content-type", "connection"] {
        assert!(
            headers.iter().any(|(n, _)| n == required),
            "missing standard header {required}; got {headers:?}"
        );
    }
    let cl = headers
        .iter()
        .find(|(n, _)| n == "content-length")
        .map(|(_, v)| v.as_str())
        .unwrap();
    assert_eq!(cl, "19", "content-length matches body bytes");
}

#[tokio::test]
async fn healthy_endpoint_passes_through_to_backend() {
    let listener_port = reserve_port();
    let backend_port = reserve_port();
    let _backend = spawn_backend(backend_port, 200).await;
    wait_ready(
        ("127.0.0.1", backend_port).into(),
        Duration::from_secs(10),
    )
    .await
    .expect("backend ready");
    let _envoy = spawn_envoy_bin(listener_port, backend_port).await;
    wait_ready(
        ("127.0.0.1", listener_port).into(),
        Duration::from_secs(10),
    )
    .await
    .expect("envoy-bin ready");

    // Settle past healthy-convergence (the healthy_threshold=1 transition
    // fires after the first successful probe).
    tokio::time::sleep(Duration::from_millis(SETTLE_MS)).await;

    let (status, _headers, body) = http1_get(("127.0.0.1", listener_port).into(), "/").await;
    assert_eq!(status, 200, "pass-through to healthy backend");
    assert_eq!(body, b"ok\n", "backend data-path body");
}
```

  Add the `tempfile` dev-dep to `crates/envoy-bin/Cargo.toml` if not already present (the
  phase-09/10/11 backstops use it):

```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: Run the backstop tests**

Run: `cargo test -p envoy-bin --test upstream_active_health_check`
Expected: BOTH PASS (each test takes ~5s — backend boot + envoy-bin boot + 3.5s settle +
probe round-trip).

- [ ] **Step 3: clippy + fmt + commit**

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Expected: clean.
Run: `cargo fmt --all -- --check`
Expected: clean.

```bash
git add crates/envoy-bin/tests/upstream_active_health_check.rs crates/envoy-bin/Cargo.toml
git commit -m "phase 12.2: task 6 — D7.3 in-process H1 backstop (both directions; 5-header presence assertion on 503 per 10 REVIEW M1)"
```

---

## Task 7: D-corpus fuzz seed (parse_bootstrap; corpus 19 → 20)

**Files:**
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_upstream_active_health_check.yaml`
- Modify: `crates/envoy-config/fuzz/.gitignore` (allow-list)
- Modify: `crates/envoy-config/src/bootstrap.rs` (`fuzz_corpus_seeds_parse_or_reject_cleanly`
  SUCCESS array)

> Per the 09/10/11/12.1 Task-6 lesson: the new seed file, the `.gitignore` allow-list entry,
> AND the SUCCESS-array extension land in the **same commit**. Genuinely non-redundant vs
> 12.1's `cluster_health_check.yaml` seed — that seed is a header-only HC bootstrap; this
> one exercises the FULL fixture-0019 shape (HCM + router + HC-configured cluster +
> panic-disabled + route gating).

- [ ] **Step 1: Create the seed** at
  `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_upstream_active_health_check.yaml`

```yaml
static_resources:
  listeners:
    - name: ingress_http
      address:
        socket_address:
          address: 127.0.0.1
          port_value: 10000
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                codec_type: HTTP1
                stat_prefix: ingress_http
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: local
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: hc_backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: hc_backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      dns_lookup_family: V4_ONLY
      common_lb_config:
        healthy_panic_threshold: { value: 0 }
      health_checks:
        - timeout: 1s
          interval: 1s
          healthy_threshold: 1
          unhealthy_threshold: 1
          http_health_check:
            path: /healthz
            expected_statuses:
              - { start: 200, end: 201 }
      load_assignment:
        cluster_name: hc_backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: localhost
                      port_value: 7000
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
```

- [ ] **Step 2: Allow-list it** — append to `crates/envoy-config/fuzz/.gitignore` (after the
  `cluster_health_check.yaml` line from 12.1):

```
!corpus/parse_bootstrap/hcm_upstream_active_health_check.yaml
```

- [ ] **Step 3: Extend the SUCCESS array** in `bootstrap.rs`
  `fuzz_corpus_seeds_parse_or_reject_cleanly` (the success-seed slice; after
  `"fuzz/corpus/parse_bootstrap/cluster_health_check.yaml",`):

```rust
            "fuzz/corpus/parse_bootstrap/hcm_upstream_active_health_check.yaml",
```

- [ ] **Step 4: Verify the seed is not gitignored + the SUCCESS array test passes**

Run: `git check-ignore crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_upstream_active_health_check.yaml`
Expected: exit non-zero (NOT ignored — the `.gitignore` allow-list entry won).

Run: `cargo test -p envoy-config fuzz_corpus_seeds_parse_or_reject_cleanly`
Expected: PASS (20 success seeds now).

- [ ] **Step 5: commit**

```bash
git add crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_upstream_active_health_check.yaml \
        crates/envoy-config/fuzz/.gitignore crates/envoy-config/src/bootstrap.rs
git commit -m "phase 12.2: task 7 — D-corpus parse_bootstrap seed hcm_upstream_active_health_check.yaml (corpus 19->20)"
```

---

## Task 8: state-4 phase-done verification + STATE advance to state-5-next

**Files:**
- Modify: `docs/envoy-rust/phases/12.2-active-http-probe-and-fixture/PROGRESS.md`
  (per-task narrative + the §7.5 gate evidence)
- Modify: `docs/envoy-rust/STATE.md` (advance to state-5-next)

> This is the state-4 verification task per `BOOTSTRAP_PROMPT.md` §7.5 + the 05.3 → 12.1
> evidence-discipline chain. It is docs-only (no code). Run every gate, quote the output
> into PROGRESS, then advance STATE.

- [ ] **Step 1: Run the 5 stable-toolchain gates and capture output**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace
cargo deny check
```
Expected: all clean. Quote the `test result:` line + clippy `Finished` + deny summary into
PROGRESS.

- [ ] **Step 2: Run the 19 Docker-gated differential fixtures simultaneously**

```bash
cargo test -p differential -- --include-ignored
```
Expected: all 19 fixtures (`0001-tcp-echo` → `0019-upstream-active-health-check`) green
bilaterally vs `envoyproxy/envoy:v1.33.0`. Quote the pass count.

- [ ] **Step 3: Confirm h2spec ≥95% held** (gate (c)). 12.2 touches no H2 framing path
  (the fixture + backstop are H1; the synth-503 helper is in `envoy-http1`; the new
  `envoy-health` crate is H1-only) — the gate holds vacuously at the parent-05 baseline
  99.31%. Re-run locally if h2spec available; otherwise rely on CI.

- [ ] **Step 4: Run the `parse_bootstrap` fuzz target on the 20-seed corpus**

```bash
cd crates/envoy-config && cargo +nightly fuzz run parse_bootstrap -- -runs=200000 ; cd -
```
Expected: clean (no crash). Quote the iteration count. (If nightly/cargo-fuzz unavailable
locally, the `fuzz_corpus_seeds_parse_or_reject_cleanly` unit test + CI cover this; note
in PROGRESS.)

- [ ] **Step 5: Push and confirm CI green.** Push the branch; confirm the single CI run
  lights gates (a)–(e) simultaneously. Capture the run ID + HEAD SHA + completion
  timestamp into PROGRESS per the 05.3 → 12.1 evidence discipline.

```bash
git push
gh run list --branch "$(git branch --show-current)" --limit 1
```

- [ ] **Step 6: Advance STATE.md.** Set Active phase status to `12.2 lifecycle state
  4-complete / state-5-next (implementation verified; REVIEW.md pending)`; set Next
  expected skill to `superpowers:requesting-code-review` (state 5). Append a `### Phase-12.2
  state-3 execution arc` Notes subsection summarizing Tasks 1-8 + the gate evidence.
  Preserve all prior subsections verbatim per D-3.5 + D-3.4.

- [ ] **Step 7: Commit the state-4 verification**

```bash
git add docs/envoy-rust/phases/12.2-active-http-probe-and-fixture/PROGRESS.md \
        docs/envoy-rust/STATE.md
git commit -m "phase 12.2: task 8 — state-4 phase-done verification + STATE advance to state-5-next"
```

---

## Self-Review (PLAN-writer's checklist, run against the 12.2 SPEC)

**1. Spec coverage.** D4 (active-HC probe task + new `envoy-health` crate) → Task 1. D6.2
(`hcm.rs:582`-arm no-healthy-body reconciliation per ADR-0037 — 19 bytes `no healthy
upstream`) → Task 3. D7 (`cluster.<name>.health_check.{attempt,success,failure}` counters +
contract row) → Task 2. D7.1 (synthetic health-aware backend harness primitive — the 06.3
REVIEW I2 down-payment) → Task 4. D7.2 (fixture 0019 + settle-then-probe driver
`Driver::Http1AfterSettle` + Docker wrapper) → Task 5. D7.3 (in-process backstop, both
convergence directions + 503-probe header-presence assertion per 10 REVIEW M1) → Task 6.
D-corpus fuzz seed (corpus 19 → 20) → Task 7. State-4 verification (19 fixtures green
simultaneously + 5 stable gates + h2spec ≥95% vacuous + fuzz on the 20-seed corpus) → Task
8. The M2 single-writer-contract API-boundary comment (the 12.1 REVIEW M2 forward
dependency) → Task 1 lock-in #6 fold-in. The M4 BEHAVIOR_CONTRACT Equivalence-cell
self-containment edit → Task 2 lock-in #7 fold-in. The §6.2 6-item findings are baked into
the PLAN lock-ins (lock-in #3) — NOT re-verified at PLAN-write per the parent-12 split
locked-facts discipline. All 12.2 SPEC §3 deliverables mapped.

**2. Placeholder scan.** Every code step shows complete code; every command shows expected
output. The one PLAN-writer deferral is at Task 5 Step 8 (hooking fixture 0019's backend
selection into the harness backend dispatcher) where the exact edit site is left to be
located at task time per the harness's per-phase evolution — the principle is locked
("follow the `0008-http1-router-upstream` `Http1EchoBackend` arm verbatim, substituting
`HealthAwareHttp1Backend`") but the exact line number is harness-evolution-dependent.

**3. Type consistency.** `envoy_health::Scheduler::spawn(&Bootstrap, Arc<ClusterManager>,
Arc<StatsRegistry>, CancellationToken) -> Result<Scheduler, HealthError>` — used identically
in Task 1 Step 9 (envoy-bin wiring) and Task 1 Steps 4 & 6 (definition).
`ClusterHandle::health_probe_targets() -> Option<Vec<(SocketAddr, Arc<EndpointHealth>)>>` —
defined in Task 1 Step 7, consumed in Task 1 Step 6 (`scheduler.rs`'s `Scheduler::spawn`).
`probe_loop(addr, host, path, probe_timeout, interval_dur, expected_statuses,
endpoint_health, attempt, success, failure, cancel)` 11-arg signature — defined in Task 1
Step 5 (`probe.rs`), called in Task 1 Step 6 (`scheduler.rs`). `synth_no_healthy_upstream(close:
bool) -> Response` — defined in Task 3 Step 3 (helper), called in Task 3 Step 4
(`hcm.rs:582`). `Driver::Http1AfterSettle { settle_ms, method, path, host, expected_status,
expected_body, expected_headers }` — defined in Task 5 Step 1, used in Task 5 Step 5
(`expectations.yaml`) + Task 5 Step 2 (dispatch arm). `HealthAwareHttp1Backend::{spawn,
port, container_host}` — defined in Task 4 Step 4, would be consumed in Task 5 Step 8
(harness backend dispatcher). `HealthError::{StatsRegistration, InvalidDuration}` —
defined in Task 1 Step 4, mapped from `register_counter` + `parse_duration` calls in Task 1
Step 6. The 3 counter names `cluster.<n>.health_check.{attempt,success,failure}` — used
identically in Task 1 Step 6 (registration), Task 2 Step 1 (test assertion), Task 2 Step 3
(BEHAVIOR_CONTRACT row).

---

*End of 12.2 PLAN. 8 tasks; ~1000-1200 LoC; no split; no ADR. Closes parent-12 at state-6
+ closes the first Upstream-robustness-family phase + lands the first periodic-background
primitive + lands the first synthetic-backend harness primitive (06.3 REVIEW I2 down-payment).*
