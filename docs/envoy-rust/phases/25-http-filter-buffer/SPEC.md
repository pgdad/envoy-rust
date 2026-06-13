# Phase 25 (`25-http-filter-buffer`) — SPEC

- **Phase id:** `25`
- **Slug:** `25-http-filter-buffer`
- **Family:** HTTP filters (§9) — the SEVENTH concrete HTTP-filter-family phase (after 09 `local_ratelimit` / 10 `rbac` / 11 `fault` / 22 `jwt_authn` / 23 `cors` / 24 `csrf`).
- **Depends-on:** `04` (HTTP/1.1 HCM + router) · `05` (HTTP/2) · `07` (filter framework) · `23` (per-route `typed_per_filter_config` infrastructure).
- **Lifecycle:** state-1 brainstorm output. `PLAN.md` / `PROGRESS.md` / `REVIEW.md` do not exist yet (the state-2 PLAN-write authors `PLAN.md` next session).
- **Scoping ADR:** ADR-0062 (lands at THIS SPEC commit). ADR-0063 reserved for the state-2 §6.2 reconciliation; ADR-0064 reserved for the §6.1 split (LIKELY to fire — see §0/§6.1).

---

## 0. Critical scoping finding (READ FIRST) — buffer is NOT a pure plug-in; it has a real foundation slice (H1 request-body forwarding)

Unlike phases 23/24 (cors/csrf), which were near-pure additive plug-ins on top of already-landed infrastructure, **`buffer` is the first HTTP filter that depends on the request BODY**, and a read-only reconnaissance at the brainstorm HEAD (`7243d381f`) established that the request-body data path is **asymmetric and partly absent** today:

1. **H1 does NOT forward request bodies upstream.** In `crates/envoy-http1/src/hcm.rs` the router arm builds the upstream request with `body: Some(Bytes::new())` — **always empty** (`:356`, comment: *"Chunked-request-body forwarding is a SPEC §4 non-goal"*) — and **drains-and-discards** the downstream Content-Length-delimited body into a throwaway buffer AFTER the response is built (`:678-697`). The `FilterRequest.body` handed to the pipeline is `req.body.take()` (`:635`), which is `None` on H1 (the body has not been read at that point). So envoy-rust currently **cannot proxy an H1 POST/PUT body to an upstream at all** — a pre-existing functional gap carried since phase 04.3.
2. **H2 ALREADY buffers and forwards request bodies.** In `crates/envoy-http2/src/hcm.rs` the router arm fully drains the H2 `RecvStream` DATA frames into a `BytesMut` BEFORE the pipeline runs (`:437-448`), exposes it as `FilterRequest.body: Some(Bytes)` (`:473`), and forwards it upstream (replay-safe across retries). So on H2 the full request body is already available to a filter at `decode_headers` time, and it already reaches the upstream.
3. **The filter framework is HEADERS-ONLY.** `HttpFilterInstance` / `FilterPipeline` expose only `decode_headers` + `encode_headers` returning `Decision { Continue, StopAndSend(FilterResponse) }` (`crates/envoy-filter/src/pipeline.rs:11-15,76-84`; `instance.rs:138`). There is **no streaming `decode_data` hook**. For a minimum-viable buffer (whole-body, Content-Length-delimited) **none is needed** — the entire body is available as `FilterRequest.body` once it has been read, so the filter can length-check it in `decode_headers`.

**Consequence — the two-part shape.** Because the differentially-observable buffer *happy path* (a body within the limit reaches the upstream and is echoed) is only visible if the body actually reaches the upstream, phase 25 must FIRST close the H1 request-body-forwarding gap (Part A — a foundation slice in the 07.1 / 12.1 / 14.1 / 23.1 mold), THEN add the filter (Part B). This makes the **§6.1 split LIKELY** (vs. the "nominal reserve" of phases 23/24): projected `25.1` (H1 request-body buffering + forwarding through the pipeline; regression-equivalence, no new fixture) + `25.2` (`BufferFilter` + `BufferPerRoute` + stats + fixture `0033` + close). The PLAN-writer decides at state-2 AFTER the §6.2 verification (§6.1).

