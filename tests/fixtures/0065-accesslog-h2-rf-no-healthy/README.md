# Fixture 0065 — H2 access-log `%RESPONSE_FLAGS%` no-healthy-upstream failure path (`UH`, byte-exact)

The H2 analogue of fixture `0057` (phase 49, the H1 `UH` witness) and the
SECOND fixture built on `Driver::Http2AccessLogByteExact` (opened by phase 56,
fixture `0064`). Phase 57 (ADR-0114) witnesses the SECOND H2
`%RESPONSE_FLAGS%` value, `UH` (NoHealthyUpstream), byte-exact on the H2
`pick()->None` no-healthy-upstream 503 path — AND corrects a genuine
differential-correctness bug found in the same motion (envoy-rust's H2
no-healthy arm previously returned a generic 502; Envoy returns 503).

## What this proves

Before this phase, envoy-rust's H2 `pick()->None` arm (`run_h2_attempt`,
`crates/envoy-http2/src/hcm.rs`) emitted the generic `synth_h2_502()` (status
502, empty body, `%RESPONSE_CODE_DETAILS%` = `null`, `%RESPONSE_FLAGS%` = `-`)
— a three-way divergence from live Envoy v1.33.0, which returns 503 + body
`no healthy upstream` + `rcd:"no_healthy_upstream"` + `rf:"UH"`. Phase 57 (i)
adds a dedicated `synth_h2_no_healthy_upstream()` helper (mirroring the H1
`synth_no_healthy_upstream` precedent) at the ONE `pick()->None` call site,
(ii) sets `response_code_details_for_log_h2 = Some("no_healthy_upstream")` in
the caller-loop's NEW `else` branch, and (iii) extends the phase-56 H2
one-arm `%RESPONSE_FLAGS%` derive to a two-arm match (`route_not_found` =>
`NR`, `no_healthy_upstream` => `UH`). All three trace to the SAME two code
sites the state-0 recon identified — no fourth divergence.

## Probe

| # | request (H2, `:authority` = `envoy-rust.test`) | arm | emitted JSON object (byte-identical on both sides) |
|---|---|---|---|
| 1 | `GET /` | `pick()->None` no-healthy | see below |

```
{"method":"GET","proto":"HTTP/2","rc":503,"rcd":"no_healthy_upstream","rf":"UH"}
```

The route/cluster table is the IDENTICAL shape fixture `0057` uses (a
`subset_cluster` STATIC cluster with `lb_subset_config` NO_FALLBACK, a single
route `metadata_match` selecting the non-existent `stage: nonexistent`
subset) — only `codec_type: HTTP2` + `http2_protocol_options: {}` (fixture
`0064`'s listener shape) are substituted for `0057`'s `codec_type: HTTP1`.

## Driver

`kind: http2_access_log_byte_exact` (`Driver::Http2AccessLogByteExact`,
opened at phase 56) — NO harness change this phase. Drives the probe over
H2-prior-knowledge via `drive_http2`, scrapes both files, asserts the scraped
line count equals `probes.len()` (here 1), and calls
`access_log::assert_access_log_lines_byte_identical`.

## `0001`-`0064` byte-preservation

This phase's changes are additive — gated on `cluster.pick_endpoint(...)`
returning `None`, which requires a `lb_subset_config`/NO_FALLBACK subset-miss
(or an empty CLA) on an H2 listener. NONE of the pre-existing H2 fixtures
(`0009`, `0010`, `0021`, `0064`) configures `lb_subset_config` — re-confirmed
by `grep -c lb_subset_config` over each `envoy-rust.yaml` returning `0`. So
`0001`-`0064` stay byte-identical; only the new `0065` observes the changed
status/rcd/rf.

## Cross-references

- ADR: ADR-0114 (state-1 brainstorm + state-2 PLAN — the H2 `UH` witness +
  the 502->503 reconciliation).
- Related fixtures: `0057` (the H1 `UH` witness this fixture mirrors on H2);
  `0064` (the H2 `NR` witness that opened `Driver::Http2AccessLogByteExact`).
- Reconciles: the pre-existing `BEHAVIOR_CONTRACT.md` note "the H2 no-healthy
  arm returns 502" (flagged in passing during phase 56's SPEC drafting) —
  investigated and FIXED this phase.
- Carry-forward: **M56-1** — the remaining H2 `%RESPONSE_FLAGS%` values
  (`UO`/`URX`/`UF`/`UC`) + the H2 failure-path `%RESPONSE_CODE_DETAILS%`
  strings beyond `route_not_found`/`no_healthy_upstream`, still open for
  future one-flag-at-a-time phases.
