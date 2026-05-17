# Phase 09 (`09-http-filter-local-rate-limit`) — PROGRESS

> Per-task narrative log. Appended at every task commit per the 06.2 / 06.3 / 07.x /
> 08.x cadence. State-2 PLAN-write lands this skeleton + the Task 1 preamble; state-3
> dispatch appends `### Task N — <name>` subsections in execution order.

---

## State-2 commit context

This commit (the state-2 standalone PLAN-write commit) lands:

- **CREATE** `docs/envoy-rust/phases/09-http-filter-local-rate-limit/PLAN.md` (the
  state-2 PLAN.md per `BOOTSTRAP_PROMPT.md` §5 state 2; ~1180 lines; 8 tasks; full
  `- [ ]` checkbox steps per task per the project's mature TDD cadence).
- **CREATE** `docs/envoy-rust/phases/09-http-filter-local-rate-limit/PROGRESS.md`
  (this file).
- **MODIFY** `docs/envoy-rust/ROADMAP.md` — flip row `09` `status: planned` →
  `status: in-progress`. Earlier rows unchanged.
- **MODIFY** `docs/envoy-rust/STATE.md` — Active phase status; Next expected skill;
  Last commit; Last updated; new `Phase-09 state-2 PLAN-write` subsection in Notes.

**Predecessor commit:** `3025594` — `phase 09: state-1 brainstorm — http-filter-local-rate-limit SPEC.md (HTTP-filter-family first phase; 07.2 REVIEW M1 named close site)` (the phase-09 state-1 brainstorm commit; immediate prologue).

**SPEC commit:** `3025594` (same — state-1 + state-2 are adjacent commits in the phase's
lifecycle; the SPEC didn't change between state-1 and state-2).

**ROADMAP status before this commit:** row `09` `planned` (added at state-1).
**ROADMAP status after this commit:** row `09` `in-progress`.

**STATE.md "Active phase" status before:** `phase 09 lifecycle state 1-complete / state-2-next (SPEC.md landed; PLAN.md does not exist)`.
**STATE.md "Active phase" status after:** `phase 09 lifecycle state 2-complete / state-3-next (PLAN.md landed; first task commit pending)`.

**DECISIONS.md status before AND after:** **ADR-0032** (parent-08 state-2 split
decision). No ADR lands at this commit per PLAN lock-in #34.

**BEHAVIOR_CONTRACT.md status before AND after:** Unchanged. The 4 stat-name mapping
rows land at Task 3 commit; the 1 header allow-list row lands at Task 5 commit per PLAN
lock-ins #30 + #31 (SPEC §6.5 cadence).

**ENVOY_TARGET.md + rust-toolchain.toml:** Unchanged (D-3.7 / D-3.9).

---

## PLAN scope summary

- **8 tasks** per PLAN §4. Under SPEC §6.1's ~10-13 projection on the lower end.
  Subagent-driven execution at state 3 per PLAN lock-in #37 + `feedback_execution_style`.
- **~1100-1400 LoC projected** per PLAN §3 (production ~380, tests ~655, fixture/doc
  ~260). Under SPEC §6.1's ~1500-LoC gate.
- **Single-phase; no nest-split** per PLAN lock-in #36 + parent-08 SPEC §6.1
  alternative (vi) accept-drift discipline.

---

## Task 1 preamble

### PLAN-write SPEC corrections (7 — verified against HEAD `3025594`)

Each verified by reading the on-disk surface; corrections land in execution at the
named task. Per the 06.2 → 06.3 → 07.x → 08.x precedent (06.1 0 corrections / 06.2 4
corrections / 06.3 5 corrections / 07.1 6 corrections / 07.2 8 corrections / 08.1 6
corrections / 08.2 6 corrections), the 7 corrections recorded here track the mature
PLAN-write cadence:

1. **`ConfigError` enum lives in `crates/envoy-config/src/lib.rs`, NOT
   `crates/envoy-config/src/bootstrap.rs`** as SPEC §3 D2 implies. The validator
   function `validate_http_filters` IS in `bootstrap.rs` (lines 1597-1652). Existing
   HeaderMutation `ConfigError` variants (`EmptyHeaderMutationKey` /
   `InvalidHeaderMutationKey` / `UnsupportedHeaderMutationAppendAction`) land in
   `lib.rs` lines 266-294. **Action at Task 1:** the 4 new ConfigError variants land
   in `lib.rs`; the sub-validator + LocalRateLimit dispatch arm land in
   `bootstrap.rs`. Lock-in #16.

2. **The HCM filter-pipeline build site is the HCMConfig constructor, NOT the
   request-handling functions `serve_connection`/`handle_one_stream`** as SPEC §3 D4
   says. Reading HEAD: H1 builds at `crates/envoy-http1/src/hcm.rs:185` inside
   `Http1HCMConfig::from_config`; H2 reuses the pre-built `config.filter_pipeline`
   (cloned per-stream at `crates/envoy-http2/src/hcm.rs:148`). Both H1 and H2 use
   the same `Http1HCMConfig` (H2 re-exports), so there is exactly ONE call site to
   widen. The constructor already holds `registry: Arc<StatsRegistry>` in scope.
   **Action at Task 4:** one-line extension to the call at `envoy-http1/src/hcm.rs:185`.
   Lock-in #24.

3. **`Http1HeaderRule` is a unit-variant enum with only `SetEqualModuloAllowList`**
   (line 589 of `tests/differential/src/lib.rs`), NOT `Option<Vec<HeaderRule>>` as
   SPEC §3 D8.1 hedge text implies. `Http1Probe::expected_headers` is
   `Option<Http1HeaderRule>` (line 634). **No harness extension needed.** The
   differential fixture relies on `SetEqualModuloAllowList` (both proxies emit
   `x-envoy-ratelimited: true` on 429; set-equal passes). **The direct
   per-header `x-envoy-ratelimited: true` assertion lives at the in-process backstop
   (Task 7, D8.3), not the differential fixture (Task 5, D8.1).** Mirrors the 07.2
   fixture-0013 `x-filter-response-stamp: phase-07` pattern exactly. Lock-in #29 +
   #33.

4. **`StatsRegistry::register_counter` takes `&self`, NOT `&Arc<Self>`** per
   `crates/envoy-stats/src/registry.rs:31`. `&Arc<StatsRegistry>` works via
   `Deref<Target = StatsRegistry>`. PLAN threads `&Arc<StatsRegistry>` for shared
   ownership semantics through the pipeline build path. **No API change required;**
   only a typing clarification. Lock-in #14.

5. **`HttpFilterInstance` carries 2 `#[cfg(feature = "test-util")]` variants**
   (`TestStopAndSendOnDecode(FilterResponse)` + `TestStopAndSendOnEncode(FilterResponse)`)
   in addition to `Router` + `HeaderMutation` (lines 17-25 of `instance.rs`). These
   were landed at 07.1 / 07.2 to support cross-crate HCM integration tests. SPEC §3
   D4 + D5 don't reference them. **Action at Task 4:** the new `LocalRateLimit`
   variant goes between `HeaderMutation` and the `#[cfg(feature = "test-util")]`
   block; test-util variants preserved verbatim. The `build` signature change (drop
   `_position`; add `registry`) is orthogonal — test-util variants are constructed
   via separate `test_stop_and_send_on_decode`/`test_stop_and_send_on_encode`
   constructors, NOT via `build`, so no test-util-arm edit is needed.

6. **`HeaderMutationFilter::build_from_config` is single-arg** (line 48-62 of
   `header_mutation.rs`). The new `LocalRateLimitFilter::build_from_config` has a
   **two-arg** shape (the registry param is needed for counter registration). This is
   a deliberate new precedent for any future filter that needs stats — NOT a drift
   from SPEC, recorded here for the subagent's awareness. Lock-in #10 +
   #14.

7. **SPEC §3 D1 names `LocalRateLimitFilterConfig` and `HttpStatusCode`**; PLAN
   renames them to `LocalRateLimitConfig` and `HttpStatus` respectively for
   consistency with existing schema naming (`HeaderMutationConfig`, `RouterConfig`
   — no `*Filter*` infix). Lock-in #20.

### Architecture-decision lock-ins (39 — see PLAN.md §2)

Per `feedback_pick_recommendation` ("always pick the recommended option; do not
ask"), 39 lock-ins recorded in the PLAN's lock-in table (§2). Grouped by topic for
in-execution lookup:

- **#1-#2** — module placement + new path-dep (`envoy-stats` on `envoy-filter`).
- **#3-#9** — token bucket primitive shape (state shape; numeric type; lazy-fill
  formula; CAS atomicity discipline; Mutex hold scope; poisoning posture;
  concurrency torture test REQUIRED per SPEC §6.3).
- **#10-#13** — filter struct shape (fields; decode/encode method semantics; 429
  synth response shape).
- **#14** — counter registration discipline.
- **#15-#21** — envoy-config schema (`fill_interval` as `serde_yaml::Value`; 4 new
  ConfigError variants; validator dispatch; sub-validator shape;
  `HttpFilterTypedConfig::LocalRateLimit` variant; schema struct shapes; renames
  per #7 above; `default_status` helper).
- **#22-#23** — D5 (07.2 M1 closure) co-located with D4 at Task 4; hardcoded
  `position: 0` at `header_mutation.rs::map_entry` left AS-IS per SPEC §3 D5.
- **#24-#27** — pipeline integration (HCM build site threading; widened
  `build_from_config` signature; widened `HttpFilterInstance::build` signature;
  unit test update).
