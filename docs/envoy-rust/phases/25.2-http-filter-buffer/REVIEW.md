# Phase 25.2 (`25.2-http-filter-buffer`) — REVIEW

> State-5 code review (`superpowers:requesting-code-review`) over the full `25.2`
> implementation (`envoy.filters.http.buffer`). Two INDEPENDENT read-only review
> subagents dispatched SERIALLY (`feedback_serial_subagent_dispatch`): one for
> correctness/scope, one for tests/quality. Their findings are synthesized below.
> State-5 is read-only — this file authors the verdict; any fix would re-enter
> state-3 (none required). Context-isolated (D-3.4) — readable standalone.

- **Phase:** `25.2` — `envoy.filters.http.buffer` (Part B of parent `25`; the ADR-0064 split sibling of the closed `25.1`).
- **Scope locked by:** ADR-0062 (parent scope) · ADR-0063 (§6.2 wire contract: 413 `Payload Too Large` 17B / strict `>` / NO stats / `Buffer`+`BufferPerRoute` shapes / reuse `PerRouteConfigForAbsentFilter`) · ADR-0064 (split).
- **Review surface (full `25.2` git range):** base `41ae9a12d` (`ecc674a9d~1`, the `25.1` close / state-2 PLAN-write) → HEAD `132d32cec`. Implementation commits `ecc674a9d` (T1), `f84b13066`+`ad2c9a859` (T2+T3 co-land), `d7ea6f0fd` (T4), `61229f844` (T5), `446b32d0b` (T6), `b624b8288` (T7). +1348 / −49 across 19 files (production: `envoy-config/src/bootstrap.rs`+`lib.rs`, `envoy-filter/src/buffer.rs`+`instance.rs`+`lib.rs`, `envoy-http1/src/hcm.rs`; harness `tests/differential/src/lib.rs`; fixture `0033` + acceptance test; BEHAVIOR_CONTRACT + fuzz seed).
- **State-4 evidence relied upon (read-only — no gate re-run at state-5):** §7.5 gate GREEN — full local Docker differential + h2spec 173 passed / 0 failed (all 33 fixtures incl. `0033`), build/clippy/fmt/deny clean, `parse_bootstrap` fuzz 815,824 runs / 0 crash; AUTHORITATIVE Linux CI `27510477930` on `fde99b984` = SUCCESS (ADR-0049).

---

## Verdict

**Ready to merge: YES.** Both reviewers independently approved. The implementation matches the locked wire/behavior contract (ADR-0062/0063/0064) precisely; the workspace builds clean at HEAD and the buffer unit/config/pipeline/hcm/differential tests are green (corroborated by the state-4 gate + CI).

| Severity | Count |
| --- | --- |
| **Critical (must fix)** | **0** |
| **Important (should fix)** | **0** |
| **Minor (nice-to-have)** | **4** (all non-blocking; documented non-goals or doc/coverage polish — see carry-forwards) |

**No ADR fired** — the review surfaced no finding forcing a re-plan or contract change. ADR-0065 stays UNFIRED; ledger head remains ADR-0064.

---

## Strengths (synthesized from both reviewers)

- **Buffer arithmetic is correct and overflow-safe.** `buffer.rs:89` compares `body_len as u64 > u64::from(limit)` — strict `>`, computed in u64, so a body exceeding `u32::MAX` cannot truncate and falsely pass. Boundary pinned by `at_limit_continues_strict_gt` (5 > 5 false → Continue) and `zero_limit_rejects_any_nonempty_body`.
- **Body length is read from the actual buffered body, not a header.** On H1 the full Content-Length-delimited body is read into `req.body` *before* `pipeline.decode_headers` runs; on H2 the stream is fully drained before `FilterRequest` is built. So `req.body.len()` at decode time is the real body — the filter cannot be fooled by a lying `Content-Length`. Correct integration point.
- **413 wire shape matches the contract exactly.** Filter emits empty headers + 17-byte `Bytes::from_static(b"Payload Too Large")` + reason `"Payload Too Large"` (`buffer.rs:106-113`); the H1/H2 synth decorators stamp `content-type: text/plain` + `content-length: 17` downstream (only-if-missing) — the filter correctly does NOT hardcode them. Matches the rbac/csrf precedent.
- **`@type` × `deny_unknown_fields` is safe.** Both enums are internally tagged (`#[serde(tag = "@type")]`), so serde consumes `@type` for variant selection before the inner `Buffer`/`BufferPerRoute` struct deserializes; `deny_unknown_fields` never sees `@type`. Confirmed by the passing `buffer_chain_config_rejects_unknown_field` test.
- **Per-route override is wired correctly and not swallowed.** The explicit `Buffer` arm at `instance.rs:196` sits *above* the `_ => {}` catch-all (with a warning comment); `apply_route_config` (`buffer.rs:61-78`) distinguishes disabled / lowered-limit / empty-`{}`-falls-back-to-base, each with a dedicated test.
- **Config disposition fully tested.** Plain-int parses, `0` accepted, absent → fatal, negative → fatal, unknown-field → rejected (all-fatal posture, ADR-0049/0063).
- **Tests assert REAL behavior, not tautologies.** The 8 buffer decode-side tests pin status/reason/body-bytes/len-17 across all 4 dispositions; `buffer_pipeline_backstop_all_dispositions` drives the REAL `FilterPipeline::build_from_config → apply_route_config → decode_headers` path (not a hand-rolled filter). The M25.1-2 split test genuinely forces the multi-read `while remaining > 0` reassembly loop (head consumed → `from_buf = 0` → `remaining = 11`), and `h1_forwards_large_body_grows_on_demand` (10 KB > 4 KiB chunk) proves grow-on-demand past the bounded reservation. Each would FAIL if its production change were reverted.
- **The differential fixture has no false-green risk.** The 200-echo probes reach a real `http1-echo-server` (a 200 genuinely means body forwarded+echoed); the top-level `equivalence: response_body: byte_exact` compares echo bodies cross-proxy; the 413 probes assert `byte_exact: "Payload Too Large"` on BOTH proxies. Probe-4 (`/small`, 5>4, per-route lowered) and probe-2 (`/`, 13>10, chain base) exercise genuinely distinct dispositions. Per-side YAML asymmetry follows the 0031/0032 precedent so echo bodies are byte-equal.
- **Scope fidelity on the HCM bound.** `body_len.min(INITIAL_BODY_BUF_CAP)` bounds only the up-front reservation; for any body ≤64 KiB `.min` is a no-op (byte-identical allocation); the grow-on-demand loop is unchanged. NO new stats anywhere; no out-of-scope edits.
- **Quality.** No `unsafe` in the diff; `Effective { Disabled, Limit(u32) }` is a cleaner model than `Option<u32>`; `base_max` is used (no dead code); naming/structure mirror the csrf precedent; BEHAVIOR_CONTRACT 413 row + hex independently verified accurate; fuzz seed parses; `.gitignore` whitelist matches the cors/csrf pattern.

