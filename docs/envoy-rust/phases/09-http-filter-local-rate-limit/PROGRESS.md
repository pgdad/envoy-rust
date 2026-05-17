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

**Commit:** _(this commit; SHA emitted at `git commit` time)_
**Parent:** `b5c81d2` — `phase 09: task 2 — D3 hand-rolled token bucket primitive`.

**Work summary.** Landed the `LocalRateLimitFilter` runtime struct on top of
Task-2's `TokenBucketState` primitive, per PLAN Task 3 (SPEC §3 D3 + D6 + §6.5).
The filter is the lock-in #10 shape — 10 fields: `stat_prefix: String`,
`bucket: Arc<TokenBucketState>`, `max_tokens: u64`, `tokens_per_fill: u64`,
`fill_interval: Duration`, `response_headers_to_add: Vec<(String, String)>`, plus
4 `Arc<Counter>` handles (`enabled_counter`, `ok_counter`, `rate_limited_counter`,
`enforced_counter`). `#[derive(Debug, Clone)]` so the FilterPipeline build path
can clone the per-listener filter into the per-stream pipeline at Task 4. The
4 counters are registered at `build_from_config` time via `registry.register_counter`
(idempotent — repeat registrations under the same `stat_prefix` return the same
`Arc<Counter>`, so multiple per-stream Clone instances share the underlying
counter state; per lock-in #14).

`decode_headers(&mut self, _req: &mut FilterRequest) -> Decision` per lock-in #11:
increment `enabled_counter` unconditionally; call `bucket.try_acquire(max_tokens,
tokens_per_fill, fill_interval)`; on success → inc `ok_counter`, return
`Decision::Continue`; on failure → inc `rate_limited_counter` + inc
`enforced_counter`, return `Decision::StopAndSend(FilterResponse { status: 429,
reason: Some("Too Many Requests"), headers: [("x-envoy-ratelimited", "true"),
...response_headers_to_add], body: Bytes::new() })`. The 429 synth shape matches
lock-in #13 verbatim — the `x-envoy-ratelimited: true` header is prepended ahead
of any configured `response_headers_to_add` so the rate-limit signal is
unambiguous. `content-length: 0` is added by the H1/H2 codec writers at the
06.x writer-arm convention (the filter does NOT add it).

`encode_headers(&mut self, _resp: &mut FilterResponse) -> Decision { Decision::Continue }`
per lock-in #12 — decode-only filter; encode-side method exists for framework
symmetry. The `#[cfg(test)] pub(crate) fn stat_prefix(&self) -> &str` accessor
lets the test module read the field without making it crate-public (per PLAN
Step 3 lines ~1462-1466 adjustment).

The `FilterError::InvalidConfig { message: String }` variant lands on the
existing `pub enum FilterError` in `crates/envoy-filter/src/error.rs`. It is a
defense-in-depth landing site — the primary validation gate is at
`envoy_config::bootstrap::validate_local_rate_limit_config` (landed at Task 1).
The build_from_config path raises it on `fill_interval` parse failure (string
expected but non-string seen; or recognized-string format unparseable) and on
unexpected `StatsRegistry` registration failure.

Also lands the 4 BEHAVIOR_CONTRACT.md "Stat-name mapping" rows per lock-in #31
(SPEC §6.5 cadence), appended to a new `**09 entries (LocalRateLimit filter):**`
subsection below the existing `**08.2 entries (drain machinery):**` table.

**Files modified (5):**
- `crates/envoy-filter/src/local_rate_limit.rs` — extended with the
  `LocalRateLimitFilter` struct + `build_from_config` + `decode_headers` +
  `encode_headers` + `stat_prefix()` accessor (above the existing
  `TokenBucketState`); 6 new tests appended to the existing `#[cfg(test)] mod
  tests` block.
- `crates/envoy-filter/src/lib.rs` — added `pub use local_rate_limit::LocalRateLimitFilter;`
  in alphabetical position between `instance::HttpFilterInstance` and
  `pipeline::{Decision, FilterPipeline}` (per PLAN Step 4).
- `crates/envoy-filter/src/error.rs` — added `FilterError::InvalidConfig
  { message: String }` variant (per PLAN Step 3 lines ~1456-1458).
- `crates/envoy-filter/Cargo.toml` — added `serde_yaml = "0.9"` to
  `[dev-dependencies]` (the test helpers construct
  `envoy_config::TokenBucket::fill_interval = serde_yaml::Value::String(...)`).
- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — 4 new rows appended under a new
  `**09 entries (LocalRateLimit filter):**` subsection in the `## Stat-name
  mapping` section (per PLAN Step 6).
- `docs/envoy-rust/phases/09-http-filter-local-rate-limit/PROGRESS.md` — this
  subsection (per-task PROGRESS cadence).

Also: 2 envoy-config source files touched for the `parse_duration` visibility
promotion (deviation #2 below): `crates/envoy-config/src/bootstrap.rs` (1
visibility-keyword change: `pub(crate)` → `pub`) + `crates/envoy-config/src/lib.rs`
(1 re-export addition: `parse_duration` appended to the `pub use bootstrap::{...}`
block).

Also: `Cargo.lock` carries a 1-line `+ "serde_yaml",` addition recording the new
serde_yaml dependency edge on `envoy-filter`'s dependency-listing block (no new
top-level Cargo dep resolved — serde_yaml was already a workspace dep at v0.9.x
via envoy-config; this is the same metadata-only delta shape as Task 2
deviation #1).

**Tests landed (6 new; 735 → 741 in workspace test count).**
1. `build_from_config_succeeds_and_registers_counters` — asserts
   `LocalRateLimitFilter::build_from_config` returns Ok on the minimal ok_cfg
   AND that the 4 counters appear in the registry post-build (verified via
   idempotent re-registration returning the existing handle with value 0).
2. `decode_headers_allows_request_under_limit_and_increments_ok_counter` — a
   single `decode_headers` call returns `Decision::Continue`; counters land
   `enabled=1, ok=1, rate_limited=0, enforced=0`.
3. `decode_headers_rate_limits_after_max_tokens_and_increments_rate_limited_enforced`
   — drains the 2-token bucket, then 3rd request returns `Decision::StopAndSend`
   with status 429, reason "Too Many Requests", `x-envoy-ratelimited: true`
   header present (case-insensitive match), empty body; counters land
   `enabled=3, ok=2, rate_limited=1, enforced=1`.
4. `decode_headers_appends_configured_response_headers` — configures a single
   `response_headers_to_add` entry `x-rate-limit-policy: phase-09`; after a
   pre-drain (max_tokens=1) the rate-limited response carries BOTH the
   `x-envoy-ratelimited: true` header AND the configured
   `x-rate-limit-policy: phase-09` header.
5. `encode_headers_is_noop_continue` — `encode_headers` returns
   `Decision::Continue` and increments NO counters (the `enabled_counter`
   stays at 0).
6. `build_from_config_rejects_unparseable_fill_interval` — passes a
   `fill_interval: "forever"` (unparseable by `parse_duration`); asserts the
   error is the new `FilterError::InvalidConfig { .. }` variant.

**Per-task deviations from PLAN (4).**

1. **PLAN Step 1 test code at lines ~1246-1261 used a `HeaderValueOption`
   shape pre-dating Task-1's deviation #1.** PLAN wrote
   `HeaderValueOption { header: Header { key, value } }`, but Task 1 reused
   the existing 07.2 `HeaderValueOption { header: HeaderValue { key, value },
   append_action: AppendAction }`. The
   `decode_headers_appends_configured_response_headers` test was adjusted to
   construct the actual 07.2-landed shape: `HeaderValueOption { header:
   HeaderValue { key, value }, append_action: AppendAction::AppendIfExistsOrAdd }`.
   The runtime code (`build_from_config`'s `opt.header.key`/`opt.header.value`
   access pattern at PLAN lines ~1372-1376) needs NO adjustment — `HeaderValue`
   has identical `.key`/`.value` fields as the PLAN-projected `Header`.

2. **`envoy_config::parse_duration` visibility promotion: `pub(crate)` →
   `pub` + re-export from `envoy-config/src/lib.rs`.** Task 1 landed
   `parse_duration` as `pub(crate)` inside `bootstrap.rs`. Task 3's
   `LocalRateLimitFilter::build_from_config` invokes
   `envoy_config::parse_duration(fill_str)` from the cross-crate boundary
   (envoy-filter → envoy-config), so the visibility had to be widened. The
   alternative (duplicate the parse logic inline at the filter call site) was
   rejected as more invasive and harder to keep in sync with the validator's
   parse semantics. The promotion is purely additive (no callers regress); the
   re-export sits alphabetically at the end of the existing
   `pub use bootstrap::{...}` block in `lib.rs`.

3. **`#[allow(dead_code)]` annotations stay on TokenBucketState struct + impl;
   added on LocalRateLimitFilter struct + impl.** The Task-2 PLAN-write
   anticipated that `LocalRateLimitFilter`'s consumption of `TokenBucketState`
   at Task 3 would warrant the `#[allow(dead_code)]` removal. Empirically:
   `LocalRateLimitFilter` itself is `pub(crate)` for production-side reachability
   (the framework-dispatch arm at `HttpFilterInstance::LocalRateLimit` lands at
   Task 4), so clippy's `dead_code` lint still fires in the non-test build.
   Resolution mirrors Task 2's posture: both struct + impl carry
   `#[allow(dead_code)]` with comments pointing to Task 4 as the production
   wire-up site. The annotations come off naturally at Task 4 when the
   dispatch arm activates the production-side caller chain.

4. **`reason: Option<&'static str>` (NOT `Option<String>`) on `FilterResponse`.**
   The PLAN Step 3 code at lines ~1432 wrote
   `reason: Some("Too Many Requests".to_string())`, but
   `crates/envoy-filter/src/types.rs:45` defines
   `pub reason: Option<&'static str>`. The implementation + tests use
   `Some("Too Many Requests")` directly (no `.to_string()`); this is a
   PLAN-text typo correction, not a semantic deviation.

**LoC delta (production + tests; doc-comments excluded by manual inspection).**
Production: ~+120 LoC (the `LocalRateLimitFilter` struct + `build_from_config`
+ `decode_headers` + `encode_headers` + `stat_prefix` accessor in
`local_rate_limit.rs`; 6 lines for the `InvalidConfig` variant in `error.rs`;
1 line each for the `lib.rs` re-export, the `Cargo.toml` dev-dep, the
`bootstrap.rs` visibility change, the `lib.rs` parse_duration re-export).
Tests: ~+205 LoC (6 new tests + 2 helpers — `test_request()` + `ok_cfg()` — in
the `local_rate_limit::tests` block). Fixture/doc: ~+9 LoC (4 BEHAVIOR_CONTRACT
rows + 1 subsection header + surrounding blank lines). Total: ~+334 LoC.

Against PLAN §3 row 3's projection (~110 production / ~120 tests / ~15
fixture-doc / ~245 total):
- Production: +10 over projection — 1-for-1 with the planned struct shape.
- Tests: +85 over projection. Two contributors: (i) the rustfmt-canonical
  multi-line shape of `register_counter(&format!(...))` chains across 4
  counters with `.unwrap()` per call inflates the test body for the 2
  counter-assertion tests; (ii) the 4-counter assertion blocks were expanded
  per test (rather than abstracted into a helper) for readability.
