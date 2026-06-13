# Phase 25.1 (`25.1-h1-request-body-forwarding`) — SPEC

- **Phase id:** `25.1` (sub-phase of parent `25` — `envoy.filters.http.buffer`).
- **Slug:** `25.1-h1-request-body-forwarding`.
- **Family:** HTTP filters (§9) — Part A (the foundation slice) of parent phase `25`.
- **Depends-on:** `04` (HTTP/1.1 HCM + router-proxy arm) · `07` (filter framework — `FilterRequest.body`).
- **Lifecycle:** state-2-complete at the split commit (this `SPEC.md` + `PLAN.md` + `PROGRESS.md` skeleton land together; the next session BEGINS state-3 subagent-driven implementation).
- **Split ADR:** ADR-0064 (split parent `25` into `25.1` + `25.2`). §6.2 reconciliation: ADR-0063. Parent scope: ADR-0062 + `docs/envoy-rust/phases/25-http-filter-buffer/SPEC.md`.

---

## 0. Why this sub-phase exists (read the parent SPEC §0 first)

Parent phase `25` (`envoy.filters.http.buffer`) is the project's FIRST body-dependent HTTP filter. A read-only recon (parent SPEC §0; ADR-0062) established that the H1 request-body data path is **partly absent**:

- **H1 does NOT forward request bodies upstream.** `crates/envoy-http1/src/hcm.rs` builds the per-attempt upstream request with `body: Some(Bytes::new())` — always empty (`:356`, comment "Chunked-request-body forwarding is a SPEC §4 non-goal") — and **drains-and-discards** the downstream Content-Length-delimited body into a throwaway buffer AFTER the response is built (`:678-697`). The `FilterRequest.body` handed to the pipeline is `req.body.take()` (`:635`), which is `None` on H1 (the body has not been read at that point). So envoy-rust currently **cannot proxy an H1 POST/PUT body to an upstream at all** — a pre-existing functional gap carried since phase 04.3.
- **H2 ALREADY buffers + forwards request bodies** (`crates/envoy-http2/src/hcm.rs:437-448,473`), so this sub-phase is **H1-only**.

Parent `25` was SPLIT (ADR-0064) into **`25.1` (this sub-phase — close the H1 gap)** + **`25.2` (the `BufferFilter` + `BufferPerRoute` + fixture `0033`)**, because Part A is an **always-active** change to the H1 request data path (every H1 request now reads its body before the pipeline and forwards it), which is more regression-sensitive than the inert-when-unconfigured foundation slices (07.1/12.1/14.1/23.1) and warrants an isolated green + reviewed gate before the additive filter layers on.

---

## 1. Goal and acceptance signal

**Goal:** close the H1 request-body-forwarding gap — read the Content-Length-delimited downstream request body into a `Bytes` BEFORE the filter pipeline runs, make it available to the pipeline as `FilterRequest.body`, and forward it to the upstream (replacing the always-empty `Some(Bytes::new())`).

**Acceptance signal (§7.5 phase-done gate, scoped to a foundation slice):**
- **All 32 existing Docker-gated fixtures (`0001`–`0032`) green simultaneously** on Linux CI (the AUTHORITATIVE differential evidence per ADR-0049) — the load-bearing regression-equivalence invariant. **NO new fixture** is added by `25.1`.
- The new body-forwarding capability is verified by an **in-process capturing-upstream unit test** (a within-limit H1 POST whose body bytes are observed arriving at an in-process upstream). It is DIFFERENTIALLY proven later by `25.2`'s fixture `0033` (within-limit → 200 + echoed body) — the foundation-slice-exercised-by-its-consumer pattern (07.1→07.2).
- Workspace gates green: `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check`, plus the isolated-crate build `cargo build -p envoy-http1` (per `project_isolated_crate_build_blindspot`).

---

## 2. Behavior-contract scope for phase 25.1

A single new BEHAVIOR_CONTRACT note (the "Request body forwarding" extension, parent SPEC §2.3): **H1 now forwards Content-Length-delimited request bodies upstream** (closing the phase-04.3 non-goal for the bounded-body case). Chunked / streaming request bodies remain a recorded non-goal (§4). No stats, no local-reply, no new fixture row.

---

## 3. Deliverable (D1 — the only deliverable of 25.1)

In `crates/envoy-http1/src/hcm.rs`:

1. **Read the body before the pipeline.** Relocate the Content-Length-delimited body read from the post-response discard-drain (`:678-697`) to BEFORE the filter-pipeline boundary conversion (before `:631`): accumulate the `body_len` (`:597`) bytes — those already in the connection read buffer `buf` plus any remaining read from `downstream` — into a `Bytes`, and set `req.body = Some(body)` so the boundary conversion's `req.body.take()` (`:635`) hands the real body to `decode_headers`.
2. **Forward the body upstream.** In `run_attempt` (`:315`), build `out_req.body = req.body.clone()` (`:356`) instead of `Some(Bytes::new())`. The clone is per-attempt and replay-safe (the retry loop re-invokes `run_attempt`), mirroring the H2 side.
3. **Preserve the existing invariants:** the chunked-request 501 rejection (`:652-658`) — chunked requests carry NO Content-Length and are still rejected before any body read; the `Connection`/`Transfer-Encoding` strip (`:344-348`); the content-length parse (`:597`); the connection-pool reuse / `Connection: close` single-use semantics (ADR-0059); the idle-read-timeout drain semantics (`:685-694`); the `bytes_consumed` / `buf.advance` accounting (`:676-680`); the access-log `bytes_received` derivation (`:608`).

**Chunked / streaming request bodies remain a non-goal** (Content-Length-delimited only) — the minimum-viable boundary, recorded in §4.

---

## 4. Out of scope (deferred non-goals)

- **Chunked / streaming request bodies** — Content-Length-delimited only this sub-phase; the `:652-658` 501 rejection for `Transfer-Encoding: chunked` is preserved verbatim.
- **The generic streaming `decode_data` framework hook** — not needed (whole-body is available as `FilterRequest.body`); deferred to the first body-transforming filter.
- **The `BufferFilter` / `Buffer` / `BufferPerRoute` config / fixture `0033` / stats** — all in `25.2`.
- **H2 body forwarding** — already landed (no change).
- **Response / encode-side body buffering.**

---

## 5. Architectural invariants

- **5.1 No new crate.** The change is confined to `crates/envoy-http1/src/hcm.rs` (the H1 router data path). No new dependency.
- **5.2 Always-active hot-path change → regression-equivalence is load-bearing.** D1 touches the path EVERY H1 request exercises. The invariant is "all 32 existing Docker-gated fixtures green simultaneously" — verified at state-4 (the §7.5 gate runs the full Docker differential LOCALLY per `feedback_state4_runs_docker_differential`, with the AUTHORITATIVE evidence the Linux CI anchor).
- **5.3 Body availability before the pipeline is the enabling precondition for `25.2`.** `25.2`'s `BufferFilter` length-checks `FilterRequest.body` in `decode_headers`; `25.1` is what populates it on H1. Reading the FULL body before the pipeline is intentional (the buffer filter must see the whole body to length-check it).
- **5.4 Replay-safe forwarding.** `out_req.body = req.body.clone()` per attempt — a retried POST replays the same body (mirrors the H2 buffered-clone, ADR-0044).

---

## 6. Implementation signposts for the planner

- **6.1** The exact code anchors (code-HEAD `9b0e7b925`): the per-attempt upstream-request build `run_attempt` (`hcm.rs:315-321` signature; `out_req` at `:349-357`, the always-empty `body: Some(Bytes::new())` at `:356`); the per-request handler body region (`body_len` at `:597`, `chunked` at `:598`, the boundary conversion `filter_req` at `:631-636` with `body: req.body.take()` at `:635`, write-back at `:642`, `buf.advance(consumed)` at `:676`, the discard-drain at `:678-697`); `FilterRequest { method, path, headers, body: Option<Bytes> }` (`crates/envoy-filter/src/types.rs:28-35`).
- **6.2** No §6.2 empirical verification is owned by `25.1` — Part A introduces no new wire shape (it forwards what the client sent). The parent §6.2 verification (ADR-0063) already ran at the split commit and confirmed (item 8) that a within-limit body must reach a real upstream to 200 — which `25.2`'s fixture exercises.
- **6.3** State-3 implementation is subagent-driven (`feedback_execution_style`); dispatch implementers SERIALLY (`feedback_serial_subagent_dispatch`).
- **6.4** Test model: the existing `spawn_in_process_upstream(response)` helper (`hcm.rs` tests) reads-then-ignores the request; `25.1` adds a CAPTURING variant that records the received request bytes so the test can assert the forwarded body. Heeds the phase-10 M1 backstop lesson (assert the real forwarded body, not just a status).

---

## 7. ADR posture

- **ADR-0064** (split) and **ADR-0063** (§6.2 reconciliation) land at the split commit (the same commit as this SPEC + the `25.1` PLAN). `25.1` itself is projected to need NO new ADR (it forwards client-supplied bytes; no new behavior decision). ADR-0014 in force; ADR-0028 open (not engaged).

---

## 8. Commit message format (for state 6 of the 25.1 lifecycle)

```
phase 25.1: H1 request-body forwarding (Content-Length-delimited) [ADR-0064]

<summary — 1–3 sentences>

Differential surface: no new fixture; all 32 existing Docker-gated fixtures (0001–0032) green simultaneously.
Conformance: h2spec ≥95%; fuzz parse_bootstrap clean.
```
