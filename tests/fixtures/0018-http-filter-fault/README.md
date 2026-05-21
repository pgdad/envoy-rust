# Fixture 0018: HTTP filter — Fault (abort) on an HTTP/2 listener

Phase 11 differential acceptance fixture for `envoy.filters.http.fault`
(abort path) on an **HTTP/2** listener. Both upstream Envoy (v1.33.0) and
envoy-rust must produce the deterministic status sequence
`[503, 200, 503, 200]` for 4 sequential `GET /` requests against a single
direct_response route, given an HCM filter chain of
`[envoy.filters.http.fault, envoy.filters.http.router]`. Probes alternate the
`x-fault: abort` header to exercise both the abort path (503) and the
pass-through path (200).

## Listener

`codec_type: HTTP2` (mirrors fixture 0009's H2 HCM shape). This is the
**first HTTP-filter-family fixture on an H2 listener**. The harness drives it
with `Driver::Http2ProbeList`, which runs each probe over H2 cleartext
(prior-knowledge) via `drive_http2`.

## Filter chain

```
http_filters:
  - envoy.filters.http.fault (abort: 503 @ 100%, gated on x-fault: abort)
  - envoy.filters.http.router (terminus)
```

The fault filter's `abort` block fires at a deterministic 100%
(`numerator: 100, denominator: HUNDRED`, so `selects_deterministic()` is a
pure boolean — no PRNG) and is gated by a single `HeaderMatcher` on the
`x-fault` request header with `string_match: { exact: abort }`. On each
request:

- If `x-fault: abort` is present, the fault filter short-circuits with a 503
  abort response carrying the body `"fault filter abort"`. The router never
  runs.
- If the header is absent, the fault filter passes through; the router routes
  to the direct_response → 200 OK + `"ok\n"`.

## Acceptance contract (§6.2-verified)

Per phase-11 SPEC §6.2 (empirically verified at PLAN-write via Docker run of
`envoyproxy/envoy:v1.33.0` on an H2 listener), the 503 abort wire shape is:

- **Status:** 503.
- **Body:** literal byte string `"fault filter abort"` (18 bytes, NO trailing
  newline).
- **Header set:** 4 standard HTTP/2 response headers — `server`,
  `content-length`, `content-type`, `date`. Note there is **NO `connection`
  header** (it is an HTTP/2-forbidden hop-by-hop header). This is the key
  delta from the H1 fixtures (e.g. 0017's 403 carries `connection`).

The pass-through 200 response carries the body `"ok\n"` (3 bytes) from the
direct_response `inline_string`.

## Bilateral D6 validation (closes 09 REVIEW M2)

This fixture is **load-bearing for the phase-11 D6
`decorate_filter_synth_response_h2` helper** (landed at Task 4). The 2 abort
probes (statuses 1 + 3) engage the H2 filter-synth writer path end-to-end
against BOTH proxies — envoy-rust's `decorate_filter_synth_response_h2` must
decorate the filter-emitted 503 + body with the standard H2 header set so the
set-equal-modulo-allow-list diff passes against upstream Envoy. This is the
bilateral demonstration that the 09 REVIEW M2 close is real, not just
unit-tested. The 2 pass-through probes (statuses 2 + 4) bypass the helper and
route to the direct_response, demonstrating the helper is filter-agnostic by
design.

## Assertion strategy

4 sequential `Http1Probe` entries (the struct is codec-agnostic and is reused
directly by `Driver::Http2ProbeList`) with per-probe `extra_headers`. Each
probe asserts:

- `expected_status` exact (503 for probes 1 + 3; 200 for probes 2 + 4), plus
  cross-proxy `response_status: exact`.
- `expected_body: byte_exact` (`"fault filter abort"` for 503; `"ok\n"` for
  200 — both proxies emit identical bytes), plus cross-proxy
  `response_body: byte_exact`.
- `expected_headers: set_equal_modulo_allow_list` — cross-proxy header-set
  equality modulo the `Header allow-list` table at
  `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (the `server` + `date` rows cover
  implementation-identifying / wall-clock divergences; the remaining
  `content-length` / `content-type` headers are value-exact across proxies).

The top-level `equivalence` block carries only `response_status` +
`response_body` (the harness `Equivalence` struct models no
`response_headers` field — the header axis is per-probe via
`expected_headers`).

## Per-side YAML asymmetry

`envoy.yaml` (upstream) carries an `admin` block (`port_value: 0`;
kernel-ephemeral), bind `0.0.0.0:{{PORT}}` (Docker container public bind), and
`generate_request_id: false` (envoy-rust does not inject `x-request-id`;
disable upstream injection for header-set parity). `envoy-rust.yaml` carries
the symmetric narrow shape: no `admin` block, bind `127.0.0.1:{{PORT}}`, and
no `generate_request_id` field (envoy-rust's HCM config does not model it and
rejects it via `#[serde(deny_unknown_fields)]`). The `{{PORT}}` token is
harness-substituted to a per-fixture-run port.
