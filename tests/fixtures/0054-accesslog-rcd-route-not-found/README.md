# Fixture 0054 — access-log `%RESPONSE_CODE_DETAILS%` failure path (`route_not_found`, byte-exact)

The **SECOND failure-path `%RESPONSE_CODE_DETAILS%` witness** (phase 46,
ADR-0103). Fixtures 0050/0051/0052 witnessed `%RESPONSE_CODE_DETAILS%` on the
SUCCESS path (`direct_response` / `via_upstream`); fixture 0053 witnessed the
first failure detail (`no_healthy_upstream`). This fixture witnesses the
route-miss FAILURE detail — `route_not_found` — BYTE-EXACT on the 404 path.

The harness scrapes each proxy's file and asserts every line is
**byte-identical** between upstream Envoy v1.33.0 and envoy-rust (whole-line
`==`, the same `http1_access_log_byte_exact` driver as fixtures
0040/0046/0047/0048/0049/0050/0051/0052/0053).

## What this proves (`route_not_found` is byte-exact cross-proxy)

On a request that matches the virtual host (`domains: ["*"]`) but matches NO
route, both proxies return a deterministic 404. Envoy v1.33.0 renders
`%RESPONSE_CODE_DETAILS%` = `route_not_found` on this path (state-1 recon: live
Envoy emits `{"rc":404,"rcd":"route_not_found","rf":"NR"}`). envoy-rust now SETS
`Some("route_not_found")` at its H1 no-matching-route `synth_404` arm
(`hcm.rs:1553`) — the access-log record is built unconditionally below the
writer-arm match, and the detail was left `None` (rendering `rcd:null`) until
phase 46 set it at the no-matching-route arm (the arm preceded by the
`"request rejected: no matching route"` warn). The 404 status/body/headers/flags
are UNCHANGED — the detail is purely additive.

The assertion is **pure cross-proxy equality** — there is NO static expected
literal. The route-miss synth-404 is deterministic on both sides, so the
byte-exact driver covers the line.

## The `json_format` map (route-miss 404 route table)

```yaml
route_config:
  virtual_hosts:
    - domains: ["*"]
      routes:
        - match: { prefix: "/specific" }
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
precedent fixtures `0047-accesslog-json-nested` / `0053`. Keys sort by UTF-8
byte order (ADR-0094 §A): method, proto, rc, rcd; the `json_format` AUTHORING
order `{ rc, rcd, method, proto }` is IRRELEVANT (the renderer re-sorts at emit
time). Compact separators + ONE trailing `\n` (ADR-0092 §E).

## Probe

| # | request                        | emitted JSON object (byte-identical on both sides) |
|---|--------------------------------|----------------------------------------------------|
| 1 | `GET /nomatch` (no extra hdrs) | see below                                          |

```
{"method":"GET","proto":"HTTP/1.1","rc":404,"rcd":"route_not_found"}
```

## The route-miss 404 trigger (a single `/specific` route + a `/nomatch` probe)

The vhost `domains: ["*"]` matches every request (no host-miss), and carries a
SINGLE route `match: { prefix: "/specific" }` → `direct_response: { status: 200,
... }`. The probe `GET /nomatch` matches the vhost but does NOT match the
`/specific` prefix → the route-walk yields no matching route → `synth_404` with
the new `route_not_found` detail at ROUTING time. `clusters: []` — there is no
upstream; no backend spawns and no `{{BACKEND_IP}}` machinery is needed (the
`0050` `direct_response` template shape).

Because the vhost is `domains: ["*"]`, the request NEVER hits the
"no matching virtual_host" arm (`hcm.rs:1535`) — the host-miss 404 detail is
deferred (M46-1) and stays `None`. ONLY the no-matching-route arm
(`hcm.rs:1553`) is exercised here.

## Per-side divergences

| Side       | bind address | admin block | access-log path                          | `generate_request_id` |
|------------|--------------|-------------|------------------------------------------|-----------------------|
| envoy      | `0.0.0.0`    | yes (port 0)| `/tmp/0054-envoy-mount/access.log`       | `false` (load-bearing) |
| envoy-rust | `127.0.0.1`  | omitted     | `/tmp/0054-envoy-rust-mount/access.log`  | omitted (never injects) |

The route table + vhost + `json_format` are BYTE-IDENTICAL across the two files
(only the documented per-side deltas differ — the `0050` `direct_response`
template). The parent directory is bind-mounted from the host into the Envoy
container so the harness can read the access.log file after the request
completes (same wiring as fixtures 0012 / 0040 / 0050 / 0051 / 0052 / 0053).

## Driver

`kind: http1_access_log_byte_exact` (same driver as fixtures
0040/0046/0047/0048/0049/0050/0051/0052/0053) — drives the probe sequence,
scrapes both files, asserts the scraped line count equals `probes.len()`, and
calls `access_log::assert_access_log_lines_byte_identical`. No new harness code;
no backend spawn (no `{{HTTP1_BACKEND_PORT}}` marker).

## Cross-references

- ADR: ADR-0103 (state-1 brainstorm + state-2 PLAN — the failure-path
  `%RESPONSE_CODE_DETAILS%` = `route_not_found` pick: SET the detail at the H1
  no-matching-route `synth_404` arm + witness it byte-exact via a single
  `/specific` route + a `/nomatch` probe).
- Related fixtures: 0053 (`no_healthy_upstream`, the FIRST failure-path
  `%RESPONSE_CODE_DETAILS%` witness this extends), 0052 (`%UPSTREAM_HOST%`),
  0051 (`%UPSTREAM_CLUSTER%` + `%RESPONSE_CODE_DETAILS%` success path), 0050
  (the `direct_response` access-log template this is shaped after), 0047 (the
  `%RESPONSE_CODE%` bare-json-NUMBER precedent), 0012 (default per-token
  access-log baseline), 0007 (H1 `direct_response` baseline).
- Deferred: the host-miss 404 detail (M46-1: the "no matching virtual_host" arm
  at `hcm.rs:1535` stays `None`; the `domains: ["*"]` vhost never exercises it);
  the connect-failure / overflow failure details (M45-2); the H2 path (M45-1: no
  H2 access-log differential driver).
