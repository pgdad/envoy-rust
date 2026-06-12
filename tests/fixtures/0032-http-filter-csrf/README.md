# Fixture 0032: HTTP filter — CSRF

Phase-24 differential acceptance fixture for `envoy.filters.http.csrf`.
Both upstream Envoy (v1.33.0) and envoy-rust must produce the deterministic
status sequence `[200, 403, 200, 200, 403]` for 5 sequential HTTP/1.1 requests
against a single `route: { cluster: backend }` route (proxying to the
`http1-echo-server` helper), given an HCM filter chain of
`[envoy.filters.http.csrf, envoy.filters.http.router]` with a `CsrfPolicy`
attached to the route via `typed_per_filter_config`.

## L8 lock-in — real upstream cluster required (ADR-0061)

A valid CSRF modify-method request (POST with an allowed Origin) must reach a
real upstream to yield a `200`. A `direct_response` route has no `RouteEntry`,
so the per-route `CsrfPolicy` in `typed_per_filter_config` is not engaged the
same way — the differential would be a false green.

Fixture 0032 therefore proxies to a real `http1-echo-server` cluster
(the `0030-http-filter-jwt-authn` / `0031-http-filter-cors` pattern).
**DO NOT** convert this fixture to `direct_response`. This constraint is
recorded in ADR-0061 §L8.

## Shared-message lock-in (ADR-0061 L1)

The filter-chain entry and the per-route override use the SAME proto message:
`envoy.extensions.filters.http.csrf.v3.CsrfPolicy`. `filter_enabled` is REQUIRED
(no default). envoy-rust honors only the deterministic 0%/100% `default_value`;
a present `runtime_key` is rejected (no RTDS runtime layer — ADR-0061 L6).

```
http_filters:
  - envoy.filters.http.csrf   (typed_config "@type": …csrf.v3.CsrfPolicy,
                               filter_enabled 100%)
  - envoy.filters.http.router (terminus)
```

CsrfPolicy on the route (`typed_per_filter_config`):

```yaml
envoy.filters.http.csrf:
  "@type": type.googleapis.com/envoy.extensions.filters.http.csrf.v3.CsrfPolicy
  filter_enabled:
    default_value: { numerator: 100, denominator: HUNDRED }
  additional_origins:
    - exact: "additional.csrf.test"
```

## Scheme-stripped origin matching (ADR-0061 L3)

The CSRF guard reduces the source `Origin` (or `Referer`) to its
scheme-stripped `host[:port]` via `host_and_port()` and compares it to:

1. the **target authority** (the listener `Host` value — same-origin), and
2. each `additional_origins` `StringMatcher` (also scheme-stripped).

The `additional_origins` matcher value (`additional.csrf.test`) and the
`post-additional` probe's `Origin` (`http://additional.csrf.test`) are chosen
so `host_and_port("http://additional.csrf.test")` = `additional.csrf.test`
exactly equals the `exact:` matcher. Likewise the `post-same-origin` probe's
`Origin: http://csrf.test` reduces to `csrf.test`, equal to the target
`Host: csrf.test` → same-origin 200.

## Probe burst

| # | probe            | method | Origin                          | status | rationale                            |
|---|------------------|--------|---------------------------------|--------|--------------------------------------|
| 1 | post-same-origin | POST   | http://csrf.test                | 200    | host == target authority             |
| 2 | post-evil-origin | POST   | http://evil.example.com         | 403    | matches neither → "Invalid origin"   |
| 3 | post-additional  | POST   | http://additional.csrf.test     | 200    | matches additional_origins exact     |
| 4 | get-evil-safe    | GET    | http://evil.example.com         | 200    | safe method bypasses the guard       |
| 5 | post-no-source   | POST   | (none)                          | 403    | missing source origin → "Invalid origin" |

## Assertion strategy

5 sequential `Http1Probe` entries (`Driver::Http1ProbeList`) with per-probe
`extra_headers` carrying the `Origin` value. Each probe asserts:

- `expected_status` (exact, per-probe): the `[200, 403, 200, 200, 403]` sequence.
- `expected_headers: set_equal_modulo_allow_list` — cross-proxy header-set
  equality modulo the `server` + `date` + `x-envoy-upstream-service-time`
  allow-list rows (BEHAVIOR_CONTRACT.md).
- Probes 2 and 5 additionally assert `expected_body: { kind: byte_exact, body:
  "Invalid origin" }` — the 403 body is 14 bytes, no newline (ADR-0061 L4), with
  `content-type: text/plain` auto-added by the H1 filter-synth helper.
- Probes 1, 3, 4 have no per-probe body assertion (body = echo-server response;
  value known only at runtime). Top-level `equivalence: { response_body: { kind:
  byte_exact } }` confirms both proxies forwarded the same upstream request.

## Byte-exact 403 body rationale (ADR-0061 L4)

The 403 failure body is the literal `Invalid origin` (no trailing newline). It
is a pure function of the filter (not the request), so it is byte-identical on
both proxies. Asserting it byte-exact (rather than allow-listing it) is the §6.2
L4 lock: the rejection body bytes are asserted, not assumed.

## Echo-body equivalence

Probes 1, 3, 4 forward the request to the `http1-echo-server`. The upstream
Envoy config strips the auto-injected request headers (`x-forwarded-for`,
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

- ADR-0060 — phase-24 SPEC (`envoy.filters.http.csrf` + per-route
  `typed_per_filter_config` consumer).
- ADR-0061 — phase-24 PLAN-write; §L1 (shared message), §L3 (scheme-stripped
  origin matching), §L4 (byte-exact 403 body), §L6 (no runtime layer),
  §L8 lock-in (real upstream cluster required).
