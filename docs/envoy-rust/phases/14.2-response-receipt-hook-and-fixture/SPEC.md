# Phase 14.2 (`14.2-response-receipt-hook-and-fixture`) — SPEC

- **Phase id:** `14.2`
- **Slug:** `14.2-response-receipt-hook-and-fixture`
- **Status before this SPEC lands:** _not yet in ROADMAP.md_ (created at the parent-14 state-2 SPLIT commit; alongside this SPEC, ROADMAP gains a new row `14.2 status: planned` under the existing Upstream-robustness-family §9 table — see ADR-0040).
- **Parent SPEC:** `docs/envoy-rust/phases/14-outlier-detection/SPEC.md` (parent-14 state-1 brainstorm, committed at the predecessor of THIS commit). The parent SPEC §6.1 + §7 explicitly recommend this split shape.
- **Sibling SPEC:** `docs/envoy-rust/phases/14.1-endpoint-ejection-and-lb-integration/SPEC.md` — the foundation slice landing schema + validator + per-endpoint state machine + LB integration + stats. 14.2 depends on 14.1 (the state-machine seam + the stat-namespace registration must precede the response-receipt hook + sweeper that mutate them).
- **Charter source:** `BOOTSTRAP_PROMPT.md` §9 — *"Upstream robustness family — outlier detection variants"* — projected onto the **observable-behavior slice + parent close**: the H1+H2 router-arm response-receipt hooks (D4) + the per-cluster ejection sweeper (D7) + fixture `0022-upstream-outlier-detection-consecutive-5xx` (D8.1) + the fuzz seed (D8.2 IF deferred from 14.1) + the in-process backstop (D8.3) + the parent-14 ROADMAP close-out (the closing-sub-phase invariant — flips rows `14.2` AND parent `14` `in-progress → done` SIMULTANEOUSLY).
- **Position in the project:** the **closing sub-phase** of the third concrete Upstream-robustness-family phase (parent-14 outlier detection). After 12.2 (`active-http-probe-and-fixture` — closing parent-12) + 13.2 (`h2-pool-and-cx-total-tightening` — closing parent-13), this is the **third closing-sub-phase in the Upstream-robustness family**. Lands the **fourth periodic-background primitive** (the ejection sweeper) after the foundation triad (12.2 active-HC scheduler + 13.1 H1 pool idle sweeper + 13.2 H2 pool idle sweeper) — identical `tokio_util::sync::CancellationToken` cancellation discipline + `pub async fn shutdown(self)` on the manager.
- **depends-on:** `14.1` (the state-machine seam) — the response-receipt hook (D4) calls `Cluster::record_response`, which transitions the 14.1-landed `EndpointEjection` state. The sweeper (D7) calls `EndpointEjection::try_unEject`. The fixture (D8.1) exercises the end-to-end seam via the discriminating observable.

---

## 1. Goal and acceptance signal

Phase 14.2 lands the **observable-behavior surface** of passive outlier-detection ejection. The H1 router-proxy arm at `crates/envoy-http1/src/router.rs::write_proxied_response` (and the H2 sibling at `crates/envoy-http2/src/hcm.rs` post-dispatch site) calls `cluster.record_response(endpoint, status)` AFTER the existing `upstream_rq_total` + `upstream_rq_5xx` increments fire and BEFORE the downstream response write. The per-cluster `OutlierEjectionSweeper` (a new module `outlier.rs` inside `envoy-cluster`) ticks every `interval` and un-ejects past-deadline endpoints. Fixture `0022` exercises the end-to-end seam: 3 consecutive backend-500s eject the single endpoint; request 4+ returns the 19-byte `no healthy upstream` synth-503 path (the 12.2-landed BEHAVIOR_CONTRACT row reused unchanged).

**Differential surface added by phase 14.2:**

