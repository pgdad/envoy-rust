# Phase 25.1 — PROGRESS

> Running log for the `25.1-h1-request-body-forwarding` implementation arc
> (state-3, subagent-driven per `feedback_execution_style`). Append one entry
> per task on completion (the 06.2→24 cadence). PLAN: `PLAN.md`. SPEC: `SPEC.md`.
> Scope ADRs: ADR-0062 (parent), ADR-0063 (§6.2 reconciliation), ADR-0064 (split).

---

## Task-1 preamble (state-2 PLAN-write — the §6.2 verification transcript + the recon facts)

This preamble is written at the state-2 PLAN-write (the split commit), BEFORE any state-3 implementation. It preserves (a) the parent phase-25 §6.2 empirical verification transcript (ADR-0063 Provenance pointer) and (b) the H1-body-path recon facts the PLAN relies on. A state-3 implementer reads this FIRST.

### A. The §6.2 empirical verification (ADR-0063) — transcript

Ran LOCALLY against `envoyproxy/envoy:v1.33.0` (image id `56da5afd7df3`, the ADR-0058 digest) on macOS Docker 28.0.4 on 2026-06-13. Methodology: `project_docker_sidecar_backend_for_62_verify` (sidecar `mccutchen/go-httpbin:v2.15.0` upstream on a shared Docker network — `host.docker.internal` is IPv6-only on macOS Docker → 503s). Scratch configs were in `/tmp/buf62` (`validate-ok.yaml`, `validate-absent.yaml`, `validate-bogus.yaml`, `live.yaml`); the container is torn down.

1. **413 over-limit local reply (chain `max_request_bytes: 10`, POST 11 bytes):**
   ```
   HTTP/1.1 413 Payload Too Large
   content-length: 17
   content-type: text/plain
   date: …
   server: envoy
   connection: close

   Payload Too Large            ← body, 17 bytes, NO trailing newline
   ```
   `xxd`: `50 61 79 6c 6f 61 64 20 54 6f 6f 20 4c 61 72 67 65`. Byte-identical at a per-route `BufferPerRoute`-lowered limit (`/low`, max 4, POST 5 bytes → same 413 + same body).
2. **`Buffer` shape:** `@type type.googleapis.com/envoy.extensions.filters.http.buffer.v3.Buffer`; `max_request_bytes: 10` (plain int, `UInt32Value`) → `--mode validate` `OK`.
3. **`BufferPerRoute` shape:** `@type …buffer.v3.BufferPerRoute`; oneof `{ disabled: true }` AND `{ buffer: { max_request_bytes: 4 } }` → both `--mode validate` `OK`.
4. **Stats — NONE (the ADR-0063 trigger):** `/stats` after 8×2xx + 2×413 showed NO `http.<prefix>.buffer.*` counters. The 2 over-limit rejections appeared ONLY in `http.ingress_http.downstream_rq_too_large = 2` (+ `downstream_rq_4xx = 2`, `downstream_rq_response_before_rq_complete = 2`, `downstream_rq_rx_reset = 2`). → SPEC §2.1 buffer-stats row DROPPED (25.2 wires no buffer stats). Not a 25.1 concern.
5. **No-body GET → 200** passthrough.
6. **Boundary strictly `>`:** chain max 10 → 9 B 200 / 10 B 200 / 11 B 413; per-route max 4 → 3 B 200 / 4 B 200 / 5 B 413.
7. **Absent-filter:** Envoy ACCEPTS a `BufferPerRoute` per-route config whose `buffer` filter is absent from the chain (config `OK`, inert — `buffer` is compiled-in); the bogus-`@type` negative control DID fail hard (`could not find @type`). envoy-rust's generic `PerRouteConfigForAbsentFilter` validator rejects it; 25.2 reuses that verbatim (cors/csrf precedent). Not a 25.1 concern.
8. **Real-upstream constraint (ADR-0058 L6) — CONFIRMED:** within-limit POST (5 B < 10) reached the live upstream, body `hello` echoed in the reflected `data` field → 200. So a within-limit body must reach a REAL upstream to 200 → 25.2's fixture 0033 uses a real `http1-echo-server`. **This is the property 25.1 enables and 25.2 differentially proves.**

Residual (deferred to the 25.2 PLAN-write): the absent/`0`/malformed `max_request_bytes` disposition was not probed (valid limits 10 and 4 only).

### B. The H1-body-path recon (code-HEAD `9b0e7b925`) — what 25.1 changes

- `run_attempt(config, cluster, cluster_name, req: &Request, host_header, close)` `hcm.rs:315-321`; builds `out_req` `:349-357`; the always-empty `body: Some(Bytes::new())` `:356` → becomes `req.body.clone()` (Task 1 Step 5).
- Per-request handler: `body_len = parse_content_length(&req.headers)?` `:597`; `chunked` `:598`; `let mut req = req;` `:615`; boundary conversion `filter_req` with `body: req.body.take()` `:635`; write-back `:642`; head-advance + discard-drain `:676-697` → the body read is RELOCATED before `:631` and the discard-drain is removed (Task 1 Steps 4 + 6).
- `FilterRequest { …, body: Option<Bytes> }` `crates/envoy-filter/src/types.rs:28-35`.
- Regression key: `body_len == 0` for every existing fixture (bodyless or `content-length: 0`) → the relocated read is a body-wise no-op → all 32 fixtures stay green.

### C. State-machine position

This commit is the parent-25 state-2 PLAN-write + the §6.1 split (ADR-0064): it lands the parent SPEC's two sub-phase SPECs, this 25.1 `PLAN.md` + `PROGRESS.md`, ADR-0063 + ADR-0064, the ROADMAP split, and the STATE advance to `25.1` state-2-complete / state-3-next. Per §5.1 the NEXT session BEGINS state-3 (`superpowers:subagent-driven-development`) on Task 1.

---

## Task log

_(no tasks complete yet — state-3 begins next session)_

- [ ] **Task 1** — H1 forwards the Content-Length request body upstream.
- [ ] **Task 2** — Regression invariants (chunked 501 / bodyless / keep-alive pipelining / retry replay).
- [ ] **Task 3** — BEHAVIOR_CONTRACT note + isolated-crate & workspace verification.