- Fixture/doc: -6 under projection — the 4 rows are compact.
- Total: 334 vs 245 projection — +36% relative drift on tests, within
  PLAN §3's accept-drift posture (~+50% acceptable on a single task per the
  established 06.x / 07.x / 08.x discipline).

**5-stable-toolchain attestation.** All 5 gates PASS on stable toolchain.

#### Gate 1: `cargo fmt --all -- --check`
PASS (exit 0). One mid-task `cargo fmt --all` mutation applied (the initial
test-code append carried 07.2-style indentation that rustfmt reflowed); after
the mutation, `cargo fmt --all -- --check` exits 0.

#### Gate 2: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
PASS (exit 0). Initial run flagged 7 errors: 4 `dead_code` (LocalRateLimitFilter
struct + impl; TokenBucketState struct + impl) + 3 `doc_lazy_continuation`
(the multi-line bullet on the `build_from_config` doc-comment). Resolved per
deviation #3 (re-add `#[allow(dead_code)]` annotations with Task-4-wire-up
pointers) + rewriting the multi-line bullet as a continuous flowing sentence
("and register the 4 stat counters ... primary gate" with no awkward bullet
indentation). Re-run clean.

#### Gate 3: `cargo build --workspace --all-targets`
PASS (exit 0). All 15 workspace crates compile; no warnings.

