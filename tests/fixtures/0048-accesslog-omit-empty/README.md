# Fixture 0048 — access-log `omit_empty_values` sentinel swap (byte-exact)

The first fixture exercising envoy-rust's `omit_empty_values` knob (phase 40,
ADR-0096). Mirrors fixture `0047` (recursive `json_format`) but the
`log_format` carries `omit_empty_values: true`. An H1 `direct_response`
listener with a `file` access-logger emits ONE JSON object per request; the
harness scrapes each proxy's file and asserts every line is **byte-identical**
between upstream Envoy v1.33.0 and envoy-rust (whole-line `==`, same driver as
fixtures 0040/0046/0047).

## What this proves (the sentinel swap is byte-exact cross-proxy)

ADR-0096's central correction: `omit_empty_values` is NOT a key-drop pass — it
SWAPS the absent-operator `-` sentinel for the EMPTY STRING `""` in the
command-operator MULTI-SEGMENT render.

- **§A — NO key is dropped.** All five keys (`method`/`proto`/`single_up`/`up`/
  `xff`) always emit. Keys sort by UTF-8 byte order (ADR-0094 §A).
- **§B — the swap on MULTI-SEGMENT leaves.** An absent operator embedded in a
  multi-segment/literal-prefixed value renders as `""`:
  - `up:  "up=%UPSTREAM_HOST%"`       → `"up="`   (was `"up=-"`)
  - `xff: "x=%REQ(X-FORWARDED-FOR)%"` → `"x="`    (was `"x=-"`)
- **§C — the single-operator-typed carve-out.** A value that is EXACTLY one
  operator routes through the typed encoder and is UNAFFECTED:
  - `single_up: "%UPSTREAM_HOST%"`    → `null`    (NOT `""`, NOT dropped)
- Present operators are normal (`proto`/`method`).
- **Compact separators + ONE trailing `\n`** (§E).

## The `json_format` map (with `omit_empty_values: true`)

```yaml
log_format:
  omit_empty_values: true
  json_format:
    up: "up=%UPSTREAM_HOST%"
    xff: "x=%REQ(X-FORWARDED-FOR)%"
    single_up: "%UPSTREAM_HOST%"
    proto: "%PROTOCOL%"
    method: "%REQ(:METHOD)%"
```

Every operator is deterministic given a fixed request + `direct_response` route
(no `%START_TIME%`/`%DURATION%`/`%REQ(X-REQUEST-ID)%`), so the strongest
assertion — every byte of the emitted line identical across the two proxies —
applies. `%UPSTREAM_HOST%` is absent on a `direct_response` route, keeping the
line byte-identical with ZERO `{{BACKEND_IP}}` complexity.

## Probe

| # | request                    | emitted JSON object (byte-identical on both sides) |
|---|----------------------------|----------------------------------------------------|
| 1 | `GET /` (no extra headers) | see below |

```
{"method":"GET","proto":"HTTP/1.1","single_up":null,"up":"up=","xff":"x="}
```

This is the ADR-0096 §B/§C authoritative line, **captured live** from
`envoyproxy/envoy:v1.33.0` (phase-40 T5 recon).

## Flag-off control (the default-off regression witness)

The SAME `json_format` map with NO `omit_empty_values` keeps the `-` sentinel —
live-captured from `envoyproxy/envoy:v1.33.0`:

```
{"method":"GET","proto":"HTTP/1.1","single_up":null,"up":"up=-","xff":"x=-"}
```

The default-off path is exercised byte-exact across both proxies by fixture
`0047-accesslog-json-nested` (same recursive-`json_format` shape, no flag, still
the `-` sentinel) and all fixtures `0001`-`0047` (which never set the flag). The
in-process `envoy-accesslog` backstop additionally proves the text-format swap,
the recursive (§D) swap, the §C single-op carve-out, and the default-off
round-trip.

## Per-side divergences

| Side | bind address | admin block | access-log path                         | `generate_request_id` |
|------|--------------|-------------|-----------------------------------------|-----------------------|
| envoy | `0.0.0.0`   | yes (port 0)| `/tmp/0048-envoy-mount/access.log`      | `false` (load-bearing) |
| envoy-rust | `127.0.0.1` | omitted | `/tmp/0048-envoy-rust-mount/access.log` | omitted (never injects) |

The parent directory is bind-mounted from the host into the Envoy container so
the harness can read the access.log file after the request completes (same
wiring as fixtures 0012 / 0040 / 0046 / 0047).

## Driver

`kind: http1_access_log_byte_exact` (same driver as fixtures 0040/0046/0047) —
drives the probe sequence, scrapes both files, asserts the scraped line count
equals `probes.len()`, and calls
`access_log::assert_access_log_lines_byte_identical`.

## Cross-references

- ADR: ADR-0095 (state-1 brainstorm — pick `omit_empty_values`), ADR-0096
  (state-2 PLAN — §A–§E empirical recon vs `envoyproxy/envoy:v1.33.0`: the
  sentinel swap, NOT key-drop; both `text_format` + `json_format`; single-op
  `null` carve-out; recursive; no new `ConfigError` variant).
- Related fixtures: 0047 (recursive `json_format` byte-exact — the flag-off
  witness for this exact shape), 0046 (flat `json_format`), 0040
  (command-operator `text_format_source`), 0012 (default per-token access-log
  baseline), 0007 (H1 direct_response baseline).