**Reuse facts established by the recon (SPEC §0 lineage, the ADR-0060 §0 precedent):**
- The per-route plumbing is fully landed: `PerFilterConfig` `@type`-tagged enum (`crates/envoy-config/src/bootstrap.rs:791`, variants `Cors`+`Csrf`), `Route.typed_per_filter_config` (`:1247`) + its hand-rolled deserializer (`:1413-1515`), `FilterPipeline::apply_route_config` (`pipeline.rs:66`) → `instance.rs:180-190` → the per-filter `apply_route_config` (`cors.rs` / `csrf.rs` precedent), and the `PerRouteConfigForAbsentFilter` all-fatal validator (`bootstrap.rs:2728-2735`). Buffer is the THIRD `PerFilterConfig` consumer.
- `HttpFilterInstance` (`instance.rs:32-77`) carries 8 variants (Router/HeaderMutation/LocalRateLimit/Rbac/Fault/JwtAuthn/Cors/Csrf); `build` dispatches over `HttpFilterTypedConfig` (`instance.rs:103-136`). Buffer slots in as the ninth variant.
- The H1 `decorate_filter_synth_response` (`hcm.rs:1454-1499`) and H2 `decorate_filter_synth_response_h2` (`envoy-http2/src/response.rs:70-110`) helpers stamp `content-length`/`server`/`date`(/`connection` H1) + `content-type: text/plain` (only-if-missing, only-if-non-empty-body) — a buffer 413 reuses them verbatim (the rbac/csrf precedent).
- The `http1-echo-server` helper (`tests/helpers/http1-echo-server/src/main.rs:149-244`) reads and **echoes the request body** (`body: <BODY>`), so the happy-path body-forwarding is differentially observable once Part A lands. The latest fixture is `0032-http-filter-csrf`; the next is **`0033`**.
- Conditional per-filter stats register via `registry.register_counter(&format!("http.{hcm_stat_prefix}.<filter>.<suffix>"))` in `build_from_config` (the csrf precedent, `csrf.rs:85-105`); `ConfigError` variants live in `crates/envoy-config/src/lib.rs`.

The **§6.2 EMPIRICAL verification against `envoyproxy/envoy:v1.33.0`** (the exact 413 body bytes, the `Buffer` / `BufferPerRoute` wire shapes, any buffer stat namespace) is DEFERRED to the state-2 PLAN-write per the ratified verify-at-PLAN-write discipline (it RUNS LOCALLY — buffer has no virtiofs/inotify dependency). This brainstorm locks the pick + scope, which are decisions, not empirically-discoverable wire shapes.

---

## 1. Goal and acceptance signal

Implement **`envoy.filters.http.buffer`** (minimum-viable) — the buffer filter accumulates the FULL request body before the request proceeds, enforcing `max_request_bytes`; an over-limit body is rejected with a 413 local reply, and a within-limit body proceeds (and, with Part A, reaches the upstream). The filter is per-route-configurable via `BufferPerRoute` (disable for a route, or override the per-route limit), making it the THIRD consumer of the phase-23 per-route `typed_per_filter_config` infrastructure.