---

## Issues

### Critical (Must Fix)
None.

### Important (Should Fix)
None.

### Minor (Nice-to-Have) — carry-forwards

1. **[non-goal, no action] Over-limit bodies are fully read into memory before rejection.** `hcm.rs` reads the entire declared `Content-Length` body into a `BytesMut` before `decode_headers` can reject it, so the limit governs the *response* (413 vs 200), not peak memory — unlike Envoy's watermarked decode. The differential behavior is byte-identical (correct status + body), and this is an EXPLICITLY documented deferred non-goal (`SPEC.md:52`, `PLAN.md:32`, `hcm.rs` rationale comment — the effective per-route limit isn't known at the pre-pipeline read site). The 64 KiB reservation bound (M25.1-1) softens the acute reservation-amplification vector. **No change required this phase**; flag kept on the radar for any future streaming/`decode_data` follow-up.
2. **[doc precision] BEHAVIOR_CONTRACT "verified byte-exact against v1.33.0" phrasing.** Fixture `0033` is H1-only (`codec_type: HTTP1`); the H2 decode-side over-limit path (`decorate_filter_synth_response_h2`) is covered by in-process unit/pipeline tests, not differentially. The 413 row's "verified byte-exact … against envoyproxy/envoy:v1.33.0" reads as if both codecs were differentially verified. Consider narrowing to "verified byte-exact on H1; H2 covered by the in-process synth-decorator backstop." Doc precision only.
3. **[coverage nicety] No standalone `== effective route limit` unit assertion.** Strict-`>` at the chain base (`at_limit_continues_strict_gt`, 5==5 → Continue) and over a lowered route limit (`per_route_lowered_limit_rejects`, 5>4 → reject) bracket the behavior, but there is no `== route limit` pass case (e.g. limit 4, body 4 → Continue). Optional completeness.
4. **[coverage nicety] No differential at-limit (`==`) probe.** Fixture `0033` has within (5≤10) and over (13>10, 5>4) probes but no exactly-at-limit (body==10) probe against real Envoy. The strict-`>` boundary is pinned at the unit layer (the appropriate level); a differential `==` probe would lock it cross-proxy too. Nice-to-have.

---

## Recommendations

- Carry Minor findings 2–4 forward into the next phase's planning as optional polish (BEHAVIOR_CONTRACT phrasing tweak; `==` unit + differential boundary probes). None block the `25.2`/parent-`25` close.
- Minor finding 1 (full-body buffering before rejection) is the only architectural residual; revisit it if/when a streaming `decode_data` watermark path is planned. Differentially safe today.

---

## Process notes

- **State-5 is read-only** — no build/test gate re-ran (the state-4 §7.5 gate already proved green at `fde99b984`; CI `27510477930` SUCCESS). The reviewers READ the code + the state-4 evidence.
- **No new ADR.** ADR-0065 stays UNFIRED; ledger head remains ADR-0064. The wire contract is locked by ADR-0063 and the state-4 differential confirmed byte-equivalence — the review surfaced nothing forcing a re-plan.
- **Next (state-6 deterministic close-out, no skill — BOOTSTRAP §6.1 step 4):** advance STATE to `25.2` state-5-complete / state-6-next, then the close-out flips BOTH ROADMAP row `25.2` AND parent row `25` to `done` (parent closes — `25.1` already done, `25.2` is its last sub-phase), advances STATE to phase `26`, relocates superseded blocks to `STATE_HISTORY.md`, and carries the Minor recommendations into phase-26 planning.
