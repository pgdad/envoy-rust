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

### T2 + T3 — `BufferFilter` runtime + decode-side backstop, AND the `HttpFilterInstance::Buffer` wiring + pipeline backstop (DONE — co-landed this session)

- **Why T2 + T3 co-landed in ONE session (deviation from the one-task cadence — recorded):** T1 (`ecc674a9d`) added the `HttpFilterTypedConfig::Buffer(Buffer)` variant to `envoy-config`. That made the **exhaustive** `match &hf.typed_config` in `crates/envoy-filter/src/instance.rs` (the `build` fn) non-exhaustive → `cargo build -p envoy-filter` / `cargo build --workspace` FAIL with `error[E0004]: pattern \`&HttpFilterTypedConfig::Buffer(_)\` not covered` (VERIFIED on disk at HEAD `ecc674a9d` and `f84b13066`). The PLAN's T2 note predicted a benign `dead_code` *warning*; it is in fact a hard *compile error*. T2 is scoped to NOT touch `instance.rs`, so **T2 alone cannot restore a green workspace** — only T3's `instance.rs` dispatch arm closes the exhaustive match. The next-prompt verification posture ("Each task brings the workspace green [build/test/fmt]") + D-3.6 ("every phase is a green build") make a knowingly-non-compiling pushed `main` the worse outcome (`origin/main` was already non-compiling from the pushed T1 commit). The `Buffer` *variant* (T1) and its *dispatch arm* (T3) are a single compilation unit; T2's runtime sits between them. So this session completed **T2 + T3 together** as the coupled "add the Buffer filter end-to-end in `envoy-filter`" unit, restoring `cargo build --workspace` to green. No new ADR (this is a per-task sequencing reconciliation within the locked ADR-0062/0063/0064 scope, not a new decision).
- **Skill:** `superpowers:subagent-driven-development` — two implementer subagents dispatched **SERIALLY** (T2 then T3, per `feedback_serial_subagent_dispatch`), each TDD. Then the two-stage review over the combined T2+T3 diff: spec-compliance ✅, then code-quality ✅.
- **T2 — `crates/envoy-filter/src/buffer.rs` (CREATE) + `crates/envoy-filter/src/lib.rs` (MODIFY):**
  - `pub struct BufferFilter { base_max: u32, effective: Effective }` — the NINTH `HttpFilterInstance` backing type; mirrors the `csrf.rs` precedent structurally but takes **NO stats** (ADR-0063 finding 4 — Envoy emits no buffer-scoped counters). `new(cfg: &envoy_config::Buffer) -> Self` is **infallible** (no `Result`, no `StatsRegistry`).
  - `Effective { Disabled, Limit(u32) }` per-request policy (cleaner than `Option<u32>` — distinguishes route-disabled bypass from a `Limit(0)` reject-any-nonempty-body).
  - `apply_route_config` selects the effective policy: route `BufferPerRoute` override if present (`disabled: true` → `Disabled`; `buffer: Some` → `Limit(override)`; empty `{}` → chain base), else chain base.
  - `decode_headers` rejects iff `body.len() as u64 > u64::from(limit)` (**strict `>`**, ADR-0063 finding 6; u64-safe vs >4 GiB truncation) with `StopAndSend` of a **413** local reply: reason `"Payload Too Large"`, body the **17-byte** `b"Payload Too Large"` (no newline, ADR-0063 finding 1), empty `headers` (content-type/length stamped downstream by the H1/H2 synth decorators — the rbac/csrf precedent). `encode_headers` = trivial `Continue` (decode-side only).
  - `lib.rs`: `pub mod buffer;` + `pub use buffer::BufferFilter;` (alphabetically before `cors`, matching the `pub mod`/`pub use` convention).
  - 8 decode-side backstop unit tests (within-limit / at-limit-`==`-strict-gt / over-limit-413-asserts-status+reason+body+len-17 / per-route-disabled-bypass / per-route-lowered-reject / per-route-empty-falls-back / GET-no-body-passes / zero-limit-rejects-nonempty).
  - Commit `f84b13066` (`buffer.rs` + `lib.rs`). **NOTE: this commit does not compile in isolation** (the E0004 above) — it is the intermediate of the T2+T3 unit; `ad2c9a859` (T3) restores compilation.
- **T3 — `crates/envoy-filter/src/instance.rs` (MODIFY):**
  - Import `use crate::buffer::BufferFilter;`; `Buffer(BufferFilter)` enum variant (9th production variant, after `Csrf`); `build` arm `Buffer(cfg) => Ok(HttpFilterInstance::Buffer(BufferFilter::new(cfg)))` (infallible — closes the exhaustive match, the E0004 fix); `decode_headers` / `encode_headers` dispatch arms; an EXPLICIT `apply_route_config` `Buffer` arm above the `_ => {}` catch-all (subtle correctness — else per-route overrides silently never apply).
  - `buffer_pipeline_backstop_all_dispositions` test drives the REAL `FilterPipeline::build_from_config([buffer(max=10), router]) → apply_route_config → decode_headers` path across all 4 dispositions (within→Continue / over→413 "Payload Too Large" / per-route-disabled→Continue / per-route-lowered(4)→413). Route built via struct-literal (`Buffer`/`BufferPerRoute` fields are `pub`).
  - Code-quality Minor fixed (amended into `ad2c9a859`): the `apply_route_config` doc/inline comment now enumerates `Cors`/`Csrf`/`Buffer` as the route-config readers (was stale at "only Cors/Csrf").
  - Commit `ad2c9a859` (`instance.rs`; +117/−4).
