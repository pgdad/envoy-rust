# Fixture 0060 — access-log `%RESPONSE_FLAGS%` connect-failure path (`UF`, byte-exact)

The **FIFTH non-`-` `%RESPONSE_FLAGS%` witness** (phase 52, ADR-0109), after
phase 48's `NR` (NoRoute, fixture 0056), phase 49's `UH` (NoHealthyUpstream,
fixture 0057), phase 50's `UO` (UpstreamOverflow, fixture 0058) and phase 51's
`URX` (UpstreamRetryLimitExceeded, fixture 0059). This fixture witnesses the
upstream-connect-failure flag — `UF` (UpstreamConnectionFailure) — BYTE-EXACT on
the upstream-connect-refused **503** path.

The harness scrapes each proxy's file and asserts every line is
**byte-identical** between upstream Envoy v1.33.0 and envoy-rust (whole-line
`==`, the same `http1_access_log_byte_exact` driver as fixtures
0040/0046/0053/0056/0057/0058/0059).

## What this proves (`UF` is byte-exact cross-proxy)

On an upstream connect failure (the kernel refuses the TCP connect to a dead
endpoint), both proxies return a deterministic 503. Envoy v1.33.0 renders
`%RESPONSE_FLAGS%` = `UF` on this path (state-0 recon: live Envoy emits
`{"rc":503,"rcd":"upstream_reset_before_response_started{remote_connection_failure|delayed_connect_error:_Connection_refused}","rf":"UF"}`,
status 503, byte-stable across 8 repeats AND a container restart).