- **Fixture `0022-upstream-outlier-detection-consecutive-5xx`** — bilateral assertion that both proxies, given identical bootstraps configuring an H1 cluster with `outlier_detection: {consecutive_5xx: 3, base_ejection_time: 60s, max_ejection_percent: 100, interval: 1s}` + `common_lb_config.healthy_panic_threshold: {value: 0}` + a single-endpoint cluster pointed at the 13.x configurable-status backend serving 5xx on `/fail`, drive a sequence of 4 sequential GET `/fail` over a single downstream keep-alive conn (reusing 13.1's `Driver::Http1KeepAlive` verbatim). Per §6.2 item-6 (ratified by ADR-0041): requests 1-3 return backend 500 + `server error\n` body (13 bytes from the 12.2 helper backend) + `x-envoy-upstream-service-time` header PRESENT; request 4 returns synth-503 + `no healthy upstream` body (19 bytes) + `x-envoy-upstream-service-time` header ABSENT. Counter assertions: `cluster.<name>.outlier_detection.ejections_active == 1`, `ejections_enforced_total == 1`, `ejections_enforced_consecutive_5xx == 1`, `ejections_detected_consecutive_5xx == 1`, `ejections_overflow == 0`.

**Acceptance signal (a)–(f), per `BOOTSTRAP_PROMPT.md` §7.5:**

- **(a)** Fixture `0022-upstream-outlier-detection-consecutive-5xx` green at Docker-gated CI.
- **(b)** All **21 pre-existing differential fixtures** (`0001-tcp-echo` through `0021-upstream-h2-connection-pooling`) **remain green simultaneously** at the same CI run + the new fixture 0022 → **22 Docker-gated fixtures green** at a single CI run (regression-equivalence per §7.5 (b)).
- **(c)** `h2spec` continues at ≥95% (parent-05 baseline 99.31%). 14.2's H2 response-receipt hook fires at the H2 HCM post-dispatch site, NOT at the H2 framing/codec path; the state-4 verification re-confirms the gate held.
- **(d)** `parse_bootstrap` fuzz target clean for the short-budget CI run on the extended corpus (the `cluster_outlier_detection.yaml` seed — landed at 14.1 per recommendation OR 14.2 if 14.1 deferred). Corpus 21 → 22 (or 22 if 14.1 landed at corpus 22).
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check` all clean.
- **(f)** `REVIEW.md` approved.

A **single CI run** must light up gates (a) through (e) **simultaneously**. The parent-14 ROADMAP row flips `in-progress → done` at THIS sub-phase's state-6 close-out commit per the closing-sub-phase invariant (mirrors the 02.2 / 03.2 / 07.2 / 08.2 / 12.2 / 13.2 precedents).

---

## 2. Behavior-contract scope for phase 14.2

Phase 14.2 extends `docs/envoy-rust/BEHAVIOR_CONTRACT.md` with authored additions, landed at the tasks where each is first empirically exercised (per the established 06.x → 13.2 doctrine).

### 2.1 No-healthy-upstream synth-503 path: REUSED unchanged at 14.2

Phase 14.2's ejection-driven `pick() -> None` reuses the 12.2-landed no-healthy-upstream synth-503 path (`crates/envoy-http1/src/hcm.rs:582`) verbatim. The BEHAVIOR_CONTRACT.md `Response body — no-healthy-upstream synth-503` row at `BEHAVIOR_CONTRACT.md:27-36` covers the wire shape; phase 14.2 does NOT amend the row. The fixture 0022 asserts the 19-byte `no healthy upstream` body bilaterally as the discriminating observable when the ejection forces `pick() -> None`.

Per the §6.2 item-6 empirical capture (ADR-0041): the synth-503 carries `content-length: 19`, `content-type: text/plain`, `server: envoy` AND specifically does NOT carry `x-envoy-upstream-service-time` (the upstream-side timing header). The 19-byte body bytes are `6e 6f 20 68 65 61 6c 74 68 79 20 75 70 73 74 72 65 61 6d` (hex), NO trailing newline (the ADR-0037 / 12.2-landed shape).

### 2.2 H2 ejection-driven synth-503 path: REUSED unchanged at 14.2

Per §6.2 item-8 (ADR-0041): the H2 sibling of the no-healthy-upstream synth-503 path lives in `crates/envoy-http2/src/hcm.rs` and emits the same 19-byte body under the same circumstances. The §6.2 verification confirmed H2-side parity on connect-failure ejection (the H2 connect-failure path triggers identical `cluster.<name>.outlier_detection.*` stat ticks + `no healthy upstream` body). **No H2 wire-shape gap surfaced.** No new ADR projected for the H2 path.

### 2.3 Response-receipt hook semantics (§6.2 item-9 + connect-failure classification — ratified by ADR-0041)

The `Cluster::record_response(endpoint, status)` hook (declared at 14.1 D3; wired at 14.2 D4) classifies the response status per the §6.2 item-9 empirical finding (ratified by ADR-0041 at the parent-14 state-2 SPLIT commit):

| Response source | Classification | Hook called? |
|---|---|---|
| Backend-emitted 500 (real upstream 5xx response) | `consecutive_5xx` tick only | Yes (router-arm dispatch site) |
| Backend-emitted 502/503/504 (gateway-failure subset) | BOTH `consecutive_5xx` AND `consecutive_gateway_failure` tick | Yes |
| Backend-emitted 2xx / 3xx / 4xx | Per-endpoint counters RESET to 0 | Yes |
| envoy-rust connect-failure synth-503 / synth-502 (picked endpoint failed to connect) | BOTH `consecutive_5xx` AND `consecutive_gateway_failure` tick (attributed to the picked endpoint that failed; §6.2 item-9 confirmed) | Yes (the connect-failure synth path calls `record_response`) |
| envoy-rust no-healthy-upstream synth-503 (`pick() -> None`) | **NO TICK** — no endpoint was picked, no attribution target | **No** (the synth-503 path SHORT-CIRCUITS — no `record_response` call) |

The synth-status bypass per parent §5.7 is **NUANCED** by the §6.2 item-9 finding: the bypass applies ONLY to the `pick() -> None` synth-503 path (no endpoint to attribute), NOT to the connect-failure synth-502/synth-503 paths (picked endpoint, classified-as-5xx-AND-gateway-failure mirroring Envoy). The state-3 implementer wires the bypass at the router-arm dispatch site: the connect-failure path calls `record_response`; the no-healthy-upstream path does not.

### 2.4 No DECISIONS.md amendment required at 14.2 state-2 PLAN-write

Phase 14.2's response-receipt + sweeper + fixture surfaces are mechanical extension of the parent SPEC + 14.1's foundation. The split ADR (`ADR-0040`) + the §6.2 empirical-revision ADR (`ADR-0041`) landed at the parent-14 state-2 SPLIT commit; 14.2's own state-2 PLAN-write commit lands no ADR projected per §7 below.

---

## 3. Deliverables

Phase 14.2's deliverables are **D4, D7, D8.1, D8.3, and D8.2 (if not landed at 14.1)** from the parent-14 SPEC §3, plus the parent-14 ROADMAP close-out. The 14.2 PLAN-writer organizes these into tasks per `BOOTSTRAP_PROMPT.md` §5 state 2.

### D4 — Response-receipt hooks (H1 + H2 router-proxy arms)

Modify the H1 router-proxy-arm response-receipt site at `crates/envoy-http1/src/router.rs::write_proxied_response` (and the H2 sibling at `crates/envoy-http2/src/hcm.rs` post-dispatch site) to call `cluster.record_response(endpoint, status)` AFTER the existing `upstream_rq_total` + `upstream_rq_5xx` increments fire AND BEFORE the response is written downstream. The ordering is load-bearing (the counter increments are unconditional; the ejection-decision side-effect must complete before downstream observers see the response).

**The connect-failure synth path also calls `record_response`** (per §2.3 + §6.2 item-9 lock-in): the router-arm's connect-failure handler classifies the synth-503 as both `consecutive_5xx` + `consecutive_gateway_failure` for the picked endpoint that failed.

**The no-healthy-upstream synth-503 path does NOT call `record_response`** (no picked endpoint to attribute to).

**Inert when unconfigured:** `Cluster::record_response` short-circuits at the cluster-level `outlier_detection.is_none()` check before reaching any state-machine mutation; for clusters without `outlier_detection` the hook cost is one cheap `Option::is_some()` check (or an equivalent atomic-load fast-path).

### D7 — Ejection sweeper (the fourth periodic-background primitive)

A per-cluster background `tokio::time::interval`-driven task spawned at cluster construct time when `outlier_detection` is configured. New module `crates/envoy-cluster/src/outlier.rs` mirroring the 12.2 `envoy-health::Scheduler` topology + the 13.1/13.2 H1+H2 pool idle sweepers verbatim:

```rust
// crates/envoy-cluster/src/outlier.rs

pub struct OutlierEjectionSweeper {
    cluster_name: String,
    endpoints: Arc<Vec<Arc<EndpointEjection>>>,
    config: Arc<OutlierDetectionConfig>,
    cancel: CancellationToken,
    join: tokio::task::JoinHandle<()>,
}

impl OutlierEjectionSweeper {
    pub fn spawn(cluster_name: String,
                 endpoints: Arc<Vec<Arc<EndpointEjection>>>,
                 config: Arc<OutlierDetectionConfig>) -> Self { /* ... */ }

    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.join.await;
    }
}

