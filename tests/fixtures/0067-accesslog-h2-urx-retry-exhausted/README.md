# Fixture 0067 — H2 access-log `%RESPONSE_FLAGS%` retry-limit-exceeded failure path (`URX`, byte-exact)

The H2 analogue of fixture `0059` (phase 51, the H1 `URX` witness) and the
FOURTH fixture built on `Driver::Http2AccessLogByteExact` (opened by phase
56, fixture `0064`; extended by phase 57's `0065` and phase 58's `0066`).
Phase 61 (ADR-0118) witnesses the FOURTH H2 `%RESPONSE_FLAGS%` value, `URX`
(UpstreamRetryLimitExceeded), byte-exact on the H2 retry-limit-exceeded 503
path.

## What this proves

Before this phase, envoy-rust's H2 `%RESPONSE_FLAGS%` derive had no arm for
the retry-limit-exceeded disposition — its completing response's rcd is the
SAME `via_upstream` string a normal successful upstream response carries, so
it fell to the derive's `_ => "-"` arm. Phase 61 (i) declares a new
per-stream boolean discriminator, (ii) sets it at the retry-loop's EXISTING
post-loop limit-exceeded exit (the same gate that already ticks
`upstream_rq_retry_limit_exceeded`), (iii) threads it through
`finalize_h2_stream`'s sole call site, and (iv) wraps the existing
three-arm derive with a boolean-gated check. UNLIKE fixtures `0058`
(phase 50) / `0065` (phase 57), NO status-code correction was needed — the
underlying H2 retry-limit-exceeded mechanics (status 503,
`x-envoy-attempt-count: 2`, all four retry counters) were ALREADY correct
and ALREADY covered by the existing phase-16 in-process test
`h2_retry_limit_exceeded_path_always_503`.

## Probe

| # | request (H2, `:authority` = `envoy-rust.test`) | arm | emitted JSON object (byte-identical on both sides) |
|---|---|---|---|
| 1 | `GET /retry-exhausted` | retry-limit-exceeded (`num_retries:1`, always-503 backend) | see below |

```
{"method":"GET","proto":"HTTP/2","rc":503,"rcd":"via_upstream","rf":"URX"}
```

The cluster is a PLAIN `STRICT_DNS` H1-upstream cluster (NO
`typed_extension_protocol_options`) — the retry loop is upstream-protocol-
agnostic, confirmed by BOTH the state-0 recon (live Envoy emits the
identical rcd/rf pair regardless of the cluster's upstream protocol) and the
existing phase-16 in-process test (which already drives an H1-protocol
backend through the H2 downstream path). This differs from fixture `0066`,
whose pool-overflow arm required an H2-upstream cluster to route through the
H2 connection pool.

## Driver

`kind: http2_access_log_byte_exact` (`Driver::Http2AccessLogByteExact`,
opened at phase 56) — NO harness driver change this phase. The backend
wiring gate (`tests/differential/src/lib.rs`'s `needs_health_aware_backend`
allowlist + the `/retry-exhausted=503` per-path arm, both previously keyed
to `0059` only) gains a `"0067-accesslog-h2-urx-retry-exhausted"` arm
(mechanical, two additions reusing the IDENTICAL per-path string `0059`
already uses).

## `0001`-`0066` byte-preservation

This phase's change is additive — gated on `attempts > 1 &&
!retry_budget_blocked && final_retriable`, which requires a route
`retry_policy` whose budget is fully consumed by consecutive retriable
outcomes. NONE of the pre-existing H2 fixtures (`0009`, `0010`, `0018`,
`0021`, `0064`, `0065`, `0066`) configures ANY `retry_policy` at all
(re-confirmed by a fresh `grep -n retry_policy` over each
`envoy-rust.yaml` — zero hits), so `0001`-`0066` stay byte-identical; only
the new `0067` observes the new `rf:"URX"` witness.

## Cross-references

- ADR: ADR-0118 (state-1 brainstorm + state-2 PLAN — the H2 `URX` witness).
- Related fixtures: `0059` (the H1 `URX` witness this fixture mirrors on
  H2); `0064`/`0065`/`0066` (the H2 `NR`/`UH`/`UO` witnesses that
  opened/extended `Driver::Http2AccessLogByteExact`).
- Carry-forward: **M56-1** — the remaining H2 `%RESPONSE_FLAGS%` values
  (`UF`/`UC`) + the H2 failure-path `%RESPONSE_CODE_DETAILS%` strings
  beyond `route_not_found`/`no_healthy_upstream`/`{overflow}`, still open
  for future one-flag-at-a-time phases.
