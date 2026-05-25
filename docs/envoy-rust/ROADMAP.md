# envoy-rust Roadmap

## Schema

Each phase row has the columns:

- **id** — stable numeric identifier (`00`, `01`, …; sub-phases `NN.1`, `NN.2`).
- **title** — one-line human-readable name.
- **depends-on** — space-separated phase ids that must be `done` before this one can enter `in-progress`.
- **status** — one of `planned | in-progress | blocked | done`.
- **sub-phases** — if the phase has been split (§6 of `BOOTSTRAP_PROMPT.md`), list the child ids here.
- **summary** — the differential surface this phase is expected to light up when it lands.

Rules:

- This file is **append-only history** — rows are never deleted. Only the `status` and `sub-phases` columns are mutated.
- `STATE.md` names the single active phase. A phase enters `in-progress` only when `STATE.md` points at it.
- When a phase is split, its own `status` becomes `in-progress` while its sub-phases land. The parent flips to `done` only after all sub-phases are `done`.

---

## MVP Trunk — phases 00 through 08

Phases 00–08 ship *in order*: each adds a primitive the next relies on. Splitting (§6) is still permitted inside any of them.

| id | title | depends-on | status | sub-phases | summary |
|---|---|---|---|---|---|
| 00 | Bootstrap: Cargo workspace layout, `rust-toolchain.toml`, `deny.toml`, CI, Docker reference Envoy, differential harness skeleton, `ENVOY_TARGET.md` pin, trivial echo fixture | — | done | — | harness boots; one TCP echo fixture green |
| 01 | Static bootstrap config loader (node, admin, static_resources skeleton) | 00 | done | — | config parses; admin `/ready` behaves like Envoy |
| 02 | Listener + TCP proxy filter + static cluster + round-robin LB (plaintext) | 01 | done | 02.1, 02.2 | TCP proxy fixture green |
| 02.1 | Config schema + cluster manager + echo-server helper | 01 | done | — | no new fixture; config parser + envoy-cluster + tcp-echo-server helper + fuzz corpus extended |
| 02.2 | Listener + TCP proxy filter + fixture 0003 + phase-01 rollovers I4/M1 | 02.1 | done | — | fixture 0003-tcp-proxy green; parent phase 02 flips done |
| 03 | Downstream TLS termination + upstream TLS origination + SNI | 02 | done | 03.1, 03.2 | TLS TCP fixture green |
| 03.1 | envoy-tls foundation + downstream TLS termination (single cert) + fixture 0004 | 02 | done | — | fixture 0004-tls-downstream green; envoy-tls scaffold + transport_socket schema + downstream TLS dispatch + harness PKI land |
| 03.2 | Upstream TLS origination + multi-cert SNI cert selection + tls-echo-server helper + fixtures 0005 + 0006 | 03.1 | done | — | fixtures 0005-tls-upstream + 0006-tls-sni green; parent phase 03 flips done |
| 04 | HTTP connection manager (HTTP/1.1) + route match + router filter + direct_response | 03 | done | 04.1, 04.2, 04.3 | HTTP/1.1 routing fixture green |
| 04.1 | HTTP/1.1 codec + HCM scaffold + minimal routing + direct_response + fixture 0007 | 03 | done | — | fixture 0007-http1-direct-response green; envoy-http1 crate + HCM as network filter + RouteConfig schema (prefix + path matchers) + direct_response action + harness Driver::Http1 land |
| 04.2 | HTTP route header matcher fan-out (all 7 modes) + ADR-0021 (regex permitted foundation) | 04.1 | done | — | no new fixture; fixture 0007 amended to exercise a matcher route; envoy-config gains all 7 HeaderMatcher modes + StringMatcher + invert_match; regex dep landed under ADR-0021 |
| 04.3 | Upstream HTTP/1.1 origination + router proxy arm + http1-echo-server helper + fixture 0008 | 04.2 | done | — | fixture 0008-http1-router-upstream green; envoy-http1::Client lands; router filter's Route arm proxies to cluster; parent phase 04 flips done |
| 05 | HTTP/2 downstream + upstream (low-level framer, own conn mgr) | 04 | done | 05.1, 05.2, 05.3, 05.4 | HTTP/2 fixture green; `h2spec` above threshold |
| 05.1 | Fixture-hardening preamble: `ClusterType::StrictDns` + 5-fixture coordinated edit + closes phase-02.1 REVIEW I3 (phase-04.3 REVIEW C-1 partially closed; full close deferred to follow-up sub-phase) | 04 | done | — | no new fixture; envoy-config schema gains `STRICT_DNS` cluster type; envoy-cluster `from_bootstrap` lifted to `async` with `tokio::net::lookup_host` STRICT_DNS branch; 5 fixture YAMLs flipped `STATIC` → `STRICT_DNS`; phase-02.1 REVIEW I3 closed; ADR-0023 landed; Docker-gated 5-fixture re-baseline NOT met (0008 surfaces a different defect post-flip; closure deferred to a follow-up sub-phase per REVIEW.md §5 R1) |
| 05.2 | `envoy-http2` foundation + downstream H2C HCM + fixture 0009 + `h2spec` ≥95% gate | 05.1 | done | — | fixture 0009-http2-direct-response green; envoy-http2 crate (sole dep on `h2`) + listener-side `Http2ProtocolOptions` + HCM-on-H2 dispatch + harness `Driver::Http2` + tests/conformance/h2spec/ runner at ≥95% pass |
| 05.3 | Upstream H2C origination + router H2-arm + http2-echo-server helper + fixture 0010 | 05.2 | done | — | fixture 0010-http2-router-upstream green; envoy-http2::Client lands; router H2-arm dispatches H1-or-H2 by cluster.upstream_protocol; parent phase 05 flips done |
| 05.4 | Fixture-hardening follow-up: 6 root-cause fixes substantively closing phase-04.3 REVIEW C-1 (helper bind 0.0.0.0; `dns_lookup_family: V4_ONLY`; envoy-config DnsLookupFamily schema; STRICT_DNS settle-time bump; envoy-http1 CL: 0 suppression; `tls_inspector` listener filter) | 05.1 | done | — | no new fixture; all 5 affected Docker-gated fixtures (0003/0004/0005/0006/0008) restored to green simultaneously + 3 unaffected (0001/0002/0007) remain green; envoy-config gains `Cluster.dns_lookup_family` field + `DnsLookupFamily` enum + `Listener.listener_filters` parse-and-ignore field; envoy-http1::client suppresses synthetic `content-length: 0` on empty-body GET (RFC 7230 §3.3.2 + Envoy v1.33 parity); 3 echo-server helpers bind 0.0.0.0; STRICT_DNS settle time 500ms → 2000ms for `host_gateway = true` fixtures; ADR-0024/0025/0026 landed; sibling under parent-05 (NOT a child of 05.1) per the 05.1 state-6 disposition decision |
| 06 | Access log (file sink, Envoy default format) + stats + Prometheus admin endpoint | 05 | done | 06.1, 06.2, 06.3 | access log + Prometheus fixtures green |
| 06.1 | `envoy-stats` foundation + `envoy-admin` HCM-backed listener migration + Prometheus exposition + fixture 0011 | 05 | done | — | fixture 0011-admin-stats-prometheus green; envoy-stats crate (counter/gauge primitives, hierarchical StatsRegistry, Prometheus text-exposition emitter) + envoy-admin crate (HCM-backed admin listener; `/ready` + `/stats` + `/stats/prometheus`; HTTP/1.1 only) + phase-01 admin migration + representative stats wiring (one counter per layer) + envoy-config schema additions (`Admin.access_log_path` parse-and-ignore; `HttpConnectionManagerConfig.stat_prefix` parse-and-consume) + harness `Driver::AdminScrape` + `BodyRule::PrometheusExposition` + first-time population of BEHAVIOR_CONTRACT.md `Stat-name mapping` initial entries |
| 06.2 | `envoy-accesslog` foundation + Envoy default-format emitter + HCM access-log wiring + fixture 0012 | 06.1 | done | — | fixture 0012-access-log-file-sink green; envoy-accesslog crate (AccessLogRecord, FileSink, hand-rolled default-format emitter, hand-rolled ISO-8601 timestamp emitter; `Sink` trait deferred per parent SPEC §3 D8.2 option (c)) + envoy-config schema additions (`HttpConnectionManagerConfig.access_log:` block; file-sink-only validator gate; `ConfigError::UnsupportedAccessLogType`) + HCM on-response-complete wiring (fire-and-forget) + harness `Driver::Http1WithAccessLog` + per-token `AccessLogLineRule` + first-time population of BEHAVIOR_CONTRACT.md `Access log field mapping` (14 default-format token rows) |
| 06.3 | Comprehensive stats wiring + 05.3 REVIEW I1 closure + parent-06 close | 06.2 | done | — | no new fixture; fixture 0011 expectations.yaml extended; comprehensive Envoy stat tree wired at HCM/router/listener/cluster (per-response-class HCM counters; connection-lifetime gauges; upstream-rq counters; access-log line counter; listener accept-failure counter); 05.3 REVIEW I1 (silent H1-listener × H2-cluster misnegotiation) closed at Task 1 via `ConfigError::Http2ClusterFromHttp1Listener` parse-time validator gate; BEHAVIOR_CONTRACT.md `Stat-name mapping` extended; parent phase 06 flips done at this sub-phase's state-6 commit per ROADMAP-schema invariant |
| 07 | Filter chain framework: iteration protocol, per-route config, extension registry | 06 | done | 07.1, 07.2 | framework fixtures green; trivial pluggable filter covers all iteration states |
| 07.1 | `envoy-filter` foundation + HCM filter-chain wiring (H1 5-writer-arm refactor + H2 finalize_h2_stream refactor) + terminal-router validator | 06 | done | — | no new fixture; regression-equivalence proven via all 12 existing fixtures (0001-0012) green simultaneously at Docker-gated CI; framework crate + HCM integration land as the foundation slice |
| 07.2 | `envoy.filters.http.header_mutation` filter + fixture 0013 + parent-07 close-out | 07.1 | done | — | fixture 0013-http-filter-header-mutation green; HeaderMutation bilaterally verified on decode + encode; parent phase 07 flips done |
| 08 | Minimum admin API (config_dump, stats, clusters, listeners, ready, server_info) + graceful drain | 07 | done | 08.1, 08.2 | admin + drain fixtures green |
| 08.1 | Admin endpoint surface (config_dump, server_info, clusters, listeners) + 06.1 carryforward closures (I2/M1/M4) + fixture 0014 | 07 | done | — | fixture 0014-admin-config-dump-server-info green; envoy-admin gains 4 new GET endpoints + AdminEndpoint::dispatch refactor + Bootstrap Serialize derive cascade; harness gains BodyRule::JsonShape + BodyRule::TextLines |
| 08.2 | Endpoint-triggered drain (drain_listeners, healthcheck/fail, healthcheck/ok) + DrainState + listener observation + fixture 0015 + parent-08 close (MVP-trunk close) | 08.1 | done | — | fixture 0015-admin-drain-listeners green; envoy-listener gains DrainState module re-exported from envoy-admin; 3 new POST admin endpoints + listener accept-loop drain observation + 3 new drain-related gauges; parent phase 08 flips done; MVP trunk 00→08 complete |

