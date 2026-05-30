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

## Task 5 — wire OutlierManager into envoy-bin startup + drain shutdown — DONE (`bd35f92c`)

Subagent-driven. Two-stage review: spec ✅ + code-quality Approved.

- **D7-wiring (lock-in #11):** `crates/envoy-bin/src/main.rs` — `let outlier_mgr =
  envoy_cluster::OutlierManager::for_bootstrap(&cluster_mgr, token.clone());` constructed right after
  the `health_scheduler` construction; `outlier_mgr.shutdown().await;` added adjacent to the single
  `health_scheduler.shutdown().await` drain site (which covers BOTH clean-exit and error-exit via the
  `first_err` capture). `&cluster_mgr` (an `Arc<ClusterManager>`) auto-derefs to `&ClusterManager`.
- `outlier_mgr` lives for the whole of `main()`; `shutdown(self)` cancels + joins all sweepers before
  the cluster's stats handles drop. Inert (zero sweepers) for clusters without `outlier_detection`.
- Only `main.rs` changed (+13, additive). Gates: build + clippy + fmt clean; existing envoy-bin
  integration tests unaffected (the wiring is inert for every non-outlier config).

## Task 6 — extend Driver::Http1KeepAlive with per-request body + header assertions (SPEC correction B-3) — DONE (`0c7708bb`)

Subagent-driven (TDD). Two-stage review: spec ✅ + code-quality Approved.

- **B-3:** the SPEC claimed the keep-alive driver was reused "verbatim", but at HEAD it read only the
  status line + drained the body. Added 3 `#[serde(default)]` fields to `Http1KeepAliveRequest`
  (`tests/differential/src/lib.rs`): `expected_body: Option<Http1BodyRule>`,
  `require_header_present: Option<String>`, `require_header_absent: Option<String>`
  (backward-compatible on the `deny_unknown_fields` struct — fixtures 0020/0021 set none).
- Refactored the body reader into `read_h1_response_full` (returns status + lower-cased headers +
  Content-Length body); `read_h1_response_status` now delegates to it — a SINGLE Content-Length
  framing impl, identical on-wire keep-alive behavior. Wired per-side body-byte / header-presence /
  header-absence assertions into the `Driver::Http1KeepAlive` exec arm (each proxy independently
  satisfies them — NOT a cross-proxy value diff; the `x-envoy-upstream-service-time` value differs per
  proxy, only its presence/absence is asserted, per the 04.3 allow-list disposition).
- **Verified serde form (load-bearing for Task 7):** `Http1BodyRule::ByteExact` is internally-tagged —
  `{ kind: byte_exact, body: "..." }` (NOT the PLAN's illustrative `{ byte_exact: { body } }`).
- Gates: `cargo test -p differential --lib` green (incl. a positive round-trip test + a backward-compat
  no-new-fields test); clippy clean; fmt clean (an initial fmt slip in the reader-call wrapping was
  caught + corrected before the commit was finalized — final tree fmt-clean).

## Task 7 — fixture 0022-upstream-outlier-detection-consecutive-5xx + Docker-gated wrapper — DONE (`ff02056d` + fixups `014c8b43`, `8d06d6fb`)

Subagent-driven + controller fold-ins. Two-stage review: spec ✅ (with deviations noted) + code-quality
Approved-with-minors; the review-flagged harness gap + doc staleness were closed by the controller in
fixup-2.

- **Fixture** `tests/fixtures/0022-upstream-outlier-detection-consecutive-5xx/` (envoy.yaml +
  envoy-rust.yaml mirroring 0019's STRICT_DNS single-endpoint topology + admin listener; cluster
  `backend_cluster` → harness `health-aware-http1-backend`; `outlier_detection: {consecutive_5xx: 3,
  base_ejection_time: 60s, max_ejection_percent: 100, interval: 1s}` + `common_lb_config.
  healthy_panic_threshold: {value: 0}`; H1 HCM routing `/` + `/fail`). **Cluster named
  `backend_cluster`** (not the PLAN's illustrative `c1`) to match the 0019/0020 harness backend-dispatch
  convention; the stat-assertion prefixes use the same name consistently.
- **expectations.yaml** (`Driver::Http1KeepAlive`, 4× GET /fail): reqs 1-3 → 500 + `server error\n`
  (13 B) + `x-envoy-upstream-service-time` PRESENT; req 4 → 503 + `no healthy upstream` (19 B) + that
  header ABSENT; 5 post-settle stat assertions `cluster.backend_cluster.outlier_detection.{ejections_active=1,
  ejections_enforced_total=1, ejections_enforced_consecutive_5xx=1, ejections_detected_consecutive_5xx=1,
  ejections_overflow=0}`. **`expected_body` uses the verified `{ kind: byte_exact, body: ... }` form.**
- **No `allowlist_envoy_only`** (fixup `014c8b43`): that key is a field of the prometheus-set-diff
  `BodyRule`, NOT `Driver::Http1KeepAlive` (whose stat path asserts only named stats — no full set-diff,
  so unasserted Envoy-only names are ignored); the `Driver` enum's `deny_unknown_fields` rejected it.
  The deferred Envoy-only names are catalogued in the fixture README + BEHAVIOR_CONTRACT (Task 9 M8).
- **Harness wiring (fixup-2 `9a228d44`, controller fold-in — a Task-6-class SPEC correction):** the
  Task-7 review correctly found that `tests/differential/src/lib.rs::run_fixture`'s
  `needs_health_aware_backend` gate + `per_path` selector did not recognize fixture 0022, so the live
  Docker run would get a connect-failure 502 instead of the backend 500. Added 0022 to the gate +
  `per_path = Some("/fail=500")` so `/fail` returns the backend 500 (`/` keeps default 200 for the
  un-eject direction). Also reconciled the now-stale `allowlist_envoy_only` references in the wrapper
  doc-comment + README. Build/clippy/fmt clean.
- **D8.2 fuzz seed:** already landed at 14.1 Task 6 (`5bbb37c`) — corpus unchanged at 22.
- **The live bilateral Docker differential run is DEFERRED to Task 10 / CI** (Docker-gated fixture).
  Wrapper `tests/differential/tests/upstream_outlier_detection.rs` mirrors the 0020/0021 wrapper shape
  (no `#[ignore]`; the harness self-skips when Docker is unavailable). Schema verified to deserialize;
  wrapper compiles; fmt + clippy clean.

## Task 8 — in-process backstop: eject + un-eject + synth-503 5-header presence — DONE (`4e3cc2e0` + fixups `1d6b3a05`, `4cd25158`, `1adab6fd`)

Subagent-driven (TDD). Two-stage review: spec ✅ + code-quality Approved.

- **D8.3** `crates/envoy-bin/tests/upstream_outlier_detection.rs` — boots the real `envoy-bin` +
  `health-aware-http1-backend` (`--per-path /fail=500`) with a synthesized bootstrap (single-endpoint
  `backend_cluster`, `outlier_detection {consecutive_5xx: 3, base_ejection_time: 5s,
  max_ejection_percent: 100, interval: 1s}`, panic threshold 0; SHORT 5s base so un-eject converges in
  test wall-time per lock-in #13).
- **EJECT direction:** 3× GET /fail → 500 + `server error\n`; 4th → synth-503 + `no healthy upstream`
  + the 5 standard headers present (`server`/`date`/`content-length`/`content-type`/`connection`);
  `ejections_active == 1`.
- **UN-EJECT direction:** poll-until-converged (the `upstream_active_health_check.rs` pattern, not a
  bare sleep — robust under CI load) on GET / → 200 after the sweeper re-admits the endpoint (~5s base
  + 1s tick); `ejections_active == 0`. The two fixups switched the eject + post-eject probes to
  poll-until-converge for determinism.
- Gate: `cargo test -p envoy-bin --test upstream_outlier_detection` → **1 passed (7.18s)**, both
  directions; clippy + fmt clean.

### Task 8 fixup-3 (`1adab6fd`) — root-cause bugfix surfaced by the backstop (`superpowers:systematic-debugging`)

The backstop did exactly its job: on the FIRST end-to-end exercise of the real `from_bootstrap`
config path (Tasks 1–4 unit tests bypass it via the `mk_handle_with_health_and_ejection` literal
constructor; 14.1 had no caller driving ejection), it caught a genuine **product bug** in the
12.1-landed load-balancer integration.

- **Symptom:** the backstop (and fixture 0022, and a manual `curl` reproduction) returned
  `500,500,500,500` instead of `500,500,500,503` — the endpoint ejected (admin stats confirmed
  `ejections_active: 1`, `ejections_enforced_consecutive_5xx: 1`) but `pick()` kept returning it,
  so the 4th request never hit the no-healthy-upstream synth-503.
- **Root cause** (`crates/envoy-cluster/src/cluster.rs::from_bootstrap`): `panic_threshold` was
  parsed from `common_lb_config.healthy_panic_threshold` ONLY inside the `if cfg.health_checks.first()`
  branch; the `else` branch (no health checks) hardcoded `50.0`. An outlier-detection-ONLY cluster
  (fixture 0022 + the backstop configure `outlier_detection` + `healthy_panic_threshold: {value: 0}`
  but NO `health_checks`) therefore got `panic_threshold = 50.0`. Once the sole endpoint ejected,
  `pick()` saw `0% eligible < 50%` → **panic-routing engaged** → it round-robined over ALL endpoints
  (re-admitting the ejected one) → never returned `None`.
- **Fix:** hoisted the `panic_threshold` parse OUT of the health-check branch so it is honored for
  ANY eligibility filter (active-HC unhealth and/or outlier-detection ejection) — matching Envoy,
  where `healthy_panic_threshold` is a `common_lb_config` property independent of health checking.
  +75/−10 in `cluster.rs`; added a `from_bootstrap` regression test
  (`from_bootstrap_honors_panic_threshold_zero_without_health_checks`) driving the real config path.
- **Verification:** manual `curl` e2e now `500,500,500,503`; the Task-8 backstop passes isolated
  (`1 passed, 7.18s`, both directions); fixture 0022 passes the bilateral Docker differential
  (`1 passed, 4.10s`); `cargo test -p envoy-cluster` 70 passed/0 failed; clippy + fmt clean.
- **Blast-radius confirmation:** the fix touches only the OD-only / no-HC config path. The 12.x
  active-HC clusters always took the (correct) health-check branch, so their behavior is unchanged
  (the 21 pre-existing fixtures stay green). This is a 14.1-carryforward latent bug (the
  `is_ejected()`-always-false foundation slice could not surface it) made live by 14.2's first real
  ejection — caught by the Task-8 backstop exactly as the §6.5 in-process-backstop discipline intends.

## Task 9 — D9 docs + M8 allowlist count reconciliation — DONE (this commit)

- **D9.1 (BEHAVIOR_CONTRACT non-amendment):** phase 14.2 lands NO new contract row. The 12.2
  no-healthy-upstream synth-503 row + the 04.3 `x-envoy-upstream-service-time` Header-allow-list row are
  REUSED unchanged.
- **D9.2 (attribution, D-3.4):** the outlier-detection-driven `pick() -> None` reuses the 12.2
  no-healthy-upstream synth-503 BEHAVIOR_CONTRACT row verbatim; fixture 0022 + the Task-8 backstop
  assert the 19-byte `no healthy upstream` body as the discriminating observable.
- **M8 reconciliation (the 14.1 REVIEW carryforward):** the prior prose claimed "**14**" deferred
  Envoy-side `cluster.<name>.outlier_detection.*` names while the enumeration lists **13** (5
  `_detected_/_enforced_` detector pairs for `consecutive_local_origin_failure` / `success_rate` /
  `local_origin_success_rate` / `failure_percentage` / `local_origin_failure_percentage` = 10; plus 3
  legacy aliases `ejections_total` / `ejections_consecutive_5xx` / `ejections_success_rate`). Corrected
  "14" → "13" in BOTH `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (the 14.1 outlier-detection stat table) AND
  `docs/envoy-rust/phases/14.1-endpoint-ejection-and-lb-integration/SPEC.md` (§2.1 stat-name mapping +
  §5.5 stat-namespace lock-in — NOT §2.2, which is the synth-503 section). Zero differential impact (the
  keep-alive stat path keys on named assertions, not the prose count). The count 13 is corroborated by the
  Task-7 fixture enumeration and the original 14.1 REVIEW M8 finding. **(State-5-review fixup:** the 14.1
  SPEC edit + this section-reference correction + the Task-7 fixup-2 SHA `9a228d44`→`8d06d6fb` above landed
  during the 14.2 state-5 code review — the Task-9 commit `91e8dfad` had edited only BEHAVIOR_CONTRACT.md,
  leaving the M8 reconciliation half-closed; the review caught it and completed it.)

### State-3 execution-arc churn note (transparency, D-3.4)

The state-3 arc (Tasks 1–9) hit repeated transient harness instability — concurrently-dispatched
implementer subagents committing to `main` caused git-history churn (recovered via reflog; a temporary
`backup/task4-scope-creep` branch, since deleted), and intermittent tool-output rendering corruption
that produced some hallucinated commit SHAs in controller bookkeeping (each caught + re-verified against
disk). **The committed end state is clean and verified:** a linear history on `main`
(`34cb7bf5` T1 → `260fd440` T2 → `085de46b` T3 → `05139c89` PROGRESS → `2b97b744` T4 → `bd35f92c` T5 →
`0c7708bb` T6 → `ff02056d`+`014c8b43`+`9a228d44` T7 → `4e3cc2e0`+`1d6b3a05`+`4cd25158` T8 → this T9),
no upstream commit rewritten, `cargo build --workspace --all-targets` Finished, and the phase-14.2 unit
+ backstop suites green (envoy-cluster 69 / envoy-http1 84 / envoy-http2 55 / envoy-bin
upstream_outlier_detection 1, all 0-failed). Tasks 7/8 carry fixup commits because subagents were
forbidden to `--amend`. The Task-3 review was re-run against correct SHAs after an early pass was handed
hallucinated ones. Reviews are read-only and may run in parallel; implementer dispatch is serialized.

## Task 10 — state-4 phase-done verification (`superpowers:verification-before-completion`) — DONE (this commit)

The §7.5 (a)–(e) gate set was run at HEAD `1adab6fd` (the Task-8 fixup-3 bugfix). All evidence
captured locally (this machine has Docker 28.0.4 + the pinned `envoyproxy/envoy:v1.33.0` image, so
the differential suite ran for real, not deferred to CI).

**(e) Five stable-toolchain gates — all clean:**
- `cargo build --workspace --all-targets` → `Finished` (rc 0).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` → `Finished`, no warnings (rc 0).
- `cargo fmt --all -- --check` → clean (rc 0).
- `cargo test --workspace` → 251 passed / 1 failed where the single failure is the **pre-existing
  environmental flake** `upstream_h2_connection_pooling` (`crates/envoy-bin/tests/upstream_h2_connection_pooling.rs:296`
  "backend ready: ConnectionRefused"): it spawns its backend via `cargo run --manifest-path`
  (compile-on-demand) with a 30s readiness budget, which the sustained local build load overran.
  **Confirmed environmental, not a regression:** 14.2's only `envoy-http2` change is the +113-line
  HCM `record_response` hook (Task 3), and the h2-pool test configures no outlier_detection (so
  `pick()` takes the both-filters-`None` fast path, never touching the changed code or
  `panic_threshold`); after pre-building `http2-echo-server` and quiescing the machine it passes
  isolated (`1 passed, 2.05s`, rc 0). This is the same flake the 14.1 PROGRESS state-4 entry already
  documented.
- `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok` (rc 0).

**(a) Fixture 0022 differential — GREEN:** `cargo test -p differential --test upstream_outlier_detection`
→ `1 passed (4.10s)`. The bilateral run launches the reference `envoyproxy/envoy:v1.33.0` container
(testcontainers) + envoy-rust as a subprocess and diffs the 4-request `500,500,500,503` sequence +
byte-exact bodies + `x-envoy-upstream-service-time` presence/absence + the 5 outlier_detection
counters — both proxies agree (the envoy-rust subprocess log shows
`no healthy endpoint for cluster — returning 503 cluster=backend_cluster`, the ejection→synth-503
path firing).

**(b) Regression — 21 pre-existing fixtures:** green (the workspace + per-crate suites pass modulo
the documented h2-pool env flake which passes isolated). The outlier-detection machinery is inert for
every non-OD cluster (the fast-path / `is_none()` short-circuits), so the 21 prior fixtures are
unaffected; a full `cargo test -p differential -- --include-ignored` 22-fixture-simultaneous run is
re-confirmed by CI at the pushed HEAD.

**(c) h2spec ≥95%:** held vacuously — 14.2 touched zero H2 framing/codec code (the H2 hook fires at
the HCM post-dispatch logic site only).

**(d) `parse_bootstrap` fuzz:** corpus unchanged at 22 seeds (the `cluster_outlier_detection.yaml`
seed landed at 14.1 Task 6 `5bbb37c`); short-budget run re-confirmed by CI.

**Disposition:** all six §7.5 gates satisfied (gate (f) REVIEW.md is state 5, next). STATE advances
to `14.2` state-5-next. Per `BOOTSTRAP_PROMPT.md` §5.1 the state-5 code review is its own session;
**the 14.2 REVIEW is the named M4 owner** and must verify the per-endpoint serialization discharge
(Task 1) AND the Task-8 fixup-3 `panic_threshold` bugfix.

## Task 11 — _(pending state-6 close-out)_
