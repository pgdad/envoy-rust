# Fixture 0059 — access-log `%RESPONSE_FLAGS%` retry-limit-exceeded path (`URX`, byte-exact)

The **FOURTH non-`-` `%RESPONSE_FLAGS%` witness** (phase 51, ADR-0108), after
phase 48's `NR` (NoRoute, fixture 0056), phase 49's `UH` (NoHealthyUpstream,
fixture 0057), and phase 50's `UO` (UpstreamOverflow, fixture 0058). This
fixture witnesses the retry-limit-exceeded flag — `URX`
(UpstreamRetryLimitExceeded) — BYTE-EXACT on the H1 retry-exhausted 503 path
(`retry_policy:{retry_on:"5xx", num_retries:1}`, the ADR-0045 L9 path returning
the last upstream response verbatim).

It is the **FIRST `%RESPONSE_FLAGS%` value NOT 1:1 with a unique
`%RESPONSE_CODE_DETAILS%`**: the retry-limit-exceeded path's rcd is the SHARED
`via_upstream` (the same string a normal successful upstream response carries —
the final attempt is a real upstream 503). envoy-rust already emits
`rcd:"via_upstream"` here (matching Envoy, UNCHANGED), so `URX` CANNOT be
rcd-derived — it is derived from a SEPARATE per-request boolean
(`retry_limit_exceeded_for_log`) set at the retry-loop limit-exceeded exit (the
same gate as `upstream_rq_retry_limit_exceeded`).

The harness scrapes each proxy's file and asserts every line is
**byte-identical** between upstream Envoy v1.33.0 and envoy-rust (whole-line
`==`, the same `http1_access_log_byte_exact` driver as fixtures
0040/0046/0053/0056/0057/0058).

## What this proves (`URX` is byte-exact cross-proxy)

On the retry-exhausted path, both proxies return a deterministic 503 (the last
upstream response verbatim). Envoy v1.33.0 renders `%RESPONSE_FLAGS%` = `URX`
here (state-0 recon: live Envoy emits
`{"rc":503,"rcd":"via_upstream","rf":"URX"}`). envoy-rust DERIVES
`%RESPONSE_FLAGS%` = `URX` from the `retry_limit_exceeded_for_log` boolean
(was the no-flags sentinel `-`); the `rcd` stays `via_upstream` (a real upstream
503 — UNCHANGED, already matching Envoy). The 503 status/body/headers are
UNCHANGED — the flag is purely additive (one boolean branch prepended to the
derive; the phase-48 `route_not_found => "NR"`, phase-49
`no_healthy_upstream => "UH"`, and phase-50
`upstream_reset_before_response_started{overflow} => "UO"` arms are preserved
verbatim → fixtures 0056/0057/0058 stay byte-identical).

The assertion is **pure cross-proxy equality** — there is NO static expected
literal. The retry-exhausted 503 is deterministic on both sides, so the
byte-exact driver covers the line.

## The `json_format` map (retry-limit-exceeded route)

```yaml
route_config:
  virtual_hosts:
    - routes:
        - match: { prefix: "/retry-exhausted" }
          route:
            cluster: backend
            retry_policy: { retry_on: "5xx", num_retries: 1 }
log_format:
  json_format:
    rc: "%RESPONSE_CODE%"
    rcd: "%RESPONSE_CODE_DETAILS%"
    rf: "%RESPONSE_FLAGS%"
```

| key   | operator                  | rendered value   |
|-------|---------------------------|------------------|
| `rc`  | `%RESPONSE_CODE%`         | `503` (json NUMBER) |
| `rcd` | `%RESPONSE_CODE_DETAILS%` | `via_upstream`   |
| `rf`  | `%RESPONSE_FLAGS%`        | `URX`            |

`%RESPONSE_CODE%` renders the bare json NUMBER `503` (not a quoted string) —
precedent fixture `0047-accesslog-json-nested`. Keys sort by UTF-8 byte order
(ADR-0094 §A): rc, rcd, rf; compact separators + ONE trailing `\n` (ADR-0092
§E).

## Probe

| # | request                  | emitted JSON object (byte-identical on both sides) |
|---|--------------------------|----------------------------------------------------|
| 1 | `GET /retry-exhausted`   | see below                                          |

```
{"rc":503,"rcd":"via_upstream","rf":"URX"}
```

A single probe — the retry-exhausted path is a single outcome.

## The retry-limit-exceeded trigger (a real health-aware 503 backend)

