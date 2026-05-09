# Phase 06.3 — Comprehensive stats wiring + 05.3 REVIEW I1 closure + parent-06 close-out

- **Phase id:** `06.3`
- **Parent phase:** `06-observability` (split per **ADR-0029**; parent SPEC at `docs/envoy-rust/phases/06-observability/SPEC.md`, committed at parent-06 state-1).
- **Slug:** `06.3-stats-wiring-and-close`
- **Title:** Land the comprehensive Envoy stat tree at HCM/router/listener/cluster — extending 06.1's representative-subset wiring (3 counters: `listener.<name>.downstream_cx_total`, `cluster.<name>.upstream_cx_total`, `http.<stat_prefix>.downstream_rq_total`) with per-response-class HCM counters, connection-lifetime gauges, upstream-side router counters, an access-log line counter, and a listener accept-failure counter. Closes 05.3 REVIEW I1 (silent H1-listener × H2-cluster misnegotiation per ADR-0028 option-B deferral) as a Task-1 preamble via a new `ConfigError::Http2ClusterFromHttp1Listener` parse-time validator gate at `crates/envoy-config/src/bootstrap.rs::validate`. Extends BEHAVIOR_CONTRACT.md `Stat-name mapping` with one row per new stat. Extends fixture 0011's `expectations.yaml` to assert the comprehensive set. **Parent-06 close-out**: this sub-phase's state-6 commit ALSO flips parent ROADMAP row `06` from `in-progress` to `done` per the ROADMAP-schema invariant, mirroring phase-04's `e626862`-shape and phase-05's `82c26b8`-shape close-outs.
- **Depends on:** `06.2` (sub-phase ROADMAP row `done` after 06.2's state-6 phase-done commit; the `envoy-accesslog` foundation crate landed in 06.2 D8.2 ships `AccessLogRecord` + `FileSink` + `default_format::format`, and 06.3's `http.<stat_prefix>.access_logs_total` counter increments at the HCM access-log dispatch site landed in 06.2 D10.2). Transitively depends on `06.1` (the `envoy-stats` registry + `Counter`/`Gauge` primitives + the representative-subset wiring at listener/cluster/HCM landed in 06.1 D1.1–D4.1; 06.3 extends each consumer's stats namespace without touching the registry surface). Strictly the **closing sub-phase** of parent phase 06 — its state-6 phase-done commit ALSO flips parent ROADMAP row `06` `in-progress` → `done` per the ROADMAP-schema invariant ("the parent flips to `done` only after all sub-phases are `done`"; mirrors phase-04's `e626862`-shape close-out where the 04.3 commit closed parent 04, and phase-05's `82c26b8`-shape close-out where the 05.3 commit closed parent 05).
- **Differential surface when done:**
  - **Fixtures unchanged in 06.3 (must remain green at 06.3 state-4):** `tests/fixtures/0001-tcp-echo/`, `0002-static-admin-ready/`, `0003-tcp-proxy/`, `0004-tls-downstream/`, `0005-tls-upstream/`, `0006-tls-sni/`, `0007-http1-direct-response/`, `0008-http1-router-upstream/`, `0009-http2-direct-response/`, `0010-http2-router-upstream/`, `0012-access-log-file-sink/` — all 11 stay green at the Docker-gated CI level. Fixture `0002-static-admin-ready` continues to exercise `/ready` against the HCM-backed admin listener landed in 06.1 D3.1; fixture `0012-access-log-file-sink` continues to exercise the HCM access-log dispatch landed in 06.2 D10.2.
  - **Fixture extended in 06.3:** `tests/fixtures/0011-admin-stats-prometheus/expectations.yaml` extends from 06.1's representative-subset assertion (3 counters) to the comprehensive set (the per-response-class HCM counters, the connection-lifetime gauges, the upstream-side router counters, the access-log line counter, the listener accept-failure counter). Fixture YAML files (`envoy.yaml`, `envoy-rust.yaml`) and the Docker-gated test wrapper at `tests/differential/tests/admin_stats_prometheus.rs` are unchanged.
  - **No new fixtures.** Phase 06's 2 new fixtures (`0011-admin-stats-prometheus/` from 06.1; `0012-access-log-file-sink/` from 06.2) are the parent-06 differential surface in full; 06.3 widens fixture 0011's assertion only.
  - **Conformance suite unchanged in 06.3:** `tests/conformance/h2spec/` continues at the **≥95% pass** gate landed in 05.2 D7. 06.3 does not edit the runner or the gate; the comprehensive-stats-wiring work does not touch the H2 framing surface. The 06.3 state-4 verification re-runs h2spec to confirm no regression.
- **Seeded by:** parent-06 SPEC §1 layer 3 (the goal-paragraph for sub-phase 06.3 — "Comprehensive stats wiring + parent-06 close"), §3 D14.3–D20.3 (the seven 06.3 deliverables), §4 (non-goals — the 06.3-binding subset, especially the histograms / labels / additional admin endpoints / graceful drain / connection-pooling-stat-refinement deferrals), §5 (3-way split decision context — the rationale for placing the comprehensive-wiring + parent-close coupling in its own sub-phase as cleanup after 06.1's stats foundation and 06.2's access-log foundation), §6 cross-sub-phase architectural invariants (consumers register and increment; envoy-stats exports primitives only; counter/gauge ops lock-free; representative subset → comprehensive; access-log line counter increments at queue-enter time; parent close at THIS sub-phase's state-6 commit), §7 (no new ADRs projected at 06.3 state-2 — ADR-0029 already landed at parent-06 state-2; conditional ADR-0030/0031 stay available), §8 (parent-06 artifact list, scoped to 06.3's slice + the parent ROADMAP-row flip), §9 (parent-06 final commit message format — the `[parent 06 done]` tag attaches to the 06.3 state-6 commit's title).

This SPEC is the design contract for sub-phase 06.3. The next session converts it into `PLAN.md` per the phase lifecycle (§5 of `BOOTSTRAP_PROMPT.md` / `SKILL_ROUTING.md`). It is self-contained per doctrine D-3.4; a stranger reading only this file plus the stable doctrine documents (`MISSION.md`, `BEHAVIOR_CONTRACT.md`, `DECISIONS.md`, `BOOTSTRAP_PROMPT.md`) and the landed phase-05 + phase-06.1 + phase-06.2 surface (via `git log` and the in-tree `envoy-{accesslog,admin,bin,cluster,config,http1,http2,listener,stats,tcp,tls}` shape at the post-06.2 close HEAD) must be able to execute it without consulting the parent `06-observability/SPEC.md`. The 05.3 REVIEW I1 carryforward is reproduced verbatim below (§5) for that reason.

---

## 1. Goal and acceptance signal

**Goal.** Land the comprehensive Envoy stat tree at HCM / router / listener / cluster sites and close parent phase 06 in seven coordinated layers that all ship in this single sub-phase:

1. **05.3 REVIEW I1 closure preamble (Task 1).** A new `ConfigError::Http2ClusterFromHttp1Listener { listener: String, cluster: String }` variant fires when the per-listener cluster-reachability scan at `crates/envoy-config/src/bootstrap.rs::validate` finds a route that targets an H2-cluster (a cluster whose `typed_extension_protocol_options.HttpProtocolOptions.explicit_http_config.http2_protocol_options` is set, per the schema landed at 05.3 D2.a) reachable from an H1-listener (`codec_type: HTTP1`) or an AUTO-listener (`codec_type: AUTO`, which behaves as H1-only per parent-05 SPEC §4 and parent-06 SPEC §6 architectural inheritance). The parse-time gate substantively closes the silent-misnegotiation window described in 05.3 REVIEW I1: ADR-0028's option-B deferred the H1-listener H2-arm dispatch to avoid the `envoy-http1` ↔ `envoy-http2` cycle; the deferral is correct doctrine, but the deferred path must be visibly rejected at config-load time so operators don't get a confusing 502 (or worse, a silently-misnegotiated H1-on-the-wire to an H2-only backend) at runtime. Mechanical: ~50 LoC schema + ~30 LoC validator + 5 unit tests (positive H1-listener × H1-cluster passes; positive H2-listener × H2-cluster passes; positive H2-listener × H1-cluster passes; negative H1-listener × H2-cluster rejects with the new variant; negative AUTO-listener × H2-cluster rejects with the new variant). Mirrors phase-05.1 Task 1's posture toward phase-02.1 REVIEW I3 — a previously-identified gap closed cheaply at the start of an unrelated phase, before the phase's substantive surface lands.

2. **Comprehensive HCM per-response-class counters.** `crates/envoy-http1/src/hcm.rs` and `crates/envoy-http2/src/hcm.rs` (the listener-side HCM constructors landed at 04.1 D2 and 05.2 D3; `envoy-http2::HCMConfig` is a type alias to `envoy-http1::HCMConfig` per 05.2 SPEC §3, so wiring lands once and applies to both surfaces) gain per-response-class counters: `http.<stat_prefix>.downstream_rq_2xx`, `..._3xx`, `..._4xx`, `..._5xx`. The HCM increments the per-class counter at on-response-complete time (the existing `write_response` call site landed at 04.1, where 06.1 D4.1 already increments `downstream_rq_total`); 06.3 adds the per-class increment computed via `response.status / 100 → bucket-index in [2..=5] → counter increment`. Status codes outside `[200, 600)` (e.g., 1xx informational responses, which envoy-rust does not currently emit; or non-standard 6xx codes which neither proxy emits) are silently ignored — they do not increment any class counter, matching Envoy's documented behavior at https://www.envoyproxy.io/docs/envoy/v1.33.0/configuration/observability/statistics#http-connection-manager-stats. The 06.1 D4.1 `downstream_rq_total` counter continues to increment unconditionally on every request.

3. **Comprehensive connection-lifetime gauges.** `crates/envoy-listener/src/listener.rs` (the listener accept loop landed at 02.2; 06.1 D4.1 already wires `Listener.cx_total: Arc<Counter>`) gains a `Listener.cx_active: Arc<Gauge>` gauge. The accept loop increments the gauge on every accepted TCP connection (`gauge.inc()`); the per-connection task decrements the gauge on its terminal `ConnectionHandler` outcome (`gauge.dec()`) — both successful close and error close are counted as closure. The decrement site is the per-connection task's epilogue, immediately after the spawned task returns from its `handle` invocation (or its `handle_with_tls` invocation for TLS-bearing listeners landed at 03.1; the decrement runs in either branch). Symmetric to the listener side: `crates/envoy-cluster/src/cluster.rs` (the cluster `pick_endpoint` site landed at 02.1; 06.1 D4.1 already wires `Cluster.cx_total: Arc<Counter>`) gains a `Cluster.cx_active: Arc<Gauge>` gauge incremented at upstream-connect time (the `Client::connect` call sites at `crates/envoy-http1/src/client.rs` and `crates/envoy-http2/src/client.rs`, plus the TCP-proxy `dial` site at `crates/envoy-tcp/src/proxy.rs` if reached by 06.3's wiring scan — see signpost 5 below) and decremented at upstream-close time. Gauge ops are lock-free per parent SPEC §6 invariant 6 (`Gauge::inc`/`Gauge::dec` are `AtomicI64::fetch_add(±1, Ordering::Relaxed)`).

4. **Comprehensive upstream-side router counters.** `crates/envoy-http1/src/router.rs` (the proxy-arm completion site landed at 04.3; the `write_proxied_response` helper is the natural increment site since it owns the upstream response after `Client::send_request` returns) gains two counters per cluster: `cluster.<name>.upstream_rq_total` and `cluster.<name>.upstream_rq_5xx`. The first increments unconditionally on every router-proxy completion; the second increments only when the upstream `response.status / 100 == 5`. Both increment regardless of upstream protocol (H1 or H2; the 05.3 D4 router H2-arm dispatches polymorphically on `cluster.upstream_protocol`, so the increment site sits *after* the dispatch arm, in the protocol-agnostic `write_proxied_response` helper, which sees the unified `envoy_http1::codec::Response` value type per parent-05 SPEC §3 cross-sub-phase architectural rule 2). The 5xx counter excludes 502s synthesized by the router itself on `Client::connect` failure (per 04.3's 502 fallback shape) — that path is covered by the `upstream_cx_active`-decrement site's tracing+counter, not by `upstream_rq_5xx`, because Envoy's `upstream_rq_5xx` semantics are *"upstream returned a 5xx"* not *"router synthesized a 5xx"*. Fixture 0011's `expectations.yaml` includes a 5xx-path scenario via a synthetic backend fault (see §3 D17.3 below).

5. **Access-log line counter.** `crates/envoy-http1/src/hcm.rs` (the HCM access-log dispatch site landed at 06.2 D10.2) gains an `http.<stat_prefix>.access_logs_total` counter incremented at queue-enter time — not at sink-emission-success time. Per parent SPEC §6 Rule 4 (access-log emission is fire-and-forget) and parent SPEC §6 Rule "access-log line counter increments at queue-enter time so emission failures don't deflate the count": the counter increments *before* the HCM dispatches the record to the configured sinks, so a `tracing::warn!`-logged sink-emission failure does not silently deflate the counter. Operators reading the counter see the request's intent-to-emit-access-log, not the success of emission; sink failure shows up in tracing logs (and, if scope permits in 06.3 per parent SPEC §3 D15.3's note "a `..._access_logs_failed` counter lands in 06.3 if scope permits", a sibling `http.<stat_prefix>.access_logs_failed` counter — see §3 D15.3 below for the recommended posture). The counter only increments when the HCM has at least one configured sink (i.e., `HCMConfig.access_log` is non-empty); HCMs configured without `access_log:` do not increment.

6. **Listener accept-failure counter.** `crates/envoy-listener/src/listener.rs`'s accept loop gains a `Listener.cx_accept_failed: Arc<Counter>` counter incremented when `TcpListener::accept().await` returns `Err(_)`. The kernel-level errors that surface here are typically ECONNRESET (the peer reset the connection mid-3WHS), EMFILE (per-process file descriptor exhaustion), and ENFILE (system-wide file descriptor exhaustion); per-error-kind disambiguation defers to whichever phase first needs it (Envoy emits a single `downstream_cx_accept_failed` counter without per-errno breakdown, matching this scope). The increment-on-Err shape is uniform — every accept error counts; per signpost 6 below the planner picks at PLAN time whether to scope the counter to ECONNRESET-only (the most common transient error during connection floods) or all accept errors (the recommended posture, matching Envoy's documented behavior).