#### Gate 4: `cargo test --workspace`
PASS (exit 0). Test result counts: **741 passed; 0 failed; 2 ignored** across
the workspace — +6 vs Task-2 predecessor (735 → 741), exactly matching the 6
new tests in the `local_rate_limit::tests` module under Task 3.

#### Gate 5: `cargo deny check`
PASS (exit 0). `advisories ok, bans ok, licenses ok, sources ok`. 3 cosmetic
`license-not-encountered` warnings unchanged from Task 2 (MPL-2.0,
Unicode-DFS-2016, Zlib — allowed-but-not-used at this resolution graph).

**Carryforward dispositions unchanged.** The 07.2 REVIEW M1 close site (Task 4)
is not engaged at Task 3.

**STATE.md / ROADMAP.md / DECISIONS.md / ENVOY_TARGET.md / rust-toolchain.toml
diffs at this commit:** None (per the per-task PROGRESS cadence rule; state-2
commit context above for the cadence). BEHAVIOR_CONTRACT.md DOES change at
this commit (4 new stat-name mapping rows per SPEC §6.5 + PLAN lock-in #31).

### Task 4 — D4 HttpFilterInstance::LocalRateLimit variant + D5 07.2 REVIEW M1 closure (severed `_position` plumbing)

**Commit:** _(this commit; SHA emitted at `git commit` time)_
**Parent:** `70bad43` — `phase 09: task 3 — D3 LocalRateLimitFilter runtime + D6 stats wiring + D7.1 4 stat-mapping rows`.

**Work summary.** Plugged the Task-3 `LocalRateLimitFilter` into the framework
dispatch per PLAN Task 4 (SPEC §3 D4 + D5). Three coordinated edits:

1. **D4: `HttpFilterInstance::LocalRateLimit(LocalRateLimitFilter)` variant
   landed.** Placed between `HeaderMutation` and the two
   `#[cfg(feature = "test-util")]` variants per PLAN-write SPEC correction #5 +
   lock-in. The `decode_headers` + `encode_headers` arms call the Task-3
   runtime methods straight-through; the `build` arm calls
   `LocalRateLimitFilter::build_from_config(cfg, registry)` (Task-3's two-arg
   shape per PLAN-write SPEC correction #6). The Task-1 bridge arm that
   returned `FilterError::UnsupportedFilterType { position, name }` is **replaced**
   (not augmented) — the bridge's comment block (Tasks 1-3 interim window
   marker) is gone too.

2. **D5: 07.2 REVIEW M1 lands closed at this commit.** The severed `_position`
   plumbing is closed at the named site per SPEC §3 D5 + PLAN lock-in #22. The
   closure deletes `_position: usize` from `HttpFilterInstance::build` and
   `.enumerate()` from `FilterPipeline::build_from_config`'s loop. Both
   signatures now take `&Arc<StatsRegistry>` as their last argument instead
   (registry threading is the load-bearing new wiring; position threading was
   YAGNI plumbing inherited from the 07.1 builder's interim shape that 07.2
   carried forward unconsumed). The hardcoded `position: 0` in
   `crates/envoy-filter/src/header_mutation.rs::map_entry` is **PRESERVED AS-IS**
   per SPEC §3 D5 rationale + PLAN lock-in #23 (minimum-touch the 07.2
   surface; the hardcode is unreachable in normal operation because the
   envoy-config validator rejects the corresponding failure case at parse
   time). The chain **07.2 → 09 ends at this commit.**

