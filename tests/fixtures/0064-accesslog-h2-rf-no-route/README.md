# Fixture 0064 — H2 access-log `%RESPONSE_FLAGS%` no-route failure path (`NR`, byte-exact)

The **FIRST H2 access-log differential fixture** in the project (phase 56,
ADR-0113) — opens `Driver::Http2AccessLogByteExact`, the H2 sibling of the
H1-only `Driver::Http1AccessLogByteExact` (fixtures 0040/0046-0055/0058-0063).
The H2 analogue of fixture 0056 (phase 48): witnesses the FIRST H2
`%RESPONSE_FLAGS%` value, `NR` (NoRoute), byte-exact cross-proxy on BOTH the
route-miss and host-miss `synth_404` arms.

## What this proves

`rc`/`rcd`/`proto`/`method` were ALREADY byte-identical between envoy-rust
and live Envoy on H2 for this trigger before this phase (state-0/state-2
recon) — `response_code_details_for_log_h2` has been correctly set to
`Some("route_not_found")` on both no-route arms since phase 42/43
(ADR-0099/ADR-0100). The ONLY prior gap was `%RESPONSE_FLAGS%`, hard-coded
`"-"` at the H2 record-build site. Phase 56 derives it: `"NR"` when
`%RESPONSE_CODE_DETAILS%` is `route_not_found`, else `"-"` — the H2 mirror
of the H1 phase-48 one-arm derive at its ORIGINAL scope
(`crates/envoy-http1/src/hcm.rs:1377` as it stood before phases 49-54 each
added one more arm).

## Probes

| # | request (H2, `:authority` = Host)           | arm        | emitted JSON object (byte-identical on both sides) |
|---|-----------------------------------------------|------------|------------------------------------------------------|
| 1 | `GET /nomatch` with `:authority: match.test`   | route-miss | see below                                             |
| 2 | `GET /specific` with `:authority: nomatch.test`| host-miss  | see below                                             |

```
{"method":"GET","proto":"HTTP/2","rc":404,"rcd":"route_not_found","rf":"NR"}
```

The route table is the IDENTICAL shape fixture `0056` uses (a single vhost
`domains: ["match.test"]`, one `/specific` `direct_response` route) — only
`codec_type: HTTP2` + `http2_protocol_options: {}` differ. `clusters: []` —
no upstream, no backend spawn, no `{{BACKEND_IP}}`/`{{HTTP1_BACKEND_PORT}}`
machinery needed.

## Driver

`kind: http2_access_log_byte_exact` (new this phase — `Driver::Http2AccessLogByteExact`,
`tests/differential/src/lib.rs`). Drives each probe over H2-prior-knowledge
via `drive_http2`, scrapes both files, asserts the scraped line count equals
`probes.len()` (here 2), and calls
`access_log::assert_access_log_lines_byte_identical` — the exact same
assertion machinery `http1_access_log_byte_exact` uses; only the wire driver
(`drive_http2` vs `drive_http1`) differs.

## `0001`-`0063` byte-preservation

This phase's H2 `response_flags` derive change is additive — gated on
`response_code_details_for_log_h2 == Some("route_not_found")`, which NO
existing H2 fixture (`0009`, `0010`, `0018`, `0021`) triggers (none of them
even carries an `access_log` block). `Driver::Http2AccessLogByteExact` is a
brand-new variant no pre-existing fixture references. So all `0001`-`0063`
stay byte-identical; only the new `0064` observes the changed value.

## Cross-references

- ADR: ADR-0113 (state-1 brainstorm + state-2 PLAN — opens the H2
  access-log differential driver + the H2 `NR` witness).
- Related fixtures: `0056` (the H1 `NR` witness this fixture mirrors on H2).
- New carry-forward: **M56-1** — the remaining H2 `%RESPONSE_FLAGS%` values
  (`UH`/`UO`/`URX`/`UF`/`UC`) + the H2 failure-path `%RESPONSE_CODE_DETAILS%`
  strings beyond `route_not_found`, now unblocked for future one-flag-at-a-time
  phases (the same cadence phases 49-54 used for H1 after phase 48).
