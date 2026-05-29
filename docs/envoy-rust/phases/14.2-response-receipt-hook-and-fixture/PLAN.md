# Phase 14.2 (`14.2-response-receipt-hook-and-fixture`) — PLAN

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development`
> per `feedback_execution_style` auto-memory and per the established 06.x / 07.x / 08.x /
> 09 / 10 / 11 / 12.1 / 12.2 / 13.1 / 13.2 / 14.1 cadence. Tasks 1–11 implement the phase per
> `SPEC.md`. Steps use `- [ ]` checkbox syntax for tracking. One commit per task (Tasks 1–10);
> Task 11 is the state-6 close-out (next session).

**Goal.** Land the **observable-behavior surface** of passive outlier-detection ejection
(parent-14 deliverables D4 / D7 / D8.1 / D8.3 + the D9 docs + the D10 parent-14 close, carved
into 14.2 by ADR-0040). The H1 + H2 router-proxy arms call the 14.1-landed
`Cluster::record_response(endpoint, status)` at the response-receipt site (after the
`upstream_rq_total` + `upstream_rq_5xx` increments fire, before the downstream write); the
new per-cluster `OutlierEjectionSweeper` (the **fourth periodic-background primitive**)
un-ejects past-deadline endpoints on its `interval` tick; fixture `0022` exercises the
end-to-end seam bilaterally (3 backend-500s eject the single endpoint; request 4 returns the
19-byte `no healthy upstream` synth-503). With 14.2 landed, parent phase 14 (outlier
detection) is complete.

**Architecture.** The response-receipt hook fires from `envoy-http1::hcm` and
`envoy-http2::hcm` (the router-proxy arms already hold an `Arc`-equivalent `ClusterHandle`, so
`Cluster::record_response` declared at 14.1 D3/D5 is reachable cycle-free per SPEC §5.1). The
ejection sweeper lives **inside `envoy-cluster`** as a new module `outlier.rs` (mirrors the
13.1 H1 pool sweeper inside `envoy-http1::pool` + 13.2 H2 pool sweeper inside
`envoy-http2::pool`); `OutlierManager` is an external sibling registry to `ClusterManager`
(mirrors `H1PoolManager` / `H2PoolManager` / `envoy-health::Scheduler` verbatim) wired at
`envoy-bin` startup with identical `tokio_util::sync::CancellationToken` cancellation +
`pub async fn shutdown(self)`. **M4 discharge (the load-bearing 14.1-review forward
dependency):** the response-receipt hook fires from EVERY in-flight request task — inherently
multi-writer per endpoint — and the sweeper is a fifth concurrent writer, so the 14.1
`EndpointEjection`'s `Relaxed` atomics are made race-free by a **per-endpoint serialization
lock** (`ejected_at: std::sync::Mutex<Option<Instant>>`, which doubles as the eject-timestamp
store the sweeper reads). The `Cluster::record_response` compound (record → cap-check →
eject) holds that lock; the sweeper holds it around un-eject; `pick()`'s read side stays
lock-free (`is_ejected()` is a single `Relaxed` `AtomicBool` load). **No new crate; no new
top-level Cargo dep; no cycle.**

**Tech Stack.** Zero new top-level Cargo deps. Zero new workspace path-deps. Primitives:
`tokio::time::interval`, `tokio_util::sync::CancellationToken`, `tokio::task::JoinHandle`,
`std::sync::{Arc, Mutex}`, `std::time::{Duration, Instant}`. No `unsafe` (every crate root
keeps `#![forbid(unsafe_code)]`). The H2 hook fires at the H2 HCM post-dispatch site, NOT the
H2 framing/codec path (h2spec ≥95% holds vacuously). The differential harness gains a small,
backward-compatible extension to `Driver::Http1KeepAlive` (per-request body + header
presence/absence assertions — the PLAN-time SPEC correction in §0.B-3). The `parse_bootstrap`
fuzz corpus is UNCHANGED at 22 seeds (D8.2 landed at 14.1 Task 6 `5bbb37c`).

---

## 0.A Architecture lock-ins

These decisions are settled at PLAN-write; subagents implement them as written and do NOT
re-litigate. Numbered for cross-reference from PROGRESS.

1. **No split, no nest-split.** 14.2 is ~11 tasks; ~1100–1300 LoC (production ~430, tests/
   backstop ~520, fixture/driver/docs ~330), comfortably under the `BOOTSTRAP_PROMPT.md` §6.1
   ~1500-LoC / ~25-task gate. The parent-14 split (ADR-0040) already absorbed the over-gate
   scope into 14.1 + 14.2; 14.2 does NOT nest-split. **Standalone PLAN posture per
   `feedback_pick_recommendation`** (no fork). See §6.1 of SPEC: projection ~9–11 tasks /
   ~900–1200 LoC; this PLAN materializes at 11 tasks / ~1100–1300 LoC — the small overage is
   the M4 serialization (~70 LoC) + the keep-alive driver extension (~90 LoC), both PLAN-time
   SPEC corrections (§0.B); still ≪ 1500 LoC. **No split.**

2. **No ADR lands in the 14.2 lifecycle** (SPEC §7 — all four conditional slots A–D
   recommend NO ADR). DECISIONS.md ledger head is **ADR-0041** at 14.2 start; next available
   **ADR-0042**. The M4 serialization choice (a per-endpoint `std::sync::Mutex`) is ordinary
   structure mirroring the 13.x pool's `parking_lot::Mutex` write-serialization; the sweeper
   is the external-sibling-registry pattern established by 12.2 + 13.1 + 13.2. No durable-
   record ADR. A 14.2 ADR lands ONLY if execution surfaces a genuine unforeseen architectural
   constraint (unlikely). The state-6 close-out commit carries `[parent 14 done]` but **NO
   `[ADR-NNNN]` bracket**.

3. **The §6.2 empirical verification is DONE** (parent-14 state-2 commit `0a4d225`; ratified
   by ADR-0041). **Do NOT re-run the §6.2 9-item Docker verification.** The locked facts 14.2
   bakes in (SPEC §6.2):
   - **item-5 (ejection-time semantics):** un-eject at the next `interval` tick after
     `now_monotonic - eject_time >= base_ejection_time`; per-endpoint counters reset on
     un-eject AND on any 2xx/3xx/4xx response (both pathways reset — D4 + D7 lock).
   - **item-6 (fixture observable):** requests 1–3 → backend `500` + body `server error\n`
     (13 bytes) + `x-envoy-upstream-service-time` PRESENT; request 4 → synth-`503` + body
     `no healthy upstream` (19 bytes) + `x-envoy-upstream-service-time` ABSENT; counter values
     exact (D8.1 locks the bilateral assertions).
   - **item-8 (H1 vs H2 sibling):** identical stat namespace; identical ejection-on-5xx
     semantics; the H2 router-arm hook fires at the H2 HCM post-dispatch site (D4 H2 side).
   - **item-9 (synth-status bypass nuance + connect-failure classification):** the
     `pick() -> None` no-healthy-upstream synth-503 path does NOT call `record_response`
     (no endpoint to attribute); the connect-failure synth path DOES call `record_response`
     for the picked endpoint that failed (classified BOTH `consecutive_5xx` AND
     `consecutive_gateway_failure` — automatic, since the connect-failure synth status is
     `502`, which `EndpointEjection::record_response`'s `matches!(status, 502..=504)` arm
     ticks both detectors).

