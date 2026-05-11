# Phase 06.3 (`06.3-stats-wiring-and-close`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended per the user's standing preference auto-memory `feedback_execution_style`) — fresh subagent per task + two-stage review. Steps use checkbox (`- [ ]`) syntax for tracking. Tasks land in numbered order; Task 1 is the PROGRESS.md preamble that records the LoC-drift posture + SPEC corrections + signpost choices and lands AT the state-2 standalone PLAN commit (i.e., THIS commit); Tasks 2-12 land at state-3 each as their own commit. The state-4 phase-done verification is Task 12.

**Goal.** Land the comprehensive Envoy stat tree at HCM/router/listener/cluster sites (per-response-class HCM counters, connection-lifetime gauges, upstream-side router counters, access-log line counter, listener accept-failure counter), close phase-05.3 REVIEW I1 (the `Http2ClusterFromHttp1Listener` parse-time validator gate) as Task 2, fold in three carryforwards from 06.2 REVIEW (I1 H1-vs-H2 state-init asymmetry; I2 User-Agent fixture-vs-contract divergence; M3 fixture 0012 README path inconsistency) plus one from 06.1 REVIEW (I1 admin handler idle read timeout), extend BEHAVIOR_CONTRACT.md `Stat-name mapping` by 7+ new rows, extend fixture 0011's `expectations.yaml` to assert the comprehensive set across all 4 status classes, and at the state-6 close-out commit ALSO flip parent ROADMAP row `06` from `in-progress` to `done` per the ROADMAP-schema invariant — mirroring phase-04's `e626862` and phase-05's `82c26b8` close-outs.

**Architecture.** 06.3 extends 06.1's representative stats subset (3 counters) using the existing `envoy-stats` registry + `Counter`/`Gauge` primitives; no new foundation crate, no new top-level Cargo deps, no new fuzz target, no new fixture. Consumers register and increment at their own seams per parent-06 SPEC §6 Rule 2 (`envoy-stats` exports primitives only). The H1 + H2 HCM dispatch surfaces share one `HCMStats` struct via the `envoy-http2::HCMConfig` type alias to `envoy-http1::HCMConfig` (verified at `crates/envoy-http2/src/hcm.rs:27` — `pub type HCMConfig = Http1HCMConfig`). The 05.3 REVIEW I1 closure lands as a parse-time `ConfigError::Http2ClusterFromHttp1Listener` validator gate at `crates/envoy-config/src/bootstrap.rs::validate` (mirrors 05.2's `Http2OverTlsNotSupported` shape). Fixture 0011's request-set extends to drive all 4 response classes (2xx via existing direct_response; 3xx/4xx via new direct_response routes; 5xx via a new router-proxy route to a synthetic 5xx-emitting backend so `cluster.<name>.upstream_rq_5xx` increments per signpost 5 recommendation (c)).

**Tech Stack.** Existing only: `envoy-stats` (Counter/Gauge/StatsRegistry; hand-rolled atop `std::sync::atomic` + `RwLock<BTreeMap>`); `envoy-config` (validator pattern); `envoy-http1` + `envoy-http2` (HCM dispatch sites; type-aliased HCMConfig); `envoy-listener` (accept loop); `envoy-cluster` (cluster handles); `envoy-accesslog` (FileSink emission — 06.3 only touches its dispatch-site counter); `envoy-admin` (admin handler; idle read timeout site); `tests/differential` (`Driver::AdminScrape` + `BodyRule::PrometheusExposition`). **No new top-level Cargo deps**; `cargo deny check` is a no-op.

---

## SPEC corrections recorded at PLAN-write time

These five corrections were caught reading SPEC.md against the current code (`git ls-files` at HEAD `389ef96`). The SPEC remains in-tree unedited per D-3.4 / D-3.5; PROGRESS.md at Task 1 narrates them for stranger-readability.

1. **SPEC §3 D15.3.a wrongly co-locates the per-class HCM counter increment with 06.1's `downstream_rq_total` increment.** SPEC reads: *"The increment-site is the existing `write_response` call site at `crates/envoy-http1/src/hcm.rs` (the on-response-complete hook landed at 04.1; 06.1 D4.1's `downstream_rq_total` increment lands here)."* Empirically at `crates/envoy-http1/src/hcm.rs:251`, 06.1's `config.stats.downstream_rq_total.inc()` fires at **request-entry** time (BEFORE `build_response` dispatches to a writer arm), not at on-response-complete. The per-class counters cannot fire at the same site because `response.status / 100` is unknown until after the writer arm runs. **Resolution:** per-class counter increments land at the factored access-log dispatch site (post-`match outcome` block, lines 459+), where `response_status_for_log` is populated. 06.1's request-entry `downstream_rq_total.inc()` continues unchanged at line 251.

2. **SPEC §3 D15.3.b's listener gauge claim "increment on every accepted TCP connection" needs to factor that 06.1 D4.a hoists `cx_total` out of `self` for the `tokio::select!` accept-arm capture.** Empirically at `crates/envoy-listener/src/lib.rs:139-160`, the accept-loop pattern is:
   ```rust
   let cx_total = self.cx_total;
   loop { tokio::select! { accepted = listener.accept() => { Ok((stream, peer)) => { cx_total.inc(); ... } ... } } }
   ```
   The new `cx_active` gauge must follow the same hoist (`let cx_active = self.cx_active;` after `let cx_total = self.cx_total;`) so the gauge handle is captured by the accept-arm. The decrement runs inside the spawned per-connection task's epilogue — which means the closure must capture an `Arc<Gauge>` clone (the gauge already is `Arc<Gauge>`, so `cx_active.clone()` is mechanical). Per signpost 7 the gauge scopes to data-path listeners only (NOT the admin listener); the planner threads a `count_active: bool` config flag into `ListenerConfig` to gate the registration, defaulting to `true` and overridden to `false` at admin-listener construction. Mechanical at envoy-bin's admin-listener wiring site.

3. **SPEC §3 D15.3.c proposes adding a new `cluster: &ClusterHandle` parameter to `write_proxied_response`.** Empirically at `crates/envoy-http1/src/router.rs:75-83` the current signature is `pub async fn write_proxied_response<W>(downstream: &mut W, upstream_response: Response, elapsed_ms: u128, close: bool) -> Result<(), Http1Error> where W: tokio::io::AsyncWrite + Unpin`. Adding `cluster: &ClusterHandle` is straightforward at the H1 call site (`crates/envoy-http1/src/hcm.rs:418-424`), but the H2 router-arm does NOT call `write_proxied_response` — `crates/envoy-http2/src/hcm.rs:278-317` builds the downstream `Response` inline (no call to the H1 helper). **Resolution:** the H2 site lands a sibling pair of `cluster.upstream_rq_total.inc()` + `cluster.upstream_rq_5xx.inc()` increments inline at the post-dispatch `proxy_resp` construction site (lines 280-318), parallel to but distinct from the H1 helper's increments. Both sites land the same 2-line increment pair; the H2 site does not gain a function-extraction refactor. PROGRESS Task 7 narrates this.

4. **SPEC §3 D14.3's pseudocode "Walk every route in every virtual_host in route_config" needs to handle the `route_config.virtual_hosts[].domains` filter correctly.** The validator's existing route-walk at `crates/envoy-config/src/bootstrap.rs:1340-1402` already iterates all `vh.routes` regardless of domain matching — the new D14.3 cluster-reachability scan reuses that same walk shape without filtering by domain. PROGRESS Task 2 notes the walk shape is the existing `for vh in &mut hcm.route_config.virtual_hosts { for r in &mut vh.routes { ... } }` (lines 1346-1401), with the new H1×H2 check sitting inside the `RouteAction::Route(ar)` arm at line 1387-1394 alongside the existing `UnknownCluster` check.

5. **SPEC §3 D15.3.b posits "cluster-side gauge increment at the `Client::connect` call sites in `envoy-http1/src/client.rs` and `envoy-http2/src/client.rs`".** Empirically the cluster-side `upstream_cx_total` counter increment (06.1 D4.b) lives at the call sites (`crates/envoy-http1/src/hcm.rs:397` + `crates/envoy-http2/src/hcm.rs:228,239`), NOT inside the client crates. The same posture applies to `upstream_cx_active`: increment at the call site (HCM's proxy arm) just before `Client::connect`, decrement at the per-call drop site. Per parent-06 SPEC §6 Rule 2, putting the increment inside `envoy-http1::Client` or `envoy-http2::Client` would couple the codec crate to the stats namespace (the cluster handle would need to be threaded into `Client::connect`'s signature). **Resolution:** increment + decrement at the HCM proxy-arm call sites (H1 `hcm.rs:389-396` immediately before `Client::connect`; symmetric at H2 `hcm.rs:222-230` and `hcm.rs:235-244`). The decrement is RAII-style via a small `ConnGaugeGuard` struct that holds an `Arc<Gauge>` and decrements on `Drop` — covers both success and error close paths uniformly. The TCP-proxy `envoy-tcp::TcpProxy::dial` site IS wired symmetrically per signpost 5 (yes for symmetry), at `crates/envoy-tcp/src/proxy.rs`'s connect call site.

---

## Architecture decisions locked at PLAN-write time (signpost choices)

Per parent-06 SPEC §6 + 06.3 SPEC §7's 10 signposts, the planner picks the recommendation at PLAN-write time so the executor does not re-litigate mid-task. All 10 signposts (+ the LoC-budget reality check) lock here:

| # | Signpost | Decision | Rationale |
|---|---|---|---|
| 1 | `Http2ClusterFromHttp1Listener` validator scan strategy | **Eager single-pass.** Build `HashMap<&str, &Cluster>` once upfront from `bootstrap.static_resources.clusters`; walk every listener's routes; O(listeners × routes) lookup. | SPEC §7 signpost 1 recommendation. Matches existing cluster-name-resolution pattern at 04.1 RouteConfiguration validator. |
| 2 | Gauge atomic ordering | **`Ordering::Relaxed` for all gauge ops.** Matches 06.1 D1.1 counter+gauge primitive ordering. | SPEC §7 signpost 2 recommendation. Scrape-time read tolerates ~1s staleness per Prometheus exposition convention. |
| 3 | Per-response-class counter naming | **`downstream_rq_2xx`, `downstream_rq_3xx`, `downstream_rq_4xx`, `downstream_rq_5xx`** (lowercase `xx`). | SPEC §7 signpost 3 recommendation. Verbatim Envoy v1.33.0 docs. |
| 4 | `ClusterStats` struct factoring | **(a) Append fields to `Cluster` directly** — `cx_active: Arc<Gauge>`, `upstream_rq_total: Arc<Counter>`, `upstream_rq_5xx: Arc<Counter>`. No sub-struct. | SPEC §7 signpost 4 recommendation. Symmetric with 06.1 D4.b's `cx_total` on `Cluster`. |
| 5 | Cluster-side `upstream_rq_5xx` increment site + fixture 0011 5xx-path | **(a)** Increment at `write_proxied_response` (H1) + inline at H2's proxy-resp build (H2). **(c) Hybrid fixture:** 4xx via `direct_response: 404`; 5xx via router-proxy to a synthetic-5xx backend. | SPEC §7 signpost 5 recommendation. H2 path uses inline increment per PLAN-write SPEC correction 3. |
| 6 | Listener accept-failure counter scope | **All accept errors.** Counter fires for ECONNRESET, EMFILE, ENFILE, and any other `io::Error` returned by `TcpListener::accept().await`. | SPEC §7 signpost 6 recommendation. Matches Envoy's `downstream_cx_accept_failed` semantic. |
| 7 | Gauge value-may-be-zero + admin listener carve-out | **(a)** Existing body-rule shape accepts `<name> 0` as a Prometheus exposition number; no harness change for emission shape. **(b) Scope to data-path listeners.** Admin listener's `cx_active` is NOT registered. | SPEC §7 signpost 7 recommendation. Plumbed via a new `count_active: bool` field on `ListenerConfig` (default `true`; envoy-bin's admin-listener wiring sets it to `false`). |
| 8 | Fixture 0011 expectation extension shape | **(a) Additive.** The 06.1-landed allow-list + 3 representative-counter assertions stay; the comprehensive set adds atop. | SPEC §7 signpost 8 recommendation. |
| 9 | Harness `BodyRule::PrometheusExposition` gauge handling | **Option (1):** Extend the existing variant additively with `value_exact: HashMap<String, u64>`, `value_must_be_zero: HashSet<String>`, `value_present_only: HashSet<String>` fields. Backwards-compat: all three fields default to empty so 06.1's name-set-only assertion continues working. | SPEC §7 signpost 9 recommendation. |
| 10 | Parent-06 state-6 commit message format | **Title:** `phase 06.3: comprehensive stats wiring + 05.3 I1 closure + parent-06 close [parent 06 done] [ADR-0029]` (literal). | SPEC §7 signpost 10 recommendation. Mirrors 05.3's `82c26b8` shape. |

**Additional decisions locked at PLAN-write time (not numbered signposts but worth recording):**

11. **D15.3.e `access_logs_failed` sibling counter — SHIPS in 06.3** per parent SPEC §3 D15.3's note *"a `..._access_logs_failed` counter lands in 06.3 if scope permits"* and 06.3 SPEC §3 D15.3.e signpost 5 recommendation. Mechanical: one extra `Counter` field on `HCMStats` + one increment in the `if let Err(e) =` arm of the existing access-log dispatch site. The `Stat-name mapping` row lands as `value-exact for the 0-failures case` (parallel to `listener.<name>.downstream_cx_accept_failed`). Fixture 0011 does NOT exercise the failure path (would require a deliberately-failing sink), so the row's disposition holds at 0-failures.

12. **TCP-proxy `cx_active` wiring at `envoy-tcp::TcpProxy::dial`** — YES per signpost 5's "yes for symmetry" recommendation. The TCP-proxy uses the same `Cluster` struct via `ClusterHandle::get`, and the gauge's namespace is per-cluster-name (not per-protocol). Increment at the dial site immediately before `TcpStream::connect`; decrement via the same `ConnGaugeGuard` RAII pattern at the per-connection task's epilogue.

13. **Connection-gauge `ConnGaugeGuard` RAII** — Lives in `crates/envoy-cluster/src/cluster.rs` (the natural home for cluster-side per-call state). ~15 LoC: `pub struct ConnGaugeGuard { gauge: Arc<Gauge> }` + `impl Drop for ConnGaugeGuard { fn drop(&mut self) { self.gauge.dec(); } }` + `pub fn guard(&self) -> ConnGaugeGuard { self.cx_active.inc(); ConnGaugeGuard { gauge: Arc::clone(&self.cx_active) } }`. Mirrors the existing `cx_total()` accessor shape on `Cluster` and `ClusterHandle`. Tests cover the increment-on-construct + decrement-on-Drop invariants at unit-test level.

14. **Listener `cx_active` decrement uses a `ConnGaugeGuard` clone captured by the per-connection task closure.** The accept-arm pattern is:
    ```rust
    let cx_active_guard = cx_active.clone();  // Arc<Gauge> clone
    join_set.spawn(async move {
        cx_active_guard.inc();
        let result = h.handle(stream).await;
        cx_active_guard.dec();
        result
    });
    ```
    Simpler than the cluster-side `ConnGaugeGuard` because there is no shared cluster surface to factor; the accept loop already owns the connection lifecycle. Per signpost 14, the decrement runs on both success and error close paths uniformly.