7. **BEHAVIOR_CONTRACT.md `Stat-name mapping` extension + fixture 0011 `expectations.yaml` extension + parent-06 close-out wiring.** Per §2 below the `Stat-name mapping` table grows one row per new stat in D15.3 (~7 new rows: per-response-class counters as a single "value-exact" row template; connection-lifetime gauges as "value-exact under deterministic close timing" or "name-required, value-may-differ" if the harness's burst-shape exposes timing-sensitivity — discussed in §2 below; upstream-side router counters as "value-exact"; access-log line counter as "value-exact"; listener accept-failure counter as "value-exact for the 0-failures case"). Fixture 0011's `expectations.yaml` extends from 06.1's representative-subset assertion (3 counters present, value-exact) to the comprehensive set (the new 7 rows asserted per their disposition). Parent-06 close-out wiring at the 06.3 state-6 commit flips ROADMAP row `06` from `in-progress` to `done` per the ROADMAP-schema invariant; STATE.md advances to phase 07 lifecycle state 1 (next-skill `superpowers:brainstorming` scoped to phase 07's `BOOTSTRAP_PROMPT.md` §8 row-07 charter — *"Filter chain framework: iteration protocol, per-route config, extension registry"*).

**Cross-sub-phase architectural invariants reiterated from parent SPEC §6** — these rules hold across all three sub-phases of parent phase 06; sub-phase 06.3 inherits them verbatim and they are load-bearing throughout the comprehensive-wiring slice:

