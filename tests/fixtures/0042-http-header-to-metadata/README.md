# Fixture 0042 — `header_to_metadata` + `%DYNAMIC_METADATA%` (byte-exact)

The phase-34 witness fixture for the `header_to_metadata` HTTP filter:
the filter reads an incoming request header (`x-tier`) and writes its
value into the per-request dynamic-metadata store under a configured
namespace and key. A `%DYNAMIC_METADATA(envoy.lb:tier)%` access-log
command-operator reads it back. An H1 `direct_response` listener whose
filter chain is `[header_to_metadata, router]` carries a `file`
access-logger whose `log_format` is a DETERMINISTIC `text_format_source`
built from one header-driven and one absent-namespace
`%DYNAMIC_METADATA%` read. The harness scrapes each proxy's file and
asserts every line is **byte-identical** between upstream Envoy and
envoy-rust (whole-line `==`, the same `Http1AccessLogByteExact` driver
fixtures 0040 and 0041 use).

## The custom format

```
m=%REQ(:METHOD)% tier=%DYNAMIC_METADATA(envoy.lb:tier)% missns=%DYNAMIC_METADATA(envoy.absent:k)%\n
```

Carried in YAML as a double-quoted scalar with a trailing `\n`; both
proxies' `serde_yaml` interpret the escape as a literal newline, so each
record ends with exactly one `\n`.

## §A-locked facts this fixture pins

- **§A1 — `header_to_metadata` wire shape.** The `@type` is
  `type.googleapis.com/envoy.extensions.filters.http.header_to_metadata.v3.Config`
  (the proto message is `Config`). The filter is configured with
  `request_rules` containing one rule for the `x-tier` header.
- **§A2 — default namespace vs explicit namespace.** The default metadata
  namespace for the `header_to_metadata` filter is
  `envoy.filters.http.header_to_metadata`, but this fixture sets
  `metadata_namespace: envoy.lb` explicitly on both `on_header_present`
  and `on_header_missing` to match Envoy's load-balancing metadata
  conventions. The `%DYNAMIC_METADATA(envoy.lb:tier)%` operator reads
  from this explicit namespace.
- **§A3 — present value byte form.** A scalar STRING leaf (e.g. `prod`)
  renders **RAW, UNQUOTED** `prod` (`od -c` → `[ p r o d ]`, never
  `"prod"`). The `on_header_missing.value` MUST be a QUOTED YAML string
  `value: "missing"` — a bare `none`/`null`/`~` parses as YAML null and
  is boot-fatal via the A5d validator. The `value` from
  `on_header_missing` also renders raw-unquoted (i.e. `missing`, not
  `"missing"`).
- **§A4 — absent rendering and on_header_missing fallback.** When the
  request carries no `x-tier` header, `on_header_missing` fires and
  writes the static string `missing` into `envoy.lb:tier`. An absent
  NAMESPACE (`%DYNAMIC_METADATA(envoy.absent:k)%`) renders a single dash
  `-` (never empty, never `{}`, never `null`).
- **§A6 — determinism.** When the same probe input (with or without
  `x-tier`) hits both proxies, both emit a byte-identical access-log
  line. STRONG cross-proxy whole-line byte-exact target — no fallback.

## The present + missing probe pair (the anti-echo guard)

Probe 1 sends `x-tier: prod` — `on_header_present` fires, writing
`envoy.lb:tier = prod`; the log emits `tier=prod missns=-`.
Probe 2 sends NO `x-tier` header — `on_header_missing` fires, writing
`envoy.lb:tier = missing`; the log emits `tier=missing missns=-`.

The present/missing PAIR is the anti-echo guard: a naive implementation
that simply echoed the header value would produce no line for Probe 2
(no header → no value to echo), but `on_header_missing` correctly
populates the metadata store with the configured static value `missing`.
Both probes resolve through the SAME dynamic-metadata path
(`record.dynamic_metadata["envoy.lb"]["tier"]`) — the two outcomes
(`prod` vs `missing`) prove the filter correctly branches between
`on_header_present` and `on_header_missing`.

## Probes

| # | request                     | expected byte-identical line              |
|---|-----------------------------|-------------------------------------------|
| 1 | `GET /a` + `x-tier: prod`   | `m=GET tier=prod missns=-`                |
| 2 | `GET /b` (no `x-tier`)      | `m=GET tier=missing missns=-`             |

## Per-side divergences

| Side | bind address | admin block | access-log path                           | `generate_request_id` |
|------|--------------|-------------|-------------------------------------------|-----------------------|
| envoy | `0.0.0.0`  | yes (port 0)| `/tmp/0042-envoy-mount/access.log`        | `false` (load-bearing) |
| envoy-rust | `127.0.0.1` | omitted | `/tmp/0042-envoy-rust-mount/access.log` | omitted (never injects) |

The parent directory is bind-mounted from the host into the Envoy
container so the harness can read the access.log file after the requests
complete (same wiring as fixtures 0012 / 0040 / 0041).

## Driver

`Driver::Http1AccessLogByteExact` (`kind: http1_access_log_byte_exact`,
phase 32). Drives the probe sequence via `drive_http1`, scrapes both
files, asserts the scraped line count equals `probes.len()`, and calls
`access_log::assert_access_log_lines_byte_identical`.

## Cross-references

- ADR: ADR-0083 (scope / brainstorm), ADR-0084 (§6.2 reconciliation).
- Related fixtures: 0041 (`set_metadata` + `%DYNAMIC_METADATA%` byte-exact),
  0040 (access-log command-operator byte-exact baseline), 0012 (default-format
  per-token access-log baseline), 0007 (H1 direct_response baseline).