`backend` is `STRICT_DNS` ROUND_ROBIN (`dns_lookup_family: V4_ONLY`) with ONE
endpoint at `{{BACKEND_HOST}}:{{BACKEND_PORT}}`, resolved to the harness-spawned
`HealthAwareHttp1Backend`. The harness wires that backend with
`--per-path /retry-exhausted=503` (STATELESS — always 503 on `/retry-exhausted`,
no cyclic window). On `GET /retry-exhausted`, the route's
`retry_policy{retry_on:"5xx", num_retries:1}` fires: attempt 1 503s → a single
retry is issued → attempt 2 also 503s → the retry budget of 1 is consumed with
the final attempt still retriable → the LAST upstream 503 is returned downstream
verbatim (ADR-0045 L9). `upstream_rq_retry_limit_exceeded` ticks at the
retry-loop limit-exceeded exit, the same gate that sets the
`retry_limit_exceeded_for_log` boolean.

**0059 is the FIRST access-log fixture needing a real health-aware backend** —
fixtures 0056/0057/0058 used dead/never-contacted endpoints. The harness gates
the health-aware backend on a hardcoded fixture-name allowlist
(`needs_health_aware_backend`) plus a fixture-name-gated `--per-path` map; this
phase adds `0059` to both (and the `0059` YAML carries `{{BACKEND_HOST}}`/
`{{BACKEND_PORT}}` so `needs_backend` fires). The stateless `--per-path=503`
(NOT the cyclic stateful `--retry-script` 0024 uses for `/retry-success`) is
exactly right — both attempts must 503.

## Per-side divergences

| Side       | bind address | admin block | access-log path                          |
|------------|--------------|-------------|------------------------------------------|
| envoy      | `0.0.0.0`    | yes (port 0)| `/tmp/0059-envoy-mount/access.log`       |
| envoy-rust | `127.0.0.1`  | omitted     | `/tmp/0059-envoy-rust-mount/access.log`  |

`{{BACKEND_HOST}}` resolves to `host.docker.internal` (Envoy side) /
`127.0.0.1` (envoy-rust side); `{{BACKEND_PORT}}` is the health-aware backend's
actual port (identical both sides). The cluster + route + retry_policy +
`json_format` are otherwise BYTE-IDENTICAL across the two files (only the
documented per-side deltas differ). The parent directory is bind-mounted from
the host into the Envoy container so the harness can read the access.log file
after the request completes (same wiring as fixtures 0012/0040/0053/0056/0057/
0058).

**CRITICAL (plan-review C1):** the reference `envoy.yaml` admin block uses a
LITERAL `port_value: 0`, NOT `{{ADMIN_PORT}}`. The `{{ADMIN_PORT}}` marker is
only substituted when `needs_admin_port` fires (the AdminScrape / Http1KeepAlive
/ Http2KeepAlive drivers) — NOT the `http1_access_log_byte_exact` driver 0059
uses. An unresolved `{{ADMIN_PORT}}` would be left literal → invalid bootstrap →
the reference container fails to start. Cloned from the SAME-DRIVER template
`0058-accesslog-rf-overflow/envoy.yaml`.

## Driver

`kind: http1_access_log_byte_exact` (same driver as fixtures
0040/0046/0053/0056/0057/0058) — drives the probe, scrapes both files, asserts
the scraped line count equals `probes.len()`, and calls
`access_log::assert_access_log_lines_byte_identical`. No new harness code beyond
the two fixture-name-gated backend-wiring edits (the
`needs_health_aware_backend` allowlist + the `/retry-exhausted=503` `--per-path`
arm).

## Cross-references

- ADR: ADR-0108 (phase-51 pick + scope — witness the FOURTH non-`-`
  `%RESPONSE_FLAGS%` value `URX` byte-exact on the H1 retry-limit-exceeded 503
  path, the FIRST flag NOT 1:1 with a unique rcd, by setting a
  `retry_limit_exceeded_for_log` boolean at the retry-loop limit-exceeded exit
  and prepending a boolean branch to the H1 derive that renders `"URX"`).
- Related fixtures: 0058 (`%RESPONSE_FLAGS%` = `UO`, the phase-50 sibling on the
  overflow 503 path, whose `http1_access_log_byte_exact` `{rc,rcd,rf}` shape and
  admin preamble this clones), 0057 (`%RESPONSE_FLAGS%` = `UH`, phase-49), 0056
  (`%RESPONSE_FLAGS%` = `NR`, phase-48), 0024 (the retry-on-5xx topology +
  health-aware-backend `/retry-exhausted=503` wiring this reuses), 0047 (the
  `%RESPONSE_CODE%` bare-json-NUMBER precedent), 0012 (default per-token
  access-log baseline).
- Deferred: the H2 retry-limit-exceeded path (M45-1: H2 record-build site also
  hard-codes `"-"`; no H2 access-log differential driver), the retry-BUDGET-
  overflow path and the connect-failure `UF` / downstream-termination `DC` flags
  (M45-2, non-deterministic / different-gate surfaces — `URX` now moves OUT of
  that unwitnessed set).