3. **HCM threading: H1 HCMConfig constructor extended in one line.** The
   `crates/envoy-http1/src/hcm.rs:185` call to
   `FilterPipeline::build_from_config(&cfg.http_filters)` becomes
   `FilterPipeline::build_from_config(&cfg.http_filters, &registry)`.
   `registry: Arc<envoy_stats::StatsRegistry>` is already a positional
   parameter on `HCMConfig::from_config` (the 06.1 stats-registration plumbing
   landed this; phase 09 reuses the existing parameter). The H2 path reuses
   the same `HCMConfig` type alias via `envoy-http2`'s re-export and so the
   H2 dispatch path needs zero additional edits per PLAN-write SPEC
   correction #2 + lock-in #24. Two test-helper sites in `hcm.rs`
   (`test_router_only_pipeline` + `header_mutation_pipeline`) and two
   test-helper sites in `header_mutation.rs` (the
   `round_trip_via_filter_pipeline_decode` + `iteration_order_on_encode_via_filter_pipeline`
   call sites + the `http_filter_instance_build_on_header_mutation_produces_header_mutation_variant`
   instance-build site) also pick up the new arg — mechanical cascade with no
   semantic change. `envoy-http2` only uses
   `FilterPipeline::test_from_instances` (no build_from_config call sites);
   no edits needed there.

