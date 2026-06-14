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

### T5 — differential harness: `Http1Probe.body` + `drive_http1` request-body support (DONE)

- **Skill:** `superpowers:subagent-driven-development` (one implementer subagent, serial per `feedback_serial_subagent_dispatch`; TDD — the failing harness unit test written first [arity mismatch], then the `body` param). Two-stage review: spec-compliance ✅ then code-quality ✅ (Approved; one Important test-robustness note + one Minor clippy-bait pre-empt, both applied — see below).
- **Production/test change — `tests/differential/src/lib.rs` (ONE file, +97/−20 net):**
  - **`drive_http1` (`:1531`):** new trailing `body: Option<&[u8]>` param. Request assembly now appends `Content-Length: {b.len()}\r\n` as a real header (inside `if let Some(b) = body`, BEFORE the `Connection: close\r\n\r\n` terminator), then — after `req.into_bytes()` produces the head — `wire.extend_from_slice(b)` appends the body bytes AFTER the `\r\n\r\n`. The `None` path is byte-identical to the prior behavior (no `Content-Length`, no trailing body). The response-read loop is UNCHANGED.
  - **`Http1Probe` (`:817`):** new `#[serde(default)] pub body: Option<String>` field (placed after `extra_headers`, before the `expected_*` assertion fields — it is a request-shaping field) with a doc-comment noting the driver auto-adds `Content-Length` and the probe must NOT also list `content-length` in `extra_headers`.
  - **`Driver::Http1ProbeList` arm (`:3679`):** both `drive_http1` calls (upstream `:3711`, subject `:3721`) now thread `probe.body.as_deref().map(str::as_bytes)` as the new last arg.
  - **All other `drive_http1` call sites pass `None`** (mechanical compile fix): `:1920` (multi-probe pre-request), `:1941`+`:2079` (admin scrape helpers), `:3265`/`:3268` (`Http1` arm), `:3365`/`:3368` (`Http1AfterSettle` arm), `:3934`/`:3945` (`Http1WithAccessLog` arm), `:4224`/`:4232` (multi-probe pre-requests). (`Http1WithAccessLog` keeps no `body` field — intentionally body-less; out of T5 scope.)
  - New `#[cfg(test)] mod drive_http1_body_tests` → `drive_http1_sends_request_body`: spins an in-process `TcpListener` that records received bytes (read-until-200ms-idle), drives a `POST /` with `Some(b"hello")`, asserts the recorded request `contains("Content-Length: 5")` and `ends_with("hello")`.
- **Review fixes applied (amended into the commit):** (Important) the test's driver call uses `.expect("drive_http1 must succeed")` instead of `let _ = …` so a driver/server failure surfaces clearly rather than as a confusing empty-recording assertion; (Minor) `String::from_utf8_lossy(…).into_owned()` instead of `.to_string()` (single allocation; pre-empts a later clippy `-D warnings` lint). The probe-list threading kept the PLAN's exact `…map(str::as_bytes)` form (correct after `as_deref()`).
- **Verification (state-3 per-task gate — build/test/fmt; clippy deferred to state-4 per `project_state3_arc_skips_clippy`):**
  - `cargo test -p differential drive_http1_sends_request_body` → `test result: ok. 1 passed; 0 failed`.
  - `cargo test -p differential --no-run` → compiles (the real Docker differential is a state-4 concern, NOT this task — every `drive_http1` call site fixed).
  - `cargo build -p differential` → clean (per-crate, per `project_isolated_crate_build_blindspot`).
  - `cargo build --workspace` → clean (still green after T4).
  - `cargo fmt -p differential` applied; the unit test re-confirmed green.
