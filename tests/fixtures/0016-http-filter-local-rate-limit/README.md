# Fixture 0016: HTTP filter — local rate limit

Phase 09 differential acceptance fixture for `envoy.filters.http.local_ratelimit`.
Both upstream Envoy (v1.33.0) and envoy-rust must produce the deterministic
status sequence `[200, 200, 200, 429, 429]` for 5 sequential `GET /` requests
against a single direct_response route, given a token bucket of
`max_tokens: 3, tokens_per_fill: 3, fill_interval: 60s` (no refill within
the 5-probe burst window since the burst completes in well under 60s).

## Filter chain

```
http_filters:
  - envoy.filters.http.local_ratelimit (token_bucket: 3/3/60s)
  - envoy.filters.http.router (terminus)
```

Decode-side iteration: local_ratelimit invokes first (declaration order). On
each request:

- If a token is available, `try_acquire` succeeds; `Decision::Continue` falls
  through to router which routes to direct_response → 200 OK + `"ok\n"`.
- If no tokens are available, `try_acquire` fails; `Decision::StopAndSend`
  short-circuits with status 429 + body `"local_rate_limited"` (18 bytes,
  upstream-Envoy-parity per ADR-0033).

Encode-side: local_ratelimit is a no-op (`Decision::Continue`). Router runs
on encode but a direct_response path has nothing to mutate.

## ADR-0033 contract

Per phase-09 ADR-0033 ("Phase-09 SPEC §2.2 revision per upstream Envoy v1.33
empirical observation"), the 429 response wire shape is:

- **Status:** 429 Too Many Requests.
- **Body:** literal byte string `"local_rate_limited"` (18 bytes;
  source-hardcoded on upstream Envoy v1.33; envoy-rust emits the same bytes
  via `Bytes::from_static(b"local_rate_limited")` per ADR-0033 Commit B).
- **Header set:** 5 standard HTTP/1.1 response headers — `server`, `date`,
  `content-length`, `content-type`, `connection`. envoy-rust populates these
  via the H1 HCM's `decorate_filter_synth_response` helper added at
  ADR-0033 Commit C (the filter itself only emits operator-configured
  `response_headers_to_add` entries; the standard headers are decorated at
  the writer-arm site to mirror the synth-from-build paths).
- **`x-envoy-ratelimited` is NOT emitted** by `envoy.filters.http.local_ratelimit`
  on either proxy. Empirical observation revealed at Task 5 dispatch:
  upstream Envoy v1.33's local_ratelimit emits NO `x-envoy-ratelimited`
  header (that header is owned by the global ratelimit filter
  `envoy.filters.http.ratelimit` + router-side response-flag handling, not
  by local_ratelimit). envoy-rust matches upstream per ADR-0033 Commit B.

The phase-09 PLAN's original lock-in #13 (synth response carries
`x-envoy-ratelimited: true` + empty body) and lock-in #30 (BEHAVIOR_CONTRACT
Header allow-list row for `x-envoy-ratelimited`) are both voided per
ADR-0033.

## Assertion strategy

5 sequential `Http1Probe` entries (`Driver::Http1ProbeList`). Each probe
asserts:

- `expected_status` exact (200 for probes 1-3; 429 for probes 4-5).
- `expected_body: byte_exact` (`"ok\n"` for 200; `"local_rate_limited"` for
  429 — both proxies emit the same 18 bytes per ADR-0033).
- `expected_headers: set_equal_modulo_allow_list` — cross-proxy header-set
  equality modulo the `Header allow-list` table at `docs/envoy-rust/BEHAVIOR_CONTRACT.md`
  (the 04.1-landed `server` + `date` rows cover the implementation-
  identifying / wall-clock divergences; the remaining 3 standard headers
  (`content-length`, `content-type`, `connection`) are value-exact across
  proxies under the deterministic burst).

The in-process backstop at `crates/envoy-bin/tests/http_filter_local_rate_limit.rs`
(landed at Task 7) complements this fixture with in-process exercises of
the same wire shape (no Docker dependency; complementary surface).

## Stats wired (per BEHAVIOR_CONTRACT.md `Stat-name mapping` §09 entries)

- `http_local_rate_limit.phase_09.enabled` — 5 (every decode-side invocation).
- `http_local_rate_limit.phase_09.ok` — 3 (probes 1-3 acquired tokens).
- `http_local_rate_limit.phase_09.rate_limited` — 2 (probes 4-5 failed `try_acquire`).
- `http_local_rate_limit.phase_09.enforced` — 2 (probes 4-5 emitted 429).

The stats are landed by Task 3 (`70bad43`) + BEHAVIOR_CONTRACT.md rows
appended at the same commit; this fixture's wire-level assertion is the
end-to-end demonstration that the counters fire at the right moments under
real traffic.

## Per-side YAML asymmetry

`envoy.yaml` (upstream) carries:

- An `admin` block (upstream needs admin for testcontainers harness probing).
- Bind address `0.0.0.0:{{PORT}}` (Docker container binds publicly inside
  the container; harness publishes via `-p`).
- `filter_enabled` + `filter_enforced` runtime fractional-percent fields
  set to 100% (upstream defaults both to 0% / off; envoy-rust defaults to
  always-on per phase-09 lock-in — fixture-0013 precedent for per-side
  YAML asymmetry).
- `tokens_per_fill: 3` (upstream's `TokenBucketValidator` rejects 0;
  envoy-rust accepts 0 per validator lock-in #4; the stricter intersection
  is 3 which keeps both proxies bilaterally green without changing
  semantic — no refill within the 1-second burst window regardless of
  whether `tokens_per_fill` is 0 or 3 because `fill_interval: 60s`).

`envoy-rust.yaml` carries the symmetric narrow shape:

- No `admin` block (envoy-rust harness doesn't probe admin on
  direct_response fixtures).
- Bind address `127.0.0.1:{{PORT}}` (envoy-rust is a subprocess on the
  host; `127.0.0.1` per the fixture-0013 precedent).
- No `filter_enabled` / `filter_enforced` (envoy-rust's
  `LocalRateLimitConfig` schema has `#[serde(deny_unknown_fields)]`;
  always-on per phase-09 lock-in).
- `tokens_per_fill: 3` (matches upstream for cross-proxy parity).
