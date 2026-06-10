# Fixture 0031: HTTP filter — CORS

Phase-23 differential acceptance fixture for `envoy.filters.http.cors`.
Both upstream Envoy (v1.33.0) and envoy-rust must produce the deterministic
status sequence `[200, 200, 200, 200]` for 4 sequential HTTP/1.1 requests
against a single `route: { cluster: backend }` route (proxying to the
`http1-echo-server` helper), given an HCM filter chain of
`[envoy.filters.http.cors, envoy.filters.http.router]` with a `CorsPolicy`
attached to the route via `typed_per_filter_config`.

## L6 lock-in — real upstream cluster required (ADR-0058)

Envoy's CORS filter does **NOT** engage on a `direct_response` route: a
`direct_response` route has no `RouteEntry`; the per-route `CorsPolicy` in
`typed_per_filter_config` is silently ignored (origin_valid stays 0, no
CORS headers are emitted, the preflight is NOT short-circuited).

Fixture 0031 therefore proxies to a real `http1-echo-server` cluster
(the `0008-http1-router-upstream` / `0030-http-filter-jwt-authn` pattern).
**DO NOT** convert this fixture to `direct_response` — the differential
result would be a false green (neither proxy engages CORS, so both agree
trivially). This constraint is recorded in ADR-0058 §L6.

## Filter chain

```
http_filters:
  - envoy.filters.http.cors  (typed_config "@type": …cors.v3.Cors)
  - envoy.filters.http.router (terminus)
```

CorsPolicy on the route (`typed_per_filter_config`):

```yaml
envoy.filters.http.cors:
  "@type": type.googleapis.com/envoy.extensions.filters.http.cors.v3.CorsPolicy
  allow_origin_string_match:
    - exact: "http://allowed.example.com"
  allow_methods: "GET, POST, OPTIONS"
  allow_headers: "x-custom-header, content-type"
  max_age: "3600"
```

## Probe burst

| # | probe          | Extra headers                                               | status | CORS headers present?                   |
|---|----------------|-------------------------------------------------------------|--------|-----------------------------------------|
| 1 | preflight      | Origin: http://allowed.example.com, ACRM: GET              | 200    | access-control-allow-{origin,methods,headers} + max-age |
| 2 | allowed GET    | Origin: http://allowed.example.com                         | 200    | access-control-allow-origin (echoed)    |
| 3 | evil origin    | Origin: http://evil.example.com                            | 200    | none                                    |
| 4 | no origin      | (none)                                                      | 200    | none                                    |

## Assertion strategy

4 sequential `Http1Probe` entries (`Driver::Http1ProbeList`) with per-probe
`extra_headers` carrying the `Origin` and `Access-Control-Request-Method`
values. Each probe asserts:

- `expected_status: 200` (exact, per-probe).
- `expected_headers: set_equal_modulo_allow_list` — cross-proxy header-set
  equality modulo the `server` + `date` + `x-envoy-upstream-service-time`
  allow-list rows (BEHAVIOR_CONTRACT.md). The `access-control-*` headers are
  NOT on the allow-list — they are compared value-exact cross-proxy (they are
  a pure function of the CorsPolicy + the request Origin, so identical on both
  proxies when configured with the same policy).
- Probe 1 additionally asserts `expected_body: { kind: byte_exact, body: "" }` —
  the preflight short-circuit returns an empty body (the `build_preflight_response`
  helper sets `body: Bytes::new()`; `decorate_filter_synth_response` stamps
  `content-length: 0`).
- Probes 2–4 have no per-probe body assertion (body = echo-server response;
  value known only at runtime). Top-level `equivalence: { response_body: { kind:
  byte_exact } }` confirms both proxies forwarded the same upstream request.

## Value-exact `access-control-*` header disposition

The six `access-control-*` response headers (`allow-origin`, `allow-methods`,
`allow-headers`, `max-age`, `allow-credentials`, `expose-headers`) are a pure
function of the CorsPolicy + the request Origin. Because both proxies receive
the SAME Origin header and are configured with the SAME CorsPolicy, the values
are **byte-exact cross-proxy** — they are deliberately NOT on the harness
allow-list (BEHAVIOR_CONTRACT.md §Response-headers). The `set_equal_modulo_allow_list`
mechanism confirms:

- Presence: if one proxy emits `access-control-allow-origin` and the other does
  not, the name-set check fails immediately.
- Value-exact: any value mismatch on a non-allow-listed header is reported.

This is the §6.2 L3 lock (ADR-0058): the CORS header bytes are asserted, not
assumed.

## Echo-body equivalence

Probes 2–4 forward the request to the `http1-echo-server`. The upstream Envoy
config strips the auto-injected request headers (`x-forwarded-for`,
`x-forwarded-proto`, `x-request-id`, `x-envoy-expected-rq-timeout-ms`,
`x-envoy-internal`, `x-envoy-external-address`) via `request_headers_to_remove`
and disables UUID generation via `generate_request_id: false`. The envoy-rust
config omits those fields (envoy-rust does not inject those headers).
Both proxies therefore forward an identical upstream request → identical echo
body → `equivalence: response_body: byte_exact` holds.

## Per-side YAML asymmetry

`envoy.yaml` (upstream) carries an `admin` block (`port_value: 0`;
kernel-ephemeral), bind `0.0.0.0:{{PORT}}` (Docker container public bind),
`generate_request_id: false`, and `request_headers_to_remove` at the
`route_config` level. `envoy-rust.yaml` carries the narrow shape: no `admin`
block, bind `127.0.0.1:{{PORT}}`, no `generate_request_id` field, no
`request_headers_to_remove` field. The cluster shape differs on
`dns_lookup_family: V4_ONLY` (envoy.yaml) — envoy-rust's cluster config does
not model this field; it resolves A records by default.

## ADRs referenced

- ADR-0057 — phase-23 SPEC (`envoy.filters.http.cors` + per-route
  `typed_per_filter_config` infrastructure).
- ADR-0058 — phase-23 PLAN-write; §L6 lock-in (real upstream cluster required).
