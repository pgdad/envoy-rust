# Fixture 0053 — access-log `%RESPONSE_CODE_DETAILS%` failure path (`no_healthy_upstream`, byte-exact)

The **FIRST failure-path `%RESPONSE_CODE_DETAILS%` witness** (phase 45,
ADR-0102). Fixtures 0051/0052 witnessed `%RESPONSE_CODE_DETAILS%` only on the
SUCCESS path (`via_upstream` / `direct_response`). This fixture witnesses the
no-healthy-upstream FAILURE detail — `no_healthy_upstream` — BYTE-EXACT on the
503 path.

The harness scrapes each proxy's file and asserts every line is
**byte-identical** between upstream Envoy v1.33.0 and envoy-rust (whole-line
`==`, the same `http1_access_log_byte_exact` driver as fixtures
0040/0046/0047/0048/0049/0050/0051/0052).

## What this proves (`no_healthy_upstream` is byte-exact cross-proxy)

On a NO_FALLBACK subset-miss, both proxies return a deterministic 503
`no healthy upstream`. Envoy v1.33.0 renders `%RESPONSE_CODE_DETAILS%` =
`no_healthy_upstream` on this path (state-1 recon: live Envoy emits
`{"rc":503,"rcd":"no_healthy_upstream","rf":"UH"}`). envoy-rust now SETS
`Some("no_healthy_upstream")` at its H1 no-healthy synth arm — the access-log
record is built unconditionally below the writer-arm match, and the detail was
left `None` (rendering `rcd:null`) until phase 45 added the `else` branch at the
Proxy arm (the `attempt.endpoint.is_none()` path is EXCLUSIVELY the
no-healthy `pick()->None` case). The 503 status/body/headers/flags are
UNCHANGED — the detail is purely additive.

The assertion is **pure cross-proxy equality** — there is NO static expected
literal. The no-healthy synth-503 is deterministic on both sides, so the
byte-exact driver covers the line.

## The `json_format` map (NO_FALLBACK subset-miss route)

```yaml
route_config:
  virtual_hosts:
    - routes:
        - match: { prefix: "/" }
          route:
            cluster: subset_cluster
            metadata_match: { filter_metadata: { envoy.lb: { stage: nonexistent } } }
log_format:
  json_format:
    rc: "%RESPONSE_CODE%"
    rcd: "%RESPONSE_CODE_DETAILS%"
    method: "%REQ(:METHOD)%"
    proto: "%PROTOCOL%"
```

| key      | operator                 | rendered value          |
|----------|--------------------------|-------------------------|
| `method` | `%REQ(:METHOD)%`         | `GET`                   |
| `proto`  | `%PROTOCOL%`             | `HTTP/1.1`              |
| `rc`     | `%RESPONSE_CODE%`        | `503` (json NUMBER)     |
| `rcd`    | `%RESPONSE_CODE_DETAILS%`| `no_healthy_upstream`   |

`%RESPONSE_CODE%` renders the bare json NUMBER `503` (not a quoted string) —
precedent fixture `0047-accesslog-json-nested`. Keys sort by UTF-8 byte order
(ADR-0094 §A): method, proto, rc, rcd; compact separators + ONE trailing `\n`
(ADR-0092 §E).

## Probe

| # | request                    | emitted JSON object (byte-identical on both sides) |
|---|----------------------------|----------------------------------------------------|
| 1 | `GET /` (no extra headers) | see below                                          |

```
{"method":"GET","proto":"HTTP/1.1","rc":503,"rcd":"no_healthy_upstream"}
```

## The NO_FALLBACK subset-miss trigger (NOT empty-endpoints)

`subset_cluster` is `STATIC` ROUND_ROBIN with an `lb_subset_config`
(`fallback_policy: NO_FALLBACK`, `subset_selectors: [{ keys: [stage] }]`) and
ONE endpoint carrying `metadata.filter_metadata.envoy.lb: { stage: prod }` at
the LITERAL unreachable address `127.0.0.1:1`. The single route's
`metadata_match` selects the NON-EXISTENT `stage: nonexistent` subset (the
fixture-`0038` `/nope` pattern) — `pick()` resolves to NO eligible endpoint →
`None` → 503 `no healthy upstream` at ROUTING time. The `127.0.0.1:1` endpoint
is **never dialed** — no backend spawns.

This is the load-bearing reason the trigger is a subset-miss and NOT
empty-endpoints (`endpoints: []`): empty endpoints are boot-fatal in envoy-rust
(`ConfigError::EmptyClusterEndpoints`). Using a literal address (not a
`{{BACKEND_IP}}`/`{{HTTP1_BACKEND_PORT}}` marker) keeps both configs
byte-identical with no shared-IP machinery and no backend spawn — the asserted
line logs no `%UPSTREAM_HOST%`, so the endpoint address never appears.

## Per-side divergences

| Side       | bind address | admin block | access-log path                          |
|------------|--------------|-------------|------------------------------------------|
| envoy      | `0.0.0.0`    | yes (port 0)| `/tmp/0053-envoy-mount/access.log`       |
| envoy-rust | `127.0.0.1`  | omitted     | `/tmp/0053-envoy-rust-mount/access.log`  |

There are NO upstream-specific deltas here (unlike fixture 0052): this fixture
spawns no backend and never sends an upstream-bound request, so 0052's
`generate_request_id: false` / `request_headers_to_remove` deltas do not apply.
The cluster + route + subset config + `metadata_match` are BYTE-IDENTICAL across
the two files (only the documented per-side deltas differ — the fixture-0038
simpler subset-cluster convention). The parent directory is bind-mounted from
the host into the Envoy container so the harness can read the access.log file
after the request completes (same wiring as fixtures 0012 / 0040 / 0050 / 0051 /
0052).

## Driver

`kind: http1_access_log_byte_exact` (same driver as fixtures
0040/0046/0047/0048/0049/0050/0051/0052) — drives the probe sequence, scrapes
both files, asserts the scraped line count equals `probes.len()`, and calls
`access_log::assert_access_log_lines_byte_identical`. No new harness code; no
backend spawn (no `{{HTTP1_BACKEND_PORT}}` marker).

## Cross-references

- ADR: ADR-0102 (state-1 brainstorm + state-2 PLAN — the failure-path
  `%RESPONSE_CODE_DETAILS%` = `no_healthy_upstream` pick: SET the detail at the
  H1 no-healthy synth arm + witness it byte-exact via a NO_FALLBACK subset-miss).
- Related fixtures: 0052 (`%UPSTREAM_HOST%`, the success-path access-log fixture
  this extends to the failure path), 0051 (`%UPSTREAM_CLUSTER%` +
  `%RESPONSE_CODE_DETAILS%` success path), 0038 (the NO_FALLBACK subset cluster
  + `/nope` subset-miss 503 trigger this reuses), 0047 (the `%RESPONSE_CODE%`
  bare-json-NUMBER precedent), 0012 (default per-token access-log baseline).
- Deferred: the connect-failure / overflow failure details (M45-2) and the H2
  no-healthy path (M45-1: H2 returns 502, no H2 access-log differential driver).