- **No new ADR** (T5 is a harness extension under the locked ADR-0062/0063/0064 scope; no decision surfaced). No `unsafe`. Ledger head stays **ADR-0064**.
- **Commit:** `61229f844` (amended; `tests/differential/src/lib.rs` only).
- **Next:** T6 — fixture `0033-http-filter-buffer` (H1 → real `http1-echo-server`; chain `Buffer { max_request_bytes: 10 }`; per-route disable on `/disabled`, lowered limit `4` on `/small`; 5 probes → status `[200, 413, 200, 413, 200]`) + the differential acceptance test `tests/differential/tests/http_filter_buffer.rs`.

### T6 — fixture `0033-http-filter-buffer` + the differential acceptance test (DONE)

- **Skill:** `superpowers:subagent-driven-development` (one implementer subagent, serial per `feedback_serial_subagent_dispatch`). Two-stage review: spec-compliance ✅ (byte-identical to PLAN Steps 1/2/3/5; README factually accurate; 6 files / 348 insertions / NO production change) then code-quality ✅ (Approved — the cross-proxy echo-body parity mechanism + the 413-local-reply-never-reaching-upstream path were both traced against the known-good `0032`; no false-green/flakiness risk; one Minor README convention divergence applied [see below]).
- **DATA + TEST-FILE task — NO production-crate change.** Six files created under `tests/fixtures/0033-http-filter-buffer/` + one thin acceptance test:
  - **`envoy-rust.yaml`** (the narrow side): bind `127.0.0.1:{{PORT}}`, NO admin block, NO `request_headers_to_remove`/`generate_request_id`/`dns_lookup_family`. HCM `http_filters` chain `[envoy.filters.http.buffer (max_request_bytes: 10), envoy.filters.http.router]`; three first-match routes — `/disabled` (`BufferPerRoute { disabled: true }`), `/small` (`BufferPerRoute { buffer: { max_request_bytes: 4 } }`), `/` catch-all (chain base) — all `route: { cluster: backend }` → real `http1-echo-server` (`{{BACKEND_HOST}}`/`{{HTTP1_BACKEND_PORT}}`).
  - **`envoy.yaml`** (the upstream-Envoy side): the `0031/0032` per-side asymmetry verbatim — `admin` (port 0), `0.0.0.0:{{PORT}}` bind, `generate_request_id: false`, the 6-header `request_headers_to_remove`, cluster `dns_lookup_family: V4_ONLY`; same routes + filter chain as the rust side (so the echo bodies are byte-equal cross-proxy).
  - **`expectations.yaml`**: `driver.kind: http1_probe_list`, 5 probes (`Host: buffer.test`) using the NEW T5 `Http1Probe.body` field — p1 `POST /` `hello` (5≤10)→200, p2 `POST /` `hello world!!` (13>10)→413, p3 `POST /disabled` `hello world!!` (route-disabled)→200, p4 `POST /small` `hello` (5>4)→413, p5 `GET /` (no body)→200 → status `[200,413,200,413,200]`. Probes 2/4 assert `expected_body: { kind: byte_exact, body: "Payload Too Large" }` (17 bytes, no newline — ADR-0063); all probes `expected_headers: set_equal_modulo_allow_list`; top-level `equivalence: { response_status: exact, response_body: { kind: byte_exact } }`.
  - **`inputs/.gitkeep`** (empty, 0 bytes) + **`README.md`** (modeled on `0032/README.md`: chain + per-route override description, the 5-probe table, the real-upstream-required rationale [ADR-0063 finding 8], the strict-`>` over-limit + byte-exact 413-body rationale, the echo-body equivalence + per-side YAML asymmetry sections, ADR-0062/ADR-0063 cross-refs).
  - **`tests/differential/tests/http_filter_buffer.rs`**: thin `#[tokio::test] async fn http_filter_buffer_fixture()` → `differential::run_fixture(&dir).await.expect("fixture passes")` (mirrors `http_filter_csrf.rs`; Docker-gated by the harness).
