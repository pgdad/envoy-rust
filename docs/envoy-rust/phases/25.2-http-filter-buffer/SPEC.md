# Phase 25.2 (`25.2-http-filter-buffer`) — SPEC

- **Phase id:** `25.2` (sub-phase of parent `25` — `envoy.filters.http.buffer`).
- **Slug:** `25.2-http-filter-buffer`.
- **Family:** HTTP filters (§9) — Part B (the filter) of parent phase `25`; flips parent `25` to `done` at its state-6 close.
- **Depends-on:** `23` (per-route `typed_per_filter_config` infrastructure) · `25.1` (H1 request-body forwarding — the body must be available as `FilterRequest.body` for the length check).
- **Lifecycle:** state-1-complete at the split commit (this `SPEC.md` lands; `PLAN.md` / `PROGRESS.md` / `REVIEW.md` do NOT exist yet — the `25.2` state-2 PLAN-write authors `PLAN.md` AFTER `25.1` closes).
- **ADRs:** scope ADR-0062 (parent); split ADR-0064; §6.2 reconciliation ADR-0063 (the wire shapes + the dropped stats are LOCKED — see §2/§3). Parent SPEC: `docs/envoy-rust/phases/25-http-filter-buffer/SPEC.md`.

---

## 0. Why this sub-phase exists

Part B of parent `25` (ADR-0064). With `25.1` having closed the H1 request-body-forwarding gap (the body is now available as `FilterRequest.body` on H1; already so on H2), `25.2` adds the **`envoy.filters.http.buffer`** filter: it length-checks the full request body against `max_request_bytes` and rejects an over-limit body with a 413 local reply, configurable per-route via `BufferPerRoute`. Buffer is the THIRD consumer of the phase-23 per-route `typed_per_filter_config` infrastructure (after cors + csrf).

**The §6.2 empirical verification has ALREADY RUN** (ADR-0063, at the split commit) and locked the wire shapes — see §2/§3. The `25.2` state-2 PLAN-write is therefore a straight PLAN-write (no further §6.2 recon owed, except the small residual of the absent/`0`/malformed `max_request_bytes` disposition — ADR-0063 Residual).

---

## 1. Goal and acceptance signal

**Goal:** implement `envoy.filters.http.buffer` (minimum-viable) — accumulate the full request body, enforce `max_request_bytes`; over-limit → 413 `Payload Too Large` local reply; within-limit → proceed (and, via `25.1`, reach the upstream). Per-route-configurable via `BufferPerRoute` (disable for a route, or override the per-route limit).

**Acceptance signal (§7.5 gate):** fixture **`0033-http-filter-buffer`** green on an **H1 listener** proxying to a real `http1-echo-server` cluster (per ADR-0058 L6 / ADR-0063 finding 8 — a within-limit request must reach a real upstream to yield a body-echoing 200), with **all 33 Docker-gated fixtures (`0001`–`0033`) green simultaneously** on Linux CI. The fixture drives 5 deterministic header+body probes (§6.2-confirmed dispositions, ADR-0063):
1. POST body ≤ `max_request_bytes` → **200** + body echoed by upstream (PROVES the `25.1` H1 body forwarding differentially).
2. POST body > `max_request_bytes` → **413** local reply, body byte-exact `Payload Too Large` (17 bytes, no trailing newline).
3. POST body > limit on a route whose `BufferPerRoute` **disables** the filter → **200** + body echoed.
4. POST body > a route's `BufferPerRoute`-**lowered** limit → **413**.
5. GET (no body) → **200** passthrough.

---

## 2. Behavior-contract scope for phase 25.2 (LOCKED by ADR-0063)

- **2.1 Stats — DROPPED.** Envoy emits NO `http.<stat_prefix>.buffer.*` counters (ADR-0063 finding 4). The over-limit 413 is reflected only in the generic HCM `downstream_rq_too_large` (not asserted by the fixture — the 0032 expectations precedent has no stats block). `25.2` wires NO buffer-scoped stats and adds NO stats `ConfigError`.
- **2.2 Local reply — the 413 over-limit reply (LOCKED).** A new BEHAVIOR_CONTRACT row: status **413**, body bytes `Payload Too Large` (17 bytes, hex `50 61 79 6c 6f 61 64 20 54 6f 6f 20 4c 61 72 67 65`, NO trailing newline), `content-type: text/plain`, `content-length: 17`. Reproduced by the existing `decorate_filter_synth_response{,_h2}` helpers (the rbac/csrf precedent).

---

## 3. Deliverables (D2–D6; shapes LOCKED by ADR-0063)