- **#28-#29** — fixture 0016 (bootstrap shape; probe list).
- **#30-#31** — BEHAVIOR_CONTRACT cadence (4 stat rows at Task 3 commit; 1 header
  row at Task 5 commit per SPEC §6.5).
- **#32-#33** — fuzz corpus seed; in-process backstop.
- **#34** — no ADR landing (ledger head stays ADR-0032; conditional ADR-0033 +
  ADR-0034 stay reserved).
- **#35** — `#![forbid(unsafe_code)]` posture (inherited from crate root).
- **#36** — split-gate verdict (single-phase; no split; accept up to ~+50% drift).
- **#37** — subagent-driven execution at state 3.
- **#38** — PROGRESS.md skeleton + Task 1 preamble land alongside PLAN.md
  (this commit).
- **#39** — Cargo.lock cadence (empty diff expected).

Full text + rationale per lock-in lives in PLAN.md §2. PROGRESS sub-sections at
state-3 reference lock-ins by `#NN` rather than re-explaining.

### PLAN-write deviations beyond the SPEC corrections (1)

1. **Lock-in #20: schema struct renames** (`LocalRateLimitFilterConfig` →
   `LocalRateLimitConfig`; `HttpStatusCode` → `HttpStatus`). SPEC §3 D1 named the
   types with the `*Filter*Config` / `HttpStatus*Code*` suffixes; PLAN renames per
   existing schema convention (`HeaderMutationConfig`, `RouterConfig`). Surface
   effect: zero — the types are envoy-config-internal; no project-wide rename.
   Recorded here for transparency.

