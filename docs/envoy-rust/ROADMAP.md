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
| 06 | Access log (file sink, Envoy default format) + stats + Prometheus admin endpoint | 05 | planned | — | access log + Prometheus fixtures green |
| 07 | Filter chain framework: iteration protocol, per-route config, extension registry | 06 | planned | — | framework fixtures green; trivial pluggable filter covers all iteration states |
| 08 | Minimum admin API (config_dump, stats, clusters, listeners, ready, server_info) + graceful drain | 07 | planned | — | admin + drain fixtures green |

---

## Feature Families — phases 09 and onward (headings only)

These are seeded as headings only. Each family becomes one or more concrete phase rows when it enters `in-progress`, at which point it is brainstormed and split (§6) as reality demands. Do **not** expand them into per-phase rows prematurely.

### HTTP filters family

Header manipulation, cors, compression, fault, local+global rate limit, jwt_authn, rbac, ext_authz, ext_proc, oauth2, csrf, buffer, lua, wasm, adaptive concurrency, admission control, bandwidth limit.

### Network filters family

Redis, mongo, kafka_broker, thrift, zookeeper [scope TBD], echo, direct_response, sni_cluster, rbac network.

### Load balancing family

least_request, random, ring_hash, maglev, subset LB, locality-weighted LB, priority load balancing, panic thresholds.

### Upstream robustness family

Active health checks HTTP/TCP/gRPC/custom, outlier detection variants, circuit breakers, retries + hedging, per-protocol connection pooling.

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
