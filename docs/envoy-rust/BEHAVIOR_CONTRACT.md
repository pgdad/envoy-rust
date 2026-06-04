# envoy-rust Behavior Contract

> This document is the canonical definition of what "behaviorally equivalent
> to upstream Envoy" means for the differential test harness. Every fixture's
> `expectations.yaml` is derived from the rules here. Divergences from the
> contract are resolved by either (a) fixing the implementation, or (b) landing
> an ADR that updates the contract — never both silently (doctrine D-3.3).

---

## Equivalence matrix

| Dimension | Required equivalence |
|---|---|
| Response status | Exact |
| Response body | Byte-exact for deterministic handlers; semantically equal for filter-modified bodies |
| Response headers | Set-equal modulo documented allow-list (`server`, `date`, timing/identity headers explicitly listed) |
| Response trailers | Set-equal under the same allow-list discipline |
| HTTP/2 & HTTP/3 framing | Structurally equivalent (same frame types/order on equivalent events); not byte-equal |
| Access log records | Semantically equal after field-mapping |
| Stats | Names match Envoy's documented stat tree; presence required; values exact on deterministic flows |
| xDS wire behavior | ADS message sequences match the protocol state machine; effective-config diff on identical snapshots |
| Timing | Not compared by default; a phase may opt in to latency bounds |

---

## Response body — no-healthy-upstream synth-503

> Authored per phase 12.2 SPEC §2.2 + ADR-0037. The H1 HCM per-request
> dispatch path returns a synthetic 503 when `Cluster::pick()` yields
> `None` — both proxies emit it with identical wire shape on the same
> active-HC eviction.

| Reachability path | Equivalence disposition |
|---|---|
| `pick() -> None` (HCM H1 `hcm.rs:582` arm; cluster has `health_checks` configured AND all endpoints unhealthy AND panic not engaged) | Status 503; body byte-exact `no healthy upstream` (19 bytes, hex `6e 6f 20 68 65 61 6c 74 68 79 20 75 70 73 74 72 65 61 6d`, NO trailing newline); 5 standard HTTP/1.1 response headers `{server, date, content-length: 19, content-type, connection}`. Emitted via the dedicated `synth_no_healthy_upstream` helper adjacent to `synth_status` — the helper is used ONLY on this path. The connect-fail 502 + send-fail 502 paths keep `synth_status`'s empty body (phase-04.3 wire shape). |
| `max_connections` cap overflow OR `max_pending_requests: 0` reject (HCM H1 `hcm.rs:542`/`hcm.rs:569` `PoolError::Overflow` / `PoolError::PendingOverflow` arms; 15 D5 / ADR-0043 §6.2 finding 3) | Status 503; body byte-exact `upstream connect error or disconnect/reset before headers. reset reason: overflow` (81 bytes, NO trailing newline); header `x-envoy-overloaded: true` — the wire surfacing of Envoy's `UO` response flag, which is otherwise **access-log-only** (no `%RESPONSE_FLAGS%` wire surface). **Equivalence = byte-exact body + status.** Emitted via the dedicated `synth_overflow` helper adjacent to `synth_no_healthy_upstream` (used ONLY on these two overflow arms; H2 sibling `synth_h2_overflow`, Task 5). envoy-rust emits 6 headers `{server, date, content-length: 81, content-type, connection, x-envoy-overloaded}` — Envoy's set is `{x-envoy-overloaded, content-length: 81, content-type, date, server}` (no `connection`); the extra `connection` header is **allow-listed** by the harness (the 0019/0022 synth-503 precedent). The Envoy-only `circuit_breakers.*` sibling gauges (`{default,high}.{cx_pool_open, rq_open, rq_pending_open, rq_retry_open}` + `high.cx_open`) are NOT emitted at phase-15 scope (deferred). |

---

## Header allow-list

> **To be filled per-phase as needed.**
>
> The header allow-list enumerates response headers whose values may differ
> between upstream Envoy and envoy-rust without the fixture being red.
> Membership on this list must be justified (e.g. `server` carries an
> implementation-identifying string, `date` is wall-clock non-determinism).
> Timing and identity headers must be listed explicitly — no wildcards.
>
> Every phase that introduces a new header surface (HTTP/1.1, HTTP/2, HTTP/3,
> access-log header filter, router header manipulations, etc.) updates this
> section or produces an ADR explaining why the defaults suffice.