### Carryforward dispositions

| ID | Severity | Item | Disposition at 09 |
|---|---|---|---|
| **07.2 REVIEW M1** | Minor | Severed `position` plumbing (`_position: usize` parameter on `HttpFilterInstance::build` + `.enumerate()` on `FilterPipeline::build_from_config`) | **PROJECTED-CLOSE at Task 4 (D5).** Co-located with D4 per SPEC §6.2 lock-in #22. The PROGRESS subsection at Task 4 commit will record the closure attribution. The chain 07.2 → 09 ends. |
| **07.2 REVIEW M2** | Minor | `apply_mutations` Overwrite O(n²) YAGNI | **Carry forward indefinitely.** Phase 09 does NOT touch `header_mutation.rs` per lock-in #23 + SPEC §3 D5 rationale. Activates only if a future filter-family phase amplifies the apply_mutations call rate. |
| **07.2 REVIEW M3** | Minor | fixture-0013 `expected_body` coupling | **Carry forward indefinitely.** Phase 09's fixture 0016 uses a different bootstrap shape (direct_response without backend echo). Not engaged. |
| **08.1 REVIEW M3** | Minor | Forward-looking `Arc<BTreeMap<...>>` on `command_line_options` | **Carry forward indefinitely.** Not engaged. |
| **08.2 REVIEW M1-M8** | Minor | Various code-quality / doc-polish items per 08.2 REVIEW §3-§7 | **Carry forward indefinitely.** None engaged by phase 09's surface (the filter does not touch DrainState, AdminEndpoint, Listener::serve, or other 08.2 surfaces). |
| **08.2 REVIEW T1-T3** | Minor | Test / audit-trail polish | **Carry forward indefinitely.** Not engaged. |
| **08.2 REVIEW D1-D5** | Doc | Fixture-0015 / BEHAVIOR_CONTRACT doc-staleness | **CLOSED at 08.2 state-6 close-out commit `304ce98`.** Chain ended before phase 09 began. Recorded here for completeness. |
| **06.3 REVIEW I2** | Important | Synthetic 5xx backend + 4-class `pre_requests` deferred | **Carry forward indefinitely.** Upstream-robustness family is the natural close site. Not engaged. |
| **06.2 REVIEW M1 / M2 / M4 / M5** | Minor | Various | **Carry forward indefinitely.** Not engaged. |
| **06.1 REVIEW M2 / M3 / M5 / M6** | Minor | Various | **Carry forward indefinitely.** Not engaged. |
| **05.3 REVIEW I2** | Important | Typed-error chain dissolution at H2 dispatch site | **Carry forward indefinitely.** Not engaged. |
| **05.2 REVIEW I1 / I2 / I3** | Important | Various | **Carry forward indefinitely.** Not engaged. |
| **04.1 REVIEW M5 / M9** | Minor | Cargo.lock cadence ratification ADR | **Carry forward unchanged.** Phase 09 introduces zero new top-level Cargo deps per lock-in #2 + #39. The cadence pick stays unforced. |
| **04.1 REVIEW M-claim / M1 / M2 / M4 / M7** | Minor | Various | **Carry forward indefinitely.** Not engaged. |
| **02.2 REVIEW M1** | Minor | `*EchoBackend::Drop` polling loop blocks on `std::thread::sleep` | **Carry forward unchanged.** Phase 09's fixture 0016 uses direct_response (no Echo backend); the chain continues unchanged. |
| **Phase-00 I3** | — | SIGKILL → SIGTERM graceful termination of subject subprocess (`nix` crate deferral) | **Carry forward unchanged.** Phase 09 drives the filter via deterministic HTTP request bursts; no signal-based subprocess termination. The `nix` crate stays off the permitted-foundations list. |