---

## Feature Families — phases 09 and onward (headings only)

These are seeded as headings only. Each family becomes one or more concrete phase rows when it enters `in-progress`, at which point it is brainstormed and split (§6) as reality demands. Do **not** expand them into per-phase rows prematurely.

### HTTP filters family

Header manipulation, cors, compression, fault, local+global rate limit, jwt_authn, rbac, ext_authz, ext_proc, oauth2, csrf, buffer, lua, wasm, adaptive concurrency, admission control, bandwidth limit.

| id | title | depends-on | status | sub-phases | summary |
|---|---|---|---|---|---|
| 09 | envoy.filters.http.local_ratelimit + fixture 0016 + 07.2 REVIEW M1 close | 07 | done | — | fixture 0016-http-filter-local-rate-limit green; envoy-filter gains LocalRateLimitFilter (hand-rolled token bucket + decode-side StopAndSend with 429 + body `local_rate_limited` per ADR-0033) + HttpFilterInstance::LocalRateLimit variant; envoy-config gains LocalRateLimit + TokenBucket schema + 4 new ConfigError variants; 07.2 REVIEW M1 closed (severed `position` plumbing deleted); ADR-0033 mid-execution corrective sequence per upstream Envoy v1.33 empirical parity |
| 10 | envoy.filters.http.rbac + fixture 0017 + 09 REVIEW M2 + M3 close | 07 | done | — | fixture 0017-http-filter-rbac green; envoy-filter gains RbacFilter (hand-rolled recursive tree-walk evaluator + Allow/Deny actions + Any/Header/AndRules/OrRules/NotRule Permission + Any/Header/AndIds/OrIds/NotId Principal + decode-side StopAndSend with 403 + body `"RBAC: access denied"` (19 bytes per ADR-0034) via ADR-0033 H1 HCM decorate_filter_synth_response helper) + HttpFilterInstance::Rbac variant; envoy-config gains Rbac + Policy + Permission + Principal schema + 6 new ConfigError variants; 09 REVIEW M2 closed (ADR-0033 Consequences amendment per preferred close shape (a)) + M3 closed (Task 7 backstop tokio::process::Command + kill_on_drop discipline) |
| 11 | envoy.filters.http.fault (abort) + fixture 0018 + 09 REVIEW M2 impl close | 07 | done | — | fixture 0018-http-filter-fault green (H2 listener); envoy-filter gains FaultFilter (operator-configured HTTP-status abort + optional header-match gate reusing 04.2 HeaderMatcher + deterministic 0%/100% percentage + decode-side StopAndSend) + HttpFilterInstance::Fault variant; envoy-config gains FaultConfig + FaultAbort + FractionalPercent + DenominatorType schema + ~3 new ConfigError variants (fractional percentage rejected per deterministic-only scope); envoy-http2 gains decorate_filter_synth_response_h2 helper closing the 09 REVIEW M2 H2 HCM decoration gap (implementation arm; first HTTP-filter exercised on H2) |

