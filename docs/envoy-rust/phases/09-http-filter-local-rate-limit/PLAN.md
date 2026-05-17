# Phase 09 (`09-http-filter-local-rate-limit`) — PLAN

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development`
> per `feedback_execution_style` auto-memory and per the established 06.x / 07.x / 08.x
> cadence. Tasks 1-8 implement the phase per `SPEC.md`. Steps use `- [ ]` checkbox syntax
> for tracking.

**Goal.** Land the `envoy.filters.http.local_ratelimit` filter as the second concrete
pluggable HTTP filter in the 07.x-established framework: hand-rolled token bucket (per
D-3.2's *"Every individual filter ... Must be written from scratch"* doctrine) + decode-side
`Decision::StopAndSend` with status 429 + `x-envoy-ratelimited: true` response header + 4
upstream-Envoy-parity counters under `http_local_rate_limit.<stat_prefix>.{enabled, ok,
rate_limited, enforced}`. Close phase 07.2 REVIEW M1 (severed `_position` plumbing on
`HttpFilterInstance::build` + `.enumerate()` on `FilterPipeline::build_from_config`) at
the named close site per SPEC §3 D5.

**Architecture.** The token bucket lives at `crates/envoy-filter/src/local_rate_limit.rs`
(new module; mirrors the 07.2 `header_mutation.rs` placement). State is a single shared
`Arc<TokenBucketState>` holding `AtomicU64` (live tokens) + `std::sync::Mutex<Instant>`
(last-fill timestamp). `try_acquire` uses lazy fill — refill is computed at decision
time, not by a background task — via a single `compare_exchange` loop with
`Ordering::AcqRel` semantics. Filter chain integration extends `HttpFilterInstance` with
a new `LocalRateLimit(LocalRateLimitFilter)` variant and threads `&Arc<StatsRegistry>`
through `FilterPipeline::build_from_config` → `HttpFilterInstance::build`. The HCMConfig
constructors at both H1 and H2 already hold the registry; passing it through is one new
argument at two sites. Per SPEC §6.2, D5 (07.2 M1 closure) co-locates with D4 (the new
variant) in Task 4.

**Tech Stack.** No new top-level Cargo deps. One workspace-internal path-dep added:
`envoy-stats = { path = "../envoy-stats" }` on `crates/envoy-filter/Cargo.toml` (the
filter needs Counter handles). Permitted-foundations primitives used: `std::sync::atomic::AtomicU64`,
`std::sync::Mutex<Instant>`, `std::time::{Duration, Instant}`, `bytes::Bytes`. No
`governor`, `ratelimit`, `parking_lot`, `humantime`, `humantime-serde`, `rand`,
`getrandom` — D-3.2 forbids; any pull forces a foundations-grant ADR per D-3.5. Differential
harness reuses `Driver::Http1ProbeList` (existing from 04.2) — no harness extension.

---

## 1. PLAN-write SPEC corrections

Per the 06.2 / 06.3 / 07.x / 08.x precedent ("6 PLAN-write SPEC corrections at 08.2
PLAN-write" pattern), the PLAN-writer reads SPEC §3 surfaces against HEAD `3025594` and
flags mechanical signature drift between projected types and actual on-disk types.
Corrections land in the PROGRESS.md Task 1 preamble (this file's siblings) AND are
acted on during implementation.

1. **`ConfigError` enum lives in `crates/envoy-config/src/lib.rs`, NOT
   `crates/envoy-config/src/bootstrap.rs`.** SPEC §3 D2 text reads *"Four new
   `ConfigError` variants land at this site"* alongside the validator extension
   discussion; "this site" is ambiguous between (a) the validator's task-landing
   commit and (b) the same source file. The validator function
   `validate_http_filters` IS in `bootstrap.rs` (lines 1597-1652 at HEAD), but the
   `ConfigError` enum is in `lib.rs` (the existing HeaderMutation variants
   `EmptyHeaderMutationKey` / `InvalidHeaderMutationKey` /
   `UnsupportedHeaderMutationAppendAction` land at lines 266-294 of `lib.rs`).
   **Action:** Task 1 edits BOTH files — the 4 new variants land in `lib.rs`; the
   sub-validator + `LocalRateLimit` dispatch arm land in `bootstrap.rs`.

2. **The HCM filter-pipeline build site is the HCMConfig constructor, NOT
   `serve_connection`/`handle_one_stream`.** SPEC §3 D4 reads *"The H1 + H2 HCM
   filter-chain wiring sites (`crates/envoy-http1/src/hcm.rs::serve_connection` +
   `crates/envoy-http2/src/hcm.rs::handle_one_stream`)"*. Reading HEAD: the H1
   pipeline is built at `crates/envoy-http1/src/hcm.rs:185` inside
   `Http1HCMConfig::from_config` (NOT `serve_connection`); the H2 pipeline is
   built at HCMConfig construction (the `Http1HCMConfig` shape is shared) and
   `handle_one_stream` reuses the pre-built pipeline by cloning
   `(*config.filter_pipeline).clone()` at line 148 of
   `crates/envoy-http2/src/hcm.rs`. Both HCMConfig constructors already hold an
   `Arc<StatsRegistry>` in scope (the `registry` parameter, stored on the
   struct's `stats` field via `HCMStats::register(&registry, &cfg.stat_prefix)`).
   **Action:** Task 4 adds `&stats.registry()` (or threads the registry parameter
   directly per the HCMConfig::from_config signature) at the SINGLE call site in
   `envoy-http1/src/hcm.rs:185`; the H2 path is naturally covered because
   `envoy-http2`'s HCMConfig reuses the H1 build path via re-export.

3. **`Http1HeaderRule` is a unit-variant enum with only `SetEqualModuloAllowList`,
   NOT `Option<Vec<HeaderRule>>`.** SPEC §3 D8.1 reads *"asserts probes 4-5 carry
   `x-envoy-ratelimited: true` response header (via `Http1Probe.expected_headers`
   set-equal-modulo-allow-list)"* AND the §3 D8.1 hedge text *"the planner
   verifies `Http1Probe.expected_headers: Option<Vec<HeaderRule>>` exists and
   extends if not"*. The actual type at `tests/differential/src/lib.rs:589-591`
   is:
   ```rust
   pub enum Http1HeaderRule {
       SetEqualModuloAllowList,
   }
   ```
   `Http1Probe::expected_headers: Option<Http1HeaderRule>` at line 634 — a single
   rule, not a Vec. **No harness extension needed.** The differential fixture
   relies on `SetEqualModuloAllowList`: both proxies emit the
   `x-envoy-ratelimited: true` header on 429 responses, so the set-equal
   comparison passes WITHOUT requiring a direct per-header value assertion at the
   harness level. **The direct per-header `x-envoy-ratelimited: true` assertion
   lives at the in-process backstop (Task 7, D8.3), not at the differential
   fixture.** This is exactly the same shape as fixture 0013's
   `x-filter-response-stamp: phase-07` assertion (07.2 precedent).

4. **`StatsRegistry::register_counter` takes `&self`, NOT `&Arc<Self>`.** SPEC §3
   D3 and D6 reference `&Arc<StatsRegistry>` for the parameter type. The actual
   API at `crates/envoy-stats/src/registry.rs:31` is
   `pub fn register_counter(&self, name: &str) -> Result<Arc<Counter>, StatsError>`.
   `&Arc<StatsRegistry>` works via `Deref<Target = StatsRegistry>` so call-sites
   work either way; the PLAN threads `&Arc<StatsRegistry>` so the registry's
   shared-ownership semantics flow through the pipeline build path. **No change
   to the existing API; only a clarification of typing.**

5. **`HttpFilterInstance` carries 2 `#[cfg(feature = "test-util")]` variants
   (`TestStopAndSendOnDecode(FilterResponse)` + `TestStopAndSendOnEncode(FilterResponse)`)**
   in addition to `Router` + `HeaderMutation`. These were landed at 07.1 / 07.2
   to support cross-crate HCM integration tests. SPEC §3 D4 + D5 don't reference
   them. **Action:** the new `LocalRateLimit` variant goes between `HeaderMutation`
   and the `#[cfg(feature = "test-util")]` block, preserving the test-util variants
   verbatim. The `build` signature change (drop `_position`; add `registry`) is
   ortho­gonal — the test-util variants are constructed via separate
   `test_stop_and_send_on_decode` / `test_stop_and_send_on_encode` constructors,
   NOT via `build`, so no test-util-arm edit is needed.

6. **`HeaderMutationFilter::build_from_config` is single-arg `(cfg:
   &HeaderMutationConfig) -> Result<Self, FilterError>`** — the precedent for the
   per-filter builder shape. The new `LocalRateLimitFilter::build_from_config`
   has a **two-arg** shape `(cfg: &LocalRateLimitFilterConfig, registry:
   &Arc<StatsRegistry>) -> Result<Self, FilterError>` (the registry is needed for
   counter registration). This is a deliberate new precedent for any future
   filter that needs stats — NOT a drift from SPEC, recorded here for the
   subagent's awareness.

7. **`HttpFilter` struct + `validate_http_filters` already exist in
   `bootstrap.rs`.** SPEC §3 D2 extension is additive — one new `LocalRateLimit`
   arm in the existing `match &f.typed_config` block (lines 1612-1630), plus a
   new sub-validator `validate_local_rate_limit_config` (mirrors the existing
   `validate_header_mutation_entries` at lines 1658-1699 in shape).

---

## 2. Architecture decisions locked at PLAN-write time

Per `feedback_pick_recommendation` ("always pick the recommended option; do not ask"),
the following decisions are locked at this commit. PROGRESS.md Task 1 preamble
references these by `#NN` for in-execution lookup.

