# Fixture 0057 — access-log `%RESPONSE_FLAGS%` no-healthy path (`UH`, byte-exact)

The **SECOND non-`-` `%RESPONSE_FLAGS%` witness** (phase 49, ADR-0106), after
phase 48's `NR` (NoRoute, fixture 0056). This fixture witnesses the
no-healthy-upstream flag — `UH` (NoHealthyUpstream) — BYTE-EXACT on the 503
path. It is the `%RESPONSE_FLAGS%` analogue of phase 45's first failure-path
`%RESPONSE_CODE_DETAILS%` value `no_healthy_upstream` (fixture 0053) — just as
phase 48's `NR` was the analogue of phase 46/47's `route_not_found`.

The harness scrapes each proxy's file and asserts every line is
**byte-identical** between upstream Envoy v1.33.0 and envoy-rust (whole-line
`==`, the same `http1_access_log_byte_exact` driver as fixtures
0040/0046/0053/0056).

## What this proves (`UH` is byte-exact cross-proxy)

On a NO_FALLBACK subset-miss, both proxies return a deterministic 503
`no healthy upstream`. Envoy v1.33.0 renders `%RESPONSE_FLAGS%` = `UH` on this
path (state-0 recon: live Envoy emits
`{"method":"GET","proto":"HTTP/1.1","rc":503,"rcd":"no_healthy_upstream","rf":"UH"}`).
envoy-rust now DERIVES `%RESPONSE_FLAGS%` = `UH` from `%RESPONSE_CODE_DETAILS%` =
`no_healthy_upstream` at the H1 record-build site (`hcm.rs:1232`; was the
no-flags sentinel `"-"`). `no_healthy_upstream` is set at EXACTLY one
per-request site — `hcm.rs:1001`, the `pick()->None` no-healthy synth-503 arm
(phase 45, ADR-0102) — so the derive is provably 1:1 with `UH`. The 503
status/body/headers/`%RESPONSE_CODE_DETAILS%` are UNCHANGED — the flag is purely
additive (one arm added to the derive; the phase-48 `route_not_found => "NR"`
arm is preserved verbatim → fixture 0056 stays byte-identical).

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
    rf: "%RESPONSE_FLAGS%"
    method: "%REQ(:METHOD)%"
    proto: "%PROTOCOL%"
```

| key      | operator                 | rendered value          |
|----------|--------------------------|-------------------------|
| `method` | `%REQ(:METHOD)%`         | `GET`                   |
| `proto`  | `%PROTOCOL%`             | `HTTP/1.1`              |
| `rc`     | `%RESPONSE_CODE%`        | `503` (json NUMBER)     |
| `rcd`    | `%RESPONSE_CODE_DETAILS%`| `no_healthy_upstream`   |
| `rf`     | `%RESPONSE_FLAGS%`       | `UH`                    |

`%RESPONSE_CODE%` renders the bare json NUMBER `503` (not a quoted string) —
precedent fixture `0047-accesslog-json-nested`. Keys sort by UTF-8 byte order
(ADR-0094 §A): method, proto, rc, rcd, rf — the json_format AUTHORING order
{ rc, rcd, rf, method, proto } is irrelevant; compact separators + ONE trailing
`\n` (ADR-0092 §E).

## Probe

| # | request                    | emitted JSON object (byte-identical on both sides) |
|---|----------------------------|----------------------------------------------------|
| 1 | `GET /` (no extra headers) | see below                                          |

```
{"method":"GET","proto":"HTTP/1.1","rc":503,"rcd":"no_healthy_upstream","rf":"UH"}
```

A single probe — the no-healthy path is a single `pick()->None` arm (unlike
phase 48's two no-route `synth_404` arms, which needed two probes in 0056).

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
line logs no `%UPSTREAM_HOST%`, so the endpoint address never appears. This is
the same trigger topology as fixture 0053 (the `rcd` sibling).

## Per-side divergences

| Side       | bind address | admin block | access-log path                          |
|------------|--------------|-------------|------------------------------------------|
| envoy      | `0.0.0.0`    | yes (port 0)| `/tmp/0057-envoy-mount/access.log`       |
| envoy-rust | `127.0.0.1`  | omitted     | `/tmp/0057-envoy-rust-mount/access.log`  |

There are NO upstream-specific deltas here: this fixture spawns no backend and
never sends an upstream-bound request. The cluster + route + subset config +
`metadata_match` + `json_format` are BYTE-IDENTICAL across the two files (only
the documented per-side deltas differ). The parent directory is bind-mounted
from the host into the Envoy container so the harness can read the access.log
file after the request completes (same wiring as fixtures 0012 / 0040 / 0053 /
0056).

## Driver

`kind: http1_access_log_byte_exact` (same driver as fixtures
0040/0046/0053/0056) — drives the probe, scrapes both files, asserts the scraped
line count equals `probes.len()`, and calls
`access_log::assert_access_log_lines_byte_identical`. No new harness code; no
backend spawn (no `{{HTTP1_BACKEND_PORT}}` marker).

## Cross-references

- ADR: ADR-0106 (phase-49 pick + scope — witness the SECOND non-`-`
  `%RESPONSE_FLAGS%` value `UH` byte-exact by extending the phase-48 derive at
  `hcm.rs:1232` to map `Some("no_healthy_upstream") => "UH"`).
- Related fixtures: 0053 (`%RESPONSE_CODE_DETAILS%` = `no_healthy_upstream`, the
  `rcd` sibling on this exact 503 path, whose trigger topology this reuses), 0056
  (`%RESPONSE_FLAGS%` = `NR`, the phase-48 sibling on the OTHER witnessed failure
  path), 0038 (the NO_FALLBACK subset cluster + `/nope` subset-miss 503 trigger
  this reuses), 0047 (the `%RESPONSE_CODE%` bare-json-NUMBER precedent), 0012
  (default per-token access-log baseline).
- Deferred: the H2 no-healthy path (M45-1: H2 record-build site
  `envoy-http2/src/hcm.rs:948` also hard-codes `"-"`; no H2 access-log
  differential driver) and the other non-`-` flags `UO`/`UF`/`DC`/`URX` (M45-2,
  non-deterministic connect/overflow/timeout surfaces — `UH` now moves OUT of
  that unwitnessed set).