### Network filters family

Redis, mongo, kafka_broker, thrift, zookeeper [scope TBD], echo, direct_response, sni_cluster, rbac network.

### Load balancing family

least_request, random, ring_hash, maglev, subset LB, locality-weighted LB, priority load balancing, panic thresholds.

### Upstream robustness family

Active health checks HTTP/TCP/gRPC/custom, outlier detection variants, circuit breakers, retries + hedging, per-protocol connection pooling.

| id | title | depends-on | status | sub-phases | summary |
|---|---|---|---|---|---|
| 12 | active HTTP health checking (cluster health_checks) + endpoint ejection + fixture 0019 (engages 06.3 REVIEW I2 synthetic-backend) | 02 04 | done | 12.1, 12.2 | fixture 0019-upstream-active-health-check green; envoy-cluster gains per-endpoint EndpointHealth state machine + pick() unhealthy-exclusion + panic-threshold honoring; new active HTTP health-check task (new envoy-health crate; reuses envoy-http1::Client) probes Cluster.health_checks per interval and drives healthy/unhealthy thresholds; envoy-config gains HealthCheck + HttpHealthCheck + CommonLbConfig + Percent schema + ~4-6 ConfigError variants (TCP/gRPC/custom + multiple health checks rejected per minimum-viable scope); cluster.<name>.health_check.* + membership_healthy stats; first synthetic-backend harness primitive (06.3 REVIEW I2 down-payment; full I2 closure remains with connection pooling); SPLIT into 12.1 + 12.2 at state-2 PLAN-write per ~1800-2200 LoC estimate (ADR-0036) |
| 12.1 | cluster health_checks schema + EndpointHealth state machine + pick() unhealthy-exclusion + panic threshold + health_check stats foundation slice (no new fixture) | 02 04 | done | — | no new fixture; regression-equivalence via the 18 existing fixtures (0001-0018) green simultaneously (machinery inert when health_checks unconfigured — the 05.1/07.1 foundation-slice pattern); envoy-config gains HealthCheck + HttpHealthCheck + CommonLbConfig + Percent schema + ~4-6 ConfigError variants (reuses parse_duration + Int64Range); envoy-cluster gains per-endpoint EndpointHealth (initial state Unhealthy/pending per §6.2 verification) + pick() unhealthy-exclusion + panic threshold (default 50%, strictly-below, Percent {value}); membership_healthy gauge; parse_bootstrap corpus 18→19 |
| 12.2 | active HTTP health-check probe task (new envoy-health crate) + no-healthy-upstream body reconciliation + fixture 0019 + synthetic-backend harness + parent-12 close | 12.1 | done | — | fixture 0019-upstream-active-health-check green; all 19 Docker-gated fixtures green simultaneously; new envoy-health crate (clean DAG over envoy-cluster + envoy-http1 + envoy-config + envoy-stats + tokio; no cycle) owns the first periodic-background probe task (reuses envoy-http1::Client; GET /healthz; Envoy/HC); health_check.{attempt,success,failure} counters; no-healthy-upstream synth-503 body reconciled empty → "no healthy upstream" (19 bytes) per ADR-0037; first synthetic health-aware backend harness primitive + settle-then-probe driver (06.3 REVIEW I2 down-payment, NOT full closure); in-process backstop (both convergence directions); parent phase 12 flips done |
| 13 | per-protocol upstream connection pooling (H1 + H2) + `cluster.<name>.upstream_cx_total` tightening to value-exact + synthetic 5xx backend + fixture 0020 (fully closes 06.3 REVIEW I2) | 04 05 06 12 | done | 13.1, 13.2 | fixture 0020-upstream-connection-pooling-and-per-class-counters green; envoy-http1 + envoy-http2 each gain a per-cluster/per-endpoint connection pool (H1 idle-keep-alive list; H2 multiplexed-streams pool); router-arm dispatch routes through pool acquire/release rather than per-call connect; cluster.<name>.upstream_cx_total disposition tightens to value-exact (fully closes 06.3 REVIEW I2 (b)); per-class downstream_rq_3xx/4xx/5xx + cluster.<name>.upstream_rq_5xx wire bilateral coverage via fixture 0020 (fully closes 06.3 REVIEW I2 (a)); envoy-config gains Cluster.circuit_breakers schema (max_connections; phase-13 supports DEFAULT priority only) + ~3-4 ConfigError variants; new pool stats cluster.<name>.upstream_cx_destroy + upstream_cx_http1_total + upstream_cx_http2_total; configurable-status synthetic-backend harness primitive extends the 12.2 HealthAwareHttp1Backend pattern; SPLIT into 13.1 + 13.2 at state-2 PLAN-write per ~1800-2400 LoC estimate (ADR-0038) |
| 13.1 | H1 connection pool primitive + envoy-config circuit_breakers schema + H1 router-arm pool integration + fixture 0020 + per-class counter wire bilateral coverage (06.3 REVIEW I2 (a) closed) | 04 05 06 12 | done | — | fixture 0020-upstream-connection-pooling-and-per-class-counters green; envoy-http1 gains H1Pool primitive (per-cluster per-endpoint idle keep-alive list; PoolGuard RAII with ConnGaugeGuard reuse; idle sweeper as the second periodic-background primitive); envoy-config gains Cluster.circuit_breakers schema (thresholds[0].{priority?, max_connections?}; DEFAULT priority only; phase-13-deferred fields rejected by deny_unknown_fields) + ~3 ConfigError variants; H1 router-arm dispatch through pool.acquire() rather than per-call Client::connect (upstream_cx_total increment-site migrates from hcm.rs:514 into pool); new pool stats cluster.<name>.upstream_cx_destroy + upstream_cx_http1_total; configurable-status synthetic backend extension (--per-path flag on the 12.2 health-aware-http1-backend); Driver::Http1KeepAlive harness variant (single downstream conn N-request driver — the discriminating-observable shape per §6.2 item-iv); fixture 0020 asserts per-class downstream_rq_{2,3,4,5}xx + cluster.upstream_rq_{2,3,4,5}xx byte-exact bilaterally (FULLY CLOSES 06.3 REVIEW I2 (a)); in-process H1 backstop; parse_bootstrap corpus 20→21; H2 pool + upstream_cx_total contract tightening + parent-13 close defer to 13.2 |
| 13.2 | H2 connection pool + cluster.<name>.upstream_cx_total tightening to value-exact + fixture 0021 + parent-13 close (06.3 REVIEW I2 FULLY CLOSED) | 13.1 | done | — | fixture 0021-upstream-h2-connection-pooling green; all 21 Docker-gated fixtures (0001-0021) green simultaneously; envoy-http2 gains H2Pool primitive (per-cluster per-endpoint connection list with active_streams tracking — peer SETTINGS_MAX_CONCURRENT_STREAMS honored; PoolGuard RAII with ConnGaugeGuard reuse; idle sweeper as the third periodic-background primitive); H2 router-arm dispatch through pool.acquire() (upstream_cx_total increment-site migrates from hcm.rs:280+291 into pool); new pool stat cluster.<name>.upstream_cx_http2_total; BEHAVIOR_CONTRACT cluster.<name>.upstream_cx_total row tightens from name-required,value-may-differ to value-exact for H1+H2 (TCP-proxy carved out — TCP pool defers; FULLY CLOSES 06.3 REVIEW I2 (b)); in-process H2 backstop; parent phase 13 flips done; reuses existing http2-echo-server helper as fixture 0021 backend |
| 14 | outlier detection (consecutive_5xx + consecutive_gateway_failure) + ejection sweeper + fixture 0022 | 04 06 12 13 | planned | — | fixture 0022-upstream-outlier-detection-consecutive-5xx green; envoy-cluster gains per-endpoint EndpointEjection state machine + pick() ejection-exclusion + outlier.rs module + OutlierEjectionSweeper (fourth periodic-background primitive sharing tokio_util::sync::CancellationToken cancellation discipline) + OutlierManager external sibling registry; envoy-http1 + envoy-http2 router-arms gain Cluster::record_response hook at the response-receipt site (synth-status bypass per SPEC §5.7); envoy-config gains OutlierDetection schema + ~4-6 ConfigError variants (success-rate-based + failure-percentage-based + consecutive_local_origin_failure + enforcing_* knobs rejected per minimum-viable scope); cluster.<name>.outlier_detection.{ejections_consecutive_5xx, ejections_consecutive_gateway_failure, ejections_enforced_total, ejections_overflow, ejections_active} stats; reuses 12.x EndpointHealth pick() seam + 12.2 no-healthy-upstream synth-503 path + 13.x configurable-status synthetic backend + 13.1 Driver::Http1KeepAlive; LIKELY SPLIT into 14.1 + 14.2 at state-2 PLAN-write per ~1500-2100 LoC estimate |

### HTTP/3 + QUIC family

quinn transport, downstream H3 listener, upstream H3 cluster, `h3spec` gate.

### gRPC family

gRPC bridge, gRPC-Web, gRPC-JSON transcoding, interop conformance.

### xDS / dynamic config family

ADS, delta xDS, LDS, CDS, RDS, EDS, SDS, RTDS, reconnection, initial-fetch timeout.

### Observability family

gRPC ALS, OTLP access log, OTel/Zipkin/Jaeger/Datadog/XRay tracing, stats sinks, tap filter.

### Runtime + hot restart family

### WASM host family

Own multi-phase sub-project; ABI, engine binding, proxy-wasm conformance.

### Deprecated / edge features

Explicit out-of-scope ADRs unless later re-opened.