- **Code-quality Minor applied (amended into the commit):** the README's opening line now reads `Both upstream Envoy (v1.33.0) and envoy-rust …` — restoring the `(v1.33.0)` version tag that `0031`/`0032` carry (pure convention parity; verified v1.33.0 is the pinned upstream). The second Minor (the decorative-but-harmless `Host: buffer.test`) needs no change — the buffer filter is host-agnostic.
- **No content-length cross-proxy divergence (code-quality reviewer traced):** `0032`'s explicit `content-length: 0` workaround was only needed for *bodyless POSTs* (Envoy synthesizes `content-length: 0`, envoy-rust does not). `0033` has NO bodyless POST — probes 1/3 carry real bodies so the T5 harness auto-adds `Content-Length: N` identically on BOTH proxy connections; probe 5 is a bodyless GET (identical to `0032`'s known-good bodyless GET). So no explicit content-length header is required.
- **Verification (state-3 per-task gate — build/fmt; clippy deferred to state-4 per `project_state3_arc_skips_clippy`; the REAL Docker differential for `0033` is the state-4 gate, NOT this task):**
  - `cargo test -p differential --no-run` → compiles (the new `http_filter_buffer` test executable builds: `target/debug/deps/http_filter_buffer-*`).
  - `cargo build -p differential` → clean (per-crate, per `project_isolated_crate_build_blindspot`).
  - `cargo build --workspace` → clean (still green after T5).
  - `cargo fmt -p differential` → clean. Fixture count `tests/fixtures/00*/` now **33** (was 32) — the state-4 Docker-gated regression invariant becomes 33-fixtures-green.
- **No new ADR** (T6 is fixture data + a thin acceptance test under the locked ADR-0062/0063/0064 scope; no decision surfaced). No `unsafe`. Ledger head stays **ADR-0064**.
- **Commit:** `446b32d` (amended; the 6 files only — `tests/fixtures/0033-http-filter-buffer/` + `tests/differential/tests/http_filter_buffer.rs`).
- **Next:** T7 — BEHAVIOR_CONTRACT 413 row + a `parse_bootstrap` buffer fuzz SEED (no new fuzz target) + the state-3 workspace verification gates (T7 is the LAST task → its session advances STATE to state-4-next).

### T7 — BEHAVIOR_CONTRACT 413 row + `parse_bootstrap` buffer fuzz seed + state-3 workspace gates (DONE — FINAL state-3 task)

- **Skill:** `superpowers:subagent-driven-development` (one implementer subagent, serial per `feedback_serial_subagent_dispatch`). Two-stage review: spec-compliance ✅ (both BEHAVIOR_CONTRACT blocks byte-identical to PLAN Step 1; seed YAML byte-identical to PLAN Step 2; both `BufferPerRoute` oneof arms + chain `Buffer` exercised; no out-of-scope changes — the `.gitignore` whitelist + the one-line clippy doc fix adjudicated mechanical necessities) then code-quality ✅ (Approve — every documented fact verified against `buffer.rs`/`instance.rs`/`hcm.rs`: 17-byte body + hex decode, strict `>` at `buffer.rs:89`, NO stats, content-type/length set by the synth decorators not the filter, buffer is the third per-route consumer after cors+csrf at `instance.rs:194-196`; one Minor applied [see below]).
- **DOC + FUZZ-SEED task — NO production-crate runtime change.** Four files:
  - **`docs/envoy-rust/BEHAVIOR_CONTRACT.md`** (+26 lines): the `**25 entries (Buffer filter):**` overview block + the `**Buffer over-limit local-reply wire shape (ADR-0063 finding 1).**` block, inserted after the CSRF cross-reference line (`:588`), before the H1 connection-pool `Connection: close` (ADR-0059) block. Records 413 `Payload Too Large` / 17 bytes no-newline / hex `50 61 79 6c 6f 61 64 20 54 6f 6f 20 4c 61 72 67 65` / `content-type: text/plain`+`content-length: 17` via `decorate_filter_synth_response{,_h2}` / NO buffer stats / strict `>`.
  - **`crates/envoy-config/fuzz/corpus/parse_bootstrap/seed-buffer.yaml`** (NEW, byte-identical to PLAN Step 2): a valid bootstrap with chain `Buffer { max_request_bytes: 10 }` + `BufferPerRoute` both oneof arms (`/disabled` `disabled: true`; `/small` `buffer: { max_request_bytes: 4 }`) + STRICT_DNS `backend`. NO new fuzz target — corpus seed only.
  - **`crates/envoy-config/fuzz/.gitignore`** (+1 line): `!corpus/parse_bootstrap/seed-buffer.yaml` whitelist (the corpus dir is `corpus/parse_bootstrap/*`-ignored with per-file `!` allow entries; mechanically required to track the seed — same pattern as the csrf/cors seeds above it).
  - **`tests/differential/tests/http_filter_buffer.rs`** (+1 line): a blank `//!` doc-comment continuation in the T6 module header — the ONLY clippy lint surfaced by the deferred clippy run (see below).