pub struct OutlierManager {
    sweepers: HashMap<String, OutlierEjectionSweeper>,
}
```

Per the §6.2 item-5 lock-in: at each `interval` tick, re-evaluate each ejected endpoint; un-eject if `now_monotonic - eject_time >= base_ejection_time`. Per-endpoint `consecutive_5xx` and `consecutive_gateway_failure` counters reset to 0 at un-eject (and also at any 2xx/3xx/4xx response per D4 — both pathways reset; this is the §6.2 item-5 lock-in). `ejections_active` gauge decrements at each un-eject.

**Cancellation discipline:** identical `tokio_util::sync::CancellationToken` + `pub async fn shutdown(self)` on `OutlierManager` (and a single-sweeper shutdown for surgical control) mirroring 12.2 + 13.1 + 13.2.

**Crate placement + cycle-resolution:** the sweeper lives inside `envoy-cluster` as a new module `outlier.rs` (~200-300 LoC). `envoy-cluster` already uses tokio (STRICT_DNS via `tokio::net::lookup_host`); adding a background task is consistent with `envoy-http1::pool` (13.1) + `envoy-http2::pool` (13.2). **No new crate; no cycle.** `OutlierManager` is an external sibling registry to `ClusterManager` (mirrors `H1PoolManager`/`H2PoolManager`/`envoy-health::Scheduler` verbatim). `envoy-bin` wires it at startup. **No new ADR projected for the architecture** — ordinary structure mirroring the established 12.2 + 13.x patterns.

### D8 — Fixture + Docker wrapper + in-process backstop

- **D8.1 — Fixture `tests/fixtures/0022-upstream-outlier-detection-consecutive-5xx/`** — configures:
  - A cluster with **one endpoint** pointing at the 13.x `health-aware-http1-backend` (the configurable-status backend at `tests/helpers/health-aware-http1-backend/`; reused VERBATIM per parent §3 D8.1) serving 5xx on `/fail` and 200 on `/`.
  - `outlier_detection: {consecutive_5xx: 3, base_ejection_time: 60s, max_ejection_percent: 100, interval: 1s}`.
  - `common_lb_config.healthy_panic_threshold: {value: 0}` (panic disabled — ejection is the only signal driving `pick() -> None`).
  - An H1 HCM listener routing `/` and `/fail` to the cluster.

  **Driver:** reuses 13.1's `Driver::Http1KeepAlive` verbatim — no new harness primitive required.

  **Workload:** 4 sequential GET `/fail` over a single downstream keep-alive conn.

  **Expectations.yaml (bilateral assertions per §6.2 item-6 lock-in ratified by ADR-0041):**
    - Request 1-3: status `500`, body `server error\n` (13 bytes from the backend's `per_class_body(500)`), header `x-envoy-upstream-service-time` PRESENT.
    - Request 4: status `503`, body `no healthy upstream` (19 bytes — the 12.2-landed BEHAVIOR_CONTRACT row reused), header `x-envoy-upstream-service-time` ABSENT, headers `content-length: 19` + `content-type: text/plain` PRESENT.
    - Post-settle admin scrape: `cluster.c1.outlier_detection.ejections_active == 1`, `ejections_enforced_total == 1`, `ejections_enforced_consecutive_5xx == 1`, `ejections_detected_consecutive_5xx == 1`, `ejections_overflow == 0`.
    - `allowlist_envoy_only` covers the 14 Envoy-side stat names envoy-rust does NOT emit at minimum-viable scope (§2.1 of 14.1 SPEC).

  Docker-gated wrapper at `tests/differential/tests/upstream_outlier_detection.rs` mirroring the 12.2 / 13.x wrapper shape.

- **D8.2 — Fuzz corpus seed.** New file `crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_outlier_detection.yaml` containing the `outlier_detection` bootstrap shape. Mirrors the 12.2 / 13.x corpus-seed precedent. **Recommended seam: land at 14.1** (per 14.1 SPEC §3); if deferred to 14.2, lands here. The companion files (`crates/envoy-config/fuzz/.gitignore` allow-list + `bootstrap.rs::tests::fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS-array) edit alongside per the 09-13.2 corpus-seed convention.