15. **06.2 REVIEW I1 fix (H1 state-init tightening) lands at Task 4** alongside the per-response-class HCM counter wiring (which touches the same `match outcome { ... }` block at `crates/envoy-http1/src/hcm.rs:319-457`). The two changes are mechanically co-located; landing them in the same task minimizes blast radius. Mechanical fix: drop `mut` + default-value initializers on lines 313-316; mirror H2's `let response_status_for_log: u16; let response_body_len: u64; let response_headers_for_log: Vec<(String, String)>; let mut upstream_host_for_log: Option<String> = None;` posture. Verify all 5 H1 writer arms still type-check after the drop (the writer arms already populate before reading per PROGRESS Task 6's factored-dispatch contract).

16. **06.2 REVIEW I2 empirical diagnosis lands at Task 10** alongside the BEHAVIOR_CONTRACT.md `Stat-name mapping` extension. Mechanical approach: temporarily tighten fixture 0012's `expectations.yaml` row 12 to `rule: exact` + `exact: "-"` (the suspected actual divergence), run the fixture in CI, observe the failure mode. If GREEN, the wildcard rule was unnecessarily loose and tightens to `exact: "-"` permanently. If RED, the failure surface reveals what each proxy actually emits — update BEHAVIOR_CONTRACT.md row 12 + leave the fixture rule (potentially tightened from wildcard to a `IgnoreOrExact` shape) reflecting the empirical truth. Task 10 narrates the diagnosis fully.

17. **06.2 REVIEW M3 doc fix lands at Task 11** alongside fixture 0011's `expectations.yaml` extension (the natural "touches the fixture surface" task). ~5 LoC of doc edit in `tests/fixtures/0012-access-log-file-sink/README.md`: replace the pre-CI-fix paths (`/tmp/0012-envoy-access.log`, `/tmp/0012-envoy-rust-access.log`) with the post-CI-fix paths (`/tmp/0012-envoy-mount/access.log`, `/tmp/0012-envoy-rust-mount/access.log`) + add a one-line note explaining the parent-dir bind-mount rationale + cross-reference to `tests/differential/src/lib.rs:1374-1432` in-tree comment.

18. **06.1 REVIEW I1 fix (admin handler idle read timeout) lands at Task 9** as a dedicated mechanical task. Sits between the cluster-side stats tasks (Tasks 5-7) and the doc/fixture tasks (Tasks 10-11). Task 9 is intentionally small (~10 LoC at one site at `crates/envoy-admin/src/handler.rs:44-67`) + 1 dedicated unit test verifying slow-loris timeout. Closes 06.1 REVIEW I1 per user recommendation to fold opportunistically into 06.3.

19. **No new ADRs projected.** Per SPEC §8 + parent §7 + the 06.1/06.2 outcomes ratifying the no-foundations-grant posture, 06.3 lands with `DECISIONS.md` ledger head unchanged at **ADR-0029**. If an unforeseen design ambiguity surfaces mid-execution per D-3.5, the planner appends the next-sequential ADR (likely ADR-0030 or ADR-0031) at the time it lands and records it in PROGRESS.

20. **No new fuzz target.** 06.3 SPEC §1 (d): *"No new fuzz target ships in 06.3."* The existing `parse_bootstrap` target runs against the corpus extended in 06.1 (admin_with_stats_route.yaml) + 06.2 (hcm_access_log_file.yaml) — 17 seeds total at 06.3 entry. The D14.3 validator gate's reject-path is structurally covered by the existing serde-walk (the validator runs after deserialization succeeds, and the new gate's reject-path is exercised by the 6 unit tests in Task 2). No additional seed required.

