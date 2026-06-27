# Fixture 0049 — access-log `%ROUTE_NAME%` operator (byte-exact)

The first fixture exercising envoy-rust's `%ROUTE_NAME%` access-log command
operator (phase 41, ADR-0098). Mirrors fixture `0048` (an H1 `direct_response`
listener with a `file` access-logger emitting ONE JSON object per request) but
the route is **named** (`name: myroute`) and the `json_format` carries
`%ROUTE_NAME%`. The harness scrapes each proxy's file and asserts every line is
**byte-identical** between upstream Envoy v1.33.0 and envoy-rust (whole-line
`==`, same driver as fixtures 0040/0046/0047/0048).

## What this proves (`%ROUTE_NAME%` is byte-exact cross-proxy)

ADR-0098 §C: `%ROUTE_NAME%` renders the matched route's config `name` — an
`Option<String>` IDENTICAL in shape to `%UPSTREAM_HOST%` (present → the name;
absent/unnamed → the `-` sentinel in a multi-segment leaf, json `null` in a
single-operator-typed leaf).

- **single-operator-typed leaf → quoted string.** A value that is EXACTLY one
  `%ROUTE_NAME%` routes through the typed encoder:
  - `single_rn: "%ROUTE_NAME%"`   → `"myroute"`   (NAMED route → quoted)
- **multi-segment leaf → string with the name spliced in.**
  - `rn: "r=%ROUTE_NAME%"`        → `"r=myroute"`
- Present operators are normal (`method`/`proto`).
- Keys sort by UTF-8 byte order (ADR-0094 §A); compact separators + ONE trailing
  `\n` (ADR-0092 §E).

## The `json_format` map (NAMED route)

```yaml
route_config:
  virtual_hosts:
    - routes:
        - name: myroute            # the NAMED route — %ROUTE_NAME% reads this
          match: { prefix: "/" }
          direct_response: { status: 200, body: { inline_string: "ok\n" } }
log_format:
  json_format:
    rn: "r=%ROUTE_NAME%"
    single_rn: "%ROUTE_NAME%"
    method: "%REQ(:METHOD)%"
    proto: "%PROTOCOL%"
```

Every operator is deterministic given a fixed request + the statically-named
`direct_response` route (no `%START_TIME%`/`%DURATION%`/`%REQ(X-REQUEST-ID)%`),
so the strongest assertion — every byte of the emitted line identical across the
two proxies — applies, with ZERO `{{BACKEND_IP}}` complexity (no upstream).

## Probe

| # | request                    | emitted JSON object (byte-identical on both sides) |
|---|----------------------------|----------------------------------------------------|
| 1 | `GET /` (no extra headers) | see below |

```
{"method":"GET","proto":"HTTP/1.1","rn":"r=myroute","single_rn":"myroute"}
```

This is the ADR-0098 §C authoritative line, **captured live** from
`envoyproxy/envoy:v1.33.0` (phase-41 T6 recon).

## Unnamed-route control (the `None`-absent witness)

An UNNAMED route (no `name:` key) renders `%ROUTE_NAME%` as the `-` sentinel
(multi-segment leaf) / json `null` (single-op leaf) — the `Option<String>`
`None` arm. This absent path is exercised by the in-process `envoy-accesslog`
backstop (`route_name_*` unit tests in `command_operator.rs`/`json_format.rs`)
and the HCM plumbing tests (`hcm_h1_sets_route_name_from_matched_route` /
`hcm_h2_sets_route_name_from_matched_route`, named × unnamed). All fixtures
`0001`-`0048` carry NO named route and NO `%ROUTE_NAME%`, so they stay
byte-identical (the default-absent regression proof).

## Per-side divergences

| Side | bind address | admin block | access-log path                         | `generate_request_id` |
|------|--------------|-------------|-----------------------------------------|-----------------------|
| envoy | `0.0.0.0`   | yes (port 0)| `/tmp/0049-envoy-mount/access.log`      | `false` (load-bearing) |
| envoy-rust | `127.0.0.1` | omitted | `/tmp/0049-envoy-rust-mount/access.log` | omitted (never injects) |

The parent directory is bind-mounted from the host into the Envoy container so
the harness can read the access.log file after the request completes (same
wiring as fixtures 0012 / 0040 / 0046 / 0047 / 0048).

## Driver

`kind: http1_access_log_byte_exact` (same driver as fixtures 0040/0046/0047/0048)
— drives the probe sequence, scrapes both files, asserts the scraped line count
equals `probes.len()`, and calls
`access_log::assert_access_log_lines_byte_identical`.

## Cross-references

- ADR: ADR-0097 (state-1 brainstorm), ADR-0098 (state-2 PLAN — the §6.2 PIVOT:
  the original `%REQ_WITHOUT_QUERY%` pick was VOID at v1.33.0; `%ROUTE_NAME%` is
  the recon-confirmed deterministic replacement — the matched route's config
  `name`, an `Option<String>` rendered exactly like `%UPSTREAM_HOST%`).
- Related fixtures: 0048 (the `omit_empty_values` mirror this is shaped after),
  0047 (recursive `json_format`), 0046 (flat `json_format`), 0040
  (command-operator `text_format_source`), 0012 (default per-token access-log
  baseline), 0007 (H1 direct_response baseline).
