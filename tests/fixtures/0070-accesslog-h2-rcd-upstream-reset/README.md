# 0070 — accesslog H2 upstream-reset `%RESPONSE_CODE_DETAILS%`

Phase 65 (ADR-0122). Witnesses the **deterministic H2 upstream-reset
`%RESPONSE_CODE_DETAILS%`** — `upstream_reset_before_response_started{connection_termination}` —
byte-exact cross-proxy on the H2 upstream-disconnect-before-headers 503 path, and
proves `%RESPONSE_FLAGS%` = `UC` now derives **1:1 from that rcd** (the phase-64
boolean discriminator was retired). **Consumes carry-forward M64-1.**

The H2 analogue of fixture `0062` (phase 54, ADR-0111), which witnessed the
identical rcd string on the H1 path.

## Topology

An H2C listener (`codec_type: HTTP2`) routes `/` to `backend_cluster`, a
`STRICT_DNS` cluster whose upstream protocol is H2
(`typed_extension_protocol_options` → `explicit_http_config.http2_protocol_options`).
Its single endpoint is the harness-spawned **`Http2CloseBackend`** (marker
`{{H2_CLOSE_BACKEND_PORT}}`, auto-launched by `tests/differential/src/lib.rs`):
it completes a genuine H2 handshake, accepts the request stream, then drops the
responder **without responding** — an implicit `RST_STREAM`. There is **no**
`retry_policy` and **no** `circuit_breakers`, so the reset is the *final*
attempt's outcome.

The retry-exhausted-reset path (where the rcd deliberately STAYS `via_upstream`
and the flag renders `URX`) cannot be driven by this fixture; it is pinned by the
in-process backstop
`h2_retry_exhausted_reset_keeps_via_upstream_rcd_and_renders_urx`
(`crates/envoy-http2/src/hcm.rs`).

## Emitted line (byte-identical on both proxies)

Keys sort by UTF-8 byte order (ADR-0094 §A):

    {"method":"GET","proto":"HTTP/2","rc":503,"rcd":"upstream_reset_before_response_started{connection_termination}","rf":"UC"}

## Per-side deltas

Only the documented ones fixture `0069` already uses: the reference side binds
`0.0.0.0` and carries an `admin:` block; the subject side binds `127.0.0.1`.
`{{BACKEND_HOST}}` resolves to `host.docker.internal` (reference) and
`127.0.0.1` (subject). The log line omits `%UPSTREAM_HOST%`, so that divergence
is invisible and byte-identity holds.

## Determinism

`connection_termination` is a **fixed reset-reason enum**, not OS-derived text —
unlike the connect-failure rcd (M45-2), which is why fixtures `0060`/`0068` omit
`rcd` entirely. The same fixed-enum shape is already witnessed byte-exact by
`0058`/`0066` (`{overflow}`) and `0062` (`{connection_termination}`, H1).

## Local vs CI

This fixture spawns a backend → expect **LOCAL-RED** on dev hosts whose bridge
routing cannot reach a host-spawned backend from the Envoy container (see
`tcpclosebackend-ipv6-unreachable-host-flake` / the `0061`/`0062`/`0069`
precedent). **CI is authoritative.**

## Cross-references

- ADR: **ADR-0122** (the phase-65 pick + §A-§H scope).
- Related fixtures: `0069` (the H2 `UC` flag witness this completes — its
  emitted line is UNCHANGED, `rf:"UC"` now arriving via the rcd-match);
  `0062` (the H1 witness of the identical rcd string, phase 54);
  `0058`/`0066` (the H1/H2 `{overflow}` rcd witnesses whose derivation pattern
  `UC` now follows).
- Carry-forward: **M64-1 CONSUMED.** Both protocols now share the identical
  `%RESPONSE_FLAGS%` derivation split — `{NR, UH, UO, UC}` rcd-derived,
  `{URX, UF}` boolean-derived.
