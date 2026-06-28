# Fixture 0056 — access-log `%RESPONSE_FLAGS%` no-route failure path (`NR`, byte-exact)

The **FIRST non-`-` `%RESPONSE_FLAGS%` witness** (phase 48, ADR-0105). Every
prior `%RESPONSE_FLAGS%`-logging fixture (0012 / 0040 / 0046) drives a
happy-path 200 where the flag renders Envoy's no-flags sentinel `"-"`. This
fixture witnesses the no-route 404 path, where Envoy renders `NR` (NoRoute) —
BYTE-EXACT cross-proxy — on BOTH the route-miss and host-miss `synth_404` arms.

The harness scrapes each proxy's file and asserts every line is
**byte-identical** between upstream Envoy v1.33.0 and envoy-rust (whole-line
`==`, the same `http1_access_log_byte_exact` driver as fixtures
0040/0046/0047/0048/0049/0050/0051/0052/0053/0054/0055).

## What this proves (`NR` is byte-exact cross-proxy on both no-route arms)

On a request that yields a no-route 404 — either because its `Host`
(`:authority`) matches NO virtual_host `domains` entry (host-miss) or because no
route in the matched vhost matches the path (route-miss) — both proxies return a
deterministic 404 and render `%RESPONSE_FLAGS%` = `NR` (state-1 recon: live
Envoy v1.33.0 emits `{"rc":404,"rcd":"route_not_found","rf":"NR"}` on both
arms). envoy-rust now **derives** `NR` at its H1 access-log record-build site
(`hcm.rs:1225`): the `response_flags` field, previously hard-coded `"-"`, now
renders `NR` when `%RESPONSE_CODE_DETAILS%` is `route_not_found`. That detail is
set (via the writer-arm at `hcm.rs:866`) ONLY at the two no-route `synth_404`
arms — host-miss (`hcm.rs:1536`) + route-miss (`hcm.rs:1555`) — so the derived
`NR` is 1:1 with Envoy's NoRoute flag. The 404 status/body/headers and
`%RESPONSE_CODE_DETAILS%` are UNCHANGED — the flag is purely additive.

The assertion is **pure cross-proxy equality** — there is NO static expected
literal. Both no-route synth-404 arms are deterministic on both sides, so the
byte-exact driver covers each line.

## The `json_format` map (no-route 404 route table)

