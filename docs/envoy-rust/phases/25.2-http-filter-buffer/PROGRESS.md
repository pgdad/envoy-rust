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

### T1 — `Buffer` + `BufferPerRoute` config schema + the two enum variants + the validation arm (DONE)

- **Skill:** `superpowers:subagent-driven-development` (one implementer subagent dispatched serially per `feedback_serial_subagent_dispatch`; TDD — failing serde tests first, then schema). Two-stage review: spec-compliance ✅ then code-quality ✅.
- **Production change — `crates/envoy-config/src/bootstrap.rs`:**
  - New `pub struct Buffer { pub max_request_bytes: u32 }` — a REQUIRED non-`Option` `u32` with `#[serde(deny_unknown_fields)]` (the ADR-0063 residual disposition, resolved in PLAN: absent → serde missing-field → fatal [ADR-0049 all-fatal]; negative/malformed → u32-parse → fatal; `0` accepted = reject iff `body.len() > 0`). Derives `Debug, Clone, PartialEq, Serialize, Deserialize` (matched the local Cors/Csrf cluster ordering).
  - New `pub struct BufferPerRoute { #[serde(default)] disabled: bool, #[serde(default)] buffer: Option<Buffer> }` (ADR-0063 finding 3 oneof `{ disabled, buffer }`; empty `{}` → chain base at apply time).
  - `HttpFilterTypedConfig::Buffer(Buffer)` (`@type` = `…buffer.v3.Buffer`, after the `Csrf` arm) + `PerFilterConfig::Buffer(BufferPerRoute)` (`@type` = `…buffer.v3.BufferPerRoute`, the THIRD `PerFilterConfig` variant after Cors/Csrf).
  - New validation arm `HttpFilterTypedConfig::Buffer(_cfg)` in the per-filter name↔typed_config match — rejects with `ConfigError::UnsupportedHttpFilter { name }` iff `f.name != "envoy.filters.http.buffer"`; no further validation (the generic `PerRouteConfigForAbsentFilter` covers per-route for free — ADR-0063: NO stats, NO new `ConfigError` variant).
  - 7 serde tests (chain plain-int / `0`-accepted / absent-fatal / negative-fatal / per-route-disabled / per-route-lowered-limit / unknown-field-rejected).
- **Deviation from PLAN (sound):** added `Buffer, BufferPerRoute` to the `crates/envoy-config/src/lib.rs` crate-root `pub use bootstrap::{…}` re-export, mirroring how `CsrfPolicy`/`CorsPolicy` are re-exported (the PLAN's tests + downstream T2/T3 consume `crate::Buffer` / `crate::BufferPerRoute`). PLAN expected one file; this is the correct precedent-matching second file.
- **`@type` × `deny_unknown_fields` safety (code-quality reviewer verified):** both enums are internally tagged (`#[serde(tag = "@type")]`), so serde consumes `@type` into variant selection before the inner struct deserializes — `deny_unknown_fields` on `Buffer`/`BufferPerRoute` never sees `@type`. Identical to the Cors/Csrf precedent (mechanism documented at `CorsConfig`'s doc-comment).
- **Verification (state-3 per-task gate — build/test/fmt; clippy deferred to state-4 per `project_state3_arc_skips_clippy`):**
  - `cargo test -p envoy-config buffer_` → `test result: ok. 7 passed; 0 failed`.
  - `cargo build -p envoy-config` → clean (per-crate, per `project_isolated_crate_build_blindspot`).
  - `cargo test -p envoy-config` → `test result: ok. 425 passed; 0 failed`.
- **No new ADR** (T1 surfaces no decision beyond ADR-0062/0063/0064; the `max_request_bytes` disposition was already resolved in the PLAN-write → ADR-0065 stays unfired). No `unsafe`. Ledger head stays **ADR-0064**.
- **Next:** T2 — `BufferFilter` runtime (`crates/envoy-filter/src/buffer.rs`, ninth `HttpFilterInstance` variant) + decode-side backstop unit tests.