### State-3 entry routing

The next session reads STATE.md, sees `state 2-complete / state-3-next (PLAN.md
landed; first task commit pending)` + Next expected skill `superpowers:subagent-driven-development`,
and dispatches Task 1 per the PLAN.

---

## Tasks 1-8

_(Per-task `### Task N — <name>` subsections append at state-3 task commits per the
06.x / 07.x / 08.x cadence. State-2 commit lands this skeleton only.)_

### Task 1 — D1 envoy-config schema + D2 validator (co-located)

**Commit:** _(this commit; SHA emitted at `git commit` time)_
**Parent:** `b9da8d4` — `phase 09: state-2 standalone PLAN.md`.

**Work summary.** Landed the LocalRateLimit envoy-config schema + parse-time validator
per PLAN Task 1 (SPEC §3 D1 + D2). The schema adds 3 new schema structs
(`LocalRateLimitConfig`, `TokenBucket`, `HttpStatus`) + 1 new `HttpFilterTypedConfig`
enum variant (`LocalRateLimit(LocalRateLimitConfig)`) + 1 default-helper
(`default_status`) — note that `HeaderValueOption` + `HeaderValue` were NOT re-landed
(reused from 07.2; see deviation #1 below). The validator adds 4 new `ConfigError`
variants (`EmptyLocalRateLimitStatPrefix`, `TokenBucketMaxTokensMustBePositive`,
`InvalidTokenBucketFillInterval`, `UnsupportedLocalRateLimitStatusCode`) + 1 new
dispatch arm in `validate_http_filters` + 1 new sub-validator
(`validate_local_rate_limit_config`) + 1 new `parse_duration` helper.

**Files modified (4):**
- `crates/envoy-config/src/lib.rs` — 4 new `ConfigError` variants; 3 new `pub use`
  re-exports (`HttpStatus, LocalRateLimitConfig, TokenBucket` placed alphabetically
  within the existing `pub use bootstrap::{...}` block).
- `crates/envoy-config/src/bootstrap.rs` — new variant on `HttpFilterTypedConfig`; 3
  new schema struct definitions + 1 `default_status()` helper; 1 new match arm on
  `validate_http_filters`; 2 new helper functions (`validate_local_rate_limit_config`
  + `parse_duration`); `Clone` derive added to existing `HeaderValueOption` and
  `HeaderValue` (so `LocalRateLimitConfig` can derive `Clone` for downstream
  reuse — Tasks 3/7); 16 new unit tests in the new `local_rate_limit_tests`
  submodule under the existing `mod tests` block (line 7130 area).
- `crates/envoy-filter/src/instance.rs` — cross-crate bridge arm: the new
  `HttpFilterTypedConfig::LocalRateLimit` variant must be handled in the
  `HttpFilterInstance::build` match (otherwise non-exhaustive match breaks the
  workspace build). The interim arm returns `FilterError::UnsupportedFilterType`;
  Task 4 replaces it with the proper `HttpFilterInstance::LocalRateLimit` dispatch.
  Comment in the source explains the deferral.
- `docs/envoy-rust/phases/09-http-filter-local-rate-limit/PROGRESS.md` — this
  subsection (per-task PROGRESS cadence).

**Tests landed (16 new; 209 → 225 in envoy-config lib).**
1. `deserialize_local_rate_limit_minimal_succeeds`
2. `deserialize_local_rate_limit_with_status_succeeds`
3. `deserialize_local_rate_limit_with_response_headers_succeeds` (4 assertions
   including `append_action: AppendIfExistsOrAdd` — see deviation #1)
4. `deserialize_local_rate_limit_rejects_unknown_field`
5. `validate_accepts_local_rate_limit_followed_by_router`
6. `validate_rejects_empty_stat_prefix`
7. `validate_rejects_zero_max_tokens`
8. `validate_rejects_zero_fill_interval`
9. `validate_rejects_unparseable_fill_interval`
10. `validate_rejects_non_429_status_code`
11. `validate_rejects_local_rate_limit_with_wrong_name`
12. `parse_duration_accepts_seconds`
13. `parse_duration_accepts_milliseconds`
14. `parse_duration_accepts_microseconds`
15. `parse_duration_rejects_unknown_unit`
16. `parse_duration_rejects_empty`

**LoC delta (production + tests; doc-comments excluded).** Production: ~+135 LoC
(schema structs + variant + helper + 2 validator functions in `bootstrap.rs`; 4
ConfigError variants + 3 re-exports in `lib.rs`; bridge arm in `instance.rs`). Tests:
~+200 LoC (16 new tests in `local_rate_limit_tests` submodule). Total: ~+335 LoC.
Under PLAN §3's Task-1 projection (~120 production + ~150 tests = ~270); the
overshoot is the cross-crate bridge arm in `instance.rs` (~25 LoC) + the helper
plumbing for the AppendAction-bearing test assertion (~10 LoC).

**5-stable-toolchain attestation.** All 5 gates PASS on stable toolchain:
- `cargo fmt --all -- --check` — PASS
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — PASS
- `cargo build --workspace --all-targets` — PASS
- `cargo test --workspace` — PASS (729 passed, 0 failed, 2 ignored)
- `cargo deny check` — PASS (advisories ok, bans ok, licenses ok, sources ok)

**Per-task deviations from PLAN (2; the 8th + 9th discovered-at-task-time SPEC
corrections — extends the PLAN §1 list of 7).**

1. **Discovered-at-task-time PLAN-write SPEC correction (8th — extends the PLAN §1
   list of 7).** PLAN Step 4 proposed adding new `pub struct HeaderValueOption
   { pub header: Header }` and `pub struct Header { pub key: String, pub value: String }`
   definitions to `crates/envoy-config/src/bootstrap.rs`. These collide with the
   existing 07.2-landed `HeaderValueOption { header: HeaderValue, append_action:
   AppendAction }` (lines 491-498 of `bootstrap.rs` at HEAD `b9da8d4`) and `HeaderValue
   { key, value }` (lines 500-503) — both re-exported from `lib.rs:15`. The existing
   types are upstream-Envoy-canonical (`envoy.config.core.v3.HeaderValueOption`),
   and upstream Envoy v1.33's `envoy.extensions.filters.http.local_ratelimit.v3.
   LocalRateLimit.response_headers_to_add` is `repeated config.core.v3.HeaderValueOption`
   — the same proto. **Resolution: reuse the existing 07.2 types.**
   `LocalRateLimitConfig.response_headers_to_add: Vec<HeaderValueOption>` resolves
   to the existing type. The `deserialize_local_rate_limit_with_response_headers_succeeds`
   test YAML adds `append_action: APPEND_IF_EXISTS_OR_ADD` to the response-header
   entry (upstream-canonical), plus a 4th assertion verifying the parsed
   AppendAction value. lib.rs re-exports drop the (non-existent) `Header` and
   (already-re-exported) `HeaderValueOption`; keep `HttpStatus, LocalRateLimitConfig,
   TokenBucket` additions. `AppendAction` was already re-exported from `lib.rs:11`
   (verified at task time); no additional re-export needed. Sub-action: `Clone` was
   added to existing `HeaderValueOption` and `HeaderValue` so the new
   `LocalRateLimitConfig` (which embeds `Vec<HeaderValueOption>`) can derive `Clone`
   — purely additive change; HeaderMutation call sites don't clone these types. The
   unresolved follow-on impact: Task 5's fixture 0016 YAML must also include
   `append_action: APPEND_IF_EXISTS_OR_ADD` on any LocalRateLimit
   `response_headers_to_add` entries (the fixture-0016 sketch in PLAN §3 doesn't add
   `response_headers_to_add` anyway, so the surface engagement is zero). No semantic
   loss; the change is upstream-canonical.

2. **Discovered-at-task-time PLAN-write SPEC correction (9th — cross-crate ripple).**
   Adding the new `HttpFilterTypedConfig::LocalRateLimit(LocalRateLimitConfig)`
   variant in `crates/envoy-config/src/bootstrap.rs` triggers a non-exhaustive
   `match` error in `crates/envoy-filter/src/instance.rs:41` (the
   `HttpFilterInstance::build` match on `&hf.typed_config`). PLAN Task 1's "Files to
   stage" list (3 files) did not anticipate this cross-crate ripple. **Resolution:**
   stage 4 files instead of 3; add a bridge arm in `instance.rs::build` that returns
   `FilterError::UnsupportedFilterType` for the `LocalRateLimit` arm during the
   Tasks 1-3 interim window. Task 4 (which adds
   `HttpFilterInstance::LocalRateLimit(LocalRateLimitFilter)` per PLAN Step 4 +
   widens `build_from_config` per lock-in #25) replaces this interim arm with the
   proper dispatch (uses Task 3's `LocalRateLimitFilter::build_from_config(cfg,
   &registry)`). The bridge arm exists in code with a comment block explaining the
   deferral. No tests assert on `UnsupportedFilterType` for `LocalRateLimit` — the
   error is only reachable if a caller tries to build an `HttpFilterInstance` from a
   `LocalRateLimit` typed-config during the Tasks 1-3 window; the config-validator
   doesn't take this path, so the surface engagement is zero until Task 4. (At Task
   4 the bridge arm is gone, so no defensive test is wasted at Task 1.)

**Carryforward dispositions unchanged.** The 07.2 REVIEW M1 close site (Task 4) is
not engaged at Task 1.

**STATE.md / ROADMAP.md / BEHAVIOR_CONTRACT.md / DECISIONS.md / ENVOY_TARGET.md /
rust-toolchain.toml diffs at this commit:** None (per PLAN-write expectation; see
state-2 commit context above for the cadence rule).

### Task 2 — D3 hand-rolled token bucket primitive + concurrency torture test

**Commit:** _(this commit; SHA emitted at `git commit` time)_
**Parent:** `818a3c5` — `phase 09: task 1 — D1 envoy-config schema + D2 validator`.

**Work summary.** Landed the hand-rolled `TokenBucketState` token-bucket primitive
per PLAN Task 2 (SPEC §3 D3 + §5.2 + §6.3) as a new module
`crates/envoy-filter/src/local_rate_limit.rs`. The primitive is the lock-in #3-#9
shape: `AtomicU64` for the live token count + `std::sync::Mutex<Instant>` for the
last-fill timestamp; lazy-fill computed at `try_acquire` time (no background refill
task); single `compare_exchange` loop with `Ordering::AcqRel` on success +
`Ordering::Acquire` on failure (lock-in #6); the Mutex is held only briefly inside
the loop for the last-fill timestamp read + only on CAS success for the timestamp
update (lock-in #7); poisoning is fatal — `.expect("TokenBucketState
last_fill_instant Mutex poisoned")` at every lock site (lock-in #8). On CAS-success,
the new `last_fill_instant` carries forward by `intervals * fill_interval` (NOT
`Instant::now()`) so partial intervals are preserved (lock-in #5).

The runtime filter struct (`LocalRateLimitFilter` wrapping `Arc<TokenBucketState>`)
defers to Task 3; the `HttpFilterInstance::LocalRateLimit` variant + dispatch glue
defers to Task 4. At this commit the module is module-private (no `lib.rs`
re-export) per PLAN Step 1. The 4th-file cross-crate bridge arm landed by Task 1 in
`crates/envoy-filter/src/instance.rs` stays AS-IS at this commit — Task 4 replaces
it with the proper dispatch.

The REQUIRED concurrency torture test (`token_bucket_concurrent_acquire_does_not_double_count`)
runs under `#[tokio::test(flavor = "multi_thread", worker_threads = 8)]` and spawns
8 tasks × 10_000 acquires against a bucket sized `max_tokens = 1000` with
`tokens_per_fill = 0` (no refill window). The assertion is `observed_success_count
== min(N*M, max_tokens) = 1000` AND `state.tokens.load(Acquire) == 0` post-run.
This is the SPEC §6.3 + PLAN lock-in #9 + 08.2 Task 1 fixup TOCTOU-lesson
precedent — a naive read-then-decrement implementation would observably
double-count or lose tokens; the single-CAS-loop discipline preserves the invariant.
The test ran 3 additional times locally (1 + 3 verification runs) and passed
deterministically each time; wall-clock per run was ~10-30ms.

**Files modified (4):**
- `crates/envoy-filter/Cargo.toml` — `envoy-stats = { path = "../envoy-stats" }`
  added to `[dependencies]` (workspace-internal path-dep per PLAN lock-in #2; the
  Counter handles wired at Task 3 will need it); `tokio = { version = "1",
  default-features = false, features = ["rt-multi-thread", "macros", "sync"] }`
  added to `[dev-dependencies]` (the multi-threaded torture test runtime).
- `crates/envoy-filter/src/lib.rs` — added `pub mod local_rate_limit;` in
  alphabetical position between `instance` and `pipeline` (PLAN Step 1). No
  re-export at this commit (lock-in #1 — re-export deferred to Task 3).
- `crates/envoy-filter/src/local_rate_limit.rs` — NEW file; the entire token bucket
  primitive shape (`TokenBucketState` struct + `new` + `try_acquire`) plus the
  6-test `#[cfg(test)] mod tests` block.
- `docs/envoy-rust/phases/09-http-filter-local-rate-limit/PROGRESS.md` — this
  subsection (per-task PROGRESS cadence).

Also: `Cargo.lock` carries 2 added lines (envoy-stats + tokio entries in the
envoy-filter dependency-listing) — see deviation #1.

**Tests landed (6 new; 729 → 735 in workspace test count).**
1. `new_bucket_starts_at_capacity` — verifies the fresh bucket loads at
   `max_tokens` capacity.
2. `try_acquire_consumes_one_token_at_a_time` — drains a 3-capacity bucket via 3
   `try_acquire` calls; 4th returns false.
3. `try_acquire_returns_false_on_empty_bucket_with_no_refill` — 0-capacity bucket
   returns false on first call.
4. `try_acquire_drains_then_recovers_after_sleep` — verifies the lazy-fill formula
   refills after `intervals = elapsed / fill_interval > 0`.
5. `try_acquire_refill_caps_at_max_tokens` — verifies the `.min(max_tokens)` cap
   on the refill arithmetic (high `tokens_per_fill * intervals` does not overflow
   the bucket).
6. `token_bucket_concurrent_acquire_does_not_double_count` — REQUIRED per SPEC
   §6.3; 8-thread × 10_000-acquire torture test; asserts
   `min(N*M, max_tokens) = 1000` total successes AND `state.tokens.load(Acquire) == 0`
   post-run. Verifies the CAS atomicity discipline at lock-in #6.

**Per-task deviations from PLAN (1).**

1. **`Cargo.lock` carries a +2-line dependency-listing delta vs the lock-in #39
   "empty diff" projection.** Lock-in #39 read: *"Cargo.lock diff at the phase-09
   reviewed range is expected to be empty (envoy-stats is already a workspace
   member; the new `envoy-stats = { path = "../envoy-stats" }` entry on
   envoy-filter/Cargo.toml does NOT add to lockfile)."* Empirically, the per-crate
   dependency-listing block in Cargo.lock for `envoy-filter` IS updated to record
   the new `envoy-stats` and `tokio` dependency edges — these are metadata-only
   entries (no new package versions resolved; envoy-stats was already a workspace
   member at v0.0.0; tokio was already at v1.52.1 from the workspace cluster). The
   diff is 2 lines added to the existing `[[package]] name = "envoy-filter"` block:
   `+ "envoy-stats",` and `+ "tokio",`. No new top-level Cargo deps resolved; no
   ADR-grant engaged; the D-3.2 permitted-foundations posture is unchanged. This
   refines the lock-in #39 projection: path-dep additions DO produce metadata-only
   Cargo.lock diffs (the lockfile records the per-crate dependency edge), but
   these are not "new dep resolutions" in the cadence sense. Stage Cargo.lock with
   the commit per PLAN Step 6's "include in commit but flag as deviation"
   guidance.

**LoC delta (production + tests; doc-comments excluded by manual inspection of
`local_rate_limit.rs` totals).** Production: ~+128 LoC (the
`TokenBucketState` struct + `new` + `try_acquire` lazy-fill + single-CAS loop in
the new module file; 1 line in `lib.rs` for the `pub mod` declaration; 2 lines in
`Cargo.toml` for the path-dep + dev-dep). Tests: ~+93 LoC (the 6-test
`#[cfg(test)] mod tests` block). Fixture/doc: 0. Total: ~+222 LoC.

Against PLAN §3 row 2's projection (~70 production / ~150 tests / 0 fixture-doc /
~220 total):
- Production: +58 over projection (~70 → 128). The overshoot is the
  `#[allow(dead_code)]` annotations + the formatted `match` shape produced by
  `cargo fmt` on the CAS expression + the lazy-fill `match` shape vs the projected
  `if/else` shape (clippy `manual_checked_ops` lint forced the rewrite — see
  per-task deviation 1 below).
- Tests: -57 under projection (~150 → 93). The 6 tests are tighter than projected
  because the torture test is a single self-contained `#[tokio::test]` (no
  multi-fixture matrix).
- Total: 222 vs 220 projection — exact match within rounding.

Total LoC delta is within PLAN §3's accept-drift posture (~+50% acceptable on a
single task per the established 06.x / 07.x / 08.x discipline). Production +83%
relative drift sits at the upper edge of "accept" but the total is on-projection;
recording for transparency rather than as a concern.

**Additional discovered-at-task-time refinement.** Beyond the PLAN-write SPEC
corrections (the 7 in PROGRESS §"Task 1 preamble" + the 9th + 10th flagged at Task
1 commit), Task 2 surfaces one mechanical adjustment:

- **Clippy `manual_checked_ops` lint required rewriting the lazy-fill
  `interval_nanos == 0` guard.** The PLAN Step 3 code paragraph reads `if
  interval_nanos == 0 { ... } else { let intervals = (elapsed_nanos /
  interval_nanos) as u64; ... }`. Clippy on the stable toolchain flags this as
  `manual_checked_ops` and recommends `checked_div`. Resolution: rewrite as
  `match elapsed_nanos.checked_div(interval_nanos) { None | Some(0) => (current,
  last_fill), Some(intervals_u128) => { /* ... */ } }`. Semantics are preserved
  (None ↔ zero-divisor defensive arm; Some(0) ↔ zero-intervals-elapsed early
  return; Some(n) ↔ refill arithmetic). The `#[allow(dead_code)]` annotations on
  the struct + impl block are paired with the lock-in #1 deferred-re-export
  posture — the production-code callers land at Task 3, but the test module
  exercises the surface at this commit; the `#[allow(dead_code)]` annotations
  satisfy clippy's `--all-features --all-targets -- -D warnings` discipline. This
  follows the existing `crates/envoy-http1/src/codec.rs:51-61` precedent (3
  `#[allow(dead_code)] // wired up by Task 9's router-proxy arm`).

**5-stable-toolchain attestation.** All 5 gates PASS on stable toolchain.

#### Gate 1: `cargo fmt --all -- --check`
PASS (exit 0). One mid-task `cargo fmt --all` mutation applied (the initial PLAN
Step 3 module copy + the clippy follow-up both required rustfmt-canonical
re-formatting); after the mutation, `cargo fmt --all -- --check` exits 0.

#### Gate 2: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
PASS (exit 0). Initial run flagged 3 errors (struct-never-constructed,
methods-never-used, manual_checked_ops); resolved per the per-task refinement
above (`#[allow(dead_code)]` annotations + `checked_div` rewrite). Re-run clean.

#### Gate 3: `cargo build --workspace --all-targets`
PASS (exit 0). All 15 workspace crates compile; no warnings.

#### Gate 4: `cargo test --workspace`
PASS (exit 0). Test result counts: **735 passed; 0 failed; 2 ignored** across the
workspace — +6 vs Task-1 predecessor (729 → 735), exactly matching the 6 new
tests in the `local_rate_limit::tests` module. The torture test ran ~10-30ms
wall-clock per run; 4 verification runs all green.

#### Gate 5: `cargo deny check`
PASS (exit 0). `advisories ok, bans ok, licenses ok, sources ok`. 3 cosmetic
`license-not-encountered` warnings unchanged from Task 1 (MPL-2.0,
Unicode-DFS-2016, Zlib — allowed-but-not-used at this resolution graph).

**Carryforward dispositions unchanged.** The 07.2 REVIEW M1 close site (Task 4) is
not engaged at Task 2.

**STATE.md / ROADMAP.md / BEHAVIOR_CONTRACT.md / DECISIONS.md / ENVOY_TARGET.md /
rust-toolchain.toml diffs at this commit:** None (per the per-task PROGRESS
cadence rule; state-2 commit context above for the cadence).

### Task 3 — D3 LocalRateLimitFilter runtime + D6 stats wiring + D7.1 4 stat-mapping rows

_(Pending state-3 dispatch.)_

### Task 4 — D4 HttpFilterInstance::LocalRateLimit variant + D5 07.2 REVIEW M1 closure

_(Pending state-3 dispatch.)_

### Task 5 — D8.1 fixture 0016 + Docker-gated wrapper + D7.2 x-envoy-ratelimited row

_(Pending state-3 dispatch.)_

### Task 6 — D8.2 parse_bootstrap fuzz corpus seed

_(Pending state-3 dispatch.)_

### Task 7 — D8.3 in-process backstop http_filter_local_rate_limit.rs

_(Pending state-3 dispatch.)_

### Task 8 — state-4 phase-done verification + STATE advance to state-5-next

_(Pending state-3 dispatch.)_

---

*End of PROGRESS skeleton. State-3 task commits append per-task narrative sections per
the 06.x / 07.x / 08.x cadence.*