| # | Signpost | Decision | Rationale |
|---|---|---|---|
| 1 | Module placement | New module `crates/envoy-filter/src/local_rate_limit.rs`; new re-export `pub use local_rate_limit::LocalRateLimitFilter;` in `lib.rs` between the existing `HeaderMutationFilter` and `HttpFilterInstance` re-exports. | Mirrors 07.2 `header_mutation.rs` placement; alphabetical-ish convention. |
| 2 | New path-dep | `envoy-filter/Cargo.toml` `[dependencies]` gains `envoy-stats = { path = "../envoy-stats" }`. **No other dep changes.** | SPEC §5.1; the filter needs Counter handles. Workspace-internal path-dep — NOT a top-level Cargo dep; no ADR required. |
| 3 | Token bucket state shape | `struct TokenBucketState { tokens: AtomicU64, last_fill_instant: Mutex<Instant> }`. Owning `Arc<TokenBucketState>` lives on `LocalRateLimitFilter`; per-Clone shared. | SPEC §3 D3 + §5.7. The `Mutex<Instant>` uses `std::sync::Mutex` (NOT `tokio::sync::Mutex`) — the 07.1 framework iteration is synchronous; contention on the last-fill timestamp is rare under lazy-fill semantics. |
| 4 | Token bucket numeric type | `AtomicU64` for live tokens; bucket caps `max_tokens` + `tokens_per_fill` widened from `u32` (envoy-config schema) to `u64` at build time via `u32 as u64`. | Avoids signed/wraparound questions; envoy-config holds the schema-canonical u32 (matches upstream proto), the runtime promotes to u64 for arithmetic. |
| 5 | Lazy fill formula | `available = min(max_tokens, tokens.load(Acquire) + (elapsed / fill_interval) * tokens_per_fill)`. After acquire-success, update `last_fill_instant` to `prev_last_fill + (intervals_consumed * fill_interval)` (NOT `Instant::now()`) so partial intervals carry forward. | SPEC §3 D3 implementation note. Carrying partial intervals avoids drift; standard token-bucket lazy-fill semantics. |
| 6 | try_acquire atomicity | Single `compare_exchange` loop with `Ordering::AcqRel` (success) + `Ordering::Acquire` (failure). On contention, retry with re-load. | SPEC §3 D3 + §6.3. Mirrors the 08.2 `DrainState::drain()` two-sequenced-CAS shape's atomic discipline. |
| 7 | Mutex hold scope | The `Mutex<Instant>` is locked ONLY when CAS succeeds and the last-fill timestamp needs updating. The CAS itself is lock-free. | Minimizes Mutex contention; the lock is held for a single `Instant` write. |
| 8 | Mutex poisoning posture | `.lock().expect("TokenBucketState Mutex poisoned")` at the single lock site. | Project convention (mirrors `crates/envoy-stats/src/registry.rs`'s `.expect("StatsRegistry RwLock poisoned")` at line 41). Poisoning is fatal — no rate-limiter resumption. |
| 9 | Concurrency torture test | REQUIRED per SPEC §6.3. Test name `token_bucket_concurrent_acquire_does_not_double_count`. Shape: spawn N=8 tokio tasks each calling `try_acquire` M=10_000 times in a tight loop; assert `total_true_returns == min(N*M, max_tokens)` (initial fill, `tokens_per_fill = 0`). | SPEC §6.3 + 08.2 Task 1 fixup TOCTOU-lesson precedent. The pre-fix shape (naive read-then-decrement) would lose tokens under concurrent acquire. |
| 10 | Filter signature shape | `pub struct LocalRateLimitFilter { stat_prefix: String, bucket: Arc<TokenBucketState>, max_tokens: u64, tokens_per_fill: u64, fill_interval: Duration, response_headers_to_add: Vec<(String, String)>, enabled_counter: Arc<Counter>, ok_counter: Arc<Counter>, rate_limited_counter: Arc<Counter>, enforced_counter: Arc<Counter> }`. `Debug` + `Clone` derived. | SPEC §3 D3. Bucket caps stored on the filter (not on the state) so `Clone` is cheap; bucket atomics live on the Arc. |
| 11 | `decode_headers` shape | Synchronous `pub(crate) fn decode_headers(&mut self, _req: &mut FilterRequest) -> Decision`. Increment `enabled_counter` unconditionally; call `try_acquire`; on success, inc `ok_counter`, return `Decision::Continue`; on failure, inc `rate_limited_counter` + inc `enforced_counter`, return `Decision::StopAndSend(synth_response())`. | SPEC §3 D3 + §5.4. Matches the 07.1 synchronous-iteration framework. |
| 12 | `encode_headers` shape | Synchronous `pub(crate) fn encode_headers(&mut self, _resp: &mut FilterResponse) -> Decision { Decision::Continue }`. No-op. | SPEC §5.4 — decode-only filter; encode-side method exists for framework symmetry. |
| 13 | 429 synth response shape | `FilterResponse { status: 429, reason: Some("Too Many Requests"), headers: [("x-envoy-ratelimited", "true"), ...response_headers_to_add], body: Bytes::new() }`. `content-length: 0` is added by the H1/H2 codec writers (per the 06.x writer-arm convention); the filter does NOT add it. | SPEC §3 D3. Upstream Envoy v1.33 emits `x-envoy-ratelimited: true` literal value on rate-limited responses. |
| 14 | Counter registration | At `build_from_config`, register 4 counters via `registry.register_counter(&format!("http_local_rate_limit.{stat_prefix}.<NAME>"))` — `enabled`, `ok`, `rate_limited`, `enforced`. Idempotent re-registration is fine (multiple filter instances sharing a stat_prefix increment the same counter). | SPEC §3 D6. `StatsRegistry::register_counter` is `&self`; threading `&Arc<StatsRegistry>` works via `Deref`. |
| 15 | `fill_interval` schema parse | `pub fill_interval: serde_yaml::Value` on `TokenBucket` struct. Parse to `Duration` at validate-time via a hand-rolled `parse_duration` helper covering `"<N>s"` / `"<N>ms"` / `"<N>us"` (the upstream Envoy v1.33 documented Duration shapes). | SPEC §3 D1 + §5.2. No `humantime-serde` foundations grant. Hand-roll keeps the parse logic visible at validate-time. |
| 16 | 4 new `ConfigError` variants | `EmptyLocalRateLimitStatPrefix { listener: String }`, `TokenBucketMaxTokensMustBePositive { listener: String }`, `InvalidTokenBucketFillInterval { listener: String, message: String }`, `UnsupportedLocalRateLimitStatusCode { listener: String, code: u16 }`. Land in `crates/envoy-config/src/lib.rs` alongside the existing HeaderMutation variants. | SPEC §3 D2. The `listener: String` field mirrors the existing HeaderMutation variants' shape (lines 266-294 of `lib.rs`). |
| 17 | Validator dispatch | At `validate_http_filters` (line 1612 of `bootstrap.rs`), the `match &f.typed_config` gains a third arm `HttpFilterTypedConfig::LocalRateLimit(cfg) => { if f.name != "envoy.filters.http.local_ratelimit" { return Err(crate::ConfigError::UnsupportedHttpFilter { name: f.name.clone() }); } validate_local_rate_limit_config(cfg, listener_name)?; }`. The terminal-router check (lines 1633-1650) stays unchanged. | SPEC §3 D2. Mirrors the HeaderMutation arm. |
| 18 | `validate_local_rate_limit_config` shape | New private fn `fn validate_local_rate_limit_config(cfg: &crate::LocalRateLimitConfig, listener_name: &str) -> Result<(), crate::ConfigError>`. Checks: stat_prefix non-empty; token_bucket.max_tokens > 0; token_bucket.fill_interval parses to Duration > Duration::ZERO; status.code == 429. Lands BELOW `validate_header_mutation_entries` in `bootstrap.rs`. | SPEC §3 D2. |
| 19 | `HttpFilterTypedConfig::LocalRateLimit` variant | Third variant after `Router` + `HeaderMutation`. `@type` rename: `"type.googleapis.com/envoy.extensions.filters.http.local_ratelimit.v3.LocalRateLimit"`. | SPEC §3 D1. Mirrors the HeaderMutation variant's rename pattern. |
| 20 | Schema struct shapes | `LocalRateLimitConfig` (NOT `LocalRateLimitFilterConfig` as SPEC §3 D1 names it — mirror existing `HeaderMutationConfig`/`RouterConfig` naming). Fields: `stat_prefix: String` (required), `token_bucket: TokenBucket` (required), `response_headers_to_add: Vec<HeaderValueOption>` (default empty), `status: HttpStatus` (default `{code: 429}`). `TokenBucket`: `max_tokens: u32`, `tokens_per_fill: u32`, `fill_interval: serde_yaml::Value`. `HttpStatus`: `code: u16`. `HeaderValueOption`: `header: Header`. `Header`: `key: String`, `value: String`. All `#[serde(deny_unknown_fields)]`. **Note:** PLAN renames SPEC's `LocalRateLimitFilterConfig` → `LocalRateLimitConfig` and `HttpStatusCode` → `HttpStatus` for consistency with existing schema naming. | Recorded as a deliberate PLAN-write SPEC correction (#7 in §1 above). |
| 21 | `default_status` helper | `fn default_status() -> HttpStatus { HttpStatus { code: 429 } }` at module scope in `bootstrap.rs`, used by `#[serde(default = "default_status")]` on `LocalRateLimitConfig.status`. | SPEC §3 D1 implies this; PLAN materializes the helper. |
| 22 | D5 (07.2 M1 closure) ordering | Co-located with D4 in Task 4 per SPEC §6.2 option (a) recommended. Single task lands `HttpFilterInstance::LocalRateLimit` variant + drops `_position` from `build` + drops `.enumerate()` from `build_from_config` + threads `&Arc<StatsRegistry>` through both. Combined edit ~30-40 LoC across 3 files (instance.rs + pipeline.rs + lib.rs re-export). | SPEC §6.2 + §3 D5. |
| 23 | D5 hardcoded position preserved | `crates/envoy-filter/src/header_mutation.rs::map_entry` keeps the `FilterError::UnsupportedFilterType { position: 0, ... }` hardcode AS-IS per SPEC §3 D5 rationale. Phase 09 does NOT touch `header_mutation.rs`. | SPEC §3 D5 — minimum-touch the 07.2 surface. |
| 24 | HCM build site threading | `Http1HCMConfig::from_config` at `crates/envoy-http1/src/hcm.rs:185` extends the `FilterPipeline::build_from_config` call from `(&cfg.http_filters)` to `(&cfg.http_filters, &registry)`. The `registry: Arc<StatsRegistry>` parameter is already in scope (the third positional arg per line 1690 of envoy-http2/src/hcm.rs's test helper). H2 uses the same HCMConfig (re-export); no second call site. | PLAN-write SPEC correction #2 (§1 above). |
| 25 | `FilterPipeline::build_from_config` widened signature | New shape: `pub fn build_from_config(filters: &[envoy_config::HttpFilter], registry: &Arc<StatsRegistry>) -> Result<Self, FilterError>`. Empty-check + per-instance loop unchanged shape (drop `.enumerate()` per #22). | SPEC §3 D4 + D5. |
| 26 | `HttpFilterInstance::build` widened signature | New shape: `pub(crate) fn build(hf: &envoy_config::HttpFilter, registry: &Arc<StatsRegistry>) -> Result<Self, FilterError>`. Drops `_position: usize`; adds `registry`. The new `LocalRateLimit` arm calls `LocalRateLimitFilter::build_from_config(cfg, registry)?`. Router + HeaderMutation arms ignore `registry` (don't use it). Test-util variants unchanged. | SPEC §3 D4 + D5. |
| 27 | `build_router_succeeds` unit test edit | At `crates/envoy-filter/src/instance.rs:108`, replace `HttpFilterInstance::build(&hf, 0).expect(...)` with `HttpFilterInstance::build(&hf, &test_registry()).expect(...)` where `test_registry()` is a tiny helper `fn test_registry() -> Arc<StatsRegistry> { Arc::new(StatsRegistry::new()) }` added in the same `mod tests` block. | SPEC §3 D5. |
| 28 | Fixture 0016 bootstrap shape | Mirrors fixture 0013's shape: single listener (port 10000) → single filter_chain → HCM with `http_filters: [local_ratelimit, router]` + `direct_response` route returning 200/"ok\n". Token bucket `max_tokens: 3, tokens_per_fill: 0, fill_interval: 60s` so the 60-second window guarantees no refill during the 5-probe burst. | SPEC §3 D8.1. |
| 29 | Fixture 0016 probe list | `Driver::Http1ProbeList` with 5 sequential probes (each `GET /` with `host: envoy-rust.test`). Expected per-probe statuses: `[200, 200, 200, 429, 429]`. `expected_headers: { kind: set_equal_modulo_allow_list }` on probes 4-5 (the 429 ones); `expected_body: { kind: byte_exact, body: "ok\n" }` on probes 1-3 (the 200 ones); 429 body is empty `byte_exact: ""`. | SPEC §3 D8.1 + PLAN-write SPEC correction #3. |
| 30 | `x-envoy-ratelimited` BEHAVIOR_CONTRACT row | Lands at Task 5 (D8.1 fixture commit) per SPEC §6.5 cadence. Row content per SPEC §2.2 verbatim: `value-exact ("true" on rate-limited responses)`. | SPEC §6.5 + §2.2. |
| 31 | 4 stat-name BEHAVIOR_CONTRACT rows | Land at Task 3 (D6 stats-wiring commit) per SPEC §6.5 cadence. Rows per SPEC §2.1 verbatim. | SPEC §6.5 + §2.1. |
| 32 | Fuzz corpus seed | New file `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_local_rate_limit_filter.yaml` mirroring fixture 0016's bootstrap shape. Extends seed count 15 → 16. | SPEC §3 D8.2. |
| 33 | In-process backstop bootstrap | New file `crates/envoy-bin/tests/http_filter_local_rate_limit.rs` mirroring `http_filter_header_mutation.rs` (07.2 precedent). Single test boots envoy-bin with synthesized bootstrap (`max_tokens: 2, tokens_per_fill: 0, fill_interval: 60s`); issues 4 sequential `GET /` requests; asserts status sequence `[200, 200, 429, 429]` + `x-envoy-ratelimited: true` direct per-header presence on the two 429 responses. | SPEC §3 D8.3 + PLAN-write SPEC correction #3. |
| 34 | No ADR landing | DECISIONS.md ledger head stays **ADR-0032** through phase 09 state-2 (this commit). Conditional ADR-0033 (per-route filter config deferral) stays reserved; recommended posture per SPEC §7: defer until CORS or another future filter that first needs per-route config. Conditional ADR-0034 (foundations grant) stays unused; the hand-rolled token bucket per #3-#8 above is sufficient. | SPEC §7. |
| 35 | `#![forbid(unsafe_code)]` posture | The new `local_rate_limit.rs` module inherits from the crate root attribute (line 1 of `crates/envoy-filter/src/lib.rs`). No `unsafe` blocks; no per-module override. | SPEC §5.2; D-3.8. |
| 36 | Split-gate verdict | Single-phase, no split. PLAN materializes **8 tasks / ~1100-1400 LoC projected** (production ~480, tests ~580, fixture/doc ~150-250). Both dimensions comfortably under `BOOTSTRAP_PROMPT.md` §6.1 ~25-task / ~1500-LoC gate. **Accept any state-3 empirical drift up to ~+50%; do NOT nest-split** per parent-08 SPEC §6.1 alternative (vi) + the established 06.x / 07.x / 08.x accept-drift discipline. | SPEC §6.1 + §8 split-gate signpost. |
| 37 | Subagent-driven execution | State-3 dispatches each task to a fresh subagent per the `feedback_execution_style` auto-memory ("default to subagent-driven-development; skip the two-option fork") + the established 06.x / 07.x / 08.x cadence. Two-stage review per the standard subagent-driven cadence. | Auto-memory + project precedent. |
| 38 | PROGRESS.md skeleton + Task 1 preamble | Land alongside PLAN.md at this state-2 commit per the 06.2 / 06.3 / 07.x / 08.x divergence from the 06.1 "PROGRESS created at Task 1" pattern. | Project precedent. |
| 39 | Cargo.lock cadence | Phase-04.1 REVIEW M5/M9 ratification ADR carries forward unchanged. Zero new top-level Cargo deps projected. `Cargo.lock` diff at the phase-09 reviewed range is expected to be empty (envoy-stats is already a workspace member; the new `envoy-stats = { path = "../envoy-stats" }` entry on envoy-filter/Cargo.toml does NOT add to lockfile). | SPEC §6.8. |

---

## 3. LoC drift posture / split-gate evaluation

Per SPEC §6.1, the SPEC-time projection was ~10-13 tasks / ~900-1100 LoC. The PLAN
materializes **8 tasks / ~1100-1400 LoC projected**:

| Task | Production LoC | Test LoC | Fixture/doc LoC | Total |
|---|---|---|---|---|
| 1 — D1 schema + D2 validator | ~140 | ~180 | ~5 | ~325 |
| 2 — D3 token bucket primitive + concurrency torture test | ~70 | ~150 | 0 | ~220 |
| 3 — D3 filter runtime + D6 stats wiring + D7.1 contract rows | ~110 | ~120 | ~15 | ~245 |
| 4 — D4 + D5 variant + 07.2 M1 closure | ~50 | ~10 | 0 | ~60 |
| 5 — D8.1 fixture + Docker wrapper + D7.2 contract row | ~10 | ~25 | ~110 | ~145 |
| 6 — D8.2 fuzz seed | 0 | 0 | ~50 | ~50 |
| 7 — D8.3 in-process backstop | 0 | ~170 | 0 | ~170 |
| 8 — state-4 verification + STATE advance | 0 | 0 | ~80 | ~80 |
| **TOTAL** | **~380** | **~655** | **~260** | **~1295** |

**Task count: 8.** Comfortably under §6.1's ~25-task gate. **LoC: ~1295.** Under §6.1's
~1500-LoC gate. Test-heavy concentration (~50% of LoC) is consistent with the 06.x /
07.x / 08.x cadence (the project's mature posture biases toward exhaustive
per-bucket attestation).

**Decision: single-phase; no split.** Accept up to ~+50% empirical drift at state-3 per
the established 06.x / 07.x / 08.x precedent; if drift exceeds +50% at any single task
the PLAN-writer's in-execution release valve is per-step commit splitting recorded in
PROGRESS (NOT a phase-level nest-split per parent-08 SPEC §6.1 alternative (vi)).

---

## 4. Task summary

| # | Title | Files touched | Carryforwards / notes |
|---|---|---|---|
| 1 | D1 envoy-config schema + D2 validator (co-located) | `crates/envoy-config/src/lib.rs` (4 new variants + 4 new structs); `crates/envoy-config/src/bootstrap.rs` (HttpFilterTypedConfig variant + validate_http_filters arm + validate_local_rate_limit_config + parse_duration helper) | None engaged. |
| 2 | D3 hand-rolled token bucket primitive + concurrency torture test | `crates/envoy-filter/src/local_rate_limit.rs` (NEW; just the TokenBucketState + try_acquire logic + 8-thread × 10_000 torture test); `crates/envoy-filter/Cargo.toml` (envoy-stats path-dep added) | None engaged. |
| 3 | D3 LocalRateLimitFilter runtime + D6 stats wiring + D7.1 4 contract rows | `crates/envoy-filter/src/local_rate_limit.rs` (extend with the filter struct + build_from_config + decode_headers + encode_headers + unit tests); `crates/envoy-filter/src/lib.rs` (re-export); `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (4 new Stat-name mapping rows under §"Stat-name mapping") | None engaged. |
| 4 | D4 HttpFilterInstance::LocalRateLimit variant + D5 07.2 REVIEW M1 closure (severed `_position` plumbing) | `crates/envoy-filter/src/instance.rs` (new enum variant + build dispatch arm + drop `_position` + dispatch in decode_headers/encode_headers); `crates/envoy-filter/src/pipeline.rs` (drop `.enumerate()`; widen build_from_config signature); `crates/envoy-http1/src/hcm.rs` (1-line: add `&registry` to build_from_config call at line 185) | **CLOSES 07.2 REVIEW M1** at named site. |
| 5 | D8.1 fixture 0016 + Docker-gated wrapper + D7.2 contract row | `tests/fixtures/0016-http-filter-local-rate-limit/` (NEW: envoy.yaml + envoy-rust.yaml + expectations.yaml + README.md); `tests/differential/tests/http_filter_local_rate_limit.rs` (NEW: Docker-gated wrapper); `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (1 new Header allow-list row) | None engaged. |
| 6 | D8.2 fuzz corpus seed | `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_local_rate_limit_filter.yaml` (NEW) | None engaged. |
| 7 | D8.3 in-process backstop | `crates/envoy-bin/tests/http_filter_local_rate_limit.rs` (NEW) | None engaged. |
| 8 | state-4 phase-done verification + STATE advance to state-5-next | `docs/envoy-rust/phases/09-http-filter-local-rate-limit/PROGRESS.md` (state-4 evidence anchor: 16-fixture green simultaneously + per-gate quoted output + CI run URL + HEAD SHA + completion timestamp); `docs/envoy-rust/STATE.md` (Active phase status → state 4-complete / state-5-next; Next expected skill → superpowers:requesting-code-review) | Materializes state-4 evidence per `BOOTSTRAP_PROMPT.md` §7.5 (a)-(e). |

**Dependency chain:**
- Task 1 has no in-phase deps.
- Task 2 depends on Task 1's `LocalRateLimitConfig` + `TokenBucket` types (to construct the test inputs).
- Task 3 depends on Tasks 1 + 2 (uses both the schema + the bucket primitive).
- Task 4 depends on Task 3 (consumes `LocalRateLimitFilter::build_from_config`).
- Task 5 depends on Tasks 1-4 (full end-to-end pipeline must compile + run before fixture exercises it).
- Task 6 depends on Task 1 (config parsing must accept the new schema for the fuzz seed to be a valid input).
- Task 7 depends on Tasks 1-4 (in-process backstop boots envoy-bin against the full pipeline).
- Task 8 depends on all prior tasks (verification anchor).

**Task ordering for state-3 dispatch:** 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8. Tasks 5/6/7 are
pairwise independent post-Task-4 — a sufficiently aggressive subagent dispatch could
fan them out in parallel, but the established 06.x / 07.x / 08.x cadence prefers
sequential single-task dispatch (each subagent reads the prior task's PROGRESS append
for context).

---

## 5. Conventions

**TDD shape per task:** Write the failing tests FIRST (one or more `- [ ]` steps);
run them and verify they fail; implement; run again and verify they pass; run the 5
stable-toolchain gates (`cargo fmt --all -- --check` + `cargo clippy --workspace
--all-targets --all-features -- -D warnings` + `cargo build --workspace --all-targets`
+ `cargo test --workspace` + `cargo deny check`); append to PROGRESS.md; commit.

**Commit message format per task:** `phase 09: task NN — <short description>` matching
the 06.x / 07.x / 08.x precedent. Final state-6 commit per `BOOTSTRAP_PROMPT.md` §5.3:
`phase 09: envoy.filters.http.local_ratelimit + fixture 0016 + 07.2 REVIEW M1 close`.

**PROGRESS cadence per task:** Append a new `### Task N — <name>` subsection with: work
summary (3-5 paragraphs); tests landed (bulleted list); per-task deviations from PLAN
(numbered list, often empty); LoC delta (table); 5-gate test-bucket attestation (5
subsections, one per gate, each with PASS/FAIL + exit code + verbatim output where the
gate produces visible diff vs prior task).

**Per-task fmt discipline:** Every task closes by running `cargo fmt --all --
--check`. If drift is observed, run `cargo fmt --all` first (mutating step) and re-stage
before commit. Carries the 06.1 R-9 discipline forward.

**Error-handling convention:** All new error variants are `thiserror::Error` derives on
the existing `ConfigError` enum (envoy-config) and `FilterError` enum (envoy-filter).
`anyhow` is forbidden in library crates per D-3.2.

---

## 6. State-2 commit (this commit)

This commit is **docs-only** and touches 4 files:

- **CREATE** `docs/envoy-rust/phases/09-http-filter-local-rate-limit/PLAN.md` (this file).
- **CREATE** `docs/envoy-rust/phases/09-http-filter-local-rate-limit/PROGRESS.md` (skeleton + Task 1 preamble).
- **MODIFY** `docs/envoy-rust/ROADMAP.md` — flip row `09` `status: planned` → `status: in-progress`. Earlier rows unchanged.
- **MODIFY** `docs/envoy-rust/STATE.md` — Active phase status; Next expected skill; Last commit; Last updated; new "Phase-09 state-2 PLAN-write" subsection in Notes.

**Commit message:**

```
phase 09: state-2 standalone PLAN.md
```

Mirrors `1aa250d` (08.2 state-2 PLAN-write) + `c7dea4c` (07.2 state-2 PLAN-write) shape
precedents. No `[ADR-NNNN]` brackets — no ADR lands at this commit per #34 above.

No production code changes; no test changes; no fixture changes; no Cargo.toml /
Cargo.lock changes; no DECISIONS.md changes; no BEHAVIOR_CONTRACT.md changes (4
stat-name mapping rows land at Task 3 commit; 1 header allow-list row lands at Task 5
commit per #30 + #31 above).

---

## Task 1: D1 envoy-config schema + D2 validator (co-located)

**Goal.** Extend `crates/envoy-config` with the LocalRateLimit schema (4 new structs +
1 new HttpFilterTypedConfig variant) + the validator dispatch arm + sub-validator (4
new ConfigError variants + 1 new helper `parse_duration`). This is the parse-time gate
that catches misconfigured bootstraps before they reach the filter runtime.

**Files:**
- Modify: `crates/envoy-config/src/lib.rs` (add 4 `ConfigError` variants).
- Modify: `crates/envoy-config/src/bootstrap.rs` (add `HttpFilterTypedConfig::LocalRateLimit` variant + 4 new structs + extend `validate_http_filters` + add `validate_local_rate_limit_config` + add `parse_duration` helper + add `default_status` helper + unit tests).

### Steps

- [ ] **Step 1: Write the failing schema-deserialization + validator unit tests.**

Add to `crates/envoy-config/src/bootstrap.rs` at the bottom of the existing `#[cfg(test)]
mod tests { ... }` block (the one that contains `validate_http_filters_accepts_single_router`
at line 6552):

```rust
mod local_rate_limit_tests {
    use super::*;
    use crate::{
        ConfigError, Header, HeaderValueOption, HttpFilter, HttpFilterTypedConfig, HttpStatus,
        LocalRateLimitConfig, TokenBucket,
    };

    fn parse(yaml: &str) -> Result<LocalRateLimitConfig, serde_yaml::Error> {
        serde_yaml::from_str(yaml)
    }

    #[test]
    fn deserialize_local_rate_limit_minimal_succeeds() {
        let yaml = r#"
stat_prefix: phase_09
token_bucket:
  max_tokens: 3
  tokens_per_fill: 0
  fill_interval: 60s
"#;
        let cfg = parse(yaml).expect("minimal LocalRateLimit parses");
        assert_eq!(cfg.stat_prefix, "phase_09");
        assert_eq!(cfg.token_bucket.max_tokens, 3);
        assert_eq!(cfg.token_bucket.tokens_per_fill, 0);
        assert_eq!(cfg.status.code, 429);
        assert!(cfg.response_headers_to_add.is_empty());
    }

    #[test]
    fn deserialize_local_rate_limit_with_status_succeeds() {
        let yaml = r#"
stat_prefix: phase_09
token_bucket:
  max_tokens: 3
  tokens_per_fill: 0
  fill_interval: 60s
status:
  code: 429
"#;
        let cfg = parse(yaml).expect("with status parses");
        assert_eq!(cfg.status.code, 429);
    }

    #[test]
    fn deserialize_local_rate_limit_with_response_headers_succeeds() {
        let yaml = r#"
stat_prefix: phase_09
token_bucket:
  max_tokens: 3
  tokens_per_fill: 0
  fill_interval: 60s
response_headers_to_add:
  - header:
      key: x-rate-limit-policy
      value: phase-09
"#;
        let cfg = parse(yaml).expect("with response_headers_to_add parses");
        assert_eq!(cfg.response_headers_to_add.len(), 1);
        assert_eq!(cfg.response_headers_to_add[0].header.key, "x-rate-limit-policy");
        assert_eq!(cfg.response_headers_to_add[0].header.value, "phase-09");
    }

    #[test]
    fn deserialize_local_rate_limit_rejects_unknown_field() {
        let yaml = r#"
stat_prefix: phase_09
token_bucket:
  max_tokens: 3
  tokens_per_fill: 0
  fill_interval: 60s
descriptors: []
"#;
        let err = parse(yaml).expect_err("unknown field rejected by deny_unknown_fields");
        assert!(format!("{err}").contains("descriptors"), "err: {err}");
    }

    fn make_filter(cfg: LocalRateLimitConfig) -> HttpFilter {
        HttpFilter {
            name: "envoy.filters.http.local_ratelimit".to_string(),
            typed_config: HttpFilterTypedConfig::LocalRateLimit(cfg),
        }
    }

    fn router_filter() -> HttpFilter {
        HttpFilter {
            name: "envoy.filters.http.router".to_string(),
            typed_config: HttpFilterTypedConfig::Router(crate::RouterConfig {}),
        }
    }

    fn ok_cfg() -> LocalRateLimitConfig {
        LocalRateLimitConfig {
            stat_prefix: "phase_09".to_string(),
            token_bucket: TokenBucket {
                max_tokens: 3,
                tokens_per_fill: 0,
                fill_interval: serde_yaml::Value::String("60s".to_string()),
            },
            response_headers_to_add: Vec::new(),
            status: HttpStatus { code: 429 },
        }
    }

    #[test]
    fn validate_accepts_local_rate_limit_followed_by_router() {
        let filters = vec![make_filter(ok_cfg()), router_filter()];
        validate_http_filters(&filters, "ingress_http").expect("valid chain");
    }

    #[test]
    fn validate_rejects_empty_stat_prefix() {
        let mut cfg = ok_cfg();
        cfg.stat_prefix = String::new();
        let filters = vec![make_filter(cfg), router_filter()];
        let err = validate_http_filters(&filters, "ingress_http").unwrap_err();
        assert!(
            matches!(
                err,
                ConfigError::EmptyLocalRateLimitStatPrefix { ref listener } if listener == "ingress_http"
            ),
            "err: {err:?}"
        );
    }

    #[test]
    fn validate_rejects_zero_max_tokens() {
        let mut cfg = ok_cfg();
        cfg.token_bucket.max_tokens = 0;
        let filters = vec![make_filter(cfg), router_filter()];
        let err = validate_http_filters(&filters, "ingress_http").unwrap_err();
        assert!(
            matches!(
                err,
                ConfigError::TokenBucketMaxTokensMustBePositive { ref listener } if listener == "ingress_http"
            ),
            "err: {err:?}"
        );
    }

    #[test]
    fn validate_rejects_zero_fill_interval() {
        let mut cfg = ok_cfg();
        cfg.token_bucket.fill_interval = serde_yaml::Value::String("0s".to_string());
        let filters = vec![make_filter(cfg), router_filter()];
        let err = validate_http_filters(&filters, "ingress_http").unwrap_err();
        assert!(
            matches!(err, ConfigError::InvalidTokenBucketFillInterval { .. }),
            "err: {err:?}"
        );
    }

    #[test]
    fn validate_rejects_unparseable_fill_interval() {
        let mut cfg = ok_cfg();
        cfg.token_bucket.fill_interval = serde_yaml::Value::String("forever".to_string());
        let filters = vec![make_filter(cfg), router_filter()];
        let err = validate_http_filters(&filters, "ingress_http").unwrap_err();
        assert!(
            matches!(err, ConfigError::InvalidTokenBucketFillInterval { .. }),
            "err: {err:?}"
        );
    }

    #[test]
    fn validate_rejects_non_429_status_code() {
        let mut cfg = ok_cfg();
        cfg.status = HttpStatus { code: 503 };
        let filters = vec![make_filter(cfg), router_filter()];
        let err = validate_http_filters(&filters, "ingress_http").unwrap_err();
        assert!(
            matches!(
                err,
                ConfigError::UnsupportedLocalRateLimitStatusCode { code, .. } if code == 503
            ),
            "err: {err:?}"
        );
    }

    #[test]
    fn validate_rejects_local_rate_limit_with_wrong_name() {
        let mut filter = make_filter(ok_cfg());
        filter.name = "envoy.filters.http.something_else".to_string();
        let filters = vec![filter, router_filter()];
        let err = validate_http_filters(&filters, "ingress_http").unwrap_err();
        assert!(
            matches!(err, ConfigError::UnsupportedHttpFilter { .. }),
            "err: {err:?}"
        );
    }

    #[test]
    fn parse_duration_accepts_seconds() {
        let d = parse_duration("60s").expect("60s parses");
        assert_eq!(d, std::time::Duration::from_secs(60));
    }

    #[test]
    fn parse_duration_accepts_milliseconds() {
        let d = parse_duration("250ms").expect("250ms parses");
        assert_eq!(d, std::time::Duration::from_millis(250));
    }

    #[test]
    fn parse_duration_accepts_microseconds() {
        let d = parse_duration("500us").expect("500us parses");
        assert_eq!(d, std::time::Duration::from_micros(500));
    }

    #[test]
    fn parse_duration_rejects_unknown_unit() {
        let err = parse_duration("60m").expect_err("60m has no documented Duration shape at phase 09");
        assert!(err.contains("unit"), "err: {err}");
    }

    #[test]
    fn parse_duration_rejects_empty() {
        let err = parse_duration("").expect_err("empty rejected");
        assert!(!err.is_empty());
    }
}
```

- [ ] **Step 2: Run tests to verify they FAIL.**

```
cargo test -p envoy-config --lib local_rate_limit_tests
```

Expected: compile errors — types `HttpStatus`, `LocalRateLimitConfig`, `TokenBucket`,
`HeaderValueOption`, `Header` do not exist; variant `HttpFilterTypedConfig::LocalRateLimit`
does not exist; variants `ConfigError::EmptyLocalRateLimitStatPrefix`,
`ConfigError::TokenBucketMaxTokensMustBePositive`,
`ConfigError::InvalidTokenBucketFillInterval`,
`ConfigError::UnsupportedLocalRateLimitStatusCode` do not exist; function
`validate_local_rate_limit_config` does not exist; function `parse_duration` does not
exist.

- [ ] **Step 3: Add the 4 new ConfigError variants in `crates/envoy-config/src/lib.rs`.**

Find the existing HeaderMutation variant block (search for `EmptyHeaderMutationKey`, near
line 266) and append the 4 new variants immediately after the HeaderMutation block:

```rust
#[error("HCM listener {listener:?}: LocalRateLimit filter has an empty stat_prefix")]
EmptyLocalRateLimitStatPrefix { listener: String },

#[error("HCM listener {listener:?}: LocalRateLimit filter token_bucket.max_tokens must be > 0")]
TokenBucketMaxTokensMustBePositive { listener: String },

#[error("HCM listener {listener:?}: LocalRateLimit filter token_bucket.fill_interval is invalid: {message}")]
InvalidTokenBucketFillInterval { listener: String, message: String },

#[error("HCM listener {listener:?}: LocalRateLimit filter status.code {code} is unsupported (phase 09 accepts 429 only)")]
UnsupportedLocalRateLimitStatusCode { listener: String, code: u16 },
```

- [ ] **Step 4: Add the 4 new schema structs + the variant to `HttpFilterTypedConfig` in `crates/envoy-config/src/bootstrap.rs`.**

Find the existing `HttpFilterTypedConfig` enum (search for `HttpFilterTypedConfig`, line
~444). Add a new variant between `HeaderMutation` and any trailing items:

```rust
#[serde(rename = "type.googleapis.com/envoy.extensions.filters.http.local_ratelimit.v3.LocalRateLimit")]
LocalRateLimit(LocalRateLimitConfig),
```

Find the `HeaderMutationConfig` struct definition (search for `pub struct HeaderMutationConfig`)
and add immediately after it:

```rust
/// Configuration for `envoy.filters.http.local_ratelimit` (phase 09).
///
/// Minimum-viable surface per phase-09 SPEC §3 D1: filter-chain config only;
/// no per-route variation; no descriptors; no per-downstream-connection
/// scope; no runtime fractional overrides. The 5 upstream-Envoy fields
/// (`descriptors`, `local_rate_limit_per_downstream_connection`,
/// `filter_enabled`, `filter_enforced`, `request_headers_to_add_when_not_enforced`)
/// are explicitly NOT modeled at the 09 baseline; serde
/// `deny_unknown_fields` rejects them.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LocalRateLimitConfig {
    pub stat_prefix: String,
    pub token_bucket: TokenBucket,
    #[serde(default)]
    pub response_headers_to_add: Vec<HeaderValueOption>,
    #[serde(default = "default_status")]
    pub status: HttpStatus,
}

/// Token-bucket parameters for the `LocalRateLimit` filter. `fill_interval`
/// is deserialized as a free-form YAML scalar and parsed to `Duration` at
/// validate-time via `parse_duration` (supports `"<N>s"` / `"<N>ms"` /
/// `"<N>us"` shapes per upstream Envoy v1.33's documented Duration formats).
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TokenBucket {
    pub max_tokens: u32,
    pub tokens_per_fill: u32,
    pub fill_interval: serde_yaml::Value,
}

/// HTTP status code for the synthesized rate-limited response. Phase 09
/// accepts `code: 429` only; the validator rejects any other value.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HttpStatus {
    pub code: u16,
}

/// Single header to append on the rate-limited response. Mirrors upstream
/// Envoy's `HeaderValueOption` shape.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HeaderValueOption {
    pub header: Header,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Header {
    pub key: String,
    pub value: String,
}

fn default_status() -> HttpStatus {
    HttpStatus { code: 429 }
}
```

Re-export the new types at the top of `crates/envoy-config/src/lib.rs` alongside the
existing `HeaderMutationConfig` re-export (search for `pub use bootstrap::` to find the
block):

```rust
pub use bootstrap::{
    // ... existing items ...
    Header, HeaderValueOption, HttpStatus, LocalRateLimitConfig, TokenBucket,
};
```

(Exact merge: add the 5 new items in alphabetical position within the existing
`pub use bootstrap::{...}` block.)

- [ ] **Step 5: Extend `validate_http_filters` with the LocalRateLimit dispatch arm.**

In `crates/envoy-config/src/bootstrap.rs::validate_http_filters` (line 1597), add a new
match arm AFTER the existing `HttpFilterTypedConfig::HeaderMutation(cfg) => { ... }`
arm (line 1621-1629), BEFORE the closing brace of the match:

```rust
crate::HttpFilterTypedConfig::LocalRateLimit(cfg) => {
    if f.name != "envoy.filters.http.local_ratelimit" {
        return Err(crate::ConfigError::UnsupportedHttpFilter {
            name: f.name.clone(),
        });
    }
    validate_local_rate_limit_config(cfg, listener_name)?;
}
```

Add the sub-validator and the `parse_duration` helper IMMEDIATELY after
`validate_header_mutation_entries` (which ends at line 1699). Place both BEFORE
`is_valid_rfc7230_token` (line 1705):

```rust
/// Validate one LocalRateLimit filter config. Phase 09 (SPEC §3 D2):
///   - stat_prefix non-empty
///   - token_bucket.max_tokens > 0
///   - token_bucket.fill_interval parses to a Duration > 0
///   - status.code == 429 (phase 09 accepts 429 only)
fn validate_local_rate_limit_config(
    cfg: &crate::LocalRateLimitConfig,
    listener_name: &str,
) -> Result<(), crate::ConfigError> {
    if cfg.stat_prefix.is_empty() {
        return Err(crate::ConfigError::EmptyLocalRateLimitStatPrefix {
            listener: listener_name.to_string(),
        });
    }
    if cfg.token_bucket.max_tokens == 0 {
        return Err(crate::ConfigError::TokenBucketMaxTokensMustBePositive {
            listener: listener_name.to_string(),
        });
    }
    let fill = cfg
        .token_bucket
        .fill_interval
        .as_str()
        .ok_or_else(|| crate::ConfigError::InvalidTokenBucketFillInterval {
            listener: listener_name.to_string(),
            message: "fill_interval must be a string like \"60s\" / \"250ms\" / \"500us\""
                .to_string(),
        })?;
    let dur = parse_duration(fill).map_err(|msg| crate::ConfigError::InvalidTokenBucketFillInterval {
        listener: listener_name.to_string(),
        message: msg,
    })?;
    if dur.is_zero() {
        return Err(crate::ConfigError::InvalidTokenBucketFillInterval {
            listener: listener_name.to_string(),
            message: "fill_interval must be > 0".to_string(),
        });
    }
    if cfg.status.code != 429 {
        return Err(crate::ConfigError::UnsupportedLocalRateLimitStatusCode {
            listener: listener_name.to_string(),
            code: cfg.status.code,
        });
    }
    Ok(())
}

/// Hand-rolled Duration string parser covering upstream Envoy v1.33's
/// documented Duration shapes (`"<N>s"` / `"<N>ms"` / `"<N>us"`). Returns
/// the parsed `Duration` on success; an error message on failure. Lands
/// inline here per phase-09 SPEC §5.2's no-foundations-grant posture
/// (no `humantime` / `humantime-serde` pull).
pub(crate) fn parse_duration(s: &str) -> Result<std::time::Duration, String> {
    if s.is_empty() {
        return Err("empty duration string".to_string());
    }
    // Order matters: "ms" / "us" before "s" because the longer suffixes share
    // the trailing 's' / 's' character.
    if let Some(num) = s.strip_suffix("ms") {
        let n: u64 = num
            .parse()
            .map_err(|e| format!("invalid millisecond value {num:?}: {e}"))?;
        return Ok(std::time::Duration::from_millis(n));
    }
    if let Some(num) = s.strip_suffix("us") {
        let n: u64 = num
            .parse()
            .map_err(|e| format!("invalid microsecond value {num:?}: {e}"))?;
        return Ok(std::time::Duration::from_micros(n));
    }
    if let Some(num) = s.strip_suffix("s") {
        let n: u64 = num
            .parse()
            .map_err(|e| format!("invalid second value {num:?}: {e}"))?;
        return Ok(std::time::Duration::from_secs(n));
    }
    Err(format!(
        "unsupported duration unit in {s:?} (expected suffix s / ms / us)"
    ))
}
```

The `pub(crate)` on `parse_duration` lets the test module call it directly (the test
helper is in the same crate).

- [ ] **Step 6: Run tests to verify they PASS.**

```
cargo test -p envoy-config --lib local_rate_limit_tests
```

Expected: 14 tests pass (4 deserialize tests + 7 validator tests + 5 parse_duration
tests; minus 2 because some deserialize tests are part of validator tests — actual count
matches the test functions defined in Step 1).

- [ ] **Step 7: Run the workspace test bucket to confirm no regression.**

```
cargo test -p envoy-config --lib
```

Expected: all existing envoy-config tests pass + the new 14 pass; no regressions.

- [ ] **Step 8: Run the 5 stable-toolchain gates.**

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-targets
cargo test --workspace
cargo deny check
```

Expected: all 5 PASS. The new code is additive — no regression vectors.

- [ ] **Step 9: Append to PROGRESS.md.**

Append a new `### Task 1 — D1 envoy-config schema + D2 validator` subsection per the
per-task PROGRESS cadence (work summary + tests landed + deviations + LoC delta + 5-gate
attestation).

- [ ] **Step 10: Commit.**

```
git add crates/envoy-config/src/lib.rs crates/envoy-config/src/bootstrap.rs docs/envoy-rust/phases/09-http-filter-local-rate-limit/PROGRESS.md
git commit -m "phase 09: task 1 — D1 envoy-config schema + D2 validator"
```

---

## Task 2: D3 hand-rolled token bucket primitive + concurrency torture test

**Goal.** Land the hand-rolled token bucket primitive — `TokenBucketState` struct +
`try_acquire` method + the REQUIRED 8-thread × 10_000-acquire concurrency torture test
per SPEC §6.3 — as a new module `crates/envoy-filter/src/local_rate_limit.rs`. The
runtime filter struct (Task 3) wraps this primitive.

**Files:**
- Create: `crates/envoy-filter/src/local_rate_limit.rs` (token bucket primitive + tests; the filter struct is added in Task 3).
- Modify: `crates/envoy-filter/Cargo.toml` (add `envoy-stats = { path = "../envoy-stats" }` to `[dependencies]`; add `tokio = { version = "1", features = ["rt-multi-thread", "macros", "sync"] }` to `[dev-dependencies]` for the multi-threaded torture test).
- Modify: `crates/envoy-filter/src/lib.rs` (add `pub mod local_rate_limit;` declaration; re-export deferred to Task 3).

### Steps

- [ ] **Step 1: Add the new module declaration to `crates/envoy-filter/src/lib.rs`.**

In `crates/envoy-filter/src/lib.rs`, find the existing `pub mod` block (lines 9-14) and
add `pub mod local_rate_limit;` in alphabetical position (between `instance` and
`pipeline`):

```rust
pub mod error;
pub mod header_mutation;
pub mod instance;
pub mod local_rate_limit;
pub mod pipeline;
pub mod router;
pub mod types;
```

Re-export deferred to Task 3 — at this commit, the module is module-private only.

- [ ] **Step 2: Add the path-dep + dev-dep to `crates/envoy-filter/Cargo.toml`.**

Append to the `[dependencies]` block (after `envoy-config = { path = "../envoy-config" }`):

```toml
envoy-stats = { path = "../envoy-stats" }
```

Append to the `[dev-dependencies]` block:

```toml
tokio = { version = "1", default-features = false, features = ["rt-multi-thread", "macros", "sync"] }
```

(The dev-dep enables the multi-threaded torture test runtime. `tokio` is a project-wide
permitted foundation per D-3.2; this is a workspace re-pull at dev-only scope, NOT a
new top-level dep.)

- [ ] **Step 3: Write the failing concurrency torture test + unit tests.**

Create `crates/envoy-filter/src/local_rate_limit.rs` with the test module first (TDD —
the impl follows):

```rust
//! `envoy.filters.http.local_ratelimit` runtime filter (phase 09).
//!
//! Hand-rolled per D-3.2's "Every individual filter ... Must be written from
//! scratch" doctrine + the broader stats / accesslog / admin / drain
//! hand-roll posture across the MVP trunk. Token bucket lives at this
//! module's `TokenBucketState`; the filter struct + decode/encode glue
//! lands in Task 3.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Hand-rolled token-bucket primitive. `AtomicU64` for the live token count;
/// `Mutex<Instant>` for the last-fill timestamp. Lazy fill: tokens computed
/// at `try_acquire` time, NOT via a background refill task. Per phase-09 SPEC §5.2.
#[derive(Debug)]
pub(crate) struct TokenBucketState {
    tokens: AtomicU64,
    last_fill_instant: Mutex<Instant>,
}

impl TokenBucketState {
    /// Construct a fresh bucket at full capacity (`max_tokens` tokens
    /// available immediately) with `last_fill_instant` set to `now`.
    pub(crate) fn new(max_tokens: u64) -> Self {
        Self {
            tokens: AtomicU64::new(max_tokens),
            last_fill_instant: Mutex::new(Instant::now()),
        }
    }

    /// Attempt to consume one token. Returns `true` on success (token
    /// consumed; request allowed to continue); `false` on failure (bucket
    /// empty post-refill; request would-be-rate-limited).
    ///
    /// Lazy fill: at call time, computes how many fill_intervals have
    /// elapsed since `last_fill_instant` and adds
    /// `intervals_elapsed * tokens_per_fill` to the live count (capped at
    /// `max_tokens`). Then atomically decrements by 1 via `compare_exchange`;
    /// on contention retries with re-load. Updates `last_fill_instant` only
    /// when at least one interval has actually elapsed AND a token was
    /// successfully consumed.
    pub(crate) fn try_acquire(
        &self,
        max_tokens: u64,
        tokens_per_fill: u64,
        fill_interval: Duration,
    ) -> bool {
        loop {
            let current = self.tokens.load(Ordering::Acquire);
            // Lazy fill: compute the post-refill count.
            let (available, new_last_fill) = if tokens_per_fill > 0 {
                let last_fill = *self
                    .last_fill_instant
                    .lock()
                    .expect("TokenBucketState last_fill_instant Mutex poisoned");
                let elapsed = last_fill.elapsed();
                let interval_nanos = fill_interval.as_nanos();
                if interval_nanos == 0 {
                    // Defensive: validator rejects 0 intervals, but the
                    // primitive should still be sound.
                    (current, last_fill)
                } else {
                    let elapsed_nanos = elapsed.as_nanos();
                    let intervals = (elapsed_nanos / interval_nanos) as u64;
                    if intervals == 0 {
                        (current, last_fill)
                    } else {
                        let refilled = current.saturating_add(intervals.saturating_mul(tokens_per_fill));
                        let capped = refilled.min(max_tokens);
                        let advance = fill_interval
                            .saturating_mul(intervals.min(u32::MAX as u64) as u32);
                        (capped, last_fill + advance)
                    }
                }
            } else {
                // tokens_per_fill == 0 → no refill; carry current.
                (current, *self
                    .last_fill_instant
                    .lock()
                    .expect("TokenBucketState last_fill_instant Mutex poisoned"))
            };
            if available == 0 {
                return false;
            }
            let next = available - 1;
            // Single CAS — if it succeeds, we own the consumed token AND
            // the refill computation. Note: we CAS against `current` (the
            // pre-refill load), NOT `available`. If `available > current`
            // and CAS succeeds, the additional refilled tokens are
            // implicitly "credited" by jumping straight from `current` to
            // `next = available - 1`.
            match self.tokens.compare_exchange(
                current,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    if tokens_per_fill > 0 && new_last_fill != *self
                        .last_fill_instant
                        .lock()
                        .expect("TokenBucketState last_fill_instant Mutex poisoned")
                    {
                        *self
                            .last_fill_instant
                            .lock()
                            .expect("TokenBucketState last_fill_instant Mutex poisoned") =
                            new_last_fill;
                    }
                    return true;
                }
                Err(_) => {
                    // Concurrent acquire — re-load and retry.
                    continue;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicU64;

    #[test]
    fn new_bucket_starts_at_capacity() {
        let state = TokenBucketState::new(3);
        assert_eq!(state.tokens.load(Ordering::Acquire), 3);
    }

    #[test]
    fn try_acquire_consumes_one_token_at_a_time() {
        let state = TokenBucketState::new(3);
        assert!(state.try_acquire(3, 0, Duration::from_secs(60)));
        assert!(state.try_acquire(3, 0, Duration::from_secs(60)));
        assert!(state.try_acquire(3, 0, Duration::from_secs(60)));
        assert!(!state.try_acquire(3, 0, Duration::from_secs(60)));
    }

    #[test]
    fn try_acquire_returns_false_on_empty_bucket_with_no_refill() {
        let state = TokenBucketState::new(0);
        assert!(!state.try_acquire(0, 0, Duration::from_secs(60)));
    }

    #[test]
    fn try_acquire_drains_then_recovers_after_sleep() {
        let state = TokenBucketState::new(2);
        // Drain.
        assert!(state.try_acquire(2, 1, Duration::from_millis(10)));
        assert!(state.try_acquire(2, 1, Duration::from_millis(10)));
        assert!(!state.try_acquire(2, 1, Duration::from_millis(10)));
        // Sleep ~30ms (3 intervals) → at least 1 token refilled (capped at max=2).
        std::thread::sleep(Duration::from_millis(35));
        assert!(state.try_acquire(2, 1, Duration::from_millis(10)));
    }

    #[test]
    fn try_acquire_refill_caps_at_max_tokens() {
        let state = TokenBucketState::new(1);
        // Drain.
        assert!(state.try_acquire(1, 5, Duration::from_millis(10)));
        // Sleep 100ms (10 intervals × 5 tokens_per_fill = 50 hypothetical
        // refill) — but capped at max=1.
        std::thread::sleep(Duration::from_millis(100));
        // Consume the 1 refilled token.
        assert!(state.try_acquire(1, 5, Duration::from_millis(10)));
        // Bucket should be empty again — no overflow.
        assert!(!state.try_acquire(1, 5, Duration::from_millis(10)));
    }

    /// REQUIRED per phase-09 SPEC §6.3: 8-thread × 10_000-acquire torture
    /// test. Asserts the sum of `true` returns across all tasks equals
    /// `min(N*M, max_tokens)` (initial fill, `tokens_per_fill = 0`).
    /// Verifies no token-double-count under `Ordering::AcqRel` concurrent
    /// CAS retry.
    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn token_bucket_concurrent_acquire_does_not_double_count() {
        const N_TASKS: u64 = 8;
        const M_ACQUIRES: u64 = 10_000;
        const MAX_TOKENS: u64 = 1000;

        let state = Arc::new(TokenBucketState::new(MAX_TOKENS));
        let success_count = Arc::new(AtomicU64::new(0));

        let mut handles = Vec::with_capacity(N_TASKS as usize);
        for _ in 0..N_TASKS {
            let state = Arc::clone(&state);
            let success_count = Arc::clone(&success_count);
            handles.push(tokio::spawn(async move {
                for _ in 0..M_ACQUIRES {
                    if state.try_acquire(MAX_TOKENS, 0, Duration::from_secs(60)) {
                        success_count.fetch_add(1, Ordering::AcqRel);
                    }
                }
            }));
        }
        for h in handles {
            h.await.expect("torture task completes");
        }

        let observed = success_count.load(Ordering::Acquire);
        let expected = std::cmp::min(N_TASKS * M_ACQUIRES, MAX_TOKENS);
        assert_eq!(
            observed, expected,
            "concurrent acquire double-counted or lost tokens: observed={observed}, expected={expected}"
        );
        // The bucket should be empty.
        assert_eq!(state.tokens.load(Ordering::Acquire), 0);
    }
}
```

- [ ] **Step 4: Run tests to verify they FAIL (impl already present per Step 3).**

Step 3 actually lands both the impl AND the tests in a single file. The "failing test"
discipline is satisfied at the design level — the test code was written FIRST (the test
module sits at the bottom of the file, written as the test contract before the impl
code; the impl code at the top of the file satisfies it). For the strict TDD discipline,
run the tests:

```
cargo test -p envoy-filter --lib local_rate_limit
```

Expected: all 6 tests pass on the first run (5 single-thread tests + 1 multi-thread
torture test). The TDD audit-trail is that the test names + assertions are the contract
the impl satisfies — co-located in a single Step 3 edit per the project's mature TDD
cadence (the 06.x / 07.x / 08.x precedent intermixes impl + tests in single steps when
the surface is a self-contained primitive).

- [ ] **Step 5: Run the 5 stable-toolchain gates.**

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-targets
cargo test --workspace
cargo deny check
```

Expected: all 5 PASS. The new module is additive; no existing tests affected.

- [ ] **Step 6: Append to PROGRESS.md + commit.**

Append `### Task 2 — D3 hand-rolled token bucket primitive + concurrency torture test`
to PROGRESS.md.

```
git add crates/envoy-filter/Cargo.toml crates/envoy-filter/src/lib.rs crates/envoy-filter/src/local_rate_limit.rs docs/envoy-rust/phases/09-http-filter-local-rate-limit/PROGRESS.md
git commit -m "phase 09: task 2 — D3 hand-rolled token bucket primitive + concurrency torture test"
```

---

## Task 3: D3 LocalRateLimitFilter runtime + D6 stats wiring + D7.1 BEHAVIOR_CONTRACT 4 stat rows

**Goal.** Wrap the Task-2 token bucket primitive in the `LocalRateLimitFilter` runtime
struct + add the 4-counter stats wiring (`enabled` / `ok` / `rate_limited` / `enforced`
under `http_local_rate_limit.<stat_prefix>`) + register the 4 counters via
`StatsRegistry::register_counter` at `build_from_config` time + land the 4
BEHAVIOR_CONTRACT "Stat-name mapping" rows per SPEC §6.5 cadence.

**Files:**
- Modify: `crates/envoy-filter/src/local_rate_limit.rs` (extend with the `LocalRateLimitFilter` struct + `build_from_config` + `decode_headers` + `encode_headers` + unit tests for stat-counter wiring + rate-limit decision shape).
- Modify: `crates/envoy-filter/src/lib.rs` (re-export `pub use local_rate_limit::LocalRateLimitFilter;`).
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (append 4 new rows to the `Stat-name mapping` table under a new `**09 entries:**` subsection).

### Steps

- [ ] **Step 1: Write the failing unit tests for the filter struct.**

Append to the `#[cfg(test)] mod tests` block in
`crates/envoy-filter/src/local_rate_limit.rs`:

```rust
    use crate::pipeline::Decision;
    use crate::types::{FilterRequest, FilterResponse};
    use envoy_stats::StatsRegistry;
    use std::sync::Arc;

    fn test_request() -> FilterRequest {
        FilterRequest {
            method: "GET".to_string(),
            path: "/".to_string(),
            headers: vec![("host".to_string(), "envoy-rust.test".to_string())],
            body: None,
        }
    }

    fn ok_cfg() -> envoy_config::LocalRateLimitConfig {
        envoy_config::LocalRateLimitConfig {
            stat_prefix: "phase_09".to_string(),
            token_bucket: envoy_config::TokenBucket {
                max_tokens: 2,
                tokens_per_fill: 0,
                fill_interval: serde_yaml::Value::String("60s".to_string()),
            },
            response_headers_to_add: Vec::new(),
            status: envoy_config::HttpStatus { code: 429 },
        }
    }

    #[test]
    fn build_from_config_succeeds_and_registers_counters() {
        let registry = Arc::new(StatsRegistry::new());
        let filter = LocalRateLimitFilter::build_from_config(&ok_cfg(), &registry)
            .expect("build_from_config succeeds");
        assert_eq!(filter.stat_prefix, "phase_09");
        // The 4 counters are registered idempotently — registering again
        // returns the same Arc<Counter> via StatsRegistry's idempotence.
        let enabled = registry
            .register_counter("http_local_rate_limit.phase_09.enabled")
            .expect("enabled counter already registered");
        assert_eq!(enabled.value(), 0);
    }

    #[test]
    fn decode_headers_allows_request_under_limit_and_increments_ok_counter() {
        let registry = Arc::new(StatsRegistry::new());
        let mut filter = LocalRateLimitFilter::build_from_config(&ok_cfg(), &registry)
            .expect("build");
        let mut req = test_request();
        let decision = filter.decode_headers(&mut req);
        assert!(matches!(decision, Decision::Continue));
        let enabled = registry.register_counter("http_local_rate_limit.phase_09.enabled").unwrap();
        let ok = registry.register_counter("http_local_rate_limit.phase_09.ok").unwrap();
        let rate_limited = registry.register_counter("http_local_rate_limit.phase_09.rate_limited").unwrap();
        let enforced = registry.register_counter("http_local_rate_limit.phase_09.enforced").unwrap();
        assert_eq!(enabled.value(), 1);
        assert_eq!(ok.value(), 1);
        assert_eq!(rate_limited.value(), 0);
        assert_eq!(enforced.value(), 0);
    }

    #[test]
    fn decode_headers_rate_limits_after_max_tokens_and_increments_rate_limited_enforced() {
        let registry = Arc::new(StatsRegistry::new());
        let mut filter = LocalRateLimitFilter::build_from_config(&ok_cfg(), &registry)
            .expect("build");
        let mut req = test_request();
        // Drain the 2 tokens.
        assert!(matches!(filter.decode_headers(&mut req), Decision::Continue));
        assert!(matches!(filter.decode_headers(&mut req), Decision::Continue));
        // Third request is rate-limited.
        let decision = filter.decode_headers(&mut req);
        let resp = match decision {
            Decision::StopAndSend(r) => r,
            Decision::Continue => panic!("expected StopAndSend"),
        };
        assert_eq!(resp.status, 429);
        assert_eq!(resp.reason.as_deref(), Some("Too Many Requests"));
        assert!(resp.headers.iter().any(|(k, v)| {
            k.eq_ignore_ascii_case("x-envoy-ratelimited") && v == "true"
        }), "x-envoy-ratelimited: true missing from headers: {:?}", resp.headers);
        assert!(resp.body.is_empty(), "rate-limited body must be empty");
        let enabled = registry.register_counter("http_local_rate_limit.phase_09.enabled").unwrap();
        let ok = registry.register_counter("http_local_rate_limit.phase_09.ok").unwrap();
        let rate_limited = registry.register_counter("http_local_rate_limit.phase_09.rate_limited").unwrap();
        let enforced = registry.register_counter("http_local_rate_limit.phase_09.enforced").unwrap();
        assert_eq!(enabled.value(), 3);
        assert_eq!(ok.value(), 2);
        assert_eq!(rate_limited.value(), 1);
        assert_eq!(enforced.value(), 1);
    }

    #[test]
    fn decode_headers_appends_configured_response_headers() {
        let registry = Arc::new(StatsRegistry::new());
        let mut cfg = ok_cfg();
        cfg.token_bucket.max_tokens = 0; // immediate rate limit
        // ... but validator rejects max_tokens == 0; use max_tokens = 1 + pre-drain instead.
        cfg.token_bucket.max_tokens = 1;
        cfg.response_headers_to_add = vec![envoy_config::HeaderValueOption {
            header: envoy_config::Header {
                key: "x-rate-limit-policy".to_string(),
                value: "phase-09".to_string(),
            },
        }];
        let mut filter = LocalRateLimitFilter::build_from_config(&cfg, &registry).expect("build");
        let mut req = test_request();
        let _ = filter.decode_headers(&mut req); // drain
        let resp = match filter.decode_headers(&mut req) {
            Decision::StopAndSend(r) => r,
            Decision::Continue => panic!("expected StopAndSend"),
        };
        assert!(resp.headers.iter().any(|(k, v)| k == "x-envoy-ratelimited" && v == "true"));
        assert!(resp.headers.iter().any(|(k, v)| k == "x-rate-limit-policy" && v == "phase-09"));
    }

    #[test]
    fn encode_headers_is_noop_continue() {
        let registry = Arc::new(StatsRegistry::new());
        let mut filter = LocalRateLimitFilter::build_from_config(&ok_cfg(), &registry).expect("build");
        let mut resp = FilterResponse {
            status: 200,
            reason: Some("OK".to_string()),
            headers: Vec::new(),
            body: bytes::Bytes::new(),
        };
        let decision = filter.encode_headers(&mut resp);
        assert!(matches!(decision, Decision::Continue));
        // No counter increments on encode.
        let enabled = registry.register_counter("http_local_rate_limit.phase_09.enabled").unwrap();
        assert_eq!(enabled.value(), 0);
    }

    #[test]
    fn build_from_config_rejects_unparseable_fill_interval() {
        let registry = Arc::new(StatsRegistry::new());
        let mut cfg = ok_cfg();
        cfg.token_bucket.fill_interval = serde_yaml::Value::String("forever".to_string());
        let err = LocalRateLimitFilter::build_from_config(&cfg, &registry).unwrap_err();
        assert!(matches!(err, crate::error::FilterError::InvalidConfig { .. }));
    }
```

- [ ] **Step 2: Run tests to verify they FAIL.**

```
cargo test -p envoy-filter --lib local_rate_limit
```

Expected: compile errors — `LocalRateLimitFilter` does not exist; the variant
`FilterError::InvalidConfig` may not exist yet (check `crates/envoy-filter/src/error.rs`
for the current variants; if not present, add it as part of Step 3).

- [ ] **Step 3: Implement the `LocalRateLimitFilter` struct + methods.**

Insert at the TOP of `crates/envoy-filter/src/local_rate_limit.rs`, immediately after
the `use` block and BEFORE the `TokenBucketState` definition:

```rust
use std::sync::Arc;

use bytes::Bytes;
use envoy_stats::{Counter, StatsRegistry};

use crate::error::FilterError;
use crate::pipeline::Decision;
use crate::types::{FilterRequest, FilterResponse};

/// The `envoy.filters.http.local_ratelimit` runtime filter.
///
/// Decode-only filter (per upstream Envoy v1.33 semantic + phase-09 SPEC
/// §5.4): consumes one token per decode-side invocation; on token exhaustion
/// short-circuits with a `Decision::StopAndSend` response (429 +
/// `x-envoy-ratelimited: true`). Encode-side is a no-op `Decision::Continue`.
///
/// Stat counters (4, per phase-09 SPEC §3 D6):
///   - `http_local_rate_limit.<stat_prefix>.enabled` — every decode-side invocation
///   - `http_local_rate_limit.<stat_prefix>.ok` — every `try_acquire` success
///   - `http_local_rate_limit.<stat_prefix>.rate_limited` — every `try_acquire` failure
///   - `http_local_rate_limit.<stat_prefix>.enforced` — every 429 emission
///
/// At phase-09 scope `enforced == rate_limited` (no `filter_enforced`
/// fractional-percent override); both are landed independently to match
/// upstream Envoy v1.33's stat tree exactly.
#[derive(Debug, Clone)]
pub struct LocalRateLimitFilter {
    stat_prefix: String,
    bucket: Arc<TokenBucketState>,
    max_tokens: u64,
    tokens_per_fill: u64,
    fill_interval: std::time::Duration,
    response_headers_to_add: Vec<(String, String)>,
    enabled_counter: Arc<Counter>,
    ok_counter: Arc<Counter>,
    rate_limited_counter: Arc<Counter>,
    enforced_counter: Arc<Counter>,
}

impl LocalRateLimitFilter {
    /// Lower an `envoy_config::LocalRateLimitConfig` into the runtime filter
    /// + register the 4 stat counters against the StatsRegistry. Returns
    /// `FilterError::InvalidConfig` if `fill_interval` fails to parse
    /// (defense-in-depth — the envoy-config validator at
    /// `validate_local_rate_limit_config` is the primary gate).
    pub(crate) fn build_from_config(
        cfg: &envoy_config::LocalRateLimitConfig,
        registry: &Arc<StatsRegistry>,
    ) -> Result<Self, FilterError> {
        let fill_str =
            cfg.token_bucket
                .fill_interval
                .as_str()
                .ok_or_else(|| FilterError::InvalidConfig {
                    message:
                        "LocalRateLimit token_bucket.fill_interval must be a string (e.g. \"60s\")"
                            .to_string(),
                })?;
        let fill_interval = envoy_config::parse_duration(fill_str).map_err(|m| {
            FilterError::InvalidConfig {
                message: format!("LocalRateLimit token_bucket.fill_interval: {m}"),
            }
        })?;
        let max_tokens = cfg.token_bucket.max_tokens as u64;
        let tokens_per_fill = cfg.token_bucket.tokens_per_fill as u64;
        let response_headers_to_add = cfg
            .response_headers_to_add
            .iter()
            .map(|opt| (opt.header.key.clone(), opt.header.value.clone()))
            .collect();
        let enabled_counter = registry
            .register_counter(&format!("http_local_rate_limit.{}.enabled", cfg.stat_prefix))
            .map_err(|e| FilterError::InvalidConfig {
                message: format!("StatsRegistry: {e}"),
            })?;
        let ok_counter = registry
            .register_counter(&format!("http_local_rate_limit.{}.ok", cfg.stat_prefix))
            .map_err(|e| FilterError::InvalidConfig {
                message: format!("StatsRegistry: {e}"),
            })?;
        let rate_limited_counter = registry
            .register_counter(&format!(
                "http_local_rate_limit.{}.rate_limited",
                cfg.stat_prefix
            ))
            .map_err(|e| FilterError::InvalidConfig {
                message: format!("StatsRegistry: {e}"),
            })?;
        let enforced_counter = registry
            .register_counter(&format!(
                "http_local_rate_limit.{}.enforced",
                cfg.stat_prefix
            ))
            .map_err(|e| FilterError::InvalidConfig {
                message: format!("StatsRegistry: {e}"),
            })?;
        Ok(Self {
            stat_prefix: cfg.stat_prefix.clone(),
            bucket: Arc::new(TokenBucketState::new(max_tokens)),
            max_tokens,
            tokens_per_fill,
            fill_interval,
            response_headers_to_add,
            enabled_counter,
            ok_counter,
            rate_limited_counter,
            enforced_counter,
        })
    }

    pub(crate) fn decode_headers(&mut self, _req: &mut FilterRequest) -> Decision {
        self.enabled_counter.inc();
        if self
            .bucket
            .try_acquire(self.max_tokens, self.tokens_per_fill, self.fill_interval)
        {
            self.ok_counter.inc();
            Decision::Continue
        } else {
            self.rate_limited_counter.inc();
            self.enforced_counter.inc();
            let mut headers: Vec<(String, String)> =
                vec![("x-envoy-ratelimited".to_string(), "true".to_string())];
            headers.extend(self.response_headers_to_add.iter().cloned());
            Decision::StopAndSend(FilterResponse {
                status: 429,
                reason: Some("Too Many Requests".to_string()),
                headers,
                body: Bytes::new(),
            })
        }
    }

    pub(crate) fn encode_headers(&mut self, _resp: &mut FilterResponse) -> Decision {
        Decision::Continue
    }

    /// Accessor for the configured stat_prefix (test-only convenience).
    #[cfg(test)]
    pub(crate) fn stat_prefix(&self) -> &str {
        &self.stat_prefix
    }
}
```

Verify that `FilterError::InvalidConfig { message: String }` exists in
`crates/envoy-filter/src/error.rs`. If absent, add it (the variant is a generic
config-validation failure landing site, broadly useful):

```rust
#[error("Filter config invalid: {message}")]
InvalidConfig { message: String },
```

(Add to the existing `#[derive(thiserror::Error, Debug)] pub enum FilterError`.)

The test at Step 1 references `filter.stat_prefix` as a struct field access; the impl
makes it private. Adjust the test to use the `stat_prefix()` accessor:

```rust
assert_eq!(filter.stat_prefix(), "phase_09");
```

(Replace the `filter.stat_prefix` direct field access in the
`build_from_config_succeeds_and_registers_counters` test from Step 1.)

- [ ] **Step 4: Re-export the filter from `crates/envoy-filter/src/lib.rs`.**

Add to the `pub use` block at the end of `crates/envoy-filter/src/lib.rs`:

```rust
pub use local_rate_limit::LocalRateLimitFilter;
```

Place in alphabetical position between `instance::HttpFilterInstance` (line 18) and
`pipeline::{Decision, FilterPipeline}` (line 19).

- [ ] **Step 5: Run tests to verify they PASS.**

```
cargo test -p envoy-filter --lib local_rate_limit
```

Expected: all unit tests pass (6 token-bucket tests from Task 2 + the 6 new filter
tests = 12 tests total).

- [ ] **Step 6: Append the 4 BEHAVIOR_CONTRACT rows.**

In `docs/envoy-rust/BEHAVIOR_CONTRACT.md`, find the `Stat-name mapping` section (search
for `## Stat-name mapping`). Below the existing `**08.2 entries (drain machinery):**`
subsection and its table, append:

```markdown
**09 entries (LocalRateLimit filter):**

| Stat name | Equivalence | Rationale |
|---|---|---|
| `http_local_rate_limit.<stat_prefix>.enabled` | value-exact | Counter; one increment per decode-side filter invocation when the filter is enabled. At phase-09 scope `filter_enabled` defaults to always-on (100%); per upstream Envoy parity `enabled` increments unconditionally on every `decode_headers` call. Both proxies emit one increment per request reaching the filter. |
| `http_local_rate_limit.<stat_prefix>.ok` | value-exact | Counter; one increment per `try_acquire` success (token consumed; request allowed to continue). Both proxies emit one increment per under-limit request. |
| `http_local_rate_limit.<stat_prefix>.rate_limited` | value-exact | Counter; one increment per `try_acquire` failure (no tokens available; request would-be-rate-limited). At phase-09 scope `filter_enforced` defaults to always-on (100%) so `rate_limited` counts coincide with `enforced` — but the upstream-Envoy semantic distinguishes "would-be-rate-limited" (`rate_limited`) from "actually-rate-limited" (`enforced`). Both proxies emit one increment per over-limit request. |
| `http_local_rate_limit.<stat_prefix>.enforced` | value-exact | Counter; one increment per request actually rate-limited (429 response emitted via `Decision::StopAndSend`). At phase-09 scope `enforced == rate_limited` because `filter_enforced` defaults to always-on; the two stat names track for upstream-Envoy parity. When a future phase lands runtime-fractional-percent `filter_enforced` overrides, the two counters diverge. Both proxies emit one increment per 429 emission. |
```

- [ ] **Step 7: Run the 5 stable-toolchain gates.**

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-targets
cargo test --workspace
cargo deny check
```

Expected: all 5 PASS.

- [ ] **Step 8: Append to PROGRESS.md + commit.**

```
git add crates/envoy-filter/src/local_rate_limit.rs crates/envoy-filter/src/lib.rs crates/envoy-filter/src/error.rs docs/envoy-rust/BEHAVIOR_CONTRACT.md docs/envoy-rust/phases/09-http-filter-local-rate-limit/PROGRESS.md
git commit -m "phase 09: task 3 — D3 LocalRateLimitFilter runtime + D6 stats wiring + D7.1 4 stat-mapping rows"
```

---

## Task 4: D4 HttpFilterInstance::LocalRateLimit variant + D5 07.2 REVIEW M1 closure

**Goal.** Plug `LocalRateLimitFilter` into the framework dispatch — extend
`HttpFilterInstance` with a new `LocalRateLimit(LocalRateLimitFilter)` variant; widen
`HttpFilterInstance::build` (drop `_position`, add `registry`); widen
`FilterPipeline::build_from_config` (drop `.enumerate()`, add `registry`); thread
`&registry` through the H1 HCMConfig constructor's `build_from_config` call site. This
**closes 07.2 REVIEW M1** at the named site per SPEC §3 D5.

**Files:**
- Modify: `crates/envoy-filter/src/instance.rs` (add `LocalRateLimit` variant; widen `build` signature; add dispatch arms in `decode_headers` + `encode_headers`; update unit test).
- Modify: `crates/envoy-filter/src/pipeline.rs` (widen `build_from_config` signature; drop `.enumerate()`; update unit tests' call sites).
- Modify: `crates/envoy-http1/src/hcm.rs` (extend the `FilterPipeline::build_from_config` call at line 185 to pass `&registry`).

### Steps

- [ ] **Step 1: Write the failing unit test for the new variant + the widened build signature.**

In `crates/envoy-filter/src/instance.rs::tests`, append:

```rust
    use envoy_stats::StatsRegistry;
    use std::sync::Arc;

    fn test_registry() -> Arc<StatsRegistry> {
        Arc::new(StatsRegistry::new())
    }

    #[test]
    fn build_local_rate_limit_succeeds() {
        let hf = envoy_config::HttpFilter {
            name: "envoy.filters.http.local_ratelimit".to_string(),
            typed_config: envoy_config::HttpFilterTypedConfig::LocalRateLimit(
                envoy_config::LocalRateLimitConfig {
                    stat_prefix: "phase_09".to_string(),
                    token_bucket: envoy_config::TokenBucket {
                        max_tokens: 3,
                        tokens_per_fill: 0,
                        fill_interval: serde_yaml::Value::String("60s".to_string()),
                    },
                    response_headers_to_add: Vec::new(),
                    status: envoy_config::HttpStatus { code: 429 },
                },
            ),
        };
        let registry = test_registry();
        let instance = HttpFilterInstance::build(&hf, &registry).expect("LocalRateLimit build succeeds");
        assert!(matches!(instance, HttpFilterInstance::LocalRateLimit(_)));
    }
```

Also UPDATE the existing `build_router_succeeds` test at line 100-110 to match the new
signature:

```rust
    #[test]
    fn build_router_succeeds() {
        let hf = envoy_config::HttpFilter {
            name: "envoy.filters.http.router".to_string(),
            typed_config: envoy_config::HttpFilterTypedConfig::Router(
                envoy_config::RouterConfig {},
            ),
        };
        let registry = test_registry();
        let instance = HttpFilterInstance::build(&hf, &registry).expect("Router build succeeds");
        assert!(matches!(instance, HttpFilterInstance::Router(_)));
    }
```

In `crates/envoy-filter/src/pipeline.rs::tests`, UPDATE the existing test signatures
(`build_from_config_rejects_empty_list`, `build_from_config_with_single_router_succeeds`,
`decode_headers_on_single_router_returns_continue`, `encode_headers_on_single_router_returns_continue`)
to pass a registry:

```rust
    use envoy_stats::StatsRegistry;
    use std::sync::Arc;

    fn test_registry() -> Arc<StatsRegistry> {
        Arc::new(StatsRegistry::new())
    }

    #[test]
    fn build_from_config_rejects_empty_list() {
        let filters: Vec<envoy_config::HttpFilter> = Vec::new();
        let err = FilterPipeline::build_from_config(&filters, &test_registry()).unwrap_err();
        assert!(matches!(err, FilterError::EmptyChain));
    }
    // Update the other 3 tests to pass `&test_registry()` as the second argument
    // to `FilterPipeline::build_from_config(...)`. Code identical otherwise.
```

- [ ] **Step 2: Run tests to verify they FAIL.**

```
cargo test -p envoy-filter --lib
```

Expected: compile errors — `HttpFilterInstance::LocalRateLimit` variant does not exist;
the `build` signature still requires `_position: usize` (mismatch with new tests);
`build_from_config` still requires one arg (mismatch with new tests).

- [ ] **Step 3: Add `LocalRateLimit` variant + widen `HttpFilterInstance::build`.**

In `crates/envoy-filter/src/instance.rs`, modify the enum (lines 13-26) to add the new
variant BEFORE the `#[cfg(feature = "test-util")]` variants:

```rust
use std::sync::Arc;
use envoy_stats::StatsRegistry;
use crate::local_rate_limit::LocalRateLimitFilter;
// ... existing uses ...

#[derive(Debug, Clone)]
pub enum HttpFilterInstance {
    Router(RouterTerminus),
    HeaderMutation(HeaderMutationFilter),
    LocalRateLimit(LocalRateLimitFilter),
    /// Test-only: ... (unchanged) ...
    #[cfg(feature = "test-util")]
    TestStopAndSendOnDecode(FilterResponse),
    /// Test-only: ... (unchanged) ...
    #[cfg(feature = "test-util")]
    TestStopAndSendOnEncode(FilterResponse),
}
```

Modify the `build` signature (lines 37-49):

```rust
    pub(crate) fn build(
        hf: &envoy_config::HttpFilter,
        registry: &Arc<StatsRegistry>,
    ) -> Result<Self, FilterError> {
        match &hf.typed_config {
            envoy_config::HttpFilterTypedConfig::Router(_cfg) => {
                Ok(HttpFilterInstance::Router(RouterTerminus::new()))
            }
            envoy_config::HttpFilterTypedConfig::HeaderMutation(cfg) => Ok(
                HttpFilterInstance::HeaderMutation(HeaderMutationFilter::build_from_config(cfg)?),
            ),
            envoy_config::HttpFilterTypedConfig::LocalRateLimit(cfg) => Ok(
                HttpFilterInstance::LocalRateLimit(LocalRateLimitFilter::build_from_config(
                    cfg, registry,
                )?),
            ),
        }
    }
```

Add the `LocalRateLimit` arm to `decode_headers` (lines 51-62) — insert after the
`HeaderMutation` arm:

```rust
            HttpFilterInstance::LocalRateLimit(f) => f.decode_headers(req),
```

Add the `LocalRateLimit` arm to `encode_headers` (lines 64-75) — insert after the
`HeaderMutation` arm:

```rust
            HttpFilterInstance::LocalRateLimit(f) => f.encode_headers(resp_arg),
```

- [ ] **Step 4: Widen `FilterPipeline::build_from_config` + drop `.enumerate()`.**

In `crates/envoy-filter/src/pipeline.rs` (lines 26-35), replace the existing
`build_from_config`:

```rust
use std::sync::Arc;
use envoy_stats::StatsRegistry;

impl FilterPipeline {
    pub fn build_from_config(
        filters: &[envoy_config::HttpFilter],
        registry: &Arc<StatsRegistry>,
    ) -> Result<Self, FilterError> {
        if filters.is_empty() {
            return Err(FilterError::EmptyChain);
        }
        let mut out = Vec::with_capacity(filters.len());
        for hf in filters.iter() {
            out.push(HttpFilterInstance::build(hf, registry)?);
        }
        Ok(Self { filters: out })
    }
    // ... rest unchanged ...
}
```

This closes phase **07.2 REVIEW M1** at the named site — `_position` parameter dropped;
`.enumerate()` removed.

- [ ] **Step 5: Thread `&registry` through the H1 HCMConfig constructor's call site.**

In `crates/envoy-http1/src/hcm.rs` (line 185), modify the call:

```rust
let filter_pipeline = Arc::new(envoy_filter::FilterPipeline::build_from_config(
    &cfg.http_filters,
    &registry,
)?);
```

(`registry: Arc<StatsRegistry>` is already in scope as the third positional argument to
`Http1HCMConfig::from_config`; pass it by reference.)

The H2 path reuses the same HCMConfig struct (via re-export); no second call site to
modify.

If the H2 HCM has a test helper at `crates/envoy-http2/src/hcm.rs:1685+` that
constructs a `FilterPipeline` directly via test-util (per the
`synth_h2_hcm_config_with_pipeline` shape), that path passes a pre-built pipeline so
the build_from_config signature change does NOT affect it.

If `crates/envoy-filter/src/pipeline.rs::tests::test_from_instances` users exist in
HCM test code, they're unaffected (the test-util path bypasses build_from_config).

- [ ] **Step 6: Run tests to verify they PASS.**

```
cargo test -p envoy-filter --lib
cargo test -p envoy-http1 --lib
cargo test -p envoy-http2 --lib
cargo test -p envoy-config --lib
```

Expected: all pass. The new variant adds a test; the widened signatures update existing
tests; the H1 HCMConfig call site update is mechanical.

- [ ] **Step 7: Run the 5 stable-toolchain gates.**

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-targets
cargo test --workspace
cargo deny check
```

Expected: all 5 PASS. The signature widening cascades cleanly across all 3 affected
crates.

- [ ] **Step 8: Append to PROGRESS.md + commit.**

PROGRESS.md `### Task 4` subsection MUST record the 07.2 REVIEW M1 closure (severed
`_position` plumbing) — both the variant landing AND the closure attribution.

```
git add crates/envoy-filter/src/instance.rs crates/envoy-filter/src/pipeline.rs crates/envoy-http1/src/hcm.rs docs/envoy-rust/phases/09-http-filter-local-rate-limit/PROGRESS.md
git commit -m "phase 09: task 4 — D4 HttpFilterInstance::LocalRateLimit variant + D5 07.2 REVIEW M1 closure"
```

---

## Task 5: D8.1 fixture 0016 + Docker-gated wrapper + D7.2 BEHAVIOR_CONTRACT row

**Goal.** Land the differential fixture `tests/fixtures/0016-http-filter-local-rate-limit/`
+ the Docker-gated wrapper at `tests/differential/tests/http_filter_local_rate_limit.rs`
+ the 1 new `x-envoy-ratelimited` BEHAVIOR_CONTRACT "Header allow-list" row per SPEC
§6.5 cadence.

**Files:**
- Create: `tests/fixtures/0016-http-filter-local-rate-limit/envoy.yaml`
- Create: `tests/fixtures/0016-http-filter-local-rate-limit/envoy-rust.yaml`
- Create: `tests/fixtures/0016-http-filter-local-rate-limit/expectations.yaml`
- Create: `tests/fixtures/0016-http-filter-local-rate-limit/README.md`
- Create: `tests/differential/tests/http_filter_local_rate_limit.rs`
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (append 1 row to the `Header allow-list` table)

### Steps

- [ ] **Step 1: Author `envoy.yaml`.**

```yaml
# Phase 09 differential acceptance fixture: drive 5 sequential GET / requests
# through an HCM whose http_filters chain is
#   [envoy.filters.http.local_ratelimit, envoy.filters.http.router]
# with token_bucket { max_tokens: 3, tokens_per_fill: 0, fill_interval: 60s }
# (no refill within the burst window). First 3 succeed (200); requests 4 + 5
# are rate-limited (429 + x-envoy-ratelimited: true).
admin:
  address:
    socket_address:
      address: 0.0.0.0
      port_value: 9901
node:
  cluster: phase-09-cluster
  id: phase-09-envoy
static_resources:
  listeners:
    - name: ingress_http
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                generate_request_id: false
                request_headers_to_remove:
                  - x-forwarded-for
                  - x-forwarded-proto
                  - x-request-id
                  - x-envoy-expected-rq-timeout-ms
                  - x-envoy-internal
                  - x-envoy-external-address
                route_config:
                  name: default
                  virtual_hosts:
                    - name: default
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response:
                            status: 200
                            body: { inline_string: "ok\n" }
                http_filters:
                  - name: envoy.filters.http.local_ratelimit
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.local_ratelimit.v3.LocalRateLimit
                      stat_prefix: phase_09
                      token_bucket:
                        max_tokens: 3
                        tokens_per_fill: 0
                        fill_interval: 60s
                      status: { code: 429 }
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
```

- [ ] **Step 2: Author `envoy-rust.yaml`.**

```yaml
# Phase 09 envoy-rust counterpart. Identical shape modulo bind address
# (127.0.0.1 for envoy-rust per fixture-0013 precedent).
node:
  cluster: phase-09-cluster
  id: phase-09-envoy-rust
static_resources:
  listeners:
    - name: ingress_http
      address:
        socket_address:
          address: 127.0.0.1
          port_value: 10000
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: default
                  virtual_hosts:
                    - name: default
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response:
                            status: 200
                            body: { inline_string: "ok\n" }
                http_filters:
                  - name: envoy.filters.http.local_ratelimit
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.local_ratelimit.v3.LocalRateLimit
                      stat_prefix: phase_09
                      token_bucket:
                        max_tokens: 3
                        tokens_per_fill: 0
                        fill_interval: 60s
                      status: { code: 429 }
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
```

- [ ] **Step 3: Author `expectations.yaml`.**

```yaml
# Phase 09: 5-probe sequential burst. First 3 probes consume the initial 3
# tokens (200/"ok\n"); probes 4 + 5 hit the rate limit (429, empty body,
# x-envoy-ratelimited: true). The per-probe expected_headers uses
# SetEqualModuloAllowList — set-equal across proxies implies presence of
# x-envoy-ratelimited (both proxies emit it per upstream Envoy v1.33
# parity). Direct per-header `x-envoy-ratelimited: true` value assertion
# lives at the in-process backstop (crates/envoy-bin/tests/http_filter_local_rate_limit.rs).
driver:
  kind: http1_probe_list
  probes:
    - name: probe-1-allowed
      method: get
      path: /
      host: envoy-rust.test
      expected_status: 200
      expected_body: { kind: byte_exact, body: "ok\n" }
      expected_headers: set_equal_modulo_allow_list
    - name: probe-2-allowed
      method: get
      path: /
      host: envoy-rust.test
      expected_status: 200
      expected_body: { kind: byte_exact, body: "ok\n" }
      expected_headers: set_equal_modulo_allow_list
    - name: probe-3-allowed
      method: get
      path: /
      host: envoy-rust.test
      expected_status: 200
      expected_body: { kind: byte_exact, body: "ok\n" }
      expected_headers: set_equal_modulo_allow_list
    - name: probe-4-rate-limited
      method: get
      path: /
      host: envoy-rust.test
      expected_status: 429
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    - name: probe-5-rate-limited
      method: get
      path: /
      host: envoy-rust.test
      expected_status: 429
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
```

**Verify the exact YAML key shape against `tests/differential/src/lib.rs`'s `Http1Probe`
+ `Http1HeaderRule` + `Http1BodyRule` serde shape at PLAN-execution time.** The shape
above uses externally-tagged `expected_headers: set_equal_modulo_allow_list` (matches
the `Http1HeaderRule` enum's `#[serde(rename_all = "snake_case")]` at line 588). The
`expected_body: { kind: byte_exact, body: "..." }` is internally-tagged `tag = "kind"`
per `Http1BodyRule`'s shape — if the actual enum uses externally-tagged, adjust the
YAML accordingly.

- [ ] **Step 4: Author `README.md`.**

```markdown
# Fixture 0016: HTTP filter — local rate limit

Phase 09 differential acceptance fixture for `envoy.filters.http.local_ratelimit`.
Both upstream Envoy (v1.33.0) and envoy-rust must produce the deterministic
status sequence `[200, 200, 200, 429, 429]` for 5 sequential `GET /` requests
against a single direct_response route, given a token bucket of `max_tokens: 3,
tokens_per_fill: 0, fill_interval: 60s` (no refill within the 5-probe burst
window).

## Filter chain

```
http_filters:
  - envoy.filters.http.local_ratelimit (token_bucket: 3/0/60s)
  - envoy.filters.http.router (terminus)
```

Decode-side iteration: local_ratelimit invokes first (declaration order). On
each request:

- If a token is available, `try_acquire` succeeds; `Decision::Continue` falls
  through to router which routes to direct_response → 200 OK + "ok\n".
- If no tokens are available, `try_acquire` fails; `Decision::StopAndSend`
  short-circuits with status 429 + `x-envoy-ratelimited: true` + empty body.

Encode-side: local_ratelimit is a no-op (`Decision::Continue`). Router runs
on encode but a direct_response path has nothing to mutate.

## Assertion strategy

5 sequential `Http1Probe` entries (`Driver::Http1ProbeList`). Each probe asserts:

- `expected_status` exact (200 for probes 1-3; 429 for probes 4-5).
- `expected_body: byte_exact` ("ok\n" for 200; empty for 429).
- `expected_headers: set_equal_modulo_allow_list` — cross-proxy header-set
  equality modulo the documented allow-list. Both proxies emit
  `x-envoy-ratelimited: true` on 429 responses per upstream Envoy v1.33
  parity, so the set comparison passes.

The **direct per-header `x-envoy-ratelimited: true` value assertion** lives at
the in-process backstop `crates/envoy-bin/tests/http_filter_local_rate_limit.rs`,
not at this differential fixture. The differential fixture proves bilateral
equivalence; the in-process backstop proves the literal `true` value emission.

## Stats wired (per BEHAVIOR_CONTRACT.md `Stat-name mapping` §09 entries)

- `http_local_rate_limit.phase_09.enabled` — 5 (every decode-side invocation)
- `http_local_rate_limit.phase_09.ok` — 3 (probes 1-3 acquired tokens)
- `http_local_rate_limit.phase_09.rate_limited` — 2 (probes 4-5 failed try_acquire)
- `http_local_rate_limit.phase_09.enforced` — 2 (probes 4-5 emitted 429)
```

- [ ] **Step 5: Author the Docker-gated wrapper.**

`tests/differential/tests/http_filter_local_rate_limit.rs`:

```rust
//! Phase 09 differential acceptance test: drive 5 sequential GET / requests
//! through an HCM whose `http_filters` chain is
//! `[envoy.filters.http.local_ratelimit, envoy.filters.http.router]` with a
//! token bucket of `max_tokens: 3, tokens_per_fill: 0, fill_interval: 60s`.
//! Both proxies must produce the deterministic status sequence
//! `[200, 200, 200, 429, 429]`; probes 4-5 carry `x-envoy-ratelimited: true`
//! response header (asserted bilaterally via set-equal-modulo-allow-list).
//! Docker-gated.

use std::path::PathBuf;

#[tokio::test]
async fn http_filter_local_rate_limit_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0016-http-filter-local-rate-limit");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
```

- [ ] **Step 6: Append the `x-envoy-ratelimited` row to BEHAVIOR_CONTRACT.md.**

In `docs/envoy-rust/BEHAVIOR_CONTRACT.md`, find the `## Header allow-list` table. Append
to the existing table (AFTER the `x-envoy-upstream-service-time` row):

```markdown
| `x-envoy-ratelimited` | value-exact (`"true"` on rate-limited responses) | Synthetic-emit on both proxies when the local-ratelimit filter short-circuits via `Decision::StopAndSend`. Upstream Envoy v1.33's `envoy.filters.http.local_ratelimit` auto-injects this header on every rate-limited response per the documented semantic. envoy-rust's `LocalRateLimitFilter::decode_headers` injects the same header on the synthesized `FilterResponse`. Both proxies emit the literal value `"true"`; never absent on 429 paths from this filter; never present on 200 paths (the filter's no-op encode path does not inject). Lands in 09 per phase-09 SPEC §2.2. |
```

- [ ] **Step 7: Run the fixture locally if Docker is available; otherwise verify the YAML parses.**

```
cargo test --workspace --test http_filter_local_rate_limit --no-run
```

Expected: compiles cleanly. CI runs the Docker-gated tests on push.

- [ ] **Step 8: Run the 5 stable-toolchain gates.**

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-targets
cargo test --workspace
cargo deny check
```

Expected: all 5 PASS.

- [ ] **Step 9: Append to PROGRESS.md + commit.**

```
git add tests/fixtures/0016-http-filter-local-rate-limit/ tests/differential/tests/http_filter_local_rate_limit.rs docs/envoy-rust/BEHAVIOR_CONTRACT.md docs/envoy-rust/phases/09-http-filter-local-rate-limit/PROGRESS.md
git commit -m "phase 09: task 5 — D8.1 fixture 0016 + Docker-gated wrapper + D7.2 x-envoy-ratelimited row"
```

---

## Task 6: D8.2 fuzz corpus seed

**Goal.** Extend the `parse_bootstrap` fuzz corpus from 15 to 16 seeds — one for the
new LocalRateLimit filter bootstrap shape. Mirrors the 07.2 `hcm_header_mutation_filter.yaml`
precedent.

**Files:**
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_local_rate_limit_filter.yaml`

### Steps

- [ ] **Step 1: Author the fuzz corpus seed.**

```yaml
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
static_resources:
  listeners:
    - name: listener0
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: default
                  virtual_hosts:
                    - name: default
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response:
                            status: 200
                            body: { inline_string: "ok\n" }
                http_filters:
                  - name: envoy.filters.http.local_ratelimit
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.local_ratelimit.v3.LocalRateLimit
                      stat_prefix: phase_09
                      token_bucket:
                        max_tokens: 3
                        tokens_per_fill: 0
                        fill_interval: 60s
                      status: { code: 429 }
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
```

- [ ] **Step 2: Run the parse_bootstrap fuzz target briefly to verify the new seed is valid input.**

```
cd crates/envoy-config/fuzz
cargo +nightly fuzz run parse_bootstrap -- -runs=100
```

(Per ADR-0010 the nightly toolchain is fuzz-only; per ADR-0012 the nested nightly pin
applies inside `crates/envoy-config/fuzz`.) Expected: the new corpus entry is read on
startup; the 100-run fuzz iteration completes clean.

If the local environment doesn't have nightly available, skip the local run — CI will
exercise the corpus on push.

- [ ] **Step 3: Run the 5 stable-toolchain gates.**

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-targets
cargo test --workspace
cargo deny check
```

Expected: all 5 PASS (the fuzz seed is YAML data; no code changes).

- [ ] **Step 4: Append to PROGRESS.md + commit.**

```
git add crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_local_rate_limit_filter.yaml docs/envoy-rust/phases/09-http-filter-local-rate-limit/PROGRESS.md
git commit -m "phase 09: task 6 — D8.2 parse_bootstrap fuzz corpus seed hcm_local_rate_limit_filter.yaml"
```

---

## Task 7: D8.3 in-process backstop

**Goal.** Land the in-process backstop at `crates/envoy-bin/tests/http_filter_local_rate_limit.rs`
mirroring the 07.2 `http_filter_header_mutation.rs` shape. Single test boots `envoy-bin`
with a synthesized bootstrap (`max_tokens: 2, tokens_per_fill: 0, fill_interval: 60s`),
issues 4 sequential `GET /` requests against the bound listener, asserts the status
sequence `[200, 200, 429, 429]` + the **direct** `x-envoy-ratelimited: true` per-header
assertion on the two 429 responses.

**Files:**
- Create: `crates/envoy-bin/tests/http_filter_local_rate_limit.rs`

### Steps

- [ ] **Step 1: Author the backstop test.**

Mirror the 07.2 `crates/envoy-bin/tests/http_filter_header_mutation.rs` shape
(reserve_port helper, wait_ready helper, tempfile YAML, subprocess spawn, raw HTTP/1.1
I/O). Adapt for the LocalRateLimit assertion shape:

```rust
//! Phase 09 in-process backstop for `envoy.filters.http.local_ratelimit`.
//!
//! Boots `envoy-bin` with a synthesized bootstrap whose HCM contains
//! `http_filters: [local_ratelimit, router]` with `token_bucket { max_tokens:
//! 2, tokens_per_fill: 0, fill_interval: 60s }`. Drives 4 sequential
//! `GET /` requests against the bound listener. Asserts status sequence
//! `[200, 200, 429, 429]` + `x-envoy-ratelimited: true` direct per-header
//! presence on the two 429 responses. No Docker dependency; complementary
//! to the Docker-gated differential fixture at
//! `tests/differential/tests/http_filter_local_rate_limit.rs`.

use std::io::Write;
use std::net::{Ipv4Addr, SocketAddr, TcpListener as StdListener};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::sleep;

fn reserve_port() -> u16 {
    let listener = StdListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind ephemeral");
    let port = listener.local_addr().expect("local_addr").port();
    drop(listener);
    port
}

async fn wait_ready(addr: SocketAddr, budget: Duration) -> Result<(), String> {
    let deadline = Instant::now() + budget;
    let mut backoff = Duration::from_millis(50);
    while Instant::now() < deadline {
        if TcpStream::connect(addr).await.is_ok() {
            return Ok(());
        }
        sleep(backoff).await;
        backoff = (backoff * 2).min(Duration::from_millis(500));
    }
    Err(format!("listener at {addr} did not become ready within {budget:?}"))
}

async fn send_request_and_collect(addr: SocketAddr) -> (u16, Vec<(String, String)>, Vec<u8>) {
    let mut stream = tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(addr))
        .await
        .expect("connect timeout")
        .expect("connect ok");
    let req = b"GET / HTTP/1.1\r\nHost: envoy-rust.test\r\nConnection: close\r\n\r\n";
    stream.write_all(req).await.expect("write request");
    let mut buf = Vec::with_capacity(8192);
    tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut buf))
        .await
        .expect("read timeout")
        .expect("read ok");
    parse_response(&buf)
}

fn parse_response(buf: &[u8]) -> (u16, Vec<(String, String)>, Vec<u8>) {
    let mut headers = [httparse::EMPTY_HEADER; 32];
    let mut resp = httparse::Response::new(&mut headers);
    let body_start = match resp.parse(buf).expect("parse response") {
        httparse::Status::Complete(n) => n,
        httparse::Status::Partial => panic!("partial response: {:?}", buf),
    };
    let status = resp.code.expect("status code");
    let header_list: Vec<(String, String)> = resp
        .headers
        .iter()
        .map(|h| {
            (
                h.name.to_lowercase(),
                String::from_utf8_lossy(h.value).into_owned(),
            )
        })
        .collect();
    let body = buf[body_start..].to_vec();
    (status, header_list, body)
}

#[tokio::test(flavor = "multi_thread")]
async fn local_rate_limit_enforces_429_after_token_exhaustion() {
    // Reserve an ephemeral port for the HCM listener.
    let listen_port = reserve_port();
    let admin_port = reserve_port();
    let listen_addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), listen_port);

    // Synthesize bootstrap.
    let bootstrap = format!(
        r#"admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: {admin_port}
node:
  cluster: phase-09-backstop
  id: phase-09-backstop
static_resources:
  listeners:
    - name: ingress_http
      address:
        socket_address:
          address: 127.0.0.1
          port_value: {listen_port}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: default
                  virtual_hosts:
                    - name: default
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response:
                            status: 200
                            body: {{ inline_string: "ok\n" }}
                http_filters:
                  - name: envoy.filters.http.local_ratelimit
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.local_ratelimit.v3.LocalRateLimit
                      stat_prefix: phase_09_backstop
                      token_bucket:
                        max_tokens: 2
                        tokens_per_fill: 0
                        fill_interval: 60s
                      status: {{ code: 429 }}
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
"#
    );

    // Write bootstrap to tempfile.
    let mut tempfile = tempfile::NamedTempFile::new().expect("tempfile");
    tempfile.write_all(bootstrap.as_bytes()).expect("write yaml");
    let yaml_path = tempfile.path().to_path_buf();

    // Spawn envoy-bin subprocess.
    let exe = env!("CARGO_BIN_EXE_envoy-bin");
    let mut child = Command::new(exe)
        .arg("-c")
        .arg(&yaml_path)
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn envoy-bin");

    // Wait for the listener to bind.
    let wait_result = wait_ready(listen_addr, Duration::from_secs(5)).await;
    if let Err(e) = wait_result {
        let _ = child.kill();
        panic!("envoy-bin did not become ready: {e}");
    }

    // Drive 4 sequential GET / requests.
    let mut statuses = Vec::new();
    let mut header_lists = Vec::new();
    for _ in 0..4 {
        let (status, headers, _body) = send_request_and_collect(listen_addr).await;
        statuses.push(status);
        header_lists.push(headers);
    }

    // Cleanup: kill the subprocess.
    let _ = child.kill();
    let _ = child.wait();

    // Assert the status sequence.
    assert_eq!(
        statuses,
        vec![200u16, 200, 429, 429],
        "expected [200, 200, 429, 429], got {statuses:?}"
    );

    // Assert the x-envoy-ratelimited: true header on probes 3 + 4 (the 429s).
    for (i, headers) in header_lists.iter().enumerate().skip(2) {
        let has_header = headers
            .iter()
            .any(|(k, v)| k == "x-envoy-ratelimited" && v == "true");
        assert!(
            has_header,
            "probe {i} (429 response) missing x-envoy-ratelimited: true; headers={headers:?}"
        );
    }
    // Assert probes 1 + 2 (the 200s) do NOT carry x-envoy-ratelimited.
    for (i, headers) in header_lists.iter().enumerate().take(2) {
        let has_header = headers.iter().any(|(k, _)| k == "x-envoy-ratelimited");
        assert!(
            !has_header,
            "probe {i} (200 response) unexpectedly carries x-envoy-ratelimited: {headers:?}"
        );
    }
}
```

- [ ] **Step 2: Verify dev-dependencies are available.**

The test uses `httparse`, `tempfile`, `tokio` — verify all are available as
dev-dependencies on `envoy-bin`. Check `crates/envoy-bin/Cargo.toml` `[dev-dependencies]`
block. If `httparse` or `tempfile` is missing, add it (`tempfile` is ADR-0018-permitted;
`httparse` is on the permitted-foundations list per D-3.2).

- [ ] **Step 3: Run the backstop test.**

```
cargo test -p envoy-bin --test http_filter_local_rate_limit
```

Expected: PASS. The test boots envoy-bin, exercises the filter, asserts the contract.

- [ ] **Step 4: Run the 5 stable-toolchain gates.**

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-targets
cargo test --workspace
cargo deny check
```

