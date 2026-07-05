# Fixture 0066 — H2 access-log `%RESPONSE_FLAGS%` pool/circuit-breaker overflow failure path (`UO`, byte-exact)

The H2 analogue of fixture `0058` (phase 50, the H1 `UO` witness) and the
THIRD fixture built on `Driver::Http2AccessLogByteExact` (opened by phase 56,
fixture `0064`; extended by phase 57, fixture `0065`). Phase 58 (ADR-0115)
witnesses the THIRD H2 `%RESPONSE_FLAGS%` value, `UO` (UpstreamOverflow),
byte-exact on the H2 pool/circuit-breaker overflow 503 path.

## What this proves

Before this phase, envoy-rust's H2 caller-loop unconditionally set
`response_code_details_for_log_h2 = Some("via_upstream")` whenever an attempt
had a picked endpoint (`endpoint: Some`) — including the pool-overflow
`H2AttemptResult` (`endpoint: Some`, `outcome: None`, `crates/envoy-http2/src/hcm.rs:407`-`417`),
which is NOT a real upstream response. Phase 58 (i) discriminates the
overflow outcome from a real response at that caller-loop site (mirroring
the H1 phase-50 discriminator), (ii) tags the pre-route request-budget
`Rejected` arm directly (it bypasses the retry loop entirely), and (iii)
extends the H2 `%RESPONSE_FLAGS%` derive to a third arm. UNLIKE fixtures
`0058` (phase 50) and `0065` (phase 57), NO status-code correction was
needed — envoy-rust's H2 overflow status was already correct (503, via the
pre-existing `synth_h2_overflow()`).

## Probe

| # | request (H2, `:authority` = `envoy-rust.test`) | arm | emitted JSON object (byte-identical on both sides) |
|---|---|---|---|
| 1 | `GET /` | pool-overflow (`max_pending_requests:0`) | see below |

```
{"method":"GET","proto":"HTTP/2","rc":503,"rcd":"upstream_reset_before_response_started{overflow}","rf":"UO"}
```

The cluster is the IDENTICAL shape fixture `0058` uses (`circuit_breakers.thresholds:[{max_connections:1,max_pending_requests:0}]`,
a literal dead endpoint `127.0.0.1:1`), PLUS `typed_extension_protocol_options`
(an H2 upstream — required for envoy-rust's side to route through the H2 pool
and hit `PoolError::PendingOverflow`; the state-0 recon confirmed live Envoy
emits identical output regardless of the cluster's upstream protocol, so both
sides use the H2-upstream shape for config parity) — only `codec_type: HTTP2`
+ `http2_protocol_options: {}` (fixture `0064`/`0065`'s listener shape) are
substituted for `0058`'s `codec_type: HTTP1`.

## Driver

`kind: http2_access_log_byte_exact` (`Driver::Http2AccessLogByteExact`,
opened at phase 56) — NO harness change this phase. Drives the probe over
H2-prior-knowledge via `drive_http2`, scrapes both files, asserts the scraped
line count equals `probes.len()` (here 1), and calls
`access_log::assert_access_log_lines_byte_identical`.

## `0001`-`0065` byte-preservation

This phase's changes are additive — gated on (a) an attempt with
`endpoint:Some, outcome:None` (uniquely the pool-overflow result), and (b)
`try_acquire_request()` returning `Rejected` (requires `circuit_breakers.thresholds.max_requests: 0`).
NONE of the pre-existing H2 fixtures (`0009`, `0010`, `0018`, `0021`, `0064`,
`0065`) configures a `circuit_breakers` threshold that could reach either
path — re-confirmed by a fresh `grep -n circuit_breakers` over each
`envoy-rust.yaml` (only `0021`'s `max_connections: 4`, headroom only). So
`0001`-`0065` stay byte-identical; only the new `0066` observes the changed
rcd/rf.

## Cross-references

- ADR: ADR-0115 (state-1 brainstorm + state-2 PLAN — the H2 `UO` witness).
- Related fixtures: `0058` (the H1 `UO` witness this fixture mirrors on H2);
  `0064`/`0065` (the H2 `NR`/`UH` witnesses that opened/extended
  `Driver::Http2AccessLogByteExact`).
- Carry-forward: **M56-1** — the remaining H2 `%RESPONSE_FLAGS%` values
  (`URX`/`UF`/`UC`) + the H2 failure-path `%RESPONSE_CODE_DETAILS%` strings
  beyond `route_not_found`/`no_healthy_upstream`/`{overflow}`, still open for
  future one-flag-at-a-time phases. Also notes a candidate future
  carry-forward slice: the H2 request-budget arm's OWN differential
  access-log witness (a `max_requests: 0` trigger, distinct from this
  fixture's pool trigger) — covered at the in-process level only this phase
  (§F2), mirroring how H1's equivalent gap (M50-C) was later closed cheaply
  by phase 55.