- **Code-quality Minor applied:** the overview heading `**25 entries (Buffer filter).**` → `**25 entries (Buffer filter):**` (trailing colon), matching the `17`–`24 entries (…):` convention in the doc (the `.` was carried verbatim from PLAN; the colon is the established BEHAVIOR_CONTRACT entry-heading style).
- **T7 IS the state-3 §7.5 gate — the DEFERRED clippy ran here for the first time** (`project_state3_arc_skips_clippy`: clippy was skipped per-task across T1–T6). **Result: the phase-25.2 code was already clippy-clean** — `buffer.rs` (T2), `instance.rs` (T3), `hcm.rs` (T4), and the T5 differential harness all passed `-D warnings` with zero changes; the ONLY lint was the one-line doc-comment continuation in the T6 test file (above).
- **Verification (FULL state-3 workspace gate — all green with evidence):**
  - `cargo fmt --all -- --check` → clean.
  - Seed sanity-check: a throwaway `tests/_tmp_seed_check.rs` calling `envoy_config::parse_bootstrap` on the seed → `1 passed` (Ok), then **deleted** (not committed). envoy-bin has no validate mode; Task-1 parse tests already cover the same shapes.
  - `cargo build -p envoy-config -p envoy-filter -p envoy-http1` (isolated-crate, `project_isolated_crate_build_blindspot`) → clean.
  - `cargo build --workspace --all-targets` → clean (1m28s).
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings` → clean.
  - `cargo test --workspace` → 134 passed; 3 differential `--lib` process-spawn tests (`drive_http2_round_trip_against_in_process_listener`, `subject::tests::starts_and_shuts_down_envoy_rust`, `drive_admin_scrape_round_trips_against_envoy_bin_admin`) flaked on 5s-readiness/startup contention (`project_workspace_test_nested_cargo_backstop_flake` + `project_flaky_access_log_fixture_0012` family — T7 changed zero runtime code so not a regression); re-run **standalone** `cargo test -p differential --lib` → **137 passed; 0 failed** ✅.
- **No new ADR** (T7 is doc + a fuzz seed + the verification gate under the locked ADR-0062/0063/0064 scope; no decision surfaced — ADR-0065 stays UNFIRED). No `unsafe`. Ledger head stays **ADR-0064**.
- **Commit:** `b624b828` (the 4 files above).
- **Next:** T7 was the LAST of the 7 PLAN tasks → **lifecycle state-3 is COMPLETE**. STATE advances to `25.2` state-3-complete / state-4-next; the next session runs the state-4 §7.5 verification gate (`superpowers:verification-before-completion`): re-confirm the workspace gate + `cargo deny check` + `cargo fuzz run parse_bootstrap` short-budget + the **Docker 33-fixture differential LOCALLY** (`feedback_state4_runs_docker_differential`; Linux CI is the authoritative anchor per ADR-0049).
