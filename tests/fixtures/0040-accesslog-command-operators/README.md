# Fixture 0040 — access-log command-operator formatter (byte-exact)

The first fixture exercising envoy-rust's CUSTOM access-log `log_format`
(the command-operator formatter landed in phase 32 Tasks 1-5). An H1
`direct_response` listener with a `file` access-logger whose `log_format`
is a custom `text_format_source` of DETERMINISTIC command operators emits
one access-log line per request; the harness scrapes each proxy's file and
asserts every line is **byte-identical** between upstream Envoy and
envoy-rust (whole-line `==`, NOT the per-token default-format comparison
fixture 0012 uses).

## Why whole-line byte-exact (vs 0012's per-token)

Fixture 0012 asserts the *default* format, which contains
non-deterministic tokens (`%START_TIME%`, `%DURATION%`,
`%REQ(X-REQUEST-ID)%`) that can only be compared per-token with
tolerance rules. Fixture 0040 deliberately uses a format built ONLY from
deterministic operators, so the strongest possible assertion — every byte
of every line identical across the two proxies — applies.

## The custom format

```
m=%REQ(:METHOD)% p=%REQ(:PATH)% proto=%PROTOCOL% code=%RESPONSE_CODE% flags=%RESPONSE_FLAGS% rx=%BYTES_RECEIVED% tx=%BYTES_SENT% ua=%REQ(USER-AGENT)% xff=%REQ(X-FORWARDED-FOR)% auth=%REQ(:AUTHORITY)% up=%UPSTREAM_HOST%\n
```

Carried in YAML as a double-quoted scalar with a trailing `\n`; both
Envoy and envoy-rust's `serde_yaml` interpret the escape as a literal
newline, so each emitted record ends with exactly one `\n`.

Every operator in this format is deterministic given a fixed request +
route:

- `%REQ(:METHOD)%`, `%REQ(:PATH)%`, `%PROTOCOL%`, `%REQ(:AUTHORITY)%` —
  echo the request line / `:authority`.
- `%RESPONSE_CODE%` / `%RESPONSE_FLAGS%` — `200` / `-` for the
  `direct_response`.
- `%BYTES_RECEIVED%` / `%BYTES_SENT%` — `0` / `3` (`ok\n`) for both GET
  probes.
- `%REQ(USER-AGENT)%` / `%REQ(X-FORWARDED-FOR)%` — render `-` when absent
  (probe 1) and the literal header value when present (probe 2).
- `%UPSTREAM_HOST%` — renders `-` because the route is a
  `direct_response` (NO upstream). This is the key design decision: it
  keeps every operator byte-identical cross-proxy with ZERO
  `{{BACKEND_IP}}` complexity. The real `ip:port` render of
  `%UPSTREAM_HOST%` is proven separately by the in-process
  evaluator backstop (envoy-accesslog Task-2 tests), so this fixture does
  not need a live upstream.

EXCLUDED (non-deterministic / timing — backstop-only, never in this
fixture): `%START_TIME%`, `%DURATION%`, `%REQ(X-REQUEST-ID)%`,
`%RESP(X-ENVOY-UPSTREAM-SERVICE-TIME)%`.

## Probes

| # | request                                   | `ua=` | `xff=` |
|---|-------------------------------------------|-------|--------|
| 1 | `GET /` (no extra headers)                | `-`   | `-`    |
| 2 | `GET /` + `user-agent` + `x-forwarded-for`| `curl/8.0` | `203.0.113.7` |

Expected emitted lines (byte-identical on both sides):

```
m=GET p=/ proto=HTTP/1.1 code=200 flags=- rx=0 tx=3 ua=- xff=- auth=envoy-rust.test up=-
m=GET p=/ proto=HTTP/1.1 code=200 flags=- rx=0 tx=3 ua=curl/8.0 xff=203.0.113.7 auth=envoy-rust.test up=-
```

## Per-side divergences

| Side | bind address | admin block | access-log path                          | `generate_request_id` |
|------|--------------|-------------|------------------------------------------|-----------------------|
| envoy | `0.0.0.0`   | yes (port 0)| `/tmp/0040-envoy-mount/access.log`       | `false` (load-bearing) |
| envoy-rust | `127.0.0.1` | omitted | `/tmp/0040-envoy-rust-mount/access.log` | omitted (never injects) |

The parent directory is bind-mounted from the host into the Envoy
container so the harness can read the access.log file after the requests
complete (same wiring as fixture 0012). `generate_request_id: false` is
retained for parity with 0012 even though this fixture's format does not
emit `%REQ(X-REQUEST-ID)%`.

## Driver

`Driver::Http1AccessLogByteExact` (phase 32 Task 6 NEW; `kind:
http1_access_log_byte_exact`). Drives the probe sequence via the 04.1-
landed `drive_http1` helper (reused exactly as `Http1WithAccessLog`
does), then scrapes both files, asserts the scraped line count equals
`probes.len()`, and calls
`access_log::assert_access_log_lines_byte_identical`.

## Cross-references

- ADR: ADR-0078 (state-1 brainstorm), ADR-0079 (state-2 PLAN — §6.2 recon
  + the deterministic-operators-only / direct_response design decisions).
- Related fixtures: 0012 (default-format per-token access-log baseline),
  0007 (H1 direct_response baseline).