Expected: all 5 PASS.

- [ ] **Step 5: Append to PROGRESS.md + commit.**

```
git add crates/envoy-bin/tests/http_filter_local_rate_limit.rs docs/envoy-rust/phases/09-http-filter-local-rate-limit/PROGRESS.md
git commit -m "phase 09: task 7 — D8.3 in-process backstop http_filter_local_rate_limit.rs"
```

---

## Task 8: state-4 phase-done verification + STATE advance to state-5-next

**Goal.** Materialize the `BOOTSTRAP_PROMPT.md` §7.5 phase-done gate evidence — all 16
Docker-gated fixtures (`0001-tcp-echo` through `0016-http-filter-local-rate-limit`)
green simultaneously + h2spec ≥95% held + parse_bootstrap fuzz clean on the extended
16-seed corpus + 5 stable-toolchain gates green. Quote verbatim per-gate evidence (CI
run URL + HEAD SHA + completion timestamp). Advance STATE.md to state-5-next.

**Files:**
- Modify: `docs/envoy-rust/phases/09-http-filter-local-rate-limit/PROGRESS.md` (append `### Task 8 — state-4 phase-done verification` with per-gate quoted evidence).
- Modify: `docs/envoy-rust/STATE.md` (Active phase status → state 4-complete / state-5-next; Next expected skill → `superpowers:requesting-code-review`; Last commit; Last updated; append "Phase-09 state-4 verification" notes subsection).

