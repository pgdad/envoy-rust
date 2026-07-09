# Fixture 0069 — H2 access-log `%RESPONSE_FLAGS%` upstream-reset path (`UC`, byte-exact)

The H2 analogue of fixture `0061` (phase 53, the H1 `UC` witness) and the
SIXTH fixture built on `Driver::Http2AccessLogByteExact` (opened by phase 56,
fixture `0064`; extended by phase 57's `0065`, phase 58's `0066`, phase 61's
`0067`, and phase 63's `0068`). Phase 64 (ADR-0121) witnesses the SIXTH and
FINAL H2 `%RESPONSE_FLAGS%` value, `UC` (UpstreamConnectionTermination),
byte-exact on the H2 upstream-disconnect-before-headers 503 path — CLOSING
carry-forward M56-1 (no H2 `%RESPONSE_FLAGS%` value remains open).

## What this proves

Before this phase, envoy-rust's H2 `AcquireOutcome::Sent(Err(e))` arm emitted
a generic `synth_h2_502()` — a genuine, previously-unvalidated status
divergence (upstream Envoy returns 503 here), the SAME class of bug phases
52 (H1) / 57 / 63 (H2) each fixed for their own arms. Phase 64 (i) renames
`synth_h2_502()` → `synth_h2_reset()` in place and corrects the status
(503), (ii) declares a new per-stream boolean `reset_for_log_h2`, set
post-loop by reading the EXISTING `final_outcome_h2` capture (phase 63) a
SECOND time — no new loop-scoped state, (iii) threads it through
`finalize_h2_stream`'s sole call site, and (iv) extends the H2
`%RESPONSE_FLAGS%` derive with a boolean-gated `UC` branch ordered AFTER
`UF`.

**UNLIKE the H1 `UC` witness (fixture 0061, which reuses `TcpCloseBackend` — a
raw TCP accept-then-close backend), this fixture needs a NEW H2-protocol-aware
backend.** envoy-rust's own H2 client (`Client::connect`) folds the
TCP-connect and `h2::client::handshake` into one call with a 10 ms
handshake-failure-detection window; a raw accept-then-close backend (no H2
bytes at all) fails entirely inside that window, landing in the
ALREADY-FIXED `ConnectFailure`/`UF` arm — NOT the `Sent(Err(e))`/`UC` arm
this phase fixes. The fixture's backend (`http2-echo-server
--close-before-response`, via the NEW `Http2CloseBackend` harness struct)
instead completes a GENUINE H2 handshake, accepts the request stream, then
drops it without responding — confirmed (state-0 recon + this session's own
PLAN-write re-verification, both empirically) to drive BOTH envoy-rust's H2
client into `Sent(Err(e))`/`Reset` AND live upstream Envoy v1.33.0 into the
IDENTICAL `503`/`UC` disposition.

> **⚠ LOCAL-RED expected; CI is AUTHORITATIVE.** This fixture SPAWNS a
> backend, so it is subject to the host's Docker bridge-IP differential flake
> (memory `differential-host-bridge-ip-192-168-65-2`): expect LOCAL-RED on
> this dev host and GREEN on native-Linux CI.

## Probe

| # | request (H2, `:authority` = `envoy-rust.test`) | arm | emitted JSON object (byte-identical on both sides) |
|---|---|---|---|
| 1 | `GET /` | reset (handshake completes, stream reset before response) | see below |

```
{"method":"GET","proto":"HTTP/2","rc":503,"rf":"UC"}
```

## Driver

`kind: http2_access_log_byte_exact` (`Driver::Http2AccessLogByteExact`,
opened at phase 56) — NO harness driver change this phase. The backend spawn
is PURELY marker-driven: the `{{H2_CLOSE_BACKEND_PORT}}` marker in the
cluster endpoint triggers the `Http2CloseBackend` launch arm in `run_fixture`
(`tests/differential/src/lib.rs`) — a distinct marker from H1's
`{{CLOSE_BACKEND_PORT}}` (`TcpCloseBackend`), since the two backends are
fundamentally different (raw-TCP-close vs. genuine-H2-handshake-then-reset).

## `0001`-`0068` byte-preservation

This phase's changes are additive — gated on the `AcquireOutcome::Sent(Err(e))`
arm, which requires a backend that completes an H2 handshake then resets a
stream without responding. NONE of the pre-existing H2 fixtures (`0009`,
`0010`, `0018`, `0021`, `0064`-`0068`) reaches this arm — re-confirmed by a
fresh `grep -n "circuit_breakers\|retry_policy\|127.0.0.1:1"` over each
`envoy-rust.yaml` this session (`0021`'s `circuit_breakers` gates a reachable,
always-responding backend; `0065`'s `127.0.0.1:1` is excluded pre-dial;
`0066`'s `circuit_breakers` pending-gate rejects pre-connect; `0067`'s
`retry_policy` drives a REAL always-503 `Http2EchoBackend`, which always
responds; `0068`'s literal dead endpoint hits `ConnectFailure`, not
`Sent(Err)`) — so `0001`-`0068` stay byte-identical; only the new `0069`
observes the new `rf:"UC"` witness.

## Cross-references

- ADR: ADR-0121 (state-1 brainstorm + state-2 PLAN — the H2 `UC` witness,
  closing carry-forward M56-1).
- Related fixtures: `0061` (the H1 `UC` witness whose derive mechanism this
  mirrors, but NOT its `TcpCloseBackend` harness — see "What this proves"
  above); `0064`/`0065`/`0066`/`0067`/`0068` (the H2 `NR`/`UH`/`UO`/`URX`/`UF`
  witnesses that opened/extended `Driver::Http2AccessLogByteExact`).
- Carry-forward: **M56-1 CLOSED** — all six H2 `%RESPONSE_FLAGS%` values
  (`NR`/`UH`/`UO`/`URX`/`UF`/`UC`) are now witnessed, matching H1's own
  six-flag completion at phase 53. **NEW carry-forward M64-1** — the H2-side
  deterministic `UC` `%RESPONSE_CODE_DETAILS%`
  (`upstream_reset_before_response_started{connection_termination}`),
  deferred to keep this witness minimum-viable (mirrors H1's own deferred
  rcd at phase 53, later consumed by phase 54's M53-1 — M64-1 is the H2-side
  analogue, distinct and still open).

> **Update — phase 65 (ADR-0122): M64-1 is now CONSUMED.** Fixture `0070`
> witnesses the deterministic H2 reset `%RESPONSE_CODE_DETAILS%`
> (`upstream_reset_before_response_started{connection_termination}`) byte-exact
> on this same path, and H2's `UC` now derives **1:1 from that rcd** — the
> phase-64 boolean discriminator described under "What this proves" above was
> RETIRED. That prose is left verbatim as the historical record of phase 64
> (doctrine D-3.4/D-3.5: backward-looking narrative is never retroactively
> rewritten); it describes the phase-64 mechanism, not today's. **This
> fixture's own emitted line is UNCHANGED** — `rf:"UC"` is output-equivalent
> under the new rcd-derivation, which is exactly the byte-preservation the
> phase-65 additivity invariant requires.