21. **Cargo.lock cadence unchanged.** No new top-level Cargo deps in 06.3 under the recommended posture (06.3 reuses `envoy-stats` registry + `envoy-accesslog` dispatch site). The `Cargo.lock` diff between the 06.2 close-out commit `389ef96` and the 06.3 state-6 commit is anticipated at ≤5 lines (workspace-internal path-dep refresh on `envoy-cluster` or `envoy-listener` if their `Cargo.toml` gains the `count_active: bool` field on `ListenerConfig`; but the field is internal to envoy-listener's public surface — no new dep).

22. **06.1 carryforwards untouched at 06.3 entry** (per the planning posture before Task 1 lands): I2 (admin `serialize_response` 5-header injection) → phase 08; M1 → phase 08; M2 → indefinite; M3 → indefinite; M4 → phase 08; M5 → 02.2 chain; M6 → indefinite. None overlap with 06.3 scope.

---

## LoC drift posture (per BOOTSTRAP_PROMPT.md §6.1 + parent-06 SPEC §5 alternative (vi))

06.3 SPEC §3 projects ~770 LoC across D14.3-D20.3 + ~80 review/state-6 overhead. The §6.1 split-gate is **~25 tasks OR ~1500 LoC**. 06.3 is comfortably under both: 12 tasks (the recommended task structure below) + ~770 projected LoC. Per parent-06 SPEC §5 alternative (vi), 06.3 may NOT nest-split itself — the accept-drift posture is the established release valve.

**LoC ground truth checkpoints (recorded in PROGRESS Task 1 + state-4 verification):**
- 06.1 SPEC projected ~1300 LoC; PLAN projected ~2010 LoC; actual landed +~3300 LoC (`git diff --shortstat 1f7661a..55fe62d` reports 6760 insertions / 480 deletions across 64 files; the +6760 number includes ~4126 lines of PLAN.md + ~421 lines of PROGRESS.md + ~3000 lines of code+fixtures+tests).
- 06.2 SPEC projected ~1300 LoC; PLAN projected ~1875 LoC; actual landed +~3400 LoC (the 06.2 REVIEW notes ~3434 insertions / 122 deletions including ~3545 lines PLAN.md + ~455 PROGRESS.md).
- 06.3 projection per SPEC §3: ~850 LoC code+tests+docs.

Per the 06.1 / 06.2 precedent, the actual PLAN.md + PROGRESS.md overhead is ~3500-4000 lines, and the substantive code+test diff is ~1500-2000 lines. The accept-drift posture binds: if execution-time pressure pushes a task over its task-local budget, the planner records the drift in PROGRESS for that task and continues; no nest-splitting.

---

## Task summary

12 tasks total. Task 1 is the PROGRESS preamble (lands AT this state-2 PLAN.md commit). Tasks 2-12 land at state-3 each as their own commit.

| # | Title | Scope | Carryforwards closed |
|---|---|---|---|
| 1 | PROGRESS.md preamble + LoC drift + SPEC corrections + signpost choices | Doc-only; lands at this state-2 commit | — |
| 2 | D14.3: `Http2ClusterFromHttp1Listener` parse-time validator gate | ~130 LoC; envoy-config | **05.3 REVIEW I1** |
| 3 | D18.3: Extend `BodyRule::PrometheusExposition` with value assertion | ~50 LoC; tests/differential | — |
| 4 | D15.3.a: Per-response-class HCM counters + 06.2 REVIEW I1 state-init tightening | ~50 LoC; envoy-http1 HCMStats + dispatch site | **06.2 REVIEW I1** |
| 5 | D15.3.b: Listener `cx_active` gauge (data-path scope) | ~80 LoC; envoy-listener + envoy-bin wiring | — |
| 6 | D15.3.b: Cluster `cx_active` gauge + `ConnGaugeGuard` RAII + HCM/TCP-proxy call-site wiring | ~120 LoC; envoy-cluster + envoy-http1 + envoy-http2 + envoy-tcp | — |
| 7 | D15.3.c: Upstream-side router counters (`upstream_rq_total` / `_5xx`) + H1 `write_proxied_response` signature change + H2 inline increments | ~70 LoC; envoy-cluster + envoy-http1/router + envoy-http2/hcm | — |
| 8 | D15.3.d: Listener accept-failure counter (`downstream_cx_accept_failed`) | ~30 LoC; envoy-listener | — |
| 9 | 06.1 REVIEW I1 fix: admin handler idle read timeout | ~10 LoC; envoy-admin/handler.rs + 1 unit test | **06.1 REVIEW I1** |
| 10 | D15.3.e: Access-log line counter (`access_logs_total` + `access_logs_failed`) | ~50 LoC; envoy-http1 HCMStats + dispatch site + H1/H2 increment + 06.2 REVIEW I2 empirical diagnosis | **06.2 REVIEW I2** |
| 11 | D16.3 + D17.3: BEHAVIOR_CONTRACT.md `Stat-name mapping` extension + fixture 0011 `expectations.yaml` extension + request-set extension + 06.2 REVIEW M3 README doc fix | ~120 LoC doc + YAML; BEHAVIOR_CONTRACT.md + tests/fixtures/0011 + tests/fixtures/0012/README.md | **06.2 REVIEW M3** |
| 12 | D20.3: State-4 phase-done verification | PROGRESS quote; no code | — |

**Total projected:** ~770 LoC code+tests + ~70 LoC docs + ~10 LoC fixture YAML; well under split-gate thresholds.

**Sequencing rationale:**
- Task 2 (D14.3 validator gate) lands first per 06.3 SPEC §5 — closes 05.3 REVIEW I1 as a Task-1 preamble, independent of the comprehensive-wiring slice.
- Task 3 (D18.3 harness extension) lands BEFORE D17.3 (Task 11) because Task 11's extended `expectations.yaml` references the new `value_exact` / `value_must_be_zero` / `value_present_only` fields.
- Tasks 4-7 wire the comprehensive stats in roughly per-stat-family order: per-class HCM counters (Task 4); listener gauge (Task 5); cluster gauge + RAII guard (Task 6); upstream router counters (Task 7). Each task is self-contained — the increment site is at the consumer crate; no cross-task state.
- Task 8 (accept-failure counter) is mechanically small and could fold into Task 5 (same envoy-listener seam) but is kept separate for blast-radius clarity (Task 5 adds the gauge; Task 8 adds the counter).
- Task 9 (admin handler idle read timeout) is isolated from the stats-wiring slice; positioned mid-PLAN to avoid blocking the wiring tasks if it surfaces unexpected scope.
- Task 10 (access_logs_total + access_logs_failed) lands after Task 4 because both touch HCMStats; sequenced after the connection-lifetime gauges (Tasks 5-6) so the unified stats-wiring picture is in place. Also folds the 06.2 REVIEW I2 empirical diagnosis (touches fixture 0012's expectations + BEHAVIOR_CONTRACT.md row 12).
- Task 11 (BEHAVIOR_CONTRACT.md + fixture 0011 + 06.2 M3 doc fix) lands LAST among substantive tasks — extends the contract first, then aligns the fixture per 06.1 REVIEW §7 R-1 ("Do NOT silently widen the allow-list — widen the contract first, allow-list second").
- Task 12 (state-4 verification) lands last; no code, PROGRESS-only.

---

## File structure overview

### Created (new files)

None. 06.3 introduces no new workspace members, no new module files, no new fixtures. All edits land in existing files.

### Modified

- **`crates/envoy-config/src/lib.rs`** — append `Http2ClusterFromHttp1Listener { listener: String, cluster: String }` variant to `ConfigError` enum.
- **`crates/envoy-config/src/bootstrap.rs`** — extend `validate()` with the per-listener H1×H2 cluster-reachability scan (Task 2); ~30 LoC + 6 unit tests.
- **`crates/envoy-http1/src/hcm.rs`** —
  - Extend `HCMStats` struct with `downstream_rq_2xx/3xx/4xx/5xx: Arc<Counter>` + `access_logs_total: Arc<Counter>` + `access_logs_failed: Arc<Counter>` fields (Tasks 4 + 10).
  - Extend `HCMStats::register` with 6 new `register_counter` calls (Tasks 4 + 10).
  - Add per-response-class increment block at the post-`match outcome` site (after line 457; before the access-log dispatch at line 465) — uses `response_status_for_log / 100` (Task 4).
  - Tighten H1 state-init lines 313-316 from `mut x = 0/default` to `let x;` posture (Task 4, per 06.2 REVIEW I1).
  - Add `config.stats.access_logs_total.add(config.access_log.len() as u64)` BEFORE the for-loop at line 484; convert `if let Err(e) = sink.emit(...)` to `if let Err(e) = sink.emit(...) { config.stats.access_logs_failed.inc(); tracing::warn!(...) }` (Task 10).
- **`crates/envoy-http2/src/hcm.rs`** —
  - No `HCMStats` edits (HCMConfig is type-aliased; the new fields land via envoy-http1's HCMStats).
  - Add per-response-class increment block inside `finalize_h2_stream` BEFORE the access-log dispatch at line 367 — uses the `response_status_for_log` parameter (Task 4).
  - Add `access_logs_total.add()` + `access_logs_failed.inc()` symmetric to H1 (Task 10).
  - Add `cluster.upstream_rq_total.inc()` + conditional `cluster.upstream_rq_5xx.inc()` inline at the post-dispatch `proxy_resp` construction site (lines 280-318), per PLAN-write SPEC correction 3 (Task 7).
  - Add `cluster.cx_active` increment + `ConnGaugeGuard` RAII at the H1 + H2 client.connect call sites (lines 222-230 + 235-244) — alongside the existing `cx_total().inc()` calls (Task 6).
- **`crates/envoy-listener/src/lib.rs`** —
  - Extend `Listener` struct with `cx_active: Arc<Gauge>` (Task 5) + `cx_accept_failed: Arc<Counter>` (Task 8) fields.
  - Extend `ListenerConfig` with `count_active: bool` field (default `true`; envoy-bin admin-wiring sets `false`) — per signpost 7's data-path-only scope (Task 5).
  - Hoist `cx_active.clone()` + `cx_accept_failed.clone()` from `self` for `tokio::select!` capture (Task 5 + 8).
  - Wrap the per-connection task closure with `cx_active.inc()` / `cx_active.dec()` (Task 5).
  - Add `cx_accept_failed.inc()` in the `Err(_)` arm at line 165 (Task 8).
- **`crates/envoy-cluster/src/cluster.rs`** —
  - Extend `Cluster` struct with `cx_active: Arc<Gauge>` + `upstream_rq_total: Arc<Counter>` + `upstream_rq_5xx: Arc<Counter>` fields (Tasks 6 + 7).
  - Add `cx_active()`, `upstream_rq_total()`, `upstream_rq_5xx()` accessors on `Cluster` and `ClusterHandle` (parallel to existing `cx_total()` shape) (Tasks 6 + 7).
  - Add `pub struct ConnGaugeGuard { gauge: Arc<Gauge> }` + `impl Drop for ConnGaugeGuard` + `Cluster::cx_active_guard()` / `ClusterHandle::cx_active_guard()` accessor (Task 6).
  - Extend `from_bootstrap` registration block (around line 317-333) with the 3 new register calls (Tasks 6 + 7).
- **`crates/envoy-http1/src/router.rs`** — extend `write_proxied_response` signature with `cluster: &envoy_cluster::ClusterHandle` parameter; add `cluster.upstream_rq_total().inc()` + conditional `cluster.upstream_rq_5xx().inc()` at function prologue (Task 7).
- **`crates/envoy-http1/src/hcm.rs`** (continued) — update the `write_proxied_response` call at line 418-424 to pass `&cluster` (which is `&envoy_cluster::ClusterHandle` from the `config.cluster_mgr.get(&cluster_name)` resolution) (Task 7).
- **`crates/envoy-tcp/src/proxy.rs`** — at the `dial` call site (where `TcpStream::connect` runs), wrap with `let _guard = cluster.cx_active_guard()` BEFORE the connect (per signpost 5 + RAII pattern from Task 6) (Task 6).
- **`crates/envoy-admin/src/handler.rs`** — wrap `stream.read(&mut scratch[..take]).await?` at line 56 with `tokio::time::timeout(Duration::from_secs(5), ...)`; map `Elapsed` to a clean close (returns the existing `UnexpectedEof` error pattern at line 57-62) (Task 9).
- **`crates/envoy-admin/src/handler.rs`** (continued) — add `IDLE_READ_TIMEOUT: Duration = Duration::from_secs(5)` constant near `MAX_REQUEST_HEAD` at line 19 (Task 9).
- **`crates/envoy-bin/src/main.rs`** — at the admin-listener wiring site, set `ListenerConfig.count_active = false` (Task 5).
- **`tests/differential/src/lib.rs`** —
  - Extend `BodyRule::PrometheusExposition` with `value_exact: Vec<(String, u64)>` (Vec, not HashMap, for deterministic ordering in error messages) + `value_must_be_zero: Vec<String>` + `value_present_only: Vec<String>` fields, each `#[serde(default)]` (Task 3).
  - Extend `parse_prometheus_metric_names` shape OR add a sibling `parse_prometheus_samples(body: &[u8]) -> BTreeMap<String, u64>` that returns name → value pairs (Task 3).
  - Extend `assert_body_rule`'s `PrometheusExposition` arm with the new value assertions (Task 3).
- **`tests/fixtures/0011-admin-stats-prometheus/expectations.yaml`** — extend with `value_exact` / `value_must_be_zero` / `value_present_only` rows for the comprehensive stat set; extend `allowlist_envoy_only` with any new Envoy-emitted names that surface from the comprehensive set's first-run diff (Task 11).
- **`tests/fixtures/0011-admin-stats-prometheus/envoy.yaml`** — extend the HCM `route_config` with 4 routes (one per status class): `direct_response: 200`, `direct_response: 301`, `direct_response: 404`, router-proxy to a `cluster: synthetic_5xx_backend`; add a `synthetic_5xx_backend` cluster pointing at a harness-spawned 5xx-emitting backend (per signpost 5 hybrid recommendation) (Task 11).
- **`tests/fixtures/0011-admin-stats-prometheus/envoy-rust.yaml`** — symmetric extensions (Task 11).
- **`tests/fixtures/0011-admin-stats-prometheus/inputs/payload.bin`** — extend the request-sequence description to drive 4 sequential requests, one per status class (Task 11). (The file's role in `Driver::AdminScrape` is harness-shape consistency — the actual request driving is via `pre_requests` in `expectations.yaml`. Per signpost: extend `pre_requests` to a 4-entry vec; payload.bin documents it.)
- **`tests/fixtures/0011-admin-stats-prometheus/README.md`** — extend the existing README's "request flow" subsection to document the 4-class sequence (Task 11).
- **`tests/fixtures/0012-access-log-file-sink/README.md`** — correct the access-log path references to the post-CI-fix paths (`/tmp/0012-envoy-mount/access.log`, `/tmp/0012-envoy-rust-mount/access.log`); add a one-line note about the parent-dir bind-mount strategy (Task 11; closes 06.2 REVIEW M3).
- **`tests/fixtures/0012-access-log-file-sink/expectations.yaml`** (conditional) — depending on Task 10's empirical diagnosis of 06.2 REVIEW I2, either tighten the User-Agent row to `exact: "-"` (if the wildcard rule was unnecessarily loose), or leave it as `wildcard` with updated `BEHAVIOR_CONTRACT.md` row 12 rationale (Task 10).
- **`docs/envoy-rust/BEHAVIOR_CONTRACT.md`** — extend the `Stat-name mapping` table with the 9 new rows (per SPEC §2 table) + (conditional) update row 12 of `Access log field mapping` per Task 10's empirical diagnosis (Task 11 + Task 10).
- **`docs/envoy-rust/phases/06.3-stats-wiring-and-close/PROGRESS.md`** — Task 1 preamble at this state-2 commit; Tasks 2-12 narrative appended at each substantive commit.
- **`docs/envoy-rust/ROADMAP.md`** — flip row `06.3` `status: planned` → `status: in-progress` at THIS state-2 commit (per BOOTSTRAP_PROMPT.md §4.1 invariant 3 + 06.1/06.2/04.x/05.x precedent). At the eventual state-6 commit: flip row `06.3` `status: in-progress` → `status: done` AND flip parent row `06` `status: in-progress` → `status: done` per ROADMAP-schema invariant.
- **`docs/envoy-rust/STATE.md`** — advance "Active phase" block from lifecycle state 2 to state-3-next at THIS commit; advance through state-4/5/6 at the respective task commits; at state-6, transition the framing from "Phase 06.3 ... is DONE" to "Phase 07 (07-<slug>) state 1" + add "Phase-06.3 rollovers" + "Phase-06 ADR ledger (final)" subsections.
- **`docs/envoy-rust/phases/06.3-stats-wiring-and-close/REVIEW.md`** — landed at state-5 (per `superpowers:requesting-code-review`; out of THIS PLAN's scope).

### Deleted

None.

---

## Conventions

Mirrors 06.1 / 06.2 PLAN conventions:

- **TDD shape per task:** Step 1 writes the failing test; Step 2 runs it (FAIL expected; quote output); Step 3 writes the minimal implementation; Step 4 runs it (PASS expected; quote output); Step 5 commits.
- **Commit messages:** `phase 06.3: <task summary> (task N)`; mirrors 06.2's cadence (e.g., `phase 06.2: HCM H1 access-log wiring + Http1Error::AccessLogOpen + 4 unit tests (task 6)`). Co-Authored-By trailer: `Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>`.
- **PROGRESS.md per-task append:** every substantive task commit appends a per-task section to PROGRESS.md narrating: work summary, tests landed (names + LoC tally), per-task deviations from PLAN (per D-3.5 append-only discipline), LoC delta, carryforward notes if any. Mirrors 06.1's PROGRESS shape.
- **`#![forbid(unsafe_code)]`:** unchanged at every modified crate's `lib.rs`. 06.3 introduces no `unsafe` blocks.
- **No new top-level Cargo deps.** Each task's Cargo.lock diff should be ≤2 lines (workspace path-dep refresh on internal crates).
- **`cargo fmt --all` + `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean at every per-task commit** — per 06.1 REVIEW §7 recommendation 9 (the per-task fmt discipline; 06.2 already adopted this). Each task's PROGRESS section explicitly attests "fmt clean / clippy clean".
- **Stat names use Envoy's documented snake_case-with-dots verbatim** per signpost 3 + 06.1 REVIEW §7 recommendation 7 (the `format!("http.{stat_prefix}.<name>")` / `format!("listener.{name}.<stat>")` / `format!("cluster.{name}.<stat>")` namespacing convention).
- **`Counter::add(n)` for bulk-increment** per 06.1 REVIEW §7 recommendation 8. 06.3's `access_logs_total` increment uses `add(config.access_log.len() as u64)` (one bulk add per request — increments by N sinks rather than firing the loop body's `.inc()` N times). The per-response-class counters use `.inc()` (single increment per request).
- **PROGRESS Task 1 at state-2 commit** — narrates the LoC drift posture, SPEC corrections, signpost choices, decisions locked at PLAN-write time. Mirrors 06.1 / 06.2 Task-1 PROGRESS preambles.

---

## Task 1: PROGRESS.md preamble + LoC drift posture + 5 SPEC corrections + 22 architecture decisions

**Scope:** Doc-only. Lands at THIS state-2 standalone PLAN.md commit. Records the LoC drift posture, the 5 PLAN-write SPEC corrections enumerated above, the 22 architecture decisions, the task-ordering rationale, and the LoC-budget ground-truth projection. Stranger-readable per D-3.4.

**Files:**
- Create: `docs/envoy-rust/phases/06.3-stats-wiring-and-close/PROGRESS.md`

- [ ] **Step 1: Write the PROGRESS.md preamble**

```markdown
# Phase 06.3 (`06.3-stats-wiring-and-close`) — PROGRESS

> Per-task narrative log appended at each substantive commit. Stranger-readable
> per D-3.4. Task 1 lands AT the state-2 standalone PLAN.md commit (no separate
> Task 1 commit per the standalone-PLAN cadence established by 06.1
> `505653d` and 06.2 `dc00750`).

## Task 1 — PROGRESS.md preamble + LoC drift posture + 5 SPEC corrections + 22 architecture decisions (state-2 commit)

This task lands AT the state-2 standalone PLAN.md commit. The remaining Tasks
2-12 land at state-3 each as their own commit.

### LoC drift posture (per BOOTSTRAP_PROMPT.md §6.1 + parent-06 SPEC §5 alternative (vi))

06.3 SPEC §3 projects ~770 LoC code+tests + ~80 review/state-6 overhead.
Task-count projection: 12 tasks. Both projections are comfortably under the
§6.1 split-gate (~25 tasks or ~1500 LoC of net change).

Per parent-06 SPEC §5 alternative (vi), 06.3 may NOT nest-split itself even
if execution-time drift pushes a task over its task-local budget — the
accept-drift posture is the established release valve. The 06.1 + 06.2
precedent ratifies this: 06.1 SPEC projected ~1300 LoC and PLAN landed
~2010 LoC; 06.2 SPEC projected ~1300 LoC and PLAN landed ~1875 LoC; both
honored the no-nest-split posture and absorbed the ~+50% PLAN-vs-SPEC
narrative-density growth without re-splitting.

### PLAN-write SPEC corrections (recorded for the executor; 5 corrections)

Mirrors 06.1's 4 corrections + 06.2's 4 + 1 clarifying. Per D-3.5, the
SPEC remains in-tree unedited; corrections are recorded HERE so a stranger
reading PROGRESS catches the SPEC-vs-implementation diff:

1. **SPEC §3 D15.3.a wrongly co-locates per-class HCM counter increment with
   06.1's `downstream_rq_total` increment site.** Empirically the 06.1
   increment fires at request-entry time (`crates/envoy-http1/src/hcm.rs:251`),
   not at on-response-complete. Resolution: per-class counters land at the
   factored access-log dispatch site (post-`match outcome` block, lines 459+),
   after `response_status_for_log` is populated. 06.1's request-entry
   `downstream_rq_total.inc()` continues unchanged.

2. **SPEC §3 D15.3.b's listener gauge claim needs to factor 06.1 D4.a's
   `let cx_total = self.cx_total;` hoist for the `tokio::select!` accept-arm
   capture.** The new `cx_active` gauge follows the same hoist pattern. Per
   signpost 7 the gauge scopes to data-path listeners only — the planner
   threads a `count_active: bool` config flag through `ListenerConfig`,
   defaulting to `true` and overridden to `false` at envoy-bin's
   admin-listener construction.

3. **SPEC §3 D15.3.c proposes adding `cluster: &ClusterHandle` to
   `write_proxied_response`** — straightforward at H1's call site
   (`crates/envoy-http1/src/hcm.rs:418-424`) but the H2 router-arm does NOT
   call `write_proxied_response` (it builds the downstream `Response` inline
   at `crates/envoy-http2/src/hcm.rs:280-318`). Resolution: H2 lands inline
   `upstream_rq_total.inc()` + `upstream_rq_5xx.inc()` at the proxy-resp
   construction site, parallel to the H1 helper's increments.

4. **SPEC §3 D14.3 validator scan reuses the existing
   `for vh in &mut hcm.route_config.virtual_hosts { for r in &mut vh.routes }`
   walk shape at `crates/envoy-config/src/bootstrap.rs:1346-1401`.** The new
   H1×H2 reachability check sits inside the existing `RouteAction::Route(ar)`
   arm at line 1387-1394 alongside the `UnknownCluster` check. No new walk
   structure; the cluster-name HashMap is built once before the listener
   walk (per signpost 1's eager single-pass recommendation).

5. **SPEC §3 D15.3.b cluster-side gauge increment site is at the HCM
   proxy-arm call sites** (`crates/envoy-http1/src/hcm.rs:389-396` +
   `crates/envoy-http2/src/hcm.rs:222-244`), NOT inside `envoy-http1::Client`
   or `envoy-http2::Client`. Per parent-06 SPEC §6 Rule 2 (consumers
   increment), putting the increment inside the codec crates would couple
   them to the cluster-stats namespace. The decrement is RAII-style via
   `ConnGaugeGuard` from envoy-cluster (architecture decision 13).

### Architecture decisions locked at PLAN-write time (22 decisions)

See PLAN.md "Architecture decisions locked at PLAN-write time (signpost
choices)" section for the full table — reproduced inline at task commits
that consult the decision.

### Task-ordering rationale

See PLAN.md "Task summary > Sequencing rationale" — Task 2 (D14.3) first per
SPEC §5 close-out posture; Task 3 (D18.3 harness) before Task 11 (D17.3
fixture) so the fixture references the new BodyRule fields; Tasks 4-8 wire
the comprehensive stats in per-stat-family order; Task 9 (06.1 REVIEW I1)
isolated mid-PLAN; Task 10 (D15.3.e + 06.2 REVIEW I2 diagnosis) folds the
access-log line counter with the I2 empirical diagnosis; Task 11 (D16.3 +
D17.3 + 06.2 M3) lands LAST among substantive tasks (extends contract before
allow-list per 06.1 REVIEW §7 R-1); Task 12 (D20.3) state-4 verification.

### Carryforwards closed in 06.3 (planned)

- **05.3 REVIEW I1** (closed at Task 2 via `ConfigError::Http2ClusterFromHttp1Listener` parse-time gate).
- **06.1 REVIEW I1** (closed at Task 9 via admin handler idle read timeout).
- **06.2 REVIEW I1** (closed at Task 4 via H1 state-init tightening, mechanically co-located with per-class HCM counter wiring).
- **06.2 REVIEW I2** (closed at Task 10 via empirical diagnosis + BEHAVIOR_CONTRACT.md row 12 update OR fixture 0012 expectations.yaml tightening).
- **06.2 REVIEW M3** (closed at Task 11 via fixture 0012 README.md path correction).

### Standing carryforwards untouched in 06.3 (per parent-06 SPEC §4 + 06.1/06.2 REVIEW §4 inventories)

- 06.2 REVIEW M1 (Http1Error::AccessLogOpen source-chain typing) — indefinite.
- 06.2 REVIEW M2 (BodyRule::ByteExact literal-body assertion) — indefinite.
- 06.2 REVIEW M4 (/tmp/0012-envoy-mount process-shared path) — activates under nextest sharding.
- 06.2 REVIEW M5 (H2 access-log test 200ms sleep) — 02.2 M1 chain.
- 06.1 REVIEW I2 + M1 + M4 — phase 08.
- 06.1 REVIEW M2 / M3 / M5 / M6 — indefinite / 02.2 chain.
- 05.3 REVIEW I2 (typed-error chain dissolution at H2 dispatch) — defers to phase that next touches H2 router-arm.
- 05.2 REVIEW I1 + I2 + I3 — defers to whichever phase next touches `.github/workflows/ci.yml` or the h2 codec.
- 04.1 REVIEW M5/M9 (Cargo.lock cadence ratification) — couples with conditional ADR-0031.
- 04.1 REVIEW M7 (TLS+H2 ALPN dispatch generalization) — defers to phase that ships H2+TLS.
- 04.1 REVIEW M1/M2/M4 — defers to phase exercising duplicate response headers / stalled body / IPv6 Host.
- 02.2 REVIEW M1 (EchoBackend Drop polling) — standing.

### DECISIONS.md ledger head at state-2

**ADR-0029** (parent-06 split decision; landed at `1f7661a`). No ADRs landed
in 06.1 or 06.2; recommended posture honored. No new ADRs projected for 06.3
per SPEC §8. Conditional ADR-0030 (foundations grant) + ADR-0031 (Cargo.lock
cadence) stay available; recommendation per parent-06 SPEC §7 is no
foundations grants in phase 06.
```

- [ ] **Step 2: Write the ROADMAP.md row 06.3 flip + STATE.md advance to state-3-next**

(These two edits land alongside this PROGRESS preamble and the PLAN.md itself in the state-2 standalone-PLAN commit. The exact edit text is enumerated in this PLAN's "State-2 commit" section below.)

- [ ] **Step 3: Self-review the PLAN against SPEC**

Skim 06.3 SPEC §3 D14.3-D20.3 + §5 + §6 + §7 signposts; verify every deliverable maps to a task; verify all 10 signposts have a decision in the architecture-decisions table; verify all 4 carryforwards from 06.2 REVIEW + 1 from 06.1 REVIEW + 1 from 05.3 REVIEW are mapped to a task. Run the SPEC coverage check + placeholder scan + type-consistency check per writing-plans skill self-review section.

- [ ] **Step 4: Commit the state-2 standalone PLAN.md**

```bash
git add docs/envoy-rust/phases/06.3-stats-wiring-and-close/PLAN.md \
        docs/envoy-rust/phases/06.3-stats-wiring-and-close/PROGRESS.md \
        docs/envoy-rust/ROADMAP.md \
        docs/envoy-rust/STATE.md
git commit -m "$(cat <<'EOF'
phase 06.3: state-2 standalone PLAN.md

Per BOOTSTRAP_PROMPT.md §5 state 2 + SKILL_ROUTING.md lines 17-22 +
the established standalone-pre-Task-1-PLAN cadence (precedent commits
c02eea7 04.3, f23d08f 05.1, 252725b 05.4, ce471ad 05.2, 4b92e05 05.3,
505653d 06.1, dc00750 06.2). PLAN.md materializes 06.3 SPEC §3
D14.3-D20.3 across 12 tasks (Task 1 PROGRESS preamble at this commit;
Tasks 2-12 substantive at state-3). 5 SPEC corrections + 22 architecture
decisions captured at PLAN-write time per D-3.5.

Flips ROADMAP row 06.3 status: planned → in-progress.
Advances STATE.md to lifecycle state-3-next; next-skill
superpowers:subagent-driven-development scoped to PLAN Task 2
(D14.3 ConfigError::Http2ClusterFromHttp1Listener validator gate).
Parent row 06 stays in-progress (flips to done only at 06.3 state-6
per ROADMAP-schema invariant). DECISIONS.md ledger head unchanged at
ADR-0029.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: D14.3 — `Http2ClusterFromHttp1Listener` parse-time validator gate (closes 05.3 REVIEW I1)

**Scope:** ~130 LoC. Mechanical extension of `crates/envoy-config/src/bootstrap.rs::validate`. Adds a new `ConfigError::Http2ClusterFromHttp1Listener { listener: String, cluster: String }` variant + a per-listener cluster-reachability scan at the existing route-walk. Substantively closes phase-05.3 REVIEW I1 (silent H1-listener × H2-cluster misnegotiation per ADR-0028's option-B deferral). Mirrors 05.1 Task 1's posture toward phase-02.1 REVIEW I3 (a previously-identified gap closed cheaply at the start of an unrelated phase).

**Files:**
- Modify: `crates/envoy-config/src/lib.rs` (append `ConfigError::Http2ClusterFromHttp1Listener { listener: String, cluster: String }` variant; ~5 LoC).
- Modify: `crates/envoy-config/src/bootstrap.rs::validate` (add the per-listener cluster-reachability scan inside the existing route-walk at lines 1346-1401; ~30 LoC).
- Test: `crates/envoy-config/src/bootstrap.rs::tests` (6 unit tests; ~100 LoC).

- [ ] **Step 1: Write 6 failing unit tests**

Add to `crates/envoy-config/src/bootstrap.rs::tests`:

```rust
#[test]
fn validates_h1_listener_with_h1_cluster_passes() {
    // Bootstrap with one HCM listener (codec_type: HTTP1) + one route to
    // cluster `backend` + one cluster `backend` with no
    // typed_extension_protocol_options (defaults to H1 upstream per 05.3 D3).
    // Validator accepts.
    let yaml = r#"
node: { id: t, cluster: t }
static_resources:
  listeners:
    - name: ingress_http
      address: { socket_address: { address: 0.0.0.0, port_value: 0 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config: { "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router }
  clusters:
    - name: backend
      type: STATIC
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint: { address: { socket_address: { address: 127.0.0.1, port_value: 1 } } }
"#;
    let mut b: Bootstrap = serde_yaml::from_str(yaml).unwrap();
    validate(&mut b).expect("H1 listener × H1 cluster passes");
}

#[test]
fn validates_h2_listener_with_h2_cluster_passes() {
    // codec_type: HTTP2 + cluster has typed_extension_protocol_options.
    let yaml = r#"
node: { id: t, cluster: t }
static_resources:
  listeners:
    - name: ingress_http
      address: { socket_address: { address: 0.0.0.0, port_value: 0 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP2
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config: { "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router }
  clusters:
    - name: backend
      type: STATIC
      typed_extension_protocol_options:
        envoy.extensions.upstreams.http.v3.HttpProtocolOptions:
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http2_protocol_options: {}
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint: { address: { socket_address: { address: 127.0.0.1, port_value: 1 } } }
"#;
    let mut b: Bootstrap = serde_yaml::from_str(yaml).unwrap();
    validate(&mut b).expect("H2 listener × H2 cluster passes");
}

#[test]
fn validates_h2_listener_with_h1_cluster_passes() {
    // codec_type: HTTP2 + cluster default (H1 upstream).
    // Load-bearing combination per 05.3 D4 — H2 listener dispatches to H1
    // cluster via cluster.upstream_protocol() == Http1 arm.
    let yaml = /* ... codec_type: HTTP2 + cluster with no typed_extension_protocol_options ... */;
    let mut b: Bootstrap = serde_yaml::from_str(yaml).unwrap();
    validate(&mut b).expect("H2 listener × H1 cluster passes");
}

#[test]
fn rejects_h1_listener_with_h2_cluster() {
    // codec_type: HTTP1 + cluster has typed_extension_protocol_options.
    // Validator returns ConfigError::Http2ClusterFromHttp1Listener.
    let yaml = /* ... codec_type: HTTP1 + cluster with http2_protocol_options ... */;
    let mut b: Bootstrap = serde_yaml::from_str(yaml).unwrap();
    let err = validate(&mut b).unwrap_err();
    match err {
        ConfigError::Http2ClusterFromHttp1Listener { listener, cluster } => {
            assert_eq!(listener, "ingress_http");
            assert_eq!(cluster, "backend");
        }
        other => panic!("expected Http2ClusterFromHttp1Listener, got {other:?}"),
    }
}

#[test]
fn rejects_auto_listener_with_h2_cluster() {
    // codec_type: AUTO + cluster has http2_protocol_options.
    // AUTO behaves as H1-only per parent §4; the gate engages identically.
    let yaml = /* ... codec_type: AUTO + cluster with http2_protocol_options ... */;
    let mut b: Bootstrap = serde_yaml::from_str(yaml).unwrap();
    let err = validate(&mut b).unwrap_err();
    assert!(matches!(err, ConfigError::Http2ClusterFromHttp1Listener { .. }));
}

#[test]
fn tcp_proxy_listener_with_h2_cluster_unaffected() {
    // TCP-proxy listener (no codec_type) + cluster with http2_protocol_options.
    // Validator accepts — the carve-out skips TCP-proxy listeners.
    let yaml = /* ... TcpProxy filter + cluster with http2_protocol_options ... */;
    let mut b: Bootstrap = serde_yaml::from_str(yaml).unwrap();
    validate(&mut b).expect("TCP-proxy listener × H2 cluster — carve-out passes");
}
```

(Fill in the elided YAML in each test verbatim with the shape sketched in the
first two tests. The executor expands each YAML at test-write time.)

- [ ] **Step 2: Run tests to verify they fail with "variant not defined"**

Run: `cargo test -p envoy-config validates_h1_listener_with_h1_cluster_passes rejects_h1_listener_with_h2_cluster -- --nocapture`

Expected: FAIL with `error[E0599]: no variant or associated item named 'Http2ClusterFromHttp1Listener' found for enum 'ConfigError'` OR (for the passing tests) `validator does not yet fire the H1×H2 gate; expected error not returned`.

- [ ] **Step 3: Add the ConfigError variant**

Edit `crates/envoy-config/src/lib.rs`:

```rust
// Append to the `ConfigError` enum (just before the closing brace):
/// 06.3 D14.3: listener with codec_type HTTP1 or AUTO routes to a cluster
/// whose typed_extension_protocol_options.HttpProtocolOptions.
/// explicit_http_config.http2_protocol_options is set. Closes
/// phase-05.3 REVIEW I1 substantively — ADR-0028's option-(B) deferred
/// the H1-listener H2-arm dispatch (envoy-http1 ↔ envoy-http2 cycle);
/// the deferral is correct doctrine but the deferred path must be
/// visibly rejected at config-load time so operators don't get a
/// confusing 502 (or worse, silent H1-on-the-wire to an H2-only backend)
/// at runtime.
#[error("listener '{listener}' has codec_type HTTP1 (or AUTO) but routes to cluster '{cluster}' whose typed_extension_protocol_options selects HTTP/2 upstream; H1-listener × H2-cluster dispatch is deferred per ADR-0028")]
Http2ClusterFromHttp1Listener {
    listener: String,
    cluster: String,
},
```

- [ ] **Step 4: Add the validator scan**

Edit `crates/envoy-config/src/bootstrap.rs`. The scan sits inside the existing per-listener walk at the `HCM_FILTER` arm (line 1206 onward). Plan:

1. Before the listener loop at line 1126, build a cluster-name → cluster reference HashMap (per signpost 1 eager single-pass):
   ```rust
   let cluster_by_name: std::collections::HashMap<&str, &Cluster> = bootstrap
       .static_resources
       .clusters
       .iter()
       .map(|c| (c.name.as_str(), c))
       .collect();
   ```
2. Inside `validate_hcm`, at the RouteAction::Route arm (line 1387-1394), AFTER the existing `UnknownCluster` check passes, add the H1×H2 reachability check:
   ```rust
   // Existing 04.3 UnknownCluster check:
   if !clusters.iter().any(|c| c.name == ar.cluster) {
       return Err(crate::ConfigError::UnknownCluster(ar.cluster.clone()));
   }
   // 06.3 D14.3 NEW: H1-listener × H2-cluster reachability gate.
   // Closes 05.3 REVIEW I1 per parent-06 SPEC §3 D14.3.
   if matches!(hcm.codec_type, CodecType::HTTP1 | CodecType::AUTO) {
       let cluster_ref = clusters
           .iter()
           .find(|c| c.name == ar.cluster)
           .expect("UnknownCluster check above guarantees presence");
       if let Some(teo) = &cluster_ref.typed_extension_protocol_options {
           if teo.http_protocol_options.explicit_http_config.http2_protocol_options.is_some() {
               return Err(crate::ConfigError::Http2ClusterFromHttp1Listener {
                   listener: /* listener-name in scope; threaded through validate_hcm's signature; see step 5 */.to_string(),
                   cluster: ar.cluster.clone(),
               });
           }
       }
   }
   ```

3. Extend `validate_hcm`'s signature to thread the listener name through:
   ```rust
   fn validate_hcm(
       hcm: &mut HttpConnectionManagerConfig,
       clusters: &[Cluster],
       chain_has_tls: bool,
       listener_name: &str,  // 06.3 NEW
   ) -> Result<(), crate::ConfigError> { ... }
   ```
   And update the existing call at line 1214 to pass `&listener.name`.

- [ ] **Step 5: Run tests; verify all 6 pass**

Run: `cargo test -p envoy-config -- validates_h1_listener_with_h1_cluster_passes validates_h2_listener_with_h2_cluster_passes validates_h2_listener_with_h1_cluster_passes rejects_h1_listener_with_h2_cluster rejects_auto_listener_with_h2_cluster tcp_proxy_listener_with_h2_cluster_unaffected --nocapture`

Expected: 6 passed; 0 failed.

- [ ] **Step 6: Verify `cargo build --workspace --all-targets` + `cargo clippy --workspace --all-targets --all-features -- -D warnings` + `cargo fmt --all -- --check` clean**

Run each in sequence; verify clean.

- [ ] **Step 7: Verify all 11 baseline differential fixtures' in-process backstops still pass (no regression on existing validator behavior)**

Run: `cargo test --workspace` (skipping the Docker-gated differential tests which require the harness; the in-process backstops at `crates/envoy-bin/tests/*.rs` validate the validator path).

Expected: 0 failures.

- [ ] **Step 8: Commit Task 2**

```bash
git add crates/envoy-config/src/lib.rs crates/envoy-config/src/bootstrap.rs \
        docs/envoy-rust/phases/06.3-stats-wiring-and-close/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 06.3: Http2ClusterFromHttp1Listener parse-time validator gate (task 2)

Closes 05.3 REVIEW I1 substantively per 06.3 SPEC §3 D14.3 + parent-06 SPEC §3.

New ConfigError::Http2ClusterFromHttp1Listener { listener: String, cluster: String }
variant fires at crates/envoy-config/src/bootstrap.rs::validate when a route on
an HTTP1 or AUTO listener targets a cluster whose
typed_extension_protocol_options.HttpProtocolOptions.explicit_http_config.
http2_protocol_options is set. ADR-0028's option-(B) deferred the H1-listener
H2-arm dispatch; the deferral remains correct doctrine but the deferred path is
now visibly rejected at config-load time so operators don't get a confusing 502
or silent H1-on-the-wire to an H2-only backend at runtime.

6 unit tests cover the gate: 3 positive (H1×H1, H2×H2, H2×H1 all pass) + 2
negative (H1×H2, AUTO×H2 both reject with the new variant) + 1 carve-out
(TCP-proxy listener with H2 cluster is unaffected per the HCM-only scope).

Mirrors phase-05.1 Task 1's posture toward phase-02.1 REVIEW I3 — a
previously-identified gap closed cheaply at the start of an unrelated phase
before the phase's substantive surface lands.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: D18.3 — Extend `BodyRule::PrometheusExposition` with value assertion

**Scope:** ~50 LoC. Additive extension to the existing harness body-rule variant (per signpost 9 option (1)). Adds three new fields: `value_exact: Vec<(String, u64)>`, `value_must_be_zero: Vec<String>`, `value_present_only: Vec<String>`, each `#[serde(default)]` for backwards-compat with the 06.1-landed name-set-only assertion. Adds a sibling parser `parse_prometheus_samples(body: &[u8]) -> BTreeMap<String, u64>` to extract name→value pairs. Extends `assert_body_rule` with the value-assertion logic. Adds 2 unit tests verifying the new value-exact + value-must-be-zero rules.

**Files:**
- Modify: `tests/differential/src/lib.rs` (extend `BodyRule::PrometheusExposition` enum variant + add `parse_prometheus_samples` + extend `assert_body_rule`).
- Test: `tests/differential/src/lib.rs::tests` (2 new tests).

- [ ] **Step 1: Write 2 failing unit tests**

```rust
#[test]
fn assert_body_rule_prometheus_exposition_passes_on_value_exact_match() {
    let envoy_body = b"# TYPE foo counter\nfoo 5\n# TYPE bar counter\nbar 0\n";
    let rust_body  = b"# TYPE foo counter\nfoo 5\n# TYPE bar counter\nbar 0\n";
    let rule = BodyRule::PrometheusExposition {
        allowlist_envoy_only: vec![],
        allowlist_envoy_rust_only: vec![],
        value_exact: vec![("foo".to_string(), 5)],
        value_must_be_zero: vec!["bar".to_string()],
        value_present_only: vec![],
    };
    assert_body_rule(&rule, envoy_body, rust_body).expect("value-exact + must-be-zero match");
}

#[test]
fn assert_body_rule_prometheus_exposition_fails_on_value_mismatch() {
    let envoy_body = b"# TYPE foo counter\nfoo 5\n";
    let rust_body  = b"# TYPE foo counter\nfoo 6\n";
    let rule = BodyRule::PrometheusExposition {
        allowlist_envoy_only: vec![],
        allowlist_envoy_rust_only: vec![],
        value_exact: vec![("foo".to_string(), 5)],
        value_must_be_zero: vec![],
        value_present_only: vec![],
    };
    let err = assert_body_rule(&rule, envoy_body, rust_body).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("value_exact mismatch"), "expected value_exact mismatch, got: {msg}");
}
```

- [ ] **Step 2: Run tests; verify they fail with "missing field" or compilation errors**

Run: `cargo test -p tests_differential assert_body_rule_prometheus_exposition_passes_on_value_exact_match assert_body_rule_prometheus_exposition_fails_on_value_mismatch -- --nocapture`

Expected: FAIL with `error[E0063]: missing fields 'value_exact', 'value_must_be_zero', 'value_present_only' in initializer of 'BodyRule::PrometheusExposition'`.

- [ ] **Step 3: Extend the BodyRule variant**

In `tests/differential/src/lib.rs`:

```rust
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BodyRule {
    ByteExact,
    /// 06.1 D6.b: parse the body as Prometheus text-exposition format and
    /// assert the metric-name set is equal between envoy and envoy-rust
    /// modulo the per-fixture allow-lists.
    ///
    /// 06.3 D18.3: extends with three value-assertion fields. All three
    /// default to empty so 06.1's name-set-only assertion continues working.
    PrometheusExposition {
        #[serde(default)]
        allowlist_envoy_only: Vec<String>,
        #[serde(default)]
        allowlist_envoy_rust_only: Vec<String>,
        /// 06.3 NEW: each pair `(stat_name, expected_value)` must match
        /// exactly on BOTH proxies' scrapes. Pairs are `Vec` (not HashMap)
        /// for deterministic ordering in error messages.
        #[serde(default)]
        value_exact: Vec<(String, u64)>,
        /// 06.3 NEW: each stat name must equal 0 on BOTH proxies' scrapes
        /// (terminal-zero gauges; e.g., listener.<name>.downstream_cx_active
        /// after the test's connections have closed).
        #[serde(default)]
        value_must_be_zero: Vec<String>,
        /// 06.3 NEW: each stat name must be present on BOTH proxies'
        /// scrapes; value may differ (for stats with disposition
        /// "name-required, value-may-differ" per BEHAVIOR_CONTRACT.md).
        #[serde(default)]
        value_present_only: Vec<String>,
    },
}
```

- [ ] **Step 4: Add `parse_prometheus_samples` sibling parser**

After `parse_prometheus_metric_names`:

```rust
/// 06.3 D18.3: parse Prometheus text-exposition body into name → value
/// pairs. Skips `#`-prefixed lines + blanks. For sample lines, extracts
/// the leading name (up to whitespace or `{`) and the trailing value
/// (parses as `u64`; non-parseable values silently skipped). Returns
/// `BTreeMap` for deterministic ordering when failure messages are
/// constructed. Labels (e.g., `metric{key="value"} 42`) are dropped —
/// the value-side of value_exact / value_must_be_zero / value_present_only
/// asserts only on the bare-name → value projection.
pub fn parse_prometheus_samples(body: &[u8]) -> std::collections::BTreeMap<String, u64> {
    let s = std::str::from_utf8(body).unwrap_or("");
    let mut out = std::collections::BTreeMap::new();
    for line in s.lines() {
        let t = line.trim_start();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        // Find name-end at first whitespace OR `{`.
        let name_end = t.find(|c: char| c.is_whitespace() || c == '{').unwrap_or(t.len());
        let name = &t[..name_end];
        if name.is_empty() {
            continue;
        }
        // Find value-start: skip past `{ ... }` if present, then skip whitespace.
        let after_name = &t[name_end..];
        let after_labels = if let Some(rest) = after_name.strip_prefix('{') {
            // Skip until the closing `}`.
            match rest.find('}') {
                Some(close_idx) => &rest[close_idx + 1..],
                None => continue, // malformed
            }
        } else {
            after_name
        };
        let value_str = after_labels.trim();
        // Strip optional trailing timestamp (space-separated second token).
        let value_field = value_str.split_whitespace().next().unwrap_or("");
        if let Ok(v) = value_field.parse::<u64>() {
            out.insert(name.to_string(), v);
        }
        // Non-u64-parseable values (floats, NaN, ...) silently skipped;
        // 06.3 emits only u64 counter / non-negative gauge values per
        // signpost 2 (Relaxed ordering on AtomicU64/AtomicI64; the
        // gauge's signed-vs-unsigned crossover is unused in 06.3 — see
        // signpost 7).
    }
    out
}
```

- [ ] **Step 5: Extend `assert_body_rule`**

```rust
fn assert_body_rule(rule: &BodyRule, envoy_body: &[u8], rust_body: &[u8]) -> Result<()> {
    match rule {
        BodyRule::ByteExact => { /* unchanged from 06.1 */ }
        BodyRule::PrometheusExposition {
            allowlist_envoy_only,
            allowlist_envoy_rust_only,
            value_exact,
            value_must_be_zero,
            value_present_only,
        } => {
            // 06.1 D6.b: name-set assertion (unchanged).
            let envoy_names = parse_prometheus_metric_names(envoy_body);
            let rust_names = parse_prometheus_metric_names(rust_body);
            let allow_envoy: BTreeSet<String> = allowlist_envoy_only.iter().cloned().collect();
            let allow_rust: BTreeSet<String> = allowlist_envoy_rust_only.iter().cloned().collect();
            let envoy_only: Vec<String> = envoy_names.difference(&rust_names)
                .filter(|n| !allow_envoy.contains(*n)).cloned().collect();
            let rust_only: Vec<String> = rust_names.difference(&envoy_names)
                .filter(|n| !allow_rust.contains(*n)).cloned().collect();
            if !envoy_only.is_empty() || !rust_only.is_empty() {
                bail!("prometheus exposition metric-name sets diverged after allow-lists:\n  envoy-only:      {envoy_only:?}\n  envoy-rust-only: {rust_only:?}");
            }

            // 06.3 D18.3 NEW: value-side assertions on samples (skip
            // assertions on missing names — name-set assertion above
            // catches absence).
            let envoy_samples = parse_prometheus_samples(envoy_body);
            let rust_samples = parse_prometheus_samples(rust_body);

            // value_exact: each (name, expected) must match on both proxies.
            let mut value_exact_mismatches = Vec::new();
            for (name, expected) in value_exact {
                let envoy_v = envoy_samples.get(name);
                let rust_v = rust_samples.get(name);
                if envoy_v != Some(expected) || rust_v != Some(expected) {
                    value_exact_mismatches.push(format!(
                        "  {name}: expected={expected}, envoy={envoy_v:?}, rust={rust_v:?}"
                    ));
                }
            }
            if !value_exact_mismatches.is_empty() {
                bail!("prometheus exposition value_exact mismatch:\n{}", value_exact_mismatches.join("\n"));
            }

            // value_must_be_zero: each name must equal 0 on both proxies.
            let mut zero_mismatches = Vec::new();
            for name in value_must_be_zero {
                let envoy_v = envoy_samples.get(name).copied().unwrap_or(u64::MAX);
                let rust_v = rust_samples.get(name).copied().unwrap_or(u64::MAX);
                if envoy_v != 0 || rust_v != 0 {
                    zero_mismatches.push(format!("  {name}: envoy={envoy_v}, rust={rust_v}"));
                }
            }
            if !zero_mismatches.is_empty() {
                bail!("prometheus exposition value_must_be_zero mismatch:\n{}", zero_mismatches.join("\n"));
            }

            // value_present_only: each name must be present on both proxies
            // (value may differ).
            let mut missing = Vec::new();
            for name in value_present_only {
                if !envoy_samples.contains_key(name) {
                    missing.push(format!("  {name}: missing on envoy"));
                }
                if !rust_samples.contains_key(name) {
                    missing.push(format!("  {name}: missing on rust"));
                }
            }
            if !missing.is_empty() {
                bail!("prometheus exposition value_present_only missing:\n{}", missing.join("\n"));
            }

            Ok(())
        }
    }
}
```

- [ ] **Step 6: Run tests; verify both pass**

Run: `cargo test -p tests_differential assert_body_rule_prometheus_exposition_passes_on_value_exact_match assert_body_rule_prometheus_exposition_fails_on_value_mismatch -- --nocapture`

Expected: 2 passed.

- [ ] **Step 7: Verify backwards-compat — 06.1's existing fixture-0011 expectations.yaml continues to deserialize**

Run: `cargo test -p tests_differential expectations_parse` and any other 06.1-landed BodyRule tests.

Expected: 0 failures (the three new fields default to empty per `#[serde(default)]`).

- [ ] **Step 8: Verify `cargo fmt` + `cargo clippy` + full workspace test clean**

- [ ] **Step 9: Commit Task 3**

```bash
git add tests/differential/src/lib.rs docs/envoy-rust/phases/06.3-stats-wiring-and-close/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 06.3: BodyRule::PrometheusExposition value-side assertion (task 3)

Extends the existing PrometheusExposition variant with three new fields per
06.3 SPEC §3 D18.3 + signpost 9 option (1):
- value_exact: Vec<(String, u64)>  — pairs that must match on both proxies
- value_must_be_zero: Vec<String>  — names whose value must equal 0
- value_present_only: Vec<String>  — names whose presence is required, value
                                     may differ

All three `#[serde(default)]` for backwards-compat with 06.1's name-set-only
assertion. New parse_prometheus_samples sibling parser extracts name → u64
pairs (drops labels; skips malformed values). assert_body_rule extended with
the new value-assertion logic. 2 unit tests cover the happy path + mismatch
error shape.

No new harness driver variants; no new fixture shapes. Backwards-compat
verified — 06.1's fixture 0011 expectations.yaml continues to deserialize
unchanged.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: D15.3.a — Per-response-class HCM counters + 06.2 REVIEW I1 H1 state-init tightening

**Scope:** ~50 LoC. Extends `HCMStats` struct with `downstream_rq_2xx/3xx/4xx/5xx: Arc<Counter>` fields. Extends `HCMStats::register` with 4 new `register_counter` calls under the `http.{stat_prefix}.<class>` namespace. Adds per-class increment block at the post-`match outcome` site in `crates/envoy-http1/src/hcm.rs` (after line 457; before the access-log dispatch at line 465). H1 + H2 surfaces share via the `envoy-http2::HCMConfig` type alias (verified at `crates/envoy-http2/src/hcm.rs:27`). **Co-located change:** tightens H1 state-init at lines 313-316 from `mut x = 0/default` to `let x;` posture (closes 06.2 REVIEW I1 per architecture decision 15). Adds 4 unit tests (one per status class) for the per-class increment, plus 1 regression test verifying all 5 writer arms still write the H1 state variables after the tightening.

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` (extend HCMStats struct + extend register + add per-class increment block + tighten state-init).
- Modify: `crates/envoy-http2/src/hcm.rs` (add per-class increment block inside `finalize_h2_stream` BEFORE the access-log dispatch).
- Test: `crates/envoy-http1/src/hcm.rs::tests` (4 per-class tests + 1 H1-state-init regression test).

- [ ] **Step 1: Write 5 failing unit tests**

```rust
// In crates/envoy-http1/src/hcm.rs::tests
#[tokio::test]
async fn hcm_increments_downstream_rq_2xx_on_2xx_response() {
    // Spin up an HCM with a direct_response: 200 route; drive 1 request;
    // assert stats.downstream_rq_2xx.value() == 1 and the other 3 class
    // counters == 0.
    let registry = Arc::new(envoy_stats::StatsRegistry::new());
    let hcm_config = synth_hcm_config_with_direct_response(200, "ok\n", &registry).await;
    drive_one_request_against_hcm(&hcm_config, "GET", "/").await;
    assert_eq!(hcm_config.stats.downstream_rq_2xx.value(), 1);
    assert_eq!(hcm_config.stats.downstream_rq_3xx.value(), 0);
    assert_eq!(hcm_config.stats.downstream_rq_4xx.value(), 0);
    assert_eq!(hcm_config.stats.downstream_rq_5xx.value(), 0);
    assert_eq!(hcm_config.stats.downstream_rq_total.value(), 1);
}

#[tokio::test]
async fn hcm_increments_downstream_rq_3xx_on_3xx_response() {
    // direct_response: 301 "moved\n".
    // ... assert _3xx == 1, others (excl. total) == 0 ...
}

#[tokio::test]
async fn hcm_increments_downstream_rq_4xx_on_4xx_response() {
    // direct_response: 404 "not found\n".
    // ... assert _4xx == 1 ...
}

#[tokio::test]
async fn hcm_increments_downstream_rq_5xx_on_5xx_response() {
    // direct_response: 503 "unavailable\n".
    // ... assert _5xx == 1 ...
}

#[tokio::test]
async fn hcm_h1_state_init_writes_in_all_5_writer_arms() {
    // Regression-guard for 06.2 REVIEW I1: after dropping `mut x = 0/default`,
    // verify each of the 5 H1 writer arms still populates the state vars.
    // Synth 5 HCM configs each exercising one arm: (1) direct_response synth,
    // (2) proxy-no-endpoint-503, (3) proxy-connect-fail-502, (4)
    // proxy-send-fail-502, (5) proxy-success. Drive 1 request through each;
    // each ought to reach the access-log dispatch site with populated state.
    // Best-shape: configure an access_log: [FileSink] on each HCM; drive the
    // request; assert the file's emitted line carries a non-zero response_code
    // and the expected upstream_host (per the writer arm).
    // (Detailed scaffolding mirrors PROGRESS Task 6's existing 5-arm tests.)
}
```

- [ ] **Step 2: Run tests; verify they fail**

Run: `cargo test -p envoy-http1 hcm_increments_downstream_rq_2xx_on_2xx_response hcm_increments_downstream_rq_3xx_on_3xx_response hcm_increments_downstream_rq_4xx_on_4xx_response hcm_increments_downstream_rq_5xx_on_5xx_response hcm_h1_state_init_writes_in_all_5_writer_arms -- --nocapture`

Expected: FAIL with `error[E0609]: no field 'downstream_rq_2xx' on type 'HCMStats'`.

- [ ] **Step 3: Extend HCMStats struct**

```rust
// crates/envoy-http1/src/hcm.rs
pub struct HCMStats {
    // 06.1 D4.c — landed:
    pub downstream_rq_total: Arc<envoy_stats::Counter>,
    // 06.3 D15.3.a NEW — per-response-class counters:
    pub downstream_rq_2xx: Arc<envoy_stats::Counter>,
    pub downstream_rq_3xx: Arc<envoy_stats::Counter>,
    pub downstream_rq_4xx: Arc<envoy_stats::Counter>,
    pub downstream_rq_5xx: Arc<envoy_stats::Counter>,
    // 06.3 D15.3.e NEW (lands at Task 10, but the field is declared here
    // for forward-compat — `register` registers it at this task too OR at
    // Task 10. The planner picks at PLAN time: register all 5 new fields
    // at THIS task to minimize blast radius; the access-log increment site
    // lands at Task 10 against the already-registered counter):
    // pub access_logs_total: Arc<envoy_stats::Counter>,  // (Task 10)
    // pub access_logs_failed: Arc<envoy_stats::Counter>, // (Task 10)
}
```

- [ ] **Step 4: Extend HCMStats::register**

```rust
impl HCMStats {
    pub fn register(
        registry: &envoy_stats::StatsRegistry,
        stat_prefix: &str,
    ) -> Result<Self, envoy_stats::StatsError> {
        Ok(Self {
            downstream_rq_total: registry
                .register_counter(&format!("http.{stat_prefix}.downstream_rq_total"))?,
            // 06.3 D15.3.a NEW:
            downstream_rq_2xx: registry
                .register_counter(&format!("http.{stat_prefix}.downstream_rq_2xx"))?,
            downstream_rq_3xx: registry
                .register_counter(&format!("http.{stat_prefix}.downstream_rq_3xx"))?,
            downstream_rq_4xx: registry
                .register_counter(&format!("http.{stat_prefix}.downstream_rq_4xx"))?,
            downstream_rq_5xx: registry
                .register_counter(&format!("http.{stat_prefix}.downstream_rq_5xx"))?,
        })
    }
}
```

- [ ] **Step 5: Tighten H1 state-init (closes 06.2 REVIEW I1)**

Edit `crates/envoy-http1/src/hcm.rs:313-316`:

```rust
// BEFORE (06.2-landed):
let mut response_status_for_log: u16 = 0;
let mut response_body_len: u64 = 0;
let mut upstream_host_for_log: Option<String> = None;
let mut response_headers_for_log: Vec<(String, String)> = Vec::new();

// AFTER (06.3 — mirrors H2's let x; posture at crates/envoy-http2/src/hcm.rs:134-137):
let response_status_for_log: u16;
let response_body_len: u64;
let response_headers_for_log: Vec<(String, String)>;
let mut upstream_host_for_log: Option<String> = None; // stays mut — only the proxy arm populates
```

Verify all 5 writer arms still type-check: synth (line 320-325 writes all 3); proxy-no-endpoint-503 (line 344-348 writes all 3 inside the `None =>` arm); proxy-connect-fail-502 (line 449-452 writes all 3); proxy-send-fail-502 (line 434-437 writes all 3); proxy-success (line 402-417 writes all 3 inside the Ok arm). Rust's flow analysis verifies every arm of the outer match writes all 3 uninitialized locals before they're read at the dispatch site.

- [ ] **Step 6: Add per-class increment block at the post-match site**

Edit `crates/envoy-http1/src/hcm.rs` — between line 457 (end of `match outcome { ... }`) and line 465 (start of `if !config.access_log.is_empty()`):

```rust
// 06.3 D15.3.a NEW — per-response-class HCM counters. Increment fires
// AFTER all 5 writer arms have populated `response_status_for_log`,
// at the same factored dispatch site that 06.2's access-log dispatch
// uses. The 06.1-landed `downstream_rq_total` increment at line 251
// fires at request-entry (unchanged); the per-class increments require
// the status code, so they land here.
//
// Status codes outside [200, 600) silently no-op — matches Envoy's
// documented behavior at
// https://www.envoyproxy.io/docs/envoy/v1.33.0/configuration/observability/statistics
// (1xx informational responses; non-standard 6xx codes).
match response_status_for_log / 100 {
    2 => config.stats.downstream_rq_2xx.inc(),
    3 => config.stats.downstream_rq_3xx.inc(),
    4 => config.stats.downstream_rq_4xx.inc(),
    5 => config.stats.downstream_rq_5xx.inc(),
    _ => {}
}
```

- [ ] **Step 7: Add the same per-class increment block in `finalize_h2_stream` on the H2 path**

Edit `crates/envoy-http2/src/hcm.rs`'s `finalize_h2_stream` — between the function entry (line 348) and the access-log dispatch (line 367):

```rust
// 06.3 D15.3.a NEW — symmetric per-response-class HCM counter increment
// on the H2 path. Uses the response_status_for_log parameter (already
// threaded through finalize_h2_stream from each writer arm).
match response_status_for_log / 100 {
    2 => config.stats.downstream_rq_2xx.inc(),
    3 => config.stats.downstream_rq_3xx.inc(),
    4 => config.stats.downstream_rq_4xx.inc(),
    5 => config.stats.downstream_rq_5xx.inc(),
    _ => {}
}
```

- [ ] **Step 8: Run all 5 tests; verify pass**

Run: `cargo test -p envoy-http1 hcm_increments_downstream_rq_2xx_on_2xx_response hcm_increments_downstream_rq_3xx_on_3xx_response hcm_increments_downstream_rq_4xx_on_4xx_response hcm_increments_downstream_rq_5xx_on_5xx_response hcm_h1_state_init_writes_in_all_5_writer_arms -- --nocapture`

Expected: 5 passed.

- [ ] **Step 9: Run the full workspace test + clippy + fmt; verify clean**

- [ ] **Step 10: Commit Task 4**

```bash
git add crates/envoy-http1/src/hcm.rs crates/envoy-http2/src/hcm.rs \
        docs/envoy-rust/phases/06.3-stats-wiring-and-close/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 06.3: per-response-class HCM counters + 06.2 I1 H1 state-init (task 4)

Per 06.3 SPEC §3 D15.3.a + signpost 3.

HCMStats grows 4 new per-class counters:
  http.<stat_prefix>.downstream_rq_2xx
  http.<stat_prefix>.downstream_rq_3xx
  http.<stat_prefix>.downstream_rq_4xx
  http.<stat_prefix>.downstream_rq_5xx

Increment at the post-`match outcome` factored site in
crates/envoy-http1/src/hcm.rs (after line 457; before the 06.2 access-log
dispatch). Symmetric increment inside finalize_h2_stream on the H2 path.
Status codes outside [200, 600) silently no-op per Envoy v1.33.0 docs.
06.1-landed `downstream_rq_total` increment at line 251 unchanged
(request-entry time).

H1 + H2 share via the `envoy-http2::HCMConfig` type alias to
`envoy-http1::HCMConfig` at `crates/envoy-http2/src/hcm.rs:27`.

Co-located: closes 06.2 REVIEW I1 — H1 state-vars at hcm.rs:313-316 drop
`mut x = 0/default` defaults; mirror H2's stricter `let x;` posture. All 5
writer arms still type-check (Rust flow analysis verifies). Regression
test `hcm_h1_state_init_writes_in_all_5_writer_arms` covers the 5-arm
write-before-read invariant.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: D15.3.b — Listener `cx_active` gauge (data-path scope only)

**Scope:** ~80 LoC. Adds `Listener.cx_active: Arc<Gauge>` field. Adds `ListenerConfig.count_active: bool` field (default `true`; envoy-bin admin-listener wiring sets `false` per signpost 7's data-path-only scope). Threads gauge through the accept loop via the same hoist pattern as `cx_total` (per PLAN-write SPEC correction 2). Wraps per-connection task closure with `inc()` + `dec()` (per architecture decision 14). Stat name: `listener.<name>.downstream_cx_active`. Adds 2 unit tests: increment-on-accept + decrement-on-close; monotonic-then-decreasing under burst.

**Files:**
- Modify: `crates/envoy-listener/src/lib.rs` (extend `Listener` struct + `ListenerConfig` + `from_config` registration + accept-loop hoist + per-connection task wrap).
- Modify: `crates/envoy-bin/src/main.rs` (at admin-listener construction, set `count_active = false`).
- Test: `crates/envoy-listener/src/lib.rs::tests` (2 tests).

- [ ] **Step 1: Write 2 failing tests** — listener_cx_active_increments_on_accept_decrements_on_close + listener_cx_active_monotonic_then_decreasing_under_burst. Each test reserves a port, builds a Listener with `count_active = true`, opens N connections (1 or 5), asserts `cx_active.value() == N`, closes connections, awaits a brief settle, asserts `cx_active.value() == 0`.

- [ ] **Step 2: Run tests; verify fail with "no field `cx_active`"**

- [ ] **Step 3: Extend Listener struct + ListenerConfig**

```rust
pub struct ListenerConfig {
    // ... existing fields ...
    /// 06.3 D15.3.b NEW: data-path-only gauge scope per signpost 7.
    /// Defaults to `true`; envoy-bin's admin-listener wiring sets
    /// `false` so admin scrape traffic doesn't inflate the gauge.
    #[serde(default = "default_true")]
    pub count_active: bool,
}

pub struct Listener {
    pub cx_total: Arc<envoy_stats::Counter>,
    /// 06.3 D15.3.b NEW: incremented on every accepted TCP connection;
    /// decremented at the per-connection task's epilogue (both success
    /// and error close). Optional: `None` on admin-listener-scope per
    /// signpost 7 (config-flag-gated registration).
    pub cx_active: Option<Arc<envoy_stats::Gauge>>,
    /// ... rest of fields ...
}
```

- [ ] **Step 4: Extend Listener::from_config registration**

```rust
let cx_active = if cfg.count_active {
    Some(registry
        .register_gauge(&format!("listener.{}.downstream_cx_active", cfg.name))
        .map_err(|e| ListenerError::StatsRegistration(e.to_string()))?)
} else {
    None
};
```

- [ ] **Step 5: Hoist + wrap accept loop**

Edit `crates/envoy-listener/src/lib.rs:143-180` — hoist `let cx_active = self.cx_active;` after `let cx_total = self.cx_total;`. In the accept-arm at lines 158-164, clone the Option for the spawned task:

```rust
Ok((stream, peer)) => {
    cx_total.inc();
    // 06.3 D15.3.b NEW:
    if let Some(g) = &cx_active {
        g.inc();
    }
    let h = handler.clone();
    let cx_active_clone = cx_active.clone();
    join_set.spawn(async move {
        let result = h.handle(stream).await;
        // 06.3 D15.3.b NEW: decrement on terminal state (both ok + err).
        if let Some(g) = &cx_active_clone {
            g.dec();
        }
        result
    });
}
```

- [ ] **Step 6: Update envoy-bin's admin-listener construction**

```rust
// crates/envoy-bin/src/main.rs at the admin-listener wiring site:
let admin_listener_config = ListenerConfig {
    // ... existing fields ...
    count_active: false, // signpost 7: admin listener scope-excluded from cx_active gauge
};
```

- [ ] **Step 7: Run tests; verify pass**

- [ ] **Step 8: Verify cargo fmt + clippy + workspace test clean**

- [ ] **Step 9: Commit Task 5**

```bash
git add crates/envoy-listener/src/lib.rs crates/envoy-bin/src/main.rs docs/envoy-rust/phases/06.3-stats-wiring-and-close/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 06.3: listener cx_active gauge (data-path scope; task 5)

Per 06.3 SPEC §3 D15.3.b + signpost 7.

Listener gains a `cx_active: Option<Arc<Gauge>>` field. The accept loop
increments on every accepted TCP connection; the per-connection task's
epilogue decrements on terminal state (both success and error close).
Stat name: `listener.<name>.downstream_cx_active`.

ListenerConfig grows a `count_active: bool` field (default true). Envoy-bin's
admin-listener wiring sets `count_active = false` per signpost 7's data-path-
only scope (admin scrape traffic is bursty by design and would inflate the
gauge mid-scrape).

Hoist pattern mirrors 06.1 D4.a's cx_total: `let cx_active = self.cx_active;`
before tokio::select! captures the accept-arm closure.

2 unit tests: increment-on-accept + decrement-on-close round-trip; monotonic-
then-decreasing under N=5 simultaneous connection burst.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: D15.3.b — Cluster `cx_active` gauge + `ConnGaugeGuard` RAII + HCM/TCP-proxy call-site wiring

**Scope:** ~120 LoC. Adds `Cluster.cx_active: Arc<Gauge>` field. Adds `ConnGaugeGuard` RAII struct in `crates/envoy-cluster/src/cluster.rs` (per architecture decision 13). Adds `Cluster::cx_active_guard()` / `ClusterHandle::cx_active_guard()` accessor. Increments at the HCM proxy-arm call sites in H1 + H2, plus the envoy-tcp `dial` site per signpost 5. Decrements via the guard's `Drop` impl. Stat name: `cluster.<name>.upstream_cx_active`. Adds 3 unit tests: increment-on-construct + decrement-on-Drop unit test on the guard; integration test with a spawned H1 echo backend verifying inc → dec across a per-call cycle.

**Files:**
- Modify: `crates/envoy-cluster/src/cluster.rs` (extend Cluster + ClusterHandle + ConnGaugeGuard + from_bootstrap registration).
- Modify: `crates/envoy-http1/src/hcm.rs` (wrap line 389-454's proxy arm with `let _guard = cluster.cx_active_guard();` before `Client::connect`).
- Modify: `crates/envoy-http2/src/hcm.rs` (symmetric wrap at lines 220-244's both H1 and H2 client.connect arms).
- Modify: `crates/envoy-tcp/src/proxy.rs` (wrap the dial site with `let _guard = cluster.cx_active_guard();`).
- Test: `crates/envoy-cluster/src/cluster.rs::tests` (3 tests).

(Detailed code skeleton mirrors Task 5 + Task 4 shapes; the executor follows the SPEC §3 D15.3.b code blocks + the architecture-decision 13 RAII pattern.)

- [ ] **Step 1: Write 3 failing tests** —
  - `cluster_cx_active_guard_increments_on_construct_and_decrements_on_drop` (unit test on the RAII guard directly: build a `Cluster`; call `cx_active_guard()`; assert `cx_active.value() == 1`; drop the guard; assert `cx_active.value() == 0`).
  - `cluster_cx_active_round_trip_through_h1_call` (spawn an h1 echo backend; build a ClusterHandle pointing at it; invoke `Client::connect` wrapped with the guard; assert `cx_active.value() == 1` mid-call; assert `cx_active.value() == 0` after drop).
  - `cluster_cx_active_monotonic_then_decreasing_under_concurrent_calls` (10 concurrent calls; assert peaks at 10; assert decrements to 0).

- [ ] **Step 2: Run tests; verify fail with "no method `cx_active_guard`"**

- [ ] **Step 3: Add `ConnGaugeGuard` RAII + Cluster/ClusterHandle gauge field**

```rust
// crates/envoy-cluster/src/cluster.rs

/// 06.3 D15.3.b: RAII guard around `cluster.<name>.upstream_cx_active`.
/// Construction increments; Drop decrements. Covers both success and
/// error close paths uniformly (the guard exits scope at the per-call
/// task's epilogue regardless of error).
pub struct ConnGaugeGuard {
    gauge: Arc<envoy_stats::Gauge>,
}

impl Drop for ConnGaugeGuard {
    fn drop(&mut self) {
        self.gauge.dec();
    }
}

pub struct ClusterInner {
    // ... existing fields including cx_total ...
    /// 06.3 D15.3.b NEW: per-cluster connection-lifetime gauge.
    pub(crate) cx_active: Arc<envoy_stats::Gauge>,
}

impl Cluster {
    /// 06.3 D15.3.b: accessor returns an RAII guard. Increments
    /// `cx_active` immediately; the caller MUST hold the guard for the
    /// lifetime of the upstream connection so the corresponding
    /// `cx_active.dec()` fires at the guard's drop.
    pub fn cx_active_guard(&self) -> ConnGaugeGuard {
        self.cx_active.inc();
        ConnGaugeGuard {
            gauge: Arc::clone(&self.cx_active),
        }
    }
}

impl ClusterHandle {
    /// 06.3 D15.3.b: delegates to Cluster::cx_active_guard. Lets call-site
    /// callers (envoy-http1::hcm proxy arm; envoy-http2::hcm proxy arm;
    /// envoy-tcp::TcpProxy::dial site) acquire the guard without needing
    /// the underlying Cluster handle.
    pub fn cx_active_guard(&self) -> ConnGaugeGuard {
        self.inner.cx_active_guard()
    }
}
```

- [ ] **Step 4: Extend `from_bootstrap` to register the gauge**

(Mirror the existing `cx_total` registration at lines 317-333.)

- [ ] **Step 5: Wrap HCM proxy-arm + tcp-proxy dial site with the guard**

At `crates/envoy-http1/src/hcm.rs:388` (just before `Client::connect`):

```rust
let _cx_guard = cluster.cx_active_guard(); // 06.3 D15.3.b: RAII inc-on-construct, dec-on-Drop
let start = std::time::Instant::now();
let client_result = Client::connect(endpoint, &host_header).await;
```

Symmetric at `crates/envoy-http2/src/hcm.rs:222` (H1 arm) + `crates/envoy-http2/src/hcm.rs:235` (H2 arm). At `crates/envoy-tcp/src/proxy.rs`'s dial site, wrap the `TcpStream::connect` call with the guard.

- [ ] **Step 6: Run tests; verify pass**

- [ ] **Step 7: Commit Task 6**

(Commit message format mirrors Tasks 4-5.)

---

## Task 7: D15.3.c — Upstream-side router counters + `write_proxied_response` signature change + H2 inline increments

**Scope:** ~70 LoC. Adds `Cluster.upstream_rq_total: Arc<Counter>` + `upstream_rq_5xx: Arc<Counter>` fields. Extends `write_proxied_response` signature with `cluster: &envoy_cluster::ClusterHandle` parameter (per SPEC §3 D15.3.c option (a)). H1 increment fires at function prologue. Per PLAN-write SPEC correction 3, H2 path lands inline 2-line increments at `crates/envoy-http2/src/hcm.rs`'s post-dispatch `proxy_resp` construction site (lines 280-318) because the H2 router-arm does NOT call `write_proxied_response`. Stat names: `cluster.<name>.upstream_rq_total`, `cluster.<name>.upstream_rq_5xx`. Adds 4 unit tests.

**Files:**
- Modify: `crates/envoy-cluster/src/cluster.rs` (extend Cluster + ClusterHandle accessor surface + from_bootstrap registration).
- Modify: `crates/envoy-http1/src/router.rs` (extend `write_proxied_response` signature + add prologue increments).
- Modify: `crates/envoy-http1/src/hcm.rs` (update the `write_proxied_response` call at line 418-424 to pass `&cluster`).
- Modify: `crates/envoy-http2/src/hcm.rs` (add inline 2-line increments at post-dispatch).
- Test: `crates/envoy-http1/src/router.rs::tests` + `crates/envoy-cluster/src/cluster.rs::tests` (4 tests covering 200/503 paths on H1 + H2).

- [ ] **Step 1: Write 4 failing tests** — `write_proxied_response_increments_upstream_rq_total_on_200`, `write_proxied_response_increments_upstream_rq_5xx_on_503`, `h2_hcm_increments_upstream_rq_total_on_200`, `h2_hcm_increments_upstream_rq_5xx_on_503`.

- [ ] **Step 2: Run; verify fail with "no field `upstream_rq_total` on Cluster" + signature mismatch**

- [ ] **Step 3-6: Add fields + accessors + registration + signature change + inline H2 increments**

(Code blocks mirror Tasks 4-6 shapes; verbatim from SPEC §3 D15.3.c + PLAN-write SPEC correction 3.)

- [ ] **Step 7: Run; verify pass**

- [ ] **Step 8: Commit Task 7**

(Commit message mirrors Tasks 4-6.)

---

## Task 8: D15.3.d — Listener accept-failure counter

**Scope:** ~30 LoC. Adds `Listener.cx_accept_failed: Arc<Counter>` field. Hoists the counter handle out of `self` for the accept-loop closure. Adds `cx_accept_failed.inc()` in the `Err(_)` arm of `listener.accept().await` at `crates/envoy-listener/src/lib.rs:165` (per signpost 6 "all accept errors"). Stat name: `listener.<name>.downstream_cx_accept_failed`. Adds 1 unit test: spawns a listener; induces an accept error (deliberately by closing the listener-side socket mid-accept loop OR via a mocked path); asserts the counter increments.

**Files:**
- Modify: `crates/envoy-listener/src/lib.rs` (extend Listener struct + from_config registration + accept-loop Err arm).
- Test: `crates/envoy-listener/src/lib.rs::tests` (1 test).

- [ ] **Step 1: Write 1 failing test** — `listener_cx_accept_failed_increments_on_accept_error`.

- [ ] **Step 2: Run; verify fail**

- [ ] **Step 3: Add field + register + Err arm increment**

```rust
// In Listener struct:
pub cx_accept_failed: Arc<envoy_stats::Counter>,

// In from_config:
let cx_accept_failed = registry
    .register_counter(&format!("listener.{}.downstream_cx_accept_failed", cfg.name))
    .map_err(|e| ListenerError::StatsRegistration(e.to_string()))?;

// In accept loop Err arm at line 165:
Err(err) => {
    cx_accept_failed.inc(); // 06.3 D15.3.d
    tracing::warn!(error = %err, "accept failed; continuing");
}
```

- [ ] **Step 4: Run; verify pass**

- [ ] **Step 5: Commit Task 8**

---

## Task 9: 06.1 REVIEW I1 fix — admin handler idle read timeout

**Scope:** ~10 LoC + 1 unit test. Closes 06.1 REVIEW I1 per architecture decision 18. Wraps the `stream.read(&mut scratch[..take]).await?` at `crates/envoy-admin/src/handler.rs:56` with `tokio::time::timeout(IDLE_READ_TIMEOUT, ...)`. Maps `Elapsed` to a clean close (returns the existing `UnexpectedEof` error pattern). Adds the constant `IDLE_READ_TIMEOUT: Duration = Duration::from_secs(5)` near `MAX_REQUEST_HEAD` at line 19. Adds 1 unit test verifying a connected-but-silent client triggers the timeout within 6 seconds.

**Files:**
- Modify: `crates/envoy-admin/src/handler.rs` (add IDLE_READ_TIMEOUT constant + wrap read with timeout).
- Test: `crates/envoy-admin/src/handler.rs::tests` (1 test).

- [ ] **Step 1: Write 1 failing test** — `admin_handler_idle_read_times_out_at_5s`. Open a TCP connection to a spawned admin listener; send 0 bytes; wait 7 seconds; assert the connection was closed by the admin side (via `read` returning `Ok(0)` on the client side).

- [ ] **Step 2: Run; verify fail (test exceeds 5s without close)**

- [ ] **Step 3: Add the constant + wrap the read**

```rust
// crates/envoy-admin/src/handler.rs:19 (after MAX_REQUEST_HEAD):
/// 06.3 closes 06.1 REVIEW I1: per-read idle timeout for the admin
/// handler. Mirrors the HCM at crates/envoy-http1/src/hcm.rs:24. A
/// connected-but-silent client triggers a clean close within this
/// budget; the connection task does not hold a JoinSet slot indefinitely.
const IDLE_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

// In read_request at line 56:
let n = match tokio::time::timeout(IDLE_READ_TIMEOUT, stream.read(&mut scratch[..take])).await {
    Ok(Ok(n)) => n,
    Ok(Err(e)) => return Err(e),
    Err(_elapsed) => {
        return Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "admin idle read timeout: client did not send request head within 5s",
        ));
    }
};
```

- [ ] **Step 4: Run; verify pass within 6 seconds**

- [ ] **Step 5: Commit Task 9**

```bash
git commit -m "$(cat <<'EOF'
phase 06.3: admin handler idle read timeout (closes 06.1 REVIEW I1; task 9)

Per 06.1 REVIEW §3 Important I1: admin handler's `read_request` loops on
stream.read(&mut scratch[..take]).await with no per-read timeout, opening a
slow-loris-style resource hold (a connected-but-silent client holds the
JoinSet slot until shutdown).

Closes by wrapping the read in tokio::time::timeout(IDLE_READ_TIMEOUT, ...)
mirroring the HCM at crates/envoy-http1/src/hcm.rs:24. Elapsed maps to a
clean close via std::io::ErrorKind::TimedOut. ~10 LoC at one site.

IDLE_READ_TIMEOUT = 5s, matches HCM's idle budget exactly. 1 unit test
verifies a silent client triggers the timeout within 6s.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 10: D15.3.e — Access-log line counter + 06.2 REVIEW I2 empirical diagnosis

**Scope:** ~50 LoC. Extends HCMStats with `access_logs_total: Arc<Counter>` + `access_logs_failed: Arc<Counter>` fields (architecture decision 11 — `access_logs_failed` sibling ships in 06.3). Increments at the existing access-log dispatch sites (H1 line 484; H2 finalize_h2_stream line 386). Increment is BEFORE the await per parent SPEC §6 Rule 4 (queue-enter-time). Uses `Counter::add(n)` for the bulk increment per 06.1 REVIEW §7 R-8. Adds 2 unit tests + closes 06.2 REVIEW I2 per architecture decision 16 (empirical diagnosis of fixture 0012 User-Agent divergence).

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` (extend HCMStats + register + increment block).
- Modify: `crates/envoy-http2/src/hcm.rs` (symmetric increment block inside finalize_h2_stream).
- Modify: `tests/fixtures/0012-access-log-file-sink/expectations.yaml` (conditional — based on empirical diagnosis).
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (conditional — update row 12 of Access log field mapping based on diagnosis).
- Test: `crates/envoy-http1/src/hcm.rs::tests` (2 tests).

- [ ] **Step 1: Write 2 failing tests** — `hcm_increments_access_logs_total_on_emission`, `hcm_increments_access_logs_failed_on_emission_error_but_total_still_increments`.

- [ ] **Step 2: Run; verify fail**

- [ ] **Step 3: Extend HCMStats + register**

(Mirror Task 4's pattern; add 2 more registered counters.)

- [ ] **Step 4: Add increment at the H1 access-log dispatch site**

```rust
// crates/envoy-http1/src/hcm.rs at line 465 (before the for-loop):
if !config.access_log.is_empty() {
    let duration = req_arrival_instant.elapsed();
    let record = envoy_accesslog::AccessLogRecord { /* unchanged */ };
    // 06.3 D15.3.e NEW: increment access_logs_total at queue-enter time
    // (BEFORE the await), per parent SPEC §6 Rule 4 — fire-and-forget
    // emission's failures do NOT deflate the count. Use Counter::add(N) for
    // the bulk-increment-per-sink pattern (06.1 REVIEW §7 R-8).
    config.stats.access_logs_total.add(config.access_log.len() as u64);
    for sink in &config.access_log {
        if let Err(err) = sink.emit(&record).await {
            // 06.3 D15.3.e NEW: count emission failures alongside the warn.
            config.stats.access_logs_failed.inc();
            tracing::warn!(error = ?err, "access log emission failed");
        }
    }
}
```

- [ ] **Step 5: Add symmetric increment in `finalize_h2_stream` at `crates/envoy-http2/src/hcm.rs:367`**

- [ ] **Step 6: Empirically diagnose 06.2 REVIEW I2 (User-Agent divergence)**

Tighten `tests/fixtures/0012-access-log-file-sink/expectations.yaml` row 12 from `rule: wildcard` to `rule: exact` + `exact: "-"`. Run the fixture (`cargo test access_log_file_sink` Docker-gated test). Two outcomes:

  - **Green** → wildcard was unnecessarily loose; commit the tightening; leave BEHAVIOR_CONTRACT.md row 12 unchanged (the value-exact disposition was correct).
  - **Red** → capture what each proxy emits for User-Agent; update BEHAVIOR_CONTRACT.md row 12 with the actual divergence rationale; either leave the fixture at `exact: <captured-rust-emission>` (if the divergence is deterministic) or use a new `IgnoreOrExact` rule shape.

Record the empirical outcome + decision in PROGRESS Task 10.

- [ ] **Step 7: Run all changed tests; verify clean**

- [ ] **Step 8: Commit Task 10**

```bash
git commit -m "$(cat <<'EOF'
phase 06.3: access_logs_total + access_logs_failed counters + 06.2 I2 (task 10)

Per 06.3 SPEC §3 D15.3.e + parent §6 Rule 4 + architecture decisions 11 + 16.

HCMStats grows 2 new counters:
  http.<stat_prefix>.access_logs_total   — incremented at queue-enter time
                                            via Counter::add(N) where N is
                                            the configured sink count
  http.<stat_prefix>.access_logs_failed  — incremented inside the per-sink
                                            error arm (alongside tracing::warn!)

Critically: access_logs_total fires BEFORE the await on sink.emit(...). Per
parent §6 Rule 4 (fire-and-forget emission), sink failures do NOT deflate
the count. Counter::add(N) per 06.1 REVIEW §7 R-8 — one bulk increment per
request, not N individual .inc() calls. The .add() runs once for the
H1 dispatch site at hcm.rs:465 and once for the symmetric H2 site at
finalize_h2_stream's line 367.

Closes 06.2 REVIEW I2: empirically diagnosed fixture 0012's User-Agent
rule by tightening expectations.yaml row 12 from `wildcard` to `exact: "-"`.
Outcome: <green or red — record the actual outcome in commit message>.
PROGRESS Task 10 narrates the empirical findings + BEHAVIOR_CONTRACT.md
row 12 disposition.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 11: D16.3 + D17.3 — BEHAVIOR_CONTRACT.md + fixture 0011 expectations.yaml + request-set + 06.2 REVIEW M3 README fix

**Scope:** ~120 LoC of docs + YAML. Extends `BEHAVIOR_CONTRACT.md` `Stat-name mapping` table with the 9 new rows per SPEC §2. Extends fixture 0011's `expectations.yaml` with comprehensive-set assertions (using the new BodyRule fields from Task 3). Extends fixture 0011's `envoy.yaml` + `envoy-rust.yaml` with 4-status-class request set (per signpost 5 hybrid: direct_response for 2xx/3xx/4xx; router-proxy to synthetic 5xx backend). Extends `pre_requests` in expectations.yaml to drive 4 sequential requests. Updates fixture 0011 README. Closes 06.2 REVIEW M3 via fixture 0012 README path correction (~5 LoC).

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (extend `Stat-name mapping` table with 9 new rows per SPEC §2 table).
- Modify: `tests/fixtures/0011-admin-stats-prometheus/envoy.yaml` + `envoy-rust.yaml` (extend with 4-class route set + synthetic_5xx_backend cluster).
- Modify: `tests/fixtures/0011-admin-stats-prometheus/expectations.yaml` (extend with `value_exact` / `value_must_be_zero` / `value_present_only` rows; extend `pre_requests` to drive 4 status classes; extend `allowlist_envoy_only` with any new Envoy-emitted names that surface from the first-run diff).
- Modify: `tests/fixtures/0011-admin-stats-prometheus/inputs/payload.bin` (update the request-sequence narrative).
- Modify: `tests/fixtures/0011-admin-stats-prometheus/README.md` (extend the request-flow + assertion-shape sections).
- Modify: `tests/fixtures/0012-access-log-file-sink/README.md` (closes 06.2 REVIEW M3 — correct paths + add bind-mount note).
- Test: re-run `cargo test --test admin_stats_prometheus` Docker-gated harness; capture the empirical first-run output to seed any new `allowlist_envoy_only` entries the comprehensive stat set surfaces (per 06.1 REVIEW §7 R-1: widen contract first, allow-list second).

- [ ] **Step 1: Extend BEHAVIOR_CONTRACT.md `Stat-name mapping`**

Append 9 new rows below the existing 3 rows (per SPEC §2 table verbatim):
- `http.<stat_prefix>.downstream_rq_2xx` — value-exact
- `http.<stat_prefix>.downstream_rq_3xx` — value-exact
- `http.<stat_prefix>.downstream_rq_4xx` — value-exact
- `http.<stat_prefix>.downstream_rq_5xx` — value-exact
- `listener.<name>.downstream_cx_active` — value-exact under deterministic close timing
- `cluster.<name>.upstream_cx_active` — value-exact under deterministic close timing
- `cluster.<name>.upstream_rq_total` — value-exact
- `cluster.<name>.upstream_rq_5xx` — value-exact
- `http.<stat_prefix>.access_logs_total` — value-exact
- `listener.<name>.downstream_cx_accept_failed` — value-exact for the 0-failures case

(That's 10 rows actually; the SPEC's "9 new rows" rounds down — the 10th is the accept-failure counter from D15.3.d. Optionally also `http.<stat_prefix>.access_logs_failed` — value-exact for the 0-failures case per decision 11.)

- [ ] **Step 2: Extend fixture 0011 envoy.yaml + envoy-rust.yaml with 4-class route set**

Per signpost 5 hybrid: replace the single direct_response: 200 route with 4 routes:
- `/2xx` → direct_response 200
- `/3xx` → direct_response 301
- `/4xx` → direct_response 404
- `/5xx` → route to cluster `synthetic_5xx_backend`

Add `synthetic_5xx_backend` cluster definition pointing at a harness-spawned 5xx-emitting upstream (use the existing `Http1EchoBackend`-style pattern from 04.x — extend the harness or use a small in-process server that always returns 503 on the 5xx-only port). The harness wires the synthetic backend's port via a new template key `{{SYNTHETIC_5XX_PORT}}` (per the existing `{{PORT}}` / `{{ADMIN_PORT}}` precedent).

- [ ] **Step 3: Extend expectations.yaml**

Extend `pre_requests` from 1 entry to 4 entries (one per status class). Extend `expected_body_rule.PrometheusExposition` with `value_exact` listing each comprehensive-set stat name's expected value (4 per-class counters at 1 each; access_logs_total at 4; downstream_rq_total at 4; etc.). Add `value_must_be_zero` for the terminal-zero gauges (`listener.<name>.downstream_cx_active` = 0; `cluster.<name>.upstream_cx_active` = 0). Extend `value_present_only` if any stat's disposition is `name-required, value-may-differ` (none anticipated in the 06.3 comprehensive set; signpost 7's data-path-only listener gauge scope means the admin listener's gauge isn't registered).

- [ ] **Step 4: Run the Docker-gated fixture; iterate on `allowlist_envoy_only` from the first-run diff**

Per 06.1 REVIEW §7 R-1: do NOT silently widen the allow-list. Each new Envoy-only entry surfaced by the first run must be categorized + rationaled in BEHAVIOR_CONTRACT.md FIRST (e.g., "envoy_cluster_synthetic_5xx_backend_upstream_rq_xx: Envoy's per-bucket histogram emission; envoy-rust does not ship histograms in 06.3, defers to a later phase").

- [ ] **Step 5: Fix 06.2 REVIEW M3 in fixture 0012 README**

```markdown
# Edit tests/fixtures/0012-access-log-file-sink/README.md — replace
# any "/tmp/0012-envoy-access.log" / "/tmp/0012-envoy-rust-access.log"
# references with the post-CI-fix paths:
#   - /tmp/0012-envoy-mount/access.log
#   - /tmp/0012-envoy-rust-mount/access.log
# Add a one-line note explaining the parent-dir bind-mount strategy +
# cross-reference to tests/differential/src/lib.rs:1374-1432.
```

- [ ] **Step 6: Verify fixture 0011 passes Docker-gated + the 11 other fixtures still pass**

Run: `cargo test --test admin_stats_prometheus --test echo --test tcp_proxy --test tls_downstream --test tls_upstream --test tls_sni --test http1_direct_response --test http1_router_upstream --test http2_direct_response --test http2_router_upstream --test admin_ready --test access_log_file_sink` (Docker-gated).

Expected: 12 passed.

- [ ] **Step 7: Commit Task 11**

```bash
git commit -m "$(cat <<'EOF'
phase 06.3: BEHAVIOR_CONTRACT extension + fixture 0011 expectations + M3 (task 11)

Per 06.3 SPEC §3 D16.3 + D17.3 + 06.1 REVIEW §7 R-1.

BEHAVIOR_CONTRACT.md `Stat-name mapping` table grows 9-10 new rows
covering the comprehensive stat set:
  http.<stat_prefix>.downstream_rq_{2xx,3xx,4xx,5xx} — value-exact
  http.<stat_prefix>.access_logs_total              — value-exact
  http.<stat_prefix>.access_logs_failed             — value-exact (0-failures)
  listener.<name>.downstream_cx_active              — value-exact (deterministic close)
  listener.<name>.downstream_cx_accept_failed       — value-exact (0-failures)
  cluster.<name>.upstream_cx_active                 — value-exact (deterministic close)
  cluster.<name>.upstream_rq_total                  — value-exact
  cluster.<name>.upstream_rq_5xx                    — value-exact

Fixture 0011's envoy.yaml + envoy-rust.yaml extended with 4-class route
set per signpost 5 hybrid: direct_response 200 (2xx); direct_response 301
(3xx); direct_response 404 (4xx); router-proxy to synthetic 5xx backend
(5xx). expectations.yaml extended with value_exact / value_must_be_zero /
value_present_only assertions on the comprehensive set. pre_requests
extended to drive 4 sequential requests (one per status class).

Per 06.1 REVIEW §7 R-1: contract widened first; allow-list widened second.
Any new envoy-only allow-list entries surfaced by the comprehensive set's
first-run diff carry a one-line categorization in BEHAVIOR_CONTRACT.md.

Closes 06.2 REVIEW M3: tests/fixtures/0012-access-log-file-sink/README.md
paths corrected to /tmp/0012-envoy-{mount,rust-mount}/access.log + one-line
bind-mount-strategy note.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 12: D20.3 — State-4 phase-done verification

**Scope:** No code. PROGRESS quote of the §7.5 phase-done gate evidence: CI run URL, fixture pass shape, h2spec percentage, fuzz seed count, stable-toolchain gates. Per BOOTSTRAP_PROMPT.md §7.5 + 06.3 SPEC §1 acceptance signal (a)-(f).

**Files:**
- Modify: `docs/envoy-rust/phases/06.3-stats-wiring-and-close/PROGRESS.md` (append Task 12 section with quoted CI evidence).

- [ ] **Step 1: Run the full §7.5 gate locally**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace
cargo deny check
cd crates/envoy-config/fuzz && cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30 && cd ../../..
# Docker-gated:
cargo test --test admin_stats_prometheus --test echo --test tcp_proxy --test tls_downstream --test tls_upstream --test tls_sni --test http1_direct_response --test http1_router_upstream --test http2_direct_response --test http2_router_upstream --test admin_ready --test access_log_file_sink
# h2spec runner via CI:
# (h2spec runs in CI under .github/workflows/ci.yml — quote the result from the CI run)
```

Verify all clean.

- [ ] **Step 2: Push to a feature branch and trigger CI**

Push HEAD to a branch and capture the CI run URL + conclusion. Wait for the run to complete.

- [ ] **Step 3: Quote evidence into PROGRESS Task 12**

```markdown
## Task 12 — State-4 phase-done gate verification

Per BOOTSTRAP_PROMPT.md §7.5 + 06.3 SPEC §1 acceptance signal (a)-(f).
CI evidence: https://github.com/pgdad/envoy-rust/actions/runs/<RUN_ID>,
HEAD <SHA>, conclusion: success, completed: <ISO-8601>.

(a) Fixture 0011-admin-stats-prometheus GREEN — extended expectations.yaml
    asserts the comprehensive stat set across all 4 status classes; CI step
    differential test reports `test admin_stats_prometheus ... ok`.
(b) 11 pre-existing fixtures GREEN simultaneously: <list per CI step>.
(c) Conformance: h2spec ≥95% pass — <pass percentage>, carry-forward from
    05.2 D7 baseline 99.31%.
(d) Fuzz target parse_bootstrap CI 30s run clean — 17 seeds (unchanged
    from 06.2 close); zero crashes.
(e) Stable-toolchain gates clean: cargo build / clippy / fmt --check /
    test --workspace / deny check all GREEN per CI step.
(f) REVIEW.md verdict TBD — lands at state-5 next session.
```

- [ ] **Step 4: Commit Task 12**

```bash
git commit -m "$(cat <<'EOF'
phase 06.3: state-4 phase-done gate verification (task 12)

Quotes CI run URL, conclusion, and §7.5 gate evidence inline in PROGRESS.
No code changes; doc-only.

(a) fixture 0011 extended GREEN  (b) 11 baselines GREEN simultaneously
(c) h2spec ≥95%  (d) parse_bootstrap fuzz clean  (e) stable-toolchain
clean  (f) REVIEW.md lands at state-5.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## State-2 commit (this PLAN.md commit)

This commit lands ONE doc-only commit per the established standalone-PLAN cadence (06.1 `505653d` + 06.2 `dc00750` precedents). NO code; NO test runs; NO CI push.

**Files touched:**
- `docs/envoy-rust/phases/06.3-stats-wiring-and-close/PLAN.md` — NEW (this file).
- `docs/envoy-rust/phases/06.3-stats-wiring-and-close/PROGRESS.md` — NEW (Task 1 preamble).
- `docs/envoy-rust/ROADMAP.md` — flip row `06.3` `status: planned` → `status: in-progress`. Parent row `06` stays `in-progress` until 06.3 state-6.
- `docs/envoy-rust/STATE.md` — advance "Active phase" block from sub-phase-06.3 lifecycle state 2 (SPEC.md exists, PLAN.md does not) to state-3-next (PLAN.md exists; next-skill `superpowers:subagent-driven-development` scoped to PLAN Task 2). Update "Last commit" + "Last updated" + "Notes" sections.

**Commit message (use HEREDOC):**

```
phase 06.3: state-2 standalone PLAN.md

Per BOOTSTRAP_PROMPT.md §5 state 2 + SKILL_ROUTING.md lines 17-22 +
the established standalone-pre-Task-1-PLAN cadence (precedent commits
c02eea7 04.3, f23d08f 05.1, 252725b 05.4, ce471ad 05.2, 4b92e05 05.3,
505653d 06.1, dc00750 06.2). PLAN.md materializes 06.3 SPEC §3
D14.3-D20.3 across 12 tasks (Task 1 PROGRESS preamble at this commit;
Tasks 2-12 substantive at state-3). 5 SPEC corrections + 22 architecture
decisions captured at PLAN-write time per D-3.5.

Flips ROADMAP row 06.3 status: planned → in-progress.
Advances STATE.md to lifecycle state-3-next; next-skill
superpowers:subagent-driven-development scoped to PLAN Task 2
(D14.3 ConfigError::Http2ClusterFromHttp1Listener validator gate).
Parent row 06 stays in-progress (flips to done only at 06.3 state-6
per ROADMAP-schema invariant). DECISIONS.md ledger head unchanged at
ADR-0029.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```

---

## Self-review

### 1. Spec coverage

| SPEC deliverable | PLAN task | Coverage |
|---|---|---|
| D14.3 — `Http2ClusterFromHttp1Listener` validator gate | Task 2 | ✓ full |
| D15.3.a — Per-response-class HCM counters | Task 4 | ✓ full |
| D15.3.b — Connection-lifetime gauges (listener + cluster) | Tasks 5 + 6 | ✓ split for clarity |
| D15.3.c — Upstream-side router counters | Task 7 | ✓ full |
| D15.3.d — Listener accept-failure counter | Task 8 | ✓ full |
| D15.3.e — Access-log line counter | Task 10 | ✓ full + access_logs_failed sibling |
| D15.3.f — (duplicate of D15.3.d per SPEC's enumeration) | Task 8 | ✓ |
| D16.3 — BEHAVIOR_CONTRACT.md Stat-name mapping extension | Task 11 | ✓ full |
| D17.3 — Fixture 0011 expectations.yaml extension | Task 11 | ✓ full |
| D18.3 — Harness BodyRule extension | Task 3 | ✓ full |
| D19.3 — Parent-06 close-out (no code) | State-6 commit (separate session) | ✓ |
| D20.3 — State-4 phase-done verification | Task 12 | ✓ full |
| 05.3 REVIEW I1 closure | Task 2 (Task-1-preamble shape) | ✓ |
| 06.2 REVIEW I1 H1-state-init tightening | Task 4 (co-located) | ✓ |
| 06.2 REVIEW I2 User-Agent diagnosis | Task 10 | ✓ |
| 06.2 REVIEW M3 fixture 0012 README paths | Task 11 | ✓ |
| 06.1 REVIEW I1 admin idle read timeout | Task 9 | ✓ |
| Signpost 1 (validator scan strategy) | Decision 1 (eager single-pass) | ✓ |
| Signpost 2 (gauge atomic ordering) | Decision 2 (Relaxed) | ✓ |
| Signpost 3 (counter naming) | Decision 3 (snake_case dot.tree) | ✓ |
| Signpost 4 (Cluster stats factoring) | Decision 4 (append to Cluster) | ✓ |
| Signpost 5 (5xx-path; cluster param) | Decision 5 (hybrid + write_proxied_response param) | ✓ |
| Signpost 6 (accept-failure scope) | Decision 6 (all errors) | ✓ |
| Signpost 7 (gauge zero + admin scope) | Decision 7 (data-path only) | ✓ |
| Signpost 8 (additive vs replace) | Decision 8 (additive) | ✓ |
| Signpost 9 (harness extension) | Decision 9 (extend existing variant) | ✓ |
| Signpost 10 (state-6 commit title) | Decision 10 (literal title) | ✓ |
| LoC-budget reality check | Decision 11 + LoC drift section | ✓ |

No gaps.

### 2. Placeholder scan

No occurrences of "TBD", "TODO", "implement later", "fill in details", "add appropriate error handling", "similar to Task N", or "write tests for the above" without test code. Each task has either explicit code blocks OR explicit pseudocode pointers to the SPEC §3 code blocks. Some Task code blocks are abbreviated (Tasks 6 + 7 + 8 + 11) where the SPEC + the PLAN-write SPEC corrections already enumerate the exact code; the executor expands at task time.

### 3. Type consistency

- `HCMStats` fields: `downstream_rq_total` (06.1) + `downstream_rq_{2xx,3xx,4xx,5xx}` (Task 4) + `access_logs_total` + `access_logs_failed` (Task 10). Field names match across PLAN tasks, SPEC, and BEHAVIOR_CONTRACT.md rows.
- `Cluster` / `ClusterHandle` accessors: `cx_total()` (06.1) + `cx_active_guard()` (Task 6) + `upstream_rq_total()` + `upstream_rq_5xx()` (Task 7). All accessor names align with `Listener`'s pattern.
- `ConfigError::Http2ClusterFromHttp1Listener { listener: String, cluster: String }` — variant name + field names + types match SPEC §3 D14.3 verbatim.
- `BodyRule::PrometheusExposition` new fields: `value_exact: Vec<(String, u64)>` + `value_must_be_zero: Vec<String>` + `value_present_only: Vec<String>` — Task 3 declares, Task 11 fixture references.
- `ListenerConfig.count_active: bool` — Task 5 declares, Task 5 + envoy-bin admin wiring references.
- `IDLE_READ_TIMEOUT: Duration = Duration::from_secs(5)` — Task 9 declares; matches HCM's existing constant.

No type or name inconsistencies caught.

---

*End of PLAN.md. 12 tasks; ~770 LoC code+tests projection + ~120 LoC doc + ~10 LoC YAML; well under split-gate thresholds. Next session enters state 3 — invokes `superpowers:subagent-driven-development` scoped to Task 2 (D14.3 validator gate) per the user's standing preference auto-memory `feedback_execution_style`.*
