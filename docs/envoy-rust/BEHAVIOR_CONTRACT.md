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
| `cluster.<name>.upstream_cx_total` | name-required, value-may-differ | Counter; one increment per established upstream TCP connection. Envoy's stat semantics are "per-established-connection-from-the-pool" with default connection pooling enabled; envoy-rust under the no-pooling regime (per phase-04.3 / 05.3 posture) increments once per upstream call. Both are correct under their respective contracts. When connection pooling lands (upstream-robustness family), the disposition tightens to value-exact. |
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
| `/config_dump` | GET | JSON object | Top-level shape `{ "configs": [...] }`. envoy-rust emits exactly one entry: `{ "@type": "type.googleapis.com/envoy.admin.v3.BootstrapConfigDump", "bootstrap": <static-bootstrap-as-JSON>, "last_updated": <ISO-8601 timestamp> }`. Envoy may emit additional entries for xDS-derived configs; those land on `allowlist_envoy_only`. `bootstrap.static_resources` content value-exact-after-roundtrip (modulo serde renamings; the harness's `JsonShape::required_subtree` covers this). `last_updated` name-required-value-may-differ (wall-clock non-determinism). |
| `/server_info` | GET | JSON object | Required keys `state`, `version`, `node`, `uptime_current_epoch_seconds`, `uptime_all_epochs_seconds`, `hot_restart_version`, `command_line_options`. `state` value-exact, sourced from `DrainState::current()` via the mapping `Live | HealthcheckFailing → "LIVE"`, `Draining → "DRAINING"` (08.1 emitted the literal constant `"LIVE"` as a placeholder; 08.2's D5e patches the value-binding source at Task 5 — the struct shape is unchanged at the 08.1 → 08.2 boundary); `node.*` value-exact from the parsed bootstrap; `version` + `hot_restart_version` + `command_line_options` allowlist-each-side (envoy-rust emits its own version string; Envoy emits its own); `uptime_*` name-required-value-may-differ (wall clock). |
| `/clusters` | GET | text/plain | Set-equal `<cluster_name>::observability_name::<name>` + `<cluster_name>::default_priority::endpoints` lines per Envoy v1.33's plain-text format. Per-endpoint numeric fields (success/error/timeout counts) name-required-value-may-differ; envoy-rust at 08.1 emits only the minimum two lines per cluster (architecture-decision lock-in #10) — Envoy's richer output is allow-listed envoy-only on fixture 0014. Cluster output order is deterministic by name (sorted in `ClusterManager::clusters()`). |
| `/listeners` | GET | text/plain | Set-equal `<listener_name>::<address>:<port>` lines. Order: sorted-by-name (deterministic on both sides). |
| `/drain_listeners` | POST | empty | Status 200; empty body (`content-length: 0`); effect-only endpoint. Invokes `DrainState::drain()`. Sticky — repeat POSTs are idempotent. Both proxies emit 200 OK on first AND subsequent POSTs. |
| `/healthcheck/fail` | POST | empty | Status 200; empty body; effect-only endpoint. Invokes `DrainState::fail_healthcheck()`. Flips `/ready` to 503 (per parent-08 SPEC §5.5 wire-state mapping); `/server_info.state` stays `"LIVE"` (server-state is independent of healthcheck-failure). |
| `/healthcheck/ok` | POST | empty | Status 200; empty body; effect-only endpoint. Invokes `DrainState::ok_healthcheck()`. Restores from `HealthcheckFailing` → `Live`. Sticky-drain: `/healthcheck/ok` AFTER `/drain_listeners` does NOT un-drain (the `HealthcheckFailing → Live` compare_exchange fails silently against the `Draining` state). |

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

_(empty; populated when xDS family begins)_

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