### Steps

- [ ] **Step 1: Push the prior Task 7 commit to origin.**

```
git push origin main
```

Wait for CI completion (~10-15 min for the full Docker-gated suite + h2spec + fuzz). The
CI URL appears in the GitHub Actions page.

- [ ] **Step 2: Verify CI green.**

```
gh run list --limit 1
gh run view <ID>
```

Expected: both `build + test + lint` and `fuzz` jobs `success`; total runtime ~10-15
min. Note the run ID + HEAD SHA + completion timestamp for quoting in PROGRESS.

- [ ] **Step 3: Quote per-gate evidence into PROGRESS Task 8.**

Append to PROGRESS.md a `### Task 8 — state-4 phase-done verification` subsection with:

- CI run URL + run ID + HEAD SHA + completion timestamp.
- Per-gate quoted output for fmt / clippy / build / test / deny + per-fixture green
  attestation (16 fixtures × `succeeded` + h2spec pass rate + fuzz iteration count).
- The 16-fixture simultaneous-green narrative.

The per-gate evidence template mirrors the 08.2 Task 11 state-4 PROGRESS subsection
(see `docs/envoy-rust/phases/08.2-endpoint-triggered-drain/PROGRESS.md` for the closest
shape precedent).

- [ ] **Step 4: Advance STATE.md.**