- **D8.3 — In-process backstop.** New file `crates/envoy-bin/tests/upstream_outlier_detection.rs` mirroring the 12.2 / 13.x backstop shape with the standing `tokio::process::Command` + `.kill_on_drop(true)` + `Stdio::null()`/`piped()` discipline. Boots `envoy-bin` with a synthesized bootstrap + an in-process configurable-status backend; exercises:
  - **Ejection direction:** 3 backend-500s → 4th request synth-503 (status, body bytes, 5-standard-header presence on the synth-503 — per the 10/11/12.2/13.x in-process-backstop discipline).
  - **Un-ejection direction:** after `base_ejection_time` (use a short timer like 5s for testing), the endpoint un-ejects + the next request to a backend now serving 200 returns 200 (`ejections_active` gauge back to 0; per-endpoint counters reset). Per §6.2 item-5 lock-in.

  Per parent §6.4: include BOTH convergence directions.

### D9 — Documentation deliverables (rolled in with prior tasks)

- **D9.1 — BEHAVIOR_CONTRACT.md non-amendment** — the no-healthy-upstream synth-503 row (12.2-landed) is REUSED unchanged. The `x-envoy-upstream-service-time` allow-list row (04.3-landed) is REUSED unchanged. Phase 14.2 lands NO new BEHAVIOR_CONTRACT row.
- **D9.2 — PROGRESS.md attribution** — outlier-detection-driven `pick() -> None` reuses the 12.2 BEHAVIOR_CONTRACT row verbatim; PROGRESS narrates the reuse honestly (D-3.4).

