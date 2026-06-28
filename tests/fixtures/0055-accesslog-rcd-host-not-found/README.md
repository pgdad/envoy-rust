# Fixture 0055 — access-log `%RESPONSE_CODE_DETAILS%` host-miss failure path (`route_not_found`, byte-exact)

The **THIRD failure-path `%RESPONSE_CODE_DETAILS%` witness** (phase 47,
ADR-0104). Fixtures 0050/0051/0052 witnessed `%RESPONSE_CODE_DETAILS%` on the
SUCCESS path (`direct_response` / `via_upstream`); fixture 0053 witnessed the
first failure detail (`no_healthy_upstream`); fixture 0054 witnessed the
route-miss FAILURE detail (`route_not_found`). This fixture witnesses the
HOST-miss (no-matching-virtual_host) FAILURE detail — `route_not_found` —
BYTE-EXACT on the 404 path. It **CONSUMES carry-forward M46-1**.

The harness scrapes each proxy's file and asserts every line is
**byte-identical** between upstream Envoy v1.33.0 and envoy-rust (whole-line
`==`, the same `http1_access_log_byte_exact` driver as fixtures
0040/0046/0047/0048/0049/0050/0051/0052/0053/0054).

## What this proves (`route_not_found` is byte-exact cross-proxy on the host-miss path)

On a request whose `Host` (`:authority`) matches NO virtual_host `domains`
entry, both proxies return a deterministic 404. Envoy v1.33.0 renders
`%RESPONSE_CODE_DETAILS%` = `route_not_found` on this path (state-1 recon: live
Envoy emits `{"rc":404,"rcd":"route_not_found","rf":"NR"}`). envoy-rust now SETS
`Some("route_not_found")` at its H1 no-matching-virtual_host `synth_404` arm
(`hcm.rs:1535`) — the access-log record is built unconditionally below the
writer-arm match, and the host-miss detail was left `None` (rendering
`rcd:null`) until phase 47 set it at the no-matching-virtual_host arm (the arm
preceded by the `"request rejected: no matching virtual_host"` warn). The 404
status/body/headers/flags are UNCHANGED — the detail is purely additive. Both
route-walk 404 arms (host-miss `:1535` + route-miss `:1553`) now carry
`route_not_found`.

The assertion is **pure cross-proxy equality** — there is NO static expected
literal. The host-miss synth-404 is deterministic on both sides, so the
byte-exact driver covers the line.

## The `json_format` map (host-miss 404 route table)

