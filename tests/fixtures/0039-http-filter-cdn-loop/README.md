# Fixture 0039: HTTP filter — CDN-Loop (RFC 8586)

Phase-31 differential acceptance fixture for `envoy.filters.http.cdn_loop`.
Both upstream Envoy (v1.33.0) and envoy-rust must produce the deterministic
status sequence `[200, 502, 200, 400, 200]` for 5 sequential HTTP/1.1 requests
(`Host: cdn.test`) against a single `route: { cluster: backend }` route
(proxying to the `http1-echo-server` helper), given an HCM filter chain of
`[envoy.filters.http.cdn_loop, envoy.filters.http.router]` with a
`CdnLoopConfig { cdn_id: "mycdn.example", max_allowed_occurrences: 0 }`.

This is the phase's **STRONG cross-proxy byte-exact differential** — the
headline correctness proof for the CDN-Loop append byte-shape and the two
local-reply bodies, validated against live Envoy.

## Filter chain

```
http_filters:
  - envoy.filters.http.cdn_loop   (cdn_id "mycdn.example", max_allowed_occurrences 0)
  - envoy.filters.http.router     (terminus)
```

## RFC 8586 loop-detection behaviour (ADR-0077 §6.2-LOCKED)

The cdn_loop decode side coalesces all `CDN-Loop` request-header values, parses
them under the RFC 7230 list/token/parameter grammar, then:

- **malformed** → `400`, byte-exact body `Invalid CDN-Loop header in request.`
  (35 bytes, no newline).
- **count(cdn_id) > max_allowed_occurrences** → `502`, byte-exact body
  `The server has detected a loop between CDNs.` (44 bytes, no newline).
- **otherwise** → append this proxy's `cdn_id` **comma-only** to the RAW
  coalesced bytes (empty list entries preserved) and forward ONE coalesced
  `CDN-Loop` header, then `200` from the upstream echo.

## Probe burst

| # | probe              | CDN-Loop request value | status | forwarded `cdn-loop` (echo) / body                        |
|---|--------------------|------------------------|--------|-----------------------------------------------------------|
| 1 | no-header          | (none)                 | 200    | `mycdn.example` (bare append)                              |
| 2 | self-loop          | `mycdn.example`        | 502    | body `The server has detected a loop between CDNs.`       |
| 3 | foreign-append     | `othercdn.example`     | 200    | `othercdn.example,mycdn.example` (comma-only)             |
| 4 | malformed          | `"abc`                 | 400    | body `Invalid CDN-Loop header in request.`                |
| 5 | trailing-comma     | `othercdn.example,`    | 200    | `othercdn.example,,mycdn.example` (empty entry preserved) |

P4's value is the literal unterminated quoted-string `"abc` — a quoted-string
`cdn-id` is not a bare RFC 7230 token, and the quote never closes, so the value
is malformed. The driver writes `extra_headers` verbatim onto the HTTP/1.1 wire
(`cdn-loop: "abc`), so the literal quote reaches the filter unmodified.

## Assertion strategy

5 sequential `Http1Probe` entries (`Driver::Http1ProbeList`) with per-probe
`extra_headers` carrying the `CDN-Loop` request value. Each probe asserts:

- `expected_status` (exact, per-probe): the `[200, 502, 200, 400, 200]` sequence.
- `expected_headers: set_equal_modulo_allow_list` — cross-proxy header-set
  equality modulo the `server` + `date` + `x-envoy-upstream-service-time`
  allow-list rows (BEHAVIOR_CONTRACT.md).
- Probes 2 (502) and 4 (400) additionally assert `expected_body: { kind:
  byte_exact, body: ... }` — the local-reply body is a pure function of the
  filter (not the request), so it is byte-identical on both proxies.
- Probes 1, 3, 5 (the append probes) have no per-probe body assertion (body =
  echo-server response, value known only at runtime). Top-level `equivalence:
  { response_body: { kind: byte_exact } }` confirms both proxies forwarded the
  IDENTICAL upstream request — i.e. the appended `CDN-Loop` byte-shape matches
  cross-proxy. This is the STRONG byte-exact append proof.

## Echo-body observation of the forwarded CDN-Loop header (0013 mechanism)

The append probes forward the request to the `http1-echo-server`, which reflects
received request headers into the response body as alphabetically-**sorted**,
**lowercase** `  name: value` lines. The forwarded (mutated) `CDN-Loop` header
therefore surfaces as a `  cdn-loop: <value>` line, **independent of the wire
casing** either proxy emits for the header name. So the cross-proxy byte-exact
echo body validates the appended **value** byte-shape (comma-only join; empty
entries preserved) without coupling to the header-name casing decision.

The upstream Envoy config strips its auto-injected request headers
(`x-forwarded-*`, `x-request-id`, `x-envoy-*`) via `request_headers_to_remove`
and disables UUID generation via `generate_request_id: false`; envoy-rust injects
none of those. Both proxies therefore forward an identical upstream request →
identical echo body → `equivalence: response_body: byte_exact` holds.

## connection header on the reject probes

Every differential probe driver sends `Connection: close`. The shared
`decorate_filter_synth_response` keys the reply's `connection` header off the
per-request close flag (NOT the status), so BOTH the 502 and 400 reject probes
carry `connection: close` on BOTH proxies under the close-driver. `connection`
is NOT on the header allow-list → it is value-compared and resolves clean both
sides — exactly as the 0032 csrf-403 reject already does. It is NOT suppressed.

## Per-side YAML asymmetry

`envoy.yaml` (upstream) carries an `admin` block (`port_value: 0`;
kernel-ephemeral), bind `0.0.0.0:{{PORT}}` (Docker container public bind),
`generate_request_id: false`, `request_headers_to_remove` at the `route_config`
level, and cluster `dns_lookup_family: V4_ONLY`. `envoy-rust.yaml` carries the
narrow shape: no `admin` block, bind `127.0.0.1:{{PORT}}`, no
`generate_request_id`, no `request_headers_to_remove`, no `dns_lookup_family`.
The harness substitutes `{{BACKEND_HOST}}` per-side (`host.docker.internal`
upstream, `127.0.0.1` subject) and `{{PORT}}` / `{{HTTP1_BACKEND_PORT}}`.

## ADRs referenced

- ADR-0076 — phase-31 SPEC (`envoy.filters.http.cdn_loop`).
- ADR-0077 — phase-31 PLAN-write; §6.2-LOCKED (502/400 bodies, comma-only
  append, empty-entry preservation, case-sensitive cdn-id matching).