| Header | Equivalence | Rationale |
|---|---|---|
| `server` | name-required, value-may-differ | Implementation-identifying. Both proxies emit `server: <name>`; envoy-rust's HCM default is `server: envoy-rust`, Envoy's default is `server: envoy`. When HCM `server_name` config field is set (deferred to phase 05+ per parent SPEC §4), value tightens to exact-match on both sides. |
| `date` | name-required, value-may-differ | Wall-clock non-determinism (RFC 7231 §7.1.1.2 IMF-fixdate format). Both proxies stamp the response with the wall-clock at response-write time; values diverge because the two proxies write at slightly different instants. |
| `x-envoy-upstream-service-time` | name-required, value-may-differ | Per-request upstream-side latency in milliseconds. envoy-rust measures from `Client::connect` start to last-response-byte-read end (computed in the router proxy arm before the response is written downstream). Envoy emits the same header (its semantics are documented at `https://www.envoyproxy.io/docs/envoy/v1.33.0/configuration/http/http_filters/router_filter#x-envoy-upstream-service-time`). Only present on responses that proxied through to an upstream cluster (NOT on `direct_response` paths — that's 04.1's surface where this header is never emitted). Both proxies emit on every router-proxy response; values diverge by measurement. Lands in 04.3 per phase-04 parent SPEC §2 + 04.3 SPEC §2. |
| `x-envoy-attempt-count` | value-exact (total upstream attempts; `2` after one retry) | Present on the downstream response **only** when the matched VirtualHost sets `include_attempt_count_in_response: true` (ADR-0045 finding L5/L6 — NOT automatic; absent without the flag regardless of whether a `retry_policy` is configured). Injection reuses the `x-envoy-upstream-service-time` machinery at the retry-loop exit: H1 at `crates/envoy-http1/src/hcm.rs` (constant defined in `crates/envoy-http1/src/router.rs`); H2 at `crates/envoy-http2/src/hcm.rs`. Exercised by fixture 0024 with `include_attempt_count_in_response: true` on both proxy configs; both probes assert `x-envoy-attempt-count: 2` (one retry each). **Phase-17 L11 extension:** the header IS also emitted on synthesized overflow local replies (value `1` — one admitted attempt even though no upstream request was sent) when the vhost flag is set — verified empirically vs Envoy v1.33 at the phase-17 §6.2 verification (closes the phase-16 review's M16-3); fixture 0025 asserts it on all three probes (values 1/2/1) including the overflow local reply. |

**Phase 08.1 D1 dedupe note:** With phase 08.1's case-insensitive dedupe in
`crates/envoy-admin/src/handler.rs::serialize_response`, a future endpoint may
legitimately set its own `cache-control` (or any of the other 3 standard
headers). The dedupe guarantees no duplicate header lands on the wire; only one
instance of the header name appears in the response, and the caller-supplied
value wins.

---

## Stat-name mapping

> **To be filled per-phase as needed.**
>
> Upstream Envoy emits stats under a documented, hierarchical name tree.
> envoy-rust must emit the same tree. Mapping entries are recorded here only
> when envoy-rust must produce a stat under a different internal label that
> needs to be projected back to the Envoy-canonical name at the stats sink.
> The default assumption is that stat names match one-to-one.
>
> Every phase that introduces a new stat family (connection counters, HTTP
> response-code counters, cluster health counters, filter-local stats, admin
> stats, etc.) updates this section or produces an ADR explaining why the
> defaults suffice.

**06.1 initial entries:**

| Stat name | Equivalence | Rationale |
|---|---|---|
| `listener.<name>.downstream_cx_total` | value-exact | Counter; one increment per accepted TCP connection on the listener. envoy-rust internal label matches Envoy's documented name one-to-one. Both proxies emit on every accept; under deterministic harness load (a fixed connection count) the values are byte-equal. |
| `cluster.<name>.upstream_cx_total` | value-exact (H1 + H2 clusters under the harness's single-downstream-keep-alive-conn driver); name-required, value-may-differ (TCP-proxy clusters — TCP pool defers to a follow-up phase per parent-13 SPEC §4) | Counter; one increment per established upstream TCP connection at pool-create time. Under H1/H2 pooling (phase 13), both proxies emit the same small N under deterministic load: 1 if the workload fits in one pooled connection (the fixture 0020 + 0021 baseline shape); more if the harness exceeds `max_concurrent_streams` or `max_connections`, in which case both proxies still emit identical N because the cap is bilaterally configured. The increment site lives in the H1/H2 pool's `acquire()` connect-on-miss branch (one source of truth per protocol; H1 at `crates/envoy-http1/src/pool.rs::H1Pool::acquire` per 13.1; H2 at `crates/envoy-http2/src/pool.rs::H2Pool::acquire` per 13.2). The TCP-proxy increment at `crates/envoy-tcp/src/lib.rs:108` remains per-call until TCP pooling lands; existing TCP fixtures (`0001/0003/0004/0005/0006`) carry the pre-13.2 name-required, value-may-differ disposition under the carve-out (their `expectations.yaml` assertions are presence-only — the tightened value-exact disposition is satisfied trivially on the H1/H2 side, the TCP side remains presence-only via the carve-out). The value-exact disposition is **conditional on the harness driver issuing multiple requests over a single downstream keep-alive conn** (per parent-13 SPEC §6.2 item-iv; else N upstream conns per N downstream conns regardless of pool — the harness's `Driver::Http1KeepAlive` from 13.1 D10 makes this configurable per-fixture). **This row tightening fully closes 06.3 REVIEW I2 (b)** — combined with the 13.1 fixture-0020-driven I2 (a) closure (per-class HCM `downstream_rq_{2,3,4,5}xx` + cluster `upstream_rq_5xx` bilateral assertions), **the full 06.3 REVIEW I2 carryforward is CLOSED at the phase-13 close.** |
| `http.<stat_prefix>.downstream_rq_total` | value-exact | Counter; one increment per HCM-handled request (any response code; any method). Both proxies emit on every request; under deterministic harness load (a fixed request count) the values are byte-equal. The `<stat_prefix>` segment is sourced from `HttpConnectionManagerConfig.stat_prefix`. |

**06.3 entries:**

| Stat name | Equivalence | Rationale |
|---|---|---|
| `http.<stat_prefix>.downstream_rq_2xx` | value-exact | Counter; one increment per 2xx HCM-handled response. Fires at the factored access-log dispatch site AFTER all 5 writer arms have populated `response_status_for_log`. Status-class bucketing via integer division `status / 100`. Both proxies emit on every 2xx response; under deterministic harness load the values are byte-equal. Sibling: `downstream_rq_3xx/4xx/5xx`. |
| `http.<stat_prefix>.downstream_rq_3xx` | value-exact | Counter; one increment per 3xx HCM-handled response. See `downstream_rq_2xx`. |
| `http.<stat_prefix>.downstream_rq_4xx` | value-exact | Counter; one increment per 4xx HCM-handled response. See `downstream_rq_2xx`. |
| `http.<stat_prefix>.downstream_rq_5xx` | value-exact | Counter; one increment per 5xx HCM-handled response. See `downstream_rq_2xx`. Fires on direct_response 5xx, proxy synth-502/503 (no-endpoint, connect-fail, send-fail), AND upstream-emitted 5xx responses — the per-class counter is symmetric on `response_status_for_log`, agnostic to synth-vs-proxy origin. |
| `http.<stat_prefix>.access_logs_total` | value-exact | Counter; incremented at queue-enter time via `Counter::add(N)` where N is the configured sink count. Fires BEFORE the per-sink `sink.emit(...).await` per parent-06 SPEC §6 Rule 4 (fire-and-forget emission). Both proxies emit one increment-by-N per request when access_log is configured; 0 when no access_log is configured. |
| `http.<stat_prefix>.access_logs_failed` | value-exact (0-failures case) | Counter; incremented inside the per-sink error arm before `tracing::warn!`. Both proxies emit 0 under the deterministic-success harness; non-zero values are only seen under sink-emission failure (file-path permission issues, disk full, etc.). 06.3 verifies the 0-case; future fixtures could exercise emission failure deterministically. |
| `listener.<name>.downstream_cx_active` | value-exact (deterministic close) | Gauge; incremented on every accepted TCP connection, decremented at the per-connection task's epilogue (Drop on success and error paths uniformly). Scope: data-path listeners only — admin listener excluded via code-path (envoy-bin's admin listener uses `tokio::net::TcpListener` + `envoy_admin::serve` directly, not `envoy_listener::Listener::bind`). Terminal-zero gauge: returns to 0 after all per-connection tasks complete and Drop fires. The harness's post-request settle window (50-100ms) gives the gauge time to return to 0 before the scrape captures the value. |
| `listener.<name>.downstream_cx_accept_failed` | value-exact (0-failures case) | Counter; incremented inside the listener accept loop's `Err(_)` arm BEFORE `tracing::warn!`. Signpost 6: all accept errors count (no carve-outs). Both proxies emit 0 under harness conditions (the harness produces well-formed connections; OS accept errors are extremely rare in lab settings). |
| `cluster.<name>.upstream_cx_active` | value-exact (deterministic close) | Gauge; incremented at the HCM proxy-arm and TCP-proxy dial sites via the `ConnGaugeGuard` RAII (architecture decision 13). Decrement fires via `Drop` at scope exit, covering both success and error close paths uniformly. Terminal-zero gauge; same settle-window considerations as the listener gauge. |
| `cluster.<name>.upstream_rq_total` | value-exact | Counter; one increment per upstream response received (NOT per upstream connect attempt). H1: fires at `write_proxied_response` function prologue; H2: fires inline at the post-dispatch success site in `finalize_h2_stream`. Synth-502 paths (envoy-rust-side 502 on connect-fail) do NOT increment — these are not upstream responses. Both proxies emit one increment per `upstream_resp` received. |
| `cluster.<name>.upstream_rq_5xx` | value-exact | Counter; conditional sibling of `upstream_rq_total`, increments when `upstream_resp.status / 100 == 5`. Synth-502 paths bypass for the same reason as `upstream_rq_total`. |

**08.2 entries (drain machinery):**

| Stat name | Equivalence | Rationale |
|---|---|---|
| `server.live` | value-exact | Gauge; `1` when `DrainState::current() == Live`; `0` otherwise (HealthcheckFailing and Draining both emit `0`). Updated inline at the `DrainState::{fail_healthcheck, ok_healthcheck, drain}` CAS-success sites (one source of truth — NOT polled). Initial value `1` at process start. Both proxies emit on every snapshot. |
| `server.state` | value-exact (Live=0 baseline; Draining=2 post-drain) | Gauge; discriminant of `DrainStage` (`Live=0`, `HealthcheckFailing=1`, `Draining=2`). The `#[repr(u8)]` on `DrainStage` makes the discriminant load-bearing for the gauge value. Updated inline at the same CAS-success sites as `server.live` (one source of truth). Initial value `0` at process start. Fixture 0015 asserts the post-drain value `2`. |
| `listener_manager.total_listeners_active` | value-exact | Gauge; count of currently-active data-plane listeners (HCM + tcp_proxy paths going through `envoy_listener::Listener::bind`/`serve`). Echo path (fixture 0002 only) + admin path use `tokio::net::TcpListener` directly and are naturally excluded. RAII-guarded at `Listener::serve` entry (inc) / exit (dec); decrement fires AFTER drain completes and AFTER stragglers join. Mirrors the 06.3 `listener.<name>.downstream_cx_active` gauge pattern but is global (not per-listener-named); registered idempotently inside `Listener::bind`. |

**09 entries (LocalRateLimit filter):**

| Stat name | Equivalence | Rationale |
|---|---|---|
| `http_local_rate_limit.<stat_prefix>.enabled` | value-exact | Counter; one increment per decode-side filter invocation when the filter is enabled. At phase-09 scope `filter_enabled` defaults to always-on (100%); per upstream Envoy parity `enabled` increments unconditionally on every `decode_headers` call. Both proxies emit one increment per request reaching the filter. |
| `http_local_rate_limit.<stat_prefix>.ok` | value-exact | Counter; one increment per `try_acquire` success (token consumed; request allowed to continue). Both proxies emit one increment per under-limit request. |
| `http_local_rate_limit.<stat_prefix>.rate_limited` | value-exact | Counter; one increment per `try_acquire` failure (no tokens available; request would-be-rate-limited). At phase-09 scope `filter_enforced` defaults to always-on (100%) so `rate_limited` counts coincide with `enforced` — but the upstream-Envoy semantic distinguishes "would-be-rate-limited" (`rate_limited`) from "actually-rate-limited" (`enforced`). Both proxies emit one increment per over-limit request. |
| `http_local_rate_limit.<stat_prefix>.enforced` | value-exact | Counter; one increment per request actually rate-limited (429 response emitted via `Decision::StopAndSend`). At phase-09 scope `enforced == rate_limited` because `filter_enforced` defaults to always-on; the two stat names track for upstream-Envoy parity. When a future phase lands runtime-fractional-percent `filter_enforced` overrides, the two counters diverge. Both proxies emit one increment per 429 emission. |

**10 entries (RBAC filter):**

| Stat name | Equivalence | Rationale |
|---|---|---|
| `http.<hcm_stat_prefix>.rbac.allowed` | value-exact | Counter; one increment per request allowed under the primary rules — either by explicit Allow-action policy match OR by Deny-action no-match (per phase-10 SPEC §5.6 decision matrix). Both proxies emit one increment per allowed request at the decision site in `RbacFilter::decode_headers` (synchronously, before `Decision::Continue`). Upstream Envoy v1.33 emits the same name at the same `http.<hcm_stat_prefix>.rbac.*` namespace per the §6.2 empirical verification at PLAN-write. |
| `http.<hcm_stat_prefix>.rbac.denied` | value-exact | Counter; one increment per request denied under the primary rules — either by explicit Deny-action policy match OR by Allow-action no-match. Both proxies emit one increment per denied request at the decision site in `RbacFilter::decode_headers` (synchronously, before constructing the `Decision::StopAndSend(FilterResponse)` 403). The `allowed + denied == total_requests_to_filter` invariant holds per SPEC §2.1 (each counter incremented at its own fire site; no double-counting). |

**11 entries (Fault filter):**

| Stat name | Equivalence | Rationale |
|---|---|---|
| `http.<hcm_stat_prefix>.fault.aborts_injected` | value-exact | Counter; one increment per request the filter aborts (the header gate matches AND the deterministic percentage selects at 100%). Both proxies emit one increment per aborted request at the abort decision site in `FaultFilter::decode_headers` (synchronously, before constructing the `Decision::StopAndSend(FilterResponse)` abort). Never increments on pass-through (gate miss OR 0% percentage). Upstream Envoy v1.33 emits the same name at the `http.<hcm_stat_prefix>.fault.*` namespace per the §6.2 empirical verification at phase-11 state-2 PLAN-write (`http.ingress_http.fault.aborts_injected: 4` after 4 aborts). The `<hcm_stat_prefix>` is sourced from the parent HCM's `stat_prefix` (the fault filter has no `stat_prefix` field of its own — same threading as RBAC at phase 10). |

**12.1 entries (active health checking):**

| Stat name | Equivalence | Rationale |
|---|---|---|
| `cluster.<name>.membership_healthy` | value-exact (12.2 steady state; reads 0 at 12.1) | Gauge; the count of currently-healthy endpoints in the cluster. Registered at `from_bootstrap` time only when the cluster configures `health_checks`; updated inline at each `EndpointHealth` Healthy/Unhealthy flip (one source of truth, NOT polled — the 08.2 `server.live` pattern). At 12.1, with no probe task, a configured-HC cluster's gauge reads its initial value 0 (all endpoints start Unhealthy per §6.2 item-1); 12.2's probe task drives it to the converged steady state. Inert when `health_checks` is unconfigured (no such gauge registered). The 3 `cluster.<name>.health_check.{attempt,success,failure}` counters defer to 12.2 where the probe task increments them (12.1 D6 lock-in). |

**12.2 entries (active health checking — counters):**

| Stat name | Equivalence | Rationale |
|---|---|---|
| `cluster.<name>.health_check.attempt` | name-required, value-may-differ | Counter; one increment per health-check probe issued by the `envoy-health` scheduler. The count is **timing-dependent** — both proxies tick on their own independent `tokio::time::interval` schedules from independent process-start instants, so the elapsed-probe count over a fixed test window differs across proxies. Both proxies emit the name; the equivalence dimension is name-required only (value-exact is not feasible without timing-tolerance opt-in per §Timing tolerances, which phase 12 does NOT take). Registered at `Scheduler::spawn` time only when the cluster configures `health_checks`. |
| `cluster.<name>.health_check.success` | name-required, value-may-differ | Counter; one increment per probe whose response status ∈ `expected_statuses` (default exactly 200, half-open `Int64Range`). Same timing-dependence rationale as `.attempt`. |
| `cluster.<name>.health_check.failure` | name-required, value-may-differ | Counter; one increment per probe whose response status is NOT in `expected_statuses`, OR connect failure, OR per-probe `tokio::time::timeout` elapsed, OR malformed response (the network-failure-class results fold into `failure` at phase-12 scope; the dedicated `network_failure` sub-counter defers per parent SPEC §4). Same timing-dependence rationale as `.attempt`. |

**13.1 entries (H1 connection pool):**

| Stat name | Equivalence | Rationale |
|---|---|---|
| `cluster.<name>.upstream_cx_destroy` | value-exact (0-failures case) | Counter; incremented at every pool eviction. Three eviction paths: (a) idle-sweeper past-deadline (the second periodic-background primitive — sweeps every `idle_timeout / 4`); (b) `PoolGuard::invalidate()` flag on protocol error (Drop's None-arm); (c) connect-failure rollback (the `established` count decrement does NOT fire `cx_destroy` per 13.1 D3 — only successful-acquire-then-destroy paths count). Under the deterministic harness load with no forced-close + the hardcoded 60 s idle timeout (well past the ~5 s fixture settle window per 13.1 §5.4 lock-in), no idle eviction fires during fixture lifetime → both proxies emit 0 within the fixture window. Future fixtures exercising forced-close or longer settle would harden the disposition. Registered at `H1PoolManager::for_bootstrap` time only for clusters whose `upstream_protocol()` is `Http1`. |
| `cluster.<name>.upstream_cx_http1_total` | value-exact | Counter; one increment per H1 pool connect-on-miss (fires at the same site as the existing `cluster.<name>.upstream_cx_total` for H1 clusters — the H1 pool's `acquire()` connect-on-miss branch per 13.1 D3 + D4). Under the fixture 0020 single-downstream-keep-alive-conn driver issuing 10 sequential requests → both proxies emit 1 (full pool reuse). Registered at `H1PoolManager::for_bootstrap` time only for clusters whose `upstream_protocol()` is `Http1`. The existing `cluster.<name>.upstream_cx_total` BEHAVIOR_CONTRACT row at line `:89` (06.1 initial entry) STAYS `name-required, value-may-differ` AT 13.1 — the row tightening to `value-exact` is the **13.2 D7.1 deliverable** (the 06.3 REVIEW I2 (b) full-closure site; fires only when both H1 + H2 pools uniformly, since the row mentions no protocol carve-out and tightening at 13.1 would falsify the H2 surface that still increments per-call). |

**13.2 entries (H2 connection pool):**

| Stat name | Equivalence | Rationale |
|---|---|---|
| `cluster.<name>.upstream_cx_http2_total` | value-exact | Counter; one increment per H2 pool connect-on-miss (fires at the same site as the existing `cluster.<name>.upstream_cx_total` for H2 clusters — the H2 pool's `acquire()` connect-on-miss branch per 13.2 D5 + D6, at `crates/envoy-http2/src/pool.rs::H2Pool::acquire`). Under the fixture 0021 single-downstream-keep-alive-conn driver issuing 5 sequential requests over an H2-upstream cluster → both proxies emit 1 (single upstream H2 connection multiplexing 5 concurrent stream slots; per the H2 pool's per-entry `active_streams` claim loop). Under hypothetical workloads beyond `DEFAULT_MAX_CONCURRENT_STREAMS = 100` (the RFC 7540 §6.5.2 default when peer SETTINGS is unobserved) the H2 pool would establish additional connections and the counter would tick again — fixture 0021's 5-request workload stays well under the cap so the bilateral value is deterministic 1. Registered at `H2PoolManager::for_bootstrap` time only for clusters whose `upstream_protocol()` is `Http2`. Sibling of `cluster.<name>.upstream_cx_http1_total` (13.1 entry); together they enumerate the per-protocol breakdown of `cluster.<name>.upstream_cx_total`. |

**14.1 entries (outlier detection):**

| Stat name | Equivalence | Rationale |
|---|---|---|
| `cluster.<name>.outlier_detection.ejections_active` | value-exact (14.2 steady state; reads 0 at 14.1) | Gauge; count of currently-ejected endpoints in the cluster. Registered at `from_bootstrap` time only when `outlier_detection` is configured; updated inline at each `EndpointEjection::eject` / `try_un_eject` edge (one source of truth, NOT polled — the 12.1 `membership_healthy` pattern). At 14.1 the gauge reads its initial value 0 (all endpoints start never-ejected per §6.2 item-3); 14.2's response-receipt hook + sweeper drive it to the converged steady state. Inert when `outlier_detection` unconfigured (no such gauge registered). **The only gauge in the namespace** — the 6 sibling stats are counters. |
| `cluster.<name>.outlier_detection.ejections_enforced_total` | value-exact (14.2 steady state; reads 0 at 14.1) | Counter; one increment per actual ejection enforced (after the `max_ejection_percent` cap check at the cluster level). Sum across detector types modulo overflow. Per-detector siblings `ejections_enforced_consecutive_5xx` + `ejections_enforced_consecutive_gateway_failure` break it down. At 14.1 the value is 0 (no caller drives ejection until 14.2 D4). |
| `cluster.<name>.outlier_detection.ejections_overflow` | value-exact (0-case at fixture 0022's `max_ejection_percent: 100`; reads 0 at 14.1) | Counter; **per the §6.2 item-4 finding**, increments per detection-tick on cap-blocked enforcement (NOT once-per-host — overflow is a re-fire counter). Cluster-level (lives on `OutlierDetectionState`, not per-endpoint). Fixture 0022's `max_ejection_percent: 100` keeps this at 0 in steady state. At 14.1 the value is 0 (no caller drives the cap check until 14.2 D4). |
| `cluster.<name>.outlier_detection.ejections_detected_consecutive_5xx` | value-exact (14.2 steady state; reads 0 at 14.1) | Counter; per-detector-type tick fired at every threshold-crossing on the consecutive_5xx detector, **regardless of whether the cap permits enforcement** (per ADR-0041 §6.2 item-2). Sibling of `ejections_enforced_consecutive_5xx`. Incremented inline by `EndpointEjection::record_response` at the threshold-crossing site. At 14.1 the value is 0 (no caller). |
| `cluster.<name>.outlier_detection.ejections_enforced_consecutive_5xx` | value-exact (14.2 steady state; reads 0 at 14.1) | Counter; per-detector-type tick fired only when the threshold-crossing actually drives an ejection (cap honored). Equal to `ejections_detected_consecutive_5xx` minus the per-detector overflow share. At `enforcing_consecutive_5xx: 100` (the fixture-0022 setting and envoy-rust's only supported value at phase-14 scope per parent SPEC §4 deferral of `enforcing_*` knobs), `enforced == detected` modulo the cap. At 14.1 the value is 0 (no caller). |
| `cluster.<name>.outlier_detection.ejections_detected_consecutive_gateway_failure` | value-exact (0-case at fixture 0022; reads 0 at 14.1) | Counter; same shape as the `_consecutive_5xx` sibling. The fixture-0022 backend serves status 500 (NOT 502/503/504), so the gateway-failure detector never fires during fixture lifetime; both proxies emit 0. At 14.1 the value is 0 (no caller). |
| `cluster.<name>.outlier_detection.ejections_enforced_consecutive_gateway_failure` | value-exact (0-case at fixture 0022; reads 0 at 14.1) | Counter; sibling of `_detected_consecutive_gateway_failure`. 0-case at fixture-0022. At 14.1 the value is 0 (no caller). |

The remaining 13 Envoy-side names under `cluster.<name>.outlier_detection.*` (the `_detected_/_enforced_` pairs for `consecutive_local_origin_failure`, `success_rate`, `local_origin_success_rate`, `failure_percentage`, `local_origin_failure_percentage` = 10; the legacy aliases `ejections_total` + `ejections_consecutive_5xx` + `ejections_success_rate` = 3) are NOT emitted by envoy-rust at phase-14 minimum-viable scope (out per parent §4 deferral; ratified by ADR-0041 §6.2 item-2). **14.2 M8 reconciliation:** the count is **13** (5 detector pairs + 3 legacy aliases), correcting the prior "14" claim to match the enumeration. Fixture 0022's `expectations.yaml` does NOT need an `allowlist_envoy_only` for these: its `Driver::Http1KeepAlive` stat path asserts only the named `expected_stats` (no full set-diff), so unasserted Envoy-only names are ignored (unlike the 0011 prometheus-set-diff path, whose `allowlist_envoy_only` key does not exist on the keep-alive driver).

**15 entries (circuit breakers):**

| Stat name | Equivalence | Rationale |
|---|---|---|
| `cluster.<name>.upstream_rq_pending_overflow` | value-exact bilaterally (fixture 0023: 1 bilaterally) | Counter; `+1` per request rejected on the connect-on-miss path when `max_pending_requests == 0` (the reject-on-establish gate, ADR-0043 §6.2 finding 1). One source of truth at the pool pending-gate (one site per protocol, BEFORE the cap-check). Registered only when `circuit_breakers` is configured (inert-when-unconfigured per lock-in #4); an unconfigured cluster defaults `max_pending_requests` to 1024 so the gate never fires and no such stat is registered. |
| `cluster.<name>.upstream_cx_overflow` | value-exact-at-0 bilaterally (fixtures 0020/0023 never trip the cap); the NON-ZERO cross-proxy value DIVERGES — validated non-zero IN-PROCESS only (the Task-8 backstop) | Counter; `+1` per upstream-connection demand rejected because the pool is AT `max_connections` (cap-hit; ADR-0043 §6.2 finding 2). One source of truth at the pool cap-check branch. On a cap-hit Envoy queues→counts the cap-hit but (with default pending) serves 200; envoy-rust has no pending queue at phase-15 scope and 503s — so the counter name+semantics match and the value matches at 0, but the non-zero value (and the downstream status multiset) is a **known divergence pending the deferred pending-queue phase** (§0.C finding 2 / ADR-0043). Registered only when `circuit_breakers` is configured. |
| `cluster.<name>.circuit_breakers.default.cx_open` | value-exact-at-0 bilaterally; non-zero in-process only (the Task-8 backstop) | Gauge 0/1; `1` while `upstream_cx_active == max_connections` (at-cap inclusive, ADR-0043 §6.2 finding 4), `0` otherwise; **edge-driven** (set at the `established`-count mutation edges, NOT polled), terminal-0 (returns to 0 after drain). `default` = the only supported `RoutingPriority` at phase-15 scope. Envoy always emits the full `circuit_breakers.{default,high}.{cx_open, cx_pool_open, rq_open, rq_pending_open, rq_retry_open}` 10-gauge set regardless of config; envoy-rust emits ONLY `default.cx_open` at phase-15 scope (the other 9 are Envoy-only, deferred). Fixture 0023's `Driver::Http1KeepAlive` scrapes only NAMED stats (no full set-diff), so no `allowlist_envoy_only` enumeration is needed for the Envoy-only siblings. Registered only when `circuit_breakers` is configured. |

**Overflow-model divergence note (ADR-0043 §6.2).** Under `max_pending_requests: 0`, Envoy rejects ALL establish-on-miss requests via `upstream_rq_pending_overflow` (NOT `upstream_cx_overflow`); the pool never warms (`upstream_cx_total: 0`, backend never contacted), and `upstream_cx_overflow`/`cx_open` stay inert-0 because no connection demand reaches the cap. The `{200,503}` cx-overflow multiset asserted in-process (Task-8 backstop) is **in-process-only**: on a `max_connections` cap-hit with a default (non-zero) pending budget, Envoy queues the cap-overflow request and serves it 200, yielding a bilateral `{200,200}` queue-and-serve shape; envoy-rust 503s the overflow. The bilateral `{200,200}` queue-and-serve fixture therefore **defers to the future pending-queue phase** (the `max_pending_requests > 0` queue, deferred per ADR-0042 §4 / ADR-0043 option d). See ADR-0043. The overflow-503 wire shape (status + 81-byte `…reset reason: overflow` body + `x-envoy-overloaded: true`) is captured in the Equivalence matrix row above (Task 4) and is NOT duplicated here.

**16 entries (HTTP retries):**

| Stat name | Equivalence | Rationale |
|---|---|---|
| `cluster.<name>.upstream_rq_retry` | value-exact | Counter; `+1` per retry attempted (per re-dispatch beyond the first attempt). One source of truth at the retry-loop classification site — H1 at `crates/envoy-http1/src/hcm.rs`, H2 at `crates/envoy-http2/src/hcm.rs` (one site per protocol). Fixture 0024: 2 cumulative over both probes (one retry per probe). Registered unconditionally for every cluster at `from_bootstrap` time (`crates/envoy-cluster/src/cluster.rs`); inert at 0 when no route configures `retry_policy`. |
| `cluster.<name>.upstream_rq_retry_success` | value-exact | Counter; `+1` when a retried request ultimately produces a non-retriable outcome (i.e., the final attempt is not itself retriable — the request "succeeds out of retry"). Registered and incremented at the same H1/H2 retry-loop classification site as `upstream_rq_retry`. Fixture 0024: 1 (probe 1 only — 503→200 path). |
| `cluster.<name>.upstream_rq_retry_limit_exceeded` | value-exact | Counter; `+1` when `num_retries` is exhausted and the final attempt is still retriable (limit-exceeded path; the final upstream response is surfaced verbatim downstream). Registered and incremented at the same H1/H2 retry-loop classification site. Fixture 0024: 1 (probe 2 only — both attempts 503; see wire-shape note below). |

**Per-attempt counting reconciliation (ADR-0045 finding L5).** `cluster.<name>.upstream_rq_total` counts per upstream **attempt** — a request with one retry ticks it twice. Fixture 0024 asserts 4 over 2 probes (2 attempts × 2 probes). `cluster.<name>.upstream_rq_5xx` reflects the **completing** (downstream-returned) response only; the retried-away 5xx does **not** tick the main `upstream_rq_5xx` counter — it surfaces in the Envoy-only `cluster.<name>.retry.upstream_rq_{503,5xx,completed}` sub-scope which envoy-rust does NOT emit (allow-listed per ADR-0045 option (b)). Fixture 0024 asserts `upstream_rq_5xx: 1` (probe 2's completing 503 only). The completing-response tick fires only when the completing attempt received a real upstream response — synthetic local replies (the no-healthy-upstream synth-503, connect-failure synth-502, reset synth-502, and overflow synth-503 paths) do not tick `upstream_rq_5xx`, preserving the pre-phase-16 baseline where these paths never ticked it (state-5 review fix). The Envoy-only `upstream_rq_retry_overflow` / `upstream_rq_retry_backoff_*` / `retry_or_shadow_abandoned` / `circuit_breakers.*.rq_retry_open` names are similarly NOT emitted (allow-listed; per ADR-0045). **This paragraph supersedes the 06.3 `cluster.<name>.upstream_rq_total` row's "one increment per upstream response received" wording for retried requests.** For non-retried requests, per-attempt == per-response-received — the 06.3 row's wording remains accurate for all pre-phase-16 fixtures. The per-attempt semantic applies from phase 16 forward, per ADR-0045.

**Retry-limit-exceeded wire shape (ADR-0045 finding L9).** When `num_retries` is exhausted and the final attempt is still retriable, the downstream response is the **last upstream response verbatim** (status + body + headers) — NOT a synthetic local reply. This is distinct from the no-healthy-upstream and overflow synth-503 paths (which produce local replies with fixed bodies). Envoy's `%RESPONSE_FLAGS%` shows `URX` on this path, which is **access-log-only** and never surfaces as a response header.

**17 entries (circuit-breaker budgets):**

| Stat name | Equivalence | Rationale |
|---|---|---|
| `cluster.<name>.upstream_rq_retry_overflow` | value-exact | Counter; `+1` per retry abandoned because the retry budget (`max_retries`) is exhausted; ticks inside the failed `try_acquire_retry` (single source of truth, one site per budget — NOT per protocol; the budget lives in `envoy-cluster::BudgetState` per ADR-0046 §5.4). Registered UNCONDITIONALLY for every cluster (inert at 0 — the phase-16 retry-counter posture). Fixture 0025: 1 (budget_zero) / 0 (budget_default) / 0 (rq_zero). |
| `cluster.<name>.circuit_breakers.default.rq_retry_open` | value-exact-at-0 bilaterally (fixture 0025); NON-ZERO edge in-process only (the Task-9 backstop's >0-cap concurrency path) | Gauge 0/1; MOMENTARY semantic per ADR-0047 L4: `1` iff `active_retries > 0 AND active_retries >= max_retries`; never latched; `0` in every sequential-regime scrape. Registered only when `circuit_breakers` is configured. |
| `cluster.<name>.circuit_breakers.default.rq_open` | value-exact-at-0 bilaterally (fixture 0025); NON-ZERO edge in-process only (the Task-9 backstop's >0-cap concurrency path) | Gauge 0/1; same shape as `rq_retry_open` but for the request budget (`active_requests` vs `max_requests`). Registered only when `circuit_breakers` is configured. |
| `cluster.<name>.circuit_breakers.default.remaining_retries` | value-exact, registered ONLY when `track_remaining: true` (ADR-0047 L8: absent — not present-at-0 — otherwise) | Gauge; `= max_retries − active_retries`, floored at 0. Fixture 0025: 0 (budget_zero, cap 0) / 3 (budget_default, the Envoy default read back bilaterally). |
| `cluster.<name>.circuit_breakers.default.remaining_rq` | value-exact, registered ONLY when `track_remaining: true` (same conditionality as `remaining_retries`) | Gauge; `= max_requests − active_requests`, floored at 0. Fixture 0025: 1024 (budget_default — the Envoy default). |

**The L3 overflow co-firing paragraph (ADR-0047).** The `max_requests`-overflow local reply ticks `cluster.<name>.upstream_rq_pending_overflow` (inside the failed `try_acquire_request` — the same counter name phase 15 wired for `max_pending_requests`, idempotently shared) AND `cluster.<name>.upstream_rq_5xx` (at the HCM caller site) — **the ONLY synthetic local reply that ticks `upstream_rq_5xx`**; this narrowly supersedes the phase-16 "synthetic local replies do not tick `upstream_rq_5xx`" sentence for exactly this path (per ADR-0047; all other synth paths keep the phase-16 posture). `upstream_rq_total` stays 0 (matches Envoy). `upstream_cx_total` on the overflow cluster is a KNOWN DIVERGENCE left unasserted: Envoy 1 (connection-pool prefetch) vs envoy-rust 0 (no pool contact). Envoy additionally co-fires `upstream_rq_503`/`upstream_rq_completed`/`external.upstream_rq_503` (Envoy-only, not emitted by envoy-rust, unasserted).

**The §5.4 registration-seam paragraph (ADR-0046).** The `circuit_breakers.default.*` namespace now has TWO registration sites — the per-protocol POOLS register `cx_open` (phase 15: connection-lifecycle concept) while the CLUSTER registers `rq_open`/`rq_retry_open`/`remaining_*` (phase 17: cluster-wide budget concepts spanning both protocol pools). The `upstream_rq_pending_overflow` counter handle is idempotently shared between the phase-15 pool gate and the phase-17 request-budget gate.

**The L12 Envoy-only enumeration paragraph.** Per cluster with `circuit_breakers`, Envoy always emits the 10-gauge family `circuit_breakers.{default,high}.{cx_open, cx_pool_open, rq_open, rq_pending_open, rq_retry_open}`; with `track_remaining: true` it adds 5 `circuit_breakers.default.remaining_*` gauges (`remaining_cx`, `remaining_pending`, `remaining_rq`, `remaining_retries`, `remaining_cx_pools`). envoy-rust at phase-17 scope emits: `default.cx_open` (pools, phase 15) + `default.rq_open` + `default.rq_retry_open` (cluster, conditional on `circuit_breakers`) + `default.remaining_retries`/`default.remaining_rq` (conditional on `track_remaining`). The rest are Envoy-only unasserted names (ignored by the named-stat scrape).

**18 entries (file-based CDS):**

> The xDS-family opener. These are the project's first **top-level-scope**
> (non-resource-prefixed) `cluster_manager.*` stats, all derived from the
> §6.2 empirical lock-in **L3** (verified against `envoyproxy/envoy:v1.33.0`,
> digest `sha256:56da5afd…`, 2026-06-02). Envoy emits 18 names under
> `cluster_manager.*` after a successful CDS load; envoy-rust's minimum-viable
> subset is **6 names**. Registered at `ClusterManager::from_bootstrap` time
> (`crates/envoy-cluster/src/cluster.rs`), **conditionally** — ONLY when
> `dynamic_resources.cds_config` is configured.

| Stat name | Equivalence | Rationale |
|---|---|---|
| `cluster_manager.cds.update_attempt` | value-exact (fixture 0026: 1) | Counter; one increment per CDS load attempt. envoy-rust's synchronous `load_dynamic_resources` ticks it once at the file-read+parse step. Both proxies emit 1 on the single initial load (no hot-reload at phase-18 scope — L11 inconclusive/deferred). |
| `cluster_manager.cds.update_success` | value-exact (fixture 0026: 1) | Counter; one increment per CDS load that produced an installed cluster set. The load-bearing differential proof of the phase: `update_success: 1` can only pass if real Envoy genuinely loaded the CDS file (fixture 0026 asserts it bilaterally). |
| `cluster_manager.cds.update_failure` | value-exact (fixture 0026: 0) | Counter; in Envoy, `+1` per CDS load that hit a **parse error** (malformed envelope) — Envoy then warns-and-serves with `active_clusters: 0`. **In envoy-rust this is structurally 0:** all CDS load errors are FATAL pre-construction (the L4 all-fatal posture, ADR-0049 — see the xDS-wire-state-machine §(c)), so the process exits rather than reach a non-zero `update_failure` state. Registered at 0 and unreachable non-zero. Bilaterally satisfiable at 0 on fixture 0026 (a successful load). |
| `cluster_manager.cds.update_rejected` | value-exact (fixture 0026: 0) | Counter; in Envoy, `+1` per CDS load whose resource was **semantically invalid** (PGV violation / cluster-build failure) — distinct from the parse-error `update_failure` bucket; Envoy warns-and-serves. **In envoy-rust this is structurally 0** for the same reason as `update_failure` (the all-fatal posture; the process exits instead). Registered at 0 and unreachable non-zero. Bilaterally satisfiable at 0 on fixture 0026. |
| `cluster_manager.cluster_added` | value-exact (fixture 0026: 1) | Counter; `+1` per cluster ADDED to the manager. The count includes **static clusters** — Envoy counts ALL clusters added to the manager, not just CDS-supplied ones; envoy-rust mirrors (`= all_clusters().count()`, the merged static+dynamic size). Bilateral on fixture 0026 because it has **zero static clusters** (the single dynamic cluster yields 1 on both sides); a fixture mixing static + dynamic clusters would assert the combined count. |
| `cluster_manager.active_clusters` | value-exact (fixture 0026: 1) | Gauge; the count of currently-active clusters in the manager — the same merged static+dynamic size as `cluster_added`, and the same static-inclusion caveat applies (bilateral on fixture 0026 only because it has zero static clusters). The lone gauge of the 6 names (the other 5 are counters). |

**The §5.2 conditional-registration narrowing (recorded divergence, L10/ADR-0049).** All 6 names register ONLY when `dynamic_resources.cds_config` is configured. This is a **deliberate divergence** from Envoy's tree: Envoy emits the `cluster_manager.cds.*` subtree conditionally (the cds subtree exists only with CDS configured — both proxies agree here), but Envoy emits the **base** `cluster_manager.*` names (`active_clusters`, `cluster_added`, …) **unconditionally** on every bootstrap. envoy-rust narrows the base names to the same CDS-configured condition (registers nothing on non-CDS fixtures), so on all non-CDS fixtures the base `cluster_manager.*` names stay **Envoy-only-unasserted** (fixture 0011's Prometheus set-diff posture is unchanged; zero existing-fixture edits). Recorded explicitly per doctrine D-3.3.

**The L3 Envoy-only enumeration paragraph.** After a successful load Envoy emits **18** `cluster_manager.*` names; envoy-rust emits the **6** above. The 12 Envoy-only unasserted names (ignored by fixture 0026's named-stat scrape — no set-diff on the `Http1KeepAlive` driver) are: `cds.update_time`, `cds.version`, `cds.version_text`, `cds.update_duration`, `cds.init_fetch_timeout`, `cluster_modified`, `cluster_removed`, `cluster_updated`, `cluster_updated_via_merge`, `update_merge_cancelled`, `update_out_of_merge_window`, `warming_clusters`. (Envoy additionally carries a `cds.control_plane.*` family, irrelevant to the filesystem transport.) None of the 6 emitted `cluster_manager.*` values change pre- vs post-GET — request counters live under `cluster.<name>.*`.

**19 entries (file-based LDS):**

> The xDS-family continuation (ADR-0050 SPEC / PLAN). file-based LDS loads
> listeners from `dynamic_resources.lds_config.path_config_source.path` at
> startup; these are the first top-level-scope `listener_manager.*` stats. All
> derived from the §6.2 empirical lock-in **L3** (verified against
> `envoyproxy/envoy:v1.33.0`, digest `sha256:56da5afd…`, 2026-06-02). Envoy
> emits **21** names under `listener_manager.*` after a successful LDS load;
> envoy-rust's minimum-viable subset is **6 names**. The 4 `lds.*` names +
> `listener_added` register **conditionally** — ONLY when
> `dynamic_resources.lds_config` is configured (`register_lds_stats`, Task 4);
> `total_listeners_active` keeps its pre-existing 08.2 **unconditional**
> registration, here tightened to a bilateral assertion on fixture 0027.

| Stat name | Equivalence | Rationale |
|---|---|---|
| `listener_manager.lds.update_attempt` | value-exact (fixture 0027: 1) | Counter; one increment per LDS load attempt. envoy-rust's synchronous `load_dynamic_resources` ticks it once at the file-read+parse step. Both proxies emit 1 on the single initial load (no hot-reload at phase-19 scope). |
| `listener_manager.lds.update_success` | value-exact (fixture 0027: 1) | Counter; one increment per LDS load that produced an installed listener set. The load-bearing differential proof of the phase: `update_success: 1` can only pass if real Envoy genuinely loaded the LDS file (fixture 0027 asserts it bilaterally). |
| `listener_manager.lds.update_failure` | value-exact (fixture 0027: 0) | Counter; in Envoy, `+1` per LDS load that hit a **parse error** (malformed envelope / missing `@type`) — Envoy then warns-and-serves. **In envoy-rust this is structurally 0:** all LDS load errors are FATAL pre-construction (the L4 all-fatal posture, ADR-0049 extended to LDS by ADR-0050 — see the xDS-wire-state-machine LDS §(c)), so the process exits rather than reach a non-zero `update_failure` state. Registered at 0 and **structurally unreachable non-zero**. Bilaterally satisfiable at 0 on fixture 0027 (a successful load). |
| `listener_manager.lds.update_rejected` | value-exact (fixture 0027: 0) | Counter; in Envoy, `+1` per LDS load whose resource was **semantically invalid** (PGV violation / listener-build failure) — distinct from the parse-error `update_failure` bucket; Envoy warns-and-serves. **In envoy-rust this is structurally 0** for the same reason as `update_failure` (the all-fatal posture; the process exits instead). Registered at 0 and **structurally unreachable non-zero**. Bilaterally satisfiable at 0 on fixture 0027. |
| `listener_manager.listener_added` | value-exact (fixture 0027: 1) | Counter; `+1` per listener ADDED to the manager. The count includes **static listeners** — Envoy counts ALL listeners added, not just LDS-supplied ones; envoy-rust mirrors. Bilateral on fixture 0027 because it has **zero static listeners** (the single dynamic listener yields 1 on both sides); the L7 collision backstop (a static listener defined under the same name as the LDS entry) asserts 1 (the static listener only — the collision-skipped LDS entry does not re-tick). **Conditional registration narrowing** — see the §5.2 paragraph below. |
| `listener_manager.total_listeners_active` | value-exact (fixture 0027: 1) | Gauge; the count of currently-active listeners in the manager. **Distinct from `listener_added` in registration:** this gauge keeps its pre-existing 08.2 **unconditional** registration (it predates LDS); phase 19 only tightens it to a bilateral assertion on fixture 0027. The lone gauge of the 6 names (the other 5 are counters). |

**The §5.2 conditional-registration narrowing (recorded divergence, L10/ADR-0050).** The 4 `lds.*` names **and** `listener_added` register ONLY when `dynamic_resources.lds_config` is configured. This is a **deliberate divergence** from Envoy's tree: Envoy emits the `listener_manager.lds.*` subtree conditionally (the lds subtree exists only with LDS configured — both proxies agree here), but Envoy emits the **base** `listener_manager.*` names (`listener_added`, `listener_create_success`, `total_listeners_active`, `workers_started`, …) **unconditionally** on every bootstrap. envoy-rust narrows the base name `listener_added` to the same LDS-configured condition (registers nothing on non-LDS fixtures — verified by the backstop's inertness path (vi), which asserts `listener_added` is ABSENT and no `lds.*` names appear when no `lds_config` is present), so on the fixture-0026 topology (CDS configured, NO lds_config) the `listener_manager.lds.*` subtree + the base `listener_added` name stay **Envoy-only-unasserted**. `total_listeners_active` is the **exception** — it keeps its unconditional 08.2 registration on both LDS and non-LDS fixtures. Recorded explicitly per doctrine D-3.3.

**The L3 Envoy-only enumeration paragraph (LDS).** After a successful load Envoy emits **21** `listener_manager.*` names; envoy-rust emits the **6** above. The 15 Envoy-only unasserted names (ignored by fixture 0027's named-stat scrape — no set-diff on the `Http1KeepAlive` driver) are: `listener_create_success`, `listener_create_failure`, `listener_modified`, `listener_removed`, `listener_stopped`, `listener_in_place_updated`, `total_listeners_warming`, `total_listeners_draining`, `total_filter_chains_draining`, `workers_started`, `lds.update_time`, `lds.update_duration`, `lds.version`, `lds.version_text`, `lds.init_fetch_timeout`. **✧ `listener_create_success` is PER-WORKER** — observed at **12 on a 12-core host** (one tick per worker thread per listener); it is host-core-count-dependent, **NEVER asserted bilaterally**, and is NOT in the 6-name subset. (Envoy additionally carries an `lds.control_plane.*` family, irrelevant to the filesystem transport.) Fixture 0027 also carries the phase-18 `cluster_manager.*` 6-name subset, here with **`cluster_added: 2` / `active_clusters: 2`** (TWO clusters: the static `static_backend` + the CDS-supplied `dynamic_backend`) and `cds.update_attempt/success/failure/rejected` = 1/1/0/0.

**20 entries (file-based RDS):**

> The xDS-family continuation (ADR-0051 SPEC / ADR-0052 PLAN). file-based RDS
> loads route tables from `rds.config_source.path_config_source.path` on each
> HCM at startup; these are the project's first **per-HCM-scoped** xDS stats
> (every name is prefixed `http.<stat_prefix>.rds.<route_config_name>.`, NOT a
> top-level-scope `*_manager.*` name). All derived from the §6.2 empirical
> lock-ins (verified against `envoyproxy/envoy:v1.33.0`, digest
> `sha256:56da5afd…`, 2026-06-02). Envoy emits a fuller `http.<prefix>.rds.<name>.*`
> family after a successful RDS update; envoy-rust's minimum-viable subset is
> **5 names**. Registered **conditionally** — ONLY when the owning HCM's `rds`
> is configured (an inline-route HCM emits no `rds.*` names — L10).

| Stat name | Equivalence | Rationale |
|---|---|---|
| `http.<stat_prefix>.rds.<route_config_name>.update_attempt` | value-exact (fixture 0028: 1) | Counter; one increment per RDS update attempt. envoy-rust's synchronous initial load ticks it once at the file-read+parse step. Initial-load-only at phase-20 scope (no hot-reload) → exactly `1` after startup on both proxies. |
| `http.<stat_prefix>.rds.<route_config_name>.update_success` | value-exact (fixture 0028: 1) | Counter; one increment per successful RDS update (an installed route table). The load-bearing differential proof of the phase: `update_success: 1` can only pass if real Envoy genuinely loaded the RDS file (fixture 0028 asserts it bilaterally). |
| `http.<stat_prefix>.rds.<route_config_name>.update_failure` | value-exact (fixture 0028: 0) | Counter; in Envoy, `+1` per RDS update that hit a **parse error** (malformed envelope / missing `@type`) — Envoy then warns-and-serves. **In envoy-rust this is structurally 0:** all RDS load errors are FATAL pre-construction (the all-fatal posture, ADR-0049 decision 2 extended to RDS by ADR-0052 L4 — see the xDS-wire-state-machine RDS §(c)), so the process exits rather than reach a non-zero `update_failure` state. Registered at 0 and **structurally unreachable non-zero**. Bilaterally satisfiable at 0 on fixture 0028 (a successful load). |
| `http.<stat_prefix>.rds.<route_config_name>.update_rejected` | value-exact (fixture 0028: 0) | Counter; in Envoy, `+1` per RDS update whose resource was **semantically invalid** (PGV violation / route-build failure) — distinct from the parse-error `update_failure` bucket; Envoy warns-and-serves. **In envoy-rust this is structurally 0** for the same reason as `update_failure` (the all-fatal posture; the process exits instead). Registered at 0 and **structurally unreachable non-zero**. Bilaterally satisfiable at 0 on fixture 0028. |
| `http.<stat_prefix>.rds.<route_config_name>.config_reload` | value-exact (fixture 0028: 1) | Counter; `+1` per route-config version applied. **Ticks at initial load** (§6.2 L3 verified — the initial route table counts as the first reload), so the single synchronous load drives it to `1` on both proxies. Subsequent hot-reloads (deferred at phase-20 scope) would tick it again. |

**The per-HCM scoping paragraph (L1).** Every name in the 5-name subset is prefixed `http.<stat_prefix>.rds.<route_config_name>.` — both the `<stat_prefix>` (from the owning HCM's `stat_prefix`) AND the `<route_config_name>` (from the `rds.route_config_name`) are dynamic segments. Fixture 0028's concrete prefix is `http.ingress_http1.rds.local_route.`. This is the project's first xDS stat family scoped to a per-HCM, per-route-config name (vs the phase-18 `cluster_manager.*` / phase-19 `listener_manager.*` top-level-scope names).

**The conditional-registration narrowing (recorded divergence, L5/ADR-0052).** The `rds.*` names register ONLY when the owning HCM's `rds` is configured — a deliberate, recorded narrowing vs Envoy. An **inline-route HCM emits no `rds.*` names** (the route table comes from the static `route_config` on the HCM, with no RDS update lifecycle to count). All **27 pre-existing fixtures** (inline-route HCMs, or CDS/LDS-only topologies) therefore see **zero new envoy-rust names** under `http.<prefix>.rds.*`; only fixture 0028 (the first `rds`-on-HCM fixture) exercises the family. Recorded explicitly per doctrine D-3.3.

**The Envoy-only enumeration paragraph.** After a successful RDS update Envoy emits a fuller `http.<prefix>.rds.<name>.*` family; envoy-rust emits the **5** above. The Envoy-only unasserted names (ignored by fixture 0028's named-stat scrape — no set-diff on the `Http1KeepAlive` driver) are: `version`, `version_text`, `update_time`, `config_reload_time_ms`, `update_empty`, `init_fetch_timeout`, `update_duration`. (Envoy additionally carries an `rds.<name>.control_plane.*` family, irrelevant to the filesystem transport.)

**06.1 Prometheus exposition shape divergence (06.1 fixture 0011):**

> Upstream Envoy's Prometheus emitter projects dynamic name segments
> (the `<name>` in `listener.<name>.downstream_cx_total`, the
> `<stat_prefix>` in `http.<stat_prefix>.downstream_rq_total`, etc.) into
> Prometheus *labels*: the wire shape is
> `envoy_listener_downstream_cx_total{envoy_listener_address="0.0.0.0_10000"} 0`.
> envoy-rust's emitter (`crates/envoy-stats/src/prometheus.rs`) instead
> projects the dynamic segment directly into the metric name:
> `envoy_listener_ingress_http_downstream_cx_total 0`.
>
> Both projections carry the same counter; only the Prometheus
> name-vs-label shape differs. Fixture 0011 bridges this via paired
> `allowlist_envoy_only` / `allowlist_envoy_rust_only` entries (see
> `tests/fixtures/0011-admin-stats-prometheus/expectations.yaml`).
> The dot-tree contract above (`http.<stat_prefix>.downstream_rq_total`
> as value-exact) remains the authoritative semantic — the emitter-side
> shape divergence does not loosen the equivalence dimension.
>
> This divergence is documented for transparency; resolution defers to
> a later phase that adds a `StatsTagExtractor`-equivalent which
> extracts the dynamic segments back into Prometheus labels at scrape
> time. When that lands, the paired allow-list entries drop together
> and this paragraph is removed (no contract loosening).

---

## Admin endpoint body shapes

> **To be filled per-phase as needed.**
>
> Authored per phase 08.1 SPEC §2.1. One row per admin endpoint with the body
> kind + per-endpoint equivalence disposition. Tasks 6/7/8/9 of phase 08.1
> populate `/config_dump`, `/server_info`, `/clusters`, `/listeners`
> respectively. Future POST-bearing admin surfaces (08.2 family) and any
> later admin endpoints append rows here with the same columns.

| Endpoint | Method | Body kind | Equivalence disposition |
|---|---|---|---|
| `/config_dump` | GET | JSON object | Top-level shape `{ "configs": [...] }`. envoy-rust emits the `BootstrapConfigDump` entry at `configs[0]`: `{ "@type": "type.googleapis.com/envoy.admin.v3.BootstrapConfigDump", "bootstrap": <static-bootstrap-as-JSON>, "last_updated": <ISO-8601 timestamp> }`. Envoy may emit additional entries for xDS-derived configs; those land on `allowlist_envoy_only`. `bootstrap.static_resources` content value-exact-after-roundtrip (modulo serde renamings; the harness's `JsonShape::required_subtree` covers this). `last_updated` name-required-value-may-differ (wall-clock non-determinism). The `BootstrapConfigDump` shows the bootstrap **as parsed from disk** — dynamic (CDS) clusters do NOT appear here (SPEC §5.5 config_dump separation); they surface in the `ClustersConfigDump` entry below. |
| `/config_dump` `ClustersConfigDump` (phase 18, L5/ADR-0049) | GET | JSON object (a `configs[]` entry) | **Conditional emission:** envoy-rust emits this entry ONLY when `dynamic_resources.cds_config` is configured; on non-CDS fixtures it is absent (fixture 0014's single-`BootstrapConfigDump`-entry shape preserved). When present, it lands at `configs[1]` on **both** proxies (Envoy's order: `BootstrapConfigDump`[0], `ClustersConfigDump`[1], …). Shape: `{ "@type": "type.googleapis.com/envoy.admin.v3.ClustersConfigDump", "dynamic_active_clusters": [ { "cluster": { "@type": "type.googleapis.com/envoy.config.cluster.v3.Cluster", <full cluster config> }, "last_updated": <ISO-8601> } ], "static_clusters": [ … when non-empty ] }`. The inner `cluster` object carries its own `@type` plus the full flattened cluster config. **Empty-key omission (proto3-JSON style):** `static_clusters` and `dynamic_active_clusters` are each `skip_serializing_if = Vec::is_empty` on both sides — a static-only Envoy emits the entry with ONLY a `static_clusters` key (no `dynamic_active_clusters`); there is NO `version_info` key (the CDS file had none — proto3 JSON omits empty fields). `last_updated` name-required-value-may-differ (wall-clock; reuses the BootstrapConfigDump ISO-8601 emitter). **Bilateral anchor (fixture 0026):** `configs.1.dynamic_active_clusters.0.cluster.name == dynamic_backend` (`JsonShape::required_subtree`; both sides equal the expected value AND each other). The surrounding `configs` array content otherwise differs substantially per side (envoy emits its full protobuf-canonical projection; envoy-rust the narrower parsed-bootstrap projection) — `value_may_differ_keys: ["configs"]`, mirroring fixture 0014. Note: envoy-rust's cluster JSON uses snake_case field names while Envoy's proto3-JSON defaults to camelCase for multi-word fields — irrelevant for the `name` anchor (single-word, identical) but binding if a future fixture asserts deeper nested cluster fields. |
| `/config_dump` `ListenersConfigDump` (phase 19, L5/ADR-0050) | GET | JSON object (a `configs[]` entry) | **Conditional emission:** envoy-rust emits this entry ONLY when `dynamic_resources.lds_config` is configured; on non-LDS fixtures it is absent (the backstop inertness path (vi) verifies `/config_dump` does NOT contain `"ListenersConfigDump"` on a CDS-only bootstrap). When present with **both** LDS+CDS configured, it lands at `configs[2]` on **both** proxies — **AFTER** the `ClustersConfigDump` at `configs[1]` (Envoy's verified order: `BootstrapConfigDump`[0], `ClustersConfigDump`[1], `ListenersConfigDump`[2], …; fixture 0026's `configs[1]` Clusters assertion needs NO amendment). Shape: `{ "@type": "type.googleapis.com/envoy.admin.v3.ListenersConfigDump", "dynamic_listeners": [ { "name": "dynamic_listener", "active_state": { "listener": { "@type": "type.googleapis.com/envoy.config.listener.v3.Listener", <full listener config> }, "last_updated": <ISO-8601> } } ], "static_listeners": [ … when non-empty ] }`. **Note the DIFFERENT nesting from the CDS dump:** the listener is nested under `dynamic_listeners[].active_state.listener` (vs the CDS dump's flatter `dynamic_active_clusters[].cluster`), and each entry carries a top-level `name` key. **No `version_info` key** — `active_state` has NO `version_info` (file-based LDS; the LDS file had none — proto3 JSON omits empty fields). **Empty-key omission:** `static_listeners` and `dynamic_listeners` are each `skip_serializing_if = Vec::is_empty` — a static-only Envoy emits the entry with ONLY `static_listeners`. `last_updated` name-required-value-may-differ (wall-clock; reuses the BootstrapConfigDump ISO-8601 emitter). **Bilateral anchor (fixture 0027):** `configs.2.dynamic_listeners.0.name == dynamic_listener` (`JsonShape::required_subtree`; both sides equal the expected value AND each other). The surrounding `configs` array otherwise differs per side — `value_may_differ_keys: ["configs"]`. **Known narrowing (LDS-only bootstrap):** on an LDS-only (no-CDS) bootstrap, envoy-rust's Listeners entry would land at `configs[1]` vs Envoy's `configs[2]` (Envoy emits a `ClustersConfigDump` for static clusters unconditionally, occupying `[1]`; envoy-rust's `ClustersConfigDump` is CDS-conditional per phase-18 L10, so it is absent and Listeners shifts up). Fixture 0027 configures BOTH LDS+CDS so the indices align at `[2]`; the divergence is recorded for any future LDS-only fixture (none exercises it today). |
| `/config_dump` `RoutesConfigDump` (phase 20, L5/ADR-0052) | GET | JSON object (a `configs[]` entry) | **Conditional emission:** envoy-rust emits this entry ONLY when **some HCM uses `rds`**; on non-RDS fixtures it is absent (vs Envoy's **always-emitted** `RoutesConfigDump`, which carries `static_route_configs` even without any RDS). Shape: `{ "@type": "type.googleapis.com/envoy.admin.v3.RoutesConfigDump", "dynamic_route_configs": [ { "route_config": { "@type": "type.googleapis.com/envoy.config.route.v3.RouteConfiguration", "name": "local_route", "virtual_hosts": [ … ] }, "last_updated": <ISO-8601> } ] }`. **No `version_info` key** — the RDS file had none (proto3 JSON omits empty fields; same posture as the CDS/LDS dumps). `last_updated` name-required-value-may-differ (wall-clock; reuses the BootstrapConfigDump ISO-8601 emitter). **Index divergence + per-side reconciliation:** the entry lands at **`configs[4]`** on Envoy (Bootstrap[0]/Clusters[1]/Listeners[2]/ScopedRoutes[3]/Routes[4]/Secrets[5]) but **`configs[2]`** on envoy-rust on fixture 0028 (Bootstrap[0]/Clusters[1]/Routes[2] — Listeners gated off, no `lds_config` on 0028) — bridged by a **per-side `JsonSubtreeRule` path override** in the harness (Envoy `configs.4.…` vs envoy-rust `configs.2.…`). **Bilateral anchor (fixture 0028):** the `route_config.name == local_route` subtree (`JsonShape::required_subtree`; both sides equal the expected value AND each other). The surrounding `configs` array otherwise differs per side — `value_may_differ_keys: ["configs"]`. Fixtures 0026/0027 hold (the RoutesConfigDump entry is RDS-conditional and absent there; their Clusters[1]/Listeners[2] assertions are NOT displaced). |
| `/server_info` | GET | JSON object | Required keys `state`, `version`, `node`, `uptime_current_epoch_seconds`, `uptime_all_epochs_seconds`, `hot_restart_version`, `command_line_options`. `state` value-exact, sourced from `DrainState::current()` via the mapping `Live | HealthcheckFailing → "LIVE"`, `Draining → "DRAINING"` (08.1 emitted the literal constant `"LIVE"` as a placeholder; 08.2's D5e patches the value-binding source at Task 5 — the struct shape is unchanged at the 08.1 → 08.2 boundary); `node.*` value-exact from the parsed bootstrap; `version` + `hot_restart_version` + `command_line_options` allowlist-each-side (envoy-rust emits its own version string; Envoy emits its own); `uptime_*` name-required-value-may-differ (wall clock). |
| `/clusters` | GET | text/plain | Set-equal `<cluster_name>::observability_name::<name>` + `<cluster_name>::default_priority::endpoints` lines per Envoy v1.33's plain-text format. Per-endpoint numeric fields (success/error/timeout counts) name-required-value-may-differ; envoy-rust at 08.1 emits only the minimum two lines per cluster (architecture-decision lock-in #10) — Envoy's richer output is allow-listed envoy-only on fixture 0014. Cluster output order is deterministic by name (sorted in `ClusterManager::clusters()`). |
| `/listeners` | GET | text/plain | Set-equal `<listener_name>::<address>:<port>` lines. Order: sorted-by-name (deterministic on both sides). **LDS extension (phase 19, L5/ADR-0050):** LDS-supplied listeners appear in the output alongside static ones — envoy-rust migrated the endpoint to enumerate the merged `all_listeners()` set (static + LDS-delivered), so fixture 0027's `dynamic_listener` line is emitted on both sides. The per-side address shapes are **prefix-matched** (Envoy binds `dynamic_listener::0.0.0.0:<port>`; envoy-rust binds `dynamic_listener::127.0.0.1:<kernel-ephemeral>`) — the differential harness matches on the `dynamic_listener::` line prefix bilaterally with per-side `allowlist_*_line_prefixes` for the address+port tail. |
| `/drain_listeners` | POST | empty | Status 200; empty body (`content-length: 0`); effect-only endpoint. Invokes `DrainState::drain()`. Sticky — repeat POSTs are idempotent. Both proxies emit 200 OK on first AND subsequent POSTs. |
| `/healthcheck/fail` | POST | empty | Status 200; empty body; effect-only endpoint. Invokes `DrainState::fail_healthcheck()`. Flips `/ready` to 503 (per parent-08 SPEC §5.5 wire-state mapping); `/server_info.state` stays `"LIVE"` (server-state is independent of healthcheck-failure). |
| `/healthcheck/ok` | POST | empty | Status 200; empty body; effect-only endpoint. Invokes `DrainState::ok_healthcheck()`. Restores from `HealthcheckFailing` → `Live`. Sticky-drain: `/healthcheck/ok` AFTER `/drain_listeners` does NOT un-drain (the `HealthcheckFailing → Live` compare_exchange fails silently against the `Draining` state). |

---

## Admin-action effect equivalence

> Authored per phase 08.2 SPEC §2.3. States the cross-proxy invariant that
> admin-action POSTs (`/drain_listeners`, `/healthcheck/fail`,
> `/healthcheck/ok`) must drive observable wire-level effects on both
> proxies. The internal mechanism is implementation-specific; only the
> wire-level observable is contract.
>
> **For `POST /drain_listeners`, the bilateral wire-level invariant is
> `data_plane_connection_refused` on the data-plane listener** —
> kernel-side ECONNREFUSED / immediate-EOF / RST within the 5s
> `DRAIN_BUDGET`. The admin-bookkeeping `/ready` flip is NOT a
> bilateral invariant on `/drain_listeners`: upstream Envoy v1.33's
> `/ready` does NOT flip to 503 on `POST /drain_listeners` without the
> server-level `--drain-strategy immediate` CLI flag (NOT
> bootstrap-configurable); envoy-rust per parent-08 SPEC §5.5 flips
> `/ready` immediately on drain. Fixture 0015 (D17.2) therefore pairs
> the `data_plane_connection_refused` post-assertion (the bilateral
> wire-level invariant) with a `/server_info` JSON scrape (bilaterally
> 200-with-JSON across the drain transition; `state` key presence is
> the bilateral structural invariant; `state` VALUE is permitted to
> differ across proxies). The envoy-rust-side `/ready=503 DRAINING`
> flip is verified in isolation by the in-process backstop at
> `crates/envoy-bin/tests/admin_drain_listeners.rs` (Task 10), which
> does not face the cross-proxy `--drain-strategy` asymmetry. The
> `/healthcheck/fail` + `/healthcheck/ok` rows below DO assert the
> bilateral `/ready` flip because both proxies flip `/ready`
> synchronously on those endpoints (no CLI-flag gap).

| Action | Wire-level invariant |
|---|---|
| `POST /drain_listeners` | Both proxies MUST refuse-or-immediately-close new connections on their data-plane listeners within the drain window (5s `DRAIN_BUDGET`). The harness `AdminAssertion::DataPlaneConnectionRefused { listener_address, within_ms }` polls for ECONNREFUSED OR immediate-EOF on connect; either disposition satisfies the invariant. Admin listener stays serving during drain (operator reachability per parent-08 SPEC §5.5). Sticky — subsequent `POST /healthcheck/ok` does NOT un-drain. |
| `POST /healthcheck/fail` | Both proxies MUST flip `/ready` to 503 within 100ms; `/server_info.state` stays `"LIVE"` (server-state independent of healthcheck-failure). |
| `POST /healthcheck/ok` | Both proxies MUST flip `/ready` back to 200 within 100ms IF and ONLY IF current state is `HealthcheckFailing`; if current state is `Draining`, the action is a no-op (sticky drain). |

---

## Access log field mapping

> **To be filled per-phase as needed.**
>
> Upstream Envoy's default-format access log is specified as a fixed sequence
> of substitution tokens (`%START_TIME%`, `%REQ(…)%`, `%RESPONSE_CODE%`, etc.).
> envoy-rust must reproduce every token's semantic content, but the underlying
> data source inside envoy-rust may differ. This section records the mapping
> from token → envoy-rust internal field so the harness can diff accurately.
>
> Populated in phase 06 when access logs first ship. Extended whenever a new
> filter adds new log-only fields.

**06.2 first-time population (per parent-06 SPEC §2.2).** Envoy's default
access-log format (per upstream Envoy v1.33's documentation) is a fixed
sequence of 14 tokens emitted per request:

```
[%START_TIME%] "%REQ(:METHOD)% %REQ(X-ENVOY-ORIGINAL-PATH?:PATH)% %PROTOCOL%" %RESPONSE_CODE% %RESPONSE_FLAGS% %BYTES_RECEIVED% %BYTES_SENT% %DURATION% %RESP(X-ENVOY-UPSTREAM-SERVICE-TIME)% "%REQ(X-FORWARDED-FOR)%" "%REQ(USER-AGENT)%" "%REQ(X-REQUEST-ID)%" "%REQ(:AUTHORITY)%" "%UPSTREAM_HOST%"
```

Tokens absent on a given record (e.g., `%REQ(USER-AGENT)%` when the
request did not carry a `User-Agent:` header) emit `-` in their
position. Quoted tokens emit `"-"` (a literal `"-"` between the
surrounding quotes).

| Token | envoy-rust internal source | Equivalence disposition | Rationale |
|---|---|---|---|
| `%START_TIME%` | `AccessLogRecord.start_time: SystemTime`, formatted by `default_format::format_iso8601` as `YYYY-MM-DDTHH:MM:SS.sssZ` (UTC, ms resolution). Captured at HCM `serve_connection` request-arrival time. | name-required, value-may-differ | Wall-clock non-determinism: the two proxies stamp the response at slightly different instants. The harness asserts ISO-8601 parse via `AccessLogLineRule::Iso8601Format`. |
| `%REQ(:METHOD)%` | `AccessLogRecord.method`, sourced from `Request.method` at HCM record-build time. | value-exact | Both proxies receive the same method bytes; rendering matches. |
| `%REQ(X-ENVOY-ORIGINAL-PATH?:PATH)%` | `AccessLogRecord.path`, populated at HCM record-build time by checking `Request.headers` for `x-envoy-original-path` (case-insensitive); if present, that value; else `Request.path`. | value-exact | Both proxies see the same request bytes; both render the same path. |
| `%PROTOCOL%` | `AccessLogRecord.protocol`, determined by the dispatch path: `"HTTP/1.1"` on the H1 HCM (`envoy_http1::hcm`), `"HTTP/2"` on the H2 HCM (`envoy_http2::hcm`). | value-exact | The protocol is fixed by which HCM module is dispatching; both proxies emit the same string. |
| `%RESPONSE_CODE%` | `AccessLogRecord.response_code: u16`, sourced from `Response.status`. | value-exact | Both proxies route the request through the same VH/route/action; both produce the same response code. |
| `%RESPONSE_FLAGS%` | `AccessLogRecord.response_flags: String`. 06.2 always emits the literal `"-"` (Envoy's no-flags sentinel). Future fixtures exercising non-`-` flag combinations need per-flag equivalence rules added to this table. | value-exact (06.2 no-flags case) | Fixture 0012's direct_response happy-path produces `-`; both proxies emit `-`. |
| `%BYTES_RECEIVED%` | `AccessLogRecord.bytes_received: u64`, from `Request.body.as_ref().map_or(0, |b| b.len() as u64)`. Header bytes NOT counted (matches Envoy's semantic). | value-exact | Both proxies see the same wire request body bytes. |
| `%BYTES_SENT%` | `AccessLogRecord.bytes_sent: u64`, from `response.body.len() as u64`. Symmetric to `%BYTES_RECEIVED%`. | value-exact | Both proxies render the same response body bytes. |
| `%DURATION%` | `AccessLogRecord.duration: Duration`, from `start.elapsed()` at HCM record-build time. Rendered as integer milliseconds via `Duration::as_millis()`. | name-required, value-may-differ | Per-request wall-clock latency diverges across runtimes/processes/HCM impls. The harness asserts non-negative-integer parse via `AccessLogLineRule::DurationMs`. |
| `%RESP(X-ENVOY-UPSTREAM-SERVICE-TIME)%` | `AccessLogRecord.upstream_service_time: Option<Duration>`, populated at HCM record-build time by reading `Response.headers` for `x-envoy-upstream-service-time`. When present (router-proxy path per 04.3 emission), rendered as `Duration::as_millis()`; when absent (direct_response path), rendered as literal `-`. | name-required, value-may-differ (when present); value-exact `-` (when absent on direct_response paths) | The header value's equivalence is inherited from the 04.3-landed `Header allow-list` row for the same header. Fixture 0012's direct_response path produces `-` on both sides. |
| `%REQ(X-FORWARDED-FOR)%` | `AccessLogRecord.forwarded_for: Option<String>`, read from `Request.headers` (lowercased per the 04.x normalization posture). | value-exact | If present on the request both proxies see the same bytes; if absent both emit `-`. |
| `%REQ(USER-AGENT)%` | `AccessLogRecord.user_agent: Option<String>`, sourced symmetrically. | value-exact | Same rationale as `%REQ(X-FORWARDED-FOR)%`. |
| `%REQ(X-REQUEST-ID)%` | `AccessLogRecord.request_id: Option<String>`, sourced symmetrically. envoy-rust never injects `x-request-id` per 04.3 SPEC §4; fixture 0012's `envoy.yaml` sets `generate_request_id: false` to align both proxies on the omit-injection posture. | value-exact | Both proxies omit injection; both render `-`. |
| `%REQ(:AUTHORITY)%` | `AccessLogRecord.authority: Option<String>`, populated from the `Host:` header on the H1 path (envoy_http1::codec produces this from the request-line) or the `:authority` pseudo-header on the H2 path (translated by 05.2 D3's adapter). | value-exact | Both proxies see the same wire-level request authority; both render the same value. |
| `%UPSTREAM_HOST%` | `AccessLogRecord.upstream_host: Option<String>`, populated at HCM record-build time from the router-arm's resolved upstream `SocketAddr` (formatted via `SocketAddr` Display). `None` on direct_response paths. | value-exact `-` (direct_response, fixture 0012); value-exact for STRICT_DNS single-A-record resolution; name-required, value-may-differ for multi-A non-deterministic resolution | Fixture 0012's direct_response path produces `-`; both proxies emit `-`. Future router-proxy fixtures use STRICT_DNS with single-A resolution (matches the 04.3 fixture 0008 / 05.3 fixture 0010 posture). |

Format-string customization is OUT of scope in 06.2. The `envoy-config`
validator at `validate_access_logs` rejects non-`envoy.access_loggers.file`
access-log names and fixtures that supply format strings on the FileAccessLog
typed_config (the `format` / `log_format` / `json_format` / `typed_json_format`
fields on the upstream proto are not in envoy-rust's `FileAccessLog` struct;
serde `deny_unknown_fields` rejects them). Future observability-family phases
extend this section with new tokens (`%FILTER_STATE%`, `%DYNAMIC_METADATA%`,
`%RESPONSE_CODE_DETAILS%`, etc.) when the corresponding machinery lands.

---

## xDS wire state machine

> **To be filled per-phase as needed.**
>
> The xDS state machine describes the legal sequence of
> `DiscoveryRequest` / `DiscoveryResponse` messages on both SotW (State of the
> World) and delta streams: which version and nonce fields are populated in
> which direction, how ACK and NACK are represented, how initial-fetch timeouts
> manifest, and how reconnection + resource caching interact. envoy-rust must
> match this state machine on the wire; effective-config snapshots must match
> upstream Envoy's config_dump on identical inputs.
>
> Populated when the xDS family (§9 of `BOOTSTRAP_PROMPT.md`) enters
> `in-progress`.

### Filesystem transport (`path_config_source`) — phase 18

> The xDS-family opener (ADR-0048 SPEC / ADR-0049 PLAN). file-based CDS loads
> clusters from `dynamic_resources.cds_config.path_config_source.path` at
> startup. All findings below are the §6.2 empirical lock-ins (L1–L12),
> verified against `envoyproxy/envoy:v1.33.0` (digest `sha256:56da5afd…`,
> 2026-06-02) and reconciled by ADR-0049; what is bilaterally asserted lives in
> fixture 0026, the negative paths live in the in-process backstop
> (`crates/envoy-bin/tests/xds_file_based_cds.rs`).

**(a) The CDS file envelope (L1).** Both the bare `resources:` list AND the full
`DiscoveryResponse` shape (`version_info` + `resources`) are accepted; Envoy
treats `version_info` as load-bearing, envoy-rust accepts-and-ignores it. Each
resource MUST carry an `@type` (omitting it → Envoy `update_failure: 1` + log
`missing @type in Any is only allowed for an empty object` + the route 503s);
CDS files carry Cluster resources only. The byte-exact minimal working CDS file:

```yaml
resources:
- "@type": type.googleapis.com/envoy.config.cluster.v3.Cluster
  name: dynamic_backend
  type: STRICT_DNS
  dns_lookup_family: V4_ONLY
  load_assignment:
    cluster_name: dynamic_backend
    endpoints:
    - lb_endpoints:
      - endpoint:
          address:
            socket_address: { address: host.docker.internal, port_value: 8124 }
```

**Recorded divergence (ADR-0049): parser selection.** Envoy selects its parser by
**file extension** — `.yaml`/`.yml` → YAML parser (which also accepts JSON
syntax); any other or absent extension → JSON-only parser (YAML content in a
`.json`/extensionless file fails with `update_failure`). envoy-rust's
`parse_cds_file` is **always-YAML** (`serde_yaml`, regardless of extension) —
strictly more lenient on non-`.yaml` extensions. No differential observable: the
fixture's CDS file ends in `.yaml` and the Envoy-side container path
(`/etc/envoy-cds/cds.yaml`) is structurally `.yaml`. envoy-rust requires the
`@type` per resource (the ADR-0014 internally-tagged-on-`@type` pattern; a
non-Cluster `@type` rejects loudly).

**(b) Initial-load / readiness ordering (L2).** Readiness implies loaded on both
proxies. Envoy's startup log order: `cds: add 1 cluster(s)` → `cm init: all
clusters initialized` → `all dependencies initialized. starting workers`; the
dynamic cluster is routable the instant `/ready` first returns 200. envoy-rust
mirrors this naturally — `load_dynamic_resources` runs **synchronously** (a
`std::fs::read_to_string` between `parse_bootstrap` and `ClusterManager`
construction, before listeners bind). No settle/timing machinery is needed on
either side; fixture 0026's single GET fires after readiness and routes through
the CDS-supplied cluster.

**(c) Negative-path disposition (L4) — recorded divergence (ADR-0049).** This is
the load-bearing reconciliation. Envoy's disposition is a **3-way split**:

| CDS load fault | Envoy | envoy-rust |
|---|---|---|
| Nonexistent `path` | hard startup failure (container exits non-zero; `paths must refer to an existing path in the system` — a bootstrap-level PGV check) | **FATAL** (`CdsFileError`; process exits) — agrees with Envoy on this one class |
| File exists, malformed YAML/JSON | **starts and serves** (`/ready` 200), `cluster_manager.cds.update_failure: 1`, `active_clusters: 0`, the route 503s | **FATAL** (`CdsParseError`; process exits) — **diverges** |
| Valid YAML, semantically-invalid resource (PGV violation, e.g. empty `name`; cluster-build failure) | starts and serves, `update_rejected: 1` ticks (NOT `update_failure`), the route 503s | **FATAL** (per-cluster `validate_cluster` failure; process exits) — **diverges** |
| Unknown field inside a resource | **warn-accepted** (lenient protobuf parsing) | **FATAL** (`deny_unknown_fields` on the `Cluster` schema; process exits) — **diverges** |
| STRICT_DNS cluster with no `load_assignment` (zero-endpoint) | accepted as a zero-endpoint cluster (route → `no healthy upstream` 503) | **FATAL** (the existing `EmptyClusterEndpoints` invariant) — **diverges** |

envoy-rust treats **ALL CDS load errors as FATAL at startup** — the project's
fail-loud posture (every deferred field rejects loudly today). The warn-and-serve
alternative would require honoring `validate_clusters: false` at runtime + a
503-on-unknown-cluster data-plane path — machinery with zero differential
coverage (a deliberately-broken Envoy-side fixture is not a thing this project
does). **Consequence for the stats contract:** `cluster_manager.cds.update_failure`
and `cluster_manager.cds.update_rejected` register at 0 and are **structurally
unreachable non-zero** in envoy-rust (the process exits before any non-zero
state). fixture 0026 asserts both at 0 bilaterally (satisfiable on both sides — a
successful load); the negative paths are **backstop-only** (Envoy exits the
process on a fatal CDS error, which the differential harness cannot observe as a
data-plane response).

**(d) Static/dynamic name collision: STATIC WINS (L9) — ADR-0049.** A cluster
defined both statically and in the CDS file: **both proxies keep the STATIC one
and skip the CDS entry** as unmodified; no error, no startup failure. Envoy logs
`added/updated 0 cluster(s), skipped 1 unmodified cluster(s)`, `update_success`
still ticks 1, `/config_dump` shows the cluster under `static_clusters` only, and
the data plane serves the static endpoint. envoy-rust mirrors — on collision the
dynamic cluster is SKIPPED (with a `tracing::warn!`), the static cluster wins, no
error. (This reverses the SPEC D1 projection; the projected `DuplicateClusterName`
ConfigError variant was DROPPED. The backstop asserts the static endpoint's
distinct body serves on the data plane and that `dynamic_active_clusters` is
absent.)

**(e) Bootstrap prerequisites (L12) — recorded divergence (ADR-0049).**
- **`node.id` + `node.cluster` are REQUIRED by Envoy when CDS is configured** —
  without them Envoy exits at startup (`node 'id' and 'cluster' are required`).
  Both fixture sides carry a `node:` block (every existing fixture already does);
  envoy-rust parses `Node { id, cluster }` (phase 01) but adds **no mirror
  requirement validator** (both sides are always configured; no differential
  observable).
- **The static `route_config` referencing a CDS-supplied cluster requires
  `validate_clusters: false`** — without it Envoy exits at startup (`route:
  unknown cluster 'dynamic_backend'`), because Envoy's inline route-table
  validation runs against the static cluster set only. Both fixture sides set it.
  envoy-rust gains `RouteConfiguration.validate_clusters: Option<bool>` as
  **parse-and-accept** (the ADR-0024/0026 parse-only precedent) and does **NOT**
  honor its literal runtime-503 semantics. Instead envoy-rust enforces references
  via **defer-then-revalidate**: cluster-reference checks DEFER while
  `dynamic_resources` is configured-but-unloaded (`Bootstrap::cds_configured_but_unloaded()`)
  and RE-ENFORCE post-merge (inside `load_dynamic_resources`, against the
  effective static+dynamic list). **Recorded narrow divergence:** a route to a
  cluster in NEITHER list still **fails envoy-rust startup** (`UnknownCluster`),
  vs Envoy's runtime-503 under `validate_clusters: false`.

**(f) gRPC/ADS message-sequence state machine: UNPOPULATED.** The SotW/delta
`DiscoveryRequest`/`DiscoveryResponse` wire sequence (version/nonce population,
ACK/NACK representation, init-fetch timeouts, reconnection + resource caching)
remains **deferred to the gRPC-xDS phase**, which also **supersedes ADR-0014**
(the YAML-native typed-config shim) per ADR-0048. The intro blockquote above
describes that machine; phase 18 populates only the filesystem transport, which
has no on-the-wire message sequence (it is a synchronous file read).

### Filesystem transport (`path_config_source`) — phase 19 LDS extension

> The xDS-family continuation (ADR-0050 SPEC / PLAN). file-based LDS loads
> listeners from `dynamic_resources.lds_config.path_config_source.path` at
> startup. The lock-ins below (L1–L10) are the §6.2 empirical findings, verified
> against `envoyproxy/envoy:v1.33.0` (digest `sha256:56da5afd…`, 2026-06-02) and
> reconciled by ADR-0050; what is bilaterally asserted lives in fixture 0027, the
> negative/fatal paths + the static/dynamic collision live in the in-process
> backstop (`crates/envoy-bin/tests/xds_file_based_lds.rs`). The LDS transport
> mirrors the phase-18 CDS transport structurally — the per-finding letters below
> intentionally parallel the CDS §(a)–(f).

**(a) The LDS file envelope (L1).** Same dual-envelope posture as CDS: both the
bare `resources:` list AND the full `DiscoveryResponse` shape (`version_info` +
`resources`) are accepted; Envoy treats `version_info` as load-bearing, envoy-rust
accepts-and-ignores it. Each resource MUST carry an `@type` (omitting it → Envoy
`lds.update_failure: 1`); LDS files carry **Listener** resources only, with the
type URL `type.googleapis.com/envoy.config.listener.v3.Listener`. envoy-rust's
`parse_lds_file` is **always-YAML** (`serde_yaml`, regardless of extension — the
same strictly-more-lenient stance as `parse_cds_file`; the Envoy-side container
path is structurally `.yaml`) and **requires** the `@type` per resource (the
ADR-0014 internally-tagged-on-`@type` pattern; a non-Listener `@type` rejects
loudly).

**(b) Initial-load / readiness ordering (L2).** Readiness implies loaded on both
proxies; the dynamic listener accepts connections the instant `/ready` first
returns 200. Envoy's startup log order with zero static listeners +
both LDS+CDS configured: `loading 0 listener(s)` → cds init → `cds: add N
cluster(s)` → `cm init: all clusters initialized` → `lds: add/update listener
'dynamic_listener'` → `all dependencies initialized. starting workers`.
**Clusters initialize BEFORE listeners are added** — this mirrors the §5.7
merge-ordering invariant (the dynamic listener's route_config can reference the
CDS-supplied cluster only because clusters land first). envoy-rust mirrors
naturally: `load_dynamic_resources` runs **synchronously** (CDS merge then LDS
merge, before listeners bind), so the sync-load order reproduces Envoy's
cds→clusters→lds→workers sequence. No settle/timing machinery is needed; fixture
0027's two GETs fire after readiness.

**(c) Negative-path disposition (L4) — recorded divergence (ADR-0050).** Envoy's
LDS disposition is the **same 3-way split** as its CDS split:

| LDS load fault | Envoy | envoy-rust |
|---|---|---|
| Nonexistent `path` | hard startup failure (container exits non-zero; `paths must refer to an existing path in the system` — a bootstrap-level PGV check) | **FATAL** (process exits) — agrees with Envoy on this one class (backstop path (ii)) |
| File exists, malformed YAML / missing `@type` | **starts and serves** (`/ready` 200), `listener_manager.lds.update_failure: 1` | **FATAL** (process exits) — **diverges** (backstop path (iii)) |
| Valid YAML, semantically-invalid listener (PGV violation) | starts and serves, `lds.update_rejected: 1` ticks (NOT `update_failure`) | **FATAL** (process exits) — **diverges** (backstop path (iv)) |
| Unknown field inside a resource | **warn-accepted** (lenient protobuf parsing) | **FATAL** (`deny_unknown_fields`; process exits) — **diverges** |

envoy-rust treats **ALL LDS load errors as FATAL at startup** — the ADR-0049
decision-2 all-fatal posture extended to LDS (pre-ratified by ADR-0050): missing/
unreadable file, malformed YAML, missing `@type`, unknown fields, per-listener
validation failure all exit the process before construction completes.
**Consequence for the stats contract:** `listener_manager.lds.update_failure` and
`listener_manager.lds.update_rejected` register at 0 and are **structurally
unreachable non-zero** in envoy-rust. fixture 0027 asserts both at 0 bilaterally
(satisfiable on both sides — a successful load); the negative paths are
**backstop-only** (Envoy exits the process on a fatal LDS error, which the
differential harness cannot observe as a data-plane response).

**(d) Static/dynamic listener name collision: STATIC WINS (L7) — ADR-0050.** A
listener defined both statically and in the LDS file: **both proxies keep the
STATIC one and skip the LDS entry**; only the static listener's port binds, no
error, no startup failure. Envoy: `lds.update_success` still ticks 1,
`listener_added: 1`, `/config_dump` shows `static_listeners` only, no
error/warning log. envoy-rust mirrors — on collision the dynamic listener is
SKIPPED (with a `tracing::warn!`), the static listener wins. The backstop (path
(v)) asserts the static listener's port serves while the LDS listener's port
refuses connections, and `listener_added == 1` / `total_listeners_active == 1`.

**(e) LDS+CDS composition + the LDS-route validation divergence (L6) — recorded
divergence (ADR-0050).** The composition works on both proxies (both `/static`
and `/dynamic` routes return 200). The `route_config` inside an LDS-supplied
listener does **NOT** require `validate_clusters: false` on Envoy — **Envoy skips
inline route-table cluster validation entirely for dynamically-delivered
listeners** (no `validate_clusters` knob needed; the check that CDS-delivered
static routes need suppressed simply does not run for LDS listeners). envoy-rust's
posture is **UNCHANGED** from phase 18: dynamic-listener routes go through the
same **defer-then-revalidate** enforcement (cluster-reference checks defer while
`dynamic_resources` is configured-but-unloaded, then RE-ENFORCE post-merge inside
`load_dynamic_resources` against the effective static+dynamic list). **Recorded
narrow divergence:** an LDS-listener route to a cluster in **NEITHER** list
**fails envoy-rust startup** (`UnknownCluster`), vs Envoy's start-and-runtime-503
— extending ADR-0049 decision-4's class to LDS routes (per ADR-0050 / SPEC §5.7).
`node.id` + `node.cluster` apply identically (both fixture sides carry a `node:`
block).

**(f) L10 conditionality narrowing (recorded divergence — ADR-0050).** On the
fixture-0026 topology (CDS configured, NO `lds_config`): Envoy emits ZERO
`listener_manager.lds.*` names but DOES emit the base `listener_manager.*` names
(`listener_added`, `listener_create_success`, `total_listeners_active`,
`workers_started`) **unconditionally**, AND a `ListenersConfigDump` entry for the
static-only listeners (at `configs[2]`). envoy-rust **gates both** on `lds_config`:
the 4 `lds.*` names + the base `listener_added` register only with `lds_config`
configured, and the `ListenersConfigDump` entry is emitted only with `lds_config`
configured (`total_listeners_active` is the sole exception — unconditional, per
its 08.2 registration). The backstop's inertness path (vi) verifies this on a
CDS-only bootstrap: no `lds.*` names, no `listener_added`, and `/config_dump` does
NOT contain `"ListenersConfigDump"`.

### Filesystem transport (`path_config_source`) — phase 20 RDS extension

> The xDS-family continuation (ADR-0051 SPEC / ADR-0052 PLAN). file-based RDS
> loads route tables from `rds.config_source.path_config_source.path` on the HCM
> at startup — completing the CDS+LDS+RDS filesystem triad. The lock-ins below
> (L1–L11) are the §6.2 empirical findings, verified against
> `envoyproxy/envoy:v1.33.0` (digest `sha256:56da5afd…`, 2026-06-02) and
> reconciled by ADR-0052; what is bilaterally asserted lives in fixture 0028, the
> negative/fatal paths + the exactly-one-of dispositions live in the in-process
> backstop (`crates/envoy-bin/tests/xds_file_based_rds.rs`). The RDS transport
> mirrors the phase-18 CDS / phase-19 LDS transports structurally — the
> per-finding letters below intentionally parallel the CDS/LDS §(a)–(f).

**(a) The RDS file envelope (L1).** Same dual-envelope posture as CDS/LDS: both the
bare `resources:` list AND the full `DiscoveryResponse` shape (`version_info` +
`resources`) are accepted; Envoy treats `version_info` as load-bearing, envoy-rust
accepts-and-ignores it. Each resource MUST carry an `@type` with the type URL
`type.googleapis.com/envoy.config.route.v3.RouteConfiguration`; RDS files carry
**RouteConfiguration** resources only. The `rds`-on-HCM config shape is
`rds: { route_config_name, config_source: { path_config_source: { path }, resource_api_version? } }`.
envoy-rust's RDS parse is **always-YAML** (`serde_yaml`, regardless of extension —
the same strictly-more-lenient stance as `parse_cds_file`/`parse_lds_file`; the
Envoy-side container path is structurally `.yaml`) and **requires** the `@type`
per resource (the ADR-0014 internally-tagged-on-`@type` pattern; a
non-RouteConfiguration `@type` rejects loudly).

**(b) Initial-load / readiness ordering (L2).** Readiness implies loaded on both
proxies; the RDS route table is **active before `/ready` first returns 200** —
**no warm-up**. Envoy loads the route table at HCM construction (the RDS
`config_source` resolves synchronously for filesystem transport) so the route is
routable the instant the listener serves. envoy-rust mirrors this naturally via a
**synchronous load** (the RDS file is read between bootstrap parse and HCM
construction, before listeners bind); fixture 0028's GET fires after readiness and
routes through the RDS-supplied route table.

**(c) Negative-path disposition (L4) — recorded divergence (ADR-0052).** Envoy's
RDS disposition is the **same 3-way split** as its CDS/LDS splits:

| RDS load fault | Envoy | envoy-rust |
|---|---|---|
| Nonexistent `path` | hard startup failure (container exits non-zero; `paths must refer to an existing path in the system` — a bootstrap-level PGV check) | **FATAL** (`RdsFileError`; process exits) — agrees with Envoy on this one class |
| File exists, malformed YAML / missing `@type` | **starts and serves** (`/ready` 200), `http.<prefix>.rds.<name>.update_failure: 1` | **FATAL** (`RdsParseError`; process exits) — **diverges** |
| Valid YAML, semantically-invalid route config (PGV violation) | starts and serves, `rds.<name>.update_rejected: 1` ticks (NOT `update_failure`) | **FATAL** (process exits) — **diverges** |
| `route_config_name` mismatch (the file's `RouteConfiguration.name` ≠ the HCM's `rds.route_config_name`) (L6) | starts and serves, `rds.<name>.update_rejected: 1` + runtime 404 (the named route table never installs) | **FATAL** (`RdsRouteConfigNotFound`; process exits) — **diverges** |
| Unknown field inside a resource | **warn-accepted** (lenient protobuf parsing) | **FATAL** (`deny_unknown_fields`; process exits) — **diverges** |

envoy-rust treats **ALL RDS load errors as FATAL at startup** — the ADR-0049
decision-2 all-fatal posture extended to RDS (`RdsFileError`/`RdsParseError` fatal
at startup): missing/unreadable file, malformed YAML, missing `@type`, unknown
fields, `route_config_name` mismatch, per-route validation failure all exit the
process before construction completes. **Consequence for the stats contract:**
`http.<prefix>.rds.<name>.update_failure` and `…update_rejected` register at 0 and
are **structurally unreachable non-zero** in envoy-rust. fixture 0028 asserts both
at 0 bilaterally (satisfiable on both sides — a successful load); the negative
paths are **backstop-only** (Envoy exits the process on a fatal RDS error, which
the differential harness cannot observe as a data-plane response).

**(d) The exactly-one-of route-source disposition (L9) — ADR-0052.** An HCM's
route source is an **exactly-one-of** between the inline `route_config` and the
`rds` reference. **Both** sources present, OR **neither** present, are **FATAL on
both proxies** — no differential divergence:

| Route-source fault | Envoy | envoy-rust |
|---|---|---|
| Both `route_config` AND `rds` set | hard startup failure (protobuf `oneof` reject) | **FATAL** (`AmbiguousRouteSource`; parse-time exactly-one-of check) — **agrees** |
| Neither set | hard startup failure (PGV `route_specifier required`) | **FATAL** (`MissingRouteSource`; parse-time exactly-one-of check) — **agrees** |

envoy-rust enforces the disposition at **parse time** (the exactly-one-of check on
the HCM config), matching Envoy's startup-fatal disposition on both arms.

**(e) RDS+CDS composition + the route-revalidation divergence (L7) — recorded
divergence (ADR-0052).** An RDS-supplied route to a CDS-supplied cluster resolves
at **initial load**: **CDS merges BEFORE the RDS-route re-validation** (the §5.7
merge-ordering invariant — clusters land first, then the RDS route table
re-validates its cluster references against the effective static+dynamic list). An
RDS→CDS route needs **NO `validate_clusters: false`** — RDS behaves like LDS (the
dynamically-delivered route table is not subject to the static inline-validation
that CDS-static routes need suppressed), **not like a CDS-static route**; the
ADR-0050 L6 finding (LDS routes skip inline cluster validation) is **confirmed for
RDS**. envoy-rust's posture is the same **defer-then-revalidate** as phases 18/19:
cluster-reference checks defer while `dynamic_resources` is configured-but-unloaded,
then RE-ENFORCE post-merge. **Recorded narrow divergence:** an RDS-route to a
cluster in **NEITHER** list **fails envoy-rust startup** (`UnknownCluster`), vs
Envoy's start-and-runtime-503 — the same defer-then-revalidate narrow divergence
recorded for CDS (ADR-0049 §(e)) and LDS (ADR-0050 §(e)).

**(f) L5 conditional-emission narrowing (recorded divergence — ADR-0052).**
envoy-rust emits a `RoutesConfigDump` `/config_dump` entry **ONLY when some HCM
uses `rds`**; vs Envoy's **always-emitted** `RoutesConfigDump` (Envoy emits it with
`static_route_configs` even without any RDS — the inline route tables surface
there). On fixture 0028 the entry lands at **different `configs[]` indices** per
side, reconciled by a per-side `JsonSubtreeRule` path override in the harness:

| Side | `configs[]` layout (fixture 0028) | RoutesConfigDump index |
|---|---|---|
| Envoy | Bootstrap[0] / Clusters[1] / Listeners[2] / ScopedRoutes[3] / Routes[4] / Secrets[5] | `configs[4]` |
| envoy-rust | Bootstrap[0] / Clusters[1] / Routes[2] (Listeners gated off — no `lds_config` on 0028) | `configs[2]` |

The per-side path override bridges the index gap; **fixtures 0026/0027 hold** —
their Clusters[1] / Listeners[2] assertions are NOT displaced (the RoutesConfigDump
entry is RDS-conditional and absent on those topologies).

**Note (L8): the RDS file is SHAREABLE.** Unlike the per-side LDS templates (the
LDS file's static-listener address differs per proxy), one `rds.yaml` is consumed
**verbatim by both proxies** — the RDS route table carries no per-side address. A
single fixture file serves both Envoy and envoy-rust.

**Note (L10): an inline-route HCM emits zero `http.<prefix>.rds.*` names.** The
conditional registration (§Stat-name mapping) means an HCM whose route table is the
static inline `route_config` (no `rds`) participates in NO RDS update lifecycle and
registers none of the 5 `rds.*` names — verified by the backstop's inertness path
and by the 27 pre-existing fixtures seeing zero new names.

**Note (L11): version is Envoy-only.** Envoy's RDS update carries a `version_info`
(load-bearing on the wire); envoy-rust accepts-and-ignores it (per §(a)), and the
`rds.<name>.version` / `version_text` stats are **Envoy-only, not asserted** (per
the §Stat-name mapping Envoy-only enumeration).

---

## Timing tolerances

> **To be filled per-phase as needed.**
>
> Timing is not compared by default: envoy-rust and upstream Envoy run inside
> different processes/containers under different runtimes, so absolute latency
> numbers are incomparable in CI. A phase may opt in to a latency bound when
> the feature is fundamentally time-sensitive (e.g. outlier-detection
> ejection windows, timeout filter semantics, rate-limit windows). Every such
> opt-in records:
>
> - which metric is being bounded (p50, p99, absolute delta, count-in-window, …);
> - the bound itself and its justification;
> - whether the bound is one-sided (envoy-rust must not be slower than X) or
>   symmetric (both must lie within a shared window).
>
> Default: no opt-in, no timing comparison.

_(empty; no phase has opted in yet)_
