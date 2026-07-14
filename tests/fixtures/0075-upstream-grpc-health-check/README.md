# Fixture 0075 — `upstream-grpc-health-check`

**Phase:** 69 (ADR-0138 / ADR-0139).
**Differential surface:** post-convergence active **gRPC**-HC steady state on an H2 listener.

## Topology

- Downstream: an HTTP/2 (h2c prior-knowledge) listener on `{{PORT}}` → HCM →
  router → cluster `grpc_hc_backend`.
- Cluster: an H2-upstream cluster (`typed_extension_protocol_options` →
  `envoy.extensions.upstreams.http.v3.HttpProtocolOptions` →
  `explicit_http_config.http2_protocol_options: {}`) carrying an active
  `grpc_health_check: {}` (interval `1s`, timeout `1s`, `unhealthy_threshold:
  2`, `healthy_threshold: 1`), with `healthy_panic_threshold: { value: 0 }`
  disabling panic routing.
- Upstream: **no backend process is spawned.** `{{DEAD_BACKEND_PORT}}`
  (harness marker, `tests/differential/src/lib.rs`) reserves an ephemeral
  port via `reserve_port()` and binds no listener, so every gRPC HC probe
  (a unary `grpc.health.v1.Health/Check` call framed per ADR-0138) gets
  ECONNREFUSED for the fixture's duration (mirrors fixture 0074's
  `{{DEAD_BACKEND_PORT}}` ADR-0137 PV-2 shape).

## Observable

After ~3.5s settle, both proxies have attempted the gRPC HC probe against
`{{DEAD_BACKEND_PORT}}` ≥2 times, observed the connection refusal, transitioned
the sole endpoint to Unhealthy after `unhealthy_threshold: 2` consecutive
failures, and (with panic disabled) make `pick()` return `None`. The H2 HCM
no-healthy-upstream path fires synth-503 with body `no healthy upstream`
(19 bytes per ADR-0037 / `synth_no_healthy_upstream`) — the same fixed body
as the H1 sibling (fixture 0074) and fixture 0019.

Driver: `Driver::Http2AfterSettle` (the H2 sibling of `Driver::Http1AfterSettle`
landed at phase 12.2), `settle_ms: 3500`, then a single GET `/` driven over
h2c prior-knowledge via `drive_http2`.

## Discriminating observable / equivalence axes

- `response_status: exact` + `expected_status: 503` on both proxies.
- `response_body: byte_exact` + `expected_body: "no healthy upstream"`
  (19 bytes) on both proxies.

`expected_headers` is **deliberately omitted** — unlike fixture 0074 (which
asserts `set_equal_modulo_allow_list` on the H1 synth-503 response headers),
this fixture does NOT assert the header axis. envoy-rust's H2 no-healthy
synth-503 path currently emits a narrower header set than upstream Envoy
(`server` + `content-type` only, vs. Envoy's fuller H2 status-response header
set) — a pre-existing H2-503 gap tracked as **CF-69-1**, not a regression
introduced by this fixture. `Driver::Http2AfterSettle::expected_headers` is
`#[serde(default)]`, so omitting the field deserializes to `None` and the
harness's header-diff step is skipped entirely (see
`run_http2_after_settle_arm` in `tests/differential/src/lib.rs`).

## Why an H2 listener (ADR-0028 is NOT lifted)

`grpc_health_check` is fatal-at-load (`ConfigError::GrpcHealthCheckRequiresHttp2`,
ADR-0139) unless the cluster carries `typed_extension_protocol_options` with
`explicit_http_config.http2_protocol_options: {}`. The 06.3 D14.3 parse-time
gate (ADR-0028) rejects an H1 LISTENER paired with an H2 UPSTREAM cluster, so
this fixture's listener must also be H2 — the same "H2 listener because the
cluster is H2" shape as fixtures 0010 / 0021 (`upstream-h2-connection-pooling`).
ADR-0028's deferred H1-listener-over-H2-cluster path remains deferred; this
fixture does not attempt it.

## Relation to fixture 0074

Same downstream steady-state observable (503 + byte-exact 19-byte body) as
fixture 0074 (the TCP-HC ejection). Only the *checker type* and *codec*
differ: fixture 0074 drives a connection-only `tcp_health_check` over an H1
listener/cluster; this fixture drives an active `grpc_health_check` (a
trailers-aware unary gRPC `Health/Check` call, ADR-0138) over an H2
listener/cluster. Both use `{{DEAD_BACKEND_PORT}}` refusal as the failure
trigger and `healthy_panic_threshold: { value: 0 }` to force `pick() → None`
rather than panic-mode routing.

Integer-second durations (`1s`/`1s`) per §6.2 item-6 — the only duration form
both proxy parsers accept.