4. **M4 discharge — per-endpoint serialization (the most substantive carryforward, 14.1
   REVIEW §4 M4; the exact 12.1-M2 analog).** The 14.1 `EndpointEjection` uses `Relaxed`
   atomics on a `&self` receiver; the 14.1 REVIEW correctly flagged that the read-modify-write
   in `record_response` (`fetch_add` → threshold-check → conditional detected-counter `inc`)
   and the swap-edges in `eject`/`try_un_eject` are race-free ONLY under single-writer-per-
   endpoint. At 14.2 the writers become genuinely concurrent (one per in-flight request via
   D4, plus the sweeper via D7). **Resolution:** add one field `ejected_at:
   std::sync::Mutex<Option<std::time::Instant>>` to `EndpointEjection`. The `Cluster::
   record_response` compound (record → cap-check → eject + set timestamp) holds this lock for
   its full duration; the sweeper holds it around its per-endpoint un-eject (check elapsed →
   `try_un_eject` → clear timestamp). This guarantees exactly one serialized writer per
   endpoint. `pick()`'s read side stays lock-free — `is_ejected()` is a single `Relaxed`
   `AtomicBool` load (a momentarily-stale LB read is acceptable, matching Envoy + the 12.1
   `EndpointHealth` precedent). The `Mutex` doubles as the eject-timestamp store the sweeper
   needs (14.1's `EndpointEjection` carries no timestamp). The 14.2 REVIEW (named M4 owner)
   verifies this discharge. **Task 1 lands it.**

5. **`eject` / `try_un_eject` keep their 14.1 bodies; timestamp management lives at the lock
   sites, not inside them.** `eject(detector)` is reached only on a fresh threshold crossing
   while NOT ejected (`record_response` early-returns when already ejected ⇒ `decision.any()`
   is false ⇒ no `eject`), so `Cluster::record_response` sets `*ejected_at = Some(Instant::
   now())` right after `state.eject(detector)` (both under the held lock). The sweeper sets
   `*ejected_at = None` right after `ep.try_un_eject()` (under the held lock). `eject` /
   `try_un_eject` are NOT modified to touch `ejected_at` (avoids a self-deadlock with the
   externally-held guard). This keeps the 14.1 unit tests for `eject`/`try_un_eject`
   byte-unchanged.

6. **The sweeper reads timing from the runtime `OutlierDetectionState`, not by re-parsing the
   bootstrap.** `OutlierDetectionState` (`crates/envoy-cluster/src/cluster.rs:48-53`) gains two
   fields — `base_ejection_time: std::time::Duration` + `interval: std::time::Duration` —
   populated in `Cluster::from_bootstrap`'s configured-OD arm (Task 4) from the parsed config
   (Envoy v3 defaults `base_ejection_time = 30s`, `interval = 10s` when omitted, via the same
   `parse_duration` path 14.1 already uses for the validator). The sweeper consumes the parsed
   `Duration`s — no string re-parse in `outlier.rs`.

7. **`OutlierEjectionSweeper` + `OutlierManager` live in a new `crates/envoy-cluster/src/
   outlier.rs` module** (NOT a new crate). Struct shapes mirror `envoy-health::Scheduler`
   verbatim:
   ```rust
   pub struct OutlierEjectionSweeper {
       cancel: tokio_util::sync::CancellationToken,
       join: tokio::task::JoinHandle<()>,
   }
   pub struct OutlierManager {
       sweepers: Vec<OutlierEjectionSweeper>,
   }
   ```
   `OutlierManager::for_bootstrap(cluster_mgr, cancel)` walks the cluster manager; for every
   cluster whose runtime `Cluster` carries `Some(OutlierDetectionState)`, it spawns one
   sweeper over that cluster's `Vec<Arc<EndpointEjection>>` + its parsed `base_ejection_time` /
   `interval`. `pub async fn shutdown(self)` cancels + joins every sweeper. **`OutlierManager`
   is an external sibling registry** — it does NOT live on `ClusterManager`. (The §5.1 spec
   sketch's `endpoints: Arc<Vec<Arc<EndpointEjection>>>` + `config: Arc<OutlierDetectionConfig>`
   sweeper fields are simplified here to the two `Duration`s + the endpoint Vec — see §0.B-4.)

8. **Inert when unconfigured (regression-equivalence, acceptance gate (b)).** A cluster with no
   `outlier_detection` carries `outlier_detection: None` on the runtime `Cluster` ⇒
   `Cluster::record_response` short-circuits at the 14.1-landed `outlier_detection.is_none()`
   check (`cluster.rs:260`) ⇒ the D4 hook cost is one `Option::is_some()` branch; AND
   `OutlierManager::for_bootstrap` spawns ZERO sweepers for it. The 21 existing Docker-gated
   fixtures (0001–0021) see ZERO behavior change. The D4 call site is unconditional (it
   ALWAYS calls `cluster.record_response(...)`); the inertness lives behind the cluster-level
   `is_none()` short-circuit, NOT behind a call-site guard.

9. **The D4 hook fires at the router-arm dispatch site keyed on the FINAL response status, for
   every endpoint-attributed arm.** H1: a single `cluster.record_response(endpoint,
   outgoing.status)` immediately after the `match stream_or_synth { … }` block
   (`crates/envoy-http1/src/hcm.rs`, after the block that ends ~line 681) and BEFORE the
   no-healthy `else` arm — this covers the proxied response (any backend status incl. 500),
   the send-failure synth-502, the connect-failure synth-502, and the pool-overflow synth-503,
   all of which have `endpoint` in scope. The no-healthy `else` arm (`hcm.rs:682-692`) does NOT
   call it (no `endpoint`). H2: two insertions — (a) the success arm, immediately after the
   `upstream_rq_total`/`upstream_rq_5xx` increments (`crates/envoy-http2/src/hcm.rs:424-427`):
   `cluster.record_response(endpoint, upstream_resp.status)`; (b) the connect/send-failure
   `Err(e)` arm (`hcm.rs:399-417`), immediately before its `return finalize_h2_stream(…)`:
   `cluster.record_response(endpoint, 502)`. The H2 `pick() -> None` arm (`hcm.rs:240-257`)
   does NOT call it (no `endpoint`).

10. **Pool-overflow synth-503 records as an endpoint failure (benign over-classification at
    minimum-viable scope).** On the H1 path, `PoolError::Overflow` returns `synth_status(503,
    close)` with `endpoint` in scope, so the uniform post-match `record_response(endpoint,
    outgoing.status)` records it (503 → both detectors tick). Envoy's overflow-vs-outlier
    interplay is NOT in 14.2 scope, and fixture 0022 never triggers overflow (single endpoint,
    no `max_connections` constraint). PROGRESS Task 2 documents this; it is differential-inert
    for every fixture (no fixture exercises pool overflow on an outlier-detection cluster).

11. **`OutlierManager` wires into `envoy-bin` exactly like the health scheduler.** Constructed
    after the pool managers + health scheduler (`crates/envoy-bin/src/main.rs:165-171`
    region), passed `token.clone()`; its `shutdown().await` is awaited on BOTH the clean-exit
    and error-exit drain paths alongside `health_scheduler.shutdown().await`
    (`main.rs:467-472` region).

12. **Fixture 0022 reuses the 13.x synthetic backend + the keep-alive driver (extended).** The
    backend is `tests/helpers/health-aware-http1-backend` with `--per-path /fail=500` (it
    already serves `per_class_body(500) == b"server error\n"`, 13 bytes, and `200` + empty
    body on `/`). The driver is `Driver::Http1KeepAlive`, extended per §0.B-3 to assert
    per-request body bytes + header presence/absence. Config: `outlier_detection:
    {consecutive_5xx: 3, base_ejection_time: 60s, max_ejection_percent: 100, interval: 1s}` +
    `common_lb_config: {healthy_panic_threshold: {value: 0}}` + a single-endpoint cluster.

13. **The in-process backstop (D8.3) uses a SHORT `base_ejection_time` (5s)** so the un-eject
    direction converges within test wall-time, matching the §6.2 item-5 capture (~5–6s for
    `base_ejection_time: 5s` at envoyproxy/envoy:v1.33.0). It exercises BOTH directions +
    asserts the 5-standard-header presence on the synth-503 (`server`, `date`,
    `content-length`, `content-type`, `connection`), per parent §6.4 + SPEC §6.5.

14. **14.2 lands NO new BEHAVIOR_CONTRACT row (D9.1).** The no-healthy-upstream synth-503 row
    (12.2-landed, `BEHAVIOR_CONTRACT.md:27-36`) is REUSED unchanged; the
    `x-envoy-upstream-service-time` allow-list row (04.3-landed) is REUSED unchanged. The ONLY
    contract edit is the M8 count reconciliation (§0.C M8).

15. **M5 + M6 fold into Task 1** (14.1 REVIEW §4): M5 — expose `EndpointEjectionStats` from the
    `mk_handle_with_health_and_ejection` test helper + strengthen
    `cluster_record_response_picks_5xx_detector_on_ties` to assert `enforced_consecutive_5xx
    == 1 && enforced_consecutive_gateway_failure == 0` + drop the vestigial `let _ =
    stats_active;`. M6 — add a `max_ejection_percent = 0` edge test + a one-line comment at the
    cap site (`cluster.rs:286` region) citing §6.2 item-4. **A-M2 folds into Task 2** (the
    stale `tokio::sync::Mutex` comment at `crates/envoy-http1/src/pool.rs:322` → `parking_lot::
    Mutex`). **M8 folds into Task 9** (the `allowlist_envoy_only` 14-vs-13 count reconciliation,
    empirically resolved when fixture 0022's `expectations.yaml` engages the real Envoy stat
    tree). **M1 / M2 / M3 / M7 / M9** carry forward unchanged (no-action / opportunistic; no
    named owner).

16. **TDD per task** (D-3.1 / `BOOTSTRAP_PROMPT.md` §5 state 3): failing test first, minimal
    impl, green, commit. Subagent-driven per `feedback_execution_style`.

## 0.B PLAN-time SPEC corrections (per the 06.2 → 14.1 cadence; SPEC §6.3)

Read against the PLAN-time HEAD `b0dea44f`. These corrections are recorded here and re-stated
in the PROGRESS Task 1 preamble; the SPEC itself is ratified and NOT edited.

- **B-1. The H1 site is `construct_proxied_response`, NOT `write_proxied_response`; and
  `endpoint` is NOT in scope inside it — the hook lands at the CALLER in `hcm.rs`.** SPEC §3
  D4 + §6.3 name `crates/envoy-http1/src/router.rs::write_proxied_response`. At HEAD that
  function is `crates/envoy-http1/src/router.rs::construct_proxied_response`
  (`router.rs:87-168`); it fires `cluster.upstream_rq_total().inc()` (`router.rs:95`) +
  `cluster.upstream_rq_5xx().inc()` (`router.rs:97`) but takes only `cluster: &ClusterHandle`
  (no `endpoint`). The picked `endpoint: SocketAddr` is in scope at the CALLER
  (`crates/envoy-http1/src/hcm.rs`, inside `if let Some(endpoint) = cluster.pick_endpoint()`
  at `hcm.rs:465`), where `construct_proxied_response` is invoked at `hcm.rs:641`. **The D4-H1
  hook therefore lands in `hcm.rs` (after the `match stream_or_synth` at ~`hcm.rs:681`), NOT in
  `router.rs`.** The increment-after / write-before ordering is preserved: the increments fire
  inside `construct_proxied_response` on the success arm; `record_response` fires after the
  whole dispatch `match` resolves `outgoing`; the downstream write happens later (the HCM
  funnels `outgoing` to the wire after the proxy block).

- **B-2. The H1 no-healthy-upstream synth-503 short-circuit is at `hcm.rs:691`
  (`synth_no_healthy_upstream(close)`), NOT `hcm.rs:582`.** SPEC §2.1 + §6.3 cite
  `crates/envoy-http1/src/hcm.rs:582`. At HEAD the `pick() -> None` `else` arm is `hcm.rs:682-692`
  and the synth builder `synth_no_healthy_upstream` is `hcm.rs:1057-1077`. (`hcm.rs:582` is in
  the connect-failure region.) D4 does NOT call `record_response` on this arm.

- **B-3. `Driver::Http1KeepAlive` is reused-plus-EXTENDED, not "verbatim — no new harness
  primitive".** SPEC §1 + §3 D8.1 + §5.6 claim the driver is reused verbatim while ALSO
  requiring fixture 0022 to assert per-request body bytes (`server error\n` / `no healthy
  upstream`) + `x-envoy-upstream-service-time` presence/absence bilaterally (§6.2 item-6). At
  HEAD the keep-alive driver (`tests/differential/src/lib.rs:2668-…`, helper
  `read_h1_response_status` at `lib.rs:1689`) reads ONLY the status line + drains the body — it
  captures neither body bytes nor headers, and `Http1KeepAliveRequest` (`lib.rs:314-319`)
  carries only `method` / `path` / `host` / `expected_status`. So the SPEC's bilateral body +
  header-presence assertions are unimplementable on the verbatim driver. **Correction:** Task 6
  extends the driver — three `#[serde(default)]` optional fields on `Http1KeepAliveRequest`
  (`expected_body: Option<Http1BodyRule>`, `require_header_present: Option<String>`,
  `require_header_absent: Option<String>`) + a `read_h1_response_full` helper returning
  `(u16, Vec<(String,String)>, Vec<u8>)` + assertions in the keep-alive exec arm. The
  extension is additive + backward-compatible (existing fixtures 0020/0021 set none). ~90 LoC.

- **B-4. The §5.1 sweeper-field sketch is simplified.** SPEC §3 D7 sketches `OutlierEjection
  Sweeper { cluster_name, endpoints: Arc<Vec<Arc<EndpointEjection>>>, config:
  Arc<OutlierDetectionConfig>, cancel, join }`. At HEAD there is no `OutlierDetectionConfig`
  runtime type (the parsed config is consumed at `from_bootstrap`); the cleaner shape stores
  the two `Duration`s the sweeper actually needs on the runtime `OutlierDetectionState`
  (lock-in #6) and gives the sweeper just `{ cancel, join }` (lock-in #7), with the endpoint
  Vec + Durations captured into the spawned task at construction. `cluster_name` is retained
  only for the sweeper's `tracing` span. Behavior is identical to the sketch; the field set is
  reconciled to the on-disk types.

- **B-5. H1 connect-failure synthesizes `502` (and pool-overflow `503`); H2 no-healthy
  synthesizes `502` (`synth_h2_502`).** SPEC §2.2 says the H2 no-healthy sibling "emits the
  same 19-byte body" as the H1 503; at HEAD the H2 `pick() -> None` arm returns `synth_h2_502()`
  (status `502`, `hcm.rs:240-257`), not a 503. This is PRE-EXISTING behavior unchanged by 14.2
  and is differential-inert for fixture 0022 (H1-only); D4 does not call `record_response` on
  either no-healthy arm regardless of status. Noted for cold-readability; no code change.

- **B-6. Backend `per_class_body(500)` is `b"server error\n"` = 13 bytes** (`tests/helpers/
  health-aware-http1-backend/src/main.rs:121-133`), matching SPEC §6.2 item-6. (The PLAN-time
  recon initially miscounted 12; it is 13: `server error\n`.) `200` on `/` yields an empty
  body.

## 0.C Carryforward dispositions entering 14.2

| ID | Source | Disposition at 14.2 |
|---|---|---|
| **M4** | 14.1 REVIEW | **Discharged at Task 1** (lock-in #4) — per-endpoint serialization lock; verified at the 14.2 REVIEW (named owner). |
| **M5** | 14.1 REVIEW | **Folded into Task 1** — expose `EndpointEjectionStats` from `mk_handle_with_health_and_ejection`; strengthen the tie test; drop the vestigial `let _ = stats_active;`. |
| **M6** | 14.1 REVIEW | **Folded into Task 1** — `max_ejection_percent = 0` edge test + cap-site comment citing §6.2 item-4. |
| **M8** | 14.1 REVIEW | **Folded into Task 9** — reconcile the `allowlist_envoy_only` "14"-vs-13 count in BOTH the 14.1 SPEC §2.1 AND `BEHAVIOR_CONTRACT.md` once fixture-0022's `expectations.yaml` empirically lists the deferred Envoy names (state-3 Docker run). Zero differential impact. |
| **A-M2** | 13.2 REVIEW | **Folded into Task 2** — fix the stale `tokio::sync::Mutex` comment at `crates/envoy-http1/src/pool.rs:322` → `parking_lot::Mutex` (14.2 D4 touches `envoy-http1`). ~1 LoC. |
| **M1 / M2 / M3 / M7 / M9** | 14.1 REVIEW | No-action / opportunistic; no named owner; carry forward unchanged. |
| **ADR-0028** | ADR-0039 Consequences | Remains OPEN; named owner is a follow-up foundations-pivot phase — NOT 14.2. Carry forward. |
| 13.2/13.1/12.2/12.1/11/earlier Minors + 04.1 M5/M9 Cargo.lock cadence | multi-phase | Carry forward unchanged; none engaged by 14.2's surface beyond those listed above. |
| **§6.9 per-class `upstream_rq_{2,3,4}xx` extension** | 13.1 carryforward | **DEFER** (SPEC §6.9 recommended posture) — observability work, not outlier-detection work; folding it in inflates 14.2 scope. Not engaged. |

---

## Task 1: Ejection-state extension — M4 serialization + eject-timestamp + M5/M6 fold-ins

**Files:**
- Modify: `crates/envoy-cluster/src/ejection.rs` (add `ejected_at` field + init; no change to `eject`/`try_un_eject` bodies)
- Modify: `crates/envoy-cluster/src/cluster.rs` (hold the serialization lock across the `record_response` compound; set/clear timestamp; cap-site comment; strengthen the tie test + the M6 edge test; expose stats from the test helper)
- Test: inline `#[cfg(test)]` in `crates/envoy-cluster/src/cluster.rs` + `crates/envoy-cluster/src/ejection.rs`

- [ ] **Step 1: Write the failing test — serialization guard + timestamp are observable.**

In `crates/envoy-cluster/src/ejection.rs` `#[cfg(test)]`:

```rust
#[test]
fn ejected_at_is_none_until_eject_and_set_after() {
    let stats = test_stats(); // existing test helper building EndpointEjectionStats
    let e = EndpointEjection::new(3, 3, stats);
    assert!(e.ejected_at.lock().unwrap().is_none(), "never-ejected ⇒ no timestamp");
    // Drive 3 consecutive 5xx to cross the threshold, then eject as the
    // production compound does (record → eject + stamp).
    for _ in 0..3 {
        let _ = e.record_response(500);
    }
    e.eject(DetectorType::Consecutive5xx);
    *e.ejected_at.lock().unwrap() = Some(std::time::Instant::now());
    assert!(e.ejected_at.lock().unwrap().is_some(), "ejected ⇒ timestamp set");
    assert!(e.is_ejected());
}
```

- [ ] **Step 2: Run it to verify it fails.**

Run: `cargo test -p envoy-cluster ejected_at_is_none_until_eject_and_set_after`
Expected: FAIL — `no field ejected_at on EndpointEjection`.

- [ ] **Step 3: Add the `ejected_at` field + init (no body change to eject/try_un_eject).**

In `crates/envoy-cluster/src/ejection.rs`, add to the `EndpointEjection` struct (after the
existing atomics, before `stats`):

```rust
    /// 14.2 M4 discharge (lock-in #4): the per-endpoint serialization lock. The
    /// `Cluster::record_response` compound (record → cap-check → eject) and the
    /// `OutlierEjectionSweeper`'s per-endpoint un-eject each hold this guard for their full
    /// duration, so the `Relaxed` atomics above are mutated by exactly one writer at a time.
    /// The `Option<Instant>` payload doubles as the eject-timestamp the sweeper reads to apply
    /// `base_ejection_time` (§6.2 item-5). `pick()`'s read side stays lock-free (`is_ejected()`
    /// is a single `Relaxed` `AtomicBool` load). Set by `Cluster::record_response` right after
    /// `eject`; cleared by the sweeper right after `try_un_eject` — NOT inside `eject`/
    /// `try_un_eject` (which would self-deadlock with the externally-held guard), lock-in #5.
    pub(crate) ejected_at: std::sync::Mutex<Option<std::time::Instant>>,
```

In `EndpointEjection::new`, initialize it (no new parameter):

```rust
        ejected_at: std::sync::Mutex::new(None),
```

- [ ] **Step 4: Run it to verify it passes.**

Run: `cargo test -p envoy-cluster ejected_at_is_none_until_eject_and_set_after`
Expected: PASS.

- [ ] **Step 5: Write the failing test — `Cluster::record_response` serializes + stamps (M4); and the M6 `max_ejection_percent = 0` edge.**

In `crates/envoy-cluster/src/cluster.rs` `#[cfg(test)]`, first strengthen the M5 helper so
tests can read per-detector counters, then add the tests:

```rust
#[test]
fn cluster_record_response_stamps_ejected_at_on_eject() {
    // Single endpoint, consecutive_5xx: 3, max_ejection_percent: 100.
    let (cluster, _stats) = mk_cluster_with_outlier_detection(/* hosts */ 1, 3, 100);
    let ep = cluster.endpoints()[0];
    for _ in 0..3 { cluster.record_response(ep, 500); }
    let od = cluster.outlier_detection_for_test();
    assert!(od.endpoints[0].is_ejected());
    assert!(od.endpoints[0].ejected_at.lock().unwrap().is_some(),
        "M4: record_response stamps ejected_at under the serialization lock");
}

#[test]
fn cluster_record_response_max_ejection_percent_zero_never_ejects() {
    // M6: max_ejection_percent = 0 ⇒ cap_count = 0 ⇒ first crossing overflows, never ejects.
    let (cluster, stats) = mk_cluster_with_outlier_detection(1, 3, 0);
    let ep = cluster.endpoints()[0];
    for _ in 0..3 { cluster.record_response(ep, 500); }
    let od = cluster.outlier_detection_for_test();
    assert!(!od.endpoints[0].is_ejected(), "0% ⇒ never eject");
    assert_eq!(stats_value(&stats, "ejections_overflow"), 1, "first crossing overflows");
}
```

- [ ] **Step 6: Run them to verify they fail.**

Run: `cargo test -p envoy-cluster cluster_record_response_stamps_ejected_at_on_eject cluster_record_response_max_ejection_percent_zero_never_ejects`
Expected: FAIL (compile or assertion — `ejected_at` not set by `record_response` yet; helper signatures).

- [ ] **Step 7: Implement the M4 serialization in `Cluster::record_response`; add the M6 cap-site comment.**

In `crates/envoy-cluster/src/cluster.rs`, rewrite `Cluster::record_response`'s per-endpoint
mutation to hold the lock for the compound and stamp the timestamp:

```rust
pub fn record_response(&self, endpoint: SocketAddr, status: u16) {
    let Some(od) = self.outlier_detection.as_ref() else {
        return; // §5.3 inert (lock-in #8)
    };
    let Some(idx) = self.endpoints.iter().position(|e| *e == endpoint) else {
        return; // defense-in-depth (14.1 lock-in #10)
    };
    let state = &od.endpoints[idx];
    // 14.2 M4 (lock-in #4): hold the per-endpoint serialization lock across the whole
    // compound (record → cap-check → eject + stamp) so the `Relaxed` atomics are mutated by
    // exactly one writer at a time — the D4 hook fires from every in-flight request task and
    // the D7 sweeper is a concurrent writer.
    let mut ejected_at = state.ejected_at.lock().unwrap();
    let decision = state.record_response(status);
    if !decision.any() {
        return;
    }
    let total = self.endpoints.len();
    // 14.1 M6 (§6.2 item-4): cap_count = floor(total * max_ejection_percent / 100). When
    // max_ejection_percent == 0 ⇒ cap_count == 0 ⇒ active_count (0) >= cap_count (0) on the
    // first crossing ⇒ overflow, never ejecting (a deliberate "0% = eject nothing" edge).
    let cap_count = (total * od.max_ejection_percent as usize) / 100;
    let active_count = od.endpoints.iter().filter(|e| e.is_ejected()).count();
    if active_count >= cap_count {
        od.ejections_overflow.inc();
        return;
    }
    let detector = if decision.crossed_5xx {
        crate::DetectorType::Consecutive5xx
    } else {
        crate::DetectorType::ConsecutiveGatewayFailure
    };
    state.eject(detector);
    *ejected_at = Some(std::time::Instant::now()); // lock-in #5: stamp under the held lock
}
```

(The guard `ejected_at` drops at function end on every return path.)

- [ ] **Step 8: M5 — strengthen the tie test + expose stats from the helper; drop the vestigial binding.**

Extend `mk_handle_with_health_and_ejection` (and/or `mk_cluster_with_outlier_detection`) to
ALSO return the shared `EndpointEjectionStats`, and rewrite
`cluster_record_response_picks_5xx_detector_on_ties` to assert the detector choice:

```rust
#[test]
fn cluster_record_response_picks_5xx_detector_on_ties() {
    // A 503 crosses BOTH detectors simultaneously; 5xx must win the tie (cluster.rs ~289).
    let (cluster, stats) = mk_cluster_with_outlier_detection(1, /*c5xx*/ 1, 100);
    let ep = cluster.endpoints()[0];
    cluster.record_response(ep, 503);
    assert!(cluster.outlier_detection_for_test().endpoints[0].is_ejected());
    assert_eq!(stats_value(&stats, "ejections_enforced_consecutive_5xx"), 1);
    assert_eq!(stats_value(&stats, "ejections_enforced_consecutive_gateway_failure"), 0,
        "M5: 5xx wins the tie — gateway-failure enforced counter stays 0");
    // (Drop the prior `let stats_active = …; let _ = stats_active;` vestigial binding.)
}
```

- [ ] **Step 9: Run the full envoy-cluster suite to verify all pass + no regression.**

Run: `cargo test -p envoy-cluster`
Expected: PASS (all 14.1 ejection/cluster tests + the 4 new ones green; the 14.1
`cluster_record_response_picks_5xx_detector_on_ties` now asserts the detector).

- [ ] **Step 10: Clippy + fmt.**

Run: `cargo clippy -p envoy-cluster --all-targets --all-features -- -D warnings && cargo fmt --all -- --check`
Expected: clean.

- [ ] **Step 11: Commit.**

```bash
git add crates/envoy-cluster/src/ejection.rs crates/envoy-cluster/src/cluster.rs
git commit -m "phase 14.2 Task 1: M4 per-endpoint serialization + eject-timestamp + M5/M6 fold-ins"
```

---

## Task 2: D4-H1 — response-receipt hook on the H1 router-proxy arm + A-M2 fix

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` (one `record_response` call after the dispatch `match`, ~line 681)
- Modify: `crates/envoy-http1/src/pool.rs:322` (A-M2 stale-comment fix)
- Test: inline `#[cfg(test)]` in `crates/envoy-http1/src/hcm.rs`

- [ ] **Step 1: Write the failing test — the H1 proxy arm records the upstream status.**

In `crates/envoy-http1/src/hcm.rs` `#[cfg(test)]`, build an HCMConfig over a cluster with
`outlier_detection {consecutive_5xx: 1}` pointed at a test backend that returns 500, drive one
request, and assert the endpoint ejected (proving `record_response` was called with the 500):

```rust
#[tokio::test]
async fn h1_router_arm_records_response_and_ejects_after_threshold() {
    // Backend serves 500; consecutive_5xx threshold 1 ⇒ one 500 ejects the endpoint.
    let (cfg, cluster) = test_hcm_with_outlier_detection_backend_500(/*c5xx*/ 1).await;
    let resp = drive_one_request(&cfg, "GET", "/").await;
    assert_eq!(resp.status, 500, "first request proxies the backend 500");
    assert!(cluster.is_endpoint_ejected_for_test(0),
        "D4: record_response(endpoint, 500) ejected the endpoint at threshold 1");
}
```

- [ ] **Step 2: Run it to verify it fails.**

Run: `cargo test -p envoy-http1 h1_router_arm_records_response_and_ejects_after_threshold`
Expected: FAIL — endpoint not ejected (no `record_response` call yet).

- [ ] **Step 3: Insert the D4-H1 hook after the dispatch `match`.**

In `crates/envoy-http1/src/hcm.rs`, immediately after the `match stream_or_synth { … }` block
closes (the `}` that currently precedes the `} else {` no-healthy arm at ~`hcm.rs:681-682`),
still inside the `if let Some(endpoint) = cluster.pick_endpoint()` block, insert:

```rust
                        // 14.2 D4 (lock-in #9): response-receipt hook (H1). Classify the FINAL
                        // response status for the picked endpoint AFTER the upstream_rq_*
                        // increments fire (inside construct_proxied_response on the success arm)
                        // and BEFORE the downstream write. The connect/send-failure synth-502 +
                        // pool-overflow synth-503 ALSO record here (the picked endpoint failed;
                        // 502/503 tick both detectors per ADR-0041 §6.2 item-9, lock-in #3/#10).
                        // Inert when the cluster has no outlier_detection (record_response
                        // short-circuits at the cluster-level is_none() check, lock-in #8). The
                        // no-healthy `else` arm below does NOT call this (no endpoint to attribute).
                        cluster.record_response(endpoint, outgoing.status);
```

- [ ] **Step 4: Run it to verify it passes.**

Run: `cargo test -p envoy-http1 h1_router_arm_records_response_and_ejects_after_threshold`
Expected: PASS.

- [ ] **Step 5: A-M2 — fix the stale comment in pool.rs.**

In `crates/envoy-http1/src/pool.rs:322`, change the stale `tokio::sync::Mutex` reference in the
hand-rolled `Debug` impl comment to `parking_lot::Mutex` (the actual type post-13.2 A-I3).
Comment-only; no behavior change.

- [ ] **Step 6: Run the full envoy-http1 suite + clippy + fmt.**

Run: `cargo test -p envoy-http1 && cargo clippy -p envoy-http1 --all-targets --all-features -- -D warnings && cargo fmt --all -- --check`
Expected: clean (all pre-existing HCM tests green — the hook is inert for outlier-detection-
unconfigured clusters).

- [ ] **Step 7: Commit.**

```bash
git add crates/envoy-http1/src/hcm.rs crates/envoy-http1/src/pool.rs
git commit -m "phase 14.2 Task 2: D4 H1 response-receipt hook + A-M2 stale-comment fix"
```

---

## Task 3: D4-H2 — response-receipt hook on the H2 router-proxy arm

**Files:**
- Modify: `crates/envoy-http2/src/hcm.rs` (success-arm + connect-failure-arm `record_response` calls)
- Test: inline `#[cfg(test)]` in `crates/envoy-http2/src/hcm.rs`

- [ ] **Step 1: Write the failing test — the H2 router arm records the upstream status.**

In `crates/envoy-http2/src/hcm.rs` `#[cfg(test)]`, mirror the H1 test on the H2 listener path:

```rust
#[tokio::test]
async fn h2_router_arm_records_response_and_ejects_after_threshold() {
    let (cfg, cluster) = test_h2_hcm_with_outlier_detection_backend_500(/*c5xx*/ 1).await;
    let resp_status = drive_one_h2_request(&cfg, "GET", "/").await;
    assert_eq!(resp_status, 500);
    assert!(cluster.is_endpoint_ejected_for_test(0),
        "D4-H2: record_response(endpoint, 500) ejected the endpoint");
}
```

- [ ] **Step 2: Run it to verify it fails.**

Run: `cargo test -p envoy-http2 h2_router_arm_records_response_and_ejects_after_threshold`
Expected: FAIL — endpoint not ejected.

- [ ] **Step 3: Insert the D4-H2 success-arm hook (after the increments).**

In `crates/envoy-http2/src/hcm.rs`, immediately after the success-arm increments at
`hcm.rs:424-427` (`cluster.upstream_rq_total().inc();` + the `if … status/100==5 { …upstream_rq_5xx… }`):

```rust
                // 14.2 D4 (lock-in #9): response-receipt hook (H2 post-dispatch). Fires AFTER
                // the upstream_rq_* increments and BEFORE the downstream response is built/sent.
                // Inert when outlier_detection is None (cluster-level is_none() short-circuit).
                cluster.record_response(endpoint, upstream_resp.status);
```

- [ ] **Step 4: Insert the D4-H2 connect/send-failure-arm hook.**

In the `Err(e)` arm of `let upstream_resp = match upstream_resp_result { … }`
(`hcm.rs:399-417`), immediately before the `return finalize_h2_stream(…)`:

```rust
                        // 14.2 D4 (lock-in #9, ADR-0041 §6.2 item-9): the picked endpoint failed
                        // to connect/send — record the synth-502 (ticks BOTH consecutive_5xx
                        // AND consecutive_gateway_failure). The no-healthy `pick() -> None` arm
                        // above does NOT record (no endpoint).
                        cluster.record_response(endpoint, 502);
```

- [ ] **Step 5: Run it to verify it passes + full suite + clippy + fmt.**

Run: `cargo test -p envoy-http2 && cargo clippy -p envoy-http2 --all-targets --all-features -- -D warnings && cargo fmt --all -- --check`
Expected: PASS / clean (h2spec-framing path untouched; pre-existing H2 tests green).

- [ ] **Step 6: Commit.**

```bash
git add crates/envoy-http2/src/hcm.rs
git commit -m "phase 14.2 Task 3: D4 H2 response-receipt hook (success + connect-failure arms)"
```

---

## Task 4: D7 — `OutlierEjectionSweeper` + `OutlierManager` (the fourth periodic-background primitive)

**Files:**
- Create: `crates/envoy-cluster/src/outlier.rs`
- Modify: `crates/envoy-cluster/src/lib.rs` (`mod outlier;` + re-export `OutlierManager`)
- Modify: `crates/envoy-cluster/src/cluster.rs` (add `base_ejection_time` + `interval` to `OutlierDetectionState`; populate in `from_bootstrap`; add a `pub(crate)` accessor for the sweeper)
- Test: inline `#[cfg(test)]` in `crates/envoy-cluster/src/outlier.rs`

- [ ] **Step 1: Add the timing fields to `OutlierDetectionState` + populate in `from_bootstrap`.**

In `crates/envoy-cluster/src/cluster.rs`, extend `OutlierDetectionState` (lock-in #6):

```rust
pub(crate) struct OutlierDetectionState {
    pub(crate) endpoints: Vec<Arc<crate::EndpointEjection>>,
    pub(crate) max_ejection_percent: u32,
    pub(crate) ejections_overflow: Arc<envoy_stats::Counter>,
    pub(crate) base_ejection_time: std::time::Duration, // 14.2 D7
    pub(crate) interval: std::time::Duration,           // 14.2 D7
}
```

In `Cluster::from_bootstrap`'s configured-OD arm, parse the two durations (reuse the existing
duration-parse path; Envoy v3 defaults when omitted: `base_ejection_time = 30s`,
`interval = 10s`) and populate the new fields. Add a `pub(crate)` accessor so `outlier.rs` can
reach the per-cluster state:

```rust
impl Cluster {
    pub(crate) fn outlier_detection_state(&self) -> Option<&OutlierDetectionState> {
        self.outlier_detection.as_ref()
    }
}
```

- [ ] **Step 2: Write the failing test — the sweeper un-ejects after `base_ejection_time`.**

In `crates/envoy-cluster/src/outlier.rs` `#[cfg(test)]`:

```rust
#[tokio::test(start_paused = true)]
async fn sweeper_un_ejects_after_base_ejection_time() {
    use std::time::Duration;
    // One endpoint, ejected with a short base_ejection_time + interval.
    let ep = test_ejection_handle(); // Arc<EndpointEjection> with a real stats bundle
    ep.eject(crate::DetectorType::Consecutive5xx);
    *ep.ejected_at.lock().unwrap() = Some(std::time::Instant::now());
    assert!(ep.is_ejected());

    let cancel = tokio_util::sync::CancellationToken::new();
    let sweeper = OutlierEjectionSweeper::spawn(
        "c1".to_string(),
        vec![ep.clone()],
        /*base*/ Duration::from_secs(5),
        /*interval*/ Duration::from_secs(1),
        cancel.clone(),
    );
    // Advance virtual time past base_ejection_time + one interval tick.
    tokio::time::advance(Duration::from_secs(7)).await;
    tokio::task::yield_now().await;
    assert!(!ep.is_ejected(), "endpoint un-ejected after base_ejection_time");
    assert!(ep.ejected_at.lock().unwrap().is_none(), "timestamp cleared on un-eject");
    sweeper.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn sweeper_shutdown_joins_cleanly() {
    let cancel = tokio_util::sync::CancellationToken::new();
    let sweeper = OutlierEjectionSweeper::spawn(
        "c1".to_string(), vec![], std::time::Duration::from_secs(5),
        std::time::Duration::from_secs(1), cancel.clone());
    sweeper.shutdown().await; // cancels + joins without leaking the task
}
```

- [ ] **Step 3: Run it to verify it fails.**

Run: `cargo test -p envoy-cluster --lib outlier`
Expected: FAIL — `outlier` module / `OutlierEjectionSweeper` not found.

- [ ] **Step 4: Implement `outlier.rs`.**

```rust
//! 14.2 D7: passive outlier-detection ejection sweeper — the FOURTH periodic-background
//! primitive (after the 12.2 active-HC scheduler + 13.1 H1 pool idle sweeper + 13.2 H2 pool
//! idle sweeper). Identical `tokio_util::sync::CancellationToken` cancellation discipline +
//! `pub async fn shutdown(self)`. Lives inside `envoy-cluster` (not a new crate) so it reads
//! the per-endpoint `EndpointEjection` state cycle-free (SPEC §5.1).

use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

use crate::EndpointEjection;

/// One per-cluster sweeper task. At each `interval` tick it un-ejects every endpoint whose
/// ejection has aged past `base_ejection_time` (§6.2 item-5).
pub struct OutlierEjectionSweeper {
    cancel: CancellationToken,
    join: tokio::task::JoinHandle<()>,
}

impl OutlierEjectionSweeper {
    pub fn spawn(
        cluster_name: String,
        endpoints: Vec<Arc<EndpointEjection>>,
        base_ejection_time: Duration,
        interval: Duration,
        cancel: CancellationToken,
    ) -> Self {
        let task_cancel = cancel.clone();
        let join = tokio::spawn(async move {
            // Guard against a zero interval (validator rejects 0s, but be defensive).
            let period = interval.max(Duration::from_millis(1));
            let mut tick = tokio::time::interval(period);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = task_cancel.cancelled() => return,
                    _ = tick.tick() => {
                        sweep_once(&cluster_name, &endpoints, base_ejection_time);
                    }
                }
            }
        });
        Self { cancel, join }
    }

    /// Cancels the tick loop and joins the task (no leaked task on cluster destroy / drain).
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.join.await;
    }
}

/// 14.2 M4 (lock-in #4): hold each endpoint's serialization lock around the check-and-un-eject
/// so it cannot interleave with a concurrent `Cluster::record_response` compound.
fn sweep_once(cluster_name: &str, endpoints: &[Arc<EndpointEjection>], base: Duration) {
    for (i, ep) in endpoints.iter().enumerate() {
        let mut at = ep.ejected_at.lock().unwrap();
        if let Some(t) = *at {
            if t.elapsed() >= base {
                ep.try_un_eject(); // clears `ejected` AtomicBool + resets counters + gauge dec
                *at = None;         // clear the timestamp under the held lock (lock-in #5)
                tracing::debug!(cluster = %cluster_name, endpoint_idx = i, "outlier: un-ejected");
            }
        }
    }
}

/// External sibling registry to `ClusterManager` (mirrors `H1PoolManager` / `H2PoolManager` /
/// `envoy-health::Scheduler`). One sweeper per cluster that configures `outlier_detection`.
pub struct OutlierManager {
    sweepers: Vec<OutlierEjectionSweeper>,
}

impl OutlierManager {
    pub fn for_bootstrap(cluster_mgr: &crate::ClusterManager, cancel: CancellationToken) -> Self {
        let mut sweepers = Vec::new();
        for handle in cluster_mgr.all_clusters() { // existing iterator over ClusterHandle
            if let Some(od) = handle.inner_outlier_detection_state() {
                sweepers.push(OutlierEjectionSweeper::spawn(
                    handle.name().to_string(),
                    od.endpoints.clone(),         // cheap Arc clones
                    od.base_ejection_time,
                    od.interval,
                    cancel.clone(),
                ));
            }
        }
        Self { sweepers }
    }

    pub async fn shutdown(self) {
        for s in self.sweepers {
            s.shutdown().await;
        }
    }
}
```

Wire the helper accessors needed by `for_bootstrap`: a `ClusterManager::all_clusters()`
iterator (reuse the existing one the pool managers walk) and a `ClusterHandle`/`Cluster`
`pub(crate)` `inner_outlier_detection_state()` that returns `Option<&OutlierDetectionState>`
(delegating to `Cluster::outlier_detection_state` from Step 1). Add `mod outlier;` +
`pub use outlier::OutlierManager;` to `crates/envoy-cluster/src/lib.rs`.

- [ ] **Step 5: Run the sweeper tests + full envoy-cluster suite.**

Run: `cargo test -p envoy-cluster`
Expected: PASS (both sweeper tests + all Task-1 tests + 14.1 tests green).

- [ ] **Step 6: Clippy + fmt.**

Run: `cargo clippy -p envoy-cluster --all-targets --all-features -- -D warnings && cargo fmt --all -- --check`
Expected: clean.

- [ ] **Step 7: Commit.**

```bash
git add crates/envoy-cluster/src/outlier.rs crates/envoy-cluster/src/lib.rs crates/envoy-cluster/src/cluster.rs
git commit -m "phase 14.2 Task 4: D7 OutlierEjectionSweeper + OutlierManager (fourth periodic-background primitive)"
```

---

## Task 5: D7-wiring — `OutlierManager` startup + drain shutdown in `envoy-bin`

**Files:**
- Modify: `crates/envoy-bin/src/main.rs` (construct + spawn after the health scheduler; `shutdown().await` on both drain paths)

- [ ] **Step 1: Construct + spawn the `OutlierManager` at startup.**

In `crates/envoy-bin/src/main.rs`, after the `health_scheduler` construction
(`main.rs:165-171` region):

```rust
    // 14.2 D7 (lock-in #11): the fourth periodic-background primitive. Spawns one ejection
    // sweeper per cluster that configures outlier_detection; inert (zero sweepers) otherwise.
    let outlier_mgr = envoy_cluster::OutlierManager::for_bootstrap(&cluster_mgr, token.clone());
```

- [ ] **Step 2: Await `shutdown` on both drain paths.**

Alongside `health_scheduler.shutdown().await;` (`main.rs:467-472` region), on BOTH the
clean-exit and error-exit paths:

```rust
    outlier_mgr.shutdown().await;
```

- [ ] **Step 3: Build + clippy + fmt.**

Run: `cargo build -p envoy-bin && cargo clippy -p envoy-bin --all-targets --all-features -- -D warnings && cargo fmt --all -- --check`
Expected: clean.

- [ ] **Step 4: Commit.**

```bash
git add crates/envoy-bin/src/main.rs
git commit -m "phase 14.2 Task 5: wire OutlierManager into envoy-bin startup + drain shutdown"
```

---

## Task 6: D8.1a — extend `Driver::Http1KeepAlive` for per-request body + header assertions

**Files:**
- Modify: `tests/differential/src/lib.rs` (3 optional fields on `Http1KeepAliveRequest`; `read_h1_response_full` helper; assertions in the keep-alive exec arm)
- Test: inline `#[cfg(test)]` serde round-trip in `tests/differential/src/lib.rs`

> PLAN-time SPEC correction B-3: the SPEC's "reuses the driver verbatim" is corrected — the
> driver captures only status, so it cannot assert the §6.2 item-6 body + header observables.
> This task adds a small, backward-compatible extension. Existing fixtures 0020/0021 set none.

- [ ] **Step 1: Write the failing serde round-trip test for the new fields.**

In `tests/differential/src/lib.rs` `#[cfg(test)]`:

```rust
#[test]
fn http1_keep_alive_request_round_trips_body_and_header_assertions() {
    let yaml = r#"
method: GET
path: /fail
host: c1
expected_status: 500
expected_body: { byte_exact: { body: "server error\n" } }
require_header_present: x-envoy-upstream-service-time
"#;
    let req: Http1KeepAliveRequest = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(req.expected_status, 500);
    assert!(matches!(req.expected_body, Some(Http1BodyRule::ByteExact { .. })));
    assert_eq!(req.require_header_present.as_deref(), Some("x-envoy-upstream-service-time"));
    assert!(req.require_header_absent.is_none());
}
```

- [ ] **Step 2: Run it to verify it fails.**

Run: `cargo test -p differential http1_keep_alive_request_round_trips_body_and_header_assertions`
Expected: FAIL — unknown fields / no such fields.

- [ ] **Step 3: Add the optional fields to `Http1KeepAliveRequest`.**

In `tests/differential/src/lib.rs`, extend the struct (`lib.rs:314-319`):

```rust
pub struct Http1KeepAliveRequest {
    pub method: String,
    pub path: String,
    pub host: String,
    pub expected_status: u16,
    /// 14.2 D8.1 (SPEC correction B-3): optional per-request body-byte assertion (each side
    /// independently equals these bytes). Reuses the existing `Http1BodyRule::ByteExact`.
    #[serde(default)]
    pub expected_body: Option<Http1BodyRule>,
    /// 14.2 D8.1: assert this (lower-cased) header NAME is PRESENT on each side's response.
    #[serde(default)]
    pub require_header_present: Option<String>,
    /// 14.2 D8.1: assert this (lower-cased) header NAME is ABSENT on each side's response.
    #[serde(default)]
    pub require_header_absent: Option<String>,
}
```

- [ ] **Step 4: Add `read_h1_response_full` + wire the assertions into the keep-alive exec arm.**

Add a helper that returns status + headers + body (refactor `read_h1_response_status` to
delegate, or add a parallel reader):

```rust
/// 14.2 D8.1: like `read_h1_response_status` but also returns the response headers (names
/// lower-cased) and the Content-Length-delimited body, so the keep-alive driver can assert
/// per-request body bytes + header presence/absence.
pub async fn read_h1_response_full<R>(stream: &mut R) -> Result<(u16, Vec<(String, String)>, Vec<u8>)>
where
    R: tokio::io::AsyncRead + Unpin,
{ /* status line + header parse (lower-case names) + Content-Length body read */ }
```

In the `Driver::Http1KeepAlive` exec arm (`lib.rs:2700-2734` region), replace the
`read_h1_response_status` call with `read_h1_response_full`, keep the
`status == req.expected_status` ensure, and add:

```rust
                    if let Some(Http1BodyRule::ByteExact { body }) = &req.expected_body {
                        anyhow::ensure!(
                            resp_body == body.as_bytes(),
                            "{side_name}: body mismatch for {} {} — expected {:?}, got {:?}",
                            req.method, req.path, body.as_bytes(), resp_body,
                        );
                    }
                    if let Some(h) = &req.require_header_present {
                        anyhow::ensure!(
                            resp_headers.iter().any(|(n, _)| n.eq_ignore_ascii_case(h)),
                            "{side_name}: expected header {h} present for {} {}",
                            req.method, req.path,
                        );
                    }
                    if let Some(h) = &req.require_header_absent {
                        anyhow::ensure!(
                            !resp_headers.iter().any(|(n, _)| n.eq_ignore_ascii_case(h)),
                            "{side_name}: expected header {h} absent for {} {}",
                            req.method, req.path,
                        );
                    }
```

- [ ] **Step 5: Run the round-trip test + the existing keep-alive serde tests + clippy + fmt.**

Run: `cargo test -p differential && cargo clippy -p differential --all-targets --all-features -- -D warnings && cargo fmt --all -- --check`
Expected: PASS / clean (the existing `driver_http1_keep_alive_round_trips_through_serde` +
`driver_http2_keep_alive_round_trips_through_serde` still pass — new fields are
`#[serde(default)]`).

- [ ] **Step 6: Commit.**

```bash
git add tests/differential/src/lib.rs
git commit -m "phase 14.2 Task 6: extend Driver::Http1KeepAlive with per-request body + header assertions (SPEC correction B-3)"
```

---

## Task 7: D8.1b — fixture `0022` + Docker-gated wrapper

**Files:**
- Create: `tests/fixtures/0022-upstream-outlier-detection-consecutive-5xx/envoy.yaml`
- Create: `tests/fixtures/0022-upstream-outlier-detection-consecutive-5xx/envoy-rust.yaml`
- Create: `tests/fixtures/0022-upstream-outlier-detection-consecutive-5xx/expectations.yaml`
- Create: `tests/fixtures/0022-upstream-outlier-detection-consecutive-5xx/README.md`
- Create: `tests/differential/tests/upstream_outlier_detection.rs` (Docker-gated wrapper)

- [ ] **Step 1: Author the two bootstraps (mirror fixture 0020's topology).**

`envoy.yaml` (reference) + `envoy-rust.yaml` (subject): a single H1 HCM listener on `{{PORT}}`
routing `/` and `/fail` to a single-endpoint cluster `c1` pointed at the
`health-aware-http1-backend` (started with `--per-path /fail=500`), an admin listener on
`{{ADMIN_PORT}}`, and:

```yaml
    outlier_detection:
      consecutive_5xx: 3
      base_ejection_time: 60s
      max_ejection_percent: 100
      interval: 1s
    common_lb_config:
      healthy_panic_threshold: { value: 0 }
```

(Use the 0020 `envoy.yaml`/`envoy-rust.yaml` as the structural template; only the cluster
`outlier_detection` + `common_lb_config` blocks + the `/fail` route differ.)

- [ ] **Step 2: Author `expectations.yaml` (the §6.2 item-6 bilateral assertions).**

```yaml
driver:
  kind: http1_keep_alive
  requests:
    - { method: GET, path: /fail, host: c1, expected_status: 500,
        expected_body: { byte_exact: { body: "server error\n" } },
        require_header_present: x-envoy-upstream-service-time }
    - { method: GET, path: /fail, host: c1, expected_status: 500,
        expected_body: { byte_exact: { body: "server error\n" } },
        require_header_present: x-envoy-upstream-service-time }
    - { method: GET, path: /fail, host: c1, expected_status: 500,
        expected_body: { byte_exact: { body: "server error\n" } },
        require_header_present: x-envoy-upstream-service-time }
    - { method: GET, path: /fail, host: c1, expected_status: 503,
        expected_body: { byte_exact: { body: "no healthy upstream" } },
        require_header_absent: x-envoy-upstream-service-time }
  settle_ms: 1500   # > interval (1s) so the post-eject stat scrape is stable
  expected_stats:
    - { name: cluster.c1.outlier_detection.ejections_active, value: 1 }
    - { name: cluster.c1.outlier_detection.ejections_enforced_total, value: 1 }
    - { name: cluster.c1.outlier_detection.ejections_enforced_consecutive_5xx, value: 1 }
    - { name: cluster.c1.outlier_detection.ejections_detected_consecutive_5xx, value: 1 }
    - { name: cluster.c1.outlier_detection.ejections_overflow, value: 0 }
# allowlist_envoy_only: <the deferred Envoy-side outlier_detection.* names envoy-rust does NOT
#   emit at minimum-viable scope — enumerated empirically against the real Envoy stat tree at
#   the state-3 Docker run; resolves M8's "14"-vs-13 count, Task 9>
```

> Note: `consecutive_5xx: 3` means requests 1–3 are backend-500s (counter 1→2→3 crosses the
> threshold on request 3 ⇒ endpoint ejected); request 4 finds no healthy endpoint ⇒ synth-503.
> `ejections_detected_consecutive_5xx == 1` (single threshold crossing); `ejections_overflow ==
> 0` (cap = 100% of 1 host = 1 ≥ the 1 active ejection only AFTER it ejects, but the cap is
> checked BEFORE ejecting when active_count = 0 < cap 1 ⇒ no overflow).

- [ ] **Step 3: Author the Docker-gated wrapper.**

`tests/differential/tests/upstream_outlier_detection.rs` mirrors the 12.2/13.x wrapper shape
(`#[ignore]` Docker-gated `#[tokio::test]` invoking the fixture runner against
`envoyproxy/envoy:v1.33.0`):

```rust
//! 14.2 D8.1: Docker-gated differential wrapper for fixture
//! 0022-upstream-outlier-detection-consecutive-5xx. Mirrors the 13.x
//! upstream_connection_pooling wrapper. Run with `--include-ignored` (CI) or `--ignored`.

#[tokio::test]
#[ignore = "requires Docker + envoyproxy/envoy:v1.33.0 (differential harness)"]
async fn fixture_0022_upstream_outlier_detection_consecutive_5xx() {
    differential::run_fixture("0022-upstream-outlier-detection-consecutive-5xx")
        .await
        .expect("fixture 0022 differential parity");
}
```

(Match the exact wrapper signature/helper name used by the 0020/0021 wrappers in
`tests/differential/tests/`.)

- [ ] **Step 4: Author the fixture README.** Brief: what the fixture exercises, the
  discriminating observable (status sequence 500,500,500,503 + the ejection counters), and the
  reused 12.2 no-healthy-upstream synth-503 contract row + the reused 13.x backend + driver.

- [ ] **Step 5: Run the wrapper locally against Docker (if available); else defer to state-4 CI.**

Run: `cargo test -p differential --test upstream_outlier_detection -- --ignored`
Expected: PASS bilaterally (fixture 0022 green; both proxies agree on status sequence, bodies,
header presence/absence, and the 5 outlier_detection counters). If Docker is unavailable
locally, the state-4 verification (Task 10) runs it in CI; note the deferral in PROGRESS.

- [ ] **Step 6: Commit.**

```bash
git add tests/fixtures/0022-upstream-outlier-detection-consecutive-5xx/ tests/differential/tests/upstream_outlier_detection.rs
git commit -m "phase 14.2 Task 7: fixture 0022-upstream-outlier-detection-consecutive-5xx + Docker-gated wrapper"
```

---

## Task 8: D8.3 — in-process backstop (eject + un-eject + 5-header presence)

**Files:**
- Create: `crates/envoy-bin/tests/upstream_outlier_detection.rs`

- [ ] **Step 1: Write the backstop test skeleton (mirror `upstream_connection_pooling.rs`).**

`#[tokio::test]` with `tokio::process::Command` + `.kill_on_drop(true)` + `Stdio::null()`/
`piped()`, booting `envoy-bin` with a synthesized bootstrap (single-endpoint cluster,
`outlier_detection {consecutive_5xx: 3, base_ejection_time: 5s, max_ejection_percent: 100,
interval: 1s}`, panic threshold 0) + the in-process `health-aware-http1-backend`
(`--per-path /fail=500`). Reuse the helpers from `upstream_connection_pooling.rs`
(`spawn_backend`, the keep-alive request reader, `scrape_admin_stats`).

```rust
#[tokio::test]
async fn outlier_detection_ejects_then_un_ejects() {
    // ... boot backend (/fail=500) + envoy-bin (base_ejection_time: 5s) ...

    // EJECT direction: 3× GET /fail → 500; 4th GET /fail → synth-503.
    let mut conn = open_keep_alive(proxy).await;
    for _ in 0..3 {
        let (s, _h, b) = h1_request(&mut conn, "/fail").await;
        assert_eq!(s, 500);
        assert_eq!(b, b"server error\n");
    }
    let (s, headers, body) = h1_request(&mut conn, "/fail").await;
    assert_eq!(s, 503);
    assert_eq!(body, b"no healthy upstream");
    // 5-standard-header presence on the synth-503 (SPEC §6.5):
    for h in ["server", "date", "content-length", "content-type", "connection"] {
        assert!(headers.iter().any(|(n, _)| n.eq_ignore_ascii_case(h)), "synth-503 missing {h}");
    }
    let stats = scrape_admin_stats(admin).await;
    assert_eq!(stats["cluster.c1.outlier_detection.ejections_active"], 1);

    // UN-EJECT direction: switch the backend to serve 200 on /fail, wait > base_ejection_time
    // (5s) + one interval tick, then a fresh request re-picks the un-ejected endpoint → 200.
    set_backend_status("/fail", 200).await; // or point at "/" which already serves 200
    tokio::time::sleep(std::time::Duration::from_secs(7)).await;
    let mut conn2 = open_keep_alive(proxy).await;
    let (s, _h, _b) = h1_request(&mut conn2, "/").await; // backend now healthy
    assert_eq!(s, 200, "endpoint un-ejected after base_ejection_time → re-picked");
    let stats = scrape_admin_stats(admin).await;
    assert_eq!(stats["cluster.c1.outlier_detection.ejections_active"], 0,
        "ejections_active gauge back to 0 after un-eject");
}
```

> The un-eject direction needs the backend to be healthy after ejection. Simplest: route `/`
> to the same cluster (the backend serves 200 + empty body on `/`), eject via `/fail`, then
> request `/` after the sweeper un-ejects. (Avoids a mid-test backend restart.)

- [ ] **Step 2: Run it.**

Run: `cargo test -p envoy-bin --test upstream_outlier_detection`
Expected: PASS (both directions; the un-eject path takes ~7s of real wall-time — acceptable for
an `envoy-bin` integration backstop, matching the 12.2 settle-then-probe precedent).

- [ ] **Step 3: Clippy + fmt.**

Run: `cargo clippy -p envoy-bin --all-targets --all-features -- -D warnings && cargo fmt --all -- --check`
Expected: clean.

- [ ] **Step 4: Commit.**

```bash
git add crates/envoy-bin/tests/upstream_outlier_detection.rs
git commit -m "phase 14.2 Task 8: in-process backstop — eject + un-eject + synth-503 5-header presence"
```

---

## Task 9: D9 docs + M8 reconciliation

**Files:**
- Modify: `docs/envoy-rust/phases/14.1-endpoint-ejection-and-lb-integration/SPEC.md` (M8 count, §2.1)
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (M8 count, `allowlist_envoy_only` paragraph; NO new row)
- Modify: `docs/envoy-rust/phases/14.2-response-receipt-hook-and-fixture/PROGRESS.md` (D9.2 attribution)

- [ ] **Step 1: M8 — reconcile the `allowlist_envoy_only` deferred-name count.**

Using fixture 0022's `expectations.yaml` `allowlist_envoy_only` list (authored in Task 7
against the REAL Envoy `cluster.c1.outlier_detection.*` emission at the state-3 Docker run),
correct the deferred-name count/enumeration in BOTH the 14.1 SPEC §2.1 AND
`BEHAVIOR_CONTRACT.md`'s `allowlist_envoy_only` paragraph so the prose count matches the
enumerated list (the 14.1 REVIEW M8: "14" claimed vs 13 enumerated). ~1 LoC each. Zero
differential impact (`allowlist_envoy_only` keys on actual emission, not the prose).

- [ ] **Step 2: D9.1 — confirm NO new BEHAVIOR_CONTRACT row.** Verify the no-healthy-upstream
  synth-503 row (`BEHAVIOR_CONTRACT.md:27-36`, 12.2-landed) + the `x-envoy-upstream-service-
  time` allow-list row (04.3-landed) are reused unchanged. No row added.

- [ ] **Step 3: D9.2 — PROGRESS attribution.** Narrate honestly (D-3.4) that the outlier-
  detection-driven `pick() -> None` reuses the 12.2 BEHAVIOR_CONTRACT row verbatim; fixture
  0022 asserts the 19-byte body as the discriminating observable.

- [ ] **Step 4: Commit.**

```bash
git add docs/envoy-rust/phases/14.1-endpoint-ejection-and-lb-integration/SPEC.md docs/envoy-rust/BEHAVIOR_CONTRACT.md docs/envoy-rust/phases/14.2-response-receipt-hook-and-fixture/PROGRESS.md
git commit -m "phase 14.2 Task 9: D9 docs + M8 allowlist_envoy_only count reconciliation"
```

---

## Task 10: State-4 phase-done verification

**Files:**
- Modify: `docs/envoy-rust/phases/14.2-response-receipt-hook-and-fixture/PROGRESS.md` (quote all gate outputs)
- Modify: `docs/envoy-rust/STATE.md` (advance to state-5-next)

> This is the state-4 transition (`superpowers:verification-before-completion`). Run the full
> §7.5 (a)–(e) gate set and quote every command's output into PROGRESS. Per
> `BOOTSTRAP_PROMPT.md` §5.1, this is its own session/state.

- [ ] **Step 1: Run the five stable-toolchain gates (e) and quote outputs into PROGRESS.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace
cargo deny check
```

- [ ] **Step 2: Run the differential suite (a)+(b) — 22 fixtures green simultaneously.**

```bash
cargo test -p differential -- --include-ignored
```
Expected: all 22 Docker-gated fixtures (0001–0022) green simultaneously vs
`envoyproxy/envoy:v1.33.0`.

- [ ] **Step 3: Confirm (c) h2spec ≥95% held vacuously** (no H2-framing touch) and **(d) the
  `parse_bootstrap` fuzz target clean on the unchanged 22-seed corpus** (short-budget run).

- [ ] **Step 4: Confirm the CI run for HEAD is `completed/success`; quote the run ID + HEAD SHA
  into PROGRESS** (the state-4 evidence anchor).

- [ ] **Step 5: Advance STATE.md to `14.2` state-5-next** (next skill:
  `superpowers:requesting-code-review`); commit.

```bash
git add docs/envoy-rust/phases/14.2-response-receipt-hook-and-fixture/PROGRESS.md docs/envoy-rust/STATE.md
git commit -m "phase 14.2 Task 10: state-4 phase-done verification + STATE advance to state-5-next"
```

---

## Task 11: State-6 CLOSING-sub-phase close-out (next session, after state-5 REVIEW.md approved)

> Documented here per SPEC §3 D10 + §6.11 as the final task. **This is state-6 work, NOT this
> PLAN-write session and NOT state-3/4.** It lands only after the state-5 `REVIEW.md` is
> approved (re-enter state 3 per §5.2 if it surfaces Critical/Important findings — in
> particular the M4 discharge verification).

**Files:**
- Modify: `docs/envoy-rust/ROADMAP.md` (flip rows `14.2` AND parent `14` `in-progress → done` SIMULTANEOUSLY)
- Modify: `docs/envoy-rust/STATE.md` (advance to "awaiting next planning")

- [ ] **Step 1: Flip ROADMAP rows `14.2` AND `14` to `done` in one commit** (the closing-sub-
  phase invariant; mirrors 12.2 `3ec7fb9` / 13.2 `96630f9`).

- [ ] **Step 2: Advance STATE.md** to "awaiting next planning"; append the `### Phase-14.2
  rollovers` Notes subsection (carryforward dispositions: M4 verified at REVIEW; the residual
  Minor inventory).

- [ ] **Step 3: Commit with the closing-sub-phase title (SPEC §9).**

```bash
git commit -m "phase 14.2: response-receipt hook (H1+H2) + ejection sweeper + fixture 0022 + parent-14 close [parent 14 done]"
```

(Commit body per SPEC §9: 1–3 sentence summary; `Differential surface:` line naming fixture
0022 + the 22-fixture green CI run ID + HEAD SHA + parent-14 flip; `Conformance:` line noting
h2spec ≥95% held. NO `[ADR-NNNN]` bracket — no ADR fires at 14.2 per lock-in #2.)

---

## Self-review (run at PLAN-write per `superpowers:writing-plans`)

**1. Spec coverage.** D4 (Tasks 2+3, H1+H2 with the connect-failure classification + no-healthy
bypass per §2.3) ✅; D7 (Task 4 sweeper + Task 5 wiring, fourth periodic-background primitive,
CancellationToken + shutdown) ✅; D8.1 (Tasks 6+7, fixture 0022 + Docker wrapper + the driver
extension) ✅; D8.2 (already landed at 14.1 — corpus 22, NOT re-planned) ✅ (SPEC §3); D8.3
(Task 8, both convergence directions + 5-header presence) ✅; D9.1/D9.2 (Task 9, contract
non-amendment + attribution) ✅; D10 (Task 11, parent-14 close-out) ✅. §6.2 lock-ins 5/6/8/9
pulled forward (§0.A #3) ✅. M4 discharged (§0.A #4, Task 1) ✅; M5/M6 (Task 1) ✅; A-M2 (Task
2) ✅; M8 (Task 9) ✅. Split-gate (§0.A #1): 11 tasks / ~1100–1300 LoC ≪ 25 / 1500 → NO SPLIT ✅.

**2. Placeholder scan.** No "TBD"/"implement later"/"add appropriate X". The one empirical
deferral (the `allowlist_envoy_only` enumeration, Task 7 Step 2 / Task 9 Step 1) is explicitly
gated on the state-3 Docker run because the deferred Envoy name set is not knowable without
running Envoy (the §6.2 verification is locked at `0a4d225` per ADR-0041 and must NOT be
re-run at PLAN-write) — this is a genuine execution-time empirical step, not a vague TODO.

**3. Type/name consistency.** `Cluster::record_response(endpoint: SocketAddr, status: u16)`,
`ClusterHandle::record_response`, `EndpointEjection::{record_response, eject, try_un_eject,
is_ejected, ejected_at}`, `DetectorType::{Consecutive5xx, ConsecutiveGatewayFailure}`,
`OutlierDetectionState::{endpoints, max_ejection_percent, ejections_overflow,
base_ejection_time, interval}`, `OutlierEjectionSweeper::{spawn, shutdown}`, `OutlierManager::
{for_bootstrap, shutdown}`, `Http1KeepAliveRequest::{expected_body, require_header_present,
require_header_absent}`, `Http1BodyRule::ByteExact { body }`, `read_h1_response_full` — all
consistent across tasks and matched against HEAD `b0dea44f`. The `outgoing` (H1) /
`upstream_resp` (H2) status sources are the verified on-disk bindings. **No spec requirement
without a task.**