- **D2 — `envoy-filter::BufferFilter` runtime** (`crates/envoy-filter/src/buffer.rs`, the NINTH `HttpFilterInstance` variant): decode-side only. In `decode_headers`, with the full body available as `FilterRequest.body`: if `body.len() > effective_max_request_bytes` (strict `>`, ADR-0063 finding 6) → `Decision::StopAndSend(FilterResponse { status: 413, … })` decorated by the existing H1/H2 filter-synth helpers (the `Payload Too Large` 17-byte body); else `Decision::Continue`. The effective limit is the per-route `BufferPerRoute` override if present (else the chain-level `Buffer.max_request_bytes`); a per-route `disabled: true` bypasses the filter. NO stats.
- **D3 — Config schema** (`crates/envoy-config/src/bootstrap.rs`): `Buffer { max_request_bytes: u32 }` on `HttpFilterTypedConfig` (`@type ...buffer.v3.Buffer`); `BufferPerRoute` oneof `{ disabled: bool, buffer: Buffer }` as the third `PerFilterConfig` variant after `Cors`+`Csrf` (`@type ...buffer.v3.BufferPerRoute`). All `#[serde(deny_unknown_fields)]`. **Reuse the generic `PerRouteConfigForAbsentFilter` validator verbatim** (ADR-0063 finding 7 — buffer is covered for free). The absent/`0`/malformed `max_request_bytes` disposition is the ADR-0063 Residual — verify at the `25.2` state-2 PLAN-write (projected all-fatal per ADR-0049).
- **D4 — `HttpFilterInstance::Buffer` variant** + the `build` dispatch over `HttpFilterTypedConfig::Buffer` + the `apply_route_config` dispatch arm (the cors/csrf precedent — NO HCM change).
- **D5 — BEHAVIOR_CONTRACT extension** (§2.2 the 413 row only; §2.1 stats DROPPED) + the §2.3 H1 body-forwarding note already landed by `25.1`.
- **D6 — Fixture + harness + fuzz seed + backstop:** fixture `0033-http-filter-buffer` (H1 → real `http1-echo-server`; the 5 probes of §1); an `http1_probe_list` harness extension to carry a request body (the recon found no built-in POST-body driver — fixture-0032 probes use `content-length: 0`); a new `parse_bootstrap` fuzz seed for `Buffer`/`BufferPerRoute` (NO new fuzz target); an in-process backstop (within-limit forward + over-limit 413 + per-route disable + per-route limit-override).

---

## 4. Out of scope (deferred non-goals — parent SPEC §4)

Chunked/streaming request bodies; the generic streaming `decode_data` hook; encode-side buffering; vhost-level `BufferPerRoute` + the route>vhost cascade; `typed_per_filter_config` for filters OTHER than cors/csrf/buffer; gRPC/trailers; closing the absent-filter accept-inert divergence (ADR-0063 finding 7 — an M-track cross-cutting candidate).

---

## 5. Architectural invariants

- **5.1 No new crate** (config in `envoy-config`, filter in `envoy-filter`).
- **5.2 Hand-rolled filter** (D-3.2); no new dependency.
- **5.3 Decode-side-only** (like csrf) — a length check + 413 short-circuit.
- **5.4 Additive** — D2-D4 are an enum arm + a module, inert when the buffer filter is unconfigured.
- **5.5 Determinism** — pure function of method + body length + policy; byte-exact differential.
- **5.6 H1-first; H2 inherits for free** (H2 already buffers+forwards; no H2 fixture required).

---

## 6. Implementation signposts (parent SPEC §6.x + ADR-0063)

- **6.1** Reuse facts (parent SPEC §0): `PerFilterConfig` `@type`-tagged enum (`bootstrap.rs:791`, variants `Cors`+`Csrf`); `Route.typed_per_filter_config` (`:1247`) + deserializer (`:1413-1515`); `FilterPipeline::apply_route_config` → `instance.rs:180-190`; `HttpFilterInstance` (`instance.rs:32-77`, 8 variants; `build` dispatch `:103-136`); the decorate helpers (`hcm.rs:1454-1499` H1; `envoy-http2/src/response.rs:70-110` H2); the body-echoing `http1-echo-server` (`tests/helpers/http1-echo-server/src/main.rs:149-244`).
- **6.2** §6.2 ALREADY RAN (ADR-0063). The only residual: the absent/`0`/malformed `max_request_bytes` disposition (verify locally at the `25.2` PLAN-write; projected all-fatal).
- **6.3** State-3 subagent-driven, SERIAL dispatch.

---

## 7. ADR posture

- Shapes LOCKED by **ADR-0063**; split by **ADR-0064**; scope by **ADR-0062**. A new ADR fires at the `25.2` state-2 PLAN-write ONLY if the `max_request_bytes` residual diverges from the projected all-fatal posture. ADR-0014 in force; ADR-0028 open (not engaged).

---

## 8. Commit message format (for state 6 — flips parent `25` to `done`)

```
phase 25.2: envoy.filters.http.buffer + BufferPerRoute + fixture 0033 [ADR-0063, ...]

<summary — 1–3 sentences>

Differential surface: fixture 0033-http-filter-buffer green; all 33 Docker-gated fixtures green simultaneously; parent phase 25 flips done.
Conformance: h2spec ≥95%; fuzz parse_bootstrap clean.
```
