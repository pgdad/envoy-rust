# Fixture 0008-http1-router-upstream

## Property

Plaintext HTTP/1.1 downstream → envoy / envoy-rust → plaintext HTTP/1.1
upstream proxying via the router filter (`route: { cluster: backend }`).
Both proxies dial the same `http1-echo-server` helper (deterministic-echo
body shape per SPEC §3 D3 — alphabetically-sorted lowercase header names
+ verbatim values + verbatim body bytes), and both must produce
byte-identical response bodies and equivalent header sets back to the
harness driver.

The first fixture to exercise per-cluster routing through the HCM router
arm; also the first fixture to exercise the `x-envoy-upstream-service-time`
allow-list row in BEHAVIOR_CONTRACT.md (both proxies emit it on every
router-proxied response with their own measurement of upstream latency).

## Differential surface

The harness's `Driver::Http1 { method: GET, path: "/", host: "envoy-rust.test" }`
opens a plaintext TCP connection to each proxy, writes a single GET, reads
until the headers' CRLF terminator + the declared `Content-Length` body bytes
are consumed, and asserts:

- Response status = 200 (each side independently; envoy ↔ envoy-rust under
  `response_status: exact`).
- Response body byte-exact equal to the deterministic echo shape:

  ```
  method: GET
  path: /
  headers:
    content-length: 0
    host: envoy-rust.test
  body: 
  ```

  (with a trailing space after `body: `; the helper writes `body: ` then
  the body bytes — empty here — then `\n`). Envoy ↔ envoy-rust byte-equal
  under `response_body: byte_exact`.

- Response header set equal modulo BEHAVIOR_CONTRACT.md allow-list (`server`
  and `date` allowed to differ in value; `x-envoy-upstream-service-time`
  added by 04.3 — allowed to differ in value because the integer ms
  measurement is non-deterministic across hosts).

## Header-injection divergence — `request_headers_to_remove` on Envoy side only

Envoy v1.33 by default injects request headers on the upstream-bound side:
`x-forwarded-for`, `x-forwarded-proto`, `x-request-id`,
`x-envoy-expected-rq-timeout-ms`, `x-envoy-internal`, `x-envoy-external-address`.
envoy-rust does not inject any of these in 04.3 (per parent SPEC §4 non-goal —
deferred to a follow-on; see SPEC §4 line 570). Because the helper's body
echoes received request headers verbatim, those Envoy-injected headers would
land in the body and break envoy ↔ envoy-rust byte-equivalence.

`envoy.yaml` strips them via `route_config.request_headers_to_remove` and
disables UUID generation via `generate_request_id: false` at the HCM level.
`envoy-rust.yaml` carries neither field — envoy-rust's `HttpConnectionManagerConfig`
parser uses `#[serde(deny_unknown_fields)]` and would reject either name. The
field-set divergence between the two YAMLs is intentional and mirrors fixture
0005's `admin:` / bind-address divergence (SPEC §3 D4 line 433).

## ADRs referenced

- ADR-0015 — cross-container `host.docker.internal` + `host-gateway`
  (envoy-side backend host resolution; the helper runs on the host, the
  upstream Envoy reaches it via `host.docker.internal`, envoy-rust via
  `127.0.0.1`).
- ADR-0020 — split phase 04 into 04.1 + 04.2 + 04.3.

(No ADR-0021 dependency: this fixture's route uses `prefix: "/"` only —
the matcher fan-out (header / path / regex / range) is exercised by
04.2's amendment to fixture 0007.)

## Out of scope (deferred)

Per parent SPEC §4 + 04.3 SPEC §4:

- Upstream TLS on top of HTTP/1.1 router (the HCM-with-upstream-TLS
  combination) — deferred to a follow-on (see 04.3 SPEC §4).
- Request-body forwarding (POST with body) — 04.x is GET-only on the
  router-proxy path.
- HCM `server_name` config field overriding `Server:` — phase 05+.
- `x-envoy-original-path` / request-id / forwarded-* injection by
  envoy-rust — deferred follow-on (04.3 SPEC §4 line 570).
- Cluster `host_rewrite` / route `auto_host_rewrite` — phase 05+.
