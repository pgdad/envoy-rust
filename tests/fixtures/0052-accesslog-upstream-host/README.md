# Fixture 0052 — access-log `%UPSTREAM_HOST%` operator (byte-exact, shared-IP STATIC cluster)

The fixture that **closes the `%UPSTREAM_HOST%` gap fixture `0051` left open**
(phase 44, ADR-0101). Fixture `0051` deliberately EXCLUDED `%UPSTREAM_HOST%`
because its `STRICT_DNS` `{{BACKEND_HOST}}` cluster resolves to DIFFERENT bytes
per side (host-gateway IP on the Envoy-container side vs `127.0.0.1` on the
envoy-rust-host-subprocess side) — structurally non-byte-identical. Fixture
`0052` removes that obstacle by routing through a **`{{BACKEND_IP}}`
shared-host-LAN-IP STATIC cluster**: the harness renders `{{BACKEND_IP}}` to ONE
numeric IP IDENTICAL on BOTH sides (precedent: fixture `0036`), so both proxies
dial the SAME `<ip>:<port>` and therefore render the SAME `%UPSTREAM_HOST%`.

The harness scrapes each proxy's file and asserts every line is
**byte-identical** between upstream Envoy v1.33.0 and envoy-rust (whole-line
`==`, the same `http1_access_log_byte_exact` driver as fixtures
0040/0046/0047/0048/0049/0050/0051).

## What this proves (`%UPSTREAM_HOST%` is byte-exact cross-proxy)

`%UPSTREAM_HOST%` renders the resolved upstream endpoint `<ip>:<port>` — the
host the request was actually proxied to. It has been **implemented since phase
06**: envoy-rust renders it via `SocketAddr::to_string()` = `<ip>:<port>` (IPv4
unbracketed), which is byte-for-byte Envoy's format. The phase-44 §6.2
format-match recon PROVED no `src/` change is needed; this fixture is a
FIXTURE-ONLY differential witness of that already-correct render.

The assertion is **pure cross-proxy equality** — there is NO static expected
literal. The `%UPSTREAM_HOST%` value is the DYNAMIC-but-SHARED
`<host-LAN-IP>:<port>`: its exact bytes vary per CI run (the host's LAN IPv4 +
the kernel-ephemeral backend port), but because BOTH proxies dial the SAME
`{{BACKEND_IP}}:{{HTTP1_BACKEND_PORT}}`, the rendered `uh` value is IDENTICAL on
both sides. The byte-exact driver covers the dynamic token without a hard-coded
value.

## The `json_format` map (`route: { cluster: backend }` route)

```yaml
route_config:
  virtual_hosts:
    - routes:
        - match: { prefix: "/" }
          route: { cluster: backend }
log_format:
  json_format:
    uh: "%UPSTREAM_HOST%"
    uc: "%UPSTREAM_CLUSTER%"
    rcd: "%RESPONSE_CODE_DETAILS%"
    method: "%REQ(:METHOD)%"
    proto: "%PROTOCOL%"
```

Alongside the dynamic-but-shared `%UPSTREAM_HOST%`, the leaf carries
deterministic cross-proxy anchors so a divergence in the surrounding bytes is
also caught:

| key      | operator                 | rendered value                       |
|----------|--------------------------|--------------------------------------|
| `method` | `%REQ(:METHOD)%`         | `GET`                                |
| `proto`  | `%PROTOCOL%`             | `HTTP/1.1`                           |
| `rcd`    | `%RESPONSE_CODE_DETAILS%`| `via_upstream` (real-upstream success)|
| `uc`     | `%UPSTREAM_CLUSTER%`     | `backend`                            |
| `uh`     | `%UPSTREAM_HOST%`        | `<ip>:<port>` (shared, per-run)      |

Keys sort by UTF-8 byte order (ADR-0094 §A): method, proto, rcd, uc, uh;
compact separators + ONE trailing `\n` (ADR-0092 §E).

## Probe

| # | request                    | emitted JSON object (byte-identical on both sides) |
|---|----------------------------|----------------------------------------------------|
| 1 | `GET /` (no extra headers) | see below                                          |

```
{"method":"GET","proto":"HTTP/1.1","rcd":"via_upstream","uc":"backend","uh":"<ip>:<port>"}
```

`<ip>:<port>` is the per-run shared `<host-LAN-IP>:<backend-port>`; the assertion
is cross-proxy equality, not a match against this literal.

## Shared-IP STATIC cluster — both proxies dial the SAME `<ip>:<port>`

The `backend` cluster is `STATIC` with a SINGLE endpoint
`address: {{BACKEND_IP}}, port_value: {{HTTP1_BACKEND_PORT}}`. The harness renders
`{{BACKEND_IP}}` to ONE numeric IP (the host's LAN IPv4) IDENTICAL on BOTH sides
(precedent: fixture 0036's RING_HASH cluster) and auto-spawns the
`Http1EchoBackend` on `{{HTTP1_BACKEND_PORT}}` (marker-driven, like fixture
0008). This is the load-bearing difference from fixture 0051: a STATIC
shared-IP cluster makes `%UPSTREAM_HOST%` byte-identical cross-proxy, whereas
0051's STRICT_DNS `{{BACKEND_HOST}}` cluster splits per-side. The backend's echo
response is irrelevant to the asserted line (no response-body operator logged).

## Per-side divergences

| Side       | bind address | admin block | access-log path                          | `generate_request_id` | `request_headers_to_remove` |
|------------|--------------|-------------|------------------------------------------|-----------------------|-----------------------------|
| envoy      | `0.0.0.0`    | yes (port 0)| `/tmp/0052-envoy-mount/access.log`       | `false` (load-bearing)| present (6 headers)         |
| envoy-rust | `127.0.0.1`  | omitted     | `/tmp/0052-envoy-rust-mount/access.log`  | omitted (never injects)| omitted (parser rejects it)|

`request_headers_to_remove` + `generate_request_id: false` on the Envoy side
align it with envoy-rust's no-inject posture on the upstream-bound request (same
field-set divergence as fixtures 0008 / 0051). The parent directory is
bind-mounted from the host into the Envoy container so the harness can read the
access.log file after the request completes (same wiring as fixtures 0012 / 0040
/ 0050 / 0051).

## Driver

`kind: http1_access_log_byte_exact` (same driver as fixtures
0040/0046/0047/0048/0049/0050/0051) — drives the probe sequence, scrapes both
files, asserts the scraped line count equals `probes.len()`, and calls
`access_log::assert_access_log_lines_byte_identical`. No new harness code: the
`Http1EchoBackend` spawn is marker-driven (gated on `{{HTTP1_BACKEND_PORT}}`,
BEFORE the driver dispatch) and the `{{BACKEND_IP}}` shared-IP render is the
established fixture-0036 path.

## Cross-references

- ADR: ADR-0101 (state-1 brainstorm + state-2 PLAN — the `%UPSTREAM_HOST%`
  pick: byte-exact differential of the already-implemented operator via a
  shared-IP STATIC cluster).
- Related fixtures: 0051 (`%UPSTREAM_CLUSTER%` + `%RESPONSE_CODE_DETAILS%`, the
  real-upstream access-log fixture this directly extends — it EXCLUDED
  `%UPSTREAM_HOST%`; 0052 closes that gap), 0036 (the `{{BACKEND_IP}}`
  shared-host-LAN-IP STATIC cluster precedent), 0008 (the H1 router-upstream +
  auto-spawned `Http1EchoBackend` posture), 0012 (default per-token access-log
  baseline).
