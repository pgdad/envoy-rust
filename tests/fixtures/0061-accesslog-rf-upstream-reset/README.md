# Fixture 0061 — access-log `%RESPONSE_FLAGS%` upstream-reset path (`UC`, byte-exact)

The **SIXTH non-`-` `%RESPONSE_FLAGS%` witness** (phase 53, ADR-0110), after
phase 48's `NR` (NoRoute, fixture 0056), phase 49's `UH` (NoHealthyUpstream,
fixture 0057), phase 50's `UO` (UpstreamOverflow, fixture 0058), phase 51's
`URX` (UpstreamRetryLimitExceeded, fixture 0059) and phase 52's `UF`
(UpstreamConnectionFailure, fixture 0060). This fixture witnesses the
upstream-disconnect-before-headers flag — `UC` (UpstreamConnectionTermination) —
BYTE-EXACT on the upstream-reset **503** path.

The harness scrapes each proxy's file and asserts every line is
**byte-identical** between upstream Envoy v1.33.0 and envoy-rust (whole-line
`==`, the same `http1_access_log_byte_exact` driver as fixtures
0040/0046/0053/0056/0057/0058/0059/0060).

> **⚠ LOCAL-RED expected; CI is AUTHORITATIVE.** UNLIKE fixture 0060 (a
> no-backend dead-literal `127.0.0.1:1`), **0061 SPAWNS a backend** (the
> accept-then-close `tcp-echo-server --close-on-accept` via the
> `{{CLOSE_BACKEND_PORT}}` marker). It is therefore subject to the host's
> Docker bridge-IP differential flake (memory
> `differential-host-bridge-ip-192-168-65-2`): **expect this fixture to be
> LOCAL-RED on this dev host and GREEN on native-Linux CI** — CI is the
> authority for the §7.5 gate. This is the same backend-spawning access-log
> posture as the phase-44 / 0052 precedent.

## What this proves (`UC` is byte-exact cross-proxy)

On an upstream disconnect before response headers (the upstream completes the
TCP connect, then closes the connection — a graceful FIN — before delivering any
response), both proxies return a deterministic 503. Envoy v1.33.0 renders
`%RESPONSE_FLAGS%` = `UC` on this path (state-0 recon: live Envoy emits
`{"rc":503,"rcd":"upstream_reset_before_response_started{connection_termination}","rf":"UC"}`,
status 503, byte-stable across 8 repeats AND a container restart).

envoy-rust now (a) returns **503** (Task 2 corrected the previously unvalidated
reset synth-502 to match Envoy — the reset status was never differentially
validated, and envoy-rust already returns 503 on the sibling connect-failure +
overflow paths) and (b) DERIVES `%RESPONSE_FLAGS%` = `UC` from a new per-request
`reset_for_log` boolean — set post-loop when the FINAL attempt's `AttemptOutcome`
is `Reset` (a reset retried to success has `final_outcome = Some(Response)` → NOT
flagged) — at the H1 record-build site. `UC` is the THIRD flag NOT derivable from
`%RESPONSE_CODE_DETAILS%` (the reset rcd is the shared `via_upstream`, just as the
phase-51 `URX` and phase-52 `UF` rcds are), so it keys on the boolean, not the
rcd — reusing the phase-51/52 boolean-discriminator mechanism (and the
`final_outcome` capture phase 52 already added — no new loop state). The branch
is additive (the `URX`/`UF`/`NR`/`UH`/`UO` arms are preserved verbatim →
fixtures 0056/0057/0058/0059/0060 stay byte-identical).

The assertion is **pure cross-proxy equality** — there is NO static expected
literal. The reset synth-503 is deterministic on both sides, so the byte-exact
driver covers the line.

## The `json_format` map (upstream-reset route)

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
| `rf`  | `%RESPONSE_FLAGS%`        | `UC`                                                 |

**`%RESPONSE_CODE_DETAILS%` is OMITTED.** Unlike the connect-failure rcd (0060,
non-deterministic), the reset rcd
`upstream_reset_before_response_started{connection_termination}` is a FIXED
reset-reason enum and IS deterministic — but witnessing it is DEFERRED (M53-1) to
keep this witness minimum-viable (one flag per phase, the 48–52 cadence) and to
avoid an extra `%RESPONSE_CODE_DETAILS%` reconciliation (envoy-rust's reset arm
emits the shared `via_upstream` today, NOT `connection_termination`). The
`http1_access_log_byte_exact` driver does **not** compare the response body
(`AccessLogByteExactProbe` has no `expected_body` field), so the body is never
witnessed.