**Files modified (5):**
- `crates/envoy-filter/src/instance.rs` — added `use std::sync::Arc;` + `use envoy_stats::StatsRegistry;` + `use crate::local_rate_limit::LocalRateLimitFilter;` imports; added `LocalRateLimit(LocalRateLimitFilter)` variant; widened `build` signature (drop `_position: usize`, add `registry: &Arc<StatsRegistry>`); replaced bridge arm with dispatch arm; added `LocalRateLimit` arms to `decode_headers` + `encode_headers`; updated module doc-comment to name LocalRateLimit landing + 07.2 REVIEW M1 closure; added `build_local_rate_limit_succeeds` test + `test_registry()` helper; updated `build_router_succeeds` test to pass `&registry` instead of `0`.
- `crates/envoy-filter/src/pipeline.rs` — added `use std::sync::Arc;` + `use envoy_stats::StatsRegistry;` imports; widened `build_from_config` signature (drop `position`, add `registry`); replaced `.enumerate()` loop with plain `.iter()` loop; updated 4 existing tests (`build_from_config_rejects_empty_list`, `build_from_config_with_single_router_succeeds`, `decode_headers_on_single_router_returns_continue`, `encode_headers_on_single_router_returns_continue`) to pass `&test_registry()` as second arg; added `test_registry()` helper.
- `crates/envoy-filter/src/header_mutation.rs` — updated 3 in-test `HttpFilterInstance::build` / `FilterPipeline::build_from_config` call sites to thread `&registry` (the cross-crate signature cascade). `header_mutation.rs::map_entry`'s hardcoded `position: 0` left AS-IS per SPEC §3 D5 + lock-in #23.
- `crates/envoy-filter/src/local_rate_limit.rs` — removed `#[allow(dead_code)]` annotations from `LocalRateLimitFilter` struct + impl AND from `TokenBucketState` struct + impl (the Task 4 dispatch arm activates the production-side caller chain; clippy now passes without them). Per-field `#[allow(dead_code)]` retained on `LocalRateLimitFilter::stat_prefix` (the field is read only by the `#[cfg(test)]` accessor — see deviation #1).
- `crates/envoy-http1/src/hcm.rs` — extended the production `FilterPipeline::build_from_config(...)` call at line 185 with `&registry`; updated two test-helper sites (`test_router_only_pipeline`, `header_mutation_pipeline`) to register a fresh `StatsRegistry` and pass it through.
- `docs/envoy-rust/phases/09-http-filter-local-rate-limit/PROGRESS.md` — this
  subsection (per-task PROGRESS cadence).

**07.2 REVIEW M1 closure attribution.** Per the PROGRESS Task 1 preamble
carryforward table entry — *"07.2 REVIEW M1 ... **PROJECTED-CLOSE at Task 4
(D5).** Co-located with D4 per SPEC §6.2 lock-in #22. The PROGRESS subsection
at Task 4 commit will record the closure attribution. The chain 07.2 → 09
ends."* — **the 07.2 REVIEW M1 chain lands closed at this commit.** The
specific edits attributable to the closure: (a) `_position: usize` parameter
deleted from `HttpFilterInstance::build`; (b) `.enumerate()` deleted from
`FilterPipeline::build_from_config`'s for-loop; (c) the variable name
`position` deleted from the same loop. The hardcoded `position: 0` in
`crates/envoy-filter/src/header_mutation.rs::map_entry` is preserved as-is
because: (i) it is unreachable in normal operation (the envoy-config validator
at `validate_header_mutation_config` rejects the corresponding error case at
parse time); (ii) preserving it conforms to PLAN lock-in #23 (minimum-touch
the 07.2 surface); (iii) the surface engagement is zero pending a future
filter-family phase that exercises the `apply_mutations` error path. No new
carryforward entry created; the row simply moves from "PROJECTED-CLOSE at
Task 4" to **CLOSED at this commit**.

**Tests landed (1 new; 741 → 742 in workspace test count).**
1. `build_local_rate_limit_succeeds` — asserts that
   `HttpFilterInstance::build(&hf, &registry)` returns
   `Ok(HttpFilterInstance::LocalRateLimit(_))` for a valid LocalRateLimit
   `HttpFilter` config (`stat_prefix: "phase_09"`, `max_tokens: 3`,
   `tokens_per_fill: 0`, `fill_interval: "60s"`, `status: 429`, no response
   headers). Verifies the new variant + the widened signature wire correctly
   end-to-end. The existing `build_router_succeeds` test was updated to pass
   `&registry` (signature cascade); test count delta is +1 not +2 because
   the existing test gets re-signed rather than replaced.

**Per-task deviations from PLAN (1).**

1. **Per-field `#[allow(dead_code)]` retained on `LocalRateLimitFilter::stat_prefix`.**
   PLAN Step 6 instructed *"Remove `#[allow(dead_code)]` from the
   `LocalRateLimitFilter` struct"* unconditionally. Empirically, clippy on
   `--all-targets --all-features -- -D warnings` against the production
   (`!cfg(test)`) build flags `stat_prefix` as `field is never read` — the
   only reader is the `#[cfg(test)]` accessor `stat_prefix()`. The other 9
   fields (`bucket`, `max_tokens`, `tokens_per_fill`, `fill_interval`,
   `response_headers_to_add`, 4 `Arc<Counter>` handles) are all read by
   `decode_headers` on the production path; only `stat_prefix` is
   test-only-read. Resolution: keep the struct-level + impl-level
   `#[allow(dead_code)]` removed (per PLAN Step 6); add a per-field
   `#[allow(dead_code)]` to `stat_prefix` only, with an inline comment
   pointing to the `#[cfg(test)]` accessor + the diagnostic-parity rationale
   for retaining the field. This is a narrower allow than the original
   blanket annotation and surfaces the future "production-side stat_prefix
   reader" as a clean per-field remove site whenever it lands. The other
   `#[allow(dead_code)]` annotations identified in Task 4 instructions (the
   `LocalRateLimitFilter` impl block + the `TokenBucketState` struct + the
   `TokenBucketState` impl block) ARE all removed per the original plan.

