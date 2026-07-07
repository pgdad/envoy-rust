# Fixture 0068 — H2 access-log `%RESPONSE_FLAGS%` connect-failure path (`UF`, byte-exact)

The H2 analogue of fixture `0060` (phase 52, the H1 `UF` witness) and the
FIFTH fixture built on `Driver::Http2AccessLogByteExact` (opened by phase 56,
fixture `0064`; extended by phase 57's `0065`, phase 58's `0066`, and phase
61's `0067`). Phase 63 (ADR-0120) witnesses the FIFTH H2 `%RESPONSE_FLAGS%`
value, `UF` (UpstreamConnectionFailure), byte-exact on the H2
upstream-connect-refused 503 path.

## What this proves

Before this phase, envoy-rust's H2 `AcquireOutcome::ConnectFailure` arm
emitted a generic `synth_h2_502()` — a genuine, previously-unvalidated status
divergence (upstream Envoy returns 503 here), the SAME class of bug H1's
phase 52 fixed for the H1 side. Phase 63 (i) corrects the status via a new
`synth_h2_connect_failure()` helper (503), (ii) declares a new per-stream
boolean discriminator set post-loop from a new loop-scoped final-outcome
capture (the H2 loop's `break` carries no outcome, mirroring the H1
`final_outcome` capture), (iii) threads it through `finalize_h2_stream`'s
sole call site, and (iv) extends the H2 `%RESPONSE_FLAGS%` derive with a
boolean-gated `UF` branch ordered AFTER `URX`.

## Probe

| # | request (H2, `:authority` = `envoy-rust.test`) | arm | emitted JSON object (byte-identical on both sides) |
|---|---|---|---|
| 1 | `GET /` | connect-failure (kernel-refused `127.0.0.1:1`) | see below |

```
{"method":"GET","proto":"HTTP/2","rc":503,"rf":"UF"}
```

The cluster is the `0066` shape (STATIC, H2-upstream via
`typed_extension_protocol_options`) **minus `circuit_breakers`** — the SAME
delta H1's `0060` applied to `0058`. Without the pending-gate, envoy-rust
DIALS the literal dead endpoint and the kernel refuses the connect,
triggering the `AcquireOutcome::ConnectFailure` arm (rather than `0066`'s
pre-connect `PoolError::PendingOverflow` reject).

## Driver

`kind: http2_access_log_byte_exact` (`Driver::Http2AccessLogByteExact`,
opened at phase 56) — NO harness driver change this phase. **NO
harness backend-wiring allowlist edit needed** (unlike phase 61's `0067`) —
the fixture's literal dead endpoint carries no `{{BACKEND_PORT}}` marker, so
`scan_needs_marker`'s `needs_backend` gate stays `false` automatically,
mirroring fixture `0060`'s (and `0066`'s) NO-backend-spawned simplicity.

## `0001`-`0067` byte-preservation

This phase's changes are additive — gated on the `AcquireOutcome::ConnectFailure`
arm, which requires a dead/refused endpoint reached via an ACTUAL dial (no
`circuit_breakers` pending-gate and no `pick()->None` short-circuit ahead of
it). NONE of the pre-existing H2 fixtures (`0009`, `0010`, `0018`, `0021`,
`0064`, `0065`, `0066`, `0067`) reaches this arm — re-confirmed by a fresh
`grep -n "circuit_breakers\|retry_policy\|127.0.0.1:1"` over each
`envoy-rust.yaml` this session (`0021`'s `circuit_breakers` gates a reachable
backend; `0065`'s `127.0.0.1:1` is excluded pre-dial by a subset-miss;
`0066`'s `circuit_breakers` pending-gate rejects pre-connect; `0067`'s
`retry_policy` drives a REAL always-503 upstream) — so `0001`-`0067` stay
byte-identical; only the new `0068` observes the new `rf:"UF"` witness.

## Cross-references

- ADR: ADR-0120 (state-1 brainstorm + state-2 PLAN — the H2 `UF` witness).
- Related fixtures: `0060` (the H1 `UF` witness this fixture mirrors on H2);
  `0064`/`0065`/`0066`/`0067` (the H2 `NR`/`UH`/`UO`/`URX` witnesses that
  opened/extended `Driver::Http2AccessLogByteExact`).
- Carry-forward: **M56-1** — ONLY `UC` remains open (the last H2
  `%RESPONSE_FLAGS%` value) + the H2 failure-path `%RESPONSE_CODE_DETAILS%`
  strings beyond `route_not_found`/`no_healthy_upstream`/`{overflow}`, still
  open for a future phase.