### D10 — Parent-14 ROADMAP close-out (the closing-sub-phase invariant)

At the 14.2 state-6 close-out commit, flip ROADMAP rows `14.2` AND parent `14` `in-progress → done` SIMULTANEOUSLY in one commit (mirrors the 02.2 / 03.2 / 07.2 / 08.2 / 12.2 / 13.2 closing-sub-phase precedents). The commit message carries `[parent 14 done]` per §9 below.

---

## 4. Out of scope (deferred to 14.1 or out per parent §4)

- **D1 envoy-config schema** → 14.1.
- **D2 envoy-config validator** → 14.1.
- **D3 EndpointEjection state machine** → 14.1.
- **D5 Cluster::pick() ejection-filter integration** → 14.1.
- **D6 outlier-detection stats wiring + BEHAVIOR_CONTRACT extension** → 14.1.
- **D8.2 fuzz seed** → recommended at 14.1; if deferred, lands at 14.2 D8.2.
- **All success-rate-based / failure-percentage-based / local-origin-failure / `enforcing_*` / `max_ejection_time*` / event-log / TCP-side detectors** — defer per parent §4 (rejected by `deny_unknown_fields` at 14.1 D1).
- **The `* num_ejections` ejection-time multiplier** — out per parent §4.
- **A dedicated H2-side outlier-detection fixture** — defers per parent §4; the §6.2 item-8 verification confirmed H2 parity. 14.2's fixture exercises H1; the H2 router-arm hook fires symmetrically via D4 but no separate H2 fixture lands.

---

## 5. Architectural invariants

Phase 14.2 honors the parent-14 SPEC §5 invariants verbatim. The slice-specific signposts:

### 5.1 Crate boundaries (carryforward from 14.1)

- **Response-receipt hook fires from `envoy-http1` and `envoy-http2`** (the router-proxy-arm response-receipt sites) — these crates already depend on `envoy-cluster` (the router holds `Arc<Cluster>`), so the `Cluster::record_response` method declared at 14.1 D3 is reachable without cycle inversion. **No new crate; no cycle.**
- **Ejection sweeper lives inside `envoy-cluster`** as a new module `outlier.rs` (mirrors 13.1 H1 pool sweeper inside `envoy-http1::pool` + 13.2 H2 pool sweeper inside `envoy-http2::pool`). `OutlierManager` is an external sibling registry to `ClusterManager` (mirrors `H1PoolManager`/`H2PoolManager`/`envoy-health::Scheduler` patterns verbatim).
- **No new top-level Cargo dep; no new workspace member.** The state-3 implementer verifies the dependency graph stays acyclic.

### 5.2 Hand-rolled per D-3.2

The response-receipt hook + sweeper are hand-rolled per D-3.2 (parent §5.2). No external passive-health-check library; no `rand` (no jitter at phase-14 scope).

### 5.3 Outlier-detection is inert when unconfigured (regression-equivalence)

When `Cluster.outlier_detection` is absent, the response-receipt hook short-circuits at the cluster-level `outlier_detection.is_none()` check; NO sweeper spawns. The 21 existing fixtures see ZERO behavior change (acceptance gate (b)).

### 5.4 The fourth periodic-background primitive

Phase 14.2 introduces the **fourth periodic timer-driven background task** (after 12.2 active-HC scheduler + 13.1 H1 pool idle sweeper + 13.2 H2 pool idle sweeper). All four share identical `tokio_util::sync::CancellationToken` cancellation discipline + `pub async fn shutdown(self)` on the manager. The state-3 implementer ensures graceful cancellation (no leaked tasks on cluster destroy; tests assert task shutdown).

### 5.5 Synth-status bypass nuance (§6.2 item-9 lock-in via ADR-0041)

