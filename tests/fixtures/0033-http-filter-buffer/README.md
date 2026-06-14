# Fixture 0033: HTTP filter — Buffer

Phase-25.2 differential acceptance fixture for `envoy.filters.http.buffer`.
Both upstream Envoy (v1.33.0) and envoy-rust must produce the deterministic status
sequence `[200, 413, 200, 413, 200]` for 5 sequential HTTP/1.1 requests against
routes proxying to the `http1-echo-server` helper, given an HCM filter chain of
`[envoy.filters.http.buffer, envoy.filters.http.router]` with a chain-level
`Buffer { max_request_bytes: 10 }` and per-route `BufferPerRoute` overrides
attached via `typed_per_filter_config` (disable on `/disabled`, lowered limit 4
on `/small`).

This is the first fixture to exercise the new `Http1Probe.body` request-body
field (added in Task 5): the burst sends real request bodies so the chain-level
limit and the per-route overrides are actually engaged.

## Real upstream cluster required (ADR-0063 finding 8)

A within-limit request must reach a real upstream to yield a body-echoing
`200`. A `direct_response` route engages neither the per-route filter config nor
body forwarding — the differential would be a false green. Fixture 0033
therefore proxies to a real `http1-echo-server` cluster (the
`0030-http-filter-jwt-authn` / `0031-http-filter-cors` / `0032-http-filter-csrf`
pattern). **DO NOT** convert this fixture to `direct_response`.

## Filter chain + per-route override

The filter-chain entry uses `envoy.extensions.filters.http.buffer.v3.Buffer`
(`max_request_bytes: 10`); the per-route overrides use
`envoy.extensions.filters.http.buffer.v3.BufferPerRoute`, whose oneof is
`{ disabled, buffer }`:

- `/disabled` → `BufferPerRoute { disabled: true }` (filter bypassed).
- `/small`    → `BufferPerRoute { buffer: { max_request_bytes: 4 } }` (lowered
  limit).
- `/` (catch-all) → no per-route config; the chain limit 10 applies.

First-match route order matters: `/disabled` and `/small` precede the `/`
catch-all.

```
http_filters:
  - envoy.filters.http.buffer  (typed_config "@type": …buffer.v3.Buffer,
                                max_request_bytes 10)
  - envoy.filters.http.router  (terminus)
```

## Over-limit rejection — strict `>` (ADR-0063)

The buffer filter rejects when `body.len() > effective_max` (strict `>`): a body
exactly equal to the limit passes. Over-limit replies `413` with the literal
body `Payload Too Large` (17 bytes, NO trailing newline; `content-type:
text/plain` auto-added by the H1 filter-synth helper). There are NO buffer
stats (ADR-0063).

## Probe burst

| # | probe                  | method | path       | body                | status | rationale                              |
|---|------------------------|--------|------------|---------------------|--------|----------------------------------------|
| 1 | post-within-limit      | POST   | /          | hello (5B)          | 200    | 5 ≤ chain limit 10 → forwarded, echoed |
| 2 | post-over-limit        | POST   | /          | hello world!! (13B) | 413    | 13 > 10 → "Payload Too Large"          |
| 3 | post-route-disabled    | POST   | /disabled  | hello world!! (13B) | 200    | route disables the filter → echoed     |
| 4 | post-route-lowered     | POST   | /small     | hello (5B)          | 413    | 5 > route's lowered limit 4            |
| 5 | get-no-body            | GET    | /          | (none)              | 200    | no body → passthrough echo             |

## Assertion strategy

5 sequential `Http1Probe` entries (`Driver::Http1ProbeList`) with per-probe
`body` carrying the request body. Each probe asserts:

- `expected_status` (exact, per-probe): the `[200, 413, 200, 413, 200]` sequence.
- `expected_headers: set_equal_modulo_allow_list` — cross-proxy header-set
  equality modulo the `server` + `date` + `x-envoy-upstream-service-time`
  allow-list rows (BEHAVIOR_CONTRACT.md).
- Probes 2 and 4 additionally assert `expected_body: { kind: byte_exact, body:
  "Payload Too Large" }` — the 413 body is 17 bytes, no newline, with
  `content-type: text/plain` auto-added by the H1 filter-synth helper.
- Probes 1, 3, 5 have no per-probe body assertion (body = echo-server response;
  value known only at runtime). Top-level `equivalence: { response_body: { kind:
  byte_exact } }` confirms both proxies forwarded the same upstream request.

## Byte-exact 413 body rationale (ADR-0063)

The 413 over-limit body is the literal `Payload Too Large` (17 bytes, no
trailing newline). It is a pure function of the filter (not the request), so it
is byte-identical on both proxies. Asserting it byte-exact (rather than
allow-listing it) is the §6.2 lock: the rejection body bytes are asserted, not
assumed.

## Echo-body equivalence

Probes 1, 3, 5 forward the request to the `http1-echo-server`. The upstream
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

- ADR-0062 — phase-25 SPEC scoping (`envoy.filters.http.buffer` + per-route
  `typed_per_filter_config` consumer).
- ADR-0063 — phase-25.2 §6.2 PLAN-write wire contract: 413 `Payload Too Large`
  body (17 bytes, no newline), strict `>` over-limit reject, NO buffer stats,
  and the `Buffer` + `BufferPerRoute` (`{ disabled, buffer }` oneof) wire shapes.
