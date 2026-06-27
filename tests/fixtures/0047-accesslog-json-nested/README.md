# Fixture 0047 — access-log nested `json_format` encoder (byte-exact)

The first fixture exercising envoy-rust's RECURSIVE `json_format` access-log
output mode (phase 39, ADR-0094). Mirrors fixture `0046` (flat `json_format`)
but the `log_format.json_format` carries a NESTED object (`arequest`) and a
LIST (`blist`) alongside top-level scalars. An H1 `direct_response` listener
with a `file` access-logger emits ONE nested JSON object per request; the
harness scrapes each proxy's file and asserts every line is **byte-identical**
between upstream Envoy v1.33.0 and envoy-rust (whole-line `==`, same driver as
fixtures 0040/0046).

## What this proves (the recursion is byte-exact cross-proxy)

- **Keys SORTED by UTF-8 bytes at EVERY object level** (ADR-0094 §A) — the top
  level (`arequest` < `blist` < `mtop` < `zouter`) AND the nested `arequest`
  object (`aaa` < `method` < `zpath`) are each independently sorted.
- **List order = CONFIG order, NOT sorted** (§B) — `blist` emits its three
  elements in the configured order.
- **At-depth type inference = the phase-38 per-leaf rule** (§C) — a nested
  numeric op → unquoted number (`aaa: "%RESPONSE_CODE%"` → `200`); a string op →
  quoted (`method: "%REQ(:METHOD)%"` → `"GET"`); an absent op in the list →
  `null` (`%UPSTREAM_HOST%` on a `direct_response` route); a literal-prefixed op
  → a quoted string (`mtop: "code-%RESPONSE_CODE%"` → `"code-200"`).
- **Compact separators + ONE trailing `\n`** on the whole top-level object (§E):
  no inter-element / inter-level whitespace or newline.

## The `json_format` map

```yaml
json_format:
  zouter: "%PROTOCOL%"
  arequest:
    method: "%REQ(:METHOD)%"
    zpath: "%REQ(:PATH)%"
    aaa: "%RESPONSE_CODE%"
  blist:
    - "%REQ(:METHOD)%"
    - "%RESPONSE_CODE%"
    - "%UPSTREAM_HOST%"
  mtop: "code-%RESPONSE_CODE%"
```

Every operator is deterministic given a fixed request + `direct_response` route
(no `%START_TIME%`/`%DURATION%`/`%REQ(X-REQUEST-ID)%`), so the strongest
assertion — every byte of the emitted line identical across the two proxies —
applies. `%UPSTREAM_HOST%` is absent (`null`) on a `direct_response` route,
keeping the line byte-identical with ZERO `{{BACKEND_IP}}` complexity. The
richer recursive cases (`bool`/`null` literal leaves §D, empty `{}`/`[]` §F,
depth-3, list-of-objects, escaping at depth) are proven by the in-process
`envoy-accesslog` `json_format` encoder backstop (Task 4).

## Probe

| # | request                    | emitted JSON object (byte-identical on both sides) |
|---|----------------------------|----------------------------------------------------|
| 1 | `GET /` (no extra headers) | see below |

```
{"arequest":{"aaa":200,"method":"GET","zpath":"/"},"blist":["GET",200,null],"mtop":"code-200","zouter":"HTTP/1.1"}
```

This is the ADR-0094 §H authoritative line, captured live from
`envoyproxy/envoy:v1.33.0` (CASE-1).

## Per-side divergences

| Side | bind address | admin block | access-log path                         | `generate_request_id` |
|------|--------------|-------------|-----------------------------------------|-----------------------|
| envoy | `0.0.0.0`   | yes (port 0)| `/tmp/0047-envoy-mount/access.log`      | `false` (load-bearing) |
| envoy-rust | `127.0.0.1` | omitted | `/tmp/0047-envoy-rust-mount/access.log` | omitted (never injects) |

The parent directory is bind-mounted from the host into the Envoy container so
the harness can read the access.log file after the request completes (same
wiring as fixtures 0012 / 0040 / 0046).

## Driver

`kind: http1_access_log_byte_exact` (same driver as fixtures 0040/0046) —
drives the probe sequence, scrapes both files, asserts the scraped line count
equals `probes.len()`, and calls
`access_log::assert_access_log_lines_byte_identical`.

## Cross-references

- ADR: ADR-0093 (state-1 brainstorm — pick nested `json_format`), ADR-0094
  (state-2 PLAN — §A–§H empirical recon vs `envoyproxy/envoy:v1.33.0`: per-level
  sorting, list-order preservation, at-depth type inference, native-typed
  `bool`/`null` leaves §D, compact separators + one trailing `\n`, byte-exact §H
  line; CF-39-1 numeric literal leaves deferred).
- Related fixtures: 0046 (flat `json_format` byte-exact — the depth-1 degenerate
  case), 0040 (command-operator `text_format_source` byte-exact), 0012 (default
  per-token access-log baseline), 0007 (H1 direct_response baseline).
