# Fixture 0051 — access-log `%UPSTREAM_CLUSTER%` operator (byte-exact, first proxy-routed access-log)

The first fixture exercising envoy-rust's `%UPSTREAM_CLUSTER%` access-log
command operator (phase 43, ADR-0100), and the **first access-log fixture to
route to a real upstream backend** — all of fixtures `0040`-`0050` route via
`direct_response` (`clusters: []`). Combines fixture `0008`'s both-proxies-dial-a-
real-`Http1EchoBackend` STRICT_DNS posture with fixture `0050`'s file
access-logger emitting ONE JSON object per request. The harness scrapes each
proxy's file and asserts every line is **byte-identical** between upstream Envoy
v1.33.0 and envoy-rust (whole-line `==`, same `http1_access_log_byte_exact`
driver as fixtures 0040/0046/0047/0048/0049/0050).

## What this proves (`%UPSTREAM_CLUSTER%` is byte-exact cross-proxy)

ADR-0100: `%UPSTREAM_CLUSTER%` renders the name of the cluster the request was
routed to — an `Option<String>` IDENTICAL in shape to `%UPSTREAM_HOST%` /
`%ROUTE_NAME%` (present → the name; absent → the `-` sentinel in a multi-segment
leaf, json `null` in a single-operator-typed leaf). For a
`route: { cluster: backend }` route the value is the literal `backend`. It is
populated at the HCM proxy-arm from `BuildOutcome::Proxy { cluster }` and is
**config-deterministic** (depends only on the request + the cluster config, NOT
the backend's response body/status).

- **single-operator-typed leaf → quoted string.** A value that is EXACTLY one
  `%UPSTREAM_CLUSTER%` routes through the typed encoder:
  - `uc: "%UPSTREAM_CLUSTER%"`        → `"backend"` (quoted)
- **multi-segment leaf → string with the value spliced in.**
  - `mixed: "c=%UPSTREAM_CLUSTER%"`   → `"c=backend"`
- Also witnesses `%RESPONSE_CODE_DETAILS%` = `via_upstream` on a REAL upstream
  (the proxy-success arm — fixture 0050 only witnessed the `direct_response`
  arm). This advances carry-forward M42-1.
- Present operators are normal (`method`/`proto`).
- Keys sort by UTF-8 byte order (ADR-0094 §A): method, mixed, proto, rcd, uc;
  compact separators + ONE trailing `\n` (ADR-0092 §E).

## The `json_format` map (`route: { cluster: backend }` route)

```yaml
route_config:
  virtual_hosts:
    - routes:
        - match: { prefix: "/" }
          route: { cluster: backend }
log_format:
  json_format:
    uc: "%UPSTREAM_CLUSTER%"
    mixed: "c=%UPSTREAM_CLUSTER%"
    rcd: "%RESPONSE_CODE_DETAILS%"
    method: "%REQ(:METHOD)%"
    proto: "%PROTOCOL%"
```

Every operator here is deterministic given a fixed request + the static
`route: { cluster: backend }` route, so the strongest assertion — every byte of
the emitted line identical across the two proxies — applies.

## Probe

| # | request                    | emitted JSON object (byte-identical on both sides) |
|---|----------------------------|----------------------------------------------------|
| 1 | `GET /` (no extra headers) | see below |

```
{"method":"GET","mixed":"c=backend","proto":"HTTP/1.1","rcd":"via_upstream","uc":"backend"}
```

This is the ADR-0100 authoritative line, **captured live** from
`envoyproxy/envoy:v1.33.0` (phase-43 state-1 recon).

## `%UPSTREAM_HOST%` is DELIBERATELY EXCLUDED (per-side mismatch)

`%UPSTREAM_HOST%` (the backend `ip:port`) is **not** logged here. Its value
resolves to DIFFERENT bytes per-side — the host-gateway IP on the
Envoy-container side vs `127.0.0.1` on the envoy-rust-host-subprocess side — so
it is structurally non-byte-identical (the §6.2-LOCKED decision). The 5 tokens
this fixture DOES log depend only on the request + the cluster config, not on
the backend's wire address or its response, so they stay byte-identical
cross-proxy. The real-`ip:port` `%UPSTREAM_HOST%` render is proven in the
in-process backstop instead.

## Real upstream — both proxies dial the same `Http1EchoBackend`

The `backend` cluster is `STRICT_DNS` with
`address: {{BACKEND_HOST}}, port_value: {{HTTP1_BACKEND_PORT}}` (same as fixture
0008). The harness auto-spawns the `Http1EchoBackend` when the config carries
`{{HTTP1_BACKEND_PORT}}` (marker-driven, independent of the driver kind) and
templates `{{BACKEND_HOST}}` per-side (`host.docker.internal` on the Envoy side,
`127.0.0.1` on the envoy-rust side). The backend's echo response is irrelevant
to the asserted line (no response-body operator is logged).

## Per-side divergences

| Side | bind address | admin block | access-log path                         | `generate_request_id` | `request_headers_to_remove` |
|------|--------------|-------------|-----------------------------------------|-----------------------|-----------------------------|
| envoy | `0.0.0.0`   | yes (port 0)| `/tmp/0051-envoy-mount/access.log`      | `false` (load-bearing) | present (6 headers)         |
| envoy-rust | `127.0.0.1` | omitted | `/tmp/0051-envoy-rust-mount/access.log` | omitted (never injects) | omitted (parser rejects it) |

`request_headers_to_remove` + `generate_request_id: false` on the Envoy side
align it with envoy-rust's no-inject posture on the upstream-bound request (same
field-set divergence as fixture 0008). The parent directory is bind-mounted from
the host into the Envoy container so the harness can read the access.log file
after the request completes (same wiring as fixtures 0012 / 0040 / 0050).

## Driver

`kind: http1_access_log_byte_exact` (same driver as fixtures
0040/0046/0047/0048/0049/0050) — drives the probe sequence, scrapes both files,
asserts the scraped line count equals `probes.len()`, and calls
`access_log::assert_access_log_lines_byte_identical`. No new harness code: the
`Http1EchoBackend` spawn is marker-driven (gated on `{{HTTP1_BACKEND_PORT}}`,
BEFORE the driver dispatch), so the existing `Http1WithAccessLog` run-arm drives
the proxied request as-is.

## Cross-references

- ADR: ADR-0100 (state-1 brainstorm + state-2 PLAN — the `%UPSTREAM_CLUSTER%`
  pick: it renders the routed cluster name, an `Option<String>` shaped exactly
  like `%UPSTREAM_HOST%`).
- Related fixtures: 0050 (`%RESPONSE_CODE_DETAILS%`, the access-log mirror this
  is shaped after), 0049 (`%ROUTE_NAME%`), 0008 (the H1 router-upstream
  STRICT_DNS + auto-spawned `Http1EchoBackend` posture), 0012 (default
  per-token access-log baseline).