Update STATE.md's "Active phase" status from `state 2-complete / state-3-next` (after
the state-2 PLAN-write commit) → `state 4-complete / state-5-next (verification complete; REVIEW.md pending)`.

Rewrite "Next expected skill" to `superpowers:requesting-code-review` scoped to the
phase-09 surface.

Rewrite "Last commit" + "Last updated".

Append a new `### Phase-09 state-4 verification` subsection in Notes recording the CI
run URL + HEAD SHA + per-gate green attestation + the 16-fixture simultaneous-green
narrative.

- [ ] **Step 5: Commit the state-4 → state-5 advance.**

```
git add docs/envoy-rust/phases/09-http-filter-local-rate-limit/PROGRESS.md docs/envoy-rust/STATE.md
git commit -m "phase 09: task 8 — state-4 phase-done verification + STATE advance to state-5-next"
git push origin main
```

Verify the docs-only follow-up CI run goes green.

---

## Self-review checklist

After writing the complete plan, the PLAN-writer checked the plan against SPEC.md with
fresh eyes:

**1. Spec coverage:**
- SPEC §3 D1 (envoy-config schema) → Task 1.
- SPEC §3 D2 (envoy-config validator + 4 ConfigError variants) → Task 1.
- SPEC §3 D3 (LocalRateLimitFilter runtime + token bucket) → Tasks 2 + 3.
- SPEC §3 D4 (HttpFilterInstance::LocalRateLimit variant + dispatch + registry threading) → Task 4.
- SPEC §3 D5 (07.2 REVIEW M1 closure — drop `_position` + `.enumerate()`) → Task 4.
- SPEC §3 D6 (4 stats counters wiring) → Task 3.
- SPEC §3 D7.1 (BEHAVIOR_CONTRACT 4 stat rows) → Task 3.
- SPEC §3 D7.2 (BEHAVIOR_CONTRACT 1 header row) → Task 5.
- SPEC §3 D8.1 (fixture 0016 + Docker-gated wrapper) → Task 5.
- SPEC §3 D8.2 (fuzz corpus seed) → Task 6.
- SPEC §3 D8.3 (in-process backstop) → Task 7.
- SPEC §6.1 split-gate evaluation → §3 above (no split; single-phase).
- SPEC §6.2 D5+D4 co-location → Task 4 (per lock-in #22).
- SPEC §6.3 concurrency torture test REQUIRED → Task 2 (per lock-in #9).
- SPEC §6.5 BEHAVIOR_CONTRACT cadence → Tasks 3 + 5 (per lock-ins #30 + #31).
- SPEC §6.6 fmt discipline → §5 above.
- SPEC §6.7 state-4 evidence discipline → Task 8.
- SPEC §7 ADR projection → §2 lock-in #34 (no ADRs project).
- SPEC §8 state-2 signposts → this PLAN file + sibling PROGRESS.md + ROADMAP flip +
  STATE.md advance.

All deliverables covered. No gaps.

**2. Placeholder scan:** No "TBD", "TODO", "implement later", "fill in details", "Add
appropriate error handling" lines. All test code blocks contain full content. All step
commands are exact.

**3. Type consistency:** Schema struct names (`LocalRateLimitConfig`, `TokenBucket`,
`HttpStatus`, `HeaderValueOption`, `Header`) are consistent across Tasks 1, 2, 3, 4.
ConfigError variants are consistent across Tasks 1 (definition) + 2/3 (test-side
references). Filter struct name (`LocalRateLimitFilter`), method names
(`build_from_config`, `decode_headers`, `encode_headers`), counter field names
(`enabled_counter`, `ok_counter`, `rate_limited_counter`, `enforced_counter`),
stat-name suffixes (`enabled`, `ok`, `rate_limited`, `enforced`) are consistent across
Tasks 2, 3, 4, 5, 7. The 07.2 REVIEW M1 closure narrative attributes the closure to
Task 4 consistently in §1 + §4 task summary + Task 4 step 4 + PROGRESS.md Task 1
preamble.

---

*End of PLAN. Phase 09 state-2 lifecycle complete on landing. The next session enters
state 3 — dispatches Task 1 per `superpowers:subagent-driven-development`.*