```yaml
route_config:
  virtual_hosts:
    - domains: ["match.test"]
      routes:
        - match: { prefix: "/" }
          direct_response: { status: 200, body: { inline_string: "ok\n" } }
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
| `rc`     | `%RESPONSE_CODE%`        | `404` (json NUMBER)     |
| `rcd`    | `%RESPONSE_CODE_DETAILS%`| `route_not_found`       |

`%RESPONSE_CODE%` renders the bare json NUMBER `404` (not a quoted string) —
precedent fixtures `0047-accesslog-json-nested` / `0053` / `0054`. Keys sort by
UTF-8 byte order (ADR-0094 §A): method, proto, rc, rcd; the `json_format`
AUTHORING order `{ rc, rcd, method, proto }` is IRRELEVANT (the renderer
re-sorts at emit time). Compact separators + ONE trailing `\n` (ADR-0092 §E).

## Probe

| # | request                              | emitted JSON object (byte-identical on both sides) |
|---|--------------------------------------|----------------------------------------------------|
| 1 | `GET /` with `Host: nomatch.test`    | see below                                          |

```
{"method":"GET","proto":"HTTP/1.1","rc":404,"rcd":"route_not_found"}
```

## The host-miss 404 trigger (a `domains: ["match.test"]` vhost + a `Host: nomatch.test` probe)

The route table has a SINGLE vhost with a NON-wildcard `domains: ["match.test"]`
and a catch-all route `match: { prefix: "/" }` → `direct_response: { status:
200, ... }`. The probe `GET /` carries `Host: nomatch.test`, which matches NO
`domains` entry (`match.test`) → the vhost-walk yields no matching virtual_host
→ `synth_404` with the new `route_not_found` detail at the host-miss
(`hcm.rs:1535`) arm. The catch-all `/` route is NEVER reached (the vhost-walk
fails first), so its prefix is irrelevant. `clusters: []` — there is no
upstream; no backend spawns and no `{{BACKEND_IP}}` machinery is needed (the
`0050`/`0054` `direct_response` template shape).

The probe Host MUST be NON-EMPTY: an empty/missing Host trips the codec's
`synth_400` guard BEFORE the vhost-walk (a different path — wrong arm). A
non-matching NON-EMPTY Host reaches the vhost-walk and the host-miss arm.

This fixture exercises the host-miss arm (`hcm.rs:1535`); the SIBLING fixture
0054 (`domains: ["*"]` + a `/nomatch` route-miss probe) exercises the
no-matching-route arm (`hcm.rs:1553`). Together they witness `route_not_found`
on BOTH route-walk 404 arms.

## Per-side divergences

| Side       | bind address | admin block | access-log path                          | `generate_request_id` |
|------------|--------------|-------------|------------------------------------------|-----------------------|
| envoy      | `0.0.0.0`    | yes (port 0)| `/tmp/0055-envoy-mount/access.log`       | `false` (load-bearing) |
| envoy-rust | `127.0.0.1`  | omitted     | `/tmp/0055-envoy-rust-mount/access.log`  | omitted (never injects) |

The route table + vhost + `json_format` are BYTE-IDENTICAL across the two files
(only the documented per-side deltas differ — the `0050`/`0054` `direct_response`
template). The parent directory is bind-mounted from the host into the Envoy
container so the harness can read the access.log file after the request
completes (same wiring as fixtures 0012 / 0040 / 0050 / 0051 / 0052 / 0053 / 0054).

## Driver

`kind: http1_access_log_byte_exact` (same driver as fixtures
0040/0046/0047/0048/0049/0050/0051/0052/0053/0054) — drives the probe sequence,
scrapes both files, asserts the scraped line count equals `probes.len()`, and
calls `access_log::assert_access_log_lines_byte_identical`. The driver passes
the probe `host:` verbatim → `drive_http1` writes `Host: nomatch.test` literally.
No new harness code; no backend spawn (no `{{HTTP1_BACKEND_PORT}}` marker).

## Cross-references

- ADR: ADR-0104 (state-1 brainstorm + state-2 PLAN — the host-miss failure-path
  `%RESPONSE_CODE_DETAILS%` = `route_not_found` pick: SET the detail at the H1
  no-matching-virtual_host `synth_404` arm `:1535` + witness it byte-exact via a
  `domains: ["match.test"]` vhost + a `Host: nomatch.test` probe; CONSUMES M46-1).
- Related fixtures: 0054 (`route_not_found` on the route-miss arm, the SIBLING
  this extends to the host-miss arm), 0053 (`no_healthy_upstream`, the first
  failure-path `%RESPONSE_CODE_DETAILS%` witness), 0052 (`%UPSTREAM_HOST%`),
  0051 (`%UPSTREAM_CLUSTER%` + `%RESPONSE_CODE_DETAILS%` success path), 0050
  (the `direct_response` access-log template this is shaped after), 0047 (the
  `%RESPONSE_CODE%` bare-json-NUMBER precedent), 0012 (default per-token
  access-log baseline), 0007 (H1 `direct_response` baseline).
- Consumes: M46-1 (the host-miss 404 detail — the "no matching virtual_host" arm
  at `hcm.rs:1535`, previously `None`, now `Some("route_not_found")`).
- Deferred: the connect-failure / overflow failure details (M45-2); the H2 path
  (M45-1: no H2 access-log differential driver).
