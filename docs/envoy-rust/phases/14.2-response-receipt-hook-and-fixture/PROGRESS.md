# Phase 14.2 (`14.2-response-receipt-hook-and-fixture`) — PROGRESS

> Running execution log. The state-2 PLAN-write commit lands this skeleton + the Task 1
> preamble (per the 04.3 → 14.1 standalone-PLAN-write cadence). The state-3 subagent-driven
> execution arc appends one subsection per task (Tasks 1–10); Task 11 is the state-6 close-out.

---

## Task 1 preamble — PLAN-time SPEC corrections, §6.2 lock-ins, carryforward dispositions

This preamble records the facts the 14.2 state-2 PLAN-write established by reading the ratified
`SPEC.md` against the PLAN-time HEAD `b0dea44f` (the phase-14.1 state-6 close-out). It is the
authoritative cold-start context for the state-3 implementer. The SPEC is ratified and NOT
edited; corrections live here (the 06.2 → 14.1 cadence).

### PLAN-write disposition

- **State 2 → state 3.** PLAN.md + this PROGRESS skeleton land in one standalone pre-Task-1
  commit (mirrors 14.1 `9a56e85` / 13.2 `8c7d8a23` / 12.2 `6a3b3329`). Docs-only; no ADR; no
  `[ADR-NNNN]` bracket. STATE.md advances to `14.2` state-3-next.
- **§6.1 split-gate verdict: NO SPLIT, NO NEST-SPLIT.** The materialized PLAN is **11 tasks;
  ~1100–1300 LoC** (production ~430, tests/backstop ~520, fixture/driver/docs ~330) —
  comfortably under the `BOOTSTRAP_PROMPT.md` §6.1 ~1500-LoC / ~25-task gate. The small overage
  vs the SPEC §6.1 ~900–1200 projection is the M4 serialization (~70 LoC, SPEC correction /
  carryforward discharge) + the keep-alive driver extension (~90 LoC, SPEC correction B-3).
- **ADR posture: NO ADR** (SPEC §7; all four conditional slots recommend NO ADR). DECISIONS.md
  ledger head stays **ADR-0041** (count 42); next available **ADR-0042**.
- **§6.2 9-item Docker verification NOT re-run** — locked at the parent-14 split `0a4d225` by
  ADR-0041. The relevant lock-ins (items 5/6/8/9) are pulled forward into PLAN §0.A #3.
