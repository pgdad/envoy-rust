# Phase 12.2 (`12.2-active-http-probe-and-fixture`) — PROGRESS

> Per-task narrative log. Appended at every task commit per the 06.2 / 06.3 / 07.x / 08.x /
> 09 / 10 / 11 / 12.1 cadence. State-2 PLAN-write lands this skeleton + the Task 1 preamble;
> state-3 dispatch appends `### Task N — <name>` subsections in execution order.

---

## State-2 commit context

This commit (the state-2 standalone PLAN-write commit) lands:

- **CREATE** `docs/envoy-rust/phases/12.2-active-http-probe-and-fixture/PLAN.md` (the
  state-2 PLAN.md per `BOOTSTRAP_PROMPT.md` §5 state 2; 8 tasks; 25 architecture lock-ins;
  full `- [ ]` checkbox TDD steps with complete code per task).
- **CREATE** `docs/envoy-rust/phases/12.2-active-http-probe-and-fixture/PROGRESS.md` (this file).
- **MODIFY** `docs/envoy-rust/ROADMAP.md` — flip row `12.2` `status: planned` →
  `status: in-progress`. No other row touched (parent row `12` stays `in-progress`; `12.1`
  stays `done`).
- **MODIFY** `docs/envoy-rust/STATE.md` — Active phase status; Next expected skill; Last
  commit; Last updated; new `### Phase-12.2 state-2 PLAN-write` subsection in Notes (all
  prior subsections preserved verbatim per D-3.5 + D-3.4).

**Predecessor commit:** `67ae869` — `phase 12.1: cluster health_checks schema +
EndpointHealth state machine + pick() unhealthy-exclusion + panic threshold +
membership_healthy stats` (the phase-12.1 state-6 close-out; immediate prologue; HEAD ==
origin/main at this PLAN-write's prologue; CI run `26334623464` settled `success` per
`gh run list --branch main --limit 1` confirmation).

**SPEC commit base:** `4f9ba04` (the parent-12 state-2 split commit, where the 12.2 SPEC
was created). **This state-2 commit makes NO inline SPEC.md edits** — the §6.2 empirical
verification was completed at the parent-12 split; the 12.2 PLAN-writer baked the locked
facts into the PLAN lock-ins without re-running Docker per PLAN lock-in #3.

**ROADMAP status before this commit:** parent row `12` `in-progress` (`sub-phases: 12.1,
12.2`); row `12.1` `done` (flipped at the 12.1 state-6 close-out `67ae869`); row `12.2`
`planned` (since the parent-12 split).
**ROADMAP status after this commit:** parent row `12` `in-progress` (unchanged — flips
`done` only at the 12.2 state-6 close-out per the closing-sub-phase invariant); row `12.1`
`done` (unchanged); row `12.2` `in-progress`.

**STATE.md "Active phase" status before:** `phase 12.2 lifecycle state 1-complete / state-2-next
(SPEC.md exists; PLAN.md does NOT)`.
**STATE.md "Active phase" status after:** `phase 12.2 lifecycle state 2-complete /
state-3-next (PLAN.md + PROGRESS.md skeleton + Task 1 preamble landed at THIS commit;
ROADMAP row 12.2 flipped planned → in-progress; first implementation task pending)`.

**DECISIONS.md status before AND after:** **ADR-0037** (count 38). **No ADR lands at this
state-2 commit** (SPEC §7 + PLAN lock-in #2 — 12.2 introduces no new foundations grant, no
wire-level contract revision; the `envoy-health` crate is ordinary structure with a clean
DAG over existing crates — no creation ADR per the parent-12 state-1 brainstorm + state-2
split lock-in; the no-healthy-upstream body bytes were already pinned at the parent-12
split via ADR-0037, which 12.2 IMPLEMENTS at D6.2). Next available number stays **ADR-0038**.

**BEHAVIOR_CONTRACT.md status before AND after:** Unchanged at this commit. The 3 new
`Stat-name mapping` counter rows (under a new `**12.2 entries (active health checking —
counters):**` block), the M4 fold-in (~1 LoC edit to the 12.1 `membership_healthy`
Equivalence cell), and the new `## Response body — no-healthy-upstream synth-503`
subsection all land at Tasks 2 + 3 per the 06.x → 12.1 cadence (contract extensions land
at the task where the surface is first wired, NOT at PLAN-write time).