**Acceptance signal (the §7.5 phase-done gate):** fixture **`0033-http-filter-buffer`** green on an **H1 listener** proxying to a real `http1-echo-server` cluster (per the ADR-0058 L6 real-upstream constraint — a within-limit request must reach an upstream to yield a body-echoing 200), with **all 33 Docker-gated fixtures (`0001`–`0033`) green simultaneously** on Linux CI (the AUTHORITATIVE differential evidence per ADR-0049). The fixture drives deterministic, header+body, zero-timing/zero-crypto probes (projected; finalized at §6.2):
1. POST body ≤ `max_request_bytes` → **200** + body echoed by upstream (PROVES H1 request-body forwarding).
2. POST body > `max_request_bytes` → **413** local reply (Envoy's payload-too-large body, §6.2-verified byte-exact).
3. POST body > limit on a route whose `BufferPerRoute` **disables** the filter → **200** + body echoed (per-route override exercised).
4. POST body > a route's `BufferPerRoute`-**lowered** limit → **413** (per-route limit override exercised).
5. GET (no body) → **200** passthrough (no buffering effect; §6.2-confirm the no-body disposition).

---

## 2. Behavior-contract scope for phase 25

### 2.1 "Stat-name mapping" extension (projected; §6.2-verified)

Conditional per-HCM stats under `http.<stat_prefix>.buffer.*`, registered only when the buffer filter is present (the csrf/cors inert-when-unconfigured precedent). The exact name set is **§6.2-verified** at the PLAN-write — Envoy's buffer filter has a minimal stat surface; the projected candidates are a request-buffered counter and an over-limit/`rq_too_large` counter. If Envoy exposes **no** buffer-scoped stats, this row is dropped and the phase asserts only the data-plane 200/413 split (the §6.2 finding governs).

### 2.2 "Local reply" extension — the 413 over-limit reply (§6.2-verified)

A new BEHAVIOR_CONTRACT row for the buffer over-limit local reply: status **413**, body bytes + `content-type` **§6.2-verified byte-exact** against `envoyproxy/envoy:v1.33.0` (the rbac `"RBAC: access denied"` / csrf `"Invalid origin"` byte-exact precedent). The existing `decorate_filter_synth_response{,_h2}` helpers reproduce Envoy's framing.

### 2.3 "Request body forwarding" extension (Part A)

A new BEHAVIOR_CONTRACT note: H1 now forwards Content-Length-delimited request bodies upstream (closing the phase-04.3 non-goal for the bounded-body case); chunked/streaming request bodies remain a recorded non-goal (§4).

### 2.4 DECISIONS.md — ADR-0062 lands at THIS SPEC commit; ADR-0063 / ADR-0064 reserved

ADR-0062 (scoping) lands now. ADR-0063 is reserved for the state-2 §6.2 reconciliation (lands inline if the 413 body / `Buffer` shape / `BufferPerRoute` shape / stat namespace diverges from this projection). ADR-0064 is reserved for the §6.1 split (LIKELY to fire — Part A is a genuine foundation slice).

---

## 3. Deliverables

### D1 — H1 request-body forwarding (Part A; the foundation slice; projected `25.1`)

Close the pre-existing H1 gap so the Content-Length-delimited downstream request body is read into a `Bytes`, made available to the filter pipeline as `FilterRequest.body`, and forwarded upstream:
- Read the body BEFORE the upstream request is dispatched (relocating the drain at `hcm.rs:678-697` to a buffer-into-`Bytes` step), populate `FilterRequest.body` (`:635`), and forward it as the upstream `out_req.body` instead of `Some(Bytes::new())` (`:356`).
- **Regression-equivalence is the load-bearing invariant:** all 32 existing Docker-gated fixtures stay green simultaneously. Body-forwarding is transparent for fixtures whose upstreams ignore the body; the H1 echo-server fixtures now actually receive it. Preserve the existing `Connection`/`Transfer-Encoding` strip (`:344-348`), the content-length parse (`:597`), the chunked-request rejection (501, `:656`), and the connection-pool reuse / `Connection: close` single-use semantics (the phase-23 ADR-0059 correction).
- **Chunked/streaming request bodies remain a non-goal** (Content-Length-delimited only) — the minimum-viable boundary; recorded in §4.

### D2 — `envoy-filter::BufferFilter` runtime (Part B; projected `25.2`)

A new `crates/envoy-filter/src/buffer.rs` module — the NINTH `HttpFilterInstance` variant (`instance.rs:32`):
- **Decode-side only.** In `decode_headers`, with the full body available as `FilterRequest.body` (populated by D1 on H1, by the codec on H2): if `body.len() > effective_max_request_bytes` → `Decision::StopAndSend(FilterResponse { status: 413, … })` decorated by the existing H1/H2 filter-synth helpers; else `Decision::Continue` (the buffered body flows upstream).
- The effective limit is the per-route `BufferPerRoute` override if present (else the chain-level `Buffer.max_request_bytes`); a per-route `disabled: true` bypasses the filter entirely. The per-route policy flows in via the existing `apply_route_config` threading (the `cors.rs`/`csrf.rs` precedent — NO HCM change).
- Conditional `http.<stat_prefix>.buffer.*` stats per §2.1 (namespace + set §6.2-verified), registered in `build_from_config`.

### D3 — Config schema + the third `PerFilterConfig` variant + chain-level variant (§6.2-verified shapes)

In `crates/envoy-config/src/bootstrap.rs`:
- `Buffer { max_request_bytes: u32 }` chain-level config on `HttpFilterTypedConfig` (`:741`) — the exact field name/requiredness §6.2-verified.
- `BufferPerRoute` as the third `PerFilterConfig` variant (`:791`, after `Cors`+`Csrf`) — Envoy's `BufferPerRoute` is a oneof `{ disabled: bool, buffer: Buffer }`; the exact shape §6.2-verified.
- All `#[serde(deny_unknown_fields)]`. New `ConfigError` variant(s) as needed (e.g. an invalid/zero `max_request_bytes` disposition — §6.2-verified). Reuse the `PerRouteConfigForAbsentFilter` all-fatal validator verbatim.

### D4 — `HttpFilterInstance::Buffer` variant + dispatch + build

Add the `Buffer(BufferFilter)` arm to `HttpFilterInstance` (`instance.rs:32`), the `build` dispatch over `HttpFilterTypedConfig::Buffer` (`instance.rs:103-136`), and the `apply_route_config` dispatch arm (`instance.rs:180-190`).

### D5 — Stats wiring + BEHAVIOR_CONTRACT extension

Wire the §2.1 stats (if any) and land the §2.2/§2.3 BEHAVIOR_CONTRACT rows at the Task where each is first empirically exercised (the 06.x → 23/24 contract cadence).

### D6 — Fixture + harness + fuzz seed + in-process backstop

- Fixture `0033-http-filter-buffer` (H1 listener → real `http1-echo-server` cluster; the 5 probes of §1). A harness extension to send an **H1 POST with a body** (the recon found no built-in POST-body driver — extend the `Http1`/`Http1ProbeList` driver to carry a request body; bounded).
- A new `parse_bootstrap` fuzz seed for the `Buffer`/`BufferPerRoute` config surface (NO new fuzz target — buffer introduces no new parser crate; it reuses the bootstrap parser).
- An in-process backstop (both the within-limit forward path and the over-limit 413 path; heeds the phase-10 M1 lesson).

---

## 4. Out of scope (deferred non-goals)

Each is either rejected by `deny_unknown_fields` or simply out of this phase's minimum-viable cut:
- **Chunked / streaming request bodies** (Content-Length-delimited only this phase) — the H1 forwarding foundation handles bounded bodies; streaming decode-data iteration is a future phase.
- **The generic streaming `decode_data` framework hook** — not needed for whole-body buffering; deferred to the first filter that genuinely transforms a streamed body (e.g. `compression`).
- **Response / encode-side buffering** (Envoy's buffer is request-side; encode-side is not a separate Envoy feature here).
- **Vhost-level `BufferPerRoute`** + the route>vhost `mostSpecificPerFilterConfig` precedence cascade (phase 23 already deferred vhost-level config).
- **`typed_per_filter_config` for filters OTHER than cors/csrf/buffer** — the mechanism is general; only those three are wired/tested.
- **gRPC / trailers** interplay; **per_try_timeout / retry** interplay beyond what already exists.

---

## 5. Architectural invariants

### 5.1 No new crate
The config lives in `envoy-config`, the filter in `envoy-filter`, the H1 forwarding in `envoy-http1`. No new crate (the cors/csrf precedent; buffer reuses the bootstrap parser and the existing helpers).

### 5.2 Hand-rolled filter per D-3.2
`BufferFilter` is hand-rolled; no new third-party dependency.

### 5.3 Decode-side-only filter
Like csrf, buffer is decode-side only — a length check + 413 short-circuit. No encode-side behavior.

### 5.4 Part A is a regression-sensitive cross-cutting change; Part B is additive
D1 (H1 body forwarding) touches the H1 router data path that every H1 fixture exercises — its invariant is simultaneous green of all 32 existing fixtures (the foundation-slice pattern). D2-D4 (the filter) are purely additive (an enum arm + a module), inert when the buffer filter is unconfigured.

### 5.5 Determinism across both proxies (differential-testability invariant)
Buffer is a pure function of request method + body length + the policy — header+body, no clock, no crypto. Fully differential-testable byte-exact.

### 5.6 H1-first; H2 inherits for free
The fixture is H1 (the project norm; the foundation slice is H1-specific). H2 already buffers+forwards, so `BufferFilter` works on H2 with no extra plumbing; an H2 fixture is NOT required this phase (an H2 buffer fixture is a recorded optional follow-up, not a gate).

---

## 6. Implementation signposts for the planner

### 6.1 Split-gate evaluation (read first)
Projected **two-part** (~1300–2000 LoC / ~10–13 tasks): Part A (D1 — H1 body forwarding + regression-equivalence) is a genuine foundation slice, Part B (D2-D6 — the filter + fixture + close) is additive. **The §6.1 split is LIKELY to fire** into `25.1` (D1; no new fixture; all 32 existing fixtures green) + `25.2` (D2-D6; `BufferFilter` + `BufferPerRoute` + stats + fixture `0033` + parent-25 close), with the split ADR being **ADR-0064**. The PLAN-writer makes the call AFTER the §6.2 verification refines the LoC estimate (if Part A proves lighter than feared, a single phase is possible). This is the 07.1/07.2, 12.1/12.2, 14.1/14.2, 23.1/23.2 precedent.

### 6.2 Empirical verification at state-2 PLAN-write (the ratified verify-at-PLAN-write discipline)
Run LOCALLY against `envoyproxy/envoy:v1.33.0` (buffer has no virtiofs/inotify dependency; the phase-22/23/24 §6.2-local methodology). The checklist (gates the listed deliverable):
1. **The 413 over-limit local reply (gates D2/§2.2):** exact status (413 vs 431?), body bytes (`xxd`), `content-type`, `content-length` — byte-exact.
2. **The `Buffer` config shape (gates D3):** field name (`max_request_bytes`), type, requiredness; the chain-level `@type` URL.
3. **The `BufferPerRoute` shape (gates D3):** the oneof `{ disabled, buffer }` field names; whether `disabled: true` is the only per-route disable form; the per-route `@type` URL.
4. **The buffer stat namespace (gates D5/§2.1):** which `http.<prefix>.buffer.*` counters (if any) Envoy emits, and their semantics.
5. **The no-body / GET disposition (gates D6 probe 5):** does buffer tick/affect a no-body request, or pass it through untouched.
6. **The `==`-limit boundary (gates D2):** is the reject `>` or `>=` `max_request_bytes`.
7. **The per-route-for-absent-filter reuse (gates D3):** confirm `PerRouteConfigForAbsentFilter` covers a `buffer` per-route config whose `buffer` filter is absent from the chain.
8. **The real-upstream fixture constraint (gates D6, ADR-0058 L6):** confirm a within-limit buffered request must reach a real upstream to 200 (a `direct_response` route does not engage per-route filter config / body forwarding).

### 6.3 The 06.x stats convention + 07.x BEHAVIOR_CONTRACT cadence
Stats register conditionally in `build_from_config`; BEHAVIOR_CONTRACT rows land at the Task where each is first exercised.

### 6.4 In-process backstop assertion (heeds the phase-10 M1 lesson)
The backstop exercises BOTH the within-limit forward path (body reaches the in-process upstream) AND the over-limit 413 path, plus the per-route disable + per-route limit-override paths.

### 6.5 State-4 evidence discipline + isolated-crate build
Per `project_isolated_crate_build_blindspot`: run `cargo build -p envoy-config -p envoy-filter -p envoy-http1` in addition to `--workspace`. Per `feedback_state4_runs_docker_differential`: the state-4 gate runs the full Docker differential LOCALLY (pre-build `--no-run`), with the AUTHORITATIVE evidence the Linux CI anchor (ADR-0049). Mind the fixture flake family (`project_flaky_access_log_fixture_0012`).

### 6.6 PROGRESS.md skeleton + Task-1 preamble land alongside PLAN.md at state-2; subagent-driven execution at state-3
Per `feedback_execution_style` the state-3 implementation is subagent-driven; per `feedback_serial_subagent_dispatch` dispatch implementers SERIALLY (they race on shared `main`).

---

## 7. ADR posture

- **ADR-0062** (scoping) — lands at THIS SPEC commit: the pick (`buffer`, the third per-route consumer + the first body-dependent filter), the two-part scope (H1 forwarding foundation + the filter), the alternatives-rejected analysis (LB family / network-filters / the heavier HTTP filters), and the deferral ledger.
- **ADR-0063** — reserved for the state-2 §6.2 reconciliation (most-likely trigger: the 413 body bytes, the `BufferPerRoute` oneof shape, or the stat namespace).
- **ADR-0064** — reserved for the §6.1 split (LIKELY to fire: Part A is a genuine foundation slice).
- **ADR-0014 REMAINS IN FORCE** (the YAML-native shim — buffer does not engage the xDS protos). **ADR-0028 REMAINS OPEN** — phase 25 does not engage it. ADR-0049's all-fatal config posture is the projected default for the new buffer validators.

---

## 8. State-machine signposts for the phase-25 state-2 session

The NEXT session is state-2 (`superpowers:writing-plans` scoped to THIS SPEC). It: (a) runs the §6.2 empirical verification LOCALLY against `envoyproxy/envoy:v1.33.0`, landing ADR-0063 inline if anything diverges; (b) authors `PLAN.md` + the `PROGRESS.md` skeleton + the Task-1 preamble; (c) evaluates the §6.1 split gate and, if it fires, splits into `25.1`/`25.2` (ADR-0064) + updates ROADMAP + STATE and stops; (d) flips ROADMAP row `25` `planned → in-progress` (invariant 4.1.3). Per §5.1 (one state per session) it does NOT begin implementation.

---

## 9. Commit message format (for state 6 of the phase-25 lifecycle)

```
phase 25: envoy.filters.http.buffer + H1 request-body forwarding [ADR-0062, ADR-0063, ...]

<summary — 1–3 sentences>

Differential surface: fixture 0033-http-filter-buffer green; all 33 Docker-gated fixtures green simultaneously.
Conformance: h2spec ≥95%; fuzz parse_bootstrap clean.
```

---

## 10. State-machine commit (this commit — phase-25 state-1 brainstorm close-out)

THIS commit is the phase-25 state-1 NEW-PHASE brainstorm (`BOOTSTRAP_PROMPT.md` §5 state 0→1; the phase-12…24 single-commit brainstorm precedent collapses both states). It touches exactly **5 docs files**, docs-only, no code:
1. **CREATE** `docs/envoy-rust/phases/25-http-filter-buffer/SPEC.md` (this file).
2. **MODIFY** `docs/envoy-rust/ROADMAP.md` (add row `25`, `status: planned`, beneath the "HTTP filters family" §9 heading).
3. **MODIFY** `docs/envoy-rust/DECISIONS.md` (append **ADR-0062**).
4. **MODIFY** `docs/envoy-rust/STATE.md` (top-pointer advance to active phase `25` state-1-complete / state-2-next + a `### Phase-25 state-1 brainstorm` Notes subsection; the prior AWAITING-NEXT-PLANNING top-section blocks RELOCATED to `STATE_HISTORY.md` per ADR-0035).
5. **MODIFY** `docs/envoy-rust/STATE_HISTORY.md` (the ADR-0035 relocations).

No production/test/fixture/PLAN/PROGRESS/REVIEW/BEHAVIOR_CONTRACT/Cargo change; no `unsafe`. The brainstorm commit is docs-only so the CI run is vacuous-green (the differential evidence remains the phase-24 state-4 CI anchor `27457698815` at code-HEAD `9b0e7b925`). Per §5.1 this session EXITS after this commit; the NEXT session writes `PLAN.md` (state 2).