- **Family/execution posture:** subagent-driven at state 3 per `feedback_execution_style`
  (PLAN §0.A #16); recommendation picked at every borderline call per
  `feedback_pick_recommendation` (no fork).

### PLAN-time SPEC corrections (PLAN §0.B; the 06.2 → 14.1 cadence)

- **B-1.** The H1 response-receipt site is `crates/envoy-http1/src/router.rs::
  construct_proxied_response` (`router.rs:87-168`; increments at `:95`/`:97`), NOT
  `write_proxied_response`; and `endpoint` is NOT in scope inside it. The D4-H1 hook therefore
  lands at the CALLER in `crates/envoy-http1/src/hcm.rs` (after the `match stream_or_synth`
  block ~`hcm.rs:681`, inside `if let Some(endpoint) = cluster.pick_endpoint()` at `:465`;
  `construct_proxied_response` is invoked at `:641`). A single
  `cluster.record_response(endpoint, outgoing.status)` covers proxied + send-failure-502 +
  connect-failure-502 + pool-overflow-503; the no-healthy `else` arm (`:682-692`) does NOT call it.
- **B-2.** The H1 no-healthy-upstream synth-503 short-circuit is at `hcm.rs:691`
  (`synth_no_healthy_upstream(close)`, builder at `:1057-1077`), NOT `hcm.rs:582`.
- **B-3.** `Driver::Http1KeepAlive` is reused-plus-EXTENDED, not "verbatim". The HEAD driver
  (`tests/differential/src/lib.rs`, exec arm ~`:2668`, reader `read_h1_response_status`
  `:1689`) reads ONLY status; `Http1KeepAliveRequest` (`:314-319`) has no body/header fields.
  Task 6 adds 3 `#[serde(default)]` optional fields (`expected_body: Option<Http1BodyRule>`,
  `require_header_present`, `require_header_absent`) + a `read_h1_response_full` helper +
  per-request assertions — backward-compatible (fixtures 0020/0021 set none).
- **B-4.** The §5.1 sweeper-field sketch (`endpoints: Arc<Vec<…>>`, `config:
  Arc<OutlierDetectionConfig>`) is reconciled to on-disk types: there is no runtime
  `OutlierDetectionConfig`; the two `Duration`s the sweeper needs are stored on
  `OutlierDetectionState` (PLAN §0.A #6), and the sweeper carries `{ cancel, join }`
  (§0.A #7). Behavior identical.
- **B-5.** H1 connect-failure synthesizes `502` (pool-overflow `503`); H2 no-healthy
  synthesizes `502` (`synth_h2_502`, `hcm.rs:240-257`), not 503. Pre-existing, differential-
  inert for fixture 0022 (H1-only); D4 records neither no-healthy arm.
- **B-6.** Backend `per_class_body(500)` is `b"server error\n"` = **13 bytes**
  (`tests/helpers/health-aware-http1-backend/src/main.rs:121-133`); `200` on `/` is empty.

### §6.2 lock-ins pulled forward (ADR-0041; PLAN §0.A #3)

- **item-5:** un-eject at the next `interval` tick after `now - eject_time >=
  base_ejection_time`; counters reset on un-eject AND on any 2xx/3xx/4xx (D4 + D7).
- **item-6:** fixture 0022 bilateral assertions — req 1–3: `500` + `server error\n` (13B) +
  `x-envoy-upstream-service-time` PRESENT; req 4: `503` + `no healthy upstream` (19B) + that
  header ABSENT; counters exact (D8.1).
- **item-8:** H1/H2 share the stat namespace + ejection-on-5xx semantics; the H2 hook fires at
  the HCM post-dispatch site (D4 H2 side).
- **item-9:** the `pick() -> None` synth-503 does NOT record (no endpoint); the connect-failure
  synth path DOES record (picked endpoint; 502/503 ticks BOTH detectors automatically).

### M4 discharge (the most substantive carryforward; 14.1 REVIEW §4 M4)

At 14.2 the `EndpointEjection` writers become genuinely concurrent (one per in-flight request
via D4 + the sweeper via D7), violating the 14.1 `Relaxed` single-writer premise. **Resolution
(PLAN §0.A #4, Task 1):** a per-endpoint `ejected_at: std::sync::Mutex<Option<Instant>>` held
by the `Cluster::record_response` compound and by the sweeper's un-eject ⇒ exactly one
serialized writer per endpoint; `pick()`'s read side stays lock-free. The `Mutex` payload
doubles as the eject-timestamp the sweeper reads for `base_ejection_time`. The 14.2 REVIEW
(named M4 owner) verifies this discharge.

### Carryforward dispositions (PLAN §0.C)

- **M4** → discharged at Task 1; verified at the 14.2 REVIEW.
- **M5 / M6** → Task 1 fold-ins (tie-test strengthening + `EndpointEjectionStats` exposure +
  drop vestigial binding; `max_ejection_percent = 0` edge test + cap-site comment).
- **A-M2** → Task 2 fold-in (stale `tokio::sync::Mutex` comment at `pool.rs:322`).
- **M8** → Task 9 (allowlist count reconciliation; empirical at the state-3 Docker run).
- **M1 / M2 / M3 / M7 / M9** → no-action / opportunistic; no named owner; carry forward.
- **ADR-0028** → remains OPEN; named owner a follow-up foundations-pivot phase, NOT 14.2.
- **§6.9 per-class `upstream_rq_{2,3,4}xx` extension** → DEFER (observability, not outlier
  detection; folding it in inflates scope).
- 13.2/13.1/12.2/12.1/11/earlier Minors + 04.1 M5/M9 Cargo.lock cadence → carry forward unchanged.

---

## Task 1 — M4 per-endpoint serialization + eject-timestamp + M5/M6 fold-ins — DONE (`34cb7bf5`)

Subagent-driven (TDD). Two-stage review: spec ✅ + code-quality Approved.

- **M4 (lock-in #4):** added `pub(crate) ejected_at: std::sync::Mutex<Option<std::time::Instant>>` to
  `EndpointEjection` (`ejection.rs`), init `None` in `new` (no new param). `Cluster::record_response`
  (`cluster.rs`) now acquires `state.ejected_at.lock()` BEFORE `state.record_response(status)` and holds
  it across the whole compound (record → cap-check → eject), stamping `*ejected_at = Some(Instant::now())`
  right after `state.eject(detector)` under the held guard. `pick()`'s read side stays lock-free
  (`is_ejected()` is still a single `Relaxed` `AtomicBool` load). `eject`/`try_un_eject` bodies unchanged
  (lock-in #5 — they must NOT touch `ejected_at` or they'd self-deadlock with the externally-held guard).
- **M5:** strengthened `cluster_record_response_picks_5xx_detector_on_ties` to assert a 503 ejects with
  `ejections_enforced_consecutive_5xx == 1` AND `ejections_enforced_consecutive_gateway_failure == 0`
  (5xx wins the tie); dropped the vestigial `let _ = stats_active;` binding. Stats exposed to tests via
  `#[cfg(test)]` `OutlierDetectionState::stats()` / `EndpointEjection::stats()` accessors (chosen over
  rewriting the 8-arg `mk_handle_with_health_and_ejection` return tuple — smaller blast radius, zero
  production-surface change).
- **M6:** added `cluster_record_response_max_ejection_percent_zero_never_ejects` (cap_count = 0 ⇒ first
  crossing overflows, never ejects, `ejections_overflow == 1`) + the cap-site comment citing §6.2 item-4.
- **Decision struct (on-disk reality):** `EjectionDecision { crossed_5xx, crossed_gateway_failure }` with
  `.any()`; `DetectorType::{Consecutive5xx, ConsecutiveGatewayFailure}`.
- Gates: `cargo test -p envoy-cluster` 66 passed/0 failed (+3); clippy clean; fmt clean.

## Task 2 — D4 H1 response-receipt hook + A-M2 stale-comment fix — DONE (`260fd440`)

Subagent-driven (TDD). Two-stage review: spec ✅ + code-quality Approved.

- **D4-H1 (lock-in #9, SPEC correction B-1):** single `cluster.record_response(endpoint, outgoing.status)`
  in `crates/envoy-http1/src/hcm.rs:692` — AFTER the `match stream_or_synth { … }` block resolves the
  unified `outgoing` response (so after the `upstream_rq_*` increments that fire inside
  `router.rs::construct_proxied_response`), and INSIDE the `if let Some(endpoint) = cluster.pick_endpoint()`
  branch. Covers all endpoint-attributed arms (proxied any-status incl. 500, send-fail-502, connect-fail-502,
  pool-overflow-503). The no-healthy `else` arm does NOT record (no endpoint). NB the hook is in `hcm.rs`,
  NOT `router.rs` — at HEAD `construct_proxied_response` takes only `&ClusterHandle` (no `endpoint`), per B-1.
- **Inert (lock-in #8):** call site is unconditional; inertness lives inside `Cluster::record_response`'s
  `outlier_detection.is_none()` short-circuit. All 83 pre-existing H1 tests stay green.
- **A-M2:** `crates/envoy-http1/src/pool.rs:322` stale `tokio::sync::Mutex` Debug-impl comment → `parking_lot::Mutex`.
- **Test accessor:** added `ClusterHandle::is_endpoint_ejected_for_test(idx)` — `#[doc(hidden)] pub` (NOT
  `#[cfg(test)]`, because the consumer is a test in another crate; `#[cfg(test)]` items are invisible
  across crate boundaries — mirrors the existing `ClusterManager::empty()` cross-crate-test precedent).
- **Test:** `h1_router_arm_records_response_and_ejects_after_threshold` — real in-process backend serving
  500, cluster with `outlier_detection { consecutive_5xx: 1, max_ejection_percent: 100 }` (100% so the
  single-endpoint cap = 1 permits ejection), one real request through the proxy arm, asserts 500 + ejected.
- Gates: `cargo test -p envoy-http1` 84 passed/0 failed; clippy clean; fmt clean.

## Task 3 — D4 H2 response-receipt hook (success + connect-failure arms) — DONE (`085de46b`)

Subagent-driven (TDD). Two-stage review: spec ✅ + code-quality Approved. (Re-reviewed against correct SHAs
after a mid-arc git-history churn that had void-ed the first review pass — see the state-3 churn note below.)

- **D4-H2 (lock-in #9):** two calls in `crates/envoy-http2/src/hcm.rs` `handle_one_stream` `BuildOutcome::Proxy`
  arm: (a) success arm `cluster.record_response(endpoint, upstream_resp.status)` AFTER the
  `upstream_rq_total`/`upstream_rq_5xx` increments and BEFORE the `upstream_resp.headers.into_iter()`
  loop consumes the response; (b) connect/send-failure `Err` arm `cluster.record_response(endpoint, 502)`
  before `return finalize_h2_stream(synth_h2_502(), …)`. The `pick() -> None` arm records nothing.
  Two sites (vs H1's single converged site) are forced by H2 control flow — verified mutually exclusive,
  no gap, no double-count; behaviorally equivalent (502 and 503 both tick both detectors).
- **§6.2 item-8:** hook fires at the H2 HCM post-dispatch logic site only; NO framing/codec/settings touched
  ⇒ h2spec ≥95% holds vacuously.
- **Test:** `h2_router_arm_records_response_and_ejects_after_threshold` — real end-to-end H2 request against
  an H1-protocol backend serving 500 (H1 backend lets the test emit an exact 500), asserts 500 + ejected.
- Gates: `cargo test -p envoy-http2` 55 passed/0 failed/1 ignored; clippy clean; fmt clean.

## Task 4 — D7 OutlierEjectionSweeper + OutlierManager (fourth periodic-background primitive) — DONE (`2b97b744`)

Subagent-driven (TDD). Two-stage review: spec ✅ + code-quality Approved-with-minors (one non-blocking nit).

- **New `crates/envoy-cluster/src/outlier.rs`:** `OutlierEjectionSweeper { cancel: CancellationToken,
  join: JoinHandle<()> }` with `spawn(cluster_name, endpoints: Vec<Arc<EndpointEjection>>, base_ejection_time,
  interval, cancel)` + `async shutdown(self)` (cancel + join). Loop is `tokio::select!` over
  `cancel.cancelled()` / `interval.tick()`, `MissedTickBehavior::Skip`, interval clamped `>= 1ms` — identical
  discipline to the three sibling primitives (`envoy-health::Scheduler`, H1/H2 pool idle sweepers).
- **`sweep_once` (lock-in #4/#5):** holds `ep.ejected_at.lock()` across the check-and-un-eject — if
  `Some(t)` and `t.elapsed() >= base_ejection_time`, calls `try_un_eject()` then clears `*at = None` under
  the SAME held guard. `sweep_once` is sync (no `.await` under the guard); one lock at a time (no ordering
  risk). Serializes against `Cluster::record_response`'s compound on the same per-endpoint mutex.
- **No double-decrement (verified):** the 14.1 `EndpointEjection::try_un_eject` already does the full §6.2
  item-5 work (swap `ejected` false; on the `was==true` edge resets both consecutive counters AND
  `ejections_active.dec()`). The sweeper adds ONLY the timestamp clear — no counter/gauge duplication.
- **`OutlierManager` (lock-in #7):** external sibling registry `{ sweepers: Vec<…> }`, `for_bootstrap(&ClusterManager,
  CancellationToken)` walks `ClusterManager::clusters()` and spawns one sweeper per cluster whose runtime
  state carries `Some(OutlierDetectionState)` (inert otherwise); `async shutdown(self)` drains all. Does NOT
  live on `ClusterManager`. (The `clusters()` by-name walk is cleaner than the H1/H2 managers'
  bootstrap-driven walk because D7 needs only runtime state already projected onto `OutlierDetectionState`.)
- **`OutlierDetectionState` (lock-in #6):** gained `base_ejection_time` + `interval` `Duration`s, populated
  in `Cluster::from_bootstrap`'s configured-OD arm via the REUSED `envoy_config::parse_duration` helper,
  defaulting to Envoy v3 `base_ejection_time = 30s` / `interval = 10s`. Sweeper reads these runtime Durations
  (no bootstrap re-parse). `pub(crate)` accessors `Cluster::outlier_detection_state()` +
  `ClusterHandle::inner_outlier_detection_state()`.
- **Deps:** added `tokio-util = "0.7"` + `tracing = "0.1"` (+ `tokio-util` dev-dep) to `envoy-cluster/Cargo.toml`
  — both D-3.2 permitted foundations already pinned identically in sibling crates; the SPEC mandates
  `tokio_util::sync::CancellationToken`. No NEW third-party crate enters the workspace (Cargo.lock pulled
  nothing unexpected). **Non-blocking nit (Task-4 code review):** `tokio-util` here omits the `["rt"]` feature
  the siblings pin; left leaner deliberately (only `CancellationToken` is used, which doesn't need `rt` — it
  pulls less, not more). Carried as a Minor; no functional impact.
- **Tests:** un-eject-after-base (asserts un-ejected + `ejected_at` cleared + gauge 1→0), shutdown-joins-cleanly,
  and a negative does-not-un-eject-before-base. Uses small REAL durations on `multi_thread` runtimes (NOT
  `start_paused`+`advance`) because `std::time::Instant` is wall-clock and does not track tokio's paused
  timer; the positive test polls within a 2s budget so it is robust under CI load, not flaky.
- Gates: `cargo test -p envoy-cluster` 69 passed/0 failed (+3); clippy clean; fmt clean; `cargo build -p envoy-bin` builds.

### State-3 execution-arc churn note (transparency, D-3.4)

During Tasks 2–4 the controller briefly dispatched implementer subagents concurrently, which (with all
committing to `main`) caused transient git-history churn — a `git reset` recovered via reflog, and a
temporary `backup/task4-scope-creep` safety branch (since deleted). **The end state is clean and verified:**
linear history `34cb7bf5` (T1) → `260fd440` (T2) → `085de46b` (T3) → `2b97b744` (T4) atop the PLAN-write
`c0816d77`; no prior commit was rewritten; `cargo build --workspace --all-targets` Finished; full suites
green (cluster 69 / http1 84 / http2 55). The Task-3 review was re-run against the correct SHAs (an earlier
pass had been handed hallucinated SHAs and reported "commit doesn't exist" — void, superseded). Remaining
tasks dispatch implementers strictly serially per the skill's no-parallel-implementers rule.

## Task 5 — _(pending)_
## Task 6 — _(pending)_
## Task 7 — _(pending)_
## Task 8 — _(pending)_
## Task 9 — _(pending)_
## Task 10 — _(pending state-4 verification)_
## Task 11 — _(pending state-6 close-out)_