**ENVOY_TARGET.md + rust-toolchain.toml:** Unchanged (D-3.7 / D-3.9).

---

## PLAN scope summary

- **8 tasks** per PLAN §File-Structure + Tasks 1-8. Subagent-driven execution at state 3
  per PLAN lock-in #25 + `feedback_execution_style`.
- **~1000-1200 LoC projected** (production ~600 `envoy-health` + ~30 `hcm.rs:582` arm + new
  helper + ~60 counter wiring; tests ~250 unit + ~150 integration; fixture/harness/backend
  ~150) — comfortably under the `BOOTSTRAP_PROMPT.md` §6.1 ~1500-LoC / ~25-task gate. The
  parent-12 split (ADR-0036) already absorbed the over-gate scope into 12.1 + 12.2; 12.2
  does NOT nest-split (lock-in #1).
- **ZERO ADR landings** (lock-in #2; SPEC §7).
- **One new workspace member** `crates/envoy-health/` (clean DAG; no cycle; matches
  `envoy-stats`/`envoy-accesslog`/`envoy-filter` precedent — NO creation ADR).
- **One new helper crate** `tests/helpers/health-aware-http1-backend/` (the synthetic
  health-aware backend for D7.1 / the 06.3 REVIEW I2 down-payment).
- **One NEW differential fixture** `0019-upstream-active-health-check` (corpus 18 → 19
  Docker-gated fixtures; the FIRST Upstream-robustness-family differential fixture).
- **Zero H2 framing-path touch** (h2spec ≥95% holds vacuously at the parent-05 baseline
  99.31%; the fixture + backstop are H1; the synth-503 helper is in `envoy-http1`; the
  new `envoy-health` crate uses `envoy-http1::Client` only).

---

## Task 1 preamble

### §6.2 empirical-verification findings — locked at the parent-12 split (`4f9ba04`); NOT re-run at this PLAN-write

Per the 12.2 SPEC §2 + STATE.md `### Phase-12 state-2 split decision` + ADR-0037, the
HEAVY 6-item §6.2 verification against `envoyproxy/envoy:v1.33.0` was performed at the
parent-12 state-2 split commit `4f9ba04` (a Docker bridge network with a synthetic
health-aware backend + an active-HC Envoy; admin `/stats` + data-plane probes). The
findings binding 12.2 are LOCKED FACTS (PLAN lock-in #3); the 12.2 PLAN-writer baked them
into the PLAN without re-running Docker:

1. **Initial endpoint health state = Unhealthy/pending-until-first-pass** (MATCHED projection;
   12.1 D3 ALREADY LANDED at `32cb44a` — the `EndpointHealth` constructor starts every
   endpoint Unhealthy). 12.2's probe task is the live writer that drives the Unhealthy →
   Healthy + Healthy → Unhealthy transitions per the consecutive-success / failure thresholds.
2. **No-healthy-upstream synth body = `no healthy upstream`** = **19 bytes**, hex
   `6e 6f 20 68 65 61 6c 74 68 79 20 75 70 73 74 72 65 61 6d`, NO trailing newline (DIVERGES
   → ADR-0037) — **12.2 D6.2 lands HERE** at Task 3 via a new `synth_no_healthy_upstream`
   helper adjacent to `synth_status` in `crates/envoy-http1/src/hcm.rs`; the ONLY call-site
   change is at `hcm.rs:582` (lock-in #15 — global `synth_status` at `hcm.rs:918` is NOT
   touched; the connect-fail 502 + send-fail 502 paths keep their existing empty bodies).
3. **Panic threshold: default 50%, strictly-below (`<`), `Percent { value: f64 }`,
   panic-mode round-robins over ALL** (MATCHED; 12.1 D5 ALREADY LANDED at `d713386` —
   `Cluster.pick()` consults the panic threshold per the §5.4 implementation). 12.2's
   fixture sets `healthy_panic_threshold: { value: 0 }` (panic disabled) so 0-healthy
   yields `pick() -> None` rather than panic-routing.
4. **Health-check stat names `cluster.<name>.health_check.{attempt,success,failure}` +
   `membership_healthy`** (MATCHED; the `membership_healthy` gauge ALREADY LANDED at 12.1
   D6 commit `8ea3877`). 12.2 D7 wires the 3 counters at Task 2 inside the new
   `envoy-health::Scheduler::spawn` (registered once per configured-HC cluster; each
   per-endpoint probe task holds `Arc<Counter>` clones).
5. **HTTP probe shape: `GET <path>` + `Host: <cluster-name>` default + default
   `expected_statuses` = exactly 200 + half-open `Int64Range`** (MATCHED; 12.1 D1 ALREADY
   LANDED the `Int64Range` reuse + `HttpHealthCheck.host: Option<String>` schema at
   `9baa877`). 12.2 D4 drives the probe via `envoy_http1::client::Client::connect(addr,
   host)` + `ClientStream::send_request(Request{ method: "GET", path, host, headers,
   body: None })` at Task 1 — the probe's `user-agent` is unset (envoy-rust does not
   differentially assert the probe wire bytes per SPEC §2 item-5; the synthetic backend
   keys on method + path only).
6. **Duration config shape: integer seconds only is the shared form** (DIVERGES from the
   parent fixture sketch's `0.5s`) — fixture 0019 uses `interval: 1s` + `timeout: 1s`
   (PLAN lock-in #13). 12.2's `envoy-health::Scheduler::spawn` re-parses both via the
   existing `envoy_config::parse_duration` (the 12.1 D2 validator already accepted them at
   parse time; the re-parse is defense-in-depth — identical-result on the success path).

### PLAN-write SPEC corrections (read against HEAD `67ae869`)

The 14 corrections in PLAN §1 (verified against HEAD by direct read of the named seams):

1. **`Cluster.endpoint_health` + `Cluster.panic_threshold` are `pub(crate)`** (not
   accessible from `envoy-health`) — Task 1 lands a new public accessor
   `ClusterHandle::health_probe_targets() -> Option<Vec<(SocketAddr, Arc<EndpointHealth>)>>`
   (PLAN lock-in #5); internal fields stay `pub(crate)`.
2. **The runtime `Cluster` does NOT carry the parsed `HealthCheck` config** — only the
   thresholds-via-`EndpointHealth` + `panic_threshold` are runtime fields. `envoy-health`
   reads the HC config (`path`, `interval`, `timeout`, `host`, `expected_statuses`)
   directly from `&bootstrap.static_resources.clusters[i].health_checks[0]`. `Scheduler::
   spawn(&bootstrap, Arc<ClusterManager>, Arc<StatsRegistry>, CancellationToken)` takes
   both `&bootstrap` (for HC config) and `Arc<ClusterManager>` (for resolved (SocketAddr,
   EndpointHealth) pairs) — keeps `envoy-cluster` as the structural seam.
3. **`envoy_http1::client::Client::connect(addr: SocketAddr, host: &str) -> Result<ClientStream,
   Http1Error>`** + **`ClientStream::send_request(&mut self, request: Request) -> Result<Response,
   Http1Error>`** (verified `crates/envoy-http1/src/client.rs:33-47, :73`). `Request` is
   the existing `crates/envoy-http1/src/codec.rs::Request` struct (the same one HCM
   dispatches; `method`, `path`, `host`, `headers`, `body`). Both `Client` + `ClientStream`
   are already PUBLIC — no envoy-http1 surface widening needed.
4. **`Http1Error::UpstreamConnect { addr, source }`** is the connect-failure variant; the
   probe wraps `Client::connect` in `tokio::time::timeout(timeout, ...)`.
5. **`Response.status: u16`, `Response.headers: Vec<(String, String)>`, `Response.body:
   Bytes`** (verified `crates/envoy-http1/src/response.rs`).
6. **`hcm.rs:582` arm currently reads `outgoing = synth_status(503, close);`** in the
   `else` branch of `if cluster.pick_endpoint().is_some()` (the comment is *"No healthy
   endpoint available for this cluster."*). Task 3 changes this single line; lock-in #15.
7. **`synth_status` at `hcm.rs:918`** returns a `Response { status, reason: None,
   headers: vec![5-standard-headers], body: Bytes::new() }`. The new
   `synth_no_healthy_upstream` helper (Task 3) mirrors this shape modulo body bytes +
   content-length.
8. **`envoy-bin/src/main.rs` `from_bootstrap` call-site is at `~:124`** (verified —
   `envoy_cluster::from_bootstrap(&bootstrap, std::sync::Arc::clone(&registry)).await`).
   Task 1 wires `Scheduler::spawn(&bootstrap, cluster_mgr, registry, token.clone())`
   after this site + `scheduler.shutdown().await` at the runtime shutdown tail.
9. **`StatsRegistry::register_counter(&str) -> Result<Arc<Counter>, StatsError>`**
   (verified `crates/envoy-stats/src/registry.rs:45`); `Counter::inc()` increments by 1;
   idempotent re-register.
10. **`Int64Range { start: i64, end: i64 }` half-open `[start, end)`** (verified
    `crates/envoy-config/src/bootstrap.rs:1080`). The probe's success check uses
    `(r.start..r.end).contains(&(status as i64))`.
11. **`parse_duration` is `pub fn parse_duration(s: &str) -> Result<Duration, String>`**
    (verified `crates/envoy-config/src/bootstrap.rs:2289`; integer-only; rejects `0.5s`).
    The 12.1 D2 validator already rejects these at parse, so the `Scheduler::spawn`
    re-parse is defense-in-depth (identical-result on success).
12. **The existing `Http1EchoBackend` at `tests/differential/src/backend.rs:179`** uses
    a `cargo run`-equivalent subprocess pattern. `HealthAwareHttp1Backend` (Task 4)
    mirrors this shape with a new helper binary at
    `tests/helpers/health-aware-http1-backend/`.
13. **`tests/differential/src/lib.rs::Driver` enum dispatch arm** (verified `~:1655-1667`):
    `Http1AfterSettle` falls under `"PORT"` (same as `Http1`); Task 5 adds the variant
    + the dispatch arm.
14. **`envoy-cluster` deps (verified `crates/envoy-cluster/Cargo.toml`):** `envoy-config`
    + `envoy-stats` + `thiserror` + `tokio` (net, rt, macros). NO `envoy-http1` dep.
    **`envoy-http1` deps (verified `crates/envoy-http1/Cargo.toml`):** `envoy-cluster` +
    others. **The new `envoy-health` crate sits ABOVE both** (depends on `envoy-cluster`
    + `envoy-http1` + `envoy-config` + `envoy-stats` + `tokio` + `tokio-util` +
    `tracing` + `thiserror` + `bytes`) — clean DAG, no cycle. Verified at PLAN-write
    that `cargo` would reject any cycle attempt at build time.

### M-track carryforwards engaged by the 12.2 lifecycle

Per the 12.1 REVIEW.md §4 + the 12.2 SPEC §6:

- **M2 (the `EndpointHealth` `Relaxed`-ordering single-writer-per-endpoint premise — a
  forward-correctness dependency the 12.2 review must verify before the live probe task
  relies on it).** **PLAN-time disposition:** Task 1 folds in a ~1-LoC API-boundary
  single-writer-contract comment on `EndpointHealth` (PLAN lock-in #6 + the
  `health.rs`-edit step) that DOCUMENTS the live-writer contract at the
  `EndpointHealth` API boundary, closing the M2 forward-verification at the API site.
  The 12.2 review verifies the `Scheduler::spawn` topology instantiates EXACTLY ONE
  probe task per (cluster, endpoint) pair (the single-writer-per-endpoint invariant the
  `Relaxed`-ordering soundness rests on); Task 1's tests (`spawns_one_task_per_hc_endpoint`)
  attest the topology.
- **M4 (the `cluster.<name>.membership_healthy` BEHAVIOR_CONTRACT Equivalence-column
  self-containment — `value-exact (steady state)` reads 0 at 12.1 because no probe task
  drives it; at 12.2 the gauge becomes driven).** **PLAN-time disposition:** Task 2
  folds in the ~1-LoC contract-row Equivalence-cell edit (`value-exact (steady state)` →
  `value-exact (12.2 steady state; reads 0 at 12.1)`) at the natural revisit site
  (where the gauge becomes driven by the live probe task — PLAN lock-in #7).

### Carryforward closures engaged (down-payment, NOT full closure)

- **06.3 REVIEW I2** (the synthetic-backend harness infrastructure; the
  Upstream-robustness-family named owner) — **ENGAGED at Task 4 / D7.1** as the
  synthetic-backend harness primitive (`HealthAwareHttp1Backend` + the new helper crate
  `tests/helpers/health-aware-http1-backend/`). **Phase 12.2 does NOT fully close I2** —
  the residual (per-class `downstream_rq_3xx/4xx/5xx` + `cluster.<name>.upstream_rq_5xx`
  wire-level coverage + the `cluster.<name>.upstream_cx_total` `value-exact` tightening)
  remains tied to **connection pooling** per the 06.3 REVIEW §3 disposition. The state-3
  Task 4 PROGRESS attribution + the state-5 review BOTH note this honestly (D-3.4); do
  NOT over-claim full closure.

### Other inherited carryforward (no engagement)

12.2 touches no HTTP-filter file — the **phase-11 8 Minor M1-M8** (M1/M2/M3 doc-drift in
`instance.rs`/`pipeline.rs`; M4/M5 validator coverage; M6 test-helper-style; M7/M8
NON-ISSUES), the **12.1 M1 + M3** (validator style + `pick()` two-pass — both no-action
nits), and the standing inventory (10 M2/M3/M4/D1/D2/T1; 09 M1/D1/D2/T1/T2/T3; 08.2
REVIEW M1-M8 + T1-T3; 08.1 REVIEW M3; 07.2 REVIEW M2/M3; 06.2 REVIEW M1/M2/M4/M5; 06.1
REVIEW M2/M3/M5/M6; 05.3 REVIEW I2; 05.2 REVIEW I1/I2/I3; 04.1 REVIEW
M5/M9/M-claim/M1/M2/M4/M7; 02.2 REVIEW M1; Phase-00 I3 SIGKILL→SIGTERM) ALL carry forward
unchanged per their existing named-owner dispositions.

---

<!-- state-3 task subsections append below this line -->

## Phase-12.2 state-3 execution arc (Tasks 1-8)

All eight tasks dispatched to fresh subagents with two-stage review per
`feedback_execution_style` + the 06.x → 12.1 cadence; TDD per task; one commit per
task. `cargo fmt --all -- --check` clean at every per-task close (06.1 R-9). No
ADR landed (lock-in #2; ledger head stays ADR-0037, next available ADR-0038).

### Task 1 — D4 envoy-health crate + periodic probe task + ClusterHandle accessor + envoy-bin wiring + M2 contract

Created the new `envoy-health` workspace crate (`crates/envoy-health/{Cargo.toml,
src/{lib.rs,error.rs,probe.rs,scheduler.rs}}`) — the project's FIRST
periodic-background primitive. Clean DAG: `envoy-health → {envoy-cluster,
envoy-http1, envoy-config, envoy-stats, tokio, tokio-util, bytes, tracing,
thiserror}` (no cycle — `envoy-cluster` does NOT depend on `envoy-http1`, and
`envoy-health` sits ABOVE both — verified by `cargo build --workspace` clean).
**Zero new top-level Cargo dep** (lock-in #4 — every dep is either an
already-permitted foundation or an existing workspace path-dep).
`#![forbid(unsafe_code)]` on the new crate root (D-3.8). Added workspace member
in alphabetical position adjacent to `envoy-filter`.

`Scheduler::spawn(&Bootstrap, Arc<ClusterManager>, Arc<StatsRegistry>,
CancellationToken) -> Result<Scheduler, HealthError>` walks `bootstrap.static_resources.clusters`,
takes the first (validator-guaranteed sole) `health_checks` entry, registers the
3 `cluster.<n>.health_check.{attempt,success,failure}` counters per
configured-HC cluster, re-parses `interval`/`timeout` via `envoy_config::parse_duration`
(defense-in-depth — the 12.1 D2 validator already accepted them; identical-result
on success), and spawns EXACTLY ONE `tokio::spawn`-ed `probe_loop` per (cluster,
endpoint) pair via the new `ClusterHandle::health_probe_targets()` accessor (the
single-writer-per-endpoint topology the 12.1 REVIEW M2 contract requires; closed
at this crate's API boundary per the new ~11-LoC `EndpointHealth` API-boundary
contract comment in `crates/envoy-cluster/src/health.rs`). `Scheduler::shutdown()`
cancels the shared `CancellationToken` then awaits every `JoinHandle` — clean
drain via the `tokio::select!` cancel branch.

`probe_loop` (the periodic primitive): `tokio::time::interval(interval_dur)` with
`MissedTickBehavior::Delay` + `tokio::select!` on `cancel.cancelled()` + `ticker.tick()`;
per tick, `attempt.inc()` then `probe_once` then `success.inc()` +
`endpoint_health.record_success()` OR `failure.inc()` +
`endpoint_health.record_failure()`. `probe_once`: `Client::connect(addr, host)` +
`ClientStream::send_request(Request { method: "GET", path, version: Http11, headers: vec![], body: None })`
wrapped in `tokio::time::timeout(probe_timeout, ...)`. Success criterion: empty
`expected_statuses` → exactly 200; non-empty → `(r.start..r.end).contains(&(status as i64))`
across the union (lock-in #10).

The probe omits an explicit `Host:` header — `Client::connect` captures `host` at
connect time and `send_request` injects it when the outgoing `Request.headers`
does not already carry one (the existing client de-dup contract). The `host`
defaults to `http_health_check.host.unwrap_or(cluster_name)` per lock-in #9.

`ClusterHandle::health_probe_targets() -> Option<Vec<(SocketAddr, Arc<EndpointHealth>)>>`
zips `self.inner.endpoints` with `self.inner.endpoint_health.as_ref()?` (clean
`None` on no-HC; the §5.4 inert-when-unconfigured invariant). Internal fields
stay `pub(crate)`.

`envoy-bin` wiring: added `envoy-health = { path = "../envoy-health" }` path-dep;
constructed the scheduler after the `cluster_mgr` build site (`crates/envoy-bin/src/main.rs:128`)
with the existing `signal_token` `CancellationToken`; `health_scheduler.shutdown().await`
on the runtime drain path after `set.join_next().await` completes (line ~415).

**M2 single-writer-contract comment** landed on `EndpointHealth` per PLAN lock-in
#6 + Task 1 Step 8: ~11 LoC API-boundary contract explicitly stating that callers
obtaining an `Arc<EndpointHealth>` from `health_probe_targets()` MUST NOT call
`record_*` themselves and MUST NOT hand the `Arc` to additional writer tasks;
naming the violation consequence (concurrent load-modify-store races on `state`
may double-increment/decrement the membership gauge); pointing at the scheduler
boundary as the verification site. Closes the 12.1 REVIEW M2 forward-correctness
verification at the API boundary.

**6 unit tests** in `envoy-health`: `probe::tests::{empty_expected_statuses_accepts_only_200,
half_open_range_excludes_end, multi_range_union}` + `scheduler::tests::{spawns_one_task_per_hc_endpoint,
spawns_zero_tasks_when_no_hc_configured, shutdown_terminates_all_tasks}`. The
`spawns_one_task_per_hc_endpoint` test ATTESTS the single-writer topology
(`assert_eq!(scheduler.task_count(), 2)` on a 2-endpoint HC cluster). `cargo test -p envoy-health` →
`test result: ok. 6 passed; 0 failed; 0 ignored`.

§7.5 gates (a)/(b)/(c)/(d) hold vacuously at this task (no new fixture; pre-existing 18
unaffected; no H2 touch; no new fuzz seed). (e) the 5 stable-toolchain gates clean
locally at this commit: `cargo build --workspace --all-targets` `Finished`;
`cargo clippy --workspace --all-targets --all-features -- -D warnings` `Finished`;
`cargo fmt --all -- --check` clean; `cargo test --workspace` → **838 passed / 0 failed / 2 ignored**
(+6 over the 12.1 baseline 832 — exactly the 6 new `envoy-health` tests);
`cargo deny check` `advisories ok, bans ok, licenses ok, sources ok` (benign
unmatched-license-allowance notices unchanged from prior phases).

Spec ✅ (the implementation matches PLAN lock-ins #4 + #5 + #6 + #9 + #10 + #11
+ #12 + #13 verbatim modulo the `Request` struct shape — the PLAN sketched
`host: String` field but the actual on-disk `envoy_http1::codec::Request` carries
`version: HttpVersion`, `bytes_consumed: usize` instead, with the `Host:` header
injected by `Client::connect` at connect time; the implementation correctly uses
the on-disk shape, with an inline rustdoc note on `probe_once` explaining the
contract). Code-quality **Approved with one in-phase amend** — the reviewer
surfaced one Important finding (early `?` propagation in `envoy-bin`'s drain
loop short-circuited `health_scheduler.shutdown().await` on the error-exit
path) recovered IN-PHASE via amend per the 12.1 in-phase-recovery cadence:
hoisted the first error into a local `first_err: Option<anyhow::Error>` so the
scheduler drain runs on BOTH clean-exit and error-exit paths before the error
propagates. Re-ran the 5 stable-toolchain gates clean post-amend (build,
clippy, fmt, test, deny). Two awareness-only Minors (asymmetric defense-in-depth
posture in `scheduler.rs:86-92` between `.expect()` and `continue`; a one-liner
note that `ProbeError`'s `Debug` is the live consumer via `tracing::debug!`)
deferred to the 12.2 state-5 review per the 12.1 carryforward discipline.

### Task 2 — D7 health_check.{attempt,success,failure} BEHAVIOR_CONTRACT 12.2 block + M4 12.1 fold-in + registration-assertion tests

Appended the **`**12.2 entries (active health checking — counters):**`** block to
`docs/envoy-rust/BEHAVIOR_CONTRACT.md`'s `## Stat-name mapping` section AFTER
the existing `**12.1 entries (active health checking):**` block (preserved
verbatim modulo the single Equivalence-cell edit below); 3 new rows for
`cluster.<name>.health_check.{attempt,success,failure}` — each carries
`Equivalence: name-required, value-may-differ` per timing-dependence (independent
`tokio::time::interval` schedules from independent process-start instants yield
divergent elapsed-probe counts over a fixed test window; value-exact is not
feasible without timing-tolerance opt-in, which phase 12 does NOT take). The
3 rows trace the registration site (`Scheduler::spawn` only when the cluster
configures `health_checks`), the `.success` boundary (status ∈
`expected_statuses`, default exactly 200, half-open `Int64Range`), and the
`.failure` fold-in (status outside `expected_statuses` OR connect failure OR
per-probe `tokio::time::timeout` elapsed OR malformed response — the
network-failure-class results fold into `failure` at phase-12 scope; the dedicated
`network_failure` sub-counter defers per parent SPEC §4).

**M4 fold-in** (PLAN lock-in #7; the natural revisit site where the gauge becomes
driven): edited ONLY the `cluster.<name>.membership_healthy` row's Equivalence
cell — `value-exact (steady state)` → `value-exact (12.2 steady state; reads 0
at 12.1)`. Rationale column unchanged. Closes the 12.1 REVIEW M4
self-containment carryforward in-phase per the 12.1 carryforward discipline.

**2 new registration-attestation tests** appended to `crates/envoy-health/src/scheduler.rs`
`#[cfg(test)] mod tests` (reusing the `HC_BOOTSTRAP` + `NO_HC_BOOTSTRAP` YAML
constants Task 1 landed): `registers_three_counters_per_hc_cluster` asserts all
3 `cluster.hc_backend.health_check.{attempt,success,failure}` counter names
land in `registry.snapshot()`; `registers_no_counters_when_no_hc_configured`
asserts the 3 names are ABSENT for the no-HC cluster (`plain`). Both used the
`registry.snapshot().iter().any(|(n, _)| n == &name)` presence-check pattern
(matches the 12.1 D6 `membership_healthy` test's hardened form — never
false-passes against idempotent re-register). Both PASS immediately at this
task because Task 1's `Scheduler::spawn` already registers the 3 counters at
the per-configured-HC-cluster entry of the bootstrap walk (the substance of
this task is the contract extension; the tests are attestations of the
already-landed registration logic, per PLAN Task 2 Step 2).

`cargo test -p envoy-health` → `test result: ok. 8 passed; 0 failed; 0 ignored`
(+2 over Task 1 baseline 6: the 2 new `scheduler::tests::registers_*` tests).
`cargo test --workspace` → **840 passed / 0 failed / 2 ignored** (+2 over
Task 1 baseline 838 — exactly the 2 new tests; no other crate touched). 5
stable-toolchain gates clean locally: `cargo build --workspace --all-targets`
`Finished`; `cargo clippy --workspace --all-targets --all-features -- -D warnings`
`Finished`; `cargo fmt --all -- --check` clean; full-workspace tests as above.
No production code change beyond the 2 test functions (Task 1's registration
logic intact). No ADR. No Cargo.toml. No STATE.md. No ROADMAP edit. §7.5
gates (a)/(b)/(c)/(d) hold vacuously at this task (no new fixture; pre-existing
18 unaffected; no H2 touch; no new fuzz seed).


