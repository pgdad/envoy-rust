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

## Task 1 — _(pending state-3 execution)_
## Task 2 — _(pending)_
## Task 3 — _(pending)_
## Task 4 — _(pending)_
## Task 5 — _(pending)_
## Task 6 — _(pending)_
## Task 7 — _(pending)_
## Task 8 — _(pending)_
## Task 9 — _(pending)_
## Task 10 — _(pending state-4 verification)_
## Task 11 — _(pending state-6 close-out)_