- **Verification (state-3 per-task gate — build/test/fmt; clippy deferred to state-4 per `project_state3_arc_skips_clippy`):**
  - `cargo build -p envoy-filter` → clean (per-crate, per `project_isolated_crate_build_blindspot`).
  - `cargo build --workspace` → clean (**restored** — the whole-workspace compile that was broken since T1).
  - `cargo test -p envoy-filter` → `test result: ok. 115 passed; 0 failed` (the 8 buffer unit tests + the pipeline backstop + all pre-existing filter tests).
  - `cargo fmt -p envoy-filter` applied (cosmetic reflow); tests re-confirmed green.
- **No new ADR** (T2/T3 surface no decision beyond ADR-0062/0063/0064; the co-landing is a sequencing reconciliation). No `unsafe`. Ledger head stays **ADR-0064**.
- **Next:** T4 — M25.1-1 (bound the H1 body up-front allocation to `min(body_len, 64 KiB)`, grow-on-demand) + M25.1-2 (cross-TCP-segment body-reassembly forwarding test) in `crates/envoy-http1/src/hcm.rs`.

### T4 — M25.1-1 (bound the H1 body up-front allocation) + M25.1-2 (cross-TCP-segment forwarding test) (DONE)

- **Skill:** `superpowers:subagent-driven-development` (one implementer subagent, serial per `feedback_serial_subagent_dispatch`; M25.1-2 test written first — it PASSES against the unmodified reassembly loop, pinning existing-correct behavior). Two-stage review: spec-compliance ✅ then code-quality ✅ (Approved; only Minor notes — the 50 ms loopback sleep is an accepted convention in this test module; no changes required).
- **Production change — `crates/envoy-http1/src/hcm.rs` (ONE file, +105/−2):**
  - **M25.1-1:** new `const INITIAL_BODY_BUF_CAP: usize = 64 * 1024;` (with rationale doc comment) immediately after `IDLE_READ_TIMEOUT`; the body-read reservation changed from `BytesMut::with_capacity(body_len)` to `BytesMut::with_capacity(body_len.min(INITIAL_BODY_BUF_CAP))`. **Behavior-preserving:** for any real small body (`body_len <= 64 KiB`) `.min` is a no-op (byte-identical allocation); for an oversized declared `Content-Length` it reserves at most 64 KiB up front and grows on demand via the unchanged `extend_from_slice` path — bounding the RESERVATION, not the read. The `while remaining > 0` loop, 4096-byte read chunk, and timeout/EOF/io-error dispositions are byte-for-byte unchanged. (A true per-request cap tied to the buffer filter's effective limit stays a deferred non-goal — the effective limit is resolved later in the pipeline, not at this read site.)
  - **M25.1-7 (cosmetic):** the stale `// 4. Compute body length (for drain) …` comment now reads `(for the body read + the M25.1-1 reservation bound)`.
- **Tests added (in `#[cfg(test)] mod tests`):**
  - `drive_split` helper — writes the request HEAD, `flush`es, `sleep(50ms)`, then writes the BODY in a separate `write_all`, forcing a TCP segment boundary so head+body land in distinct reads. Reuses `serve_connection` exactly as `drive` does (NO new HCM entry point).
  - `h1_forwards_body_split_across_tcp_segments` (M25.1-2) — drives `drive_split` against `spawn_recording_upstream` (the `Arc<Mutex<Vec<u8>>>` loop-with-timeout helper, robust to a second-segment body — NOT the single-read `spawn_capturing_upstream`); asserts the upstream received the full reassembled body (`ends_with("hello world")`). This is the first coverage of the multi-read `while remaining > 0` path (the `25.1` tests wrote head+body in one `write_all` so it never ran).
  - `h1_forwards_large_body_grows_on_demand` — a 10 000-byte body (> one 4 KiB read chunk) forwarded byte-exact (`got.ends_with(&body)`), proving `extend_from_slice` grows past the now-bounded 64 KiB reservation.
- **Verification (state-3 per-task gate — build/test/fmt; clippy deferred to state-4 per `project_state3_arc_skips_clippy`):**
  - `cargo test -p envoy-http1` → `test result: ok. 112 passed; 0 failed` (the 3 new/target tests + ALL pre-existing `25.1` body-forwarding tests still green — the allocation bound is behavior-preserving; no regressions).
  - `cargo build -p envoy-http1` → clean (per-crate, per `project_isolated_crate_build_blindspot`).
  - `cargo build --workspace` → clean (still green after T3).
  - `cargo fmt -p envoy-http1` → no changes.
- **No new ADR** (T4 closes the two adjudicated-non-blocking `25.1` REVIEW carry-forwards under the existing ADR-0064/ADR-0044 scope; no decision surfaced). No `unsafe`. Ledger head stays **ADR-0064**.
- **Commit:** `d7ea6f0fd` (`crates/envoy-http1/src/hcm.rs` only).
- **Next:** T5 — differential harness extension: `Http1Probe.body` + `drive_http1` request-body support (`tests/differential/src/lib.rs`).