**LoC delta (production + tests; doc-comments excluded by manual inspection).**
Production: ~+25 LoC (`LocalRateLimit` variant + import lines + `decode_headers` arm + `encode_headers` arm + `build` arm in `instance.rs`; widened signature + `Arc`/`StatsRegistry` imports + 1-line loop change in `pipeline.rs`; 1-line registry threading in `hcm.rs`; 1-line registry threading in 2 hcm.rs test helpers; 1-line registry threading in 3 header_mutation.rs test helpers; net -8 LoC from removing the bridge arm + comment block in instance.rs; net -4 LoC from removing the 4 `#[allow(dead_code)]` annotations + comments; +5 LoC from the new per-field allow on `stat_prefix`).
Tests: ~+25 LoC (1 new test in instance.rs + test_registry helper; 4 updated test signatures in pipeline.rs + test_registry helper; 3 updated test-helper sites in header_mutation.rs; 2 updated test-helper sites in hcm.rs). Fixture/doc: 0. Total: ~+50 LoC.

Against PLAN §3 row 4's projection (~50 production / ~10 tests / 0
fixture-doc / ~60 total): production at projection; tests at +15 over
projection (the cross-crate cascade touched more sites than the lock-in #27
focus on `build_router_succeeds` alone projected). Total ~50 vs projection
60 — under projection, comfortable margin.

**5-stable-toolchain attestation.** All 5 gates PASS on stable toolchain.

#### Gate 1: `cargo fmt --all -- --check`
PASS (exit 0). One mid-task `cargo fmt --all` mutation applied (the
extended `build_from_config(&filters, &test_registry())` line in pipeline.rs
exceeded line-width and rustfmt reflowed it); after the mutation,
`cargo fmt --all -- --check` exits 0.

#### Gate 2: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
PASS (exit 0). Initial run flagged 1 error: `field stat_prefix is never read`
on `LocalRateLimitFilter` (the `#[allow(dead_code)]` removal exposed it —
see deviation #1). Resolved per deviation #1 (per-field
`#[allow(dead_code)]` with inline comment). Re-run clean.

#### Gate 3: `cargo build --workspace --all-targets`
PASS (exit 0). All 15 workspace crates compile; no warnings.

#### Gate 4: `cargo test --workspace`
PASS (exit 0). Test result counts: **742 passed; 0 failed; 2 ignored**
across the workspace — +1 vs Task-3 predecessor (741 → 742), exactly
matching the 1 new `build_local_rate_limit_succeeds` test in
`instance::tests`. The 4 updated tests in `pipeline::tests` + the
`build_router_succeeds` test in `instance::tests` are re-signs (same test
identity, same assertions; the registry arg is mechanical) so they don't
register a delta.

#### Gate 5: `cargo deny check`
PASS (exit 0). `advisories ok, bans ok, licenses ok, sources ok`. 3
cosmetic `license-not-encountered` warnings unchanged from Task 3 (MPL-2.0,
Unicode-DFS-2016, Zlib — allowed-but-not-used at this resolution graph).

**Carryforward dispositions update.** The 07.2 REVIEW M1 row moves from
**PROJECTED-CLOSE at Task 4 (D5)** to **CLOSED at this commit.** Chain
07.2 → 09 ends. All other carryforward rows unchanged. No new
carryforward entries.

**STATE.md / ROADMAP.md / BEHAVIOR_CONTRACT.md / DECISIONS.md / ENVOY_TARGET.md /
rust-toolchain.toml diffs at this commit:** None (per the per-task PROGRESS
cadence rule; state-2 commit context above for the cadence). The 1 new
BEHAVIOR_CONTRACT row (Header allow-list `x-envoy-ratelimited`) lands at
Task 5 per PLAN lock-in #31 + SPEC §6.5 cadence.

### Mid-execution corrective fixup per ADR-0033 (Commits A-D)

**Commit:** _(this commit — Commit A only; Commits B/C/D land in subsequent commits)_
**Parent:** `78128f4` — `phase 09: task 4 — D4 HttpFilterInstance::LocalRateLimit
variant + D5 07.2 REVIEW M1 closure`.

**Discovery.** Phase 09 Task 5 dispatch (the Docker-gated fixture 0016
authoring step) surfaced three empirical discrepancies between the phase-09
SPEC §2.2 + Task 3 lock-in #13 (both authored at state 1 / state 2 without
Docker-level empirical verification of upstream Envoy v1.33's actual
`envoy.filters.http.local_ratelimit` wire-level behavior) and the empirically
observed reality:

1. **`x-envoy-ratelimited` header is NOT emitted by upstream Envoy v1.33's
   `envoy.filters.http.local_ratelimit` filter.** Empirical Docker run yielded
   429 responses with header set `{content-length, content-type, date,
   server}` — the `x-envoy-ratelimited` header is absent. The SPEC §2.2 claim
   that upstream auto-injects this header is factually incorrect.
2. **Upstream's local_ratelimit 429 response body is the source-hardcoded
   string `"local_rate_limited"` (18 bytes), not empty.** Task 3 lock-in #13's
   `body: Bytes::new()` is incorrect for bilateral parity.
3. **envoy-rust's H1 HCM filter-StopAndSend writer-path skips standard-header
   decoration.** The synth-from-build path at `crates/envoy-http1/src/hcm.rs:866-887`
   (`synth_status`) emits 5 standard headers (server/date/content-length/
   content-type/connection); the filter-synth writer-arm sites at lines
   371-379 (decode-side `SynthFromDecode`) and 577 (encode-side
   `SynthFromEncode`) take `filter_resp.headers` verbatim with no
   standard-header augmentation. Phase 07.2's HeaderMutation filter never
   short-circuited via `StopAndSend`, so the gap went unobserved; phase 09's
   LocalRateLimit is the first filter to surface it.

**Disposition: ADR-0033 + 4 corrective commits (option (iii) per ADR-0033).**

- **Commit A** (this commit, docs-only): land ADR-0033 in `docs/envoy-rust/DECISIONS.md`;
  revise SPEC §2.2 in `docs/envoy-rust/phases/09-http-filter-local-rate-limit/SPEC.md`
  lines 55-63; append this preamble subsection. `phase 09: ADR-0033 + SPEC §2.2
  revision per upstream Envoy v1.33 empirical observation [ADR-0033]`.

- **Commit B** (next): Task 3 runtime fixup. `crates/envoy-filter/src/local_rate_limit.rs`
  drops the `("x-envoy-ratelimited", "true")` injection from
  `LocalRateLimitFilter::decode_headers`; changes `body: Bytes::new()` →
  `body: Bytes::from_static(b"local_rate_limited")`; 3 affected unit tests
  update (drop x-envoy-ratelimited assertions; add body-content assertions).
  PROGRESS appends a `### Task 3 fixup — upstream Envoy v1.33 parity per
  ADR-0033` subsection. Operator-configurable `response_headers_to_add`
  plumbing is preserved.

- **Commit C** (after B): Task 4 H1 HCM fixup. `crates/envoy-http1/src/hcm.rs`
  adds `decorate_filter_synth_response(resp: &mut Response, close: bool)`
  helper called from both `RequestPath::SynthFromDecode` (line ~440) and
  `RequestPath::SynthFromEncode` (line ~577) writer-arm sites; helper adds 5
  standard headers (server, date, content-length, content-type, connection)
  if not already provided by the filter (case-insensitive name check
  matching the 06.1/08.1 D1 dedupe precedent); `content-length` is ALWAYS
  derived from `body.len()` (the filter's body is the source of truth). 1-2
  new unit tests cover the decoration. PROGRESS appends a `### Task 4 fixup
  — H1 HCM filter-synth header decoration per ADR-0033` subsection.

- **Commit D** (after C): Task 5 (re-attempt). Fixture
  `tests/fixtures/0016-http-filter-local-rate-limit/{envoy.yaml,envoy-rust.yaml,
  expectations.yaml,README.md}` + Docker-gated wrapper
  `tests/differential/tests/http_filter_local_rate_limit.rs` + populated
  `### Task 5` PROGRESS subsection. **No `BEHAVIOR_CONTRACT.md` change** at
  Commit D (the `x-envoy-ratelimited` Header allow-list row PLAN lock-in #30
  projected is voided per ADR-0033; the 4 Stat-name mapping rows already
  landed at Task 3 commit `70bad43`).

**PLAN lock-ins affected by ADR-0033:**

- **#13** (429 synth response shape with `x-envoy-ratelimited` + empty body):
  voided; replaced by ADR-0033's revised contract (no `x-envoy-ratelimited`;
  `Bytes::from_static(b"local_rate_limited")` body; 5 standard headers via
  H1 HCM `decorate_filter_synth_response` helper).
- **#30** (`x-envoy-ratelimited` BEHAVIOR_CONTRACT row at Task 5): voided
  (no row appended).