Per §2.3 above: the synth-status bypass applies ONLY to the `pick() -> None` synth-503 path (no endpoint to attribute). Connect-failure synth-503/synth-502 paths DO call `record_response` (attributed to the picked endpoint that failed). The state-3 implementer wires the bypass at the router-arm dispatch site explicitly.

### 5.6 The 12.x + 13.x seam reuse (no new harness primitive)

Phase 14.2 reuses:
- The 12.2-landed no-healthy-upstream synth-503 path verbatim (`BEHAVIOR_CONTRACT.md:27-36`).
- The 12.x `EndpointHealth` / `pick()` exclusion seam (via the 14.1-landed AND-composition).
- The 13.x `health-aware-http1-backend` synthetic backend (`--per-path PATH=STATUS,...`) verbatim.
- The 13.1-landed `Driver::Http1KeepAlive` harness driver verbatim.

**This is the primary reason architectural risk is LOW-MEDIUM despite the new subsystem.**

---

## 6. Implementation signposts for the planner

The 14.2 state-2 PLAN-writer reads this section to drive PLAN structure.

### 6.1 Split-gate evaluation at 14.2

Per `BOOTSTRAP_PROMPT.md` §6.1, the 14.2 state-2 PLAN-write evaluates whether the closing-slice PLAN exceeds ~25 numbered tasks OR ~1500 LoC. Phase 14.2's surface estimate at THIS SPEC's time:

- D4 — response-receipt hooks (H1 + H2 router-arm modifications + synth-status bypass wiring) (~120 LoC modify + ~180 LoC tests).
- D7 — ejection sweeper (`outlier.rs` module + `OutlierManager` + envoy-bin wiring) (~230 LoC + ~210 LoC tests).
- D8.1 — fixture 0022 + Docker wrapper (~120 LoC YAML/wrapper).
- D8.2 — fuzz seed (~30 LoC + 2 file edits; deferred from 14.1 if applicable).
- D8.3 — in-process backstop (~280 LoC; both convergence directions).
- State-4 verification + STATE-advance + state-6 closing-sub-phase close-out (~docs).

**14.2 SPEC-time projection: ~9-11 tasks; ~900-1200 LoC** (production ~350, tests ~390, fixture/backstop ~400, docs). **Comfortably under the §6.1 gates.** No nested split projected.

### 6.2 Empirical verification — ratified by ADR-0041 at parent-14 state-2 SPLIT

The 14.2 state-2 PLAN-write does NOT re-run the §6.2 9-item empirical verification (the parent-14 state-2 split commit already ratified the findings via ADR-0041). The 14.2 PLAN-writer pulls forward the relevant §6.2 lock-ins:

- **Item 5 (ejection-time semantics):** un-eject at next interval-tick after `now_monotonic - eject_time >= base_ejection_time`; per-endpoint counters reset on un-eject AND on any 2xx/3xx/4xx response. D7 + D4 lock.
- **Item 6 (fixture observable):** request 1-3 backend 500 + `server error\n` + `x-envoy-upstream-service-time` PRESENT; request 4+ synth-503 + `no healthy upstream` + `x-envoy-upstream-service-time` ABSENT; counter values exact. D8.1 locks the bilateral assertions.
- **Item 8 (H1 vs H2 sibling):** identical stat namespace; identical ejection-on-5xx semantics; the H2 router-arm hook fires at the H2 HCM post-dispatch site. D4 (H2 side) locks.
- **Item 9 (synth-status bypass nuance + connect-failure classification):** §2.3 above; D4 wires the bypass at the router-arm dispatch site explicitly.

§6.2 items 1-4 + 7 (config defaults, stat namespace, initial state, max_ejection_percent cap, HC composition) defer their PLAN lock-ins to **14.1** (the foundation slice that lands schema + state machine + LB integration + stats).

### 6.3 PLAN-time SPEC corrections (per the 06.2 → 13.2 cadence)

The 14.2 state-2 PLAN-writer reads this SPEC against the PLAN-time HEAD and verifies the exact code surfaces:

- The exact H1 router-proxy-arm response-receipt site at `crates/envoy-http1/src/router.rs::write_proxied_response` (the existing `upstream_rq_total` + `upstream_rq_5xx` increment-fire ordering; D4 inserts the new call AFTER both).
- The exact H2 sibling site at `crates/envoy-http2/src/hcm.rs` post-dispatch (the increment-fire ordering — D4 inserts the new call AFTER).
- The exact connect-failure synth path (the picked-endpoint attribution site at both H1 + H2 router arms).
- The exact no-healthy-upstream synth-503 path at `crates/envoy-http1/src/hcm.rs:582` + H2 sibling (the `pick() -> None` short-circuit site — D4 does NOT call `record_response` here).
- The exact 14.1-landed `Cluster::record_response` signature + cluster-level `outlier_detection.is_none()` short-circuit.
- The exact 13.x `health-aware-http1-backend` `--per-path PATH=STATUS,...` flag + the `per_class_body` shape (the 12.2-landed body bytes that fixture 0022's request-1-3 captures bilaterally).
- The exact `Driver::Http1KeepAlive` shape in `tests/differential/src/lib.rs:167-173`.

Corrections land in the 14.2 PROGRESS Task 1 preamble per the 06.2 → 13.2 cadence.

### 6.4 Subagent-driven execution at state 3 (per `feedback_execution_style`)

The user's standing preference auto-memory `feedback_execution_style` applies at 14.2 state 3. The 14.2 state-2 PLAN-writer organizes tasks for subagent-driven execution per the 06.x → 13.2 cadence.

### 6.5 In-process backstop assertions (heeds the 10/11/12.2/13.x lesson)

D8.3 SHOULD exercise BOTH the ejection direction (post-3-5xx → synth-503 fires) AND the un-eject direction (post-`base_ejection_time` → endpoint re-picks; backend now serving 200 → 200 returns). Include the per-probe 5-standard-header presence assertion on the synth-503 response (`server`, `date`, `content-length`, `content-type`, `connection`). Recommended: include all three checkpoints (eject + un-eject + header presence).

For the in-process backstop's `base_ejection_time`, use a short timer (e.g. 5s) — empirical un-eject convergence at envoyproxy/envoy:v1.33.0 lands at ~5-6s for `base_ejection_time: 5s` per §6.2 item-5 capture; envoy-rust must match the same window.

### 6.6 The 06.x stats convention

StatsRegistry registration at cluster-construct time when `outlier_detection` is configured (the 14.1-landed pattern); per-cluster ownership of the Counter/Gauge handles; the `ejections_active` gauge updated inline at each ejection / un-ejection state transition (one source of truth, NOT polled — the 08.2 `server.live` / 12.1 `membership_healthy` pattern). The 14.2 D4 + D7 sites mutate the 14.1-landed registry handles.

### 6.7 The BEHAVIOR_CONTRACT extension cadence

14.2 lands NO new BEHAVIOR_CONTRACT row. The no-healthy-upstream synth-503 row (12.2-landed) is REUSED unchanged. The 14.1-landed stat rows are reused unchanged.

### 6.8 Cargo.lock cadence

Phase 04.1 REVIEW M5/M9 carries forward. Phase 14.2 adds zero new top-level Cargo deps.

### 6.9 Known-deferred small follow-up from 13.x (opportunistic close candidate)

The 13.1-surfaced **cluster per-class `upstream_rq_{2,3,4}xx` counter family extension** (carried from 13.1 / 13.2 / 14.1) is an opportunistic close candidate at 14.2 IF the PLAN-writer judges the extension cost is small. Phase 14.2's fixture 0022 exercises 5xx responses heavily; the per-class extension would surface cleanly in the bilateral assertions. **Recommended posture: defer unless cheap** — observability work, not outlier-detection work; folding it in inflates 14.2's scope.

### 6.10 Carryforward inventory entering 14.2 (all carry forward unchanged per parent §6.3)

All carryforwards entering parent-14 carry forward through 14.1 (which closes none) to 14.2. **A-M2** (13.2 stale `tokio::sync::Mutex` comment at `crates/envoy-http1/src/pool.rs:322`) — 14.2 DOES touch `envoy-http1::router` (D4); the PLAN-writer may opportunistically close A-M2 at the same task IF the touch surface is adjacent (recommended: include in the relevant D4 sub-task). **ADR-0028** (H1-listener × H2-cluster dispatch deferral) — carries forward per ADR-0039 Consequences.

### 6.11 Parent-14 close-out at 14.2 state-6 (the closing-sub-phase invariant)

The 14.2 state-6 close-out commit MUST flip ROADMAP rows `14.2` AND parent `14` `in-progress → done` SIMULTANEOUSLY (single commit) per the closing-sub-phase invariant. Commit message carries `[parent 14 done]` (mirrors the 12.2 `[parent 12 done]` + 13.2 `[parent 13 done]` precedents).

---

## 7. ADR projection

**Recommended posture at 14.2 state-2 PLAN-write: NO new ADRs.** The DECISIONS.md ledger head entering 14.2 is **ADR-0041** (from the parent-14 state-2 SPLIT commit); the next-available number at 14.2 state-2 PLAN-write is **ADR-0042** (or higher if 14.1 lands an ADR).

Conditional ADR slots, reserved for 14.2 state-2 / state-3 landing:

- **Conditional ADR (option A — additional §6.2 nuance surfaced at PLAN-write).** Only if the 14.2 PLAN-writer's verification-against-HEAD surfaces a new architectural constraint not covered by ADR-0041. **Recommended posture: NO ADR projected.**
- **Conditional ADR (option B — H2 wire-shape gap).** Only if a deeper H2 verification at PLAN-time surfaces a divergence from §6.2 item-8 (which confirmed parity on connect-failure). **Recommended posture: NO ADR projected.**
- **Conditional ADR (option C — sweeper architecture).** Only if the new-module / external-`OutlierManager`-registry choice (§5.1) warrants append-only recording. **Recommended posture: NO ADR** — the external-sibling-registry pattern is established (mirrors 12.2 `envoy-health::Scheduler` + 13.1 `H1PoolManager` + 13.2 `H2PoolManager` verbatim).
- **Conditional ADR (option D — foundations grant).** NOT PROJECTED — no external-crate dependency projected.

At most ONE ADR lands per commit. **Recommended: no ADR fires at 14.2.**

---

## 8. State-machine signposts for the 14.2 state-2 session

The next-next session (after 14.1 closes — 14.2 state 2) reads this section and acts.

- **Lifecycle state at session start:** State 2 (SPEC.md exists at THIS SPEC; PLAN.md does not).
- **Skill:** `superpowers:writing-plans` per `BOOTSTRAP_PROMPT.md` §5 state 2.
- **Output:** `PLAN.md` (~9-11 tasks; ~900-1200 LoC; under the §6.1 gates) + `PROGRESS.md` skeleton + Task 1 preamble (standalone pre-Task-1 commit per the 04.3 → 13.2 cadence). NO further split projected.
- **Empirical verification at 14.2 state 2:** does NOT re-run (parent §6.2 ratified by ADR-0041). The 14.2 PLAN-writer pulls forward the relevant lock-ins per §6.2 above.
- **PLAN-time SPEC corrections:** per the 06.2 → 13.2 cadence; corrections land in the PROGRESS Task 1 preamble.

---

## 9. Commit message format (for state 6 of the 14.2 lifecycle — the CLOSING-sub-phase close-out)

```
phase 14.2: response-receipt hook (H1+H2) + ejection sweeper + fixture 0022 + parent-14 close [parent 14 done] [ADR-NNNN, ...]

<1-3 sentence summary>

Differential surface: fixture 0022-upstream-outlier-detection-consecutive-5xx; all 22 Docker-gated fixtures (0001-0022) green simultaneously at CI run <ID> HEAD <SHA>; parent phase 14 flips done.
Conformance: h2spec ≥95% gate held at parent-05 baseline (H2 framing path untouched).
```

The `[parent 14 done]` tag is the closing-sub-phase row-flip-pair marker mirroring 12.2's `[parent 12 done]` at `3ec7fb9` + 13.2's `[parent 13 done]` at `96630f9`.

---

## 10. State-machine commit (THIS commit — parent-14 state-2 SPLIT closeout)

This SPEC is one of TWO sub-phase SPECs landing at the parent-14 state-2 SPLIT commit (alongside `14.1-endpoint-ejection-and-lb-integration/SPEC.md` + ADR-0040 (split) + ADR-0041 (§6.2 revision) + ROADMAP parent-row flip + 2 sub-phase rows + STATE.md advance to `14.1` state-2-next).

**Predecessor:** the parent-14 state-1 brainstorm commit (the immediate predecessor of THIS commit; SHA `542e8b5`).

**Origin/main:** `542e8b5` at THIS commit's prologue. After landing, the docs-only edits push to origin and the next CI run re-validates through the 5 stable-toolchain gates + the parse_bootstrap fuzz target on the unchanged 21-seed corpus.

---

*End of SPEC. Phase 14.2 lifecycle state 1 complete on landing of THIS parent-14 state-2 SPLIT commit. The next-next session (after 14.1 closes) enters 14.2 state 2 — writes PLAN.md per `superpowers:writing-plans`, performs the PLAN-time SPEC corrections per §6.3 against the PLAN-time HEAD, and lands `PLAN.md` + `PROGRESS.md` skeleton + Task 1 preamble in a single standalone pre-Task-1 commit.*
