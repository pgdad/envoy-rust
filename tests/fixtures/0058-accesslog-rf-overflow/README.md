# Fixture 0058 — access-log `%RESPONSE_FLAGS%` overflow path (`UO`, byte-exact)

The **THIRD non-`-` `%RESPONSE_FLAGS%` witness** (phase 50, ADR-0107), after
phase 48's `NR` (NoRoute, fixture 0056) and phase 49's `UH` (NoHealthyUpstream,
fixture 0057). This fixture witnesses the circuit-breaker overflow flag — `UO`
(UpstreamOverflow) — BYTE-EXACT on the 503 path. It is ALSO the **FIRST witness
of the overflow `%RESPONSE_CODE_DETAILS%`** value
`upstream_reset_before_response_started{overflow}`.

The harness scrapes each proxy's file and asserts every line is
**byte-identical** between upstream Envoy v1.33.0 and envoy-rust (whole-line
`==`, the same `http1_access_log_byte_exact` driver as fixtures
0040/0046/0053/0056/0057).

## What this proves (`UO` is byte-exact cross-proxy)

On a circuit-breaker overflow, both proxies return a deterministic 503. Envoy
v1.33.0 renders `%RESPONSE_FLAGS%` = `UO` on this path (state-0 recon: live
Envoy emits
`{"rc":503,"rcd":"upstream_reset_before_response_started{overflow}","rf":"UO"}`).
envoy-rust now sets `%RESPONSE_CODE_DETAILS%` =
`upstream_reset_before_response_started{overflow}` at the retry-loop consumption
site (the `outcome:None` overflow discriminator) and DERIVES `%RESPONSE_FLAGS%`
= `UO` from it at the H1 record-build site (was the no-flags sentinel `-` /
`via_upstream`). The 503 status/body/headers are UNCHANGED — the flag and the
`rcd` are purely additive (one arm added to the derive; the phase-48
`route_not_found => "NR"` and phase-49 `no_healthy_upstream => "UH"` arms are
preserved verbatim → fixtures 0056/0057 stay byte-identical).

The assertion is **pure cross-proxy equality** — there is NO static expected
literal. The overflow synth-503 is deterministic on both sides, so the
byte-exact driver covers the line.

## The `json_format` map (circuit-breaker overflow route)

```yaml
route_config:
  virtual_hosts:
    - routes:
        - match: { prefix: "/" }
          route: { cluster: backend_cluster }
log_format:
  json_format:
    rc: "%RESPONSE_CODE%"
    rcd: "%RESPONSE_CODE_DETAILS%"
    rf: "%RESPONSE_FLAGS%"
```

| key   | operator                  | rendered value                                       |
|-------|---------------------------|-----------------------------------------------------|
| `rc`  | `%RESPONSE_CODE%`         | `503` (json NUMBER)                                  |
| `rcd` | `%RESPONSE_CODE_DETAILS%` | `upstream_reset_before_response_started{overflow}`  |
| `rf`  | `%RESPONSE_FLAGS%`        | `UO`                                                 |

`%RESPONSE_CODE%` renders the bare json NUMBER `503` (not a quoted string) —
precedent fixture `0047-accesslog-json-nested`. Keys sort by UTF-8 byte order
(ADR-0094 §A): rc, rcd, rf; compact separators + ONE trailing `\n` (ADR-0092
§E).

## Probe

| # | request                    | emitted JSON object (byte-identical on both sides) |
|---|----------------------------|----------------------------------------------------|
| 1 | `GET /` (no extra headers) | see below                                          |

```
{"rc":503,"rcd":"upstream_reset_before_response_started{overflow}","rf":"UO"}
```

A single probe — the overflow path is a single pending-gate rejection arm.

## The circuit-breaker overflow trigger (endpoint never dialed)

`backend_cluster` is `STATIC` ROUND_ROBIN with `circuit_breakers.thresholds`
set to `max_connections: 1` / `max_pending_requests: 0` and ONE endpoint at the
LITERAL unreachable address `127.0.0.1:1`. On the first `GET /`, the
connect-on-miss pending-gate rejects the request with the overflow synth-503
BEFORE the `127.0.0.1:1` endpoint is dialed — `max_pending_requests: 0` admits
no queued request while the pool opens a connection. The `127.0.0.1:1` endpoint
is **never dialed** — no backend spawns.

Using a literal address (not a `{{BACKEND_*}}` marker) keeps both configs
byte-identical with no shared-IP machinery and no backend spawn — the asserted
line logs no `%UPSTREAM_HOST%`, so the endpoint address never appears. This is
the same NO-backend-spawned topology as fixtures 0053/0057.

## M50-C deferral (only the pool PendingOverflow arm is exercised here)

This fixture exercises ONLY the connection-pool `PendingOverflow` arm of the
overflow path (`max_connections: 1` / `max_pending_requests: 0`). The
request-budget overflow arm (`max_requests`/`max_retries` thresholds) is NOT
exercised here and is deferred as **M50-C** — it is a separate overflow surface
that would need its own fixture.

## Per-side divergences

| Side       | bind address | admin block | access-log path                          |
|------------|--------------|-------------|------------------------------------------|
| envoy      | `0.0.0.0`    | yes (port 0)| `/tmp/0058-envoy-mount/access.log`       |
| envoy-rust | `127.0.0.1`  | omitted     | `/tmp/0058-envoy-rust-mount/access.log`  |

There are NO upstream-specific deltas here: this fixture spawns no backend and
never sends an upstream-bound request. The cluster + route + circuit_breakers +
`json_format` are BYTE-IDENTICAL across the two files (only the documented
per-side deltas differ). The parent directory is bind-mounted from the host into
the Envoy container so the harness can read the access.log file after the request
completes (same wiring as fixtures 0012 / 0040 / 0053 / 0056 / 0057).

## Driver

`kind: http1_access_log_byte_exact` (same driver as fixtures
0040/0046/0053/0056/0057) — drives the probe, scrapes both files, asserts the
scraped line count equals `probes.len()`, and calls
`access_log::assert_access_log_lines_byte_identical`. No new harness code; no
backend spawn (no `{{HTTP1_BACKEND_PORT}}` marker).

## Cross-references

- ADR: ADR-0107 (phase-50 pick + scope — witness the THIRD non-`-`
  `%RESPONSE_FLAGS%` value `UO` byte-exact, AND the FIRST overflow
  `%RESPONSE_CODE_DETAILS%` value, by setting
  `upstream_reset_before_response_started{overflow}` at the overflow synth-503
  site and extending the H1 derive to map it to `"UO"`).
- Related fixtures: 0057 (`%RESPONSE_FLAGS%` = `UH`, the phase-49 sibling on the
  no-healthy 503 path, whose NO-backend-spawned literal-address topology this
  reuses), 0056 (`%RESPONSE_FLAGS%` = `NR`, the phase-48 sibling), 0023 (the
  circuit-breaker overflow cluster shape this reuses), 0047 (the
  `%RESPONSE_CODE%` bare-json-NUMBER precedent), 0012 (default per-token
  access-log baseline).
- Deferred: the H2 overflow path (M45-1: H2 record-build site also hard-codes
  `"-"`; no H2 access-log differential driver), the request-budget overflow arm
  (M50-C), and the other non-`-` flags `UF`/`DC`/`URX` (M45-2, non-deterministic
  connect/timeout surfaces — `UO` now moves OUT of that unwitnessed set).