- **Rule (consumers register and increment; envoy-stats exports primitives only)** — `envoy-stats` does NOT know about HCM, listeners, clusters, or admin endpoints. Stats wiring lives at the consumer side (`envoy-listener::Listener` registers and increments its own counters; `envoy-cluster::Cluster` registers and increments its own counters; `envoy-http1::HCM` registers and increments per-stat-prefix HCM counters). 06.3's comprehensive wiring extends each consumer's namespace without touching `envoy-stats`'s public surface (the registry's `register_counter` / `register_gauge` API landed at 06.1 D1.1 is sufficient).
- **Rule (counter Counter::inc() lock-free; gauge Gauge::set/inc/dec lock-free)** — `Counter::inc()` is `AtomicU64::fetch_add(1, Ordering::Relaxed)`; `Gauge::inc()` / `Gauge::dec()` / `Gauge::set(_)` are `AtomicI64::fetch_{add, add, store}(_, Ordering::Relaxed)`. The registry's `RwLock<HashMap<String, StatHandle>>` is read-locked only at scrape time (`/stats/prometheus` reads the full registry under a read lock; under load, scrapes are infrequent so the read lock is uncontended). 06.3's per-request increments and per-connection inc/dec pairs sit on the lock-free path.
- **Rule (representative subset → comprehensive)** — 06.3 extends 06.1's three counters with the standard Envoy stat tree systematically. The naming conventions, namespacing rules (`http.<stat_prefix>.*` for HCM-side stats; `cluster.<name>.*` for cluster-side; `listener.<name>.*` for listener-side), and registration sites established at 06.1 D4.1 are reused unchanged.
- **Rule (access-log line counter increments at queue-enter time)** — fire-and-forget HCM dispatch's emission errors do NOT deflate the count. The `http.<stat_prefix>.access_logs_total` increment site sits in `crates/envoy-http1/src/hcm.rs` *before* the `for sink in &self.access_log { sink.emit(&record).await }` loop — not inside the loop. This is parent SPEC §6 Rule 4 reproduced verbatim and is the single most-likely-to-be-mis-implemented-at-execution-time invariant in this sub-phase.
- **Rule (parent close happens at THIS sub-phase's state-6 commit)** — per the ROADMAP-schema invariant in `BOOTSTRAP_PROMPT.md` §4.1 ("when a phase is split, its own status becomes `in-progress` while its sub-phases land. The parent flips to `done` only after all sub-phases are `done`"), the 06.3 state-6 phase-done commit ALSO flips parent ROADMAP row `06` from `in-progress` to `done`. Mirrors phase-04's `e626862`-shape close-out (the 04.3 state-6 commit also flipped parent-04 done) and phase-05's `82c26b8`-shape close-out (the 05.3 state-6 commit also flipped parent-05 done). This rule drives §10's state-machine commit composition: a single commit lands the comprehensive-wiring REVIEW.md verdict + flips ROADMAP row 06.3 + flips parent ROADMAP row 06 + advances STATE.md to phase 07 lifecycle state 1 + clears the parent-06 active-phase pointer.

**Cross-phase items closed at 06.3.** One major substantive close-out:

- **Phase-05.3 REVIEW I1** (silent H1-listener × H2-cluster misnegotiation per ADR-0028 option-B deferral). Verbatim from STATE.md "Phase-05.3 rollovers" subsection: *"ADR-0028's option-(B) deferral leaves an H1-listener × H2-cluster silent runtime protocol-misnegotiation. ADR-0028 deliberately defers the H1-listener H2-arm dispatch (the H1 listener cannot dispatch to a cluster whose `upstream_protocol = Http2` because adding `envoy-http2` as a dep on `envoy-http1` would cycle). The deferral is correct doctrine — but the deferred path is not gated at parse time, so a configuration with `codec_type: AUTO` (or `HTTP1`) on the listener and a cluster with `typed_extension_protocol_options.HttpProtocolOptions.http2_protocol_options` set does NOT fail validation; it silently runs H1-on-the-wire to an H2-only upstream backend, which would either get rejected by the backend or produce a confusing 502. **Fix sketch**: add `ConfigError::Http2ClusterFromHttp1Listener { listener: String, cluster: String }` parse-time gate at the envoy-config validator (cross-validate listener `codec_type` against each cluster's `upstream_protocol` per route), or add a runtime defense at the H1 router-arm `BuildOutcome::Proxy` site that catches the misconfiguration with an explicit log + 502. **Disposition: carry forward to phase 06+.** Phase 06's brainstorm should consider folding I1 as a Task-1 preamble (the parse-time gate is a small validator extension)."* — 06.3's D14.3 selects the parse-time gate option, lands it at Task 1 mechanically, and substantively closes I1. PROGRESS.md at the corresponding task quotes "Closes 05.3 REVIEW I1" with a cross-reference to STATE.md "Phase-05.3 rollovers" and to ADR-0028.

**Cross-phase items unblocked but not closed at 06.3.** None directly — the comprehensive-stats-wiring slice does not engage 05.3's I2 (typed-error chain dissolution at H2 dispatch site; recommended fix is a structured `x-envoy-upstream-rq-failure-reason` response header) since 06.3 does not edit the H2 router-arm dispatch site or the response-write path. I2 carries forward unchanged.

**Cross-phase items continuing to carry forward through 06.3.** Verbatim from STATE.md and the parent-06 SPEC §4: phase-05.3 REVIEW I2 (typed-error chain dissolution at H2 dispatch site); phase-05.2 REVIEW I1 (CI h2spec tarball SHA-256 verification — `.github/workflows/ci.yml:43-49` provisions h2spec via unverified `curl | tar`; not closed in 06.3 because no state-4 task touches the CI workflow); phase-05.2 REVIEW I2 (`Http2Error` write-path variant rename); phase-05.2 REVIEW I3 (`MalformedH2HeaderBlock` overload split); phase-04.1 REVIEW M1/M2/M4 (header-diff value-comparison; body-drain idle silent Ok; strip_port IPv6-Host); phase-04.1 REVIEW M5/M9 (Cargo.lock cadence ratification ADR — 06.3 introduces no new top-level Cargo deps under the recommended posture, so does not force the ratification call; M5/M9 continues unchanged, anticipated to close at whichever post-parent-06 phase first adds a workspace member with a new top-level dep); phase-04.1 REVIEW M7 (`TlsAcceptingHandler.inner` concrete-typed; H2+TLS rejection at `Http2OverTlsNotSupported` parse-time gate continues unchanged); phase-02.2 REVIEW M1 (`*EchoBackend::Drop` polling loop blocks on `std::thread::sleep` from a tokio-runtime thread; standing inventory carryforward; 06.3 does not parallelize `run_fixture` so M1 continues unchanged).

**Scope-shape inheritance from the parent-06 brainstorm.** The brainstorm explicitly bounded 06.3 to: validator extension (the `Http2ClusterFromHttp1Listener` gate at envoy-config validator only — NOT any other validator extensions; NOT any schema additions to listener/cluster config); stats wiring (the comprehensive set per parent SPEC §3 D15.3 only — NOT any new stat families beyond per-response-class counters / lifetime gauges / upstream-side router counters / access-log line counter / listener accept-failure counter; NOT any histograms; NOT any tag extraction); BEHAVIOR_CONTRACT.md (the `Stat-name mapping` extension only — NOT any `Header allow-list` edits; NOT any `Access log field mapping` extension beyond what 06.2 landed); fixture extension (fixture 0011's `expectations.yaml` only — NOT any new fixtures; NOT any edit to fixtures 0001-0010, 0012); harness extensions (BodyRule::PrometheusExposition extension if needed for gauge value-may-be-zero shape only — NOT any new driver variants; NOT any new harness primitives); parent close-out (the ROADMAP row `06` flip + the STATE.md advance to phase 07 lifecycle state 1). This bounding is reproduced verbatim in §4 below as 06.3's non-goals.

**Acceptance signal** — the phase-done gate from §7.5 of `BOOTSTRAP_PROMPT.md`, scoped to 06.3's feature surface AND the parent-phase-06 acceptance surface (since 06.3 is the closing sub-phase, its state-4 gate is also the parent-06 gate):

- **(a)** the extended differential fixture `tests/fixtures/0011-admin-stats-prometheus/` (with the comprehensive-set `expectations.yaml`) is green at the Docker-gated CI level, with the CI run URL + the test result quoted inline in `PROGRESS.md`.
- **(b)** the 11 pre-existing differential fixtures `tests/fixtures/{0001-tcp-echo, 0002-static-admin-ready, 0003-tcp-proxy, 0004-tls-downstream, 0005-tls-upstream, 0006-tls-sni, 0007-http1-direct-response, 0008-http1-router-upstream, 0009-http2-direct-response, 0010-http2-router-upstream, 0012-access-log-file-sink}/` remain green at the Docker-gated CI level (they are not edited in 06.3; their fixtures were green at sub-phase-06.2 close and continue green). **All 12 fixtures (0001-0012) green simultaneously** is the parent-06 differential surface in full.
- **(c)** the conformance suite `tests/conformance/h2spec/` continues at **≥95% pass** (landed at 05.2 D7; 06.3 does not edit the runner or the gate); the 06.3 state-4 verification re-runs h2spec to confirm no regression. The expected pass shape is unchanged from the 05.3 close-out evidence (CI run `25333279366`; 144 passed / 1 failed / 1 skipped of 146 = 99.31%).
- **(d)** the existing fuzz target `parse_bootstrap` runs clean for its short-budget CI run (`cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30`) against the corpus extended in 06.1 (≥1 admin-listener-with-stats-route seed) + 06.2 (≥1 HCM `access_log` block seed) — corpus unchanged in 06.3 (the I1-closing validator gate does not introduce a new seed because the gate's accept-path is exercised by the existing 0011 / 0012 fixture YAMLs, and the gate's reject-path is covered by D14.3's negative unit tests; per signpost 1 below the planner may opportunistically add 1 seed `h1_listener_with_h2_cluster_rejects.yaml` if the validator's reject-path is worth fuzzing — recommendation is no new seed since the rejection is structural and serde-walk already covers it). No new fuzz target ships in 06.3.
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, and `cargo deny check` are all clean on the stable-toolchain CI job. `cargo deny check` is a no-op (no new top-level Cargo deps in 06.3 under the recommended no-foundations-grant posture; the `envoy-stats` registry surface from 06.1 is sufficient for the comprehensive wiring, and the access-log dispatch site from 06.2 is sufficient for the access-logs-total counter).
- **(f)** `REVIEW.md` for this sub-phase is approved.

The 06.3 phase-done commit flips ROADMAP row `06.3` from `in-progress` to `done`. **At the same commit:** parent ROADMAP row `06` (*"Access log (file sink, Envoy default format) + stats + Prometheus admin endpoint"*) flips from `in-progress` to `done` per the ROADMAP-schema invariant — since 06.1 and 06.2 are `done` at 06.3 start (per the strict 06.1 → 06.2 → 06.3 ordering established by parent SPEC §5), landing 06.3 `done` completes the parent. STATE.md advances from `06.3-stats-wiring-and-close` lifecycle state 6 to phase `07-<slug>` lifecycle state 1 (phase-07 directory does not exist; next-skill `superpowers:brainstorming` scoped to phase 07's `BOOTSTRAP_PROMPT.md` §8 row 07 charter — *"Filter chain framework: iteration protocol, per-route config, extension registry"*; expected slug `07-filter-chain-framework` or similar, the planner uses whatever slug phase-07 brainstorm chooses). **Phase 06 (Observability foundations) closes at this commit; phase 07 state-1 brainstorm is next.**

---

## 2. Behavior-contract scope for sub-phase 06.3

Phase 06.3 is the second-and-final phase to populate `docs/envoy-rust/BEHAVIOR_CONTRACT.md`'s `Stat-name mapping` section: 06.1 D4.1 lands the initial 3 entries (`listener.<name>.downstream_cx_total`, `cluster.<name>.upstream_cx_total`, `http.<stat_prefix>.downstream_rq_total`); 06.3 D16.3 extends with the comprehensive set per parent SPEC §3 D16.3.

**`Stat-name mapping` extension table** (the new rows lands as one diff against `BEHAVIOR_CONTRACT.md` at 06.3 D16.3; format reuses the existing `Header allow-list` table shape — `| Stat | Equivalence | Rationale |`):

| Stat | Equivalence | Rationale |
|---|---|---|
| `http.<stat_prefix>.downstream_rq_2xx` | value-exact | Per-class counter incremented at HCM on-response-complete on `response.status / 100 == 2`. Both proxies emit on every 2xx response; under deterministic load (fixture 0011 sends a fixed set of requests per status class), both counters land at the same value. |
| `http.<stat_prefix>.downstream_rq_3xx` | value-exact | Same shape as `..._2xx`. Fixture 0011's request set includes one 3xx response (a `direct_response` configured to return 301 — see §3 D17.3). |
| `http.<stat_prefix>.downstream_rq_4xx` | value-exact | Same shape as `..._2xx`. Fixture 0011's request set includes one 4xx response (a request to a non-matched route falls through to the HCM's default 404 handler; or a configured `direct_response: 404` per §3 D17.3 picks). |
| `http.<stat_prefix>.downstream_rq_5xx` | value-exact | Same shape as `..._2xx`. Fixture 0011's request set includes one 5xx response (a `direct_response: 503` configured route, OR a router-proxy path against a synthetic 5xx-emitting backend — the planner picks at PLAN-time, see signpost 5 below). |
| `listener.<name>.downstream_cx_active` | value-exact under deterministic close timing | Gauge incremented on accept, decremented on close. Under fixture 0011's deterministic small-burst (a fixed sequence of N requests issued sequentially against the HCM listener; each request's TCP connection completes round-trip and closes before the next begins), the gauge's terminal value at scrape time is `0` on both proxies (all connections closed). The harness asserts the gauge is present and value-exact-zero at scrape time. The gauge's monotonic-then-decreasing trajectory across the burst is verified at unit-test level in D15.3, not at fixture level (fixture 0011's scrape happens once at the end of the burst, when both proxies' gauges have re-decremented to zero; capturing the mid-burst peak is timing-sensitive and out of scope per parent SPEC §4). If the harness's ephemeral admin-scrape connection itself is counted by the gauge — see signpost 7 below — the disposition relaxes to `name-required, value-may-differ`; recommendation per signpost 7 is to scope the listener-side increment to non-admin listeners, keeping the disposition `value-exact`. |
| `cluster.<name>.upstream_cx_active` | value-exact under deterministic close timing | Gauge incremented on upstream-connect, decremented on upstream-close. Same shape as `listener.<name>.downstream_cx_active`; fixture 0011's request set does not pool upstream connections (envoy-rust per-call-per-connection per phase-04.3 / 05.3 posture; Envoy with default pool may reuse — but Envoy's gauge semantics also count-and-decrement per-stream when the stream completes, so terminal value is `0` on both sides), so the gauge's terminal value at scrape time is `0` on both proxies. |
| `cluster.<name>.upstream_rq_total` | value-exact | Counter incremented at the router proxy-arm completion site (in `write_proxied_response`). Both proxies emit on every router-proxy completion; under deterministic load, both counters land at the same value. Excludes router-synthesized 502s (per §1 layer 4 — those don't reach `write_proxied_response` because the upstream `Client::connect` errored before the response was constructed). |
| `cluster.<name>.upstream_rq_5xx` | value-exact | Counter incremented at the router proxy-arm completion site when `response.status / 100 == 5`. Same shape as `..._upstream_rq_total`. Excludes router-synthesized 502s (same rationale); if fixture 0011's 5xx-path is a synthetic-backend 5xx (recommendation per signpost 5), this counter increments; if fixture 0011's 5xx-path is a `direct_response: 503` configured route (alternative per signpost 5), this counter does NOT increment because `direct_response` paths never reach the router proxy-arm — those are `BuildOutcome::DirectResponse` outcomes that exit the HCM's outcome-match early per 04.1 D2's dispatch shape. |
| `http.<stat_prefix>.access_logs_total` | value-exact | Counter incremented at HCM access-log dispatch queue-enter time (per parent SPEC §6 Rule 4). One increment per request that emits an access log (i.e., per request when `HCMConfig.access_log` is non-empty). Fixture 0011's HCM is configured with one FileSink access-log per the 06.2 D9.2 schema, so this counter increments once per request. **Crucially:** the increment runs *before* the sink-emit dispatch; emission failures (file system errors, etc.) do not deflate the counter. |
| `listener.<name>.downstream_cx_accept_failed` | value-exact for the 0-failures-in-fixture case | Counter incremented when `TcpListener::accept().await` returns `Err(_)`. Fixture 0011 produces no accept failures (the harness drives a clean sequence of TCP connections from a controlled client; no kernel-level transient errors are induced), so both proxies' counters land at `0`. The disposition is `value-exact for the 0-failures case`; non-zero accept-failure scenarios (e.g., a fault-injection test that triggers EMFILE) defer to whichever phase first surfaces them. |

The table reuses the format from `BEHAVIOR_CONTRACT.md`'s existing `Header allow-list` table verbatim. The 3 rows landed at 06.1 D4.1 (`listener.<name>.downstream_cx_total`, `cluster.<name>.upstream_cx_total`, `http.<stat_prefix>.downstream_rq_total`) are unchanged at 06.3 — D16.3 only appends new rows.

**Header allow-list / Access log field mapping / xDS wire state machine / Timing tolerances — untouched in 06.3.** The comprehensive-stats-wiring slice produces no new response shapes, no new access-log tokens, and engages no xDS or timing-sensitive features. The `HEADER_ALLOW_LIST` constant in `tests/differential/src/lib.rs` (the 04.3-landed shape with 3 rows, plus any 06.1 / 06.2 admin-side / access-log rows landed by those sub-phases) is unedited in 06.3.

**Equivalence-matrix engagement (per `BEHAVIOR_CONTRACT.md` §7.2):** Row 7 (Stats — names match Envoy's documented stat tree; presence required; values exact on deterministic flows) — fixture 0011's extended `expectations.yaml` engages this row at scale. The asserted set covers the 06.1 + 06.3 stats union; the equivalence-matrix's "values exact on deterministic flows" clause is honored via the per-row dispositions in the table above. Rows 1–6 / 8 (response status / body / headers / trailers / framing / TLS / TCP) are not engaged by 06.3 (no fixture YAML changes touching the proxied-traffic surface).

---

## 3. Deliverables

This section enumerates the seven 06.3 deliverables (D14.3 through D20.3, mirroring parent SPEC §3 numbering) at per-task PLAN-ready cadence. The cross-sub-phase architectural rules from §1's "Cross-sub-phase architectural invariants reiterated" subsection bind on every deliverable.

### D14.3 — 05.3 REVIEW I1 closure (Task-1 preamble): `ConfigError::Http2ClusterFromHttp1Listener` parse-time validator gate

The first 06.3 deliverable. Mechanical, small, and independent of the comprehensive-wiring work — its placement at Task 1 mirrors phase-05.1 Task 1's posture toward phase-02.1 REVIEW I3 (a previously-identified gap closed cheaply at the start of an unrelated phase).

**Schema delta** in `crates/envoy-config/src/lib.rs` (the `ConfigError` enum):

```rust
// 06.3 NEW — additive variant on ConfigError:
#[error("listener '{listener}' has codec_type HTTP1 (or AUTO) but routes to cluster '{cluster}' whose typed_extension_protocol_options selects HTTP/2 upstream; H1-listener × H2-cluster dispatch is deferred per ADR-0028")]
Http2ClusterFromHttp1Listener {
    listener: String,
    cluster: String,
},
```

The error message includes both the listener's name and the cluster's name (for diagnostic context), and explicitly cross-references ADR-0028 in the message text — operators reading the rejection at config-load time get a pointer to the deferral's rationale without having to grep DECISIONS.md.

**Validator extension** in `crates/envoy-config/src/bootstrap.rs::validate` (the existing post-deserialization validation pass that fires the schema-cross-checks; landed at phase 01 and extended at every subsequent phase that adds a cross-cutting constraint — e.g., 04.1's RouteConfiguration validation, 05.1's STRICT_DNS resolution, 05.2's `Http2OverTlsNotSupported`, 05.3's `MutuallyExclusiveExplicitHttpConfig`). 06.3 D14.3 adds a **per-listener cluster-reachability scan**:

```rust
// Pseudocode — exact code lands at PLAN.md writeup:
//
// For each listener in bootstrap.static_resources.listeners:
//   Determine listener_codec_type:
//     if listener has TcpProxy filter → SKIP (TCP-proxy listeners don't engage HTTP-protocol mismatch; their cluster-reachability is unconstrained by H1/H2 semantics).
//     if listener has HttpConnectionManager filter → listener_codec_type = filter.typed_config.codec_type
//   if listener_codec_type ∈ {HTTP1, AUTO}:
//     Walk every route in every virtual_host in route_config:
//       For each route whose RouteAction == Route { cluster: <name> }:
//         Look up the cluster by name in bootstrap.static_resources.clusters.
//         if cluster.typed_extension_protocol_options.HttpProtocolOptions.explicit_http_config.http2_protocol_options.is_some():
//           return Err(ConfigError::Http2ClusterFromHttp1Listener { listener, cluster });
//   if listener_codec_type == HTTP2:
//     no scan — H2 listener can dispatch to either H1 or H2 cluster per 05.3 D4.
```

The scan's complexity is `O(listeners × routes × cluster-lookup)`; `cluster-lookup` is `O(clusters)` if implemented naively or `O(1)` with an indexed-by-name hash map — the planner picks at PLAN-time per signpost 1 below. Recommendation: build a `HashMap<&str, &Cluster>` once upfront from `bootstrap.static_resources.clusters` and reuse it across the listener-walk; the upfront build is `O(clusters)` and total scan complexity drops to `O(listeners × routes)`. Mirrors the existing `cluster-name-resolution` pattern at 04.1's RouteConfiguration validator.

**The `direct_response` route action carve-out.** Routes whose `RouteAction` is `DirectResponse` (landed at 04.1) do not engage the gate — they don't dispatch to any cluster. Routes whose `RouteAction` is `Route { cluster: <name> }` engage the gate. Routes whose `RouteAction` is anything else (none currently exist; the `Redirect` variant defers to a later phase) are skipped silently — adding new RouteAction variants is the responsibility of whichever phase adds them, and that phase's brainstorm decides whether the new variant engages this gate.

**Handling of TCP-proxy listeners.** TCP-proxy listeners (with a `TcpProxy` network filter, as in fixtures 0003 / 0004 / 0005 / 0006) are skipped — they don't have a `codec_type` and their cluster-reachability is unconstrained by H1/H2 semantics. The scan only fires for HCM-bearing listeners.

**Handling of admin listeners.** The HCM-backed admin listener landed at 06.1 D3.1 is a regular HCM listener with `codec_type: HTTP1` (per parent SPEC §6 Rule 3 — admin is HTTP/1.1 only); its routes target admin endpoints, not clusters in the static_resources cluster list. The validator skips routes whose target is not a static cluster (`bootstrap.static_resources.clusters` lookup miss → silent skip; existing `UnknownCluster` validator path fires for non-admin listeners targeting unknown clusters, which is unchanged in 06.3).

**Tests in `crates/envoy-config/src/bootstrap.rs::tests`** (5 tests projected — exactly the 5 specified in parent SPEC §3 D14.3):

1. `validates_h1_listener_with_h1_cluster_passes` — full bootstrap with one HCM listener (`codec_type: HTTP1`) + one route to cluster `backend` + one cluster `backend` with no `typed_extension_protocol_options` (defaults to H1 upstream per 05.3 D3); validator accepts; no `Http2ClusterFromHttp1Listener` error fires.
2. `validates_h2_listener_with_h2_cluster_passes` — full bootstrap with one HCM listener (`codec_type: HTTP2`) + one route to cluster `backend` + one cluster `backend` with `typed_extension_protocol_options.HttpProtocolOptions.explicit_http_config.http2_protocol_options: {}` (selects H2 upstream per 05.3 D2.a); validator accepts.
3. `validates_h2_listener_with_h1_cluster_passes` — full bootstrap with one HCM listener (`codec_type: HTTP2`) + one route to cluster `backend` + one cluster `backend` with no `typed_extension_protocol_options` (defaults to H1 upstream); validator accepts. Verifies that H2-listener × H1-cluster is the **load-bearing-supported combination** per 05.3 D4 (the H2 listener-side HCM dispatches to H1 cluster via `cluster.upstream_protocol() == Http1` arm).
4. `rejects_h1_listener_with_h2_cluster` — full bootstrap with one HCM listener (`codec_type: HTTP1`) + one route to cluster `backend` + one cluster `backend` with `typed_extension_protocol_options.HttpProtocolOptions.explicit_http_config.http2_protocol_options: {}`; validator returns `Err(ConfigError::Http2ClusterFromHttp1Listener { listener: "<name>", cluster: "backend" })`. Verifies the gate's primary reject-path.
5. `rejects_auto_listener_with_h2_cluster` — full bootstrap with one HCM listener (`codec_type: AUTO`) + one route to cluster `backend` + one cluster `backend` with `typed_extension_protocol_options.HttpProtocolOptions.explicit_http_config.http2_protocol_options: {}`; validator returns the same error as test 4. Verifies that AUTO-listener (which behaves as H1-only per parent §4) engages the gate identically.

Plus 1 regression-guard test ensuring TCP-proxy listeners are unaffected:

6. `tcp_proxy_listener_with_h2_cluster_unaffected` — full bootstrap with one TCP-proxy listener (no `codec_type`) + cluster `backend` with `typed_extension_protocol_options.HttpProtocolOptions.explicit_http_config.http2_protocol_options: {}`; validator accepts (the H2 cluster is configured but unreachable from the TCP-proxy listener per the carve-out above; the cluster's H2 surface is dormant unless reached from an HCM listener). This is contrived (TCP-proxy listeners reaching H2 clusters makes no operational sense), but the carve-out is structurally important — the gate should not over-fire on TCP-proxy listeners.

**LoC estimate D14.3:** ~50 LoC schema + validator (~10 LoC `ConfigError` variant; ~30 LoC validator extension; ~10 LoC cluster-name HashMap if not already present in `validate`) + ~80 LoC unit tests (6 × ~13 LoC each, including the YAML-input fixture inlines per the established envoy-config test pattern). Total D14.3: **~130 LoC**.

PROGRESS.md at the corresponding task quotes "Closes 05.3 REVIEW I1" with cross-references to `docs/envoy-rust/STATE.md` "Phase-05.3 rollovers" subsection and to ADR-0028's Consequences clause. The 05.3 PROGRESS.md at the I1 follow-up site is unchanged (per D-3.4 / D-3.5, prior-phase artifacts are preserved — the close-out is recorded in the *current* phase's PROGRESS, not by editing prior artifacts).

### D15.3 — Comprehensive stats wiring at HCM / router / listener / cluster

The core 06.3 runtime deliverable. Wires the standard Envoy stat tree at HCM, router, listener, and cluster sites — extending 06.1's representative subset (listener `cx_total`, cluster `cx_total`, HCM `downstream_rq_total`) with the comprehensive set per §1 layers 2–6 above.

The wiring is per-stat-class; each class lands as a dedicated unit-test pair (one increment-site verification + one emission-shape verification at the registry / Prometheus exposition layer). Total LoC estimate ~400 impl + ~250 tests = ~650 LoC; per parent SPEC §3 D15.3 projection.

**D15.3.a — Per-response-class HCM counters.** `crates/envoy-http1/src/hcm.rs` (the listener-side HCM landed at 04.1 D2; the existing `HCMConfig` struct extended at 06.1 D4.1 with `stats: Arc<HCMStats>`) gains four counters per stat_prefix in the `HCMStats` struct:

```rust
// At crates/envoy-http1/src/hcm.rs — extension to the HCMStats struct landed at 06.1 D4.1.
// Field naming mirrors Envoy's documented tree at
// https://www.envoyproxy.io/docs/envoy/v1.33.0/configuration/observability/statistics#http-connection-manager-stats:
pub struct HCMStats {
    // 06.1 D4.1 — landed:
    pub downstream_rq_total: Arc<Counter>,

    // 06.3 D15.3.a — NEW:
    pub downstream_rq_2xx: Arc<Counter>,
    pub downstream_rq_3xx: Arc<Counter>,
    pub downstream_rq_4xx: Arc<Counter>,
    pub downstream_rq_5xx: Arc<Counter>,

    // 06.3 D15.3.e — NEW:
    pub access_logs_total: Arc<Counter>,
}
```

The increment-site is the existing `write_response` call site at `crates/envoy-http1/src/hcm.rs` (the on-response-complete hook landed at 04.1; 06.1 D4.1's `downstream_rq_total` increment lands here). 06.3 adds:

```rust
// Pseudocode — exact code lands at PLAN.md writeup:
let class = response.status / 100;
match class {
    2 => self.stats.downstream_rq_2xx.inc(),
    3 => self.stats.downstream_rq_3xx.inc(),
    4 => self.stats.downstream_rq_4xx.inc(),
    5 => self.stats.downstream_rq_5xx.inc(),
    _ => {} // 1xx informational / 6xx non-standard — silently ignored.
}
self.stats.downstream_rq_total.inc(); // 06.1-landed; runs unconditionally.
```

H2 inheritance via `envoy-http2::HCMConfig` type alias — per 05.2 SPEC §3, `envoy-http2::HCMConfig` is a type alias to `envoy-http1::HCMConfig`, so the 06.1 D4.1 wiring landed once and applied to both H1 and H2 listener surfaces. 06.3 D15.3.a follows this pattern: the per-class counter wiring lives on `HCMStats` in `envoy-http1/src/hcm.rs`; the H2 listener-side HCM at `envoy-http2/src/hcm.rs` reads the same `HCMStats` instance via the shared `Arc<HCMStats>` field, so the per-class counters fire identically on both surfaces.

**Tests in `crates/envoy-http1/src/hcm.rs::tests`** (4 tests — one per status class):

- `hcm_increments_downstream_rq_2xx_on_2xx_response` — HCM with a `direct_response: 200` configured route; drive one request; assert `stats.downstream_rq_2xx.get() == 1` and `stats.downstream_rq_total.get() == 1`; the other three class counters are at 0.
- `hcm_increments_downstream_rq_3xx_on_3xx_response` — HCM with a `direct_response: 301` configured route; drive one request; assert `stats.downstream_rq_3xx.get() == 1` and the others at expected values.
- `hcm_increments_downstream_rq_4xx_on_4xx_response` — HCM with no matching route (request falls through to the HCM's default 404 handler from 04.1); drive one request; assert `stats.downstream_rq_4xx.get() == 1`.
- `hcm_increments_downstream_rq_5xx_on_5xx_response` — HCM with a `direct_response: 503` configured route; drive one request; assert `stats.downstream_rq_5xx.get() == 1`.

**LoC estimate D15.3.a:** ~30 LoC (HCMStats struct extension + increment-site + 4 unit tests at ~10 LoC each).

**D15.3.b — Connection-lifetime gauges (listener-side and cluster-side).** Both gauges are `Arc<Gauge>` fields on the relevant struct; both increment-on-open and decrement-on-close.

**Listener-side gauge** at `crates/envoy-listener/src/listener.rs`:

```rust
// Extension to the Listener struct landed at 06.1 D4.1 with cx_total: Arc<Counter>.
pub struct Listener {
    // 06.1 D4.1 — landed:
    pub cx_total: Arc<Counter>,
    // 06.3 D15.3.b — NEW:
    pub cx_active: Arc<Gauge>,
    // 06.3 D15.3.f — NEW:
    pub cx_accept_failed: Arc<Counter>,
    // ... existing fields ...
}
```

The accept loop's per-connection task wraps the `ConnectionHandler::handle` call with `cx_active.inc()` before and `cx_active.dec()` after (via a `tokio::task` spawn that owns a clone of `Arc<Gauge>`); the decrement runs on both successful close and error close, mirroring Envoy's "decrement on any terminal state" semantics. Per signpost 7 below the planner picks at PLAN-time whether the listener-side gauge applies to the admin listener too (recommendation: yes for symmetry; the harness's ephemeral admin scrape connections inflate-and-deflate the gauge transparently — the gauge's terminal value at scrape time is well-defined because the scrape itself completes before the response's exposition body emits, so the scrape's gauge contribution is already decremented to 0 by the time the body is read by the client).

**Cluster-side gauge** at `crates/envoy-cluster/src/cluster.rs`:

```rust
// Extension to the Cluster struct landed at 06.1 D4.1 with cx_total: Arc<Counter>.
pub struct Cluster {
    // 06.1 D4.1 — landed:
    pub cx_total: Arc<Counter>,
    // 06.3 D15.3.b — NEW:
    pub cx_active: Arc<Gauge>,
    // ... existing fields ...
}
```

The increment-site is at the `Client::connect` call sites (in `envoy-http1/src/client.rs` and `envoy-http2/src/client.rs`) and at the TCP-proxy `dial` site (in `envoy-tcp/src/proxy.rs` if reached by 06.3's wiring scan; per signpost 5 below, the TCP-proxy site MAY be wired too for symmetry — recommendation: yes, since TCP-proxy clusters use the same `Cluster` struct and the gauge's namespace is per-cluster, not per-protocol). The increment runs immediately before the TCP-connect call; the decrement runs immediately after the upstream TCP stream is closed (in the per-call `ClientStream` drop or the TCP-proxy's per-connection task epilogue).

The increment-site does NOT live inside `envoy-http1::Client::connect` or `envoy-http2::Client::connect` — that would couple the H1/H2 clients to the cluster-stats namespace, which violates the cross-sub-phase architectural rule "consumers register and increment". Instead, the increment runs at the *call site* of `Client::connect` (in `crates/envoy-http1/src/router.rs`'s `BuildOutcome::Proxy` arm and the symmetric H2-side dispatch at `crates/envoy-http2/src/hcm.rs` per 05.3 D4), where the `Cluster` handle is in scope; the call site reads `cluster.cx_active` and increments before connect, decrements at the per-call epilogue (after `write_proxied_response` returns, regardless of success or 502 fallback).

**Tests in `crates/envoy-listener/src/listener.rs::tests`** (2 tests):

- `listener_cx_active_increments_on_accept_decrements_on_close` — spawns a Listener on a reserved ephemeral port; opens a TCP connection from the test; asserts `listener.cx_active.get() == 1`; closes the connection; waits briefly for the per-connection task's epilogue; asserts `listener.cx_active.get() == 0`.
- `listener_cx_active_monotonic_then_decreasing_under_burst` — spawns a Listener; opens N=5 simultaneous connections; asserts `listener.cx_active.get() == 5` while connections are open; closes all; asserts `listener.cx_active.get() == 0` after epilogues.

**Tests in `crates/envoy-cluster/src/cluster.rs::tests`** (1 test): symmetric to the listener-side `cluster_cx_active_increments_on_connect_decrements_on_close` — spawns an in-process H1-echo backend; cluster `connect` increments; per-call drop decrements.

**LoC estimate D15.3.b:** ~120 LoC (struct fields + per-site inc/dec wiring across `envoy-listener` + `envoy-cluster` + `envoy-http1/router.rs` + `envoy-http2/hcm.rs` + 3 unit tests at ~20 LoC each).

**D15.3.c — Upstream-side router counters.** `crates/envoy-http1/src/router.rs` (the `write_proxied_response` helper landed at 04.3 D5; reused unchanged in 05.3 per parent-05 SPEC §3 cross-sub-phase architectural rule 2 and 05.3 SPEC §3 D4 — *"the response wire-format on the downstream is HCM-on-downstream's concern, not the upstream-protocol's"*) gains two counters per cluster:

```rust
pub struct ClusterStats {
    // 06.1 D4.1 — landed:
    pub upstream_cx_total: Arc<Counter>,

    // 06.3 D15.3.b — landed (gauge, see above):
    pub upstream_cx_active: Arc<Gauge>,

    // 06.3 D15.3.c — NEW:
    pub upstream_rq_total: Arc<Counter>,
    pub upstream_rq_5xx: Arc<Counter>,
}
```

(The struct lives on `Cluster` at 06.1 D4.1; 06.3 just appends fields. Or the planner may fold it into a separate `ClusterStats` struct at PLAN-time per signpost 4 below — the recommendation is to keep all cluster-side stats on `Cluster` for symmetry with `Listener`'s stats fields.)

The increment-site is in `write_proxied_response`, at the function's prologue (after the upstream `Response` is captured but before the response is written to the downstream):

```rust
// Pseudocode — exact code lands at PLAN.md writeup:
pub async fn write_proxied_response(
    downstream: &mut DownstreamWriter,
    upstream: Response,
    elapsed_ms: u128,
    close: bool,
    cluster: &ClusterHandle,    // 06.3 NEW parameter — see signpost 4
) -> std::io::Result<()> {
    cluster.stats().upstream_rq_total.inc();
    if upstream.status / 100 == 5 {
        cluster.stats().upstream_rq_5xx.inc();
    }
    // ... existing 04.3-landed write logic ...
}
```

The new `cluster: &ClusterHandle` parameter is the planner's call: the existing 04.3 signature does not pass cluster context to `write_proxied_response`, so 06.3 D15.3.c either (a) adds a new parameter (recommendation; mechanical; the call sites at `crates/envoy-http1/src/hcm.rs:189-288` and `crates/envoy-http2/src/hcm.rs`'s `BuildOutcome::Proxy` arm already have `cluster` in scope), or (b) plumbs a `ClusterStats` Arc through a thread-local / async-local (not recommended; introduces async-local complexity for no gain). Per signpost 4 below the recommended choice is (a).

**Tests in `crates/envoy-http1/src/router.rs::tests`** (2 tests):

- `write_proxied_response_increments_upstream_rq_total` — invoke `write_proxied_response` with a 200 upstream response; assert `cluster.stats().upstream_rq_total.get() == 1` and `..._5xx.get() == 0`.
- `write_proxied_response_increments_upstream_rq_5xx_on_5xx_status` — invoke with a 503 upstream response; assert both counters incremented.

**LoC estimate D15.3.c:** ~50 LoC (struct fields + signature change + 2-line increment + 2 unit tests at ~15 LoC each).

**D15.3.d — Listener accept-failure counter.** `crates/envoy-listener/src/listener.rs`'s accept loop. The `Listener.cx_accept_failed: Arc<Counter>` field landed at D15.3.b's struct extension. The increment-site is the `Err(_)` arm of the `match accept_result` in the accept loop:

```rust
// Pseudocode at the listener accept loop:
loop {
    match listener.accept().await {
        Ok((stream, peer)) => {
            self.cx_total.inc();
            self.cx_active.inc();
            tokio::spawn(async move {
                handle(stream, peer).await;
                self.cx_active.dec();
            });
        }
        Err(e) => {
            self.cx_accept_failed.inc();
            tracing::warn!(error = %e, "listener accept failed");
            // Continue the loop; one accept failure does not terminate the listener.
        }
    }
}
```

Per signpost 6 below the planner picks at PLAN-time whether the counter fires for ALL accept errors (recommendation; matches Envoy's documented behavior — `downstream_cx_accept_failed` is a single counter without per-errno breakdown) or only ECONNRESET errors. Recommendation is "all accept errors" — the per-errno breakdown defers to whichever phase first surfaces value in the disambiguation.

**Tests in `crates/envoy-listener/src/listener.rs::tests`** (1 test):

- `listener_cx_accept_failed_increments_on_accept_error` — spawn a Listener; force an accept error by binding a TcpListener that immediately closes (or by using a mocked accept-error path; per signpost 6 the planner picks the test scaffolding); assert `listener.cx_accept_failed.get() >= 1`.

**LoC estimate D15.3.d:** ~30 LoC (1-line increment + 1 unit test at ~25 LoC including scaffolding).

**D15.3.e — Access-log line counter.** `crates/envoy-http1/src/hcm.rs`'s on-response-complete hook (the same site as D15.3.a's per-response-class counters). The `HCMStats.access_logs_total: Arc<Counter>` field landed at D15.3.a's struct extension. The increment runs *before* the HCM's access-log dispatch loop (per parent SPEC §6 Rule 4):

```rust
// At crates/envoy-http1/src/hcm.rs's on-response-complete hook:
if !self.access_log.is_empty() {
    self.stats.access_logs_total.inc();    // 06.3 D15.3.e — queue-enter-time increment
    let record = build_access_log_record(&request, &response, /* ... */);
    for sink in &self.access_log {
        if let Err(e) = sink.emit(&record).await {
            tracing::warn!(error = %e, "access log emission failed");
        }
    }
}
```

The increment runs only when at least one sink is configured — HCMs configured without `access_log:` do not increment `access_logs_total`.

Per parent SPEC §3 D15.3's note "a `..._access_logs_failed` counter lands in 06.3 if scope permits": the recommended posture per signpost 5 below is to ship the `..._access_logs_failed` sibling counter in 06.3, since the failed-emission path is observable and the counter's wiring is mechanical (one extra `Counter` field + one increment in the `if let Err(e) =` arm). The Stat-name mapping table in §2 above does NOT include `..._access_logs_failed` because fixture 0011's deterministic file-sink does not exercise the failure path; if shipped, the row lands as `value-exact for the 0-failures case` (parallel to `listener.<name>.downstream_cx_accept_failed`). Per signpost 5 the planner ships both counters under the recommended posture.

**Tests in `crates/envoy-http1/src/hcm.rs::tests`** (2 tests):

- `hcm_increments_access_logs_total_on_emission` — HCM configured with one FileSink (writing to a temp file); drive one request; assert `stats.access_logs_total.get() == 1` and the temp file contains one line.
- `hcm_increments_access_logs_total_at_queue_enter_not_emission` — HCM configured with one MockSink that always returns Err(_); drive one request; assert `stats.access_logs_total.get() == 1` (the increment ran at queue-enter, before the failed emission); if `access_logs_failed` is shipped per signpost 5, also assert `stats.access_logs_failed.get() == 1`.

**LoC estimate D15.3.e:** ~30 LoC (struct field + 1-line increment + 2 unit tests at ~12 LoC each, including the MockSink scaffolding for test 2).

**D15.3.f — Listener accept-failure counter.** Already covered by D15.3.d above. (D15.3 has 6 sub-deliverables; D15.3.d and D15.3.f are the same — the listener accept-failure counter; the intentional-redundancy here mirrors parent SPEC §3 D15.3's enumeration which also folds the listener accept-failure counter into the comprehensive set.)

**Total D15.3 LoC estimate:** ~30 (D15.3.a) + ~120 (D15.3.b) + ~50 (D15.3.c) + ~30 (D15.3.d/f) + ~30 (D15.3.e) = **~260 LoC impl** + ~140 LoC tests = **~400 LoC total**, comparable to parent SPEC §3 D15.3's projection of "~400 LoC across envoy-http1/envoy-http2/envoy-listener/envoy-cluster + ~250 LoC unit tests". The ~250 LoC unit-test estimate from the parent SPEC absorbs the 12 unit tests across D15.3.a-f at higher per-test density (~20 LoC each including scaffolding) — the planner refines at PLAN-time.

### D16.3 — BEHAVIOR_CONTRACT.md `Stat-name mapping` extension

Doc-only diff. Per §2 above, ~7 new rows land in the `Stat-name mapping` table (`downstream_rq_2xx`/`3xx`/`4xx`/`5xx` collapsed to one templated row pair if the planner prefers; the recommended shape is one row per stat for clarity, matching the existing `Header allow-list` table's per-row granularity). The 06.1-landed 3 rows are unchanged.

**Where the doc edit lands.** The `Stat-name mapping` section in `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (lines 49-64 at the post-06.1 close HEAD; the section's standing comment "_(empty; populated starting phase 06)_" was removed in 06.1 D4.1 in favor of an introductory paragraph + a 3-row table; 06.3 D16.3 appends to the existing table). The diff is one Markdown table extension.

**LoC estimate D16.3:** ~80 LoC of doc-only diff (the 7-row table extension + ~10 LoC of preamble adjusting "as of 06.1" to "as of 06.3" if needed; mostly table rows). Total D16.3: **~80 LoC**, parallel to parent SPEC §3 D16.3's projection.

### D17.3 — Fixture 0011 `expectations.yaml` extension

`tests/fixtures/0011-admin-stats-prometheus/expectations.yaml` (landed at 06.1 D6.1 with the representative-subset assertion) extends to assert the comprehensive set per §2 above. The fixture's `envoy.yaml` and `envoy-rust.yaml` are unchanged — only the `expectations.yaml` grows.

**Shape of the extension.** The `expected_body_rule.allowlist_envoy_only` and `allowlist_envoy_rust_only` lists in the existing `BodyRule::PrometheusExposition` grow to cover the new stats (they should be empty modulo the 06.1-landed `cluster.<name>.upstream_cx_total`'s `name-required, value-may-differ` row from 06.1 D4.1 / parent SPEC §2 — that row is unchanged in 06.3). The new value-exact rows assert **exact value matching** on both proxies' scrapes; the new gauge rows (`listener.<name>.downstream_cx_active`, `cluster.<name>.upstream_cx_active`) assert value-exact-zero at scrape time per §2's disposition.

**Fixture 0011's request-set extension.** Per §2 the fixture must drive a request sequence that exercises every status class (2xx, 3xx, 4xx, 5xx). The 06.1-landed fixture drives one request through HCM/cluster/listener (a single 200 response from an in-test direct_response or upstream-echo backend); 06.3 extends the request-set to cover all 4 classes. Per signpost 5 below the planner picks at PLAN-time between (a) configuring the HCM with 4 separate routes — one each at `direct_response: {200, 301, 404, 503}` — and driving 4 sequential requests; (b) configuring the HCM with one router-proxy route to a synthetic-backend that returns each status class on demand; (c) a hybrid of (a) for 2xx/3xx/4xx + (b) for 5xx (so the `cluster.<name>.upstream_rq_5xx` counter increments — which doesn't fire on `direct_response` paths). Recommendation: (c). The fixture's `inputs/payload.bin` (which describes the request sequence per 04.3's harness pattern) extends accordingly.

**Test re-runs.** The Docker-gated `tests/differential/tests/admin_stats_prometheus.rs` (landed at 06.1 D6.1; 7-line wrapper calling `differential::run_fixture("0011-admin-stats-prometheus")`) is unedited; it picks up the extended `expectations.yaml` automatically and re-runs against both proxies.

**LoC estimate D17.3:** ~30 LoC of YAML diff (the `expectations.yaml` extension; ~20 LoC of new asserted-stat rows + ~10 LoC of `inputs/payload.bin` request-sequence extension). Total D17.3: **~30 LoC**, parallel to parent SPEC §3 D17.3's projection.

### D18.3 — Differential harness extensions if needed

Conditional on D17.3's shape. Per parent SPEC §3 D18.3: *"Differential harness extensions if needed (BodyRule::PrometheusExposition extension covering gauges' value-may-be-zero shape if needed; ~50 LoC max; may be no-op)."*

The 06.1 D6.1 `BodyRule::PrometheusExposition` body-rule asserts on **the set of metric names** present, not on values — per parent SPEC §3 D6.1's statement *"the body rule asserts equivalence on the symmetric difference of metric names between the two proxies' scrapes"*. The 06.3 extension asserts on **values too**, per the value-exact dispositions in §2. This requires either:

1. **Extend `BodyRule::PrometheusExposition`** to grow per-stat value assertions (e.g., `BodyRule::PrometheusExposition { allowlist_envoy_only, allowlist_envoy_rust_only, value_exact: HashMap<String, u64>, value_must_be_zero: HashSet<String>, value_present_only: HashSet<String> }`).
2. **Add a new `BodyRule` variant** like `BodyRule::PrometheusExpositionValueExact` that subsumes the 06.1 variant.

Per signpost 9 below the recommended posture is option (1) — extend the existing variant additively. The `value_exact` map carries `<stat_name, expected_value>` pairs that must match-exact on both proxies; the `value_must_be_zero` set carries names that must equal 0 on both proxies (for terminal-zero gauges); the `value_present_only` set carries names whose value may differ between proxies but whose presence is required (for the `cluster.<name>.upstream_cx_total` `name-required, value-may-differ` row from 06.1).

**Tests in `tests/differential/src/lib.rs::tests`** (2 tests):

- `prometheus_exposition_body_rule_asserts_value_exact` — synthesize two Prometheus scrapes with identical name+value sets; assert the rule passes.
- `prometheus_exposition_body_rule_rejects_value_mismatch` — synthesize two scrapes with same names but one value differs; assert the rule fails with a value-mismatch error.

**LoC estimate D18.3:** ~50 LoC (the `BodyRule::PrometheusExposition` extension + 2 unit tests at ~15 LoC each + ~20 LoC of harness-side parsing extension). Per parent SPEC §3 D18.3, this may be **no-op** if 06.1 D6.1 already shipped value-exact assertion (the planner verifies at Task 1 time by reading the 06.1-landed BodyRule shape; if 06.1 already covers value-exact, D18.3 is purely test-additions ~10 LoC). Total D18.3: **~50 LoC max**, possibly less.

### D19.3 — Parent-06 state-6 close-out

**No new code in D19.3.** The 06.3 state-6 phase-done commit is also the parent-phase-06 close-out commit; this deliverable enumerates the close-out wiring per parent SPEC §8 (artifacts amended at sub-phase state-6 commits) and §9 (parent close-out commit format). Mirrors phase-04's `e626862`-shape and phase-05's `82c26b8`-shape close-outs.

The 06.3 state-6 commit:

1. **Flips ROADMAP row `06.3` `status` `in-progress` → `done`.**
2. **Flips parent ROADMAP row `06` `status` `in-progress` → `done`** per the ROADMAP-schema invariant (the parent flips when all sub-phases are done; 06.1 and 06.2 already done).
3. **Advances STATE.md** from `06.3-stats-wiring-and-close` lifecycle state 6 to phase `07-<slug>` lifecycle state 1 (phase-07 directory does not exist; next-skill `superpowers:brainstorming` scoped to phase 07 — *"Filter chain framework: iteration protocol, per-route config, extension registry"* per `BOOTSTRAP_PROMPT.md` §8 row 07). The slug is whatever the phase-07 brainstorm picks; expected `07-filter-chain-framework` or similar but the planner does not pre-decide.
4. **Adds Phase-06.3 rollovers Notes subsection** to STATE.md per the established phase-05.3 / phase-05.2 / phase-05.1 / phase-04.3 / phase-04.2 / phase-04.1 / phase-03.2 / phase-03.1 / phase-02.2 / phase-02.1 / phase-01 rollovers cadence — enumerates the 06.3 REVIEW.md verdict (anticipated: Approved with M-track follow-ups at most), in-phase closures (05.3 REVIEW I1 closed at D14.3; 05.2 REVIEW M8-equivalent of structural `direct_response` 5xx attribution if surfaced by D17.3 — none expected), and the awareness-only items + any cross-phase carryforwards (the chain enumerated in §1 above continues unchanged unless 06.3 surfaced something concrete).
5. **Adds Phase-06 ADR ledger summary** to STATE.md under "Phase-06 ADR ledger (final)" — confirms ADR-0029 landed at parent-06 state-2; conditional ADR-0030/0031 stayed unused (the recommended no-foundations-grant posture held); any unforeseen ADRs landed during 06.x execution are listed.
6. **No DECISIONS.md edits anticipated** (per §8 below, no new ADRs are projected for 06.3 at state-2; if an unforeseen ambiguity surfaces during execution per D-3.5, the planner appends the next-sequential ADR — likely ADR-0030 or ADR-0031 depending on whether the conditionals fired in 06.1 / 06.2 — at the time it lands).
7. **Parent SPEC at `docs/envoy-rust/phases/06-observability/SPEC.md` is NOT edited** — remains the historical artifact committed at parent-06 state-1 per D-3.4 / D-3.5 (parent SPECs are preserved unedited as design-projection artifacts even after their sub-phases close; the 04-http1, 05-http2, 03-tls-tcp, 02-tcp-proxy parent SPECs all follow this posture). The 06.3 state-6 commit's title carries the `[parent 06 done]` tag, mirroring 05.3's `[parent 05 done]` tag at commit `82c26b8` and 04.3's `[parent 04 done]` tag at commit `e626862`.

LoC estimate D19.3: 0 LoC code, ~40 lines of ROADMAP / STATE.md / PROGRESS / REVIEW prose at the close-out commit.

### D20.3 — State-4 phase-done verification (verification deliverable, no code)

State-4 phase-done verification per the `BOOTSTRAP_PROMPT.md` §7.5 gate, scoped to 06.3's surfaces + simultaneous green on all 12 fixtures (0001-0012). PROGRESS.md quotes the CI run URL + the 0001-0012 + h2spec results inline. Mirrors 05.3 D8's verification posture and 04.3's state-4 verification at commit `e626862`'s predecessors.

**Verification commands** (per §7.5 of `BOOTSTRAP_PROMPT.md` and the 05.3 D20.3 precedent):

- `cargo build --workspace --all-targets`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo fmt --all -- --check`
- `cargo test --workspace`
- `cargo deny check`
- `cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30` (in `crates/envoy-config/fuzz/`)
- Docker-gated CI run covering all 12 differential fixtures + h2spec runner

PROGRESS.md at the corresponding task quotes the CI run URL (e.g., `https://github.com/pgdad/envoy-rust/actions/runs/<id>`), the per-fixture pass status, the h2spec pass percentage (expected ≥95%, unchanged from 05.3's 99.31%), and the clippy/fmt/test/deny clean status.

LoC estimate D20.3: 0 LoC code, ~40 lines of PROGRESS.md prose at the state-4 commit.

---

## 4. Non-goals (subset of parent SPEC §4 that bind on 06.3)

The following are out of scope for 06.3 and defer to later phases. The list is a subset of parent-06 SPEC §4, scoped to items that are predictably tempting to fold into 06.3 by a planner reading only this SPEC. Cross-references to parent SPEC §4 are explicit.

**Deferred to later phases (per parent-06 SPEC §4 — items relevant to the comprehensive-stats-wiring + parent-close surface):**

- **Histograms.** Counter + gauge primitives only. Histograms (per-request latency distributions; Envoy uses `circllhist` which we cannot replicate cheaply) defer to a later observability-family phase. Prometheus histogram exposition format also defers. **Not in 06.3 scope.**
- **Stats labels / `tag_specifiers`.** Envoy supports tag extraction from stat names (e.g., `cluster.svc_a.upstream_cx_total` → `cluster_upstream_cx_total{cluster="svc_a"}`). Phase 06 emits stats as flat names. Tag extraction defers. **Not in 06.3 scope.**
- **gRPC stat sinks.** No `metrics_service` cluster; no stats-flush-to-cluster. The Prometheus exposition is read-on-demand from the registry per 06.1's posture. External stats sinks defer to the Observability family. **Not in 06.3 scope.**
- **Admin endpoints beyond `/ready`, `/stats`, `/stats/prometheus`.** `/clusters`, `/listeners`, `/server_info`, `/config_dump`, `/runtime`, `/runtime_modify`, `/logging`, `/quitquitquit`, `/healthcheck/fail`, `/healthcheck/ok` defer to phase 08 (Minimum admin API per `BOOTSTRAP_PROMPT.md` §8 row 08). **Not in 06.3 scope.**
- **Graceful drain.** Defer to phase 08. The admin migration in 06.1 does not engage drain semantics; 06.3's listener accept-failure counter does not engage drain. **Not in 06.3 scope.**
- **Connection pooling stat refinements.** Per parent SPEC §4 — the `cluster.<name>.upstream_cx_total` row's "divergence accepted" disposition (per BEHAVIOR_CONTRACT.md `Stat-name mapping` from 06.1 D4.1) reflects envoy-rust's per-call-no-pooling regime; when connection pooling lands in the upstream-robustness family, the counter's semantics tighten to value-exact. 06.3 does not engage pooling. **Not in 06.3 scope.**
- **Stats sinks beyond the in-process registry.** Per parent SPEC §4. The Prometheus exposition is read-on-demand from the registry. **Not in 06.3 scope.**
- **JSON-format access logs.** Per parent SPEC §4. 06.2 ships text-format only. **Not in 06.3 scope.**
- **Per-request access log filtering.** Envoy supports `access_log_filter` blocks. 06.2 ships unfiltered; 06.3 does not extend. **Not in 06.3 scope.**
- **Access-log format-string customization.** Per parent SPEC §4. 06.2 ships the Envoy default format only; format-string parsing defers. 06.3 does not extend the access-log format surface. **Not in 06.3 scope.**
- **HTTP/2 admin listener.** The admin listener in 06.1 is HTTP/1.1 only. H2 admin defers. **Not in 06.3 scope.**
- **TLS admin listener.** Per parent SPEC §4. **Not in 06.3 scope.**
- **Stat-name reload / dynamic-stat lifecycle.** Stats live in the registry forever in 06.x; LRU eviction, scope-bound stats, and dynamic-cluster stat lifecycle defer to xDS-family phases. **Not in 06.3 scope.**
- **`%FILTER_STATE%`, `%DYNAMIC_METADATA%` access-log tokens.** Per parent SPEC §4 — filter-state and dynamic-metadata machinery doesn't exist yet (defers to phase 07's filter-chain framework and beyond). 06.2 ships the 14 fixed default-format tokens only; 06.3 does not extend. **Not in 06.3 scope.**
- **Stats config: `stats_config.use_all_default_tags`, `stats_matcher`, `stats_tags`.** Per parent SPEC §4. **Not in 06.3 scope.**
- **Phase 05.3 REVIEW I2** (typed-error chain dissolution at H2 dispatch site; recommended fix is a structured `x-envoy-upstream-rq-failure-reason` response header reflecting the typed Http2Error variant). Not engaged by 06.3's surfaces (06.3 does not edit the H2 router-arm dispatch site or the response-write path). Carries forward unchanged.
- **Phase 05.2 REVIEW I1** (h2spec tarball SHA-256 verification in CI). 06.3's state-4 verification touches `.github/workflows/ci.yml` only at the h2spec-gate-confirmation level (no provisioning edits); I1 carries forward unchanged.
- **Phase 05.2 REVIEW I2 / I3** (Http2Error variant rename, MalformedH2HeaderBlock split). Not engaged by 06.3; carry forward unchanged.
- **Phase 04.1 REVIEW M5/M9** (Cargo.lock cadence ratification ADR). 06.3 introduces no new top-level Cargo deps under the recommended no-foundations-grant posture; M5/M9 carries forward unchanged. The natural close site shifts to whichever post-parent-06 phase first adds a workspace member with a new top-level dep.
- **Phase 04.1 REVIEW M7** (TLS+H2 ALPN-driven dispatch generalization; `TlsAcceptingHandler.inner: Arc<TcpProxy>` concrete-typed). 06.3 doesn't ship TLS or H2 surfaces; M7 carries forward unchanged.
- **Phase 04.1 REVIEW M1/M2/M4** (`diff_headers` value-comparison silently ignores duplicate-header value mismatches; body-drain idle timeout silent-Ok; `strip_port` IPv6-Host incorrect rfind). Fixture 0011's request set does not exercise duplicate response headers, body-drain stalls, or IPv6-Host conditions; M1/M2/M4 carry forward unchanged.
- **Phase 04.2 REVIEW M5/M8/M9/M11** (whatever 04.2-bound carryforwards remain; the planner consults 04.2 REVIEW.md at 06.3 state-2 if any of these surface in 06.3 execution). Not anticipated to engage.
- **Phase 02.2 REVIEW M1** (`*EchoBackend::Drop` polling loop blocks on `std::thread::sleep` from a tokio-runtime thread). Standing inventory carryforward; 06.3 does not parallelize `run_fixture` so M1 continues to track unchanged.

**Not deferred — confirmed in scope for 06.3** (for clarity, since these have predictable confusion points):

- The `Http2ClusterFromHttp1Listener` parse-time validator gate IS landed in 06.3 D14.3 (closes 05.3 REVIEW I1 substantively; mirrors 05.1 Task 1's posture toward phase-02.1 REVIEW I3).
- The comprehensive Envoy stat tree IS wired in 06.3 D15.3 — extending 06.1's representative subset to include per-response-class HCM counters, connection-lifetime gauges, upstream-side router counters, access-log line counter, and listener accept-failure counter.
- BEHAVIOR_CONTRACT.md `Stat-name mapping` IS extended in 06.3 D16.3.
- Fixture 0011's `expectations.yaml` IS extended in 06.3 D17.3 (NOT a new fixture; the existing 06.1-landed fixture's assertion grows).
- Parent ROADMAP row `06` IS flipped `done` at 06.3's state-6 commit (per the ROADMAP-schema invariant; mirrors phase-04's `e626862` and phase-05's `82c26b8` close-outs).
- The parent SPEC at `docs/envoy-rust/phases/06-observability/SPEC.md` is NOT edited at 06.3's close-out commit (preserved unedited per D-3.4 / D-3.5).
- The 06.1 SPEC at `docs/envoy-rust/phases/06.1-stats-and-admin/SPEC.md` and 06.2 SPEC at `docs/envoy-rust/phases/06.2-access-log/SPEC.md` are NOT edited (closed at their own state-6 commits).

---

## 5. 05.3 REVIEW I1 closure posture (D14.3 detail)

This section reproduces the 05.3 REVIEW I1 carryforward verbatim from `docs/envoy-rust/STATE.md` "Phase-05.3 rollovers" subsection, and explicitly enumerates 06.3 D14.3's posture toward it. Mirrors 05.1 SPEC's posture toward phase-02.1 REVIEW I3 and 05.3 SPEC's posture toward phase-04.3 REVIEW C-1 (both of which closed prior-REVIEW carryforwards as Task-1 preambles).

**Verbatim quote of the 05.3 REVIEW I1 carryforward** (from `docs/envoy-rust/STATE.md` "Phase-05.3 rollovers" subsection, "Items carrying forward to phase 06+ (recommended closure-targets)" bullet):

> **I1 (Important; ADR-0028's option-(B) deferral leaves an H1-listener × H2-cluster silent runtime protocol-misnegotiation).** ADR-0028 deliberately defers the H1-listener H2-arm dispatch (the H1 listener cannot dispatch to a cluster whose `upstream_protocol = Http2` because adding `envoy-http2` as a dep on `envoy-http1` would cycle). The deferral is correct doctrine — but the deferred path is not gated at parse time, so a configuration with `codec_type: AUTO` (or `HTTP1`) on the listener and a cluster with `typed_extension_protocol_options.HttpProtocolOptions.http2_protocol_options` set does NOT fail validation; it silently runs H1-on-the-wire to an H2-only upstream backend, which would either get rejected by the backend or produce a confusing 502. **Fix sketch**: add `ConfigError::Http2ClusterFromHttp1Listener { listener: String, cluster: String }` parse-time gate at the envoy-config validator (cross-validate listener `codec_type` against each cluster's `upstream_protocol` per route), or add a runtime defense at the H1 router-arm `BuildOutcome::Proxy` site that catches the misconfiguration with an explicit log + 502. **Disposition: carry forward to phase 06+.** Phase 06's brainstorm should consider folding I1 as a Task-1 preamble (the parse-time gate is a small validator extension).

**STATE.md "Phase-05.3 rollovers" Recommendations forward-track R-1 echoes the same posture:**

> **R-1** — Phase 06 brainstorm to consider folding I1 (parse-time `Http2ClusterFromHttp1Listener` gate at envoy-config validator) as a Task-1 preamble. Small footprint (~30 LoC validator extension + ~10 LoC per-fixture-shape unit tests); closes the silent-misconfiguration window before phase 06+ touches access-log emission.

**06.3 D14.3 posture.** 06.3 implements the parse-time gate option (the **first** of the two fix-sketch alternatives), at envoy-config validator, mechanically per §3 D14.3 above. The runtime-defense option (the second alternative; an explicit log + 502 at the H1 router-arm `BuildOutcome::Proxy` site) is NOT taken — the parse-time gate is strictly preferable because it rejects the misconfiguration at config-load time (operators see the error at startup, not at runtime), whereas the runtime defense would still allow the proxy to run with a known-bad config and silently 502 on every request.

**Why this is Task 1.** The 05.1 SPEC §3 D5 / 05.1 PROGRESS Task 2 precedent is the closest analogue: 05.1 closed phase-02.1 REVIEW I3 (positive `ClusterType::Static` regression guard) at Task 2 by landing a small mechanical test alongside the Static-arm code path that was now structurally distinguishable from the new `StrictDns` arm. The shape is identical here: 06.3 closes 05.3 REVIEW I1 at Task 1 by landing a small mechanical validator gate that was structurally precluded before 05.3's `typed_extension_protocol_options.HttpProtocolOptions.explicit_http_config.http2_protocol_options` schema landed (the gate cross-checks listener `codec_type` against cluster `upstream_protocol`; the cross-check requires both fields to exist, which they do as of 05.3 close).

**Sequencing rationale.** Placing D14.3 at Task 1 (before D15.3's comprehensive-wiring slice) has three benefits: (1) the I1-closing validator gate is independent of the comprehensive-wiring work — they touch different files and don't share state — so landing it first is mechanically clean; (2) closing a cross-phase carryforward early in a phase reduces the carryforward inventory for the remainder of the phase, simplifying the state-5 REVIEW.md carryforward bookkeeping; (3) the precedent set by 05.1 Task 1 / 05.3 Task 1 is well-established and operators reading the 06.3 PROGRESS.md will expect to see the cross-phase closure at Task 1, not buried mid-phase. PROGRESS.md at Task 1 quotes "Closes 05.3 REVIEW I1" with explicit cross-references to STATE.md "Phase-05.3 rollovers" and to ADR-0028.

---

## 6. Parent-06 close-out posture (D19.3 detail)

This section explicitly enumerates the dual-purpose state-6 commit shape — the 06.3 state-6 phase-done commit ALSO closes parent phase 06. Mirrors phase-04's `e626862`-shape and phase-05's `82c26b8`-shape close-outs.

**Verbatim quote of the ROADMAP-schema invariant from `BOOTSTRAP_PROMPT.md` §4.1 invariant 2:**

> **`ROADMAP.md` schema:** columns `id | title | depends-on | status | sub-phases | summary`. Status ∈ `planned | in-progress | blocked | done`. Append-only history; never delete rows, only update status and sub-phases columns.

And from `ROADMAP.md`'s own "Rules" subsection (which is the canonical projection of the §4.1 invariant onto the file itself):

> When a phase is split, its own `status` becomes `in-progress` while its sub-phases land. The parent flips to `done` only after all sub-phases are `done`.

This rule drives the dual-purpose state-6 commit at 06.3 close. Since 06.1 and 06.2 are `done` at 06.3 entry (per the strict 06.1 → 06.2 → 06.3 ordering established by parent SPEC §5 / ADR-0029), landing 06.3 `done` is the last sub-phase flip; per the rule, parent row 06 also flips `done` at the same commit.

**Reference close-outs**:

- **Phase 04 → 04.3 close-out at commit `e626862`** (the 04.3 state-6 phase-done commit). The commit's title carried `[parent 04 done]`; its diff included the parent ROADMAP row 04's `status` flip from `in-progress` to `done` alongside row 04.3's flip; STATE.md advanced from `04.3-router-upstream` lifecycle state 6 to phase 05 lifecycle state 1.
- **Phase 05 → 05.3 close-out at commit `82c26b8`** (the 05.3 state-6 phase-done commit). Same shape as 04.3's close-out: title `[parent 05 done]`, parent row 05's `status` flip alongside row 05.3's flip, STATE.md advances to phase 06 lifecycle state 1.
- **(Looking ahead) Phase 06 → 06.3 close-out at <commit-SHA-of-06.3-state-6>** (this sub-phase's state-6 phase-done commit). Title carries `[parent 06 done]`; diff includes parent row 06's `status` flip from `in-progress` to `done` alongside row 06.3's flip; STATE.md advances from `06.3-stats-wiring-and-close` lifecycle state 6 to phase 07 lifecycle state 1; next-skill `superpowers:brainstorming` scoped to phase 07 (filter chain framework per `BOOTSTRAP_PROMPT.md` §8 row 07).

**What the 06.3 state-6 commit does to disk** (per §10 below):

1. ROADMAP row `06.3` `status: in-progress` → `status: done`.
2. ROADMAP row `06` `status: in-progress` → `status: done` (alongside row 06.3).
3. STATE.md `Active phase` block transitions from "Phase 06.3 (06.3-stats-wiring-and-close) is DONE as of this commit" framing (mirroring 05.3 STATE's framing at predecessor `82c26b8`) to "Phase 07 (07-<slug>) state 1" framing.
4. STATE.md `Last commit` block updates with the 06.3 state-6 commit SHA + summary.
5. STATE.md `Next expected skill` block advances to `superpowers:brainstorming` scoped to phase 07's `BOOTSTRAP_PROMPT.md` §8 row 07 charter.
6. STATE.md `Notes` section gains a "Phase-06.3 rollovers" subsection per the established cadence + a "Phase-06 ADR ledger (final)" subsection summarizing parent-06's ADR landings.
7. DECISIONS.md ledger head: ADR-0029 (or higher if conditional ADRs landed during 06.x execution; per §8 below the recommended posture is no foundations grants).
8. Parent SPEC at `docs/envoy-rust/phases/06-observability/SPEC.md` is NOT edited (preserved unedited per D-3.4 / D-3.5; mirrors parent-04 / parent-05 SPECs' posture at their close-out commits).
9. The 06.1 SPEC at `docs/envoy-rust/phases/06.1-stats-and-admin/SPEC.md` and 06.2 SPEC at `docs/envoy-rust/phases/06.2-access-log/SPEC.md` are NOT edited (closed at their own state-6 commits per D-3.4 / D-3.5).

**Commit message format** per `BOOTSTRAP_PROMPT.md` §5.3 with the `[parent 06 done]` tag — see §9 below for the explicit format and an example.

---

## 7. Implementation signposts for the planner

Notes flagging predictable planner questions so the 06.3 planner resolves them in-plan rather than mid-execution. Inherits parent-06 SPEC §6 cross-sub-phase invariants where they bind on 06.3, plus 06.3-local signposts.

**Signpost 1 — `Http2ClusterFromHttp1Listener` validator scan strategy (eager vs lazy / single-pass vs incremental).** The validator gate at D14.3 has two implementation shapes: (a) **eager single-pass**: build a `HashMap<&str, &Cluster>` once upfront from `bootstrap.static_resources.clusters`, then walk every listener's routes and look up each cluster O(1); (b) **lazy per-route**: re-walk `bootstrap.static_resources.clusters` for each route's cluster name, accepting O(listeners × routes × clusters) complexity. **Recommendation:** (a) — the upfront HashMap build is mechanical, matches the existing cluster-name-resolution pattern at 04.1's RouteConfiguration validator, and is asymptotically cheaper. The fuzz corpus's worst-case bootstrap (~50 clusters × ~50 listeners × ~50 routes) under (b) hits 125k cluster lookups; under (a) it hits 2.5k route walks + 1 hashmap build, ~50× faster. Per signpost 1 the planner picks (a) at PLAN time.

**Signpost 2 — Gauge implementation atomic ordering for inc-then-dec correctness.** The connection-lifetime gauges (D15.3.b) increment-on-open and decrement-on-close. The atomic-ordering question: are `Gauge::inc()` and `Gauge::dec()` correct under `Ordering::Relaxed` for the read-side scrape, or do they need `Ordering::AcqRel`? **Answer:** `Relaxed` is correct because the scrape reads the gauge's current value via `AtomicI64::load(Ordering::Relaxed)`; the gauge's eventual-consistency under high load is acceptable per parent SPEC §6 invariant 6 ("Counter::inc lock-free; Gauge::set/inc/dec lock-free; registry RwLock read-locked only at scrape time"). The scrape's read may see a stale value (a connection that just incremented but whose epilogue hasn't run yet, or vice versa) — this is acceptable because Prometheus exposition is inherently eventually-consistent; operators reading scrapes accept ~1-second-staleness as the norm. **Recommendation:** `Ordering::Relaxed` for all gauge ops, matching the counter ops landed at 06.1 D1.1.

**Signpost 3 — Per-response-class counter naming (snake-case-with-underscores vs other shapes).** Envoy's documented stat names use snake-case-with-underscores throughout: `downstream_rq_2xx`, `upstream_cx_active`, etc. envoy-rust matches this exactly per the `Stat-name mapping` doctrine ("default assumption is that stat names match one-to-one"). The planner does NOT introduce kebab-case, camelCase, or any other convention. The Prometheus exposition format also accepts snake_case, so no escape-or-translation is needed at the exposition layer. **Recommendation:** verbatim Envoy naming throughout; the per-class counters are `downstream_rq_2xx`, `downstream_rq_3xx`, `downstream_rq_4xx`, `downstream_rq_5xx` (lowercase `xx`, not `XX`).

**Signpost 4 — `ClusterStats` struct factoring (fold into `Cluster` vs separate struct).** D15.3.b/c land 4 stats on `Cluster` (`cx_total` from 06.1, `cx_active`, `upstream_rq_total`, `upstream_rq_5xx`). The planner picks at PLAN-time between (a) appending fields to `Cluster` directly (mechanical; matches 06.1 D4.1's pattern); (b) introducing a `ClusterStats` substruct on `Cluster` (cleaner; reusable if external consumers need the stats handle). **Recommendation:** (a) — match the 06.1-landed shape; the fields are `Arc<Counter>` / `Arc<Gauge>` clones so the per-call overhead is identical, and (b)'s reusability benefit is zero in 06.3 since no external consumer of Cluster's stats exists in the planned scope. Symmetric reasoning applies to `Listener`'s D15.3.b/d fields.

**Signpost 5 — Cluster-side `upstream_rq_5xx` attribution: router proxy-arm vs HCM dispatch site.** Two candidate increment sites for `upstream_rq_5xx`: (a) `crates/envoy-http1/src/router.rs::write_proxied_response` (the protocol-agnostic helper landed at 04.3 D5; sees the upstream `Response` after dispatch); (b) the HCM's `BuildOutcome::Proxy` arm at `crates/envoy-http1/src/hcm.rs:189-288` (the dispatch-site that calls `Client::connect` + `send_request` + `write_proxied_response`). **Recommendation:** (a) — the increment lives where the upstream `Response` is in scope and where the H1/H2 dispatch arms have already converged, so the per-protocol increment-site duplication of (b) is avoided. The new `cluster: &ClusterHandle` parameter on `write_proxied_response` is the mechanical cost.

Per the same signpost, the **fixture 0011 5xx-path** is the planner's call: (a) `direct_response: 503` configured route (does NOT increment `cluster.<name>.upstream_rq_5xx` because direct_response paths bypass the router proxy-arm); (b) router-proxy to a synthetic-backend that returns 5xx (DOES increment `cluster.<name>.upstream_rq_5xx`); (c) hybrid — separate routes for 4xx-class (direct_response: 404) and 5xx-class (router-proxy synthetic 503). **Recommendation:** (c) — the 4xx-class is exercised cleanly by direct_response 404 (and the HCM's `downstream_rq_4xx` increments while `upstream_rq_*` does not); the 5xx-class is exercised by router-proxy 503 (and both `downstream_rq_5xx` and `upstream_rq_5xx` increment).

**Signpost 6 — Listener accept-failure counter scope (ECONNRESET-only or all errors).** Per §3 D15.3.d the recommendation is "all accept errors" — Envoy's `downstream_cx_accept_failed` counter does not break down by errno. The per-errno breakdown defers to whichever phase first surfaces value in disambiguation (e.g., a fault-injection test that distinguishes EMFILE from ECONNRESET would benefit, but no such test is anticipated through phase 08). **Recommendation:** all accept errors increment the counter; the per-errno detail surfaces in `tracing::warn!` lines for log-level debuggability.

**Signpost 7 — Gauge value-may-be-zero in Prometheus exposition + admin listener carve-out.** Two related questions: (a) at scrape time, gauges that have never incremented (or have decremented to zero) emit as `<name> 0` in the Prometheus exposition — the harness's body-rule must accept this shape (currently `0` is a valid Prometheus exposition number; existing 06.1 BodyRule should accept it). (b) does the admin listener's gauge contribute to `listener.<name>.downstream_cx_active`? — the harness's ephemeral admin-scrape connection inflates-and-deflates the gauge on the admin listener; if the scrape reads the gauge mid-emission, the value is non-zero (an artifact of the scrape itself).

**Recommendation for (a):** the harness's body-rule already accepts `<name> 0` as a valid value-exact match for any value-exact assertion; no extension needed.

**Recommendation for (b):** scope the listener-side gauge to the data-path listener only (not the admin listener), OR ensure the scrape happens after the admin listener's accept-and-handle epilogue — the scrape itself completes its handle before the response body emits, so by the time the client reads the body, the admin listener's gauge has already decremented. Recommendation per signpost 7: **scope to data-path listeners**; the gauge's namespace is per-listener-name, and the admin listener's `cx_active` is uninteresting to operators (admin scrape traffic is bursty by design).

**Signpost 8 — Fixture 0011 expectation extension shape (additive only or replace).** D17.3 extends `expectations.yaml`; the planner picks at PLAN-time between (a) **additive** — the existing 06.1 representative-subset assertion stays, and the comprehensive set adds atop; (b) **replace** — the comprehensive set replaces the representative subset. **Recommendation:** (a) — purely additive. The 06.1-landed assertion already includes the 3 representative counters (`listener.<name>.downstream_cx_total`, `cluster.<name>.upstream_cx_total`, `http.<stat_prefix>.downstream_rq_total`); the comprehensive set adds the 7+ new stats (per §2). Removing the 06.1 assertion has no benefit and adds a regression risk if the comprehensive set's value-exact dispositions shift the representative-subset's emission.

**Signpost 9 — Harness `BodyRule::PrometheusExposition` gauge handling.** Per §3 D18.3 the recommendation is option (1) — extend the existing `BodyRule::PrometheusExposition` variant additively with `value_exact: HashMap<String, u64>`, `value_must_be_zero: HashSet<String>`, and `value_present_only: HashSet<String>` fields. Backwards-compat: if 06.1 D6.1 shipped only the name-set assertion (no value assertion), 06.3 D18.3 adds value assertion as a new optional surface; existing usage (06.1's representative subset asserted as name-present-only) continues to work. **Recommendation:** option (1); 2 new harness unit tests; ~50 LoC max.

**Signpost 10 — Parent-06 state-6 commit message format finalization.** Per §9 below the format is `phase 06.3: <06.3 title> [parent 06 done] [ADR-NNNN, ...]`. The planner finalizes the title text at PLAN-time; recommended title shape: `phase 06.3: comprehensive stats wiring + 05.3 I1 closure + parent-06 close [parent 06 done]`. Mirror precedent: 05.3's title was `phase 05.3: HTTP/2 upstream origination + router H2-arm + fixture 0010 [parent 05 done] [ADR-0028]` (per the recent commit `82c26b8`). The bracketed ADR list enumerates the actual ADRs landed across the parent-06 execution arc — at minimum `ADR-0029` (split decision); plus any conditional ADRs that landed during 06.x execution (likely zero per the recommended posture).

**Inherited signposts from parent-06 SPEC §6 — re-binding for 06.3:**

The five cross-sub-phase architectural invariants from parent SPEC §6 (consumers register and increment; envoy-stats exports primitives only; counter/gauge ops lock-free; representative subset → comprehensive; access-log line counter increments at queue-enter time; parent close at THIS sub-phase's state-6 commit) are reproduced in §1 above as the load-bearing invariants of 06.3. The planner verifies at Task-1 time that the 06.1-landed `envoy-stats` registry surface and the 06.2-landed access-log dispatch site preserve these invariants; if any deviation surfaced during 06.1 / 06.2 execution (e.g., a foundations grant via in-execution ADR), the 06.3 planner adapts at PLAN-time and notes the deviation in PROGRESS Task 1.

**LoC-budget reality check at PLAN-write time.** Parent-06 SPEC §3 / ADR-0029 projected 06.3 at "~1200 LoC, ~10 tasks." This SPEC's §3 D14.3–D20.3 deliverable estimates total approximately **~770 LoC** (~130 D14.3 + ~400 D15.3 + ~80 D16.3 + ~30 D17.3 + ~50 D18.3 + ~0 D19.3 + ~0 D20.3 + ~80 review/state-6 overhead) — comfortably under the parent's projection and the §6.1 split-gate. The planner does NOT need to nest-split; per parent-05 SPEC §5 rule "do not nest-split a sub-phase that was itself produced by a split" applies here. The PLAN-write planner records the LoC-reality-check posture in PROGRESS Task 1.

---

## 8. ADR projection

**No ADRs are projected for 06.3 state-2.** Per parent SPEC §7, ADR-0029 already landed at parent-06 state-2 (split decision); conditional ADR-0030 (foundations grant for `time = "0.3"` or `async_trait = "0.1"`) and ADR-0031 (Cargo.lock cadence ratification, conditional on ADR-0030 actually landing) stay available but are NOT pre-projected. **06.3's projected ADR landings are zero.**

The `Http2ClusterFromHttp1Listener` parse-time validator gate (D14.3) is a mechanical extension of the existing envoy-config validator pattern (mirrors 05.2's `Http2OverTlsNotSupported`, 05.3's `MutuallyExclusiveExplicitHttpConfig`, 05.1's `ClusterDnsResolutionFailed` shapes — all landed without ADR cover) — no doctrine call. The comprehensive-stats wiring (D15.3) extends 06.1's representative subset using the established `Counter` / `Gauge` primitives — no doctrine call. The `Stat-name mapping` extension (D16.3) follows the established BEHAVIOR_CONTRACT.md pattern — no doctrine call. The fixture extension (D17.3) reuses the 06.1-landed `BodyRule::PrometheusExposition` body-rule surface — no doctrine call. The harness extension (D18.3) is purely additive on the existing variant — no doctrine call. The parent-06 close-out (D19.3) is mechanical per the established phase-04 / phase-05 close-out shape — no doctrine call.

If an unforeseen design ambiguity surfaces during 06.3 execution per D-3.5 (decisions are written, not remembered), the planner appends the next-sequential available ADR at the time it lands. The DECISIONS.md ledger head before 06.3 Task 1 is one of:

- **ADR-0029** — if neither ADR-0030 nor ADR-0031 landed during 06.1 / 06.2 execution.
- **ADR-0030** — if only ADR-0030 landed (a foundations grant during 06.1 or 06.2; recommendation per parent §7 was no foundations grants, so this is unlikely).
- **ADR-0031** — if both ADR-0030 and ADR-0031 landed (conditional cascade).

Per parent §7's projection, recommendation is that conditional ADRs did not land during 06.x execution, so the most likely ledger head at 06.3 entry is **ADR-0029**. The 06.3 planner cross-checks the actual ledger head at Task 1 by reading the latest ADR in `docs/envoy-rust/DECISIONS.md` and adopts whatever the next-sequential available number is.

**Possible additional ADRs** (not anticipated; listed for projection completeness):

- **ADR-NEXT — Stat-name validation rule** if D15.3 surfaces an ambiguity between Envoy's documented naming and envoy-rust's hand-rolled emission (e.g., Envoy emits `cluster.<name>.upstream_rq_total` but documents it as `cluster.<name>.upstream_rq_completed` — the planner verifies at Task 1 time by cross-referencing the Envoy v1.33.0 docs at `https://www.envoyproxy.io/docs/envoy/v1.33.0/configuration/observability/statistics`). **Not anticipated** — the names in §1 / §2 are taken from the Envoy docs verbatim.
- **ADR-NEXT — Prometheus exposition format edge case** if D18.3's harness extension surfaces a value-may-be-zero or value-may-be-NaN edge case in the exposition format. **Not anticipated** — per signpost 7 the `<name> 0` shape is valid Prometheus and the harness body-rule accepts it.
- **ADR-NEXT — `Http2ClusterFromHttp1Listener` cross-validation scope expansion** if D14.3 surfaces additional listener × cluster mismatch shapes (e.g., TLS+H1 listener × cleartext-H2 cluster). **Not anticipated** — phase 06 does not engage TLS or H2 surfaces beyond the I1 closure; cross-validation expansion defers to whichever phase first ships TLS+H2.

If any of these fire, they take the next-sequential available ADR number at the time they land. The 06.3 planner may also find the need for sub-phase-local ADRs; those land at the relevant Task-N commit per D-3.5.

---

## 9. Commit message format

The 06.3 phase-done commit (state-6 close-out commit) ALSO closes parent phase 06 in a single commit (mirrors phase 04's `e626862`-shape close-out where the 04.3 commit closed parent 04, and phase 05's `82c26b8`-shape close-out where the 05.3 commit closed parent 05). Format includes the `[parent 06 done]` tag in the title per `BOOTSTRAP_PROMPT.md` §5.3.

**Format per `BOOTSTRAP_PROMPT.md` §5.3** (reproduced verbatim from the prompt):

```
phase NN: <title> [ADR-NNNN, ADR-MMMM, ...]

<summary — 1–3 sentences>

Differential surface: <what new/existing fixtures are now green>
Conformance: <what conformance suites were run and their pass rate>
```

**06.3-specific shape** (the `[parent 06 done]` tag attaches to the title):

```
phase 06.3: comprehensive stats wiring + 05.3 I1 closure + parent-06 close [parent 06 done] [ADR-NNNN, ...]

envoy-config validator gains a new ConfigError::Http2ClusterFromHttp1Listener
parse-time gate at crates/envoy-config/src/bootstrap.rs::validate that
rejects HTTP1 (and AUTO) listener configurations whose routes target a
cluster with typed_extension_protocol_options.HttpProtocolOptions.
explicit_http_config.http2_protocol_options set; closes 05.3 REVIEW I1
substantively (silent H1-listener × H2-cluster misnegotiation per
ADR-0028's option-(B) deferral). Comprehensive Envoy stat tree wired at
HCM/router/listener/cluster: per-response-class HCM counters
(downstream_rq_{2xx,3xx,4xx,5xx}); connection-lifetime gauges
(listener.<name>.downstream_cx_active; cluster.<name>.upstream_cx_active);
upstream-side router counters (cluster.<name>.upstream_rq_{total,5xx});
access-log line counter (http.<stat_prefix>.access_logs_total) at the
queue-enter site so emission failures don't deflate the count; listener
accept-failure counter (listener.<name>.downstream_cx_accept_failed).
BEHAVIOR_CONTRACT.md Stat-name mapping table grows 7 new rows per the
value-exact / name-required-value-may-differ disposition rules.
Fixture 0011-admin-stats-prometheus expectations.yaml extends to assert
the comprehensive stat set across all 4 status classes (2xx/3xx/4xx/5xx).

Closes parent phase 06 (Observability foundations). Sub-phases:
- 06.1 (commit <SHA>): envoy-stats foundation + envoy-admin HCM-backed
  listener migration + Prometheus exposition + fixture 0011 + representative
  stats wiring.
- 06.2 (commit <SHA>): envoy-accesslog foundation + Envoy default-format
  emitter + HCM access_log: schema + FileSink + fixture 0012.
- 06.3 (this commit): comprehensive stats wiring + 05.3 REVIEW I1 closure
  + parent-06 close [ADR-NNNN].

Differential surface: tests/fixtures/0001-tcp-echo green (unchanged);
  0002-static-admin-ready green (unchanged through 06.1 admin migration);
  0003-tcp-proxy through 0006-tls-sni green (unchanged from 05.4 baseline);
  0007-http1-direct-response through 0010-http2-router-upstream green
  (unchanged from 05.3 baseline); 0011-admin-stats-prometheus GREEN with
  comprehensive expectations.yaml extended in 06.3; 0012-access-log-file-sink
  green (unchanged from 06.2).
Conformance: tests/conformance/h2spec at ≥95% pass (gate landed at 05.2
  D7; unchanged in 06.x; state-4 re-run confirms no regression).
```

The `[parent 06 done]` tag attaches to the title verbatim, mirroring 05.3's `[parent 05 done]` tag at commit `82c26b8` and 04.3's `[parent 04 done]` tag at commit `e626862`. The bracketed ADR list enumerates ADRs landed across the parent-06 execution arc — at minimum **ADR-0029** (split decision; landed at parent-06 state-2); plus any conditional ADRs that landed during 06.1 / 06.2 / 06.3 execution. Per the recommended no-foundations-grant posture, the bracket is `[ADR-0029]` only.

---

## 10. State-machine commit (the parent-06 close-out)

This section enumerates what the 06.3 state-6 commit does to disk. Mirrors 05.3 SPEC §10 / 04.3 SPEC §10 in shape; differs in the specific phase-id transitions.

**At the 06.3 state-6 phase-done commit:**

1. **`docs/envoy-rust/ROADMAP.md`** — flip row `06.3` `status: in-progress` → `status: done`. **AT THE SAME COMMIT:** flip parent row `06` `status: in-progress` → `status: done` per the ROADMAP-schema invariant (rows `06.1` and `06.2` are already `done` from their own state-6 commits earlier in the parent's execution; landing 06.3 done is the last sub-phase flip and triggers the parent flip).
2. **`docs/envoy-rust/STATE.md`** —
   - **Active phase block:** transitions from "Phase 06.3 (06.3-stats-wiring-and-close) is DONE as of this commit. The closing sub-phase of parent-06 lit up the comprehensive Envoy stat tree at HCM/router/listener/cluster sites... [REVIEW.md verdict and rollover summary]" framing (mirroring 05.3 STATE's framing at predecessor `82c26b8`'s state) to "Phase 07 (07-<slug>) state 1" framing — slug consistent with `BOOTSTRAP_PROMPT.md` §8 row 07 (*"Filter chain framework: iteration protocol, per-route config, extension registry"*); expected slug `07-filter-chain-framework` or similar — the planner uses whatever slug phase-07 brainstorm chooses; lifecycle state advances to phase 07 lifecycle state 1 (phase-07 directory does not exist; SPEC.md does not exist).
   - **Last commit block** updates with the 06.3 state-6 commit SHA + summary.
   - **Next expected skill block** advances to `superpowers:brainstorming` scoped to phase 07's `BOOTSTRAP_PROMPT.md` §8 row 07 charter. Standing-context bullets list (a) the 05.3 REVIEW I1 closure at 06.3 D14.3; (b) the 7 stats added in 06.3 with their dispositions; (c) any open carryforwards remaining at 06.3 close (per §1 above's enumeration).
   - **Notes section** gains a "Phase-06.3 rollovers" subsection per the established cadence — enumerates 06.3 REVIEW.md verdict (anticipated: Approved with M-track follow-ups at most), in-phase closures (05.3 REVIEW I1 closed at D14.3 / Task 1; any 05.2 / 06.1 / 06.2 carryforwards opportunistically closed during 06.3), awareness-only items, and the parent-06 close-out summary. Adds a "Phase-06 ADR ledger (final)" subsection — confirms ADR-0029 landed at parent-06 state-2; confirms ADR-0030 / ADR-0031 stayed unused (assuming the recommended posture held); lists any unforeseen ADRs landed during 06.x execution.
3. **`docs/envoy-rust/DECISIONS.md`** — **no anticipated edits at the 06.3 state-6 commit.** Per §8 above, no new ADRs are projected for 06.3 state-2; if an unforeseen ADR fires during execution per D-3.5, the planner appends the next-sequential ADR at the time it lands (NOT at the state-6 commit). Ledger head: ADR-0029 (or higher if conditional ADRs landed).
4. **Parent SPEC at `docs/envoy-rust/phases/06-observability/SPEC.md`** — NOT edited (preserved unedited per D-3.4 / D-3.5; mirrors parent-04 / parent-05 SPECs' posture at their close-outs).
5. **Sub-phase SPECs at `docs/envoy-rust/phases/06.{1,2}/SPEC.md`** — NOT edited (closed at their own state-6 commits).
6. **`docs/envoy-rust/phases/06.3-stats-wiring-and-close/{PLAN,PROGRESS,REVIEW}.md`** — landed as part of the 06.3 lifecycle (PLAN at state-2 close-out; PROGRESS appended through tasks 1-N; REVIEW at state-5 close-out); the state-6 commit is the next-and-final commit after REVIEW lands. The 06.3 REVIEW.md carries forward whatever Minor / Important findings remain to phase 07+.

**The next session after 06.3 state-6** enters phase 07 state 1 — runs `superpowers:brainstorming` scoped to phase 07's `BOOTSTRAP_PROMPT.md` §8 row 07 charter, lands `docs/envoy-rust/phases/07-<slug>/SPEC.md`, and the cycle continues.

**Execution invariants (unchanged from parent-04 / parent-05):**
- The parent-06 state-6 close-out happens at 06.3's state-6 commit (the last sub-phase's commit also flips parent row 06 to `done`), per the ROADMAP-schema invariant in `BOOTSTRAP_PROMPT.md` §4.1 and the established phase-04 / phase-05 close-out shape.
- 06.3 honors the phase-done gate from `BOOTSTRAP_PROMPT.md` §7.5 in full at its state-4.
- 06.3 produces its own REVIEW.md at state-5 per `superpowers:requesting-code-review`.
- The 06.3 state-6 commit's title carries the `[parent 06 done]` tag per §9 above.