envoy-rust now (a) returns **503** (Task 1 corrected the previously unvalidated
connect-failure synth-502 to match Envoy — the connect-failure status was never
differentially validated, and envoy-rust already returns 503 on the sibling
overflow path) and (b) DERIVES `%RESPONSE_FLAGS%` = `UF` from a new per-request
`connect_failure_for_log` boolean — set post-loop when the FINAL attempt's
`AttemptOutcome` is `ConnectFailure` — at the H1 record-build site. `UF` is the
SECOND flag NOT derivable from `%RESPONSE_CODE_DETAILS%` (the connect-failure
rcd is the shared `via_upstream`, just as the phase-51 `URX` path's rcd is), so
it keys on the boolean, not the rcd — reusing phase-51's boolean-discriminator
mechanism. The branch is additive (the `URX`/`NR`/`UH`/`UO` arms are preserved
verbatim → fixtures 0056/0057/0058/0059 stay byte-identical).

The assertion is **pure cross-proxy equality** — there is NO static expected
literal. The connect-failure synth-503 is deterministic on both sides, so the
byte-exact driver covers the line.

## The `json_format` map (connect-failure route)

```yaml
route_config:
  virtual_hosts:
    - routes:
        - match: { prefix: "/" }
          route: { cluster: backend_cluster }
log_format:
  json_format:
    rc: "%RESPONSE_CODE%"
    rf: "%RESPONSE_FLAGS%"
```

| key   | operator                  | rendered value                                       |
|-------|---------------------------|-----------------------------------------------------|
| `rc`  | `%RESPONSE_CODE%`         | `503` (json NUMBER)                                  |
| `rf`  | `%RESPONSE_FLAGS%`        | `UF`                                                 |

**`%RESPONSE_CODE_DETAILS%` is OMITTED.** The connect-failure rcd carries the
OS-derived transport-failure reason
(`upstream_reset_before_response_started{remote_connection_failure|delayed_connect_error:_Connection_refused}`)
— the `Connection refused` strerror/errno-class string is non-deterministic
across environments (the M45-2 non-determinism class), so logging it would break
byte-identity. The response BODY likewise carries that reason; the
`http1_access_log_byte_exact` driver does **not** compare the response body
(`AccessLogByteExactProbe` has no `expected_body` field), so it is never
witnessed.

`%RESPONSE_CODE%` renders the bare json NUMBER `503` (not a quoted string) —
precedent fixture `0047-accesslog-json-nested`. Keys sort by UTF-8 byte order
(ADR-0094 §A): rc, rf; compact separators + ONE trailing `\n` (ADR-0092 §E).

## Probe

| # | request                    | emitted JSON object (byte-identical on both sides) |
|---|----------------------------|----------------------------------------------------|
| 1 | `GET /` (no extra headers) | see below                                          |

```
{"rc":503,"rf":"UF"}
```

A single probe — the connect-failure path is a single synth-503 arm.

## The connect-failure trigger (endpoint DIALED and kernel-refused)

`backend_cluster` is `STATIC` ROUND_ROBIN with **NO `circuit_breakers` and NO
`retry_policy`** and ONE endpoint at the LITERAL unreachable address
`127.0.0.1:1`. On the first `GET /`, both proxies **DIAL** the `127.0.0.1:1`
endpoint and the kernel refuses the connect → the connect-failure synth-503
(`rf:"UF"`).

This is the **key contrast with fixture 0058**: 0058 sets
`circuit_breakers.max_pending_requests: 0`, whose pending-gate rejects the
request with the overflow synth-503 (`UO`) BEFORE the endpoint is dialed. With
the `circuit_breakers` REMOVED, there is no pending-gate to reject pre-connect,
so the endpoint IS dialed and the kernel-refused connect produces the
connect-failure synth-503 (`UF`) instead.

Using a literal address (not a `{{BACKEND_*}}` marker) keeps both configs
byte-identical with no shared-IP machinery and no backend spawn — the asserted
line logs no `%UPSTREAM_HOST%`, so the endpoint address never appears. This is
the same NO-backend-spawned topology as fixtures 0053/0057/0058.

## Per-side divergences

| Side       | bind address | admin block | access-log path                          |
|------------|--------------|-------------|------------------------------------------|
| envoy      | `0.0.0.0`    | yes (port 0)| `/tmp/0060-envoy-mount/access.log`       |
| envoy-rust | `127.0.0.1`  | omitted     | `/tmp/0060-envoy-rust-mount/access.log`  |

There are NO upstream-specific deltas here: this fixture spawns no backend (the
endpoint is the literal dead `127.0.0.1:1`). The cluster + route + `json_format`
are BYTE-IDENTICAL across the two files (only the documented per-side deltas
differ). The parent directory is bind-mounted from the host into the Envoy
container so the harness can read the access.log file after the request completes
(same wiring as fixtures 0012 / 0040 / 0053 / 0056 / 0057 / 0058 / 0059).

## Driver

`kind: http1_access_log_byte_exact` (same driver as fixtures
0040/0046/0053/0056/0057/0058/0059) — drives the probe, asserts each side's
status == `expected_status` (503), scrapes both files, asserts the scraped line
count equals `probes.len()`, and calls
`access_log::assert_access_log_lines_byte_identical`. It does NOT compare the
downstream response body (the non-deterministic connect-failure body is never
witnessed). No new harness code; no backend spawn (no `{{BACKEND_*}}` marker);
no `needs_health_aware_backend` allowlist entry and no `--per-path` map arm (the
dead-endpoint pattern has no backend, no shared-IP machinery).

## Cross-references

- ADR: ADR-0109 (phase-52 pick + scope — witness the FIFTH non-`-`
  `%RESPONSE_FLAGS%` value `UF` byte-exact on the upstream-connect-refused 503
  path, by correcting the connect-failure synth status 502→503 and deriving
  `"UF"` from the connect-failure final-outcome boolean).
- Related fixtures: 0059 (`%RESPONSE_FLAGS%` = `URX`, the phase-51 sibling whose
  boolean-discriminator derive mechanism this reuses), 0058 (`%RESPONSE_FLAGS%`
  = `UO`, the phase-50 sibling whose NO-backend-spawned literal-address topology
  this reuses MINUS the `circuit_breakers`), 0057 (`UH`), 0056 (`NR`), 0047 (the
  `%RESPONSE_CODE%` bare-json-NUMBER precedent), 0012 (default access-log
  baseline).
- Deferred: the upstream-RESET/disconnect `UC` flag + the reset 502→503 status
  (M52-1: the `hcm.rs` send/recv-failure reset arm, a different post-connect
  path with a different flag and an un-recon'd trigger), the connect-failure
  `%RESPONSE_CODE_DETAILS%` + response body (M45-2, non-deterministic transport-
  failure reason — NOT logged/NOT compared), the H2 connect-failure path (M45-1:
  H2 record-build site hard-codes `"-"`; no H2 access-log differential driver),
  and the `DC` flag + retry-budget-overflow slice (M45-2). `UF` now moves OUT of
  the M45-2 unwitnessed set (leaving `DC`).