`%RESPONSE_CODE%` renders the bare json NUMBER `503` (not a quoted string) —
precedent fixture `0047-accesslog-json-nested`. Keys sort by UTF-8 byte order
(ADR-0094 §A): rc, rf; compact separators + ONE trailing `\n` (ADR-0092 §E).

## Probe

| # | request                    | emitted JSON object (byte-identical on both sides) |
|---|----------------------------|----------------------------------------------------|
| 1 | `GET /` (no extra headers) | see below                                          |

```
{"rc":503,"rf":"UC"}
```

A single probe — the upstream-reset path is a single synth-503 arm.

## The upstream-reset trigger (endpoint DIALED, connect completes, then FIN)

`backend_cluster` is `STRICT_DNS` ROUND_ROBIN with **NO `circuit_breakers` and
NO `retry_policy`** and ONE endpoint = the SPAWNED accept-then-close backend at
`{{BACKEND_HOST}}:{{CLOSE_BACKEND_PORT}}`. On the first `GET /`, both proxies
**DIAL** the backend, the TCP connect **completes** (UNLIKE the connect-failure
path of 0060, where the connect is refused), the upstream **reads the request
then drops the socket** (a graceful FIN with no response) → the post-connect
reset synth-503 (`rf:"UC"`).

The **read-then-close** posture (the backend drains the request bytes before
closing) GUARANTEES both proxies classify the event POST-connect
(`Reset`/`UC`), never PRE-connect (`ConnectFailure`/`UF`): the kernel completes
the handshake before the app close, so the first send/recv fails post-connect →
the reset arm.

## Per-side divergences

| Side       | bind address | admin block | access-log path                          | `{{BACKEND_HOST}}`     |
|------------|--------------|-------------|------------------------------------------|------------------------|
| envoy      | `0.0.0.0`    | yes (port 0)| `/tmp/0061-envoy-mount/access.log`       | `host.docker.internal` |
| envoy-rust | `127.0.0.1`  | omitted     | `/tmp/0061-envoy-rust-mount/access.log`  | `127.0.0.1`            |

The upstream Envoy container reaches the host-running backend via
`host.docker.internal:<port>`; envoy-rust on the host reaches the SAME
accept-then-close process via `127.0.0.1:<port>` (ADR-0015, the STRICT_DNS
`TcpProxyBackend` precedent of 0003/0004). Because the asserted line is
`{rc,rf}`-only (NO `%UPSTREAM_HOST%`), the per-side `{{BACKEND_HOST}}` divergence
never appears → byte-identity holds. The cluster + route + `json_format` are
otherwise BYTE-IDENTICAL across the two files. The access-log parent directory is
bind-mounted from the host into the Envoy container so the harness can read the
file after the request completes.

## Driver

`kind: http1_access_log_byte_exact` (same driver as fixtures
0040/0046/0053/0056/0057/0058/0059/0060) — drives the probe, asserts each side's
status == `expected_status` (503), scrapes both files, asserts the scraped line
count equals `probes.len()`, and calls
`access_log::assert_access_log_lines_byte_identical`. It does NOT compare the
downstream response body. The backend spawn is PURELY marker-driven: the
`{{CLOSE_BACKEND_PORT}}` marker in the cluster endpoint triggers the
`TcpCloseBackend` launch arm in `run_fixture` (`tests/differential/src/lib.rs`);
no per-driver backend allowlist.

## Cross-references

- ADR: ADR-0110 (phase-53 pick + scope — witness the SIXTH non-`-`
  `%RESPONSE_FLAGS%` value `UC` byte-exact on the upstream-disconnect-before-
  headers 503 path, by correcting the reset synth status 502→503 and deriving
  `"UC"` from the reset final-outcome boolean).
- Related fixtures: 0060 (`%RESPONSE_FLAGS%` = `UF`, the phase-52 sibling whose
  boolean-discriminator derive mechanism + `final_outcome` capture this reuses),
  0059 (`URX`), 0058 (`UO`), 0057 (`UH`), 0056 (`NR`), 0052 (the backend-
  spawning access-log precedent), 0047 (the `%RESPONSE_CODE%` bare-json-NUMBER
  precedent), 0012 (default access-log baseline).
- Deferred: the deterministic `UC` `%RESPONSE_CODE_DETAILS%`
  `upstream_reset_before_response_started{connection_termination}` (M53-1 — the
  SIXTH `%RESPONSE_CODE_DETAILS%` witness, the FIRST deterministic upstream-reset
  rcd; envoy-rust's reset arm emits the shared `via_upstream` today), the H2
  reset path (M45-1: H2 record-build site hard-codes `"-"`; no H2 access-log
  differential driver), and the `DC` flag (M45-2, timing-dependent downstream
  disconnect). `UC` now CONSUMES M52-1.