```yaml
route_config:
  virtual_hosts:
    - domains: ["match.test"]
      routes:
        - match: { prefix: "/specific" }
          direct_response: { status: 200, body: { inline_string: "ok\n" } }
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
| `rc`     | `%RESPONSE_CODE%`        | `404` (json NUMBER)     |
| `rcd`    | `%RESPONSE_CODE_DETAILS%`| `route_not_found`       |
| `rf`     | `%RESPONSE_FLAGS%`       | `NR`                    |

`%RESPONSE_CODE%` renders the bare json NUMBER `404` (not a quoted string) —
precedent fixtures `0047-accesslog-json-nested` / `0053` / `0054` / `0055`. Keys
sort by UTF-8 byte order (ADR-0094 §A): method, proto, rc, rcd, rf; the
`json_format` AUTHORING order `{ rc, rcd, rf, method, proto }` is IRRELEVANT
(the renderer re-sorts at emit time). Compact separators + ONE trailing `\n`
(ADR-0092 §E).

## Probes

| # | request                                     | arm                | emitted JSON object (byte-identical on both sides) |
|---|---------------------------------------------|--------------------|----------------------------------------------------|
| 1 | `GET /nomatch` with `Host: match.test`      | route-miss `:1555` | see below                                          |
| 2 | `GET /specific` with `Host: nomatch.test`   | host-miss `:1536`  | see below                                          |

```
{"method":"GET","proto":"HTTP/1.1","rc":404,"rcd":"route_not_found","rf":"NR"}
```

Both probes emit the SAME line (the flag, detail, and code are identical across
the two no-route arms); the byte-exact driver asserts each of the two scraped
lines is identical cross-proxy.

## The two no-route 404 triggers (a `domains: ["match.test"]` vhost)

The route table has a SINGLE vhost with a NON-wildcard `domains: ["match.test"]`
and a single route `match: { prefix: "/specific" }` → `direct_response: {
status: 200, ... }`. The non-wildcard `domains` is load-bearing: it lets the
host-miss probe miss the vhost.

- **Probe 1 (route-miss, `hcm.rs:1555`):** `GET /nomatch` with `Host:
  match.test` MATCHES the vhost but matches NO route → the no-matching-route
  `synth_404`.
- **Probe 2 (host-miss, `hcm.rs:1536`):** `GET /specific` with `Host:
  nomatch.test` matches NO `domains` entry (`match.test`) → the
  no-matching-virtual_host `synth_404` (the route-walk never runs, so the
  `/specific` route is irrelevant here).

`clusters: []` — there is no upstream; no backend spawns and no `{{BACKEND_IP}}`
machinery is needed (the `0050`/`0054`/`0055` `direct_response` template shape).

The probe Host MUST be NON-EMPTY: an empty/missing Host trips the codec's
`synth_400` guard BEFORE the vhost-walk (a different path — wrong arm). A
non-matching NON-EMPTY Host reaches the vhost-walk and the host-miss arm.

This fixture exercises BOTH route-walk 404 arms in one run — the SIBLING
fixtures 0054 (route-miss) and 0055 (host-miss) each witnessed
`%RESPONSE_CODE_DETAILS%` = `route_not_found` on one arm; this fixture extends
the same two arms to the `%RESPONSE_FLAGS%` = `NR` witness.

## Per-side divergences

| Side       | bind address | admin block | access-log path                          | `generate_request_id` |
|------------|--------------|-------------|------------------------------------------|-----------------------|
| envoy      | `0.0.0.0`    | yes (port 0)| `/tmp/0056-envoy-mount/access.log`       | `false` (load-bearing) |
| envoy-rust | `127.0.0.1`  | omitted     | `/tmp/0056-envoy-rust-mount/access.log`  | omitted (never injects) |

The route table + vhost + `json_format` are BYTE-IDENTICAL across the two files
(only the documented per-side deltas differ — the `0050`/`0054`/`0055`
`direct_response` template). The parent directory is bind-mounted from the host
into the Envoy container so the harness can read the access.log file after the
requests complete (same wiring as fixtures 0012 / 0040 / 0050 / 0054 / 0055).

## `0001`-`0055` byte-preservation

The derive at `hcm.rs:1225` changes the rendered `%RESPONSE_FLAGS%` value ONLY
on the no-route 404 path (where it was `"-"`, now `NR`). The existing
`%RESPONSE_FLAGS%`-logging fixtures are exactly `0012` / `0040` / `0046` — all
happy-path 200s, where the flag stays `"-"`. The no-route 404 fixtures `0054` /
`0055` log only `rc` / `rcd` / `method` / `proto`, NOT `%RESPONSE_FLAGS%`. So no
existing fixture both hits a no-route 404 AND logs the flag → all `0001`-`0055`
stay byte-identical; only the new `0056` observes the changed value.

## Driver

`kind: http1_access_log_byte_exact` (same driver as fixtures
0040/0046/0047/0048/0049/0050/0051/0052/0053/0054/0055) — drives the probe
sequence, scrapes both files, asserts the scraped line count equals
`probes.len()` (here 2), and calls
`access_log::assert_access_log_lines_byte_identical`. The driver passes each
probe `host:` verbatim → `drive_http1` writes the `Host:` header literally. No
new harness code; no backend spawn (no `{{HTTP1_BACKEND_PORT}}` marker).

## Cross-references

- ADR: ADR-0105 (state-1 brainstorm + state-2 PLAN — the no-route failure-path
  `%RESPONSE_FLAGS%` = `NR` pick: DERIVE the flag from `route_not_found` at the
  H1 record-build site `hcm.rs:1225` + witness it byte-exact via a
  `domains: ["match.test"]` vhost probed twice — route-miss + host-miss).
- Related fixtures: 0054 (`route_not_found` on the route-miss arm), 0055
  (`route_not_found` on the host-miss arm — the two arms this fixture extends to
  the `NR` flag), 0046 / 0040 / 0012 (the happy-path `%RESPONSE_FLAGS%` = `"-"`
  baselines), 0047 (the `%RESPONSE_CODE%` bare-json-NUMBER precedent), 0050 (the
  `direct_response` access-log template this is shaped after).
- Deferred: the H2 no-route `%RESPONSE_FLAGS%` path (M45-1: no H2 access-log
  differential driver; `envoy-http2/src/hcm.rs` also hard-codes `"-"`); the
  other non-`-` flags `UH` / `UF` / `UO` / `DC` / `URX` (M45-2, which ride
  non-deterministic connect/overflow/timeout surfaces).
