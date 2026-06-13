# Phase 25.2 (`25.2-http-filter-buffer`) — PROGRESS

> Running log for the `25.2` state-3 subagent-driven implementation. The state-3
> session appends one entry per completed task (BOOTSTRAP §5 state 3). This
> skeleton lands alongside `PLAN.md` at the state-2 PLAN-write (parent SPEC §6.6
> cadence). Context-isolated (D-3.4) — readable standalone.

- **Phase:** `25.2` — `envoy.filters.http.buffer` (Part B of parent `25`; the ADR-0064 split sibling of the closed `25.1`).
- **Scope locked by:** ADR-0062 (parent scope) · ADR-0063 (§6.2 wire contract: 413 `Payload Too Large` 17B / strict `>` / NO stats / `Buffer`+`BufferPerRoute` shapes / reuse `PerRouteConfigForAbsentFilter`) · ADR-0064 (split).
- **PLAN:** `docs/envoy-rust/phases/25.2-http-filter-buffer/PLAN.md` — 7 TDD tasks (D2–D6 + the two `25.1` REVIEW carry-forwards M25.1-1/M25.1-2).

---

## State-2 PLAN-write (this commit)

- Authored `PLAN.md` via `superpowers:writing-plans` against `25.2`'s `SPEC.md` and the locked ADR-0062/ADR-0063/ADR-0064 scope. 7 subagent-dispatchable tasks:
  - **T1** — `Buffer { max_request_bytes: u32 }` + `BufferPerRoute { disabled, buffer }` config schema + the `HttpFilterTypedConfig`/`PerFilterConfig` variants + the name↔typed_config validation arm.
  - **T2** — `BufferFilter` runtime (`crates/envoy-filter/src/buffer.rs`; ninth `HttpFilterInstance` variant; decode-side-only `body.len() > effective_max → 413` else Continue; NO stats) + the decode-side backstop unit tests.
  - **T3** — `HttpFilterInstance::Buffer` wiring (variant + build/decode/encode/`apply_route_config` dispatch; NO HCM change) + an in-process `FilterPipeline` backstop (all 4 dispositions).
  - **T4** — M25.1-1 (bound the H1 body up-front allocation to `min(body_len, 64 KiB)`, grow-on-demand) + M25.1-2 (cross-TCP-segment forwarding test) in `crates/envoy-http1/src/hcm.rs`.
  - **T5** — harness `Http1Probe.body` + `drive_http1` request-body support.
  - **T6** — fixture `0033-http-filter-buffer` (H1 → real `http1-echo-server`; 5 probes → status `[200, 413, 200, 413, 200]`) + the differential acceptance test.
  - **T7** — BEHAVIOR_CONTRACT 413 row + a `parse_bootstrap` buffer fuzz seed (no new fuzz target) + the state-3 workspace gates.
- **§6.1 split gate:** held — 7 tasks / ~800 LoC, UNDER the `>25 tasks / >1500 LoC` gate (ADR-0064 already split parent 25; 25.2 ships single).
- **ADR-0063 `max_request_bytes` Residual — RESOLVED in the PLAN (NO new ADR):** modeled as a REQUIRED non-`Option` `u32` serde field → absent/malformed → fatal startup error (ADR-0049 all-fatal posture; matches Envoy's `required` proto field); `0` accepted (reject iff `body.len() > 0`). Differentially safe (the cors/csrf finding-7 stricter-than-Envoy precedent; fixture always supplies a valid limit). The projected all-fatal posture HOLDS → **ADR-0065 unfired**. No new `ConfigError` variant.
- **Ledger head:** ADR-0064 (count 65; next available ADR-0065, unfired). ADR-0014 in force; ADR-0028 open.
- Docs-only commit (new `PLAN.md` + this `PROGRESS.md` skeleton + STATE advance + STATE_HISTORY relocation); NO production/test/fixture/Cargo change.

---

## State-3 implementation

_(appended per task by the state-3 `superpowers:subagent-driven-development` session)_
