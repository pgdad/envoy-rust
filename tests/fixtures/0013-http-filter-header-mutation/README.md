# Fixture 0013 — `envoy.filters.http.header_mutation` end-to-end

Exercises the HeaderMutation HTTP filter in front of the Router on a
Router-proxied route to an `http1-echo-server` backend cluster. This is
phase 07.2's differential surface — the first concrete pluggable filter
proven wire-equivalent to upstream Envoy on both decode and encode
iteration states.

## Filter chain

    http_filters:
      - HeaderMutation   # request_mutations + response_mutations
      - Router           # terminus

Iteration order under the 07.1 framework: `decode_headers` runs
declaration order (HeaderMutation first, then Router no-op terminus;
route-match runs AFTER decode_headers per parent-07 SPEC §6 Rule 7);
`encode_headers` runs reverse declaration order (Router no-op first,
HeaderMutation second).

## Assertions

- **Request-side stamp at backend** (`decode_headers`).
  HeaderMutation adds `x-filter-stamp: phase-07` to the request via
  `APPEND_IF_EXISTS_OR_ADD`. The `http1-echo-server` echoes received
  request headers into the response body as sorted `  name: value`
  lines. The `expected_body: { kind: byte_exact }` assertion (both the
  per-proxy `body:` string and the `equivalence.response_body` cross-proxy
  check) confirms both proxies forwarded the same stamped request.

- **Response-side stamp at client** (`encode_headers`).
  HeaderMutation adds `x-filter-response-stamp: phase-07` to the response
  via `APPEND_IF_EXISTS_OR_ADD`. The `expected_headers:
  set_equal_modulo_allow_list` assertion confirms both proxies emitted
  the stamp (it lands identically on both — HeaderMutation is
  deterministic).

## Per-side divergence

`envoy.yaml` (reference) uses `request_headers_to_remove` +
`generate_request_id: false` to strip Envoy-v1.33-injected request
headers so the helper's deterministic echo body stays byte-equal across
both proxies (envoy-rust injects none of these per 04.3 SPEC §4).
`envoy-rust.yaml` omits `request_headers_to_remove` (envoy-rust's parser
does not recognize it), `dns_lookup_family`, and the `admin` block, and
binds `127.0.0.1` — mirrors fixture 0008's `envoy-rust.yaml` shape
(STRICT_DNS cluster per 05.1 ADR-0023; the harness substitutes
`{{BACKEND_HOST}}` per-side).
