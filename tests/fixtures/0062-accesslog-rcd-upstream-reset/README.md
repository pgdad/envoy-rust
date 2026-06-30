# Fixture 0062 — access-log `%RESPONSE_CODE_DETAILS%` upstream-reset path (`upstream_reset_before_response_started{connection_termination}`, byte-exact)

The **SEVENTH differentially-witnessed `%RESPONSE_CODE_DETAILS%` value** (phase
54, ADR-0111), after `direct_response`/`via_upstream` (phase 42, fixture 0050),
`no_healthy_upstream` (phase 45, fixture 0053), `route_not_found` route-miss
(phase 46, fixture 0054) + host-miss (phase 47, fixture 0055), and
`upstream_reset_before_response_started{overflow}` (phase 50, fixture 0058) —
and the **FIRST deterministic upstream-reset rcd**. Witnesses
`upstream_reset_before_response_started{connection_termination}` BYTE-EXACT on
the upstream-disconnect-before-headers **503** path. CONSUMES carry-forward
**M53-1** (the deterministic `UC` rcd the phase-53 SPEC §4 earmarked).

This fixture is a structural clone of **0061** (phase 53, the `{rc,rf}`-only
`UC`-flag witness) — same accept-then-close `STRICT_DNS` backend reused via the
`{{CLOSE_BACKEND_PORT}}` marker — with the json_format extended to add
`rcd: "%RESPONSE_CODE_DETAILS%"`.

> **⚠ LOCAL-RED expected; CI is AUTHORITATIVE.** 0062 SPAWNS a backend (the
> accept-then-close `tcp-echo-server --close-on-accept` via the
> `{{CLOSE_BACKEND_PORT}}` marker) and is therefore subject to the host's Docker
> bridge-IP differential flake (memory `differential-host-bridge-ip-192-168-65-2`):
> **expect LOCAL-RED on this dev host and GREEN on native-Linux CI** — CI is the
> authority for the §7.5 gate (the phase-53/0061 precedent).

## What this proves

On an upstream disconnect before response headers (the upstream completes the
TCP connect, then closes — a graceful FIN — before delivering any response),
both proxies return a deterministic **503** and render
`%RESPONSE_CODE_DETAILS%` = `upstream_reset_before_response_started{connection_termination}`
+ `%RESPONSE_FLAGS%` = `UC`. The brace content `connection_termination` is a
FIXED reset-reason enum (NOT OS-derived, UNLIKE the connect-failure rcd) → byte-
exact deterministic, structurally identical to the phase-50 `{overflow}` rcd
(fixture 0058). state-0 recon (live v1.33.0, digest sha256:56da5afd…): byte-
stable across 3 probes + a container restart.

envoy-rust now (§A) SETS the deterministic reset rcd on the pure-reset final-
outcome path (overriding the in-loop shared `via_upstream`, guarded
`!retry_limit_exceeded_for_log` so a retry-exhausted reset keeps `via_upstream` +
`URX`), and (§B) DERIVES `%RESPONSE_FLAGS%` = `UC` 1:1 from that rcd (the phase-50
`{overflow} => "UO"` precedent), RETIRING the phase-53 `reset_for_log` boolean.

The assertion is **pure cross-proxy equality** — there is NO static expected
literal; the byte-exact driver compares the lines + status (NOT the body).

## The `json_format` map

| key   | operator                  | rendered value                                                  |
|-------|---------------------------|-----------------------------------------------------------------|
| `rc`  | `%RESPONSE_CODE%`         | `503` (json NUMBER)                                             |
| `rcd` | `%RESPONSE_CODE_DETAILS%` | `upstream_reset_before_response_started{connection_termination}` |
| `rf`  | `%RESPONSE_FLAGS%`        | `UC`                                                           |

Keys sort by UTF-8 byte order (ADR-0094 §A): rc, rcd, rf; compact separators +
ONE trailing `\n` (ADR-0092 §E). Emitted line:

```
{"rc":503,"rcd":"upstream_reset_before_response_started{connection_termination}","rf":"UC"}
```

## Per-side divergences

| Side       | bind address | admin block | access-log path                          | `{{BACKEND_HOST}}`     |
|------------|--------------|-------------|------------------------------------------|------------------------|
| envoy      | `0.0.0.0`    | yes (port 0)| `/tmp/0062-envoy-mount/access.log`       | `host.docker.internal` |
| envoy-rust | `127.0.0.1`  | omitted     | `/tmp/0062-envoy-rust-mount/access.log`  | `127.0.0.1`            |

Because the asserted line omits `%UPSTREAM_HOST%`, the per-side `{{BACKEND_HOST}}`
divergence never appears → byte-identity holds.

## Driver

`kind: http1_access_log_byte_exact` (same driver as 0061) — drives the `GET /`
probe, asserts each side's status == 503, scrapes both files, asserts the line
count == `probes.len()`, and calls `assert_access_log_lines_byte_identical`. The
`{{CLOSE_BACKEND_PORT}}` marker triggers the `TcpCloseBackend` launch arm in
`run_fixture` (`tests/differential/src/lib.rs`) — no per-driver backend allowlist,
no new harness code.

## Cross-references

- ADR: ADR-0111 (phase-54 pick + scope).
- Related fixtures: 0061 (`{rc,rf}`-only `UC` sibling, whose accept-then-close
  backend harness this reuses), 0058 (`{overflow}` rcd, the deterministic
  reset-reason-enum precedent), 0050 (the `%RESPONSE_CODE_DETAILS%` baseline).
- Consumes: M53-1. Retires: the phase-53 `reset_for_log` boolean.
- Deferred: the H2 reset rcd (M45-1), the `DC` flag (M45-2), an upstream RST vs
  the graceful FIN (un-recon'd reset-reason brace).
