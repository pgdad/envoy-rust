# Fixture 0007-http1-direct-response

This fixture drives a `GET /healthz` request through an HTTP/1.1 listener
configured with `envoy.filters.network.http_connection_manager`. The HCM
walks an inline `route_config` (single virtual_host with `domains: ["*"]`,
single route with `match: { prefix: "/" }`), and dispatches the matched
route's `direct_response` action: status `200`, body `inline_string: "ok\n"`.
No upstream cluster is touched.

The harness's `Driver::Http1 { method: GET, path: "/healthz", host:
"envoy-rust.test" }` opens a plaintext TCP connection to each proxy, writes
the request, reads until both the headers' CRLF terminator is seen AND
`Content-Length: 3` body bytes are consumed, asserts:

- Response status = 200 (Row 1, exact).
- Response body = `"ok\n"` (Row 2, byte_exact).
- Response header set is equal modulo the BEHAVIOR_CONTRACT.md allow-list
  (Row 3, set_equal_modulo_allow_list — `server` + `date` allowed to differ
  in value; all other headers — `content-length`, `content-type`, `connection`
  — value-exact).

Both proxies emit 5 response headers (`server`, `date`, `content-length`,
`content-type`, `connection`) per their respective HCM defaults. envoy-rust
emits `server: envoy-rust`; Envoy emits `server: envoy`. Both stamp `date:`
with the wall-clock at response-write time; the IMF-fixdate strings differ
slightly. `content-length` matches deterministically (`3`); `content-type`
matches (`text/plain`); `connection` matches (`keep-alive` — request did
not opt into `Connection: close`).

What is *out* of this fixture (each pinned to a later sub-phase or phase):

- HTTP route header matchers — sub-phase 04.2 (will amend this fixture's
  envoy.yaml + envoy-rust.yaml to add a second route with a `headers:`
  matcher demonstrating production matcher use).
- Upstream HTTP/1.1 origination — sub-phase 04.3 (fixture 0008 is the
  first fixture to proxy through to an upstream `http1-echo-server`).
- HTTP/2 / HTTP/3 — phases 05 and the QUIC family.
- HTTP filter chain (`Vec<Box<dyn HttpFilter>>` iteration protocol) —
  phase 07.
- Access logs, stats, Prometheus admin endpoint — phase 06.
- Multi-VH SNI matching with TLS — phase 05+ (HCM-with-TLS not exercised
  in 04.x; the listener filter chain has no `transport_socket`).
- HCM `server_name` config field (overrides the `server:` response header
  literal) — phase 05+; until then the BEHAVIOR_CONTRACT.md allow-list
  permits `server` to differ.

## 04.2 amendment — header-matcher route

Phase 04.2 added a second route at the head of `routes:` (so first-match-wins
reaches it before the catch-all): `match: { prefix: "/api/", headers: [{ name:
"x-foo", exact_match: "bar" }] }` returning `direct_response: { status: 418,
body: { inline_string: "teapot\n" } }`. The original `prefix: "/"` catch-all
stays second; both proxies must select the same route on each probe — the new
differential property 04.2 exercises.

The fixture now drives two probes via the harness's `Driver::Http1ProbeList`:

- `default-route` — `GET /healthz Host: envoy-rust.test` (no `X-Foo`); falls
  through to the catch-all 200 OK.
- `matcher-route` — `GET /api/widgets Host: envoy-rust.test X-Foo: bar`; hits
  the matcher route 418 teapot.

Each probe applies the same 5-axis equivalence cascade as the 04.1 single-probe
shape (status exact, body byte_exact, headers set_equal_modulo_allow_list).

The matcher route demonstrates production matcher use across all 7 of Envoy's
`HeaderMatcher` modes (which all 7 modes land in 04.2 — `exact_match`,
`prefix_match`, `suffix_match`, `safe_regex_match`, `range_match`,
`present_match`, `string_match`); this fixture exercises only `exact_match` for
maximum minimum-viable coverage. Per-mode runtime behavior is exercised by
the matcher-runtime unit tests in `crates/envoy-config/src/matcher.rs::tests`
(28 tests covering all 7 modes + invert_match XOR + StringMatcher.ignore_case).

ADR references: ADR-0011 (response-header equivalence deferral closes here
via the BEHAVIOR_CONTRACT.md `Header allow-list` table populated at this
phase), ADR-0014 (`typed_config` deserialization), ADR-0020 (split phase 04
into 04.1 + 04.2 + 04.3), ADR-0021 (`regex` permitted as a foundation for
header / route matching at config-load time).
