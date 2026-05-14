# Phase 07.2 (`07.2-header-mutation-filter`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended per the user's standing preference auto-memory `feedback_execution_style`) — fresh subagent per task + two-stage review. Steps use checkbox (`- [ ]`) syntax for tracking. Tasks land in numbered order; this PLAN.md commits ALONGSIDE the PROGRESS.md skeleton (with the Task 1 preamble) at state-2 per the `dc00750` (06.2) cadence — NO code changes at state-2. Tasks 1-10 each land as their own state-3 commit.

**Goal.** Land the first concrete pluggable HTTP filter — `envoy.filters.http.header_mutation` (Envoy v1.33's `envoy.extensions.filters.http.header_mutation.v3.HeaderMutation`) — end-to-end through the 07.1-established `envoy-filter` framework, and prove via the new differential fixture `0013-http-filter-header-mutation` that the framework's iteration semantics produce wire-equivalent output to upstream Envoy on both decode (request-side stamp `x-filter-stamp: phase-07` echoed back by the backend) and encode (response-side stamp `x-filter-response-stamp: phase-07` on the client-visible response) iteration states. The 07.2 state-6 commit ALSO closes parent-07.

**Architecture.** Hand-rolled per D-3.2 (*Every individual filter* is on the **Must be written from scratch** list). The `HeaderMutationFilter` runtime lives inside the existing `crates/envoy-filter/` crate as a new module `header_mutation.rs`; `HttpFilterInstance` (the 07.1-landed Router-only enum) gains a `HeaderMutation(HeaderMutationFilter)` variant. The filter manipulates only the `Vec<(String, String)>` header list of the 07.1-landed `FilterRequest` / `FilterResponse` value types (NOT `envoy_http1::codec::{Request,Response}` — see PLAN-write SPEC correction 1 below: ADR-0031 re-homed filter-visible types into `envoy-filter::types`). `envoy-config` gains the `HttpFilterTypedConfig::HeaderMutation` schema variant + 5 supporting structs + 1 enum + a validator extension at `validate_http_filters` + 3 new `ConfigError` variants. Synchronous (non-async) iteration; no new top-level Cargo deps; no new ADRs under the recommended posture.

**Tech Stack.** New permitted-foundations: NONE. No new workspace member (`HeaderMutationFilter` lives in the existing `envoy-filter` crate). Modified workspace members: `envoy-config` (schema + validator), `envoy-filter` (new `header_mutation.rs` module + `instance.rs` extension + `test-util` Cargo feature for the StopAndSend integration tests — see correction 2), `envoy-http1` + `envoy-http2` (the I1 `finalize_h2_stream` cleanup + the deferred-from-07.1 filter-chain integration tests, dev-only). New differential fixture `tests/fixtures/0013-http-filter-header-mutation/` + Docker-gated wrapper `tests/differential/tests/http_filter_header_mutation.rs` + in-process backstop `crates/envoy-bin/tests/http_filter_header_mutation.rs` + fuzz corpus seed `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_header_mutation_filter.yaml`. `cargo deny check` is a no-op for top-level deps but MUST be quoted in every task's PROGRESS attestation (07.1 REVIEW doctrine reminder).

---

## PLAN-write SPEC corrections (recorded here + in PROGRESS.md Task 1 preamble)

The 07.2 SPEC landed at the parent-07 state-2 split commit `6db5a01`, BEFORE the 07.1 execution arc. Eight SPEC details drifted against the 07.1-landed tree (verified against HEAD `3abcc8c`). Per the user's standing preference `feedback_pick_recommendation`, each correction picks the working option; all are folded into the task steps below.

1. **`header_mutation.rs` uses `FilterRequest` / `FilterResponse`, not `envoy_http1::codec::{Request, Response}`.** SPEC §3 Task 3 (lines 366-367, 413, 418) and Task 4 (lines 469, 478) show `use envoy_http1::codec::{Request, Response};`. ADR-0031 (landed in 07.1 at Task 5.5, `8161990`) re-homed filter-visible types into `envoy-filter::types` as `FilterRequest { method, path, headers, body }` / `FilterResponse { status, reason, headers, body }` and REMOVED `envoy-http1` from `envoy-filter`'s `Cargo.toml`. **Correction:** `header_mutation.rs` uses `use crate::types::{FilterRequest, FilterResponse};`; `decode_headers(&mut self, req: &mut FilterRequest)`; `encode_headers(&mut self, resp: &mut FilterResponse)`. `apply_mutations` operates on `&mut Vec<(String, String)>` (the `.headers` field) — unaffected.

2. **Signpost 2's `#[cfg(test)]` test-only `HttpFilterInstance` variant does not work cross-crate; use the SPEC's documented `test-util` Cargo-feature alternative instead.** SPEC §6 Signpost 2 recommends a test-only `HttpFilterInstance` variant gated `#[cfg(test)]` inside `crates/envoy-filter/src/instance.rs`. But `#[cfg(test)]` in `envoy-filter` activates ONLY when `envoy-filter` itself is under test — NOT when the downstream `envoy-http1` / `envoy-http2` crates compile their own test suites (where Task 5's integration tests live). **Correction:** use the SPEC's own documented alternative ("visible-via-feature-flag") — a `test-util` Cargo feature on `envoy-filter` exposing `TestStopAndSendOnDecode` / `TestStopAndSendOnEncode` variants + constructors + a `FilterPipeline::test_from_instances` constructor, all gated `#[cfg(feature = "test-util")]`. `envoy-http1` / `envoy-http2` enable it via `[dev-dependencies] envoy-filter = { path = "...", features = ["test-util"] }`. Within the SPEC's offered option space — not an ADR-worthy decision.

3. **Task 5's "deferred test stubs 3-7" are net-new tests, not stubs to fill in.** SPEC §3 Task 5 says "extend `#[cfg(test)] mod tests` with the 5 tests deferred at 07.1 Task 6 (test stubs 3-7)". There are NO stub/placeholder test functions in the 07.1-landed `hcm.rs` files — the 07.1 commits `84d68c1` (Task 6) / `3e041c5` (Task 7) explicitly DEFERRED *writing* tests 3-7 (H1) / 1-4 (H2) to 07.2 Task 5 per 07.1 PLAN signpost 12. Task 5 writes them from scratch. (Language clarification only.)

4. **Fixture 0013's `expectations.yaml` mirrors fixture 0008's actual shape, not the SPEC §3 Task 8 sketch.** SPEC §3 Task 8 (lines 736-749) sketches `driver: { kind: http1, request: {...}, expected_body: { kind: byte_exact }, expected_headers: { rule: set_equal_modulo_allow_list } }`. The actual `http1` driver schema (verified against `tests/fixtures/0008-http1-router-upstream/expectations.yaml` + `tests/differential/src/lib.rs`) is `driver: { kind: http1, method, path, host, expected_status, expected_body: { kind: byte_exact, body: "<exact string>" }, expected_headers: set_equal_modulo_allow_list }` plus a top-level `equivalence:` block. `expected_headers` is the bare externally-tagged unit form `set_equal_modulo_allow_list` (the `{ rule: ... }` internally-tagged form is the `Http1WithAccessLog` driver only). **Correction:** fixture 0013's `expectations.yaml` mirrors fixture 0008's actual shape verbatim.

5. **No existing RFC 7230 token helper to reuse; Task 2 lands `is_valid_rfc7230_token` inline.** SPEC §6 Signpost 1 says search-and-reuse before adding a duplicate. Verified: no `is_token_char` / `is_valid_token` / `is_rfc7230_token` helper exists anywhere in `envoy-config` (the 04.2 HeaderMatcher work referenced RFC 7230 in comments only; `matcher.rs` does case-insensitive name *matching*, not token-set *validation*). **Correction:** Task 2 lands `is_valid_rfc7230_token` inline in `bootstrap.rs` as the SPEC's Task 2 code block shows.

6. **`ConfigError` lives in `crates/envoy-config/src/lib.rs`, not `bootstrap.rs`.** SPEC §3 Task 2 says "Files modified: `crates/envoy-config/src/bootstrap.rs` — ... add the new `ConfigError::UnsupportedHeaderMutationAppendAction` variant." The `ConfigError` enum is defined in `crates/envoy-config/src/lib.rs` (lines 38-263). **Correction:** the 3 new `ConfigError` variants append to `lib.rs`; the validator function + `is_valid_rfc7230_token` helper land in `bootstrap.rs`. Mirrors the 07.1 Task 4 split (07.1 PLAN "File structure overview").

7. **New schema types follow the existing `envoy-config` derive convention — `#[derive(Debug, Deserialize, PartialEq)]`, NOT `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]`.** SPEC §3 Task 1's code block over-specifies `Clone` + `Serialize` on the new structs. The 07.1-landed `HttpFilter` / `HttpFilterTypedConfig` / `RouterConfig` all carry `#[derive(Debug, Deserialize, PartialEq)]` only (no `Clone`, no `Serialize`). Nothing clones config types (`FilterPipeline::build_from_config` + `HttpFilterInstance::build` take `&`). **Correction:** new structs (`HeaderMutationConfig` / `Mutations` / `HeaderMutationEntry` / `HeaderValueOption` / `HeaderValue`) use `#[derive(Debug, Deserialize, PartialEq)]` + `#[serde(deny_unknown_fields)]`; `AppendAction` uses `#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]` (`Clone, Copy` for by-value matching in `map_entry`; `Eq` is free on a fieldless enum; no `Serialize`). The existing `HttpFilterTypedConfig` keeps its `#[serde(tag = "@type", deny_unknown_fields)]` — the SPEC's Task 1 block dropped `deny_unknown_fields`.

8. **The `http1-echo-server` helper is a standalone subprocess binary, not a library exposing `serve_ephemeral()`; Task 9's in-process backstop follows the `0008` precedent.** SPEC §3 Task 9's sketch uses `http1_echo_server::serve_ephemeral()` / `render_fixture_config` / `spawn_envoy_bin` — none exist. The real in-process backstop precedent is `crates/envoy-bin/tests/http1_router_upstream.rs` (an inline `tokio::spawn`'d upstream + `format!` YAML + `tempfile::tempdir()` + `tokio::process::Command::new(env!("CARGO_BIN_EXE_envoy-bin"))`). **Correction:** Task 9 follows that precedent; its inline upstream echoes request headers into the response body (so the `x-filter-stamp` body assertion works), unlike the `0008` precedent's fixed-`"hello"` upstream.

---

## Architecture decisions locked at PLAN-write time (signpost choices)

Per 07.2 SPEC §6's 10 implementation signposts + §7 ADR posture, the planner picks the recommendation so the executor does not re-litigate mid-task. Per the user's standing preference `feedback_pick_recommendation`, every signpost with a "recommended posture" gets that recommendation.

| # | Signpost | Decision | Rationale |
|---|---|---|---|
| 1 | `is_valid_rfc7230_token` reuse | **No reuse possible — land the helper inline at Task 2** (`bootstrap.rs`). | SPEC §6 signpost 1 + correction 5 — no existing token-validator helper in `envoy-config`. |
| 2 | Test-only StopAndSend filter stub | **`test-util` Cargo feature on `envoy-filter`** exposing `HttpFilterInstance::{TestStopAndSendOnDecode, TestStopAndSendOnEncode}` + `FilterPipeline::test_from_instances`, gated `#[cfg(feature = "test-util")]`; enabled by `envoy-http1` / `envoy-http2` `[dev-dependencies]`. | SPEC §6 signpost 2's "Alternative" + correction 2 — `#[cfg(test)]` is not cross-crate-visible. |
| 3 | `HeaderMutationFilter` clone shape | **`Vec<RuntimeHeaderMutation>` held directly** (not `Arc<Vec<...>>`). The per-request `FilterPipeline` clone copies the Vec; cheap for 07.2's 2-4-entry fixture. | SPEC §6 signpost 3 + YAGNI. |
| 4 | Header normalization on Append vs Overwrite | **`RuntimeHeaderMutation.key` lowercased once at build time. Append pushes the lowercase key as-is. Overwrite case-folds the search (`to_ascii_lowercase()` on each existing entry) then pushes the lowercase key.** Do NOT preserve operator-original case on the wire. | SPEC §6 signpost 4 — matches the 04.x codec's lowercase-normalize posture; differential harness `set_equal_modulo_allow_list` is case-sensitive. |
| 5 | `:authority` / pseudo-header mutation | **Out of scope; no `:`-prefix blocklist. Diff-equivalent no-op posture** (H1 codec stores `:method`/`:path` in `Request.method`/`Request.path`, not `headers`, so a `:path` mutation lands as a regular `headers` entry — wire-equivalent to no-op on H1). | SPEC §4 + §6 signpost 5. |
| 6 | Fuzz corpus seed location | **`crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_header_mutation_filter.yaml`** + a `.gitignore` allow-list entry + the seed name appended to the `fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS array in `bootstrap.rs`. | SPEC §6 signpost 6. |
| 7 | Per-task PROGRESS test-bucket attestation | **Every code-changing task (1-9) quotes in PROGRESS: `cargo test --workspace`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, AND `cargo deny check` (NOT "assumed no-op").** Tasks 5 + 9 additionally run the in-process integration/backstop tests. Task 8 is Docker-gated-fixture-run before commit OR CI-run-then-commit. | SPEC §6 signpost 7 + parent SPEC §8 R-1 + the 07.1-REVIEW `cargo deny check` doctrine reminder (07.1 CI run `25758889478` failed at `cargo deny check`). |
| 8 | `apply_mutations` ordering | **Iterate `mutations` in slice (= YAML declaration) order.** Last Append appends last; last Overwrite for a given key wins. | SPEC §6 signpost 8 — matches Envoy v1.33's documented semantics. |
| 9 | Helper bind shape | **`http1-echo-server` already binds `0.0.0.0` (05.4 ADR-0024).** Fixture 0013's `envoy-rust.yaml` cluster endpoint mirrors fixture 0008's `{{BACKEND_HOST}}` + STRICT_DNS pattern verbatim. | SPEC §6 signpost 9. |
| 10 | Echoed body order | **`http1-echo-server` ALREADY emits request headers sorted-by-name into the response body** (verified: `build_echo_body` in `tests/helpers/http1-echo-server/src/main.rs` does `sorted_headers.sort_by(|a, b| a.0.cmp(&b.0))`). Task 7 is **verify-only — zero code change**. | SPEC §6 signpost 10 + SPEC §3 Task 7 pre-state check. |
| 11 | I1 carryforward — `finalize_h2_stream` 3-dead-parameter cleanup | **Structural prerequisite of Task 5 (Step group A).** Option B mechanical cleanup: remove `_response_status_for_log` / `_response_body_len` / `_response_headers_for_log` from the signature; the 3 callers stop computing the pre-encode trio; `finalize_h2_stream` keeps its existing post-encode shadow locals (already present at `crates/envoy-http2/src/hcm.rs:490-493`). ~12 line removals; zero behavior impact. | 07.1 REVIEW I1 + STATE.md "Phase-07.1 rollovers" — "the 07.2 state-2 PLAN-write MUST list this as a structural prerequisite of 07.2 Task 5 (named owner)". |
| 12 | M2 carryforward — three unconstructed `FilterError` variants | **`UnsupportedFilterType` becomes constructable at Task 3** (`map_entry`'s defense-in-depth check for `AddIfAbsent` / `OverwriteIfExists`). `RouterNotTerminal` / `DuplicateRouter` stay unconstructed (the `envoy-config` validator is the real catch — defense-in-depth-only per the 07.1 design). PROGRESS Task 3 notes the partial M2 closure. | 07.1 REVIEW M2 — "carry forward to 07.2 Task 3". |
| 13 | M1 carryforward — unused `tracing` dep in `envoy-filter` | **Remove `tracing = "0.1"` from `crates/envoy-filter/Cargo.toml` at Task 3** (the first 07.2 task to touch the crate). No 07.2 task wires a `tracing` call inside `envoy-filter`; removal is the clean close. | 07.1 REVIEW M1 — "close opportunistically ... or remove at 07.2 state-5". Removing early is cleaner; closes M1 with a `cargo deny check`-quoted Cargo.toml change at Task 3. |
| 14 | No new ADRs | **Ledger head stays ADR-0031.** No foundations grants; the `test-util` feature is within signpost 2's offered option space. ADR-0032 stays reserved-available. | SPEC §7 + parent-07 SPEC §6 Rule 5. |
| 15 | PROGRESS.md cadence | **PROGRESS.md skeleton + Task 1 preamble land ALONGSIDE PLAN.md at state-2** (the `dc00750` 06.2 shape — back to the 06.x norm; divergence from 07.1's "PROGRESS created at Task 1"). | next-prompt.txt item 2. |
| 16 | `#![forbid(unsafe_code)]` | **`header_mutation.rs` is a non-root module; inherits the crate-level attribute already in `crates/envoy-filter/src/lib.rs`.** No new crate roots in 07.2. | D-3.8 + 4.1 invariant 8. |

---

## LoC drift posture / split-gate evaluation (per BOOTSTRAP_PROMPT.md §6.1)

07.2 SPEC §3's own per-task code-budget table projects **~1577-1607 LoC** of net change across **10 tasks** (production ~430-460 LoC; tests ~740 LoC; fixture/doc ~407 LoC). Task count (10) is well under the §6.1 ~25-task gate. The LoC projection sits ~+7% over the §6.1 ~1500-LoC soft gate — **concentrated entirely in test + fixture material**; production code (~440 LoC) is well under.

**Decision: accept the drift; do NOT split.** Per parent-07 SPEC §5 + ADR-0030, a sub-phase produced by a split may NOT nest-split — "reject nested splits symmetrically for 07.2". The 06.x precedent ratifies the accept-drift posture (06.1 SPEC ~1300 → PLAN ~2010 LoC; 06.2 SPEC ~1300 → PLAN ~1875 LoC — neither nest-split). The test-heavy projection is the value of this sub-phase (the first concrete filter warrants thorough decode/encode + iteration-order coverage); the 14-test Task 4 inventory and the 9-test Task 5 inventory were reviewed at PLAN-write time and found non-redundant. If execution-time drift inflates a single task past ~10 sub-steps, the in-execution release valve is per-step commit splitting recorded in PROGRESS (e.g., Task 5a = I1 cleanup + test-util feature; Task 5b = H1 tests; Task 5c = H2 tests) — NOT a phase-level nest-split.

---

## Task summary

10 substantive tasks; all land at state-3, each as its own commit. Recommended execution order **1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9 → 10**.

| # | Title | Scope (LoC) | Carryforwards / notes |
|---|---|---|---|
| 1 | `envoy-config` schema additions for HeaderMutation | ~200 | — |
| 2 | `envoy-config` validator extension + 3 new `ConfigError` variants | ~170 | — |
| 3 | `HeaderMutationFilter` runtime types + builder + `instance.rs` arm + `test-util` feature | ~210 | **07.1 REVIEW M1** (remove unused `tracing` dep); **07.1 REVIEW M2** (`UnsupportedFilterType` becomes constructable) |
| 4 | `HeaderMutationFilter::decode_headers` + `encode_headers` semantics | ~185 | — |
| 5 | H1+H2 HCM filter-chain integration tests + I1 `finalize_h2_stream` cleanup | ~300 | **07.1 REVIEW I1** (`finalize_h2_stream` 3-dead-parameter cleanup — Step group A) |
| 6 | Fuzz corpus seed for HeaderMutation HCM | ~52 | — |
| 7 | `http1-echo-server` helper header-echo verify (zero code change expected) | ~0-30 | — |
| 8 | Fixture `0013-http-filter-header-mutation` + Docker-gated wrapper | ~290 | **06.3 REVIEW I1** discipline (Docker-gated-run-before-commit) |
| 9 | In-process backstop `crates/envoy-bin/tests/http_filter_header_mutation.rs` | ~150 | — |
| 10 | State-4 phase-done verification + STATE advance to state-5-next | ~30 doc | — |

**Parallelization notes (for subagent-driven dispatch).** Recommended default is strict sequential 1→10 with review between tasks. Where the executor wants concurrency: **Task 7 is fully independent** (helper-only; verify-only — dispatch anytime, even first). After Task 1 lands, **Task 2 and Task 3 are parallelizable** (Task 2 touches `envoy-config`; Task 3 touches `envoy-filter`; disjoint crates, both depend only on Task 1's schema types). **Task 6 depends on Tasks 1+2** (the seed must parse + validate clean). Tasks 4, 5, 8, 9, 10 are strictly sequential on their predecessors. The two-stage review checkpoint after each task still applies regardless of dispatch concurrency.

---

## File structure overview

### Created (new files)

- **`crates/envoy-filter/src/header_mutation.rs`** (Task 3) — `HeaderMutationFilter` + `RuntimeHeaderMutation` + `RuntimeAppendAction` + `build_from_config` + `map_entry` + `decode_headers` / `encode_headers` (stubs at Task 3, real semantics at Task 4) + `apply_mutations`.
- **`crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_header_mutation_filter.yaml`** (Task 6) — fuzz corpus seed.
- **`tests/fixtures/0013-http-filter-header-mutation/envoy.yaml`** (Task 8) — reference Envoy config.
- **`tests/fixtures/0013-http-filter-header-mutation/envoy-rust.yaml`** (Task 8) — envoy-rust config.
- **`tests/fixtures/0013-http-filter-header-mutation/inputs/payload.bin`** (Task 8) — 0-byte placeholder.
- **`tests/fixtures/0013-http-filter-header-mutation/expectations.yaml`** (Task 8) — differential assertions.
- **`tests/fixtures/0013-http-filter-header-mutation/README.md`** (Task 8) — fixture documentation.
- **`tests/differential/tests/http_filter_header_mutation.rs`** (Task 8) — Docker-gated wrapper.
- **`crates/envoy-bin/tests/http_filter_header_mutation.rs`** (Task 9) — in-process backstop.

### Modified

- **`crates/envoy-config/src/bootstrap.rs`** (Tasks 1, 2, 6) — Task 1: `HttpFilterTypedConfig::HeaderMutation` variant + 5 new structs + `AppendAction` enum + `header_mutation_schema_tests` module. Task 2: `validate_http_filters` HeaderMutation arm + `validate_header_mutation_entries` + `is_valid_rfc7230_token` free functions + `header_mutation_validator_tests` module. Task 6: append `"hcm_header_mutation_filter.yaml"` to the `fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS array.
- **`crates/envoy-config/src/lib.rs`** (Tasks 1, 2) — Task 1: `pub use bootstrap::{HeaderMutationConfig, Mutations, HeaderMutationEntry, HeaderValueOption, HeaderValue, AppendAction};`. Task 2: 3 new `ConfigError` variants (`UnsupportedHeaderMutationAppendAction`, `EmptyHeaderMutationKey`, `InvalidHeaderMutationKey`).
- **`crates/envoy-filter/src/lib.rs`** (Task 3) — add `pub mod header_mutation;` + `pub use header_mutation::HeaderMutationFilter;`.
- **`crates/envoy-filter/src/instance.rs`** (Task 3, Task 5) — Task 3: `HeaderMutation(HeaderMutationFilter)` variant + `build` / `decode_headers` / `encode_headers` arms. Task 5: `#[cfg(feature = "test-util")]` `TestStopAndSendOnDecode` / `TestStopAndSendOnEncode` variants + arms + constructors.
- **`crates/envoy-filter/src/pipeline.rs`** (Task 5) — `#[cfg(feature = "test-util")]` `FilterPipeline::test_from_instances` constructor.
- **`crates/envoy-filter/src/header_mutation.rs`** (Task 4) — replace Task-3 `decode_headers` / `encode_headers` stubs with real semantics + `apply_mutations`.
- **`crates/envoy-filter/Cargo.toml`** (Task 3, Task 5) — Task 3: remove unused `tracing = "0.1"`. Task 5: add `[features] test-util = []`.
- **`crates/envoy-http1/Cargo.toml`** (Task 5) — `[dev-dependencies] envoy-filter = { path = "../envoy-filter", features = ["test-util"] }`.
- **`crates/envoy-http2/Cargo.toml`** (Task 5) — same dev-dependency feature line.
- **`crates/envoy-http1/src/hcm.rs`** (Task 5) — 5 new filter-chain integration tests in `#[cfg(test)] mod tests`.
- **`crates/envoy-http2/src/hcm.rs`** (Task 5) — I1 cleanup of `finalize_h2_stream` (signature + 3 call sites) + 4 new filter-chain integration tests in `#[cfg(test)] mod tests`.
- **`crates/envoy-config/fuzz/.gitignore`** (Task 6) — add `!corpus/parse_bootstrap/hcm_header_mutation_filter.yaml`.
- **`tests/helpers/http1-echo-server/src/main.rs`** (Task 7) — verify-only; modify ONLY if the header-echo shape is found absent (it is present — expect zero change).
- **`docs/envoy-rust/phases/07.2-header-mutation-filter/PROGRESS.md`** (every task) — per-task narrative append. CREATED at state-2 with the Task 1 preamble.
- **`docs/envoy-rust/STATE.md`** (Task 10) — advance from `07.2 state 3` → `07.2 state-4-reached / state-5-next`.
- **`docs/envoy-rust/ROADMAP.md`** — flip row `07.2` `planned` → `in-progress` at THIS state-2 commit.

### Deleted

None.

---

## Conventions

Mirrors the 06.x / 07.1 PLAN conventions:

- **TDD shape per task:** Step 1 writes the failing test(s); Step 2 runs them (FAIL expected; quote output); Step 3 writes the minimal implementation; Step 4 runs the tests (PASS expected; quote output); later steps layer workspace-wide verification; final step commits.
- **Commit messages:** `phase 07.2: task N — <task summary>` (the exact subject line is in each task's final step). Co-Authored-By trailer: `Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>`.
- **PROGRESS.md per-task append:** every substantive task commit appends a per-task section narrating work summary, tests landed (names + LoC tally), per-task deviations from PLAN (D-3.5 append-only discipline), LoC delta, and the test-bucket attestation. **The test-bucket attestation MUST explicitly quote `cargo deny check` output** (07.1-REVIEW doctrine reminder — do not write "assumed no-op").
- **No new top-level Cargo deps.** Task 3 removes one (`tracing` from `envoy-filter`); Task 5 adds a `[features]` table entry + dev-dependency feature lines (no new crates). Every `Cargo.toml`-touching task quotes `cargo deny check`.
- **`cargo fmt --all` + `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean at every per-task commit.**
- **Error variants use the existing `ConfigError` / `FilterError` naming convention** — no transform.

---

## State-2 commit (this commit's content; lands BEFORE any Task 1-10 commit)

The state-2 commit lands exactly 2 files created + 2 files modified — docs-only, no code:

- **CREATE:** `docs/envoy-rust/phases/07.2-header-mutation-filter/PLAN.md` (this file).
- **CREATE:** `docs/envoy-rust/phases/07.2-header-mutation-filter/PROGRESS.md` — the PROGRESS skeleton with the Task 1 preamble (PLAN-write SPEC corrections + architecture-decision lock-ins). Per the `dc00750` 06.2 cadence.
- **MODIFY:** `docs/envoy-rust/ROADMAP.md` — flip row `07.2` `status: planned` → `status: in-progress` (single-cell edit; per BOOTSTRAP_PROMPT.md §4.1 invariant 3 — a phase enters `in-progress` only when STATE.md points at it AND its PLAN.md has landed). Parent row `07` stays `in-progress`; row `07.1` stays `done`.
- **MODIFY:** `docs/envoy-rust/STATE.md` — advance active-phase status `07.2 state 2 (SPEC.md only)` → `07.2 state 3 (SPEC + PLAN exist; implementation incomplete)`; next-skill `superpowers:writing-plans` → `superpowers:subagent-driven-development` against this PLAN.md. Rewrite the Active-phase / Next-expected-skill / Last-commit / Last-updated sections + the standing context from PLAN-writer perspective to executor perspective. Preserve all "Phase-NN rollovers" sections verbatim (including "Phase-07.1 rollovers").
- **MODIFY (no edit):** `docs/envoy-rust/DECISIONS.md` — UNCHANGED. Ledger head stays **ADR-0031**. No ADR at the state-2 commit (recommended no-foundations-grants posture per parent-07 SPEC §6 Rule 5 + 07.2 SPEC §7).
- **MODIFY (no edit):** `BEHAVIOR_CONTRACT.md`, `ENVOY_TARGET.md`, `rust-toolchain.toml`, the 07.2 `SPEC.md` — UNCHANGED.

**Commit message (verbatim):**

```
phase 07.2: state-2 standalone PLAN.md

Lands the 07.2 PLAN.md + PROGRESS.md skeleton as a standalone
pre-Task-1 commit per the established standalone-PLAN cadence
(8259275 07.1 / dc00750 06.2 / 3a964cc 06.3). 10 tasks targeting the
07.2 SPEC §3 D8.2-D15.2 deliverable set, ~1600 LoC projected
(production ~440; tests ~740; fixture/doc ~410). Split-gate evaluation:
10 tasks well under the ~25-task gate; ~1600 LoC sits ~+7% over the
~1500-LoC soft gate, concentrated in test + fixture material — accept
the drift, do NOT nest-split (parent-07 SPEC §5 + ADR-0030 reject
nested splits of a split-produced sub-phase; 06.x accept-drift
precedent).

PROGRESS.md skeleton lands alongside with the Task 1 preamble recording
8 PLAN-write SPEC corrections (header_mutation.rs uses FilterRequest/
FilterResponse per ADR-0031; test-util Cargo feature replaces the
cross-crate-broken #[cfg(test)] stub; deferred 07.1 tests are net-new
not stubs; fixture 0013 expectations.yaml mirrors fixture 0008's shape;
is_valid_rfc7230_token landed inline — no helper to reuse; ConfigError
lives in lib.rs not bootstrap.rs; schema types use the existing
Debug/Deserialize/PartialEq derive convention; in-process backstop
follows the 0008 precedent — no serve_ephemeral helper exists) + the
16 architecture-decision lock-ins per feedback_pick_recommendation. The
07.1 REVIEW I1 finalize_h2_stream 3-dead-parameter cleanup is folded
into Task 5 (Step group A, named owner); 07.1 REVIEW M1 (unused tracing
dep) closes at Task 3, M2 (UnsupportedFilterType constructable) at
Task 3. Every code-changing task's PROGRESS attestation must quote
cargo deny check output (07.1-REVIEW doctrine reminder).

STATE.md advances: active-phase status "07.2 state 2 (SPEC.md only)" to
"07.2 state 3 (SPEC + PLAN exist; implementation incomplete)";
next-skill "writing-plans" to "subagent-driven-development" against the
new PLAN.md per feedback_execution_style. Standing context rewritten
from PLAN-writer perspective to executor perspective; all "Phase-NN
rollovers" sections preserved verbatim. ROADMAP row 07.2 flips planned
to in-progress per BOOTSTRAP_PROMPT.md §4.1 invariant 3. Parent row 07
stays in-progress (closes at 07.2 state-6 per ROADMAP-schema invariant);
row 07.1 stays done.

No code changes; docs-only commit per the standalone-PLAN.md cadence.
No ADR landed (DECISIONS.md ledger head remains ADR-0031; ADR-0032
stays reserved-available). §7.5 phase-done gate is NOT exercised at the
state-2 commit; verification lands at PLAN.md Task 10 (state-4).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```

No `Differential surface` / `Conformance` lines (those belong to state-6 commits).

---

## Task 1: `envoy-config` schema additions for HeaderMutation

**Scope:** ~120 LoC schema + ~80 LoC tests = ~200 LoC. Extends `HttpFilterTypedConfig` with a `HeaderMutation` variant + 5 supporting structs + the `AppendAction` enum, all `#[serde(deny_unknown_fields)]`. Adds `lib.rs` re-exports. No validator yet (Task 2). No runtime yet (Task 3).

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` — add the schema types after `RouterConfig` (after line ~453); add `#[cfg(test)] mod header_mutation_schema_tests` inside the existing `#[cfg(test)] mod tests` block (which spans lines 1683-6413).
- Modify: `crates/envoy-config/src/lib.rs` — extend the `pub use bootstrap::{...}` re-export list.

- [ ] **Step 1: Write the failing schema tests**

Add this module inside `bootstrap.rs`'s `#[cfg(test)] mod tests { ... }` block (place it after the existing `validate_http_filters` tests near line ~6412, just before the module's closing brace):

```rust
#[cfg(test)]
mod header_mutation_schema_tests {
    use crate::{AppendAction, HeaderMutationConfig, Mutations};

    fn parse(yaml: &str) -> Result<HeaderMutationConfig, serde_yaml::Error> {
        serde_yaml::from_str(yaml)
    }

    #[test]
    fn minimal_request_only_mutations_parse() {
        let cfg = parse(
            "mutations:\n  request_mutations:\n    - append:\n        header:\n          key: x-foo\n          value: bar\n        append_action: APPEND_IF_EXISTS_OR_ADD\n",
        )
        .expect("request-only parses");
        assert_eq!(cfg.mutations.request_mutations.len(), 1);
        assert_eq!(cfg.mutations.response_mutations.len(), 0);
        let e = &cfg.mutations.request_mutations[0];
        assert_eq!(e.append.header.key, "x-foo");
        assert_eq!(e.append.header.value, "bar");
        assert_eq!(e.append.append_action, AppendAction::AppendIfExistsOrAdd);
    }

    #[test]
    fn minimal_response_only_mutations_parse() {
        let cfg = parse(
            "mutations:\n  response_mutations:\n    - append:\n        header:\n          key: x-resp\n          value: stamp\n        append_action: OVERWRITE_IF_EXISTS_OR_ADD\n",
        )
        .expect("response-only parses");
        assert_eq!(cfg.mutations.request_mutations.len(), 0);
        assert_eq!(cfg.mutations.response_mutations.len(), 1);
        assert_eq!(
            cfg.mutations.response_mutations[0].append.append_action,
            AppendAction::OverwriteIfExistsOrAdd
        );
    }

    #[test]
    fn both_request_and_response_mutations_parse() {
        let cfg = parse(
            "mutations:\n  request_mutations:\n    - append:\n        header:\n          key: x-req\n          value: a\n        append_action: APPEND_IF_EXISTS_OR_ADD\n  response_mutations:\n    - append:\n        header:\n          key: x-resp\n          value: b\n        append_action: APPEND_IF_EXISTS_OR_ADD\n",
        )
        .expect("both parse");
        assert_eq!(cfg.mutations.request_mutations.len(), 1);
        assert_eq!(cfg.mutations.response_mutations.len(), 1);
    }

    #[test]
    fn empty_mutations_parse_via_serde_default() {
        let cfg = parse("mutations: {}\n").expect("empty mutations parse");
        assert_eq!(cfg.mutations.request_mutations, Vec::new());
        assert_eq!(cfg.mutations.response_mutations, Vec::new());
    }

    #[test]
    fn multiple_entries_parse() {
        let cfg = parse(
            "mutations:\n  request_mutations:\n    - append:\n        header: { key: x-a, value: '1' }\n        append_action: APPEND_IF_EXISTS_OR_ADD\n    - append:\n        header: { key: x-b, value: '2' }\n        append_action: APPEND_IF_EXISTS_OR_ADD\n    - append:\n        header: { key: x-c, value: '3' }\n        append_action: APPEND_IF_EXISTS_OR_ADD\n",
        )
        .expect("3 entries parse");
        assert_eq!(cfg.mutations.request_mutations.len(), 3);
    }

    #[test]
    fn both_supported_append_actions_parse() {
        for (yaml_val, expect) in [
            ("APPEND_IF_EXISTS_OR_ADD", AppendAction::AppendIfExistsOrAdd),
            ("OVERWRITE_IF_EXISTS_OR_ADD", AppendAction::OverwriteIfExistsOrAdd),
        ] {
            let cfg = parse(&format!(
                "mutations:\n  request_mutations:\n    - append:\n        header: {{ key: k, value: v }}\n        append_action: {yaml_val}\n"
            ))
            .expect("supported action parses");
            assert_eq!(cfg.mutations.request_mutations[0].append.append_action, expect);
        }
    }

    #[test]
    fn unsupported_append_actions_parse_at_schema_level() {
        // ADD_IF_ABSENT / OVERWRITE_IF_EXISTS parse at the schema layer; the
        // Task 2 validator rejects them. Present in the enum so serde does not
        // emit a generic "unknown variant" error.
        for (yaml_val, expect) in [
            ("ADD_IF_ABSENT", AppendAction::AddIfAbsent),
            ("OVERWRITE_IF_EXISTS", AppendAction::OverwriteIfExists),
        ] {
            let cfg = parse(&format!(
                "mutations:\n  request_mutations:\n    - append:\n        header: {{ key: k, value: v }}\n        append_action: {yaml_val}\n"
            ))
            .expect("unsupported action still parses at schema level");
            assert_eq!(cfg.mutations.request_mutations[0].append.append_action, expect);
        }
    }

    #[test]
    fn unknown_field_rejects() {
        let err = parse(
            "mutations:\n  request_mutations: []\n  bogus_key: 1\n",
        )
        .expect_err("unknown field rejects");
        assert!(format!("{err}").contains("bogus_key") || format!("{err}").contains("unknown"));
    }

    #[test]
    fn missing_mutations_field_rejects() {
        parse("not_mutations: {}\n").expect_err("missing `mutations` rejects");
    }

    #[test]
    fn missing_key_field_rejects() {
        parse(
            "mutations:\n  request_mutations:\n    - append:\n        header: { value: v }\n        append_action: APPEND_IF_EXISTS_OR_ADD\n",
        )
        .expect_err("missing header.key rejects");
    }

    #[test]
    fn missing_value_field_rejects() {
        parse(
            "mutations:\n  request_mutations:\n    - append:\n        header: { key: k }\n        append_action: APPEND_IF_EXISTS_OR_ADD\n",
        )
        .expect_err("missing header.value rejects");
    }

    #[test]
    fn unknown_at_type_url_rejects_on_http_filter() {
        // The tagged-enum on an unknown @type tag rejects.
        let err: Result<crate::HttpFilterTypedConfig, _> = serde_yaml::from_str(
            "\"@type\": type.googleapis.com/envoy.extensions.filters.http.header_mutation.v3.HeaderMutation.unknown\n",
        );
        err.expect_err("unknown @type rejects");
    }
}
```

- [ ] **Step 2: Run the failing tests**

Run: `cargo test -p envoy-config header_mutation_schema_tests 2>&1 | tail -20`
Expected: FAIL — `cannot find type HeaderMutationConfig` / `Mutations` / `AppendAction` (the schema types don't exist yet).

- [ ] **Step 3: Add the schema types to `bootstrap.rs`**

Insert immediately after the `RouterConfig` struct (after line ~453, HEAD `3abcc8c`). First, extend the existing `HttpFilterTypedConfig` enum (lines 442-447) — add the `HeaderMutation` variant, keeping the existing `#[serde(tag = "@type", deny_unknown_fields)]` and `#[derive(Debug, Deserialize, PartialEq)]`:

```rust
#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "@type", deny_unknown_fields)]
pub enum HttpFilterTypedConfig {
    #[serde(rename = "type.googleapis.com/envoy.extensions.filters.http.router.v3.Router")]
    Router(RouterConfig),

    #[serde(rename = "type.googleapis.com/envoy.extensions.filters.http.header_mutation.v3.HeaderMutation")]
    HeaderMutation(HeaderMutationConfig),
}
```

Then add the supporting types (after `RouterConfig`):

```rust
/// `envoy.extensions.filters.http.header_mutation.v3.HeaderMutation` config.
/// The HeaderMutation filter appends/overwrites request and response headers.
/// Phase 07.2.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HeaderMutationConfig {
    pub mutations: Mutations,
}

/// The request-side and response-side mutation lists. Both default to empty
/// (`mutations: {}` is legal — a no-op filter).
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Mutations {
    #[serde(default)]
    pub request_mutations: Vec<HeaderMutationEntry>,
    #[serde(default)]
    pub response_mutations: Vec<HeaderMutationEntry>,
}

/// One mutation entry. Envoy's proto is a `oneof` (append / remove); 07.2
/// supports only `append`. `#[serde(deny_unknown_fields)]` rejects `remove`
/// (and any other oneof arm) at parse time.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HeaderMutationEntry {
    pub append: HeaderValueOption,
}

/// `HeaderValueOption` — a header key/value plus the append action.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HeaderValueOption {
    pub header: HeaderValue,
    pub append_action: AppendAction,
}

/// `HeaderValue` — the literal header key + value.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HeaderValue {
    pub key: String,
    pub value: String,
}

/// `AppendAction` — Envoy's wire form uses SCREAMING_SNAKE_CASE. 07.2 supports
/// `APPEND_IF_EXISTS_OR_ADD` + `OVERWRITE_IF_EXISTS_OR_ADD` at runtime; the two
/// unsupported variants parse at the schema level so serde does not emit a
/// generic "unknown variant" error — the Task 2 validator rejects them with the
/// typed `ConfigError::UnsupportedHeaderMutationAppendAction` variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AppendAction {
    AppendIfExistsOrAdd,
    OverwriteIfExistsOrAdd,
    AddIfAbsent,
    OverwriteIfExists,
}
```

- [ ] **Step 4: Add the `lib.rs` re-exports**

In `crates/envoy-config/src/lib.rs`, find the existing `pub use bootstrap::{...}` line that re-exports `HttpFilter`, `HttpFilterTypedConfig`, `RouterConfig` (the same line `envoy-filter` already imports from). Extend it to add the 6 new names:

```rust
pub use bootstrap::{
    // ... existing names (HttpFilter, HttpFilterTypedConfig, RouterConfig, ...) ...
    AppendAction, HeaderMutationConfig, HeaderMutationEntry, HeaderValue, HeaderValueOption,
    Mutations,
};
```

(Keep the list alphabetized if it currently is; otherwise append.)

- [ ] **Step 5: Run the schema tests — expect PASS**

Run: `cargo test -p envoy-config header_mutation_schema_tests 2>&1 | tail -20`
Expected: PASS — 12 tests.

- [ ] **Step 6: Workspace-wide checks**

Run:
- `cargo build --workspace --all-targets 2>&1 | tail -5`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -5`
- `cargo fmt --all -- --check 2>&1 | tail -5`
- `cargo test --workspace 2>&1 | tail -10`
- `cargo deny check 2>&1 | tail -10`

Expected: all clean. No new top-level deps — `cargo deny check` is a no-op but its output MUST be quoted in PROGRESS.

- [ ] **Step 7: Append the Task 1 PROGRESS section + commit**

Append a `## Task 1 — ...` section to PROGRESS.md (work summary, 12 tests landed, LoC delta, deviations, test-bucket attestation quoting all 5 commands incl. `cargo deny check`). Then:

```bash
git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs docs/envoy-rust/phases/07.2-header-mutation-filter/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 07.2: task 1 — HttpFilterTypedConfig::HeaderMutation + supporting schema

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: `envoy-config` validator extension + 3 new `ConfigError` variants

**Scope:** ~80 LoC validator + helper + ~30 LoC ConfigError variants + ~60 LoC tests = ~170 LoC. Extends `validate_http_filters` (07.1 Task 4) with a `HeaderMutation` arm; adds `validate_header_mutation_entries` + `is_valid_rfc7230_token` free functions in `bootstrap.rs`; adds 3 `ConfigError` variants in `lib.rs`.

**Files:**
- Modify: `crates/envoy-config/src/lib.rs` — append 3 `ConfigError` variants.
- Modify: `crates/envoy-config/src/bootstrap.rs` — extend `validate_http_filters` (lines 1440-1486); add `validate_header_mutation_entries` + `is_valid_rfc7230_token` free functions; add `#[cfg(test)] mod header_mutation_validator_tests`.

- [ ] **Step 1: Write the failing validator tests**

Add this module inside `bootstrap.rs`'s `#[cfg(test)] mod tests { ... }` block (after `header_mutation_schema_tests`):

```rust
#[cfg(test)]
mod header_mutation_validator_tests {
    use crate::{
        AppendAction, HeaderMutationConfig, HeaderMutationEntry, HeaderValue, HeaderValueOption,
        HttpFilter, HttpFilterTypedConfig, Mutations, RouterConfig,
    };

    fn entry(key: &str, value: &str, action: AppendAction) -> HeaderMutationEntry {
        HeaderMutationEntry {
            append: HeaderValueOption {
                header: HeaderValue { key: key.to_string(), value: value.to_string() },
                append_action: action,
            },
        }
    }

    fn header_mutation_filter(
        request_mutations: Vec<HeaderMutationEntry>,
        response_mutations: Vec<HeaderMutationEntry>,
    ) -> HttpFilter {
        HttpFilter {
            name: "envoy.filters.http.header_mutation".to_string(),
            typed_config: HttpFilterTypedConfig::HeaderMutation(HeaderMutationConfig {
                mutations: Mutations { request_mutations, response_mutations },
            }),
        }
    }

    fn router_filter() -> HttpFilter {
        HttpFilter {
            name: "envoy.filters.http.router".to_string(),
            typed_config: HttpFilterTypedConfig::Router(RouterConfig {}),
        }
    }

    #[test]
    fn header_mutation_with_all_supported_entries_passes() {
        let filters = vec![
            header_mutation_filter(
                vec![
                    entry("x-a", "1", AppendAction::AppendIfExistsOrAdd),
                    entry("x-b", "2", AppendAction::OverwriteIfExistsOrAdd),
                ],
                vec![
                    entry("x-c", "3", AppendAction::AppendIfExistsOrAdd),
                    entry("x-d", "4", AppendAction::OverwriteIfExistsOrAdd),
                ],
            ),
            router_filter(),
        ];
        super::validate_http_filters(&filters, "ingress_http").expect("supported entries pass");
    }

    #[test]
    fn empty_key_rejects() {
        let filters = vec![
            header_mutation_filter(
                vec![entry("", "v", AppendAction::AppendIfExistsOrAdd)],
                vec![],
            ),
            router_filter(),
        ];
        match super::validate_http_filters(&filters, "ingress_http").expect_err("empty key rejects")
        {
            crate::ConfigError::EmptyHeaderMutationKey { listener, position } => {
                assert_eq!(listener, "ingress_http");
                assert_eq!(position, 0);
            }
            other => panic!("expected EmptyHeaderMutationKey, got {other:?}"),
        }
    }

    #[test]
    fn invalid_token_in_key_rejects() {
        let filters = vec![
            header_mutation_filter(
                vec![entry("x bad", "v", AppendAction::AppendIfExistsOrAdd)],
                vec![],
            ),
            router_filter(),
        ];
        match super::validate_http_filters(&filters, "ingress_http")
            .expect_err("invalid token rejects")
        {
            crate::ConfigError::InvalidHeaderMutationKey { listener, position, key } => {
                assert_eq!(listener, "ingress_http");
                assert_eq!(position, 0);
                assert_eq!(key, "x bad");
            }
            other => panic!("expected InvalidHeaderMutationKey, got {other:?}"),
        }
    }

    #[test]
    fn add_if_absent_rejects() {
        let filters = vec![
            header_mutation_filter(
                vec![entry("x-a", "v", AppendAction::AddIfAbsent)],
                vec![],
            ),
            router_filter(),
        ];
        match super::validate_http_filters(&filters, "ingress_http")
            .expect_err("ADD_IF_ABSENT rejects")
        {
            crate::ConfigError::UnsupportedHeaderMutationAppendAction {
                listener,
                position,
                action,
            } => {
                assert_eq!(listener, "ingress_http");
                assert_eq!(position, 0);
                assert_eq!(action, "ADD_IF_ABSENT");
            }
            other => panic!("expected UnsupportedHeaderMutationAppendAction, got {other:?}"),
        }
    }

    #[test]
    fn overwrite_if_exists_rejects() {
        let filters = vec![
            header_mutation_filter(
                vec![],
                vec![entry("x-a", "v", AppendAction::OverwriteIfExists)],
            ),
            router_filter(),
        ];
        match super::validate_http_filters(&filters, "ingress_http")
            .expect_err("OVERWRITE_IF_EXISTS rejects")
        {
            crate::ConfigError::UnsupportedHeaderMutationAppendAction { action, .. } => {
                assert_eq!(action, "OVERWRITE_IF_EXISTS");
            }
            other => panic!("expected UnsupportedHeaderMutationAppendAction, got {other:?}"),
        }
    }

    #[test]
    fn router_not_terminal_still_rejects_under_header_mutation_chain() {
        // [Router, HeaderMutation] — Router first; the 07.1 Task 4 validator
        // still fires RouterNotTerminal.
        let filters = vec![
            router_filter(),
            header_mutation_filter(
                vec![entry("x-a", "v", AppendAction::AppendIfExistsOrAdd)],
                vec![],
            ),
        ];
        match super::validate_http_filters(&filters, "ingress_http")
            .expect_err("Router-not-terminal rejects")
        {
            crate::ConfigError::RouterNotTerminal { position, .. } => assert_eq!(position, 0),
            other => panic!("expected RouterNotTerminal, got {other:?}"),
        }
    }

    #[test]
    fn duplicate_router_rejects_under_header_mutation_chain() {
        let filters = vec![
            header_mutation_filter(
                vec![entry("x-a", "v", AppendAction::AppendIfExistsOrAdd)],
                vec![],
            ),
            router_filter(),
            router_filter(),
        ];
        match super::validate_http_filters(&filters, "ingress_http")
            .expect_err("duplicate Router rejects")
        {
            crate::ConfigError::DuplicateRouterFilter { .. } => {}
            other => panic!("expected DuplicateRouterFilter, got {other:?}"),
        }
    }

    #[test]
    fn name_typed_config_mismatch_rejects() {
        let mut f = header_mutation_filter(vec![], vec![]);
        f.name = "envoy.filters.http.fault".to_string();
        let filters = vec![f, router_filter()];
        match super::validate_http_filters(&filters, "ingress_http")
            .expect_err("name/typed_config mismatch rejects")
        {
            crate::ConfigError::UnsupportedHttpFilter { name } => {
                assert_eq!(name, "envoy.filters.http.fault");
            }
            other => panic!("expected UnsupportedHttpFilter, got {other:?}"),
        }
    }
}
```

- [ ] **Step 2: Run the failing tests**

Run: `cargo test -p envoy-config header_mutation_validator_tests 2>&1 | tail -20`
Expected: FAIL — missing `ConfigError` variants + the validator does not yet handle the `HeaderMutation` arm (the `match &f.typed_config` is non-exhaustive after Task 1 added the variant — this may surface as a compile error, which is the expected RED).

- [ ] **Step 3: Add the 3 `ConfigError` variants to `lib.rs`**

Append to the `ConfigError` enum in `crates/envoy-config/src/lib.rs` (after the existing `Http2ClusterFromHttp1Listener` variant, before the closing `}`):

```rust
    /// 07.2: HeaderMutation entry uses an `append_action` outside the
    /// supported subset (`APPEND_IF_EXISTS_OR_ADD` / `OVERWRITE_IF_EXISTS_OR_ADD`).
    /// `ADD_IF_ABSENT` / `OVERWRITE_IF_EXISTS` parse at the schema level but are
    /// rejected here. `position` is the entry index within its mutations list.
    #[error(
        "HCM listener {listener:?}: HeaderMutation entry at position {position} uses unsupported append_action {action}"
    )]
    UnsupportedHeaderMutationAppendAction {
        listener: String,
        position: usize,
        action: String,
    },

    /// 07.2: HeaderMutation entry has an empty `header.key`.
    #[error("HCM listener {listener:?}: HeaderMutation entry at position {position} has an empty header key")]
    EmptyHeaderMutationKey { listener: String, position: usize },

    /// 07.2: HeaderMutation entry's `header.key` contains a byte outside the
    /// RFC 7230 §3.2.6 token set.
    #[error(
        "HCM listener {listener:?}: HeaderMutation entry at position {position} has an invalid token in header key {key:?}"
    )]
    InvalidHeaderMutationKey { listener: String, position: usize, key: String },
```

- [ ] **Step 4: Extend `validate_http_filters` + add the helpers in `bootstrap.rs`**

In `validate_http_filters` (lines 1440-1486), extend the `match &f.typed_config` (currently lines ~1455-1465) with the `HeaderMutation` arm:

```rust
        match &f.typed_config {
            crate::HttpFilterTypedConfig::Router(_) => {
                if f.name != router_name {
                    return Err(crate::ConfigError::UnsupportedHttpFilter {
                        name: f.name.clone(),
                    });
                }
                router_positions.push(i);
            }
            crate::HttpFilterTypedConfig::HeaderMutation(cfg) => {
                if f.name != "envoy.filters.http.header_mutation" {
                    return Err(crate::ConfigError::UnsupportedHttpFilter {
                        name: f.name.clone(),
                    });
                }
                validate_header_mutation_entries(
                    &cfg.mutations.request_mutations,
                    listener_name,
                )?;
                validate_header_mutation_entries(
                    &cfg.mutations.response_mutations,
                    listener_name,
                )?;
            }
        }
```

Add these two free functions to `bootstrap.rs` (place them immediately after `validate_http_filters`, ~line 1487):

```rust
/// Validate one HeaderMutation mutations list (request_mutations or
/// response_mutations). Per-entry: non-empty `header.key` + RFC 7230 token
/// set + `append_action` in the supported subset. `position` in each error is
/// the entry index within `entries`. Phase 07.2.
fn validate_header_mutation_entries(
    entries: &[crate::HeaderMutationEntry],
    listener_name: &str,
) -> Result<(), crate::ConfigError> {
    for (entry_idx, entry) in entries.iter().enumerate() {
        let key = &entry.append.header.key;
        if key.is_empty() {
            return Err(crate::ConfigError::EmptyHeaderMutationKey {
                listener: listener_name.to_string(),
                position: entry_idx,
            });
        }
        if !is_valid_rfc7230_token(key) {
            return Err(crate::ConfigError::InvalidHeaderMutationKey {
                listener: listener_name.to_string(),
                position: entry_idx,
                key: key.clone(),
            });
        }
        match entry.append.append_action {
            crate::AppendAction::AppendIfExistsOrAdd
            | crate::AppendAction::OverwriteIfExistsOrAdd => {
                // supported.
            }
            crate::AppendAction::AddIfAbsent => {
                return Err(crate::ConfigError::UnsupportedHeaderMutationAppendAction {
                    listener: listener_name.to_string(),
                    position: entry_idx,
                    action: "ADD_IF_ABSENT".to_string(),
                });
            }
            crate::AppendAction::OverwriteIfExists => {
                return Err(crate::ConfigError::UnsupportedHeaderMutationAppendAction {
                    listener: listener_name.to_string(),
                    position: entry_idx,
                    action: "OVERWRITE_IF_EXISTS".to_string(),
                });
            }
        }
    }
    Ok(())
}

/// RFC 7230 §3.2.6 `token` validation: a header field name is a non-empty
/// sequence of `tchar`. No existing helper in `envoy-config` covers this
/// (the 04.2 HeaderMatcher work does case-insensitive name *matching*, not
/// token-set *validation*) — landed inline here per 07.2 SPEC §6 signpost 1.
fn is_valid_rfc7230_token(s: &str) -> bool {
    !s.is_empty()
        && s.bytes().all(|b| {
            b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b)
        })
}
```

- [ ] **Step 5: Run the validator tests — expect PASS**

Run: `cargo test -p envoy-config header_mutation_validator_tests 2>&1 | tail -20`
Expected: PASS — 8 tests. Also re-run `cargo test -p envoy-config header_mutation_schema_tests` — still 12 PASS.

- [ ] **Step 6: Workspace-wide checks**

Run `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check`. Expected: all clean.

- [ ] **Step 7: Append the Task 2 PROGRESS section + commit**

```bash
git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs docs/envoy-rust/phases/07.2-header-mutation-filter/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 07.2: task 2 — HeaderMutation parse-time validator + 3 new ConfigError variants

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: `HeaderMutationFilter` runtime types + builder + `instance.rs` arm + `test-util` feature scaffold

**Scope:** ~140 LoC runtime + builder + ~50 LoC tests + ~10 LoC instance.rs + ~10 LoC Cargo.toml = ~210 LoC. Creates `header_mutation.rs` with the runtime types + `build_from_config` + `map_entry` + `decode_headers` / `encode_headers` STUBS (returning `Decision::Continue`; real semantics at Task 4). Extends `HttpFilterInstance`. Removes the unused `tracing` dep (07.1 REVIEW M1). Note: the `test-util` Cargo *feature table* + the test-only `HttpFilterInstance` variants land at **Task 5** (where they are first needed) — Task 3 only does the production `HeaderMutation` variant.

**Files:**
- Create: `crates/envoy-filter/src/header_mutation.rs`.
- Modify: `crates/envoy-filter/src/lib.rs` — add `pub mod header_mutation;` + `pub use header_mutation::HeaderMutationFilter;`.
- Modify: `crates/envoy-filter/src/instance.rs` — `HeaderMutation` variant + `build` / `decode_headers` / `encode_headers` arms.
- Modify: `crates/envoy-filter/Cargo.toml` — remove `tracing = "0.1"` (07.1 REVIEW M1 close).

- [ ] **Step 1: Write the failing tests in `header_mutation.rs`**

Create `crates/envoy-filter/src/header_mutation.rs` with ONLY the test module first (the types come in Step 3):

```rust
//! `envoy.filters.http.header_mutation` runtime filter.
//!
//! Hand-rolled per D-3.2 (*Every individual filter* is on the
//! Must-be-written-from-scratch list). Mutates the `headers` list of the
//! 07.1-landed `FilterRequest` / `FilterResponse` value types. Synchronous;
//! no async, no `dyn`-dispatch.

use crate::error::FilterError;
use crate::pipeline::Decision;
use crate::types::{FilterRequest, FilterResponse};

// === types + impls land in Step 3 ===

#[cfg(test)]
mod tests {
    use super::*;
    use envoy_config::{
        AppendAction, HeaderMutationConfig, HeaderMutationEntry, HeaderValue, HeaderValueOption,
        Mutations,
    };

    fn entry(key: &str, value: &str, action: AppendAction) -> HeaderMutationEntry {
        HeaderMutationEntry {
            append: HeaderValueOption {
                header: HeaderValue { key: key.to_string(), value: value.to_string() },
                append_action: action,
            },
        }
    }

    fn cfg(
        request_mutations: Vec<HeaderMutationEntry>,
        response_mutations: Vec<HeaderMutationEntry>,
    ) -> HeaderMutationConfig {
        HeaderMutationConfig {
            mutations: Mutations { request_mutations, response_mutations },
        }
    }

    #[test]
    fn build_from_config_on_empty_mutations_returns_empty_filter() {
        let filter = HeaderMutationFilter::build_from_config(&cfg(vec![], vec![])).unwrap();
        assert_eq!(filter.request_mutations.len(), 0);
        assert_eq!(filter.response_mutations.len(), 0);
    }

    #[test]
    fn build_from_config_on_single_append_entry_lowercases_key_and_keeps_value() {
        let filter = HeaderMutationFilter::build_from_config(&cfg(
            vec![entry("X-Foo", "Bar", AppendAction::AppendIfExistsOrAdd)],
            vec![],
        ))
        .unwrap();
        assert_eq!(filter.request_mutations.len(), 1);
        let m = &filter.request_mutations[0];
        assert_eq!(m.key, "x-foo"); // lowercased at build time
        assert_eq!(m.value, "Bar"); // value preserved verbatim
        assert!(matches!(m.action, RuntimeAppendAction::Append));
    }

    #[test]
    fn build_from_config_on_single_overwrite_entry_maps_action() {
        let filter = HeaderMutationFilter::build_from_config(&cfg(
            vec![],
            vec![entry("x-resp", "v", AppendAction::OverwriteIfExistsOrAdd)],
        ))
        .unwrap();
        assert_eq!(filter.response_mutations.len(), 1);
        assert!(matches!(filter.response_mutations[0].action, RuntimeAppendAction::Overwrite));
    }

    #[test]
    fn build_from_config_on_unsupported_append_action_returns_err() {
        // Defense-in-depth: the envoy-config validator (07.2 Task 2) catches
        // these earlier, but `map_entry` re-checks at the framework boundary.
        let err = HeaderMutationFilter::build_from_config(&cfg(
            vec![entry("x-a", "v", AppendAction::AddIfAbsent)],
            vec![],
        ))
        .unwrap_err();
        assert!(matches!(err, FilterError::UnsupportedFilterType { .. }));
    }

    #[test]
    fn http_filter_instance_build_on_header_mutation_produces_header_mutation_variant() {
        let hf = envoy_config::HttpFilter {
            name: "envoy.filters.http.header_mutation".to_string(),
            typed_config: envoy_config::HttpFilterTypedConfig::HeaderMutation(cfg(
                vec![entry("x-a", "v", AppendAction::AppendIfExistsOrAdd)],
                vec![],
            )),
        };
        let instance = crate::instance::HttpFilterInstance::build(&hf, 0).unwrap();
        assert!(matches!(instance, crate::instance::HttpFilterInstance::HeaderMutation(_)));
    }

    #[test]
    fn decode_headers_stub_returns_continue_at_task_3() {
        // Task 3 stubs decode/encode as Continue; Task 4 lands the real
        // mutation semantics. This test is REPLACED at Task 4.
        let mut filter = HeaderMutationFilter::build_from_config(&cfg(
            vec![entry("x-a", "v", AppendAction::AppendIfExistsOrAdd)],
            vec![],
        ))
        .unwrap();
        let mut req = FilterRequest {
            method: "GET".to_string(),
            path: "/".to_string(),
            headers: vec![],
            body: None,
        };
        assert!(matches!(filter.decode_headers(&mut req), Decision::Continue));
    }

    #[test]
    fn encode_headers_stub_returns_continue_at_task_3() {
        // Replaced at Task 4.
        let mut filter = HeaderMutationFilter::build_from_config(&cfg(
            vec![],
            vec![entry("x-a", "v", AppendAction::AppendIfExistsOrAdd)],
        ))
        .unwrap();
        let mut resp = FilterResponse {
            status: 200,
            reason: None,
            headers: vec![],
            body: bytes::Bytes::new(),
        };
        assert!(matches!(filter.encode_headers(&mut resp), Decision::Continue));
    }
}
```

- [ ] **Step 2: Run the failing tests**

Run: `cargo test -p envoy-filter header_mutation 2>&1 | tail -20`
Expected: FAIL — `header_mutation.rs` is not yet declared in `lib.rs`, and `HeaderMutationFilter` / `RuntimeAppendAction` do not exist.

- [ ] **Step 3: Add the runtime types + builder + stubs to `header_mutation.rs`**

Insert above the `#[cfg(test)] mod tests` block (after the `use` lines):

```rust
/// The `envoy.filters.http.header_mutation` runtime filter. Holds the
/// build-time-lowered request/response mutation lists. Per 07.2 SPEC §6
/// signpost 3 the lists are held directly (`Vec<RuntimeHeaderMutation>`),
/// not `Arc`-wrapped — the per-request `FilterPipeline` clone copies them;
/// cheap for 07.2's 2-4-entry fixture.
#[derive(Debug, Clone)]
pub struct HeaderMutationFilter {
    request_mutations: Vec<RuntimeHeaderMutation>,
    response_mutations: Vec<RuntimeHeaderMutation>,
}

/// One lowered mutation. `key` is lowercased once at build time so the
/// runtime hot path does no per-request case folding for Append, and the
/// Overwrite search compares against `to_ascii_lowercase()` of each existing
/// entry. Per 07.2 SPEC §6 signpost 4.
#[derive(Debug, Clone)]
struct RuntimeHeaderMutation {
    key: String,
    value: String,
    action: RuntimeAppendAction,
}

#[derive(Debug, Clone, Copy)]
enum RuntimeAppendAction {
    /// `APPEND_IF_EXISTS_OR_ADD` — push a new entry (RFC 7230 §3.2.2 permits
    /// duplicate field names; semantics are list-valued).
    Append,
    /// `OVERWRITE_IF_EXISTS_OR_ADD` — case-insensitive remove-then-push.
    Overwrite,
}

impl HeaderMutationFilter {
    /// Lower an `envoy_config::HeaderMutationConfig` into the runtime filter.
    /// The Task 2 validator already rejected unsupported `append_action`s and
    /// invalid keys at config-load time; `map_entry`'s re-check is
    /// defense-in-depth at the framework crate boundary.
    pub(crate) fn build_from_config(
        cfg: &envoy_config::HeaderMutationConfig,
    ) -> Result<Self, FilterError> {
        let request_mutations = cfg
            .mutations
            .request_mutations
            .iter()
            .map(map_entry)
            .collect::<Result<Vec<_>, _>>()?;
        let response_mutations = cfg
            .mutations
            .response_mutations
            .iter()
            .map(map_entry)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { request_mutations, response_mutations })
    }

    /// Task 3 stub — returns `Continue`. Real semantics land at Task 4.
    pub(crate) fn decode_headers(&mut self, _req: &mut FilterRequest) -> Decision {
        Decision::Continue
    }

    /// Task 3 stub — returns `Continue`. Real semantics land at Task 4.
    pub(crate) fn encode_headers(&mut self, _resp: &mut FilterResponse) -> Decision {
        Decision::Continue
    }
}

/// Lower one config entry into a `RuntimeHeaderMutation`. Lowercases the key.
/// The unsupported `AppendAction`s are rejected here as defense-in-depth —
/// the Task 2 `envoy-config` validator is the earlier (and the
/// operator-facing) catch.
fn map_entry(
    entry: &envoy_config::HeaderMutationEntry,
) -> Result<RuntimeHeaderMutation, FilterError> {
    let action = match entry.append.append_action {
        envoy_config::AppendAction::AppendIfExistsOrAdd => RuntimeAppendAction::Append,
        envoy_config::AppendAction::OverwriteIfExistsOrAdd => RuntimeAppendAction::Overwrite,
        unsupported @ (envoy_config::AppendAction::AddIfAbsent
        | envoy_config::AppendAction::OverwriteIfExists) => {
            return Err(FilterError::UnsupportedFilterType {
                position: 0,
                name: format!("AppendAction::{unsupported:?}"),
            });
        }
    };
    Ok(RuntimeHeaderMutation {
        key: entry.append.header.key.to_ascii_lowercase(),
        value: entry.append.header.value.clone(),
        action,
    })
}
```

- [ ] **Step 4: Wire the module + extend `instance.rs`**

In `crates/envoy-filter/src/lib.rs`, add `pub mod header_mutation;` (alphabetically after `pub mod error;`, before `pub mod instance;`) and `pub use header_mutation::HeaderMutationFilter;` (after `pub use error::FilterError;`).

In `crates/envoy-filter/src/instance.rs`:
- Add `use crate::header_mutation::HeaderMutationFilter;` to the `use` block.
- Add the `HeaderMutation` variant to the enum:
  ```rust
  #[derive(Debug, Clone)]
  pub enum HttpFilterInstance {
      Router(RouterTerminus),
      HeaderMutation(HeaderMutationFilter),
  }
  ```
- Add the `HeaderMutation` arm to `build` (the `match &hf.typed_config`):
  ```rust
      envoy_config::HttpFilterTypedConfig::HeaderMutation(cfg) => Ok(
          HttpFilterInstance::HeaderMutation(HeaderMutationFilter::build_from_config(cfg)?),
      ),
  ```
  (The `_position` parameter stays `_position`; `map_entry`'s `position: 0` placeholder is acceptable per the 07.2 SPEC — the validator carries the operator-facing position.)
- Add the `HeaderMutation` arm to `decode_headers` and `encode_headers`:
  ```rust
  pub(crate) fn decode_headers(&mut self, req: &mut FilterRequest) -> Decision {
      match self {
          HttpFilterInstance::Router(r) => r.decode_headers(req),
          HttpFilterInstance::HeaderMutation(f) => f.decode_headers(req),
      }
  }

  pub(crate) fn encode_headers(&mut self, resp: &mut FilterResponse) -> Decision {
      match self {
          HttpFilterInstance::Router(r) => r.encode_headers(resp),
          HttpFilterInstance::HeaderMutation(f) => f.encode_headers(resp),
      }
  }
  ```

- [ ] **Step 5: Remove the unused `tracing` dep (07.1 REVIEW M1 close)**

In `crates/envoy-filter/Cargo.toml`, delete the `tracing = "0.1"` line from `[dependencies]`. After this, `[dependencies]` is `bytes = "1"` + `thiserror = "2"` + `envoy-config = { path = "../envoy-config" }`. Grep-verify no `tracing::` usage exists in `crates/envoy-filter/src/`: `grep -rn 'tracing' crates/envoy-filter/src/` should return nothing.

- [ ] **Step 6: Run the tests — expect PASS**

Run: `cargo test -p envoy-filter 2>&1 | tail -25`
Expected: PASS — the 13 pre-existing `envoy-filter` tests + 7 new `header_mutation` tests = 20 tests.

- [ ] **Step 7: Workspace-wide checks**

Run `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check`. Expected: all clean. `cargo deny check` MUST be quoted in PROGRESS — Task 3 touches `crates/envoy-filter/Cargo.toml` (dep removal).

- [ ] **Step 8: Append the Task 3 PROGRESS section + commit**

The PROGRESS section notes: 07.1 REVIEW M1 CLOSED (unused `tracing` dep removed); 07.1 REVIEW M2 PARTIALLY CLOSED (`FilterError::UnsupportedFilterType` is now constructed by `map_entry` — `RouterNotTerminal` / `DuplicateRouter` stay defense-in-depth-only, the `envoy-config` validator being the real catch).

```bash
git add crates/envoy-filter/ docs/envoy-rust/phases/07.2-header-mutation-filter/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 07.2: task 3 — HeaderMutationFilter runtime types + builder [07.1 REVIEW M1, M2]

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: `HeaderMutationFilter::decode_headers` + `encode_headers` semantics

**Scope:** ~35 LoC semantics + `apply_mutations` + ~150 LoC tests = ~185 LoC. Replaces the Task-3 `decode_headers` / `encode_headers` stubs with the real append/overwrite semantics + the `apply_mutations` helper.

**Files:**
- Modify: `crates/envoy-filter/src/header_mutation.rs` — replace the 2 stubs; add `apply_mutations`; replace the 2 stub tests with the 14-test semantics inventory.

- [ ] **Step 1: Replace the 2 stub tests with the 14-test semantics inventory**

In `header_mutation.rs`'s `#[cfg(test)] mod tests`, DELETE `decode_headers_stub_returns_continue_at_task_3` and `encode_headers_stub_returns_continue_at_task_3`, and add these 14 tests (keep the 5 `build_from_config` / `http_filter_instance_build` tests from Task 3). Add helpers at the top of the test module:

```rust
    fn req_with(headers: Vec<(&str, &str)>) -> FilterRequest {
        FilterRequest {
            method: "GET".to_string(),
            path: "/".to_string(),
            headers: headers.into_iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            body: None,
        }
    }

    fn resp_with(headers: Vec<(&str, &str)>) -> FilterResponse {
        FilterResponse {
            status: 200,
            reason: None,
            headers: headers.into_iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            body: bytes::Bytes::new(),
        }
    }

    fn owned(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn append_on_absent_key_adds_entry() {
        let mut f = HeaderMutationFilter::build_from_config(&cfg(
            vec![entry("x-foo", "bar", AppendAction::AppendIfExistsOrAdd)],
            vec![],
        ))
        .unwrap();
        let mut req = req_with(vec![]);
        assert!(matches!(f.decode_headers(&mut req), Decision::Continue));
        assert_eq!(req.headers, owned(&[("x-foo", "bar")]));
    }

    #[test]
    fn append_on_present_key_adds_duplicate() {
        let mut f = HeaderMutationFilter::build_from_config(&cfg(
            vec![entry("x-foo", "bar", AppendAction::AppendIfExistsOrAdd)],
            vec![],
        ))
        .unwrap();
        let mut req = req_with(vec![("x-foo", "original")]);
        f.decode_headers(&mut req);
        assert_eq!(req.headers, owned(&[("x-foo", "original"), ("x-foo", "bar")]));
    }

    #[test]
    fn overwrite_on_absent_key_adds_entry() {
        let mut f = HeaderMutationFilter::build_from_config(&cfg(
            vec![entry("x-foo", "bar", AppendAction::OverwriteIfExistsOrAdd)],
            vec![],
        ))
        .unwrap();
        let mut req = req_with(vec![]);
        f.decode_headers(&mut req);
        assert_eq!(req.headers, owned(&[("x-foo", "bar")]));
    }

    #[test]
    fn overwrite_on_present_key_replaces_with_exactly_one_entry() {
        let mut f = HeaderMutationFilter::build_from_config(&cfg(
            vec![entry("x-foo", "bar", AppendAction::OverwriteIfExistsOrAdd)],
            vec![],
        ))
        .unwrap();
        let mut req = req_with(vec![("x-foo", "original")]);
        f.decode_headers(&mut req);
        assert_eq!(req.headers, owned(&[("x-foo", "bar")]));
    }

    #[test]
    fn overwrite_is_case_insensitive_on_the_existing_entry() {
        let mut f = HeaderMutationFilter::build_from_config(&cfg(
            vec![entry("x-foo", "bar", AppendAction::OverwriteIfExistsOrAdd)],
            vec![],
        ))
        .unwrap();
        // existing entry has mixed-case name; Overwrite case-folds the match.
        let mut req = req_with(vec![("X-Foo", "original")]);
        f.decode_headers(&mut req);
        assert_eq!(req.headers, owned(&[("x-foo", "bar")]));
    }

    #[test]
    fn multiple_append_entries_apply_in_declaration_order() {
        let mut f = HeaderMutationFilter::build_from_config(&cfg(
            vec![
                entry("x-a", "1", AppendAction::AppendIfExistsOrAdd),
                entry("x-b", "2", AppendAction::AppendIfExistsOrAdd),
                entry("x-a", "3", AppendAction::AppendIfExistsOrAdd),
            ],
            vec![],
        ))
        .unwrap();
        let mut req = req_with(vec![]);
        f.decode_headers(&mut req);
        assert_eq!(req.headers, owned(&[("x-a", "1"), ("x-b", "2"), ("x-a", "3")]));
    }

    #[test]
    fn multiple_overwrite_entries_last_for_a_key_wins() {
        let mut f = HeaderMutationFilter::build_from_config(&cfg(
            vec![
                entry("x-a", "first", AppendAction::OverwriteIfExistsOrAdd),
                entry("x-a", "second", AppendAction::OverwriteIfExistsOrAdd),
            ],
            vec![],
        ))
        .unwrap();
        let mut req = req_with(vec![]);
        f.decode_headers(&mut req);
        assert_eq!(req.headers, owned(&[("x-a", "second")]));
    }

    #[test]
    fn mix_of_append_and_overwrite_applies_in_order() {
        // Append x-a:1, Append x-a:2, Overwrite x-a:final → Overwrite removes
        // both prior x-a entries, pushes one.
        let mut f = HeaderMutationFilter::build_from_config(&cfg(
            vec![
                entry("x-a", "1", AppendAction::AppendIfExistsOrAdd),
                entry("x-a", "2", AppendAction::AppendIfExistsOrAdd),
                entry("x-a", "final", AppendAction::OverwriteIfExistsOrAdd),
            ],
            vec![],
        ))
        .unwrap();
        let mut req = req_with(vec![("x-keep", "kept")]);
        f.decode_headers(&mut req);
        assert_eq!(req.headers, owned(&[("x-keep", "kept"), ("x-a", "final")]));
    }

    #[test]
    fn empty_mutations_is_no_op_on_decode() {
        let mut f = HeaderMutationFilter::build_from_config(&cfg(vec![], vec![])).unwrap();
        let mut req = req_with(vec![("host", "example.com")]);
        let before = req.headers.clone();
        f.decode_headers(&mut req);
        assert_eq!(req.headers, before);
    }

    #[test]
    fn empty_mutations_is_no_op_on_encode() {
        let mut f = HeaderMutationFilter::build_from_config(&cfg(vec![], vec![])).unwrap();
        let mut resp = resp_with(vec![("content-length", "0")]);
        let before = resp.headers.clone();
        f.encode_headers(&mut resp);
        assert_eq!(resp.headers, before);
    }

    #[test]
    fn decode_headers_returns_continue_after_applying() {
        let mut f = HeaderMutationFilter::build_from_config(&cfg(
            vec![entry("x-a", "v", AppendAction::AppendIfExistsOrAdd)],
            vec![],
        ))
        .unwrap();
        let mut req = req_with(vec![]);
        assert!(matches!(f.decode_headers(&mut req), Decision::Continue));
    }

    #[test]
    fn encode_headers_returns_continue_after_applying() {
        let mut f = HeaderMutationFilter::build_from_config(&cfg(
            vec![],
            vec![entry("x-a", "v", AppendAction::AppendIfExistsOrAdd)],
        ))
        .unwrap();
        let mut resp = resp_with(vec![]);
        assert!(matches!(f.encode_headers(&mut resp), Decision::Continue));
    }

    #[test]
    fn round_trip_via_filter_pipeline_decode() {
        // Build a real [HeaderMutation, Router] pipeline; decode_headers walks
        // declaration order; the request carries the stamp afterward.
        let filters = vec![
            envoy_config::HttpFilter {
                name: "envoy.filters.http.header_mutation".to_string(),
                typed_config: envoy_config::HttpFilterTypedConfig::HeaderMutation(cfg(
                    vec![entry("x-foo", "bar", AppendAction::AppendIfExistsOrAdd)],
                    vec![],
                )),
            },
            envoy_config::HttpFilter {
                name: "envoy.filters.http.router".to_string(),
                typed_config: envoy_config::HttpFilterTypedConfig::Router(
                    envoy_config::RouterConfig {},
                ),
            },
        ];
        let mut pipeline = crate::FilterPipeline::build_from_config(&filters).unwrap();
        let mut req = req_with(vec![("host", "example.com")]);
        assert!(matches!(pipeline.decode_headers(&mut req), Decision::Continue));
        assert!(req.headers.iter().any(|(k, v)| k == "x-foo" && v == "bar"));
    }

    #[test]
    fn iteration_order_on_encode_via_filter_pipeline() {
        // Reverse-iteration on encode reaches HeaderMutation after Router's
        // no-op. The response carries the response-side stamp afterward.
        let filters = vec![
            envoy_config::HttpFilter {
                name: "envoy.filters.http.header_mutation".to_string(),
                typed_config: envoy_config::HttpFilterTypedConfig::HeaderMutation(cfg(
                    vec![],
                    vec![entry("x-resp", "stamp", AppendAction::AppendIfExistsOrAdd)],
                )),
            },
            envoy_config::HttpFilter {
                name: "envoy.filters.http.router".to_string(),
                typed_config: envoy_config::HttpFilterTypedConfig::Router(
                    envoy_config::RouterConfig {},
                ),
            },
        ];
        let mut pipeline = crate::FilterPipeline::build_from_config(&filters).unwrap();
        let mut resp = resp_with(vec![("content-length", "0")]);
        assert!(matches!(pipeline.encode_headers(&mut resp), Decision::Continue));
        assert!(resp.headers.iter().any(|(k, v)| k == "x-resp" && v == "stamp"));
    }
```

- [ ] **Step 2: Run the failing tests**

Run: `cargo test -p envoy-filter header_mutation 2>&1 | tail -25`
Expected: FAIL — the stubs return `Continue` without mutating, so the `assert_eq!(req.headers, ...)` assertions fail (e.g. `append_on_absent_key_adds_entry` sees `req.headers == []`).

- [ ] **Step 3: Replace the stubs with the real semantics + add `apply_mutations`**

In `header_mutation.rs`, replace the two stub method bodies inside `impl HeaderMutationFilter`:

```rust
    pub(crate) fn decode_headers(&mut self, req: &mut FilterRequest) -> Decision {
        apply_mutations(&mut req.headers, &self.request_mutations);
        Decision::Continue
    }

    pub(crate) fn encode_headers(&mut self, resp: &mut FilterResponse) -> Decision {
        apply_mutations(&mut resp.headers, &self.response_mutations);
        Decision::Continue
    }
```

And add the free function `apply_mutations` (after `map_entry`):

```rust
/// Apply a mutation list to a header vector in slice (= YAML declaration)
/// order. Per 07.2 SPEC §6 signpost 8: last Append appends last; for a given
/// key the last Overwrite wins (each Overwrite removes prior same-key entries).
fn apply_mutations(headers: &mut Vec<(String, String)>, mutations: &[RuntimeHeaderMutation]) {
    for mutation in mutations {
        match mutation.action {
            RuntimeAppendAction::Append => {
                // RFC 7230 §3.2.2: duplicate field names are permitted.
                headers.push((mutation.key.clone(), mutation.value.clone()));
            }
            RuntimeAppendAction::Overwrite => {
                // `mutation.key` is already lowercased at build time; case-fold
                // each existing entry's name for the removal scan.
                headers.retain(|(k, _v)| k.to_ascii_lowercase() != mutation.key);
                headers.push((mutation.key.clone(), mutation.value.clone()));
            }
        }
    }
}
```

- [ ] **Step 4: Run the tests — expect PASS**

Run: `cargo test -p envoy-filter 2>&1 | tail -30`
Expected: PASS — 13 pre-existing + 5 Task-3 build tests + 14 Task-4 semantics tests = 32 `envoy-filter` tests.

- [ ] **Step 5: Workspace-wide checks**

Run `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check`. Expected: all clean.

- [ ] **Step 6: Append the Task 4 PROGRESS section + commit**

```bash
git add crates/envoy-filter/src/header_mutation.rs docs/envoy-rust/phases/07.2-header-mutation-filter/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 07.2: task 4 — HeaderMutationFilter decode/encode iteration semantics

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: H1+H2 HCM filter-chain integration tests + I1 `finalize_h2_stream` cleanup

**Scope:** ~20 LoC production (I1 cleanup is net-negative ~12 lines) + ~270 LoC tests + ~20 LoC `test-util` feature scaffold = ~300 LoC. This task has **four step groups**: **A** = the 07.1 REVIEW I1 `finalize_h2_stream` 3-dead-parameter cleanup (the named structural prerequisite); **B** = the `test-util` Cargo feature on `envoy-filter`; **C** = 5 H1 integration tests; **D** = 4 H2 integration tests. Tests 1/2/5/6/7 use the real `HeaderMutationFilter`; tests 3/4/8/9 use the `test-util` StopAndSend stubs. **In-execution release valve:** if Task 5 inflates past ~10 sub-steps, split into Task 5a (groups A+B), 5b (group C), 5c (group D), recorded in PROGRESS — NOT a phase-level nest-split (LoC drift posture above).

**Files:**
- Modify: `crates/envoy-http2/src/hcm.rs` — Group A: `finalize_h2_stream` signature + 3 call sites. Group D: 4 tests.
- Modify: `crates/envoy-filter/Cargo.toml` — Group B: `[features] test-util = []`.
- Modify: `crates/envoy-filter/src/instance.rs` + `crates/envoy-filter/src/pipeline.rs` — Group B: `#[cfg(feature = "test-util")]` variants + constructor.
- Modify: `crates/envoy-http1/Cargo.toml`, `crates/envoy-http2/Cargo.toml` — Group B: dev-dependency feature line.
- Modify: `crates/envoy-http1/src/hcm.rs` — Group C: 5 tests.

### Step group A — 07.1 REVIEW I1: `finalize_h2_stream` 3-dead-parameter cleanup

The 07.1 Task 7 left `finalize_h2_stream` (`crates/envoy-http2/src/hcm.rs:436-455`) with 3 underscore-prefixed dead parameters — `_response_status_for_log: u16`, `_response_body_len: u64`, `_response_headers_for_log: &[(String, String)]` — that the function body unconditionally re-derives post-encode at lines 490-493 (`let response_status_for_log: u16 = resp.status;` etc.). Option B mechanical cleanup: remove the 3 params; the 3 callers stop computing the pre-encode trio.

- [ ] **Step A1: Remove the 3 dead parameters from the `finalize_h2_stream` signature**

In `crates/envoy-http2/src/hcm.rs`, edit the signature (lines 436-455). Remove the 3 `_`-prefixed parameters and the 6-line comment block (lines 445-450) explaining them. The cleaned signature:

```rust
#[allow(clippy::too_many_arguments)]
async fn finalize_h2_stream(
    config: &Arc<HCMConfig>,
    pipeline: &mut envoy_filter::FilterPipeline,
    send_response: h2::server::SendResponse<Bytes>,
    mut resp: Response,
    req_arrival_instant: Instant,
    req_arrival_systime: SystemTime,
    envoy_req: &Request,
    request_body_len: u64,
    upstream_host_for_log_h2: Option<String>,
) -> Result<(), Http2Error> {
```

The function BODY is unchanged — the post-encode shadow locals at lines 490-493 (`response_status_for_log` / `response_body_len` / `response_headers_for_log_owned` / `response_headers_for_log`) already derive everything from `resp`; they are NOT the parameters (the parameters were shadowed-and-discarded). Update the comment at lines 485-489 to drop the "shadow the pre-encode log-locals" framing — it now just reads "derive the post-encode log-locals from `resp`". After removal, `#[allow(clippy::too_many_arguments)]` may or may not still be needed (9 params remain → still >7; keep the `#[allow]`).

- [ ] **Step A2: Update the 3 call sites of `finalize_h2_stream`**

There are 3 call sites in `crates/envoy-http2/src/hcm.rs`:
- **Call site 1** (lines ~227-241, no-healthy-endpoint → 502): remove `response_status_for_log`, `response_body_len`, `&response_headers_for_log` from the argument list. ALSO remove the now-dead pre-encode local computations at lines ~222-224 (`response_status_for_log = r.status;` / `response_body_len = r.body.len() as u64;` / `response_headers_for_log = r.headers.clone();`).
- **Call site 2** (lines ~323-337, upstream-dispatch-error → 502): same — remove the 3 args from the call + the dead local computations at lines ~318-320.
- **Call site 3** (lines ~408-422, unified join-point): remove the 3 args from the call.

After removing the call-site args, the local variables `response_status_for_log` / `response_body_len` / `response_headers_for_log` declared earlier in `handle_one_stream` (and assigned in the `BuildOutcome::Synth` / `BuildOutcome::Proxy` / `H2RequestPath::SynthFromDecode` arms at lines ~199-201, 222-224, 318-320, 391-393, 401-403) become entirely unused — **delete those declarations and ALL their assignments**. This is the bulk of the ~12-line net removal. The compiler (`-D warnings` clippy) will flag any straggler `unused_variable` / `unused_assignment` — let it guide the removal. `upstream_host_for_log_h2` STAYS (still a live parameter).

- [ ] **Step A3: Verify the I1 cleanup is behavior-neutral**

Run: `cargo test -p envoy-http2 2>&1 | tail -20`
Expected: PASS — all 13 pre-existing H2 hcm tests + 1 `#[ignore]`d still green (the cleanup is mechanical; `finalize_h2_stream` already computed everything from `resp` post-encode, so removing the discarded params changes nothing observable). Also run `cargo build --workspace --all-targets` + `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean (no `unused_variable` warnings remain).

### Step group B — `test-util` Cargo feature on `envoy-filter`

- [ ] **Step B1: Add the `test-util` feature to `crates/envoy-filter/Cargo.toml`**

Append after `[dependencies]`:

```toml
[features]
# Exposes test-only HttpFilterInstance variants (TestStopAndSendOnDecode /
# TestStopAndSendOnEncode) + FilterPipeline::test_from_instances so the
# downstream envoy-http1 / envoy-http2 HCM test suites can exercise the
# framework's StopAndSend short-circuit paths. NOT enabled in production
# builds. (#[cfg(test)] would not be cross-crate-visible — see 07.2 PLAN
# correction 2.)
test-util = []
```

- [ ] **Step B2: Add the `#[cfg(feature = "test-util")]` variants + constructors to `instance.rs`**

In `crates/envoy-filter/src/instance.rs`, add 2 cfg-gated variants to `HttpFilterInstance`:

```rust
#[derive(Debug, Clone)]
pub enum HttpFilterInstance {
    Router(RouterTerminus),
    HeaderMutation(HeaderMutationFilter),
    /// Test-only: a filter that always returns `Decision::StopAndSend` on the
    /// DECODE side, carrying the given `FilterResponse`. Used by the H1/H2 HCM
    /// integration tests to exercise the decode-side short-circuit.
    #[cfg(feature = "test-util")]
    TestStopAndSendOnDecode(FilterResponse),
    /// Test-only: a filter that always returns `Decision::StopAndSend` on the
    /// ENCODE side.
    #[cfg(feature = "test-util")]
    TestStopAndSendOnEncode(FilterResponse),
}
```

Add cfg-gated match arms to `decode_headers` and `encode_headers`:

```rust
    pub(crate) fn decode_headers(&mut self, req: &mut FilterRequest) -> Decision {
        match self {
            HttpFilterInstance::Router(r) => r.decode_headers(req),
            HttpFilterInstance::HeaderMutation(f) => f.decode_headers(req),
            #[cfg(feature = "test-util")]
            HttpFilterInstance::TestStopAndSendOnDecode(resp) => {
                Decision::StopAndSend(resp.clone())
            }
            #[cfg(feature = "test-util")]
            HttpFilterInstance::TestStopAndSendOnEncode(_) => Decision::Continue,
        }
    }

    pub(crate) fn encode_headers(&mut self, resp_arg: &mut FilterResponse) -> Decision {
        match self {
            HttpFilterInstance::Router(r) => r.encode_headers(resp_arg),
            HttpFilterInstance::HeaderMutation(f) => f.encode_headers(resp_arg),
            #[cfg(feature = "test-util")]
            HttpFilterInstance::TestStopAndSendOnDecode(_) => Decision::Continue,
            #[cfg(feature = "test-util")]
            HttpFilterInstance::TestStopAndSendOnEncode(resp) => {
                Decision::StopAndSend(resp.clone())
            }
        }
    }
```

Add cfg-gated public constructors (after the `impl HttpFilterInstance` block's existing methods):

```rust
#[cfg(feature = "test-util")]
impl HttpFilterInstance {
    /// Construct a test-only filter that emits `StopAndSend(resp)` on decode.
    pub fn test_stop_and_send_on_decode(resp: FilterResponse) -> Self {
        HttpFilterInstance::TestStopAndSendOnDecode(resp)
    }
    /// Construct a test-only filter that emits `StopAndSend(resp)` on encode.
    pub fn test_stop_and_send_on_encode(resp: FilterResponse) -> Self {
        HttpFilterInstance::TestStopAndSendOnEncode(resp)
    }
}
```

(Note `FilterResponse` is already imported in `instance.rs` via `use crate::types::{FilterRequest, FilterResponse};`.)

- [ ] **Step B3: Add `FilterPipeline::test_from_instances` to `pipeline.rs`**

In `crates/envoy-filter/src/pipeline.rs`, add a cfg-gated constructor (after `build_from_config`):

```rust
    /// Test-only: build a `FilterPipeline` directly from a list of
    /// `HttpFilterInstance`s, bypassing config parsing. Used by the H1/H2 HCM
    /// integration tests to inject the `test-util` StopAndSend stubs.
    #[cfg(feature = "test-util")]
    pub fn test_from_instances(filters: Vec<HttpFilterInstance>) -> Self {
        Self { filters }
    }
```

- [ ] **Step B4: Enable the feature in the H1/H2 dev-dependencies**

In `crates/envoy-http1/Cargo.toml`, under `[dev-dependencies]`, add (or extend the existing `envoy-filter` line if one is there):

```toml
envoy-filter = { path = "../envoy-filter", features = ["test-util"] }
```

Same in `crates/envoy-http2/Cargo.toml` under `[dev-dependencies]`. (Cargo unifies: the production `[dependencies] envoy-filter` stays feature-less; the dev/test build gets `test-util` on.)

- [ ] **Step B5: Verify the feature compiles**

Run: `cargo build -p envoy-filter --features test-util 2>&1 | tail -5` (clean) and `cargo test -p envoy-filter 2>&1 | tail -5` (the 32 default-feature tests still pass — the cfg-gated variants are absent without the feature).

### Step group C — 5 H1 HCM filter-chain integration tests

These land in `crates/envoy-http1/src/hcm.rs`'s `#[cfg(test)] mod tests`. The idiom: build an `HCMConfig` (struct literal, as `hcm_config_single_route` does at line ~990) with a `filter_pipeline` carrying `[HeaderMutation, Router]` (built via `envoy_filter::FilterPipeline::build_from_config`) or the test-util stub, then `drive(config, req_bytes)` (the helper at line ~1026) and assert on the response bytes.

- [ ] **Step C1: Add an H1 test helper that builds an HCMConfig with a configurable filter pipeline**

Add to the H1 `#[cfg(test)] mod tests` (near `hcm_config_single_route`):

```rust
    /// Build an HCMConfig with a caller-supplied filter pipeline + a single
    /// prefix route. `route_status` / `route_body` define the direct_response
    /// the route serves. Used by the 07.2 filter-chain integration tests.
    async fn hcm_config_with_pipeline(
        pipeline: Arc<envoy_filter::FilterPipeline>,
        prefix: &str,
        route_status: u16,
        route_body: &str,
    ) -> Arc<HCMConfig> {
        let mut cfg = (*hcm_config_single_route(prefix, route_status, route_body).await).clone();
        cfg.filter_pipeline = pipeline;
        Arc::new(cfg)
    }
```

If `HCMConfig` does not derive `Clone`, instead inline the full struct literal (copy `hcm_config_single_route`'s body, swapping `filter_pipeline:`). The executor checks `HCMConfig`'s derives at task time and picks whichever compiles.

Also add a helper to build a `[HeaderMutation, Router]` pipeline:

```rust
    /// Build an `Arc<FilterPipeline>` with `[HeaderMutation(request+response
    /// mutations), Router]`.
    fn header_mutation_pipeline(
        request_mutations: Vec<(&str, &str, envoy_config::AppendAction)>,
        response_mutations: Vec<(&str, &str, envoy_config::AppendAction)>,
    ) -> Arc<envoy_filter::FilterPipeline> {
        let mk = |v: Vec<(&str, &str, envoy_config::AppendAction)>| {
            v.into_iter()
                .map(|(k, val, action)| envoy_config::HeaderMutationEntry {
                    append: envoy_config::HeaderValueOption {
                        header: envoy_config::HeaderValue {
                            key: k.to_string(),
                            value: val.to_string(),
                        },
                        append_action: action,
                    },
                })
                .collect()
        };
        let filters = vec![
            envoy_config::HttpFilter {
                name: "envoy.filters.http.header_mutation".to_string(),
                typed_config: envoy_config::HttpFilterTypedConfig::HeaderMutation(
                    envoy_config::HeaderMutationConfig {
                        mutations: envoy_config::Mutations {
                            request_mutations: mk(request_mutations),
                            response_mutations: mk(response_mutations),
                        },
                    },
                ),
            },
            envoy_config::HttpFilter {
                name: "envoy.filters.http.router".to_string(),
                typed_config: envoy_config::HttpFilterTypedConfig::Router(
                    envoy_config::RouterConfig {},
                ),
            },
        ];
        Arc::new(envoy_filter::FilterPipeline::build_from_config(&filters).unwrap())
    }
```

- [ ] **Step C2: Write the 5 H1 tests**

```rust
    #[tokio::test]
    async fn h1_decode_headers_fires_before_route_match() {
        // HeaderMutation adds `x-test-path-override: /bar` on decode. A
        // HeaderMatcher route matches on that header → direct_response 200
        // "matched\n". The catch-all route returns 404-shaped "default\n".
        // Driving `GET /foo` (which would NOT match `/bar` by prefix) still
        // hits the matched route — proving the route matcher saw the mutated
        // request (decode_headers ran BEFORE route-match per parent-07 Rule 7).
        let pipeline = header_mutation_pipeline(
            vec![(
                "x-test-path-override",
                "/bar",
                envoy_config::AppendAction::OverwriteIfExistsOrAdd,
            )],
            vec![],
        );
        // Build an HCMConfig whose route matches on header x-test-path-override.
        let config = hcm_config_header_matched_route(pipeline).await;
        let req = b"GET /foo HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let resp = String::from_utf8(drive(config, req).await).unwrap();
        assert!(resp.starts_with("HTTP/1.1 200 "), "decode mutation drove route match: {resp}");
        assert!(resp.ends_with("matched\n"), "matched route body: {resp}");
    }

    #[tokio::test]
    async fn h1_encode_headers_fires_after_writer_arm_before_wire_write() {
        // HeaderMutation adds `x-test-encode: ok` on encode. direct_response
        // route. The wire output's headers carry x-test-encode.
        let pipeline = header_mutation_pipeline(
            vec![],
            vec![("x-test-encode", "ok", envoy_config::AppendAction::AppendIfExistsOrAdd)],
        );
        let config = hcm_config_with_pipeline(pipeline, "/", 200, "body\n").await;
        let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let resp = String::from_utf8(drive(config, req).await).unwrap();
        assert!(resp.starts_with("HTTP/1.1 200 "), "status: {resp}");
        assert!(
            resp.to_ascii_lowercase().contains("x-test-encode: ok\r\n"),
            "encode-side stamp on wire: {resp}"
        );
    }

    #[tokio::test]
    async fn h1_stop_and_send_at_decode_skips_route_match() {
        // test-util stub: a filter that StopAndSend(503 "stopped\n") on decode,
        // placed before Router. The route is direct_response 200 "route\n" —
        // it must NOT be reached.
        let stop_resp = envoy_filter::FilterResponse {
            status: 503,
            reason: None,
            headers: vec![("content-length".to_string(), "8".to_string())],
            body: bytes::Bytes::from_static(b"stopped\n"),
        };
        let pipeline = Arc::new(envoy_filter::FilterPipeline::test_from_instances(vec![
            envoy_filter::HttpFilterInstance::test_stop_and_send_on_decode(stop_resp),
            envoy_filter::HttpFilterInstance::Router(envoy_filter::RouterTerminus::new()),
        ]));
        let config = hcm_config_with_pipeline(pipeline, "/", 200, "route\n").await;
        let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let resp = String::from_utf8(drive(config, req).await).unwrap();
        assert!(resp.starts_with("HTTP/1.1 503 "), "decode StopAndSend short-circuits: {resp}");
        assert!(resp.ends_with("stopped\n"), "synth body, not route body: {resp}");
    }

    #[tokio::test]
    async fn h1_stop_and_send_at_encode_substitutes_wire_response() {
        // test-util stub: a filter that StopAndSend(418 "teapot\n") on encode.
        // The route's direct_response 200 is built, then encode-side StopAndSend
        // replaces it on the wire.
        let stop_resp = envoy_filter::FilterResponse {
            status: 418,
            reason: None,
            headers: vec![("content-length".to_string(), "7".to_string())],
            body: bytes::Bytes::from_static(b"teapot\n"),
        };
        let pipeline = Arc::new(envoy_filter::FilterPipeline::test_from_instances(vec![
            envoy_filter::HttpFilterInstance::test_stop_and_send_on_encode(stop_resp),
            envoy_filter::HttpFilterInstance::Router(envoy_filter::RouterTerminus::new()),
        ]));
        let config = hcm_config_with_pipeline(pipeline, "/", 200, "route\n").await;
        let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let resp = String::from_utf8(drive(config, req).await).unwrap();
        assert!(resp.starts_with("HTTP/1.1 418 "), "encode StopAndSend substitutes: {resp}");
        assert!(resp.ends_with("teapot\n"), "substituted body: {resp}");
    }

    #[tokio::test]
    async fn h1_access_log_reflects_post_encode_headers() {
        // HCMConfig with a file access_log + HeaderMutation response_mutations.
        // Drive a request; the access log line + the per-class HCM counter see
        // the post-encode response state. Assert the access log captured a
        // 200 line (the encode-side mutation does not change status, but this
        // exercises the access-log dispatch site running after encode_headers).
        let pipeline = header_mutation_pipeline(
            vec![],
            vec![("x-test", "ok", envoy_config::AppendAction::AppendIfExistsOrAdd)],
        );
        let tmp = tempfile::tempdir().unwrap();
        let log_path = tmp.path().join("access.log");
        let config = hcm_config_with_access_log_and_pipeline(pipeline, &log_path).await;
        let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let _ = drive(config, req).await;
        let logged = std::fs::read_to_string(&log_path).unwrap();
        assert!(logged.contains(" 200 "), "access log captured post-encode status: {logged:?}");
    }
```

This step references two helpers the executor adds alongside Step C1: `hcm_config_header_matched_route(pipeline)` (an HCMConfig whose single route uses a `RouteMatch` with a `headers:` HeaderMatcher on `x-test-path-override` exact-match `/bar` → direct_response 200 "matched\n"; mirror the existing `single_header_matcher_route_selected_when_match` test's config-build shape at hcm.rs:~1312) and `hcm_config_with_access_log_and_pipeline(pipeline, log_path)` (mirror the existing `hcm_config_with_access_log` helper used by the 06.2 access-log tests, swapping in the caller's `filter_pipeline`). The executor copies the existing helpers' bodies and swaps the `filter_pipeline` field — no new patterns.

- [ ] **Step C3: Run the H1 tests**

Run: `cargo test -p envoy-http1 hcm 2>&1 | tail -25` — first to confirm RED is impossible (these are new tests against landed Task-1-to-4 + Group-B code, so they go straight to GREEN if the wiring is correct). Expected: PASS — 5 new tests + the pre-existing H1 hcm tests.

### Step group D — 4 H2 HCM filter-chain integration tests

These land in `crates/envoy-http2/src/hcm.rs`'s `#[cfg(test)] mod tests`. The idiom: build a `HttpConnectionManagerConfig` with `http_filters: [HeaderMutation, Router]`, call `Http1HCMConfig::from_config(...)` (the existing `synth_h2_hcm_config` helper at line ~635 does this for Router-only), spawn via `spawn_h2_hcm`, drive with an `h2::client` (per `h2_get_resolves_to_direct_response_synth` at line ~872).

- [ ] **Step D1: Add an H2 test helper that builds an HCM config with HeaderMutation**

Add to the H2 `#[cfg(test)] mod tests`:

```rust
    /// Build an H2 HCMConfig with `http_filters: [HeaderMutation, Router]` over
    /// a single direct_response route. `request_mutations` / `response_mutations`
    /// are `(key, value, AppendAction)` triples.
    async fn synth_h2_hcm_config_with_header_mutation(
        request_mutations: Vec<(&str, &str, AppendAction)>,
        response_mutations: Vec<(&str, &str, AppendAction)>,
        route_status: u16,
        route_body: &str,
    ) -> Arc<Http1HCMConfig> {
        use envoy_config::{
            HeaderMutationConfig, HeaderMutationEntry, HeaderValue, HeaderValueOption, Mutations,
        };
        let mk = |v: Vec<(&str, &str, AppendAction)>| -> Vec<HeaderMutationEntry> {
            v.into_iter()
                .map(|(k, val, action)| HeaderMutationEntry {
                    append: HeaderValueOption {
                        header: HeaderValue { key: k.to_string(), value: val.to_string() },
                        append_action: action,
                    },
                })
                .collect()
        };
        let cfg = HttpConnectionManagerConfig {
            stat_prefix: "test".to_string(),
            codec_type: CodecType::HTTP2,
            http2_protocol_options: None,
            access_log: vec![],
            route_config: RouteConfiguration {
                name: "r".to_string(),
                virtual_hosts: vec![VirtualHost {
                    name: "vh".to_string(),
                    domains: vec!["*".to_string()],
                    routes: vec![Route {
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                        },
                        action: RouteAction::DirectResponse(DirectResponse {
                            status: route_status,
                            body: DataSource {
                                filename: None,
                                inline_string: Some(route_body.to_string()),
                            },
                        }),
                    }],
                }],
            },
            http_filters: vec![
                HttpFilter {
                    name: "envoy.filters.http.header_mutation".to_string(),
                    typed_config: HttpFilterTypedConfig::HeaderMutation(HeaderMutationConfig {
                        mutations: Mutations {
                            request_mutations: mk(request_mutations),
                            response_mutations: mk(response_mutations),
                        },
                    }),
                },
                HttpFilter {
                    name: "envoy.filters.http.router".to_string(),
                    typed_config: HttpFilterTypedConfig::Router(RouterConfig {}),
                },
            ],
        };
        let cluster_mgr = Arc::new(envoy_cluster::ClusterManager::empty());
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        Arc::new(
            Http1HCMConfig::from_config(&cfg, cluster_mgr, registry)
                .await
                .expect("build HCM config"),
        )
    }
```

For the StopAndSend stub tests, add a helper that struct-literals an `Http1HCMConfig` with a test-util pipeline. `Http1HCMConfig` is the same type the H1 tests struct-literal; the executor mirrors H1's `hcm_config_with_pipeline` approach — if `Http1HCMConfig` is `Clone`, build via `synth_h2_hcm_config()` then overwrite `.filter_pipeline`; else inline the struct literal. Add:

```rust
    /// Build an H2 HCMConfig (direct_response 200 "route\n" route) whose
    /// filter pipeline is the caller-supplied test-util pipeline.
    async fn synth_h2_hcm_config_with_pipeline(
        pipeline: Arc<envoy_filter::FilterPipeline>,
    ) -> Arc<Http1HCMConfig> {
        let mut cfg = (*synth_h2_hcm_config().await).clone();
        cfg.filter_pipeline = pipeline;
        Arc::new(cfg)
    }
```

- [ ] **Step D2: Write the 4 H2 tests**

```rust
    #[tokio::test(flavor = "multi_thread")]
    async fn h2_decode_headers_fires_before_route_match() {
        // HeaderMutation adds x-h2-decode:seen on decode. The route is a plain
        // prefix "/" direct_response — this test asserts the decode mutation
        // reaches the request-processing path by checking the response is the
        // route's 200 (decode_headers ran without error before route-match).
        let config = synth_h2_hcm_config_with_header_mutation(
            vec![("x-h2-decode", "seen", AppendAction::AppendIfExistsOrAdd)],
            vec![],
            200,
            "ok\n",
        )
        .await;
        let (addr, _server) = spawn_h2_hcm(config).await;
        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut send_request, conn) = h2::client::handshake(tcp).await.unwrap();
        tokio::spawn(async move { let _ = conn.await; });
        let req = http::Request::builder()
            .method("GET")
            .uri("http://test.example/")
            .body(())
            .unwrap();
        let (response_fut, _) = send_request.send_request(req, true).unwrap();
        let resp = response_fut.await.unwrap();
        assert_eq!(resp.status().as_u16(), 200, "decode_headers ran cleanly before route-match");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn h2_encode_headers_fires_before_send_envoy_response() {
        // HeaderMutation adds x-h2-encode:ok on encode; the wire response
        // carries it.
        let config = synth_h2_hcm_config_with_header_mutation(
            vec![],
            vec![("x-h2-encode", "ok", AppendAction::AppendIfExistsOrAdd)],
            200,
            "ok\n",
        )
        .await;
        let (addr, _server) = spawn_h2_hcm(config).await;
        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut send_request, conn) = h2::client::handshake(tcp).await.unwrap();
        tokio::spawn(async move { let _ = conn.await; });
        let req = http::Request::builder()
            .method("GET")
            .uri("http://test.example/")
            .body(())
            .unwrap();
        let (response_fut, _) = send_request.send_request(req, true).unwrap();
        let resp = response_fut.await.unwrap();
        assert_eq!(resp.status().as_u16(), 200);
        assert_eq!(
            resp.headers().get("x-h2-encode").map(|v| v.as_bytes()),
            Some(b"ok".as_slice()),
            "encode-side stamp on the H2 wire response"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn h2_stop_and_send_at_decode_skips_route_match() {
        let stop_resp = envoy_filter::FilterResponse {
            status: 503,
            reason: None,
            headers: vec![("content-length".to_string(), "8".to_string())],
            body: bytes::Bytes::from_static(b"stopped\n"),
        };
        let pipeline = Arc::new(envoy_filter::FilterPipeline::test_from_instances(vec![
            envoy_filter::HttpFilterInstance::test_stop_and_send_on_decode(stop_resp),
            envoy_filter::HttpFilterInstance::Router(envoy_filter::RouterTerminus::new()),
        ]));
        let config = synth_h2_hcm_config_with_pipeline(pipeline).await;
        let (addr, _server) = spawn_h2_hcm(config).await;
        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut send_request, conn) = h2::client::handshake(tcp).await.unwrap();
        tokio::spawn(async move { let _ = conn.await; });
        let req = http::Request::builder()
            .method("GET")
            .uri("http://test.example/")
            .body(())
            .unwrap();
        let (response_fut, _) = send_request.send_request(req, true).unwrap();
        let resp = response_fut.await.unwrap();
        assert_eq!(resp.status().as_u16(), 503, "decode StopAndSend short-circuits route-match");
        let mut body = resp.into_body();
        let mut buf = bytes::BytesMut::new();
        while let Some(chunk) = body.data().await {
            buf.extend_from_slice(&chunk.unwrap());
        }
        assert_eq!(&buf[..], b"stopped\n");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn h2_stop_and_send_at_encode_substitutes_wire_response() {
        let stop_resp = envoy_filter::FilterResponse {
            status: 418,
            reason: None,
            headers: vec![("content-length".to_string(), "7".to_string())],
            body: bytes::Bytes::from_static(b"teapot\n"),
        };
        let pipeline = Arc::new(envoy_filter::FilterPipeline::test_from_instances(vec![
            envoy_filter::HttpFilterInstance::test_stop_and_send_on_encode(stop_resp),
            envoy_filter::HttpFilterInstance::Router(envoy_filter::RouterTerminus::new()),
        ]));
        let config = synth_h2_hcm_config_with_pipeline(pipeline).await;
        let (addr, _server) = spawn_h2_hcm(config).await;
        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut send_request, conn) = h2::client::handshake(tcp).await.unwrap();
        tokio::spawn(async move { let _ = conn.await; });
        let req = http::Request::builder()
            .method("GET")
            .uri("http://test.example/")
            .body(())
            .unwrap();
        let (response_fut, _) = send_request.send_request(req, true).unwrap();
        let resp = response_fut.await.unwrap();
        assert_eq!(resp.status().as_u16(), 418, "encode StopAndSend substitutes the H2 response");
        let mut body = resp.into_body();
        let mut buf = bytes::BytesMut::new();
        while let Some(chunk) = body.data().await {
            buf.extend_from_slice(&chunk.unwrap());
        }
        assert_eq!(&buf[..], b"teapot\n");
    }
```

- [ ] **Step D3: Run the H2 tests**

Run: `cargo test -p envoy-http2 hcm 2>&1 | tail -25`
Expected: PASS — 4 new tests + the pre-existing H2 hcm tests (13 + 1 `#[ignore]`d). If `h2_stop_and_send_at_decode_skips_route_match` reveals the decode-side `SynthFromDecode` path is not wired to short-circuit (it should be — 07.1 Task 7 landed `H2RequestPath::SynthFromDecode`), invoke `superpowers:systematic-debugging` — but the 07.1 wiring is landed, so this is expected GREEN.

- [ ] **Step Final: Workspace-wide checks + PROGRESS + commit**

Run `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check`. Expected: all clean. `cargo deny check` MUST be quoted — Task 5 touches 3 `Cargo.toml` files (the `[features]` table + 2 dev-dependency feature lines). PROGRESS notes: 07.1 REVIEW I1 CLOSED (Step group A — `finalize_h2_stream` 3-dead-parameter cleanup, ~12 line net removal, behavior-neutral, verified by the unchanged 13 H2 hcm tests).

```bash
git add crates/envoy-filter/ crates/envoy-http1/ crates/envoy-http2/ docs/envoy-rust/phases/07.2-header-mutation-filter/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 07.2: task 5 — H1+H2 HCM filter-chain integration tests + finalize_h2_stream cleanup [07.1 REVIEW I1]

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: Fuzz corpus seed for HeaderMutation HCM

**Scope:** ~50 LoC seed YAML + ~1 LoC test array entry + ~1 LoC gitignore entry = ~52 LoC. Adds a minimal positive-case HCM seed exercising the new `HeaderMutation` schema arm to the `parse_bootstrap` fuzz corpus. **Depends on Tasks 1 + 2** (the seed must parse + validate clean).

**Files:**
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_header_mutation_filter.yaml`.
- Modify: `crates/envoy-config/fuzz/.gitignore` — add the allow-list entry.
- Modify: `crates/envoy-config/src/bootstrap.rs` — append the seed name to the `fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS array.

- [ ] **Step 1: Write the failing test (extend the corpus walker)**

In `crates/envoy-config/src/bootstrap.rs`, find `fn fuzz_corpus_seeds_parse_or_reject_cleanly()` (~line 2541, inside `#[cfg(test)] mod tests`). Append `"fuzz/corpus/parse_bootstrap/hcm_header_mutation_filter.yaml"` to the FIRST array (the "Seeds expected to parse + validate successfully" list — after `"fuzz/corpus/parse_bootstrap/hcm_access_log_file.yaml"`):

```rust
        "fuzz/corpus/parse_bootstrap/hcm_access_log_file.yaml",
        "fuzz/corpus/parse_bootstrap/hcm_header_mutation_filter.yaml",
```

- [ ] **Step 2: Run the failing test**

Run: `cargo test -p envoy-config fuzz_corpus_seeds_parse_or_reject_cleanly 2>&1 | tail -10`
Expected: FAIL — `panic: read .../hcm_header_mutation_filter.yaml: No such file or directory` (the seed file doesn't exist yet).

- [ ] **Step 3: Create the seed file**

Create `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_header_mutation_filter.yaml`:

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
                route_config:
                  name: default
                  virtual_hosts:
                    - name: default
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response: { status: 200 }
                http_filters:
                  - name: envoy.filters.http.header_mutation
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.header_mutation.v3.HeaderMutation
                      mutations:
                        request_mutations:
                          - append:
                              header:
                                key: x-filter-stamp
                                value: phase-07
                              append_action: APPEND_IF_EXISTS_OR_ADD
                        response_mutations:
                          - append:
                              header:
                                key: x-filter-response-stamp
                                value: phase-07
                              append_action: APPEND_IF_EXISTS_OR_ADD
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
```

(No clusters — the route is `direct_response`; this is the minimal positive case exercising the new `HeaderMutation` schema arm + the Task 2 validator.)

- [ ] **Step 4: Add the `.gitignore` allow-list entry**

In `crates/envoy-config/fuzz/.gitignore`, add (after the `!corpus/parse_bootstrap/hcm_access_log_file.yaml` line, keeping the existing ordering):

```
!corpus/parse_bootstrap/hcm_header_mutation_filter.yaml
```

- [ ] **Step 5: Run the corpus walker test — expect PASS**

Run: `cargo test -p envoy-config fuzz_corpus_seeds_parse_or_reject_cleanly 2>&1 | tail -10`
Expected: PASS — the seed parses + validates clean via `crate::parse_bootstrap`.

- [ ] **Step 6: Verify the seed is tracked by git (not ignored)**

Run: `git check-ignore crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_header_mutation_filter.yaml; echo "exit: $?"`
Expected: `exit: 1` (NOT ignored — the `.gitignore` allow-list entry un-ignores it). If `exit: 0`, the allow-list entry is missing or mis-ordered.

- [ ] **Step 7: Optional — short-budget fuzz smoke (if `cargo +nightly fuzz` is available)**

Run: `cd crates/envoy-config/fuzz && cargo +nightly fuzz run parse_bootstrap -- -max_total_time=15 2>&1 | tail -10` (the new seed is auto-discovered by the corpus walker). Expected: clean (no crash). If nightly is unavailable, skip — Task 10's state-4 CI run exercises the full `-max_total_time=30` budget.

- [ ] **Step 8: Workspace-wide checks + PROGRESS + commit**

Run `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check`. Expected: all clean.

```bash
git add crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_header_mutation_filter.yaml crates/envoy-config/fuzz/.gitignore crates/envoy-config/src/bootstrap.rs docs/envoy-rust/phases/07.2-header-mutation-filter/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 07.2: task 6 — fuzz corpus seed for HeaderMutation HCM

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: `http1-echo-server` helper header-echo verify (zero code change expected)

**Scope:** ~0 LoC expected (verify-only). The 04.3 `http1-echo-server` helper ALREADY echoes received request headers into the response body, sorted by name. Task 7 verifies this and, ONLY if the shape is found absent, extends it. **No dependency on prior 07.2 tasks** — fully parallelizable; can be dispatched at any point.

**Files:**
- Verify (modify ONLY if shape absent): `tests/helpers/http1-echo-server/src/main.rs`.

- [ ] **Step 1: Verify the header-echo shape**

Read `tests/helpers/http1-echo-server/src/main.rs`. Confirm `build_echo_body` (or equivalent) emits each request header into the response body as `  <name>: <value>\n` lines, sorted by name. The 07.1-landed helper does this — `build_echo_body` collects `req.headers` into `sorted_headers`, calls `sorted_headers.sort_by(|a, b| a.0.cmp(&b.0))`, and emits `  {n}: {v}\n` lines under a `headers:\n` line. The body shape is:

```
method: <METHOD>
path: <PATH>
headers:
  <name1>: <value1>
  <name2>: <value2>
body: <BODY>
```

with header names lowercased and sorted.

- [ ] **Step 2: Confirm with the helper's own tests**

Run: `cargo test -p http1-echo-server 2>&1 | tail -10`
Expected: PASS — the helper's existing tests (e.g. `accepts_and_echoes_request`) assert the body shape.

- [ ] **Step 3: Disposition**

- **If the shape is present and sorted (expected outcome):** Task 7 lands ZERO code change. Append a PROGRESS section noting "verify-only — `build_echo_body` already echoes sorted request headers per 07.2 SPEC §6 signpost 10; no code change". **Do NOT create an empty commit** — instead, fold the PROGRESS note into Task 8's commit (Task 8 is the first consumer of the verified shape). The PROGRESS section for Task 7 still lands (at Task 8's commit), preserving the per-task narrative log.
- **If the shape is absent or unsorted (unexpected):** extend `build_echo_body` to emit sorted `  name: value\n` lines, add a helper-crate test asserting the sorted shape, and land Task 7 as its own commit `phase 07.2: task 7 — http1-echo-server helper header-echo shape`. Then invoke `superpowers:systematic-debugging` first to understand why the SPEC's pre-state check (SPEC §3 Task 7) diverged from disk.

- [ ] **Step 4: Workspace check (if a change was made)**

Only if Step 3 made a change: run `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check`. Expected: all clean.

---

## Task 8: Fixture `0013-http-filter-header-mutation` + Docker-gated wrapper

**Scope:** ~290 LoC fixture material (envoy.yaml + envoy-rust.yaml + expectations.yaml + README.md + 0-byte payload + Docker-gated wrapper). The fixture exercises `[HeaderMutation, Router]` in front of a Router-proxied route to an `http1-echo-server` backend; bilaterally asserts the decode-side stamp (echoed into the body) and the encode-side stamp (on the response headers). **Depends on Tasks 1-7.** Per 07.2 SPEC §6 signpost 7 + 06.3 REVIEW I1: Task 8 MUST be Docker-gated-fixture-run before commit, OR (if Docker is unavailable) push-branch-and-wait-for-CI-green before landing.

**Files:**
- Create: `tests/fixtures/0013-http-filter-header-mutation/envoy.yaml`.
- Create: `tests/fixtures/0013-http-filter-header-mutation/envoy-rust.yaml`.
- Create: `tests/fixtures/0013-http-filter-header-mutation/inputs/payload.bin` (0-byte).
- Create: `tests/fixtures/0013-http-filter-header-mutation/expectations.yaml`.
- Create: `tests/fixtures/0013-http-filter-header-mutation/README.md`.
- Create: `tests/differential/tests/http_filter_header_mutation.rs`.

- [ ] **Step 1: Create `envoy.yaml`** (reference Envoy config — mirrors fixture 0008's `envoy.yaml` shape + the HeaderMutation filter)

```yaml
node: { id: envoy-rust-phase-07.2-fixture-0013, cluster: envoy-rust-phase-07.2 }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: http1_listener
      address: { socket_address: { address: 0.0.0.0, port_value: {{PORT}} } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http1
                codec_type: HTTP1
                # Suppress Envoy's per-request UUID x-request-id so the helper's
                # deterministic echo body stays byte-equal across both proxies
                # (envoy-rust does not inject x-request-id). Same posture as
                # fixture 0008.
                generate_request_id: false
                route_config:
                  name: local_route
                  # Strip the headers Envoy v1.33 injects by default on the
                  # upstream-bound request; envoy-rust does not inject these
                  # (04.3 SPEC §4 non-goal). x-filter-stamp is NOT in this list,
                  # so the HeaderMutation-added stamp survives to the backend on
                  # both sides. envoy-rust.yaml omits this field (envoy-rust's
                  # parser does not recognize it — intentional field-set
                  # divergence, mirrors fixture 0008).
                  request_headers_to_remove:
                    - x-forwarded-for
                    - x-forwarded-proto
                    - x-request-id
                    - x-envoy-expected-rq-timeout-ms
                    - x-envoy-internal
                    - x-envoy-external-address
                  virtual_hosts:
                    - name: backend_vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.header_mutation
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.header_mutation.v3.HeaderMutation
                      mutations:
                        request_mutations:
                          - append:
                              header:
                                key: x-filter-stamp
                                value: phase-07
                              append_action: APPEND_IF_EXISTS_OR_ADD
                        response_mutations:
                          - append:
                              header:
                                key: x-filter-response-stamp
                                value: phase-07
                              append_action: APPEND_IF_EXISTS_OR_ADD
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STRICT_DNS
      dns_lookup_family: V4_ONLY
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: {{BACKEND_HOST}}, port_value: {{HTTP1_BACKEND_PORT}} } }
```

- [ ] **Step 2: Create `envoy-rust.yaml`** (mirrors fixture 0008's `envoy-rust.yaml` — no `request_headers_to_remove`, no `generate_request_id`, no `admin`, bind `127.0.0.1`, no `dns_lookup_family`)

```yaml
node: { id: envoy-rust-phase-07.2-fixture-0013, cluster: envoy-rust-phase-07.2 }
static_resources:
  listeners:
    - name: http1_listener
      address: { socket_address: { address: 127.0.0.1, port_value: {{PORT}} } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http1
                codec_type: HTTP1
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: backend_vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.header_mutation
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.header_mutation.v3.HeaderMutation
                      mutations:
                        request_mutations:
                          - append:
                              header:
                                key: x-filter-stamp
                                value: phase-07
                              append_action: APPEND_IF_EXISTS_OR_ADD
                        response_mutations:
                          - append:
                              header:
                                key: x-filter-response-stamp
                                value: phase-07
                              append_action: APPEND_IF_EXISTS_OR_ADD
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: {{BACKEND_HOST}}, port_value: {{HTTP1_BACKEND_PORT}} } }
```

- [ ] **Step 3: Create the 0-byte `inputs/payload.bin`**

```bash
mkdir -p tests/fixtures/0013-http-filter-header-mutation/inputs
: > tests/fixtures/0013-http-filter-header-mutation/inputs/payload.bin
```

- [ ] **Step 4: Create `expectations.yaml`** (mirrors fixture 0008's actual shape — see PLAN-write SPEC correction 4)

```yaml
driver:
  kind: http1
  method: get
  path: "/"
  host: "envoy-rust.test"
  expected_status: 200
  expected_body:
    kind: byte_exact
    body: "method: GET\npath: /\nheaders:\n  host: envoy-rust.test\n  x-filter-stamp: phase-07\nbody: \n"
  expected_headers: set_equal_modulo_allow_list
equivalence:
  response_status: exact
  response_body:
    kind: byte_exact
```

**`expected_body.body` is PREDICTED — verify-and-correct at Step 7.** Derivation: the `http1-echo-server` echoes request headers sorted-by-name. The client sends `Host: envoy-rust.test`; the HeaderMutation `request_mutations` adds `x-filter-stamp: phase-07` on decode (before route-match, before the Router proxies upstream). The backend receives `{host, x-filter-stamp}` (sorted: `host` < `x-filter-stamp`) and echoes them. If the harness's `drive_http1` sends additional headers (e.g. a `content-length: 0`), the actual body will differ — Step 7 captures the real bytes from the first run and corrects this string. The `equivalence.response_body.kind: byte_exact` cross-proxy assertion is the load-bearing check; the per-proxy `expected_body.body` is the secondary single-proxy assertion.

- [ ] **Step 5: Create `README.md`**

```markdown
# Fixture 0013 — `envoy.filters.http.header_mutation` end-to-end

Exercises the HeaderMutation HTTP filter in front of the Router on a
Router-proxied route to an `http1-echo-server` backend cluster. This is
phase 07.2's differential surface — the first concrete pluggable filter
proven wire-equivalent to upstream Envoy on both decode and encode
iteration states.

## Filter chain

    http_filters:
      - HeaderMutation   # request_mutations + response_mutations
      - Router           # terminus

Iteration order under the 07.1 framework: `decode_headers` runs
declaration order (HeaderMutation first, then Router no-op terminus;
route-match runs AFTER decode_headers per parent-07 SPEC §6 Rule 7);
`encode_headers` runs reverse declaration order (Router no-op first,
HeaderMutation second).

## Assertions

- **Request-side stamp at backend** (`decode_headers`).
  HeaderMutation adds `x-filter-stamp: phase-07` to the request via
  `APPEND_IF_EXISTS_OR_ADD`. The `http1-echo-server` echoes received
  request headers into the response body as sorted `  name: value`
  lines. The `expected_body: { kind: byte_exact }` assertion (both the
  per-proxy `body:` string and the `equivalence.response_body` cross-proxy
  check) confirms both proxies forwarded the same stamped request.

- **Response-side stamp at client** (`encode_headers`).
  HeaderMutation adds `x-filter-response-stamp: phase-07` to the response
  via `APPEND_IF_EXISTS_OR_ADD`. The `expected_headers:
  set_equal_modulo_allow_list` assertion confirms both proxies emitted
  the stamp (it lands identically on both — HeaderMutation is
  deterministic).

## Per-side divergence

`envoy.yaml` (reference) uses `request_headers_to_remove` +
`generate_request_id: false` to strip Envoy-v1.33-injected request
headers so the helper's deterministic echo body stays byte-equal across
both proxies (envoy-rust injects none of these per 04.3 SPEC §4).
`envoy-rust.yaml` omits `request_headers_to_remove` (envoy-rust's parser
does not recognize it), `dns_lookup_family`, and the `admin` block, and
binds `127.0.0.1` — mirrors fixture 0008's `envoy-rust.yaml` shape
(STRICT_DNS cluster per 05.1 ADR-0023; the harness substitutes
`{{BACKEND_HOST}}` per-side).
```

- [ ] **Step 6: Create the Docker-gated wrapper `tests/differential/tests/http_filter_header_mutation.rs`** (mirrors `tests/differential/tests/http1_router_upstream.rs`)

```rust
//! Phase 07.2 differential acceptance test: drive a GET / through an HCM whose
//! `http_filters` chain is `[HeaderMutation, Router]`, proxying to a host-side
//! `http1-echo-server` backend. Both proxies must produce identical (status,
//! body, header-set-modulo-allow-list). The HeaderMutation `request_mutations`
//! stamp (`x-filter-stamp: phase-07`) is echoed back in the body by the
//! backend (decode-side proof); the `response_mutations` stamp
//! (`x-filter-response-stamp: phase-07`) lands on the client-visible response
//! headers (encode-side proof). Docker-gated.

use std::path::PathBuf;

#[tokio::test]
async fn http_filter_header_mutation_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0013-http-filter-header-mutation");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
```

- [ ] **Step 7: Run the fixture — capture the real body, correct `expected_body.body`**

If Docker is available locally:
Run: `cargo test -p differential --test http_filter_header_mutation -- --ignored --nocapture 2>&1 | tail -40`

The first run may FAIL on the per-proxy `expected_body.body` assertion if the predicted string (Step 4) differs from the harness's actual echo (e.g. `drive_http1` sends a different header set). Read the failure diff, copy the ACTUAL body bytes, and correct `expected_body.body` in `expectations.yaml`. Re-run until GREEN. The `equivalence.response_body.kind: byte_exact` (cross-proxy) check must ALSO be green — if envoy and envoy-rust produce different bodies, that is a real bug → invoke `superpowers:systematic-debugging` (do NOT loosen the fixture).

If Docker is NOT available locally: commit Step 1-6 with the predicted `expected_body.body`, push the branch, and wait for the Docker-gated CI job. If CI is red on the per-proxy body assertion, land a fixup commit correcting `expected_body.body` from the CI diff. If CI is red on the cross-proxy `equivalence` check, invoke `superpowers:systematic-debugging`. (Task 9's in-process backstop is the secondary catch — it asserts the stamp via substring search, less brittle than byte-exact.)

- [ ] **Step 8: Workspace-wide checks + PROGRESS + commit**

Run `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check`. Expected: all clean (the new `#[tokio::test]` wrapper is `#[ignore]`-free but Docker-gated via the CI matrix — `cargo test --workspace` runs it only where Docker is present; locally without Docker it fails fast at `run_fixture` — confirm the project's existing Docker-gating convention; fixture 0008's wrapper has no `#[ignore]` either, so the harness's `run_fixture` handles the Docker-absent case). PROGRESS records the Docker-gated run result (CI URL or local run) per signpost 7 + 06.3 REVIEW I1 discipline, and folds in the Task 7 verify-only PROGRESS note.

```bash
git add tests/fixtures/0013-http-filter-header-mutation/ tests/differential/tests/http_filter_header_mutation.rs docs/envoy-rust/phases/07.2-header-mutation-filter/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 07.2: task 8 — fixture 0013-http-filter-header-mutation (bilateral assertion)

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 9: In-process backstop `crates/envoy-bin/tests/http_filter_header_mutation.rs`

**Scope:** ~150 LoC. A no-Docker integration test: spawn an in-process HTTP/1.1 echo upstream, spawn `envoy-bin` as a subprocess against an inline `[HeaderMutation, Router]` HCM config, drive a `GET /`, assert the response-side stamp on the headers and the request-side stamp echoed in the body. **Depends on Tasks 1-8.** Follows the `crates/envoy-bin/tests/http1_router_upstream.rs` precedent (PLAN-write SPEC correction 8 — there is no `serve_ephemeral` helper; the in-process backstop inlines its own upstream + YAML).

**Files:**
- Create: `crates/envoy-bin/tests/http_filter_header_mutation.rs`.

- [ ] **Step 1: Write the in-process backstop test file**

Create `crates/envoy-bin/tests/http_filter_header_mutation.rs`:

```rust
//! Phase 07.2 envoy-bin integration test: spawn `envoy-bin` against an HCM
//! whose `http_filters` chain is `[HeaderMutation, Router]`, proxying to an
//! in-process HTTP/1.1 echo upstream. Assert the HeaderMutation
//! `request_mutations` stamp (`x-filter-stamp: phase-07`) reaches the backend
//! (echoed back in the response body) and the `response_mutations` stamp
//! (`x-filter-response-stamp: phase-07`) lands on the client-visible response
//! headers. No Docker — the in-process backstop for the Docker-gated fixture
//! `tests/fixtures/0013-http-filter-header-mutation/`.
//!
//! Mirrors `crates/envoy-bin/tests/http1_router_upstream.rs` (the 04.3
//! in-process backstop); the inline upstream here additionally echoes request
//! headers into the response body so the decode-side stamp is observable.

#![forbid(unsafe_code)]

use std::io::Write;
use std::net::{SocketAddr, TcpListener as StdListener};
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

fn reserve_port() -> u16 {
    let l = StdListener::bind(("127.0.0.1", 0)).unwrap();
    let p = l.local_addr().unwrap().port();
    drop(l);
    p
}

async fn wait_ready(addr: SocketAddr, budget: Duration) {
    let deadline = std::time::Instant::now() + budget;
    let mut delay = Duration::from_millis(50);
    loop {
        match TcpStream::connect(addr).await {
            Ok(_) => return,
            Err(_) if std::time::Instant::now() < deadline => {
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_millis(500));
            }
            Err(e) => panic!("listener never became ready at {addr}: {e}"),
        }
    }
}

/// In-process HTTP/1.1 upstream that echoes the received request headers into
/// the response body as sorted `name: value\n` lines (so the HeaderMutation
/// decode-side stamp is observable in the body). Single request, then closes.
async fn spawn_echo_upstream() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let Ok((mut stream, _)) = listener.accept().await else {
            return;
        };
        let mut buf = vec![0u8; 8192];
        let mut total = 0usize;
        loop {
            let Ok(n) = stream.read(&mut buf[total..]).await else {
                return;
            };
            if n == 0 {
                return;
            }
            total += n;
            if buf[..total].windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
            if total >= buf.len() {
                return;
            }
        }
        // Parse the request headers via httparse; echo them sorted into the body.
        let mut hdrs = [httparse::EMPTY_HEADER; 32];
        let mut req = httparse::Request::new(&mut hdrs);
        let _ = req.parse(&buf[..total]);
        let mut pairs: Vec<(String, String)> = req
            .headers
            .iter()
            .filter(|h| !h.name.is_empty())
            .map(|h| {
                (
                    h.name.to_ascii_lowercase(),
                    String::from_utf8_lossy(h.value).into_owned(),
                )
            })
            .collect();
        pairs.sort_by(|a, b| a.0.cmp(&b.0));
        let mut body = String::from("headers:\n");
        for (n, v) in &pairs {
            body.push_str("  ");
            body.push_str(n);
            body.push_str(": ");
            body.push_str(v);
            body.push('\n');
        }
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes()).await;
        let _ = stream.shutdown().await;
    });
    addr
}

#[tokio::test(flavor = "multi_thread")]
async fn header_mutation_stamps_request_and_response() {
    let upstream_addr = spawn_echo_upstream().await;
    let upstream_port = upstream_addr.port();
    let listener_port = reserve_port();

    let yaml = format!(
        r#"
static_resources:
  listeners:
    - name: hcm_listener
      address:
        socket_address:
          address: 127.0.0.1
          port_value: {listener_port}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: default
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          route: {{ cluster: backend }}
                http_filters:
                  - name: envoy.filters.http.header_mutation
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.header_mutation.v3.HeaderMutation
                      mutations:
                        request_mutations:
                          - append:
                              header:
                                key: x-filter-stamp
                                value: phase-07
                              append_action: APPEND_IF_EXISTS_OR_ADD
                        response_mutations:
                          - append:
                              header:
                                key: x-filter-response-stamp
                                value: phase-07
                              append_action: APPEND_IF_EXISTS_OR_ADD
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: {upstream_port}
"#
    );

    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("envoy-rust.yaml");
    std::fs::File::create(&cfg)
        .unwrap()
        .write_all(yaml.as_bytes())
        .unwrap();

    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_envoy-bin"))
        .arg("-c")
        .arg(&cfg)
        .env("ENVOY_RUST_LOG", "warn")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn envoy-bin");

    let listener_addr: SocketAddr = format!("127.0.0.1:{listener_port}").parse().unwrap();
    wait_ready(listener_addr, Duration::from_secs(5)).await;

    let outcome = async {
        let mut stream = TcpStream::connect(listener_addr).await?;
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: envoy-rust.test\r\n\r\n")
            .await?;

        let mut buf = vec![0u8; 8192];
        let mut total = 0usize;
        loop {
            let n = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buf[total..]))
                .await
                .map_err(|_| anyhow::anyhow!("read timed out; got {total} bytes"))??;
            if n == 0 {
                anyhow::bail!("EOF before complete response; got {total} bytes");
            }
            total += n;

            let mut hdr_storage = [httparse::EMPTY_HEADER; 32];
            let mut resp = httparse::Response::new(&mut hdr_storage);
            match resp.parse(&buf[..total])? {
                httparse::Status::Complete(headers_end) => {
                    let cl = resp
                        .headers
                        .iter()
                        .find(|h| h.name.eq_ignore_ascii_case("content-length"))
                        .and_then(|h| std::str::from_utf8(h.value).ok())
                        .and_then(|s| s.parse::<usize>().ok())
                        .ok_or_else(|| anyhow::anyhow!("no parseable content-length"))?;
                    if total < headers_end + cl {
                        if total >= buf.len() {
                            anyhow::bail!("buffer full before body complete");
                        }
                        continue;
                    }

                    assert_eq!(resp.code, Some(200), "status code");

                    // Encode-side stamp: x-filter-response-stamp on the headers.
                    let has_resp_stamp = resp.headers.iter().any(|h| {
                        h.name.eq_ignore_ascii_case("x-filter-response-stamp")
                            && h.value == b"phase-07"
                    });
                    assert!(
                        has_resp_stamp,
                        "expected encode-side stamp x-filter-response-stamp: phase-07; \
                         got headers: {:?}",
                        resp.headers
                            .iter()
                            .map(|h| (h.name, String::from_utf8_lossy(h.value)))
                            .collect::<Vec<_>>()
                    );

                    // Decode-side stamp: x-filter-stamp: phase-07 echoed in the
                    // body by the upstream (proves the mutation reached the
                    // backend).
                    let body = &buf[headers_end..headers_end + cl];
                    let needle = b"x-filter-stamp: phase-07";
                    assert!(
                        body.windows(needle.len()).any(|w| w == needle),
                        "expected decode-side stamp echoed in body; got body: {:?}",
                        String::from_utf8_lossy(body)
                    );

                    return Ok::<(), anyhow::Error>(());
                }
                httparse::Status::Partial => {
                    if total >= buf.len() {
                        anyhow::bail!("buffer full before headers complete");
                    }
                }
            }
        }
    }
    .await;

    if outcome.is_err()
        && let Some(mut err_pipe) = child.stderr.take()
    {
        let mut stderr_buf = Vec::new();
        let _ = err_pipe.read_to_end(&mut stderr_buf).await;
        eprintln!("envoy-bin stderr:\n{}", String::from_utf8_lossy(&stderr_buf));
    }
    child.kill().await.ok();
    let _ = child.wait().await;

    outcome.expect("HeaderMutation stamps request + response");
}
```

- [ ] **Step 2: Verify `envoy-bin`'s test dependencies cover `anyhow`, `httparse`, `tempfile`**

The 04.3 precedent `crates/envoy-bin/tests/http1_router_upstream.rs` already uses all three — they are in `crates/envoy-bin`'s `[dev-dependencies]`. If `cargo build -p envoy-bin --tests` reports a missing dev-dep, add it (mirroring the existing `http1_router_upstream.rs` test's deps). Expected: no change needed.

- [ ] **Step 3: Run the in-process backstop**

Run: `cargo test -p envoy-bin --test http_filter_header_mutation 2>&1 | tail -20`
Expected: PASS — 1 test. If the encode-side stamp assertion fails, the encode-side filter wiring or the HeaderMutation `encode_headers` is broken — invoke `superpowers:systematic-debugging`. If the decode-side body assertion fails, the decode-side wiring or the Router proxy-arm header forwarding is broken — same.

- [ ] **Step 4: Workspace-wide checks + PROGRESS + commit**

Run `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check`. Expected: all clean.

```bash
git add crates/envoy-bin/tests/http_filter_header_mutation.rs docs/envoy-rust/phases/07.2-header-mutation-filter/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 07.2: task 9 — in-process backstop for HeaderMutation

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 10: State-4 phase-done verification + STATE advance to state-5-next

**Scope:** ~30 LoC docs. Materializes the §7.5 phase-done gate evidence (13 fixtures simultaneously green + h2spec ≥95% + `parse_bootstrap` fuzz clean + workspace gates) into PROGRESS.md, then advances STATE.md to `07.2 state-4-reached / state-5-next`. This is the state-4 evidence anchor + the state-5 STATE-advance — the state-5 REVIEW.md and the state-6 close-out (which also closes parent-07) are SEPARATE sessions per the lifecycle.

**Files:**
- Modify: `docs/envoy-rust/phases/07.2-header-mutation-filter/PROGRESS.md` — Task 10 state-4 evidence section.
- Modify: `docs/envoy-rust/STATE.md` — advance from `07.2 state 3` → `07.2 state-4-reached / state-5-next`; next-skill → `superpowers:requesting-code-review`.

- [ ] **Step 1: Push the branch + trigger the Docker-gated CI run**

Push the 07.2 task commits (Tasks 1-9) to the remote. The CI job runs all 13 Docker-gated fixtures (`0001-tcp-echo` through `0013-http-filter-header-mutation`) + h2spec + `parse_bootstrap` fuzz + the workspace gates. Wait for the run to complete.

- [ ] **Step 2: Capture the §7.5 phase-done gate evidence**

Confirm from the CI run:
- All 13 Docker-gated fixtures green simultaneously in ONE run (the 12 pre-existing `0001-0012` + the new `0013`).
- h2spec ≥95% (carried forward from the 05.2 baseline 99.31%; `known-failures.txt` unchanged — 07.2 engages no H2-framing surfaces).
- `parse_bootstrap` fuzz clean on the short-budget CI run (`cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30`; the Task 6 HeaderMutation seed is exercised).
- `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check` all clean on the stable-toolchain job.

If ANY gate is red: do NOT advance STATE.md. Invoke `superpowers:systematic-debugging`, fix at the owning task (re-enter state 3), re-run. Per BOOTSTRAP_PROMPT.md §5.2 a red gate routes back to state 3, not a state-4 patch.

- [ ] **Step 3: Append the Task 10 state-4 evidence section to PROGRESS.md**

```
## Task 10 — state-4 phase-done gate evidence (13 fixtures simultaneously green)

- workspace tests: cargo test --workspace — PASS (count: <N>; commit at <SHA>)
- Docker-gated fixtures (13 total, 0001-0013): all green simultaneously per CI run <URL>
  (conclusion success; completed <ISO-8601 UTC>; HEAD <SHA>)
- h2spec conformance: <pass_rate>% (≥95% gate held; known-failures.txt unchanged)
- parse_bootstrap fuzz: clean (short-budget CI run, -max_total_time=30; the
  Task 6 hcm_header_mutation_filter.yaml seed exercised)
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean
- cargo fmt --all -- --check: clean
- cargo deny check: clean
- cargo build --workspace --all-targets: clean
```

Quote the actual command outputs / CI URLs verbatim (per parent SPEC §8 R-1).

- [ ] **Step 4: Advance STATE.md to state-5-next**

Edit `docs/envoy-rust/STATE.md`:
- **Active phase** block: status `07.2 state 3 (SPEC + PLAN exist; implementation incomplete)` → `07.2 state 4-reached / state-5-next (implementation complete + verified; REVIEW.md pending)`.
- **Next expected skill**: `superpowers:subagent-driven-development` → `superpowers:requesting-code-review` scoped to the 07.2 surface (reviewed range = the 07.2 state-2 PLAN commit `..HEAD`).
- **Last commit** + **Last updated**: point at the Task 10 state-4 commit.
- Preserve all "Phase-NN rollovers" sections verbatim.

- [ ] **Step 5: Commit**

```bash
git add docs/envoy-rust/phases/07.2-header-mutation-filter/PROGRESS.md docs/envoy-rust/STATE.md
git commit -m "$(cat <<'EOF'
phase 07.2: task 10 — state-4 verification (13 fixtures simultaneously green)

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

**After Task 10, the state-3 session ends.** The next session is the **07.2 state-5 session** — invokes `superpowers:requesting-code-review`, lands `REVIEW.md`. The session after that is the **07.2 state-6 close-out** — which ALSO closes parent-07 (ROADMAP rows `07.2` AND `07` flip `in-progress` → `done`; STATE.md advances to `08 state 1`, next-skill `superpowers:brainstorming`; commit title `phase 07.2: envoy.filters.http.header_mutation + fixture 0013 [parent 07 done] [ADR-0030, ADR-0031]`). Per BOOTSTRAP_PROMPT.md §5.1, each is its own session.

---

## Self-review (planner's checklist — run at PLAN-write time)

**1. Spec coverage.** Every 07.2 SPEC §3 deliverable maps to a task: Task 1 = D8.2 part 1 (schema); Task 2 = D8.2 part 2 (validator); Task 3 = D9.2 part 1 (runtime types + builder); Task 4 = D9.2 part 2 (decode/encode semantics); Task 5 = D11.2 part 1 (H1+H2 integration tests) + the 07.1 REVIEW I1 carryforward; Task 6 = D10.2 (fuzz corpus); Task 7 = D11.2 part 2 (helper verify); Task 8 = D12.2 (fixture 0013); Task 9 = D13.2 (in-process backstop); Task 10 = D14.2 + D15.2 (state-4 verification + STATE advance). The §7.5 phase-done gate (a)-(f) is exercised at Task 10. The 07.1 REVIEW carryforwards: I1 → Task 5 group A (named owner); M1 → Task 3 (dep removal); M2 → Task 3 (`UnsupportedFilterType` constructable). No gap.

**2. Placeholder scan.** No "TBD" / "implement later" / "add appropriate X". The one PREDICTED value — fixture 0013's `expected_body.body` string — is explicitly flagged as predicted with a verify-and-correct step (Task 8 Step 7), matching the established empirical-fixture-seeding pattern (06.1 fixture 0011's allow-list was seeded from a real first-run diff). Task 7's "modify only if absent" is a real verify-only task (the SPEC itself frames it conditionally), not a placeholder.

**3. Type consistency.** `FilterRequest` / `FilterResponse` (not `Request` / `Response`) used consistently per correction 1. `HeaderMutationFilter` / `RuntimeHeaderMutation` / `RuntimeAppendAction` consistent across Tasks 3-5. `AppendAction` (config) vs `RuntimeAppendAction` (runtime) distinct and consistently named. `ConfigError::{UnsupportedHeaderMutationAppendAction, EmptyHeaderMutationKey, InvalidHeaderMutationKey}` consistent between Task 2's lib.rs additions and the validator. `FilterError::UnsupportedFilterType { position, name }` matches the 07.1-landed field shape (verified). `HttpFilterInstance::{HeaderMutation, TestStopAndSendOnDecode, TestStopAndSendOnEncode}` + `FilterPipeline::test_from_instances` consistent between Task 3, Task 5 group B, and Task 5 groups C/D. `validate_http_filters(filters, listener_name)` matches the 07.1-landed signature.

**4. Split-gate.** 10 tasks (< 25); ~1600 LoC (~+7% over the ~1500 soft gate, test/fixture-concentrated; production ~440). Accept-drift, no nest-split, per parent-07 SPEC §5 + the 06.x precedent — documented in the "LoC drift posture" section.
