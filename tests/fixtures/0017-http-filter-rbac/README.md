# Fixture 0017: HTTP filter — RBAC

Phase 10 differential acceptance fixture for `envoy.filters.http.rbac`.
Both upstream Envoy (v1.33.0) and envoy-rust must produce the deterministic
status sequence `[403, 200, 403, 200]` for 4 sequential `GET /` requests
against a single direct_response route, given an HCM filter chain of
`[envoy.filters.http.rbac, envoy.filters.http.router]` with `action: ALLOW`
and a single policy `pass_with_header` requiring the request to carry
`x-rbac-pass: yes`. Probes alternate header presence / value to exercise
both the Allow-match (200) and the Allow-no-match-default-Deny (403) paths.

## Filter chain

```
http_filters:
  - envoy.filters.http.rbac (action: ALLOW, policy pass_with_header)
  - envoy.filters.http.router (terminus)
```

Decode-side iteration: rbac invokes first (declaration order). On each
request, `RbacFilter::decode_headers`
(`crates/envoy-filter/src/rbac.rs:173`) walks the policies map in
`BTreeMap` alphabetical order, short-circuiting on the first policy whose
permission AND principal both match:

- If a policy matches under `action: ALLOW`, `Decision::Continue` falls
  through to router which routes to direct_response → 200 OK + `"ok\n"`.
  Increment `http.ingress_http.rbac.allowed`.
- If no policy matches under `action: ALLOW`, `Decision::StopAndSend`
  short-circuits with `FilterResponse { status: 403, reason:
  Some("Forbidden"), headers: vec![], body: Bytes::from_static(b"RBAC:
  access denied") }`. Increment `http.ingress_http.rbac.denied`.

Encode-side: rbac is a no-op (`Decision::Continue`) per SPEC §5.4.
Router runs on encode but a direct_response path has nothing to mutate.

## ADR-0034 contract

Per phase-10 ADR-0034 ("Phase-10 SPEC §2.2 revision per upstream Envoy
v1.33 empirical observation"), the 403 response wire shape is:

- **Status:** 403 Forbidden.
- **Body:** literal byte string `"RBAC: access denied"` (19 bytes, NO
  trailing newline; source-hardcoded on upstream Envoy v1.33;
  envoy-rust emits the same bytes via
  `Bytes::from_static(b"RBAC: access denied")` per phase-10 PLAN
  lock-in #14). The SPEC §2.2 projection of `"RBAC: access denied\n"`
  (20 bytes) was off by 1 byte and is corrected at ADR-0034.
- **Header set:** 5 standard HTTP/1.1 response headers — `server`,
  `date`, `content-length`, `content-type`, `connection`. envoy-rust
  populates these via the H1 HCM's `decorate_filter_synth_response`
  helper added at phase-09 ADR-0033 Commit C `ae2cef0`
  (`crates/envoy-http1/src/hcm.rs:932`). The filter itself only emits
  the body + status; the standard headers are decorated at the
  writer-arm site to mirror the synth-from-build paths.

This fixture is the **first non-LocalRateLimit bilateral consumer** of
the `decorate_filter_synth_response` helper. The 2 deny probes
(statuses 1 + 3) engage the helper end-to-end against both proxies;
the 2 allow probes (statuses 2 + 4) bypass the helper and pass through
to the direct_response route, demonstrating that the helper is
filter-agnostic by design.

## Assertion strategy

4 sequential `Http1Probe` entries (`Driver::Http1ProbeList`) with
per-probe `extra_headers`. This fixture is the first to exercise the
per-probe distinct-headers axis of the `Http1Probe` shape (the field
`extra_headers: Vec<(String, String)>` with `#[serde(default)]` at
`tests/differential/src/lib.rs:619-635` was landed in phase 04.2 and
sat unused by every prior fixture; fixture 0016 used uniform headers
across all 5 probes, so probe-to-probe variation begins here). Each
probe asserts:

- `expected_status` exact (403 for probes 1 + 3; 200 for probes 2 + 4).
- `expected_body: byte_exact` (`"RBAC: access denied"` for 403;
  `"ok\n"` for 200 — both proxies emit identical bytes per ADR-0034 +
  the direct_response inline_string).
- `expected_headers: set_equal_modulo_allow_list` — cross-proxy
  header-set equality modulo the `Header allow-list` table at
  `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (the 04.1-landed `server` +
  `date` rows cover implementation-identifying / wall-clock
  divergences; the remaining 3 standard headers content-length /
  content-type / connection are value-exact across proxies under the
  harness's deterministic `Connection: close` request framing).

The in-process backstop projected at Task 7
(`crates/envoy-bin/tests/http_filter_rbac.rs` — pending dispatch)
complements this fixture with in-process exercises of the same wire
shape (no Docker dependency; complementary surface).

## Stats wired (per BEHAVIOR_CONTRACT.md `Stat-name mapping` §10 entries)

- `http.ingress_http.rbac.allowed` — 2 (probes 2 + 4 matched the
  `pass_with_header` policy).
- `http.ingress_http.rbac.denied` — 2 (probes 1 + 3 failed to match
  any policy under `action: ALLOW`).

The stats are landed by Task 3 (`da32137`) + `BEHAVIOR_CONTRACT.md`
rows appended at the same commit; this fixture's wire-level assertion
is the end-to-end demonstration that the counters fire at the right
moments under real traffic.

## Per-side YAML asymmetry

`envoy.yaml` (upstream) carries:

- An `admin` block (`port_value: 0`; kernel-ephemeral per the
  fixture-0016 precedent).
- Bind address `0.0.0.0:{{PORT}}` (Docker container binds publicly
  inside the container; harness publishes via `-p`; `{{PORT}}` is
  harness-substituted).
- `generate_request_id: false` (envoy-rust does not inject
  `x-request-id`; disable upstream injection for header-set parity
  per the fixture-0016 precedent).

`envoy-rust.yaml` carries the symmetric narrow shape:

- No `admin` block (envoy-rust harness does not probe admin on
  direct_response fixtures).
- Bind address `127.0.0.1:{{PORT}}` (envoy-rust is a subprocess on the
  host; `127.0.0.1` per the fixture-0013/0016 precedent).
- No `generate_request_id` field (envoy-rust's HCM config does not
  model it and rejects it via `#[serde(deny_unknown_fields)]`;
  envoy-rust does not inject `x-request-id` by default so this is a
  no-op divergence).
