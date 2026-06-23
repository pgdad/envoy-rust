# Fixture 0041 — `set_metadata` + `%DYNAMIC_METADATA%` (byte-exact)

The phase-33 witness fixture for the smallest end-to-end dynamic-metadata
loop: the `envoy.filters.http.set_metadata` HTTP filter writes a static
value into a per-request dynamic-metadata store, and the
`%DYNAMIC_METADATA(namespace:key)%` access-log command-operator reads it
back. An H1 `direct_response` listener whose filter chain is
`[set_metadata, router]` carries a `file` access-logger whose
`log_format` is a DETERMINISTIC `text_format_source` built from one
present and two absent `%DYNAMIC_METADATA%` reads. The harness scrapes
each proxy's file and asserts every line is **byte-identical** between
upstream Envoy and envoy-rust (whole-line `==`, the same
`Http1AccessLogByteExact` driver fixture 0040 uses).

## The custom format

```
m=%REQ(:METHOD)% code=%RESPONSE_CODE% tier=%DYNAMIC_METADATA(envoy.test:tier)% missk=%DYNAMIC_METADATA(envoy.test:missing)% missns=%DYNAMIC_METADATA(envoy.absent:k)%\n
```

Carried in YAML as a double-quoted scalar with a trailing `\n`; both
proxies' `serde_yaml` interpret the escape as a literal newline, so each
record ends with exactly one `\n`.

## §A-locked facts this fixture pins (ADR-0081)

- **§A1 — `set_metadata` wire shape.** The `@type` is
  `type.googleapis.com/envoy.extensions.filters.http.set_metadata.v3.Config`
  (the proto message is `Config`, NOT `SetMetadata` — the projected
  `…v3.SetMetadata` URL DOES NOT EXIST and is boot-fatal in Envoy). The
  modern repeated form `metadata: [{ metadata_namespace, value }]` is
  used (boots clean, zero warnings). The value lands under the
  `metadata_namespace` string verbatim (`envoy.test`).
- **§A2 — operator grammar.** `%DYNAMIC_METADATA(envoy.test:tier)%` uses
  the `:` path separator and exactly two segments (`namespace:key`). No
  `:N` length suffix (boot-fatal in Envoy), no no-arg form (boot-fatal);
  this fixture exercises only the single-level two-segment MVP.
- **§A3 — present value byte form.** A scalar STRING leaf (`prod`) renders
  **RAW, UNQUOTED** `prod` (`od -c` → `[ p r o d ]`, never `"prod"`).
- **§A4 — absent rendering.** An absent KEY
  (`%DYNAMIC_METADATA(envoy.test:missing)%`) and an absent NAMESPACE
  (`%DYNAMIC_METADATA(envoy.absent:k)%`) BOTH render a single dash `-`
  (never empty, never `{}`, never `null`).
- **§A6 — determinism.** The `%DYNAMIC_METADATA%` render of a static-config
  value is a pure function of static config (no host-address / clock
  terms), so both proxies emit a byte-identical line. STRONG cross-proxy
  whole-line byte-exact target — no fallback.

## The present + absent probe pair (the anti-echo guard)

Each line carries ONE present read (`tier=`) and TWO absent reads
(`missk=`, `missns=`). The present + absent PAIR is the guard against an
echo-the-config-literal (non-store-backed) implementation: a naive
implementation that simply echoed the configured `prod` literal would
have no way to render `-` for the absent key / namespace from the SAME
store path. A correct implementation resolves all three through
`record.dynamic_metadata.get(ns)?.get(key)` — present → the stored
string, absent → `-`.

## Probes

| # | request          | expected byte-identical line                  |
|---|------------------|-----------------------------------------------|
| 1 | `GET /a`         | `m=GET code=200 tier=prod missk=- missns=-`   |
| 2 | `POST /b` (body `x`) | `m=POST code=200 tier=prod missk=- missns=-` |

Probe 2 (POST + body) proves the static metadata is request-method /
request-body independent — the `set_metadata` filter writes the same
config-static value on every request.

## Per-side divergences

| Side | bind address | admin block | access-log path                          | `generate_request_id` |
|------|--------------|-------------|------------------------------------------|-----------------------|
| envoy | `0.0.0.0`   | yes (port 0)| `/tmp/0041-envoy-mount/access.log`       | `false` (load-bearing) |
| envoy-rust | `127.0.0.1` | omitted | `/tmp/0041-envoy-rust-mount/access.log` | omitted (never injects) |

The parent directory is bind-mounted from the host into the Envoy
container so the harness can read the access.log file after the requests
complete (same wiring as fixtures 0012 / 0040).

## Driver

`Driver::Http1AccessLogByteExact` (`kind: http1_access_log_byte_exact`,
phase 32). Drives the probe sequence via `drive_http1`, scrapes both
files, asserts the scraped line count equals `probes.len()`, and calls
`access_log::assert_access_log_lines_byte_identical`.

## Cross-references

- ADR: ADR-0080 (scope), ADR-0081 (§A empirical reconciliation), ADR-0082
  (split — reserved, NOT fired).
- Related fixtures: 0040 (access-log command-operator byte-exact baseline),
  0012 (default-format per-token access-log baseline), 0007 (H1
  direct_response baseline).