- **#33** (in-process backstop at Task 7 with direct `x-envoy-ratelimited`
  per-header presence assertion): revised — Task 7 dropdown drops the
  `x-envoy-ratelimited` per-header presence assertion in favor of body-content
  (`"local_rate_limited"`) + standard-header presence assertions.

PLAN lock-ins **NOT affected**: #1-#9 (token bucket primitive shape +
atomicity; landed cleanly at Task 2); #10/#11/#12/#14 (filter struct shape +
decode/encode semantics + counter registration; remain unchanged except for
#13's affected sub-fields); #15-#27 (envoy-config schema + validator +
dispatch + signature widening; landed cleanly at Tasks 1 + 4); #22/#23
(07.2 REVIEW M1 closure at Task 4 + header_mutation.rs map_entry preservation;
landed cleanly at Task 4); #28/#29 (fixture 0016 bootstrap + probe list
shapes; Commit D applies modulo the SPEC §2.2 revision); #31 (4 Stat-name
mapping rows; landed cleanly at Task 3); #34/#35/#36/#37/#38/#39 (ADR ledger,
unsafe posture, split-gate verdict, subagent-driven execution, PROGRESS
cadence, Cargo.lock cadence; unchanged).

**DECISIONS.md ledger advance at this commit:** `ADR-0032 → ADR-0033`. The
ADR title: "Phase-09 SPEC §2.2 revision per upstream Envoy v1.33 empirical
observation (drop x-envoy-ratelimited injection; align 429 body + H1 HCM
filter-synth header decoration)".

**Files modified at Commit A (3):**

- `docs/envoy-rust/DECISIONS.md` — appended ADR-0033 after ADR-0032 (~3300
  words; Date / Status / Context / Options considered (5) / Decision (4
  commits) / Rationale / Consequences / Provenance sections per the
  established ADR-NNNN shape precedent at ADR-0028 through ADR-0032).
- `docs/envoy-rust/phases/09-http-filter-local-rate-limit/SPEC.md` — §2.2
  rewritten (was: 1-row table; now: prose explaining empirical discovery +
  ADR-0033 disposition + envoy-rust upstream-parity disposition; no
  BEHAVIOR_CONTRACT row needed).
- `docs/envoy-rust/phases/09-http-filter-local-rate-limit/PROGRESS.md` —
  this preamble subsection appended between Task 4 and Task 5 placeholder.

**STATE.md / ROADMAP.md / BEHAVIOR_CONTRACT.md / ENVOY_TARGET.md /
rust-toolchain.toml diffs at this commit:** None. STATE.md advances at Task
8 commit per the original PLAN; ROADMAP.md row 09's `in-progress` status
unchanged; BEHAVIOR_CONTRACT.md unchanged at this commit (the
`x-envoy-ratelimited` row is voided; the 4 stat-mapping rows already landed
at Task 3); ENVOY_TARGET pin + toolchain pin unchanged.

**Gate results at Commit A (5-stable-toolchain):** N/A — docs-only commit;
the 5 stable-toolchain gates are skipped per the established docs-only
commit convention (parent-08.2 state-5 commit `1dcf7f4` precedent;
parent-07.2 state-5 commit `8b69b9d`-shape precedent).

**5-gate attestation at Commit A:**
- **Gate 1 (`cargo fmt --all -- --check`):** N/A — no Rust source diff.
- **Gate 2 (`cargo clippy --workspace --all-targets --all-features -- -D warnings`):**
  N/A — no Rust source diff.
- **Gate 3 (`cargo build --workspace --all-targets`):** N/A — no Rust source diff.
- **Gate 4 (`cargo test --workspace`):** N/A — no Rust source diff.
- **Gate 5 (`cargo deny check`):** N/A — no Cargo.toml diff; no advisory window
  shift; no license-set shift.

CI on push will exercise the standard 5-gate sequence + the parse_bootstrap
fuzz target's 30s budget; both expected green (docs-only).

### Task 5 — D8.1 fixture 0016 + Docker-gated wrapper

_(Pending state-3 dispatch — Commit D per ADR-0033. The "+ D7.2
x-envoy-ratelimited row" suffix from the original placeholder is dropped:
ADR-0033 voids PLAN lock-in #30 — no BEHAVIOR_CONTRACT row lands at Task 5.)_

### Task 6 — D8.2 parse_bootstrap fuzz corpus seed

_(Pending state-3 dispatch.)_

### Task 7 — D8.3 in-process backstop http_filter_local_rate_limit.rs

_(Pending state-3 dispatch.)_

### Task 8 — state-4 phase-done verification + STATE advance to state-5-next

_(Pending state-3 dispatch.)_

---

*End of PROGRESS skeleton. State-3 task commits append per-task narrative sections per
the 06.x / 07.x / 08.x cadence.*
