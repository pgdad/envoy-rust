# Fixture 0050 — access-log `%RESPONSE_CODE_DETAILS%` operator (byte-exact)

The first fixture exercising envoy-rust's `%RESPONSE_CODE_DETAILS%` access-log
command operator (phase 42, ADR-0099). Mirrors fixture `0049` (an H1
`direct_response` listener with a `file` access-logger emitting ONE JSON object
per request) but the `json_format` carries `%RESPONSE_CODE_DETAILS%`. The
harness scrapes each proxy's file and asserts every line is **byte-identical**
between upstream Envoy v1.33.0 and envoy-rust (whole-line `==`, same driver as
fixtures 0040/0046/0047/0048/0049).

## What this proves (`%RESPONSE_CODE_DETAILS%` is byte-exact cross-proxy)

ADR-0099: `%RESPONSE_CODE_DETAILS%` renders Envoy's response-code-details
string — an `Option<String>` IDENTICAL in shape to `%ROUTE_NAME%` /
`%UPSTREAM_HOST%` (present → the string; absent → the `-` sentinel in a
multi-segment leaf, json `null` in a single-operator-typed leaf). For a
`direct_response` route the value is the literal `direct_response`.

- **single-operator-typed leaf → quoted string.** A value that is EXACTLY one
  `%RESPONSE_CODE_DETAILS%` routes through the typed encoder:
  - `single_rcd: "%RESPONSE_CODE_DETAILS%"`   → `"direct_response"` (quoted)
- **multi-segment leaf → string with the value spliced in.**
  - `rcd: "d=%RESPONSE_CODE_DETAILS%"`        → `"d=direct_response"`
- Present operators are normal (`method`/`proto`).
- Keys sort by UTF-8 byte order (ADR-0094 §A); compact separators + ONE trailing
  `\n` (ADR-0092 §E).

## The `json_format` map (`direct_response` route)

```yaml
route_config:
  virtual_hosts:
    - routes:
        - match: { prefix: "/" }
          direct_response: { status: 200, body: { inline_string: "ok\n" } }
log_format:
  json_format:
    rcd: "d=%RESPONSE_CODE_DETAILS%"
    single_rcd: "%RESPONSE_CODE_DETAILS%"
    method: "%REQ(:METHOD)%"
    proto: "%PROTOCOL%"
```

Every operator is deterministic given a fixed request + the static
`direct_response` route (no `%START_TIME%`/`%DURATION%`/`%REQ(X-REQUEST-ID)%`),
so the strongest assertion — every byte of the emitted line identical across the
two proxies — applies, with ZERO `{{BACKEND_IP}}` complexity (no upstream).

## Probe

| # | request                    | emitted JSON object (byte-identical on both sides) |
|---|----------------------------|----------------------------------------------------|
| 1 | `GET /` (no extra headers) | see below |

```
{"method":"GET","proto":"HTTP/1.1","rcd":"d=direct_response","single_rcd":"direct_response"}
```

This is the ADR-0099 authoritative line, **captured live** from
`envoyproxy/envoy:v1.33.0` (phase-42 T6 recon).

## Differential value is `direct_response` (no backend in this family)

The access-log fixture family routes via `direct_response` — it has no upstream
backend — so the witnessed `%RESPONSE_CODE_DETAILS%` value here is
`direct_response`. The proxy-success value `via_upstream` (rendered when a
request is routed to and answered by an upstream) is implemented but NOT
witnessed by this fixture. Error/filter synthesised paths render the
`Option<String>` `None` arm (the `-` sentinel / json `null`).

## Absent witness (the `None` arm)

The absent path (`%RESPONSE_CODE_DETAILS%` → `-` sentinel / json `null`) is
exercised by the in-process `envoy-accesslog` backstop and the HCM plumbing
unit tests. All fixtures `0001`-`0049` carry NO `%RESPONSE_CODE_DETAILS%`, so
they stay byte-identical (the default-absent regression proof).

## Per-side divergences

| Side | bind address | admin block | access-log path                         | `generate_request_id` |
|------|--------------|-------------|-----------------------------------------|-----------------------|
| envoy | `0.0.0.0`   | yes (port 0)| `/tmp/0050-envoy-mount/access.log`      | `false` (load-bearing) |
| envoy-rust | `127.0.0.1` | omitted | `/tmp/0050-envoy-rust-mount/access.log` | omitted (never injects) |

The parent directory is bind-mounted from the host into the Envoy container so
the harness can read the access.log file after the request completes (same
wiring as fixtures 0012 / 0040 / 0046 / 0047 / 0048 / 0049).

## Driver

`kind: http1_access_log_byte_exact` (same driver as fixtures
0040/0046/0047/0048/0049) — drives the probe sequence, scrapes both files,
asserts the scraped line count equals `probes.len()`, and calls
`access_log::assert_access_log_lines_byte_identical`.

## Cross-references

- ADR: ADR-0099 (state-1 brainstorm + state-2 PLAN — the `%RESPONSE_CODE_DETAILS%`
  pick: it renders Envoy's response-code-details string, an `Option<String>`
  rendered exactly like `%ROUTE_NAME%`).
- Related fixtures: 0049 (the `%ROUTE_NAME%` mirror this is shaped after), 0048
  (`omit_empty_values`), 0047 (recursive `json_format`), 0046 (flat
  `json_format`), 0040 (command-operator `text_format_source`), 0012 (default
  per-token access-log baseline), 0007 (H1 direct_response baseline).
