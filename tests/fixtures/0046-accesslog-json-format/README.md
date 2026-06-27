# Fixture 0046 — access-log `json_format` encoder (byte-exact)

The first fixture exercising envoy-rust's `json_format` access-log output mode
(phase 38, ADR-0092). An H1 `direct_response` listener with a `file`
access-logger whose `log_format` is a `json_format` map of DETERMINISTIC
command operators emits one sorted JSON object per request; the harness scrapes
each proxy's file and asserts every line is **byte-identical** between upstream
Envoy v1.33.0 and envoy-rust (whole-line `==`, same driver as fixture 0040).

## What this proves

The JSON envelope is byte-exact cross-proxy:

- **Keys SORTED by UTF-8 bytes** (ADR-0092 §A) — the config lists keys in
  arbitrary order; both proxies emit them sorted (`bytes_rcvd` < `bytes_sent`
  < `flags` < `method` < `mixed` < `path` < `protocol` < `status` <
  `upstream`).
- **Values TYPE-INFERRED** (ADR-0092 §B) — a value that is EXACTLY one
  operator takes its native JSON type:
  - numeric op → unquoted number (`%RESPONSE_CODE%` → `200`,
    `%BYTES_RECEIVED%` → `0`, `%BYTES_SENT%` → `3`);
  - string op → quoted string (`%REQ(:METHOD)%` → `"GET"`, `%PROTOCOL%` →
    `"HTTP/1.1"`, `%RESPONSE_FLAGS%` → `"-"`);
  - absent op → `null` (`%UPSTREAM_HOST%` on a `direct_response` route → no
    upstream → `null`);
  - a value with a literal prefix (`mixed: "code-%RESPONSE_CODE%"`) is a
    quoted string via the engine → `"code-200"`.
- **Compact separators + trailing `\n`** (ADR-0092 §D): `{"k":v,"k2":v2}\n`.

## The `json_format` map

```yaml
json_format:
  method: "%REQ(:METHOD)%"
  path: "%REQ(:PATH)%"
  protocol: "%PROTOCOL%"
  status: "%RESPONSE_CODE%"
  flags: "%RESPONSE_FLAGS%"
  bytes_rcvd: "%BYTES_RECEIVED%"
  bytes_sent: "%BYTES_SENT%"
  upstream: "%UPSTREAM_HOST%"
  mixed: "code-%RESPONSE_CODE%"
```

Every operator is deterministic given a fixed request + route (no
`%START_TIME%`/`%DURATION%`/`%REQ(X-REQUEST-ID)%`), so the strongest possible
assertion — every byte of the emitted line identical across the two proxies —
applies. `%UPSTREAM_HOST%` is absent (`null`) on a `direct_response` route,
keeping the line byte-identical with ZERO `{{BACKEND_IP}}` complexity (the
present-upstream `null`-vs-quoted classification is proven separately by the
in-process `json_format` encoder backstop tests, envoy-accesslog Task 4/5).

## Probe

| # | request                    | emitted JSON object (byte-identical on both sides) |
|---|----------------------------|----------------------------------------------------|
| 1 | `GET /` (no extra headers) | see below |

```
{"bytes_rcvd":0,"bytes_sent":3,"flags":"-","method":"GET","mixed":"code-200","path":"/","protocol":"HTTP/1.1","status":200,"upstream":null}
```

## Per-side divergences

| Side | bind address | admin block | access-log path                         | `generate_request_id` |
|------|--------------|-------------|-----------------------------------------|-----------------------|
| envoy | `0.0.0.0`   | yes (port 0)| `/tmp/0046-envoy-mount/access.log`      | `false` (load-bearing) |
| envoy-rust | `127.0.0.1` | omitted | `/tmp/0046-envoy-rust-mount/access.log` | omitted (never injects) |

The parent directory is bind-mounted from the host into the Envoy container so
the harness can read the access.log file after the request completes (same
wiring as fixtures 0012 / 0040).

## Driver

`kind: http1_access_log_byte_exact` (same driver as fixture 0040) — drives the
probe sequence, scrapes both files, asserts the scraped line count equals
`probes.len()`, and calls `access_log::assert_access_log_lines_byte_identical`.

## Cross-references

- ADR: ADR-0091 (state-1 brainstorm — pick `json_format`), ADR-0092 (state-2
  PLAN — §A–§F empirical recon vs `envoyproxy/envoy:v1.33.0`, the
  sorted-keys / typed-values / byte-exact-line facts).
- Related fixtures: 0040 (command-operator `text_format_source` byte-exact),
  0012 (default-format per-token access-log baseline), 0007 (H1
  direct_response baseline).
