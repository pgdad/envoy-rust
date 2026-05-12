# Phase 07.1 (`07.1-filter-framework-foundation`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended per the user's standing preference auto-memory `feedback_execution_style`) — fresh subagent per task + two-stage review. Steps use checkbox (`- [ ]`) syntax for tracking. Tasks land in numbered order; this PLAN.md commits ALONE at state-2 (no Task 1 PROGRESS preamble at state-2 — per 07.1 SPEC §8 the state-2 commit lands exactly PLAN.md + STATE.md advance + ROADMAP row flip; no PROGRESS.md is created at state-2 because §3 Task 1 is the envoy-filter crate scaffold landing at state-3, NOT a doc-preamble — divergence from the 06.1/06.2/06.3 cadence where Task 1 was the PROGRESS preamble). Tasks 1-9 each land as their own state-3 commit; the PROGRESS.md file is created at Task 1's commit.

**Goal.** Land the `envoy-filter` foundation crate (sole-dep-owner of HTTP filter-chain iteration logic — `FilterPipeline` + `Decision::{Continue, StopAndSend}` + `HttpFilterInstance::Router` + `RouterTerminus` + `FilterError`) and wire its iteration protocol into both H1 (`crates/envoy-http1/src/hcm.rs::serve_connection` — requires the load-bearing 5-writer-arm refactor so `encode_headers` runs AFTER the writer arm constructs the response but BEFORE the wire write) and H2 (`crates/envoy-http2/src/hcm.rs::handle_one_stream` + `finalize_h2_stream` — symmetric refactor placing `encode_headers` before `send_envoy_response`) HCM dispatch sites + the terminal-router validator at `crates/envoy-config/src/bootstrap.rs::validate_hcm` (relaxes the existing `MultipleHttpFilters` cardinality gate from `len != 1` to `len >= 1 AND Router-last AND no-duplicate-Router`; adds `ConfigError::EmptyHttpFilters` / `RouterNotTerminal` / `DuplicateRouterFilter` variants). **No new fixture lands in 07.1.** The framework's regression-equivalence under the existing Router-only chain is the differential surface — proven by all 12 pre-existing Docker-gated fixtures (`0001-tcp-echo` through `0012-access-log-file-sink`) staying green simultaneously at the state-4 phase-done gate.

**Architecture.** Hand-rolled per D-3.2 (D-3.2 lists *Filter chain engine* on its **Must be written from scratch** list — no `async_trait`, no `dyn`-dispatch indirection, no factory pattern, no runtime extension registration). Synchronous (non-async) iteration on the already-buffered request/response shape that 04.1 + 05.2 established — the framework consumes only `Vec<(String, String)>` headers + `Bytes` bodies, both fully buffered before iteration. New crate `crates/envoy-filter/` decomposed into 4 modules (`lib.rs` + `error.rs` + `pipeline.rs` + `instance.rs` + `router.rs`); depends on `envoy-config` (for `HttpFilter` config types) + `envoy-http1::codec` (for `Request` / `Response` value types — NOT `envoy-http1::hcm` to avoid cycle); consumed by both `envoy-http1::hcm` and `envoy-http2::hcm` (cross-crate stack: `envoy-config → envoy-filter → envoy-http1, envoy-http2 → envoy-bin`; no cycles per parent-07 SPEC §6 Rule 10). HCM holds `Arc<FilterPipeline>` (config-shared) + per-request `.clone()` into a working `FilterPipeline` (cheap at 07.1 since `RouterTerminus` is zero-state; structural for 07.2's HeaderMutation per-stream cloning).

**Tech Stack.** New permitted-foundations: NONE (07.1 introduces no new top-level Cargo deps under recommended posture per parent-07 SPEC §7 + 07.1 SPEC §7). New workspace member: `crates/envoy-filter/` (deps: `bytes = "1"`, `thiserror = "2"`, `tracing = "0.1"`, `envoy-config = { path = "../envoy-config" }`, `envoy-http1 = { path = "../envoy-http1" }` — all existing foundations / workspace crates). Modified workspace members: `envoy-config` (validator extension), `envoy-http1` (HCMConfig + serve_connection + router refactor), `envoy-http2` (HCMConfig consumption + finalize_h2_stream refactor + handle_one_stream wiring). No new fuzz target (existing `parse_bootstrap` corpus exercises the relaxed validator). No new fixture. `cargo deny check` is a no-op (no new top-level deps).

---

## Architecture decisions locked at PLAN-write time (signpost choices)

Per 07.1 SPEC §5's 14 implementation signposts, the planner picks the recommendation at PLAN-write time so the executor does not re-litigate mid-task. All 14 signposts (+ 4 additional decisions) lock here. Per the user's standing preference (auto-memory `feedback_pick_recommendation`), every signpost with a "recommended posture" gets that recommendation as the decision.

| # | Signpost | Decision | Rationale |
|---|---|---|---|
| 1 | Module decomposition at Task 1 | **Strict: Task 1 lands `lib.rs` + `error.rs` only. `pipeline.rs` lands at Task 2. `instance.rs` + `router.rs` land at Task 3.** Do NOT bundle. | SPEC §5 signpost 1. Keeps each task at ~100-120 LoC; preserves per-task TDD discipline. |
| 2 | `Decision::StopAndSend` variant scaffolded from day one | **Yes; ships at Task 2 with the `Decision` enum.** Even though no 07.1 filter emits it (Router is no-op terminus), the variant lands so the iteration loops have the structural shape for 07.2's HeaderMutation. | SPEC §5 signpost 2. Forward-compat scaffolding only — does not violate "Don't design for hypothetical future requirements" because 07.2 lands in the immediately-following sub-phase. |
| 3 | Pipeline mutability + Arc-clone shape | **`HCMConfig` holds `filter_pipeline: Arc<FilterPipeline>`** (Arc-shared at config-build time via `Arc::new(pipeline)`); each per-request scope at H1 (`serve_connection` per-request) and H2 (`handle_one_stream` per-stream) clones into a working `FilterPipeline` via `(*config.filter_pipeline).clone()`. At 07.1 the per-request clone is effectively a no-op (Router is zero-state); the clone shape is structural for 07.2's HeaderMutation per-stream cloning. **`FilterPipeline` derives `Clone`** (Task 2 implements). | SPEC §5 signpost 3. Mirrors the access-log `Vec<Arc<FileSink>>` shape from 06.2. |
| 4 | `outgoing: Http1Response` local at H1 unified site | **Declared `let mut outgoing: Http1Response;`** at the scope above the writer-arm match (lines ~378-388 area in `crates/envoy-http1/src/hcm.rs`). Each arm assigns to `outgoing`. After the match, the unified site runs `pipeline.encode_headers(&mut outgoing)`. The `outgoing` shape is `Http1Response` because the proxy-success arm's `construct_proxied_response` returns this type (Task 5 factoring point). | SPEC §5 signpost 4. |
| 5 | `let mut outgoing;` declaration discipline | **Uninitialized until match populates.** Per the 06.2 + 06.3 declaration discipline (`let mut x: T;` form, not `let mut x = default`). Catches accidental fall-through (compiler errors if any arm doesn't assign). Mirrors the H2 site's `let resp: envoy_http1::codec::Response;` discipline. | SPEC §5 signpost 5. |
| 6 | H2 `finalize_h2_stream` parameter threading | **New `pipeline: &mut FilterPipeline` parameter added to `finalize_h2_stream`.** Propagates from `handle_one_stream` per-stream scope. All 3 callers of `finalize_h2_stream` inside `crates/envoy-http2/src/hcm.rs` update at Task 7. | SPEC §5 signpost 6. |
| 7 | Cross-crate dep direction | **Stack: `envoy-config → envoy-filter → envoy-http1, envoy-http2 → envoy-bin`. No cycles.** `envoy-filter` depends on `envoy-config` (for `HttpFilter` config struct) + `envoy-http1` (for `codec::Request` / `codec::Response` value types — NOT `hcm` types). `envoy-http1` and `envoy-http2` both depend on `envoy-filter`. The cycle is avoided because the `codec` module within `envoy-http1` has no dependency on the `hcm` module. | SPEC §5 signpost 7 + parent-07 SPEC §6 Rule 10. |
| 8 | Validator `listener_name` parameter threading | **`validate_http_filters(filters: &[HttpFilter], listener_name: &str) -> Result<(), ConfigError>`.** The caller at `validate_hcm` threads the listener-name string from the outer `listeners` walk (which already knows the name). Mirrors the existing `Http2ClusterFromHttp1Listener { listener: String, cluster: String }` variant from 06.3's listener-name-threading. | SPEC §5 signpost 8. |
| 9 | Task-5-as-pure-refactor + Task-6-as-wiring split | **Task 5 lands the 5-writer-arm refactor ONLY (no filter invocation; `outgoing` is constructed, populated, written — but `encode_headers` is NOT yet invoked).** Task 6 lands the filter-invocation wiring ON TOP of Task 5's refactor. Task 5's commit is verifiable by in-process backstop tests + workspace tests + Docker-gated fixtures 0001-0012 (the regression-equivalence proof of the refactor alone); Task 6 then layers filter invocation onto a known-good base. | SPEC §5 signpost 9. Mirrors the 06.3 Task 4 + Task 5 split. |
| 10 | `RequestPath` / `H2RequestPath` private enums | **Per-HCM (NOT unified at framework level).** The local enum capturing `Match` (writer-arm path) vs `SynthFromDecode` (decode-side StopAndSend short-circuit) is private to each HCM module. The two shapes diverge enough that unification adds abstraction without payoff at the 07.1 surface. | SPEC §5 signpost 10. |
| 11 | No new fuzz target in 07.1 | **Existing `parse_bootstrap` corpus exercises the relaxed validator** via the existing `MultipleHttpFilters` / `UnsupportedHttpFilter` corpus seeds. The schema additions for HeaderMutation defer to 07.2 Task 4. | SPEC §5 signpost 11. |
| 12 | Test-only filter stub gating | **Defer Tasks 6+7 tests 3-7 to 07.2 Task 5** (the first task that wires HeaderMutation). 07.1's regression-equivalence proof at state-4 (all 12 existing fixtures green simultaneously) IS the no-behavior-regression test under the Router-only chain. At 07.1 scope: Tasks 6 + 7 land tests 1 + 2 + 8 (HCMConfig construction / error mapping + regression-equivalence) only. | SPEC §5 signpost 12. |
| 13 | Existing `MultipleHttpFilters` variant retention | **Keep; no longer constructed.** Per the ledger-discipline of `ConfigError` as a grow-only typed-error structure. Doc-comment on the variant notes the supersession. Existing tests that asserted the variant update to assert one of the new variants. | SPEC §5 signpost 13. |
| 14 | H1 `serve_connection` request-loop body shape after Task 6 | **`parse_request → clone_pipeline → decode_headers (→ short-circuit on StopAndSend) → build_response → match writer-arm → encode_headers → write_wire → access_log`.** The clone-per-request happens at the parse_request frontier; the early-return on `StopAndSend` from decode short-circuits to the unified site directly via the `RequestPath::SynthFromDecode` variant. | SPEC §5 signpost 14. |

**Additional decisions locked at PLAN-write time (not numbered signposts but worth recording):**

15. **`FilterPipeline` derives `Clone`.** Per signpost 3's per-request clone shape, `FilterPipeline` must be `Clone`. Implied by signpost 3; locked explicitly. The derive chain requires `HttpFilterInstance: Clone`, which requires `RouterTerminus: Clone` (auto-derived; `RouterTerminus` is `#[derive(Debug, Clone, Default)]` per Task 3's listing).

16. **`#![forbid(unsafe_code)]` at every new file's crate root** (just `crates/envoy-filter/src/lib.rs`; other modules are non-root files and inherit the crate-level attribute). Per D-3.8 + 4.1 invariant 8. No `unsafe` blocks introduced in 07.1.

17. **No new ADRs projected.** Per 07.1 SPEC §7 + parent-07 SPEC §7's recommended posture. The DECISIONS.md ledger head remains **ADR-0030** at every 07.1 task commit. Conditional ADR-0031 (foundations grant for `async_trait` or similar) stays available; lands only if an execution-time ambiguity surfaces per D-3.5. The synchronous non-async iteration shape per parent-07 SPEC §6 Rule 5 is designed to avoid `async_trait`.

18. **No empirical SPEC corrections projected at PLAN-write time.** Verified the SPEC's quoted line numbers against HEAD `0b3bff0`:
    - `crates/envoy-config/src/bootstrap.rs:420` — `pub http_filters: Vec<HttpFilter>` ✓.
    - `crates/envoy-config/src/bootstrap.rs:1335-1346` — existing cardinality gate ✓ (SPEC quotes `1335-1347`; empirical end-line is 1346; -1 line drift; negligible).
    - `crates/envoy-config/src/bootstrap.rs:444-453` — `HttpFilterTypedConfig::Router(RouterConfig)` + empty `RouterConfig {}` ✓.
    - `crates/envoy-http1/src/hcm.rs:246` — `async fn serve_connection` entry ✓.
    - `crates/envoy-http1/src/hcm.rs:319` — `build_response(&config, &req, close)` call site ✓.
    - `crates/envoy-http1/src/hcm.rs:464` — `crate::router::write_proxied_response(...)` call site ✓.
    - `crates/envoy-http1/src/router.rs:75` — `pub async fn write_proxied_response<W>(...)` signature ✓ (already has `cluster: &ClusterHandle` param per the 06.3 Task 7 landing; the SPEC's mention of "extend write_proxied_response signature with cluster: &ClusterHandle parameter" refers to the EXISTING 06.3 signature, NOT a new change).
    - `crates/envoy-http2/src/hcm.rs:88` — `async fn handle_one_stream(...)` ✓.
    - `crates/envoy-http2/src/hcm.rs:127` — `build_response(&config, &envoy_req, /* close = */ false)` ✓.
    - `crates/envoy-http2/src/hcm.rs:365` — `async fn finalize_h2_stream(...)` ✓.

    Any execution-time empirical correction surfaces in Task 1's PROGRESS preamble per the 06.1 PROGRESS Task 1 "PLAN-write SPEC corrections" pattern (recommended posture per the next-prompt).

---

## LoC drift posture (per BOOTSTRAP_PROMPT.md §6.1 + parent-07 SPEC §5 alternative (iv))

07.1 SPEC §3 projects ~1110 LoC code + tests + docs across 9 tasks. Task-count projection: 9 tasks. Both projections are comfortably under the §6.1 split-gate (~25 tasks or ~1500 LoC of net change).

Per parent-07 SPEC §5 alternative (iv), 07.1 may NOT nest-split itself even if execution-time drift pushes a task over its task-local budget — the accept-drift posture is the established release valve. The 06.1 / 06.2 / 06.3 precedent ratifies this: 06.1 SPEC projected ~1300 LoC and PLAN landed ~2010 LoC of net change; 06.2 SPEC projected ~1300 LoC and PLAN landed ~1875 LoC; 06.3 SPEC projected ~850 LoC. All three honored the no-nest-split posture and absorbed the PLAN-vs-SPEC narrative-density growth without re-splitting.

If execution-time experience at Task 5 (the H1 5-writer-arm refactor, the load-bearing complexity contributor) shows further inflation, the recommended in-execution release valve is per-arm commit splitting (Task 5a = synth arms; Task 5b = proxy-success arm + factoring of `write_proxied_response`) recorded in PROGRESS rather than nest-splitting at the phase level.

**LoC ground-truth checkpoints (recorded in Task 1 PROGRESS preamble + state-4 verification at Task 8):**
- Per 07.1 SPEC §3 task budgets (Tasks 1+2+3+4+5+6+7+8+9): ~100 + ~120 + ~110 + ~220 + ~210 + ~100 + ~170 + ~30 + ~50 = ~1110 LoC across the substantive surface.
- PLAN.md + PROGRESS.md narrative overhead projects ~2500-3500 lines (per 06.1/06.2/06.3 precedent of ~3500-4000 line PLAN+PROGRESS bundles).

---

## Task summary

9 substantive tasks total. All 9 land at state-3, each as their own commit. No Task 1 PROGRESS preamble lands at state-2 (divergence from 06.x cadence — see PLAN top header). PROGRESS.md is CREATED at Task 1's state-3 commit alongside the envoy-filter crate scaffold.

| # | Title | Scope (LoC) | Carryforwards closed |
|---|---|---|---|
| 1 | `crates/envoy-filter/` scaffold + `FilterError` typed-error enum | ~100 | — |
| 2 | `FilterPipeline` + `Decision` enum + iteration-loop skeleton | ~120 | — |
| 3 | `HttpFilterInstance` enum (Router-only) + `RouterTerminus` | ~110 | — |
| 4 | `envoy-config` validator relaxation + 3 new `ConfigError` variants | ~220 | — |
| 5 | H1 HCM 5-writer-arm refactor (factor wire-write to unified site) | ~210 | — |
| 6 | H1 HCM filter-chain decode/encode invocation | ~100 | — |
| 7 | H2 HCM `finalize_h2_stream` refactor + filter-chain invocation | ~170 | — |
| 8 | State-4 verification (12 fixtures simultaneously green) | ~30 doc | **06.3 REVIEW I1** (verification-discipline gap — per-task PROGRESS test-bucket attestation discipline applied uniformly at Tasks 5/6/7) |
| 9 | State-4 PROGRESS materialization + STATE advance | ~50 doc | — |

**Total projected:** ~725 LoC code + ~305 LoC tests + ~80 LoC docs = ~1110 LoC; well under split-gate thresholds.

**Sequencing rationale (recommended execution order 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9):**

- Tasks 1-3 are the framework crate (lib.rs + error.rs + pipeline.rs + instance.rs + router.rs). Strict module-per-task split per signpost 1. Each task is self-contained; downstream tasks depend only on earlier tasks' public types.
- Task 4 is the validator relaxation. Lands AFTER the framework crate so the validator's logic mirrors the `FilterPipeline::build_from_config` invariants (defense-in-depth at two layers; planner cross-validates by inspection).
- Task 5 is the load-bearing pure refactor (5-writer-arm factoring at H1 HCM). NO filter invocation yet — preserves wire-emitted behavior bit-equivalent under the Router-only chain. Verifiable by in-process backstop tests + workspace tests at Task 5's commit.
- Task 6 layers filter invocation onto Task 5's refactor at H1. HCMConfig gains `filter_pipeline: Arc<FilterPipeline>` field; serve_connection invokes `pipeline.decode_headers` after parse_request and before build_response; the unified factored site invokes `pipeline.encode_headers` before the wire write.
- Task 7 mirrors Task 6 at H2 — HCM type-aliased so `filter_pipeline` is already present; `finalize_h2_stream` signature gets the `pipeline: &mut FilterPipeline` parameter; encode-side invocation lands inline at finalize_h2_stream; decode-side invocation lands at handle_one_stream after `http_to_envoy_request` translation.
- Task 8 is state-4 verification — push to CI, capture the 12-fixture green CI run URL + h2spec ≥95% + parse_bootstrap fuzz clean evidence in PROGRESS.
- Task 9 advances STATE.md from `07.1` state 3 → state-4-reached / state-5-next; next-skill `superpowers:requesting-code-review`.

---

## File structure overview

### Created (new files)

- **`crates/envoy-filter/Cargo.toml`** (Task 1) — new workspace member manifest.
- **`crates/envoy-filter/src/lib.rs`** (Task 1) — crate root with `#![forbid(unsafe_code)]`, module declarations, public re-exports.
- **`crates/envoy-filter/src/error.rs`** (Task 1) — `FilterError` typed-error enum.
- **`crates/envoy-filter/src/pipeline.rs`** (Task 2) — `FilterPipeline` struct + `Decision` enum + iteration methods.
- **`crates/envoy-filter/src/instance.rs`** (Task 3) — `HttpFilterInstance` enum (Router-only at 07.1) + `HttpFilterInstance::build` constructor.
- **`crates/envoy-filter/src/router.rs`** (Task 3) — `RouterTerminus` no-op terminus filter.
- **`docs/envoy-rust/phases/07.1-filter-framework-foundation/PROGRESS.md`** (Task 1) — per-task narrative log. Task 1 entry includes PLAN-write SPEC corrections (if any surface), LoC budget tracking, signpost decision recap.

### Modified

- **`Cargo.toml`** (Task 1; workspace root) — append `crates/envoy-filter` to `[workspace] members` (insert alphabetically after `envoy-config`, before `envoy-http1`).
- **`crates/envoy-config/src/bootstrap.rs`** (Task 4) — replace the cardinality gate at lines 1335-1346 with a `validate_http_filters(&hcm.http_filters, listener_name)?` call; add new free function `validate_http_filters`; add doc-comment supersession note on `MultipleHttpFilters`.
- **`crates/envoy-config/src/lib.rs`** (Task 4) — append `EmptyHttpFilters { listener: String }` + `RouterNotTerminal { listener: String, position: usize }` + `DuplicateRouterFilter { listener: String }` variants to `ConfigError`. Add doc-comment to `MultipleHttpFilters` noting supersession.
- **`crates/envoy-http1/src/hcm.rs`** (Tasks 5 + 6) —
  - **Task 5 (refactor):** Add `let mut outgoing: Http1Response;` declaration at the scope above the writer-arm match (line ~318 area). Convert each of the 5 writer arms (synth direct_response; proxy success; proxy send-fail 502; proxy connect-fail 502; proxy no-endpoint 503) from inline-wire-write to `outgoing = <constructed_response>` + `upstream_host_for_log = ...`. Lift the wire-write `Http1Response::write_to(&outgoing, &mut downstream).await?` to the unified factored site below the match. Re-populate `response_status_for_log` / `response_body_len` / `response_headers_for_log` from `outgoing` at the unified site (these moves are bit-equivalent since no filter has mutated `outgoing` yet at Task 5).
  - **Task 6 (wiring):** Add `pub filter_pipeline: Arc<FilterPipeline>` field to `HCMConfig`. Add `HCMConfigError::FilterPipeline(envoy_filter::FilterError)` variant via `#[from]`. Add `HCMConfig::from_config` call to `FilterPipeline::build_from_config(&cfg.http_filters)` + wrap in `Arc::new(...)`. Add private `RequestPath { Match(BuildOutcome), SynthFromDecode(Http1Response) }` enum at module scope. At `serve_connection` per-request loop: after `parse_request` succeeds + before `build_response`, clone the pipeline via `let mut pipeline = (*config.filter_pipeline).clone();` + invoke `pipeline.decode_headers(&mut req)`; on `Continue` set `let outcome = RequestPath::Match(build_response(...))`; on `StopAndSend(resp)` set `let outcome = RequestPath::SynthFromDecode(resp)`. At the unified factored site established by Task 5: read `outcome` to populate `outgoing` (Match arm runs the writer-arm match; SynthFromDecode arm assigns directly); invoke `pipeline.encode_headers(&mut outgoing)` between writer-arm match end and the wire write; on `StopAndSend(replacement)` substitute `outgoing = replacement`.
- **`crates/envoy-http1/src/router.rs`** (Task 5) — factor `write_proxied_response` into `construct_proxied_response` (same body minus the wire-write step; returns `Http1Response` value); update or remove `write_proxied_response` itself (no callers outside hcm.rs after Task 5 — fully removable). The existing `cluster: &ClusterHandle` parameter from 06.3 carries through unchanged.
- **`crates/envoy-http2/src/hcm.rs`** (Task 7) — Add private `H2RequestPath { Match(BuildOutcome), SynthFromDecode(Response) }` enum at module scope. At `handle_one_stream`: after `http_to_envoy_request` translates the H2 HEADERS frame + before `build_response(&config, &envoy_req, /* close = */ false)`, clone the pipeline + invoke `pipeline.decode_headers(&mut envoy_req)`; on `Continue` proceed to `build_response`; on `StopAndSend(resp)` short-circuit to `finalize_h2_stream` with the synth response. Add `pipeline: &mut FilterPipeline` parameter to `finalize_h2_stream`; threaded from `handle_one_stream`. Inside `finalize_h2_stream`, before the existing per-class HCM counter site at line ~380: invoke `pipeline.encode_headers(&mut resp)`; on `StopAndSend(replacement)` substitute `resp = replacement`. All 3 callers of `finalize_h2_stream` (inside `crates/envoy-http2/src/hcm.rs`) update to pass `&mut pipeline`.
- **`docs/envoy-rust/STATE.md`** (Task 9) — advance from `07.1 state 3` → `07.1 state-4-reached / state-5-next`; next-skill `superpowers:requesting-code-review`.
- **`docs/envoy-rust/ROADMAP.md`** — **flip row `07.1` `status: planned` → `status: in-progress` at THIS state-2 commit** (per BOOTSTRAP_PROMPT.md §4.1 invariant 3 + 04.1 `c02eea7` / 05.1 `bfabcb6` / 06.1 / 06.2 / 06.3 precedent: a phase enters `in-progress` only when STATE.md points at it; STATE.md now points at phase 07.1 with lifecycle state 3 = implementation, so the row flips at the standalone-PLAN commit).

### Deleted

None.

---

## Conventions

Mirrors 06.1 / 06.2 / 06.3 PLAN conventions:

- **TDD shape per task:** Step 1 writes the failing test(s); Step 2 runs them (FAIL expected; quote output); Step 3 writes the minimal implementation; Step 4 runs the tests (PASS expected; quote output); Step 5+ may layer additional verification (clippy / fmt); final step commits.
- **Commit messages:** `phase 07.1: task N — <task summary>`; mirrors 06.3 cadence (e.g., `phase 06.3: comprehensive stats wiring + 05.3 I1 closure + parent-06 close [parent 06 done] [ADR-0029]`). Co-Authored-By trailer: `Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>`.
- **PROGRESS.md per-task append:** every substantive task commit appends a per-task section to PROGRESS.md narrating: work summary, tests landed (names + LoC tally), per-task deviations from PLAN (per D-3.5 append-only discipline), LoC delta, test-bucket attestation (workspace tests + clippy + fmt + deny + Docker-gated fixtures when relevant). Closes 06.3 REVIEW I1 (verification-discipline gap — per-task PROGRESS test-bucket attestation) by uniform discipline at Tasks 5/6/7 specifically.
- **`#![forbid(unsafe_code)]`:** unchanged at every modified crate's `lib.rs`. The new `crates/envoy-filter/src/lib.rs` starts with this attribute per Task 1.
- **No new top-level Cargo deps.** Each task's `Cargo.lock` diff should be ≤5 lines (workspace-internal path-dep refresh on the new `envoy-filter` crate at Task 1; nothing thereafter unless an unforeseen ADR surfaces).
- **`cargo fmt --all` + `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean at every per-task commit** — per 06.1 REVIEW §7 recommendation 9 (the per-task fmt discipline). Each task's PROGRESS section explicitly attests "fmt clean / clippy clean".
- **Bilateral attestation discipline at Tasks 5/6/7:** the PROGRESS entry for each code-changing task in the H1/H2 HCM dispatch surface enumerates which buckets ran: workspace tests, clippy, fmt, deny check, AND a notation of which Docker-gated fixtures were exercised at in-process-backstop level. The Docker-gated bilateral fixture runs themselves anchor at Task 8's CI run; per-task PROGRESS notes the in-process surrogate.
- **Stat names / error variants use Envoy's documented snake_case-with-dots verbatim** where applicable. `ConfigError::EmptyHttpFilters` / `RouterNotTerminal` / `DuplicateRouterFilter` follow the existing `ConfigError` naming convention (no transform).

---

## State-2 commit (this commit's content; lands BEFORE any Task 1-9 commit)

The state-2 commit lands exactly 3 files modified + 1 file created:

- **CREATE:** `docs/envoy-rust/phases/07.1-filter-framework-foundation/PLAN.md` (this file).
- **MODIFY:** `docs/envoy-rust/STATE.md` — advance from `07.1` lifecycle state 2 / next-skill `superpowers:writing-plans` → `07.1` lifecycle state 3 / next-skill `superpowers:subagent-driven-development` (per the user's standing preference auto-memory `feedback_execution_style` — do NOT present the inline-`executing-plans` fork at state-3 entry). Update the "Active phase" block + "Next expected skill" block + "Last commit" block + "Last updated" timestamp. Strip the "Phase-07 state-2 split decision summary" section (still relevant at parent-07 state-2 but the per-sub-phase summary will be rebuilt at 07.1's own state-6 commit). Preserve all "Phase-NN rollovers" sections verbatim.
- **MODIFY:** `docs/envoy-rust/ROADMAP.md` — flip row `07.1` from `status: planned` to `status: in-progress` (single-cell edit; per ROADMAP-schema invariant 3 + 04.1 / 05.1 / 06.1 / 06.2 / 06.3 precedent).
- **MODIFY (no edit):** `docs/envoy-rust/DECISIONS.md` — UNCHANGED (no new ADR projected per 07.1 SPEC §7 + parent-07 SPEC §7's recommended posture). Ledger head remains **ADR-0030**.
- **MODIFY (no edit):** `docs/envoy-rust/phases/07.1-filter-framework-foundation/SPEC.md` — UNCHANGED (SPEC is the input artifact per 07.1 SPEC §8; any empirical correction defers to Task 1's PROGRESS preamble per the next-prompt recommended posture).

**Commit message (verbatim format per BOOTSTRAP_PROMPT.md §5.3 + 06.3 `3a964cc` shape):**

```
phase 07.1: state-2 standalone PLAN.md (9 tasks; ~1110 LoC projected)

Per BOOTSTRAP_PROMPT.md §5 state 2 + SKILL_ROUTING.md line 21 + the
established standalone-pre-Task-1-PLAN cadence (06.1 505653d, 06.2
dc00750, 06.3 3a964cc — parent-07's first sub-phase state-2 commit
mirrors the same shape). PLAN.md materializes 07.1 SPEC §3's 9 tasks
into per-step TDD checklists. Recommended execution order
1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9: Tasks 1-3 build the envoy-filter
foundation crate (lib + error + pipeline + instance + router; ~330 LoC);
Task 4 relaxes the envoy-config terminal-router validator (~220 LoC);
Task 5 lands the H1 5-writer-arm pure refactor (~210 LoC; no filter
invocation yet); Task 6 layers H1 filter-chain decode/encode invocation
onto Task 5's refactor (~100 LoC); Task 7 mirrors at H2 with the
finalize_h2_stream parameter-threading refactor (~170 LoC); Tasks 8-9
materialize the state-4 12-fixture-simultaneously-green evidence and
advance STATE.md to state-5-next (~80 LoC docs).

Flips ROADMAP row 07.1 status: planned → in-progress. Advances STATE.md
to 07.1 lifecycle state 3; next-skill superpowers:subagent-driven-
development scoped to PLAN Task 1 (envoy-filter crate scaffold +
FilterError typed-error enum). Parent row 07 stays in-progress (flips
to done only at 07.2's state-6 close-out per ROADMAP-schema invariant).
DECISIONS.md ledger head unchanged at ADR-0030.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```

No `Differential surface` / `Conformance` lines per BOOTSTRAP_PROMPT.md §5.3 (those belong to state-6 commits; state-2 commits are doc-only).

---

## Task 1: `crates/envoy-filter/` scaffold + `FilterError` typed-error enum

**Scope:** ~100 LoC. Lands the new workspace member `crates/envoy-filter/` with `lib.rs` + `error.rs` only (no `pipeline.rs` / `instance.rs` / `router.rs` yet — those land at Tasks 2 + 3 per signpost 1's strict module decomposition). Establishes `FilterError` typed-error enum + module declarations + public re-exports. ALSO creates `PROGRESS.md` at this commit (Task 1 PROGRESS entry).

**Files:**
- Create: `crates/envoy-filter/Cargo.toml` (~12 LoC).
- Create: `crates/envoy-filter/src/lib.rs` (~25 LoC).
- Create: `crates/envoy-filter/src/error.rs` (~60 LoC including tests).
- Modify: `Cargo.toml` (workspace root) — append `crates/envoy-filter` to `[workspace] members`.
- Create: `docs/envoy-rust/phases/07.1-filter-framework-foundation/PROGRESS.md` (~40 LoC initial preamble + Task 1 entry).

- [ ] **Step 1: Create `crates/envoy-filter/Cargo.toml`**

```toml
[package]
name = "envoy-filter"
version = "0.1.0"
edition = "2021"
publish = false

[dependencies]
bytes = "1"
thiserror = "2"
tracing = "0.1"
envoy-config = { path = "../envoy-config" }
envoy-http1 = { path = "../envoy-http1" }

[dev-dependencies]
```

Rationale: all deps are existing workspace foundations (`bytes`, `thiserror`, `tracing`) or workspace members (`envoy-config`, `envoy-http1`). No new top-level Cargo deps. Tracks the project-wide edition `2021` per existing crates.

- [ ] **Step 2: Add `crates/envoy-filter` to workspace members**

Edit `Cargo.toml` (workspace root) `[workspace] members` array — insert `"crates/envoy-filter"` alphabetically between `"crates/envoy-config"` and `"crates/envoy-http1"`:

```toml
[workspace]
resolver = "2"
members = [
    "crates/envoy-accesslog",
    "crates/envoy-admin",
    "crates/envoy-bin",
    "crates/envoy-cluster",
    "crates/envoy-config",
    "crates/envoy-filter",
    "crates/envoy-http1",
    "crates/envoy-http2",
    "crates/envoy-listener",
    "crates/envoy-stats",
    "crates/envoy-tcp",
    "crates/envoy-tls",
    "tests/conformance/h2spec",
    "tests/differential",
    "tests/helpers/http1-echo-server",
    "tests/helpers/http2-echo-server",
    "tests/helpers/tcp-echo-server",
    "tests/helpers/tls-echo-server",
]
exclude = [
    "crates/envoy-config/fuzz",
]
```

- [ ] **Step 3: Create `crates/envoy-filter/src/lib.rs`**

```rust
#![forbid(unsafe_code)]

//! HTTP filter chain iteration protocol.
//!
//! Hand-rolled per D-3.2's "Must be written from scratch" doctrine for
//! filter chain engines. Synchronous (non-async) iteration on the
//! already-buffered request/response shape established by 04.1 + 05.2.

pub mod error;
pub mod instance;
pub mod pipeline;
pub mod router;

pub use error::FilterError;
pub use instance::HttpFilterInstance;
pub use pipeline::{Decision, FilterPipeline};
```

**Note on module declarations at Task 1.** This file declares all four future modules (`error`, `instance`, `pipeline`, `router`) but only `error` HAS a corresponding `.rs` file at Task 1's commit. **This will not compile yet** — the `pub mod instance;` / `pub mod pipeline;` / `pub mod router;` lines reference files that don't exist. **Resolution: at Task 1, declare only the modules that have files.** Use this Task 1-scoped `lib.rs` instead:

```rust
#![forbid(unsafe_code)]

//! HTTP filter chain iteration protocol.
//!
//! Hand-rolled per D-3.2's "Must be written from scratch" doctrine for
//! filter chain engines. Synchronous (non-async) iteration on the
//! already-buffered request/response shape established by 04.1 + 05.2.

pub mod error;

pub use error::FilterError;
```

The remaining `pub mod` lines + re-exports land at Task 2 (`pipeline`) and Task 3 (`instance`, `router`). Each task extends `lib.rs` accordingly.

- [ ] **Step 4: Create `crates/envoy-filter/src/error.rs` with the typed-error enum + tests**

```rust
//! Typed errors emitted by the filter framework.
//!
//! Most parse-time validation lives in `envoy_config::validate_http_filters`
//! (Task 4). `FilterError` exists for the residual cases where the
//! framework's `build_from_config` arm asserts an invariant the
//! validator would also catch (defense-in-depth) plus future runtime
//! errors (e.g., `StopAndSend` invariants).

use thiserror::Error;

#[derive(Debug, Error)]
pub enum FilterError {
    #[error("filter chain is empty (must contain at least Router)")]
    EmptyChain,

    #[error(
        "expected Router at terminus position {expected}, got filter named {actual:?} at position {position}"
    )]
    RouterNotTerminal {
        actual: String,
        position: usize,
        expected: usize,
    },

    #[error("filter chain contains duplicate Router at position {position}")]
    DuplicateRouter { position: usize },

    #[error("filter chain references unsupported filter type at position {position}: {name}")]
    UnsupportedFilterType { position: usize, name: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_empty_chain_is_human_readable() {
        let s = format!("{}", FilterError::EmptyChain);
        assert_eq!(s, "filter chain is empty (must contain at least Router)");
    }

    #[test]
    fn display_router_not_terminal_includes_position_and_name() {
        let e = FilterError::RouterNotTerminal {
            actual: "envoy.filters.http.fault".to_string(),
            position: 0,
            expected: 1,
        };
        let s = format!("{e}");
        assert!(s.contains("expected Router at terminus position 1"));
        assert!(s.contains("envoy.filters.http.fault"));
        assert!(s.contains("position 0"));
    }

    #[test]
    fn display_duplicate_router_includes_position() {
        let s = format!("{}", FilterError::DuplicateRouter { position: 2 });
        assert_eq!(s, "filter chain contains duplicate Router at position 2");
    }

    #[test]
    fn display_unsupported_filter_type_includes_position_and_name() {
        let e = FilterError::UnsupportedFilterType {
            position: 1,
            name: "envoy.filters.http.cors".to_string(),
        };
        let s = format!("{e}");
        assert!(s.contains("position 1"));
        assert!(s.contains("envoy.filters.http.cors"));
    }

    /// Static assertion that `FilterError` is `Send + Sync + 'static`.
    ///
    /// Required so the error can flow through tokio task boundaries.
    #[test]
    fn filter_error_is_send_sync_static() {
        fn _assert_send_sync<T: Send + Sync + 'static>() {}
        _assert_send_sync::<FilterError>();
    }
}
```

- [ ] **Step 5: Run `cargo build -p envoy-filter` to verify the crate compiles**

Run: `cargo build -p envoy-filter 2>&1 | tail -20`

Expected: clean build (no warnings, no errors). The crate has no public consumers yet so the `pub use error::FilterError;` re-export is the only public surface.

- [ ] **Step 6: Run `cargo test -p envoy-filter` to verify the 5 unit tests pass**

Run: `cargo test -p envoy-filter 2>&1 | tail -20`

Expected output (test names + counts):
```
running 5 tests
test error::tests::display_empty_chain_is_human_readable ... ok
test error::tests::display_router_not_terminal_includes_position_and_name ... ok
test error::tests::display_duplicate_router_includes_position ... ok
test error::tests::display_unsupported_filter_type_includes_position_and_name ... ok
test error::tests::filter_error_is_send_sync_static ... ok

test result: ok. 5 passed; 0 failed; 0 ignored
```

- [ ] **Step 7: Run workspace-wide checks**

Run in parallel:
- `cargo build --workspace --all-targets 2>&1 | tail -5`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -5`
- `cargo fmt --all -- --check 2>&1 | tail -5`
- `cargo test --workspace 2>&1 | tail -10`

Expected: all clean. The pre-existing `envoy-config` / `envoy-http1` / `envoy-http2` test suites stay green (no edits at Task 1 outside the new crate + workspace `Cargo.toml`).

- [ ] **Step 8: Create `docs/envoy-rust/phases/07.1-filter-framework-foundation/PROGRESS.md` with the Task 1 preamble**

```markdown
# Phase 07.1 (`07.1-filter-framework-foundation`) — PROGRESS

> Per-task narrative log appended at each substantive commit.
> Stranger-readable per D-3.4. PROGRESS.md is CREATED at Task 1's
> commit (NOT at the state-2 standalone PLAN.md commit — divergence
> from the 06.1/06.2/06.3 cadence; the 07.1 SPEC §8 cadence is "PLAN.md
> + STATE.md advance ONLY at state-2; PROGRESS lands at Task 1").

## Task 1 — `crates/envoy-filter/` scaffold + `FilterError` typed-error enum

### Work summary

Landed the new workspace member `crates/envoy-filter/` with `lib.rs` +
`error.rs` only (the strict module-per-task split per 07.1 SPEC §5
signpost 1 + PLAN architecture decision 1). `FilterError` enum with 4
variants (`EmptyChain`, `RouterNotTerminal`, `DuplicateRouter`,
`UnsupportedFilterType`) covers the framework's parse-time and
build-time invariants; the validator at envoy-config (Task 4) is the
earlier-layer catch and these are defense-in-depth at the framework
crate boundary.

Cargo.toml dependencies are existing workspace foundations only
(`bytes = "1"`, `thiserror = "2"`, `tracing = "0.1"`) plus the two
workspace path deps (`envoy-config`, `envoy-http1`). No new top-level
Cargo deps; `cargo deny check` remains a no-op for 07.1.

### Tests landed

5 unit tests at `crates/envoy-filter/src/error.rs::tests`:
- `display_empty_chain_is_human_readable` — Display impl matches the
  enum's `#[error(...)]` template.
- `display_router_not_terminal_includes_position_and_name` — substring
  asserts on each format-arg.
- `display_duplicate_router_includes_position` — exact Display match.
- `display_unsupported_filter_type_includes_position_and_name` —
  substring asserts.
- `filter_error_is_send_sync_static` — static assertion via
  `fn _assert_send_sync<T: Send + Sync + 'static>()` helper.

### LoC delta

| File | LoC |
|---|---|
| `crates/envoy-filter/Cargo.toml` | ~12 |
| `crates/envoy-filter/src/lib.rs` | ~10 |
| `crates/envoy-filter/src/error.rs` | ~75 (incl. 5 tests) |
| `Cargo.toml` (workspace root) | +1 line |
| `docs/envoy-rust/phases/07.1-filter-framework-foundation/PROGRESS.md` | ~40 |
| **Total** | **~138** |

### Deviations from PLAN

(Recorded per D-3.5 append-only discipline. If no deviations, the
section reads: "No deviations." For Task 1's actual landing, expand
this section with any PLAN-write SPEC corrections that surfaced during
empirical verification at the start of the task — per the next-prompt
recommended posture.)

### Test-bucket attestation

- `cargo test -p envoy-filter`: PASS (5 tests).
- `cargo build --workspace --all-targets`: clean.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: clean.
- `cargo fmt --all -- --check`: clean.
- `cargo test --workspace`: PASS (pre-existing tests unchanged + 5 new).
- `cargo deny check`: no-op (no new top-level deps).
```

- [ ] **Step 9: Commit Task 1**

```bash
git add crates/envoy-filter/ \
        Cargo.toml \
        docs/envoy-rust/phases/07.1-filter-framework-foundation/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 07.1: task 1 — envoy-filter crate scaffold + FilterError typed-error enum

New workspace member `crates/envoy-filter/` with `lib.rs` + `error.rs`
only at this task (strict per-task module decomposition per 07.1 SPEC
§5 signpost 1; `pipeline.rs` lands at Task 2, `instance.rs` + `router.rs`
at Task 3). FilterError enum with 4 variants (EmptyChain,
RouterNotTerminal, DuplicateRouter, UnsupportedFilterType) covers the
framework's parse-time invariants; the envoy-config validator (Task 4)
is the earlier-layer catch and FilterError is defense-in-depth at the
framework boundary.

Cargo deps: bytes / thiserror / tracing (existing foundations) +
envoy-config / envoy-http1 path deps. No new top-level Cargo deps. 5
unit tests asserting Display impls + Send+Sync+'static static
assertion. PROGRESS.md created at this commit.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: `FilterPipeline` + `Decision` enum + iteration-loop skeleton

**Scope:** ~120 LoC. Lands `crates/envoy-filter/src/pipeline.rs` with `FilterPipeline` struct + `Decision::{Continue, StopAndSend}` enum + `build_from_config` constructor + `decode_headers` + `encode_headers` iteration methods. Declares `pipeline` module + re-exports in `lib.rs`. Cannot fully compile yet because `HttpFilterInstance` doesn't exist until Task 3 — the planner ships Task 2 with a **placeholder `HttpFilterInstance` import via a temporary `cfg(test)` test-double** OR ships Task 2 + Task 3 in immediate sequence with a single workspace-tests-clean checkpoint at Task 3's end. **Decision (architecture decision 15):** Ship Task 2 with the `pipeline.rs` file using a **temporary inline `enum HttpFilterInstance { _Empty }` placeholder** declared in `instance.rs` (created at Task 2 as a minimal stub; Task 3 replaces the placeholder with the real Router-only enum). This keeps each task self-compileable and self-testable per the writing-plans TDD discipline.

**Files:**
- Create: `crates/envoy-filter/src/pipeline.rs` (~85 LoC including 4 unit tests).
- Create: `crates/envoy-filter/src/instance.rs` (~15 LoC PLACEHOLDER; Task 3 replaces).
- Modify: `crates/envoy-filter/src/lib.rs` — add `pub mod pipeline;` + `pub mod instance;` + re-exports.

- [ ] **Step 1: Write 4 failing unit tests in `pipeline.rs`**

Create `crates/envoy-filter/src/pipeline.rs` with the test module FIRST (TDD per writing-plans skill):

```rust
//! Filter chain iteration protocol.

use envoy_http1::codec::{Request, Response};
use crate::error::FilterError;
use crate::instance::HttpFilterInstance;

#[derive(Debug)]
pub enum Decision {
    Continue,
    StopAndSend(Response),
}

#[derive(Debug, Clone)]
pub struct FilterPipeline {
    filters: Vec<HttpFilterInstance>,
}

impl FilterPipeline {
    /// Build a `FilterPipeline` from a parsed envoy-config `HttpFilter` list.
    ///
    /// Returns an error if the list is empty. Per-instance build is delegated
    /// to `HttpFilterInstance::build` (Task 3). The parse-time validator at
    /// `envoy_config::validate_http_filters` performs the same cardinality
    /// checks earlier in the config-load path; this method's checks are
    /// defense-in-depth at the framework crate boundary.
    pub fn build_from_config(
        filters: &[envoy_config::HttpFilter],
    ) -> Result<Self, FilterError> {
        if filters.is_empty() {
            return Err(FilterError::EmptyChain);
        }
        let mut out = Vec::with_capacity(filters.len());
        for (position, hf) in filters.iter().enumerate() {
            out.push(HttpFilterInstance::build(hf, position)?);
        }
        Ok(Self { filters: out })
    }

    /// Iterate the filter chain in **declaration order** on the decode side.
    ///
    /// Per parent-07 SPEC §6 Rule 6: decode walks `filters.iter_mut()`.
    /// First `StopAndSend` short-circuits remaining iteration.
    pub fn decode_headers(&mut self, req: &mut Request) -> Decision {
        for filter in self.filters.iter_mut() {
            match filter.decode_headers(req) {
                Decision::Continue => continue,
                Decision::StopAndSend(resp) => return Decision::StopAndSend(resp),
            }
        }
        Decision::Continue
    }

    /// Iterate the filter chain in **reverse declaration order** on the
    /// encode side.
    ///
    /// Per parent-07 SPEC §6 Rule 6: encode walks `filters.iter_mut().rev()`.
    /// This matches Envoy v1.33's documented filter-chain semantics where
    /// the Router filter produces the response (so it fires first on encode)
    /// and other filters mutate it on the way out.
    pub fn encode_headers(&mut self, resp: &mut Response) -> Decision {
        for filter in self.filters.iter_mut().rev() {
            match filter.encode_headers(resp) {
                Decision::Continue => continue,
                Decision::StopAndSend(replacement) => {
                    return Decision::StopAndSend(replacement)
                }
            }
        }
        Decision::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_from_config_rejects_empty_list() {
        let filters: Vec<envoy_config::HttpFilter> = Vec::new();
        let err = FilterPipeline::build_from_config(&filters).unwrap_err();
        assert!(matches!(err, FilterError::EmptyChain));
    }

    #[test]
    fn build_from_config_with_single_router_succeeds() {
        // Construct a single `Router`-typed HttpFilter via envoy-config's
        // public type constructors. (Task 3 ships the real Router build path;
        // at Task 2 the placeholder HttpFilterInstance::build accepts the
        // Router variant.)
        let filters = vec![envoy_config::HttpFilter {
            name: "envoy.filters.http.router".to_string(),
            typed_config: envoy_config::HttpFilterTypedConfig::Router(
                envoy_config::RouterConfig {},
            ),
        }];
        let pipeline = FilterPipeline::build_from_config(&filters)
            .expect("single-Router build succeeds");
        assert_eq!(pipeline.filters.len(), 1);
    }

    #[test]
    fn decode_headers_on_single_router_returns_continue() {
        let filters = vec![envoy_config::HttpFilter {
            name: "envoy.filters.http.router".to_string(),
            typed_config: envoy_config::HttpFilterTypedConfig::Router(
                envoy_config::RouterConfig {},
            ),
        }];
        let mut pipeline = FilterPipeline::build_from_config(&filters).unwrap();
        let mut req = test_request();
        let decision = pipeline.decode_headers(&mut req);
        assert!(matches!(decision, Decision::Continue));
    }

    #[test]
    fn encode_headers_on_single_router_returns_continue() {
        let filters = vec![envoy_config::HttpFilter {
            name: "envoy.filters.http.router".to_string(),
            typed_config: envoy_config::HttpFilterTypedConfig::Router(
                envoy_config::RouterConfig {},
            ),
        }];
        let mut pipeline = FilterPipeline::build_from_config(&filters).unwrap();
        let mut resp = test_response();
        let decision = pipeline.encode_headers(&mut resp);
        assert!(matches!(decision, Decision::Continue));
    }

    /// Construct a minimal `envoy_http1::codec::Request` for tests.
    ///
    /// Mirrors the test-construction shape used in existing envoy-http1
    /// tests; if envoy-http1 exposes a `Request::for_test` constructor,
    /// use that instead.
    fn test_request() -> Request {
        Request {
            method: "GET".to_string(),
            path: "/".to_string(),
            headers: vec![("host".to_string(), "localhost".to_string())],
            body: bytes::Bytes::new(),
        }
    }

    fn test_response() -> Response {
        Response {
            status: 200,
            headers: vec![("content-length".to_string(), "0".to_string())],
            body: bytes::Bytes::new(),
        }
    }
}
```

**Note on test-construction shape.** The `Request` / `Response` field-construction in `test_request()` / `test_response()` mirrors the field shape established at 04.1 + 05.2 (`crates/envoy-http1/src/codec.rs` defines `Request` + `Response` as plain structs with `method` / `path` / `headers` / `body` and `status` / `headers` / `body` respectively). The executor verifies the exact field set at task-start by reading `crates/envoy-http1/src/codec.rs` and adjusts the literal-construction syntax as needed.

- [ ] **Step 2: Create placeholder `instance.rs`**

```rust
//! Placeholder for `HttpFilterInstance` enum.
//!
//! Task 2 ships this stub so `pipeline.rs` compiles. Task 3 replaces
//! the stub with the real Router-only enum + `RouterTerminus` filter
//! type. The `build` constructor accepts any `Router`-typed config and
//! rejects everything else.

use envoy_http1::codec::{Request, Response};
use crate::error::FilterError;
use crate::pipeline::Decision;

#[derive(Debug, Clone)]
pub enum HttpFilterInstance {
    /// Task-2 placeholder. Holds nothing. Replaced at Task 3 with the
    /// real `Router(RouterTerminus)` variant.
    Router,
}

impl HttpFilterInstance {
    pub(crate) fn build(
        hf: &envoy_config::HttpFilter,
        position: usize,
    ) -> Result<Self, FilterError> {
        match &hf.typed_config {
            envoy_config::HttpFilterTypedConfig::Router(_cfg) => {
                Ok(HttpFilterInstance::Router)
            }
        }
    }

    pub(crate) fn decode_headers(&mut self, _req: &mut Request) -> Decision {
        match self {
            HttpFilterInstance::Router => Decision::Continue,
        }
    }

    pub(crate) fn encode_headers(&mut self, _resp: &mut Response) -> Decision {
        match self {
            HttpFilterInstance::Router => Decision::Continue,
        }
    }
}
```

The `position` parameter is unused at Task 2 (no error path exercises it) but kept in the signature to match Task 3's final shape — avoids a signature-change at Task 3 that would ripple through `pipeline.rs`.

- [ ] **Step 3: Update `lib.rs` to declare the new modules + re-exports**

Replace the Task-1 `lib.rs` with:

```rust
#![forbid(unsafe_code)]

//! HTTP filter chain iteration protocol.
//!
//! Hand-rolled per D-3.2's "Must be written from scratch" doctrine for
//! filter chain engines. Synchronous (non-async) iteration on the
//! already-buffered request/response shape established by 04.1 + 05.2.

pub mod error;
pub mod instance;
pub mod pipeline;

pub use error::FilterError;
pub use instance::HttpFilterInstance;
pub use pipeline::{Decision, FilterPipeline};
```

- [ ] **Step 4: Run `cargo test -p envoy-filter` to verify the 4 new tests fail (or compile-fail)**

Run: `cargo test -p envoy-filter 2>&1 | tail -30`

Expected: the test functions compile + the 4 new tests PASS immediately (the placeholder Router-arm correctly returns `Continue` on both iteration sides). The test FAILures expected at Step 1 in pure TDD assume the implementation doesn't exist yet — but in this task the test code and implementation are co-located in `pipeline.rs` + `instance.rs`. **TDD shape adjustment:** for this task, the TDD discipline is satisfied by writing the test cases BEFORE the implementation in the same file (the planner's eye reviews the test names FIRST, then the body, then the implementation). If the executor prefers strict red-then-green, the executor stubs out the implementation with `todo!()` returns at Step 1, runs the tests to see RED, then fills in the implementation at Step 2/3.

Expected output (test names + counts):
```
running 4 tests
test pipeline::tests::build_from_config_rejects_empty_list ... ok
test pipeline::tests::build_from_config_with_single_router_succeeds ... ok
test pipeline::tests::decode_headers_on_single_router_returns_continue ... ok
test pipeline::tests::encode_headers_on_single_router_returns_continue ... ok

test result: ok. 4 passed; 0 failed; 0 ignored
```

Total `cargo test -p envoy-filter` count after this task: 5 (Task 1 error tests) + 4 (Task 2 pipeline tests) = 9.

- [ ] **Step 5: Run workspace-wide checks**

Run in parallel:
- `cargo build --workspace --all-targets 2>&1 | tail -5`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -5`
- `cargo fmt --all -- --check 2>&1 | tail -5`
- `cargo test --workspace 2>&1 | tail -10`

Expected: all clean. `envoy-filter` test count grows from 5 to 9; all other workspace tests unchanged.

- [ ] **Step 6: Append Task 2 entry to PROGRESS.md**

Append to `docs/envoy-rust/phases/07.1-filter-framework-foundation/PROGRESS.md`:

```markdown
## Task 2 — `FilterPipeline` + `Decision` enum + iteration-loop skeleton

### Work summary

Landed `crates/envoy-filter/src/pipeline.rs` with the `FilterPipeline`
struct + `Decision::{Continue, StopAndSend}` enum + `build_from_config`
constructor + `decode_headers` (declaration-order walk) + `encode_headers`
(reverse-declaration-order walk per parent-07 SPEC §6 Rule 6).

Per architecture decision 15, also landed a placeholder
`crates/envoy-filter/src/instance.rs` with a single-variant
`HttpFilterInstance::Router` (zero-state placeholder). Task 3 will
replace the placeholder with the real `Router(RouterTerminus)` variant
+ separate `router.rs` module. The placeholder keeps Task 2
self-compileable + self-testable per the writing-plans TDD discipline;
the `position` parameter in `HttpFilterInstance::build`'s signature is
already in its final shape so Task 3's enum-variant change does not
ripple through `pipeline.rs`.

### Tests landed

4 unit tests at `crates/envoy-filter/src/pipeline.rs::tests`:
- `build_from_config_rejects_empty_list` — `FilterError::EmptyChain`
  fires on the empty-list path.
- `build_from_config_with_single_router_succeeds` — single Router
  passes; `filters.len() == 1`.
- `decode_headers_on_single_router_returns_continue` — Router is no-op
  on decode side.
- `encode_headers_on_single_router_returns_continue` — Router is no-op
  on encode side.

Total envoy-filter test count: 5 (Task 1) + 4 (Task 2) = 9.

### LoC delta

| File | LoC |
|---|---|
| `crates/envoy-filter/src/pipeline.rs` | ~125 (incl. 4 tests + 2 test helpers) |
| `crates/envoy-filter/src/instance.rs` (placeholder) | ~32 |
| `crates/envoy-filter/src/lib.rs` (extension) | +6 |
| `docs/envoy-rust/phases/07.1-filter-framework-foundation/PROGRESS.md` (Task 2 entry) | ~40 |
| **Total** | **~203** |

### Deviations from PLAN

(Append any execution-time deviations. Likely deviation: the test
helpers `test_request()` / `test_response()` may use a different field
shape than projected if `crates/envoy-http1/src/codec.rs`'s
`Request` / `Response` structs have moved since PLAN write — adjust
inline at task-start by reading the structs first.)

### Test-bucket attestation

- `cargo test -p envoy-filter`: PASS (9 tests).
- `cargo build --workspace --all-targets`: clean.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: clean.
- `cargo fmt --all -- --check`: clean.
- `cargo test --workspace`: PASS (pre-existing tests unchanged + 9 envoy-filter tests).
- `cargo deny check`: no-op.
```

- [ ] **Step 7: Commit Task 2**

```bash
git add crates/envoy-filter/ \
        docs/envoy-rust/phases/07.1-filter-framework-foundation/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 07.1: task 2 — FilterPipeline + Decision + iteration loop skeleton

`crates/envoy-filter/src/pipeline.rs` lands with FilterPipeline struct
(holds `Vec<HttpFilterInstance>`) + Decision::{Continue, StopAndSend}
enum (StopAndSend scaffolded from day one per 07.1 SPEC §5 signpost 2
for forward-compat with 07.2 HeaderMutation; no 07.1 filter emits it) +
`build_from_config` (rejects empty list with FilterError::EmptyChain;
delegates per-instance build to HttpFilterInstance::build) +
`decode_headers` (declaration-order walk via filters.iter_mut()) +
`encode_headers` (reverse-declaration-order walk via
filters.iter_mut().rev() per parent-07 SPEC §6 Rule 6).

Placeholder `crates/envoy-filter/src/instance.rs` ships a single-variant
HttpFilterInstance::Router stub; Task 3 replaces with the real
Router(RouterTerminus) variant + separate router.rs module. The
placeholder shape preserves HttpFilterInstance::build's signature so
Task 3's enum-variant change does not ripple through pipeline.rs.

4 unit tests on FilterPipeline (empty-list reject + single-Router build
+ decode no-op + encode no-op). Total envoy-filter test count: 9.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: `HttpFilterInstance` enum (Router-only) + `RouterTerminus`

**Scope:** ~110 LoC. Replaces Task 2's placeholder `instance.rs` with the real Router-only enum holding a `RouterTerminus` payload + creates new module `router.rs` containing the `RouterTerminus` struct (no-op terminus filter; the validator guarantees Router is the last entry so on decode-side Router runs last and on encode-side Router runs first per the reverse-iteration shape — which models Envoy's semantic of "Router filter produces the response").

**Files:**
- Modify: `crates/envoy-filter/src/instance.rs` — replace Task 2 placeholder with Router-payload variant; update `build` to construct `RouterTerminus::new()`; update `decode_headers` + `encode_headers` to delegate to the contained `RouterTerminus`.
- Create: `crates/envoy-filter/src/router.rs` (~50 LoC including 3 unit tests).
- Modify: `crates/envoy-filter/src/lib.rs` — add `pub mod router;` declaration.

- [ ] **Step 1: Write 3 failing unit tests in `router.rs`**

Create `crates/envoy-filter/src/router.rs` with the test module FIRST:

```rust
//! Router filter — the terminus of every filter chain.
//!
//! `Router` is the filter that dispatches to the route's action
//! (`direct_response` or upstream proxy). At the filter-chain level it
//! is a no-op on both iteration sides — the actual dispatch happens
//! inside the HCM's writer-arm match after `pipeline.decode_headers`
//! returns and route-match runs.
//!
//! The validator (Task 4) guarantees Router is the last entry. On
//! decode this means `Router::decode_headers` runs LAST among all
//! filters; on encode (reverse order) this means `Router::encode_headers`
//! runs FIRST, which models Envoy's semantic of "Router produces the
//! response and other filters mutate it on the encode side".

use envoy_http1::codec::{Request, Response};
use crate::pipeline::Decision;

#[derive(Debug, Clone, Default)]
pub struct RouterTerminus {
    _private: (),
}

impl RouterTerminus {
    pub(crate) fn new() -> Self {
        Self { _private: () }
    }

    pub(crate) fn decode_headers(&mut self, _req: &mut Request) -> Decision {
        Decision::Continue
    }

    pub(crate) fn encode_headers(&mut self, _resp: &mut Response) -> Decision {
        Decision::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    #[test]
    fn decode_headers_returns_continue_and_does_not_mutate_request() {
        let mut router = RouterTerminus::new();
        let mut req = Request {
            method: "GET".to_string(),
            path: "/".to_string(),
            headers: vec![("host".to_string(), "example.com".to_string())],
            body: Bytes::from_static(b"hello"),
        };
        let before = (
            req.method.clone(),
            req.path.clone(),
            req.headers.clone(),
            req.body.clone(),
        );
        let decision = router.decode_headers(&mut req);
        assert!(matches!(decision, Decision::Continue));
        assert_eq!(req.method, before.0);
        assert_eq!(req.path, before.1);
        assert_eq!(req.headers, before.2);
        assert_eq!(req.body, before.3);
    }

    #[test]
    fn encode_headers_returns_continue_and_does_not_mutate_response() {
        let mut router = RouterTerminus::new();
        let mut resp = Response {
            status: 200,
            headers: vec![("content-length".to_string(), "5".to_string())],
            body: Bytes::from_static(b"hello"),
        };
        let before = (resp.status, resp.headers.clone(), resp.body.clone());
        let decision = router.encode_headers(&mut resp);
        assert!(matches!(decision, Decision::Continue));
        assert_eq!(resp.status, before.0);
        assert_eq!(resp.headers, before.1);
        assert_eq!(resp.body, before.2);
    }

    #[test]
    fn router_terminus_is_clone_and_default() {
        // Default constructor produces the same shape as `new()`.
        let r1 = RouterTerminus::default();
        let r2 = RouterTerminus::new();
        assert_eq!(format!("{r1:?}"), format!("{r2:?}"));
        // Clone produces a structurally-identical instance.
        let r3 = r1.clone();
        assert_eq!(format!("{r1:?}"), format!("{r3:?}"));
    }
}
```

- [ ] **Step 2: Replace `instance.rs` placeholder with the real Router-payload enum**

Replace `crates/envoy-filter/src/instance.rs` entirely with:

```rust
//! `HttpFilterInstance` — the per-instance variant enum.
//!
//! At 07.1 the only variant is `Router` (holding `RouterTerminus`).
//! Phase 07.2 adds `HeaderMutation(HeaderMutationFilter)` per parent-07
//! SPEC §3 D8.2-D15.2.

use envoy_http1::codec::{Request, Response};
use crate::error::FilterError;
use crate::pipeline::Decision;
use crate::router::RouterTerminus;

#[derive(Debug, Clone)]
pub enum HttpFilterInstance {
    Router(RouterTerminus),
    // 07.2 adds: HeaderMutation(HeaderMutationFilter)
}

impl HttpFilterInstance {
    /// Construct a per-instance filter from a parsed envoy-config
    /// `HttpFilter` entry.
    ///
    /// The validator at `envoy_config::validate_http_filters` (Task 4)
    /// performs the name/typed_config consistency checks at config-load
    /// time. This constructor relies on the validator's invariants but
    /// does not duplicate the checks (defense-in-depth lives at
    /// `FilterPipeline::build_from_config`, not here).
    pub(crate) fn build(
        hf: &envoy_config::HttpFilter,
        _position: usize,
    ) -> Result<Self, FilterError> {
        match &hf.typed_config {
            envoy_config::HttpFilterTypedConfig::Router(_cfg) => {
                Ok(HttpFilterInstance::Router(RouterTerminus::new()))
            }
            // 07.2's HeaderMutation arm lands here.
        }
    }

    pub(crate) fn decode_headers(&mut self, req: &mut Request) -> Decision {
        match self {
            HttpFilterInstance::Router(r) => r.decode_headers(req),
        }
    }

    pub(crate) fn encode_headers(&mut self, resp: &mut Response) -> Decision {
        match self {
            HttpFilterInstance::Router(r) => r.encode_headers(resp),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_router_succeeds() {
        let hf = envoy_config::HttpFilter {
            name: "envoy.filters.http.router".to_string(),
            typed_config: envoy_config::HttpFilterTypedConfig::Router(
                envoy_config::RouterConfig {},
            ),
        };
        let instance = HttpFilterInstance::build(&hf, 0)
            .expect("Router build succeeds");
        assert!(matches!(instance, HttpFilterInstance::Router(_)));
    }
}
```

**Note on the `_position` parameter.** Currently unused; kept in the signature per Task 2's placeholder shape so 07.2's HeaderMutation arm can use `position` for its `FilterError::RouterNotTerminal { actual, position, expected }` defense-in-depth check inside `build` itself (if needed). At 07.1 the validator catches RouterNotTerminal at config-load time so `build` does not need to construct this error.

- [ ] **Step 3: Update `lib.rs` to declare `router` module + re-export `RouterTerminus`**

Replace `crates/envoy-filter/src/lib.rs` (the Task-2 version) with:

```rust
#![forbid(unsafe_code)]

//! HTTP filter chain iteration protocol.
//!
//! Hand-rolled per D-3.2's "Must be written from scratch" doctrine for
//! filter chain engines. Synchronous (non-async) iteration on the
//! already-buffered request/response shape established by 04.1 + 05.2.

pub mod error;
pub mod instance;
pub mod pipeline;
pub mod router;

pub use error::FilterError;
pub use instance::HttpFilterInstance;
pub use pipeline::{Decision, FilterPipeline};
pub use router::RouterTerminus;
```

- [ ] **Step 4: Run tests to verify all 3 + 1 new tests pass and the prior 9 stay green**

Run: `cargo test -p envoy-filter 2>&1 | tail -30`

Expected output (test names + counts):
```
running 13 tests
test error::tests::display_empty_chain_is_human_readable ... ok
test error::tests::display_router_not_terminal_includes_position_and_name ... ok
test error::tests::display_duplicate_router_includes_position ... ok
test error::tests::display_unsupported_filter_type_includes_position_and_name ... ok
test error::tests::filter_error_is_send_sync_static ... ok
test instance::tests::build_router_succeeds ... ok
test pipeline::tests::build_from_config_rejects_empty_list ... ok
test pipeline::tests::build_from_config_with_single_router_succeeds ... ok
test pipeline::tests::decode_headers_on_single_router_returns_continue ... ok
test pipeline::tests::encode_headers_on_single_router_returns_continue ... ok
test router::tests::decode_headers_returns_continue_and_does_not_mutate_request ... ok
test router::tests::encode_headers_returns_continue_and_does_not_mutate_response ... ok
test router::tests::router_terminus_is_clone_and_default ... ok

test result: ok. 13 passed; 0 failed; 0 ignored
```

Total envoy-filter test count after Task 3: 5 (error) + 4 (pipeline) + 1 (instance) + 3 (router) = 13.

- [ ] **Step 5: Run workspace-wide checks**

Run in parallel:
- `cargo build --workspace --all-targets 2>&1 | tail -5`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -5`
- `cargo fmt --all -- --check 2>&1 | tail -5`
- `cargo test --workspace 2>&1 | tail -10`

Expected: all clean.

- [ ] **Step 6: Append Task 3 entry to PROGRESS.md + commit**

Append Task 3 narrative to PROGRESS.md (mirror Task 2's shape — work summary + tests landed + LoC delta + deviations + test-bucket attestation). Then:

```bash
git add crates/envoy-filter/ \
        docs/envoy-rust/phases/07.1-filter-framework-foundation/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 07.1: task 3 — HttpFilterInstance Router-only + RouterTerminus

Replaces Task 2's placeholder `instance.rs` with the real Router-payload
variant: `HttpFilterInstance::Router(RouterTerminus)`. Adds new module
`crates/envoy-filter/src/router.rs` with the `RouterTerminus` struct
(zero-state; derives Debug + Clone + Default; private constructor;
decode_headers + encode_headers both return Decision::Continue without
mutating the req/resp).

Router is the terminus of every filter chain per parent-07 SPEC §6 Rule
3. The validator at envoy-config (Task 4) enforces Router-at-last. The
iteration semantic — decode order walks Router last; reverse-encode walks
Router first — models Envoy's "Router produces the response, other
filters mutate on encode" shape.

3 unit tests on RouterTerminus (decode no-op + encode no-op + Clone +
Default symmetry) + 1 unit test on HttpFilterInstance::build. Total
envoy-filter test count: 13.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: `envoy-config` validator relaxation + 3 new `ConfigError` variants

**Scope:** ~220 LoC. Relaxes the existing cardinality gate at `crates/envoy-config/src/bootstrap.rs:1335-1346` from `len != 1` to `len >= 1 AND Router-last AND no-duplicate-Router` via a new free function `validate_http_filters(filters: &[HttpFilter], listener_name: &str) -> Result<(), ConfigError>`. Adds 3 new `ConfigError` variants: `EmptyHttpFilters`, `RouterNotTerminal`, `DuplicateRouterFilter`. Retains the existing `MultipleHttpFilters` variant (no longer constructed) per architecture decision (signpost 13). The existing `UnsupportedHttpFilter` variant continues firing on a `name`/`typed_config` mismatch.

**Files:**
- Modify: `crates/envoy-config/src/lib.rs` — append 3 new variants to `ConfigError`; add doc-comment supersession note on `MultipleHttpFilters`.
- Modify: `crates/envoy-config/src/bootstrap.rs` — replace lines 1335-1346 with a `validate_http_filters(&hcm.http_filters, listener_name)?` call; add the new `validate_http_filters` free function; thread `listener_name` through the call chain.
- Modify: `crates/envoy-config/src/bootstrap.rs::tests` — add 7 new unit tests + amend any existing tests that asserted `MultipleHttpFilters`.

- [ ] **Step 1: Read the existing validator surface to lock in the call-site context**

Read `crates/envoy-config/src/bootstrap.rs` around lines 1300-1400 to confirm:
- The function name that calls into the cardinality gate (expected: `validate_hcm` or similar).
- Whether `listener_name: &str` is already in scope at the call site (it should be — the outer `listeners` walk knows the listener name).
- The exact line numbers of the `match hcm.http_filters.len()` gate (SPEC quotes `1335-1347`; empirical grep showed `1335-1346` — 1-line drift, negligible).

If `listener_name` is NOT already in scope at the call site, thread it through by modifying `validate_hcm`'s signature to accept `listener_name: &str`. The outer caller (the listeners walk) has access to `listener.name`.

- [ ] **Step 2: Write 7 failing unit tests in `bootstrap.rs::tests`**

Append to `crates/envoy-config/src/bootstrap.rs`'s test module (the existing `#[cfg(test)] mod tests` block near the bottom of the file):

```rust
#[test]
fn validate_http_filters_accepts_single_router() {
    let filters = vec![crate::HttpFilter {
        name: "envoy.filters.http.router".to_string(),
        typed_config: crate::HttpFilterTypedConfig::Router(
            crate::RouterConfig {},
        ),
    }];
    let result = super::validate_http_filters(&filters, "ingress_http");
    assert!(result.is_ok(), "single Router passes; got {result:?}");
}

#[test]
fn validate_http_filters_rejects_empty_list() {
    let filters: Vec<crate::HttpFilter> = Vec::new();
    let err = super::validate_http_filters(&filters, "ingress_http")
        .expect_err("empty list rejects");
    match err {
        crate::ConfigError::EmptyHttpFilters { listener } => {
            assert_eq!(listener, "ingress_http");
        }
        other => panic!("expected EmptyHttpFilters, got {other:?}"),
    }
}

#[test]
fn validate_http_filters_rejects_duplicate_router() {
    let router = || crate::HttpFilter {
        name: "envoy.filters.http.router".to_string(),
        typed_config: crate::HttpFilterTypedConfig::Router(
            crate::RouterConfig {},
        ),
    };
    let filters = vec![router(), router()];
    let err = super::validate_http_filters(&filters, "ingress_http")
        .expect_err("duplicate Router rejects");
    match err {
        crate::ConfigError::DuplicateRouterFilter { listener } => {
            assert_eq!(listener, "ingress_http");
        }
        other => panic!("expected DuplicateRouterFilter, got {other:?}"),
    }
}

#[test]
fn validate_http_filters_rejects_name_typed_config_mismatch() {
    // Name says fault, but typed_config carries Router. Validator
    // catches this via the existing UnsupportedHttpFilter arm.
    let filters = vec![crate::HttpFilter {
        name: "envoy.filters.http.fault".to_string(),
        typed_config: crate::HttpFilterTypedConfig::Router(
            crate::RouterConfig {},
        ),
    }];
    let err = super::validate_http_filters(&filters, "ingress_http")
        .expect_err("name/typed_config mismatch rejects");
    match err {
        crate::ConfigError::UnsupportedHttpFilter { name } => {
            assert_eq!(name, "envoy.filters.http.fault");
        }
        other => panic!("expected UnsupportedHttpFilter, got {other:?}"),
    }
}

#[test]
fn validate_http_filters_listener_name_propagates() {
    // Error variants must carry the listener name through (not hardcoded
    // empty string).
    let filters: Vec<crate::HttpFilter> = Vec::new();
    let err = super::validate_http_filters(&filters, "custom_listener_42")
        .expect_err("empty list rejects");
    assert!(format!("{err:?}").contains("custom_listener_42"));
}

/// Negative — RouterNotTerminal is unreachable at 07.1 with a non-Router
/// variant (the HttpFilterTypedConfig enum is closed; only Router exists),
/// but the validator's logic must construct the error correctly if a future
/// HttpFilterTypedConfig variant lands and is positioned before Router.
///
/// At 07.1 we exercise the RouterNotTerminal path via the
/// duplicate-Router → "Router not at last position" sub-case, where the
/// validator must recognize that the first Router is NOT at the last
/// position. The current validator returns DuplicateRouterFilter for this
/// case (not RouterNotTerminal) because duplicate detection runs first;
/// this test documents the precedence.
#[test]
fn validate_http_filters_duplicate_router_takes_precedence_over_router_not_terminal() {
    let router = || crate::HttpFilter {
        name: "envoy.filters.http.router".to_string(),
        typed_config: crate::HttpFilterTypedConfig::Router(
            crate::RouterConfig {},
        ),
    };
    let filters = vec![router(), router(), router()];
    let err = super::validate_http_filters(&filters, "ingress_http")
        .expect_err("3 Routers rejects");
    // The validator may return either DuplicateRouterFilter or
    // RouterNotTerminal; the locked precedence is DuplicateRouterFilter
    // (it runs first in the validator's state-walk).
    assert!(matches!(
        err,
        crate::ConfigError::DuplicateRouterFilter { .. }
    ));
}

/// Existing 12 fixtures' http_filters: [Router] shape stays valid.
///
/// This is a fast structural check that the new validator accepts
/// every existing fixture's filter-chain shape. Mirrors the existing
/// `parse_fixture_NNNN` test pattern at the bottom of bootstrap.rs.
#[test]
fn validate_http_filters_accepts_existing_fixture_shape() {
    // Single Router, name + typed_config consistent — the shape every
    // pre-07.1 fixture declares.
    let filters = vec![crate::HttpFilter {
        name: "envoy.filters.http.router".to_string(),
        typed_config: crate::HttpFilterTypedConfig::Router(
            crate::RouterConfig {},
        ),
    }];
    super::validate_http_filters(&filters, "ingress_http")
        .expect("pre-07.1 fixture filter-chain shape stays valid");
}
```

- [ ] **Step 3: Run tests to verify they FAIL (or fail to compile)**

Run: `cargo test -p envoy-config validate_http_filters -- --nocapture 2>&1 | tail -30`

Expected: FAIL with `error[E0599]: no function or associated item named 'validate_http_filters' found for type 'crate::bootstrap'` AND/OR `error[E0599]: no variant or associated item named 'EmptyHttpFilters' / 'RouterNotTerminal' / 'DuplicateRouterFilter' found for enum 'ConfigError'`.

- [ ] **Step 4: Add the 3 new `ConfigError` variants**

Edit `crates/envoy-config/src/lib.rs`. Locate the `ConfigError` enum definition; append (just before the closing brace):

```rust
    /// 07.1 D4.1: listener's http_filters list is empty.
    ///
    /// HCM listeners must declare at least one HTTP filter (the
    /// `Router` filter — terminus). Empty lists are not legal per the
    /// terminal-router validator.
    #[error("HCM listener {listener:?} has empty http_filters list (must contain at least Router)")]
    EmptyHttpFilters { listener: String },

    /// 07.1 D4.1: listener's Router filter is not at the terminus
    /// position.
    ///
    /// The validator requires Router to be the last entry in
    /// `http_filters`. Earlier-Router placements trigger this error.
    #[error("HCM listener {listener:?}: Router filter is not at the terminus (found at position {position})")]
    RouterNotTerminal { listener: String, position: usize },

    /// 07.1 D4.1: listener's http_filters list contains more than one
    /// Router filter.
    ///
    /// The validator requires exactly one Router. Duplicate Routers
    /// trigger this error.
    #[error("HCM listener {listener:?}: filter chain contains duplicate Router filter")]
    DuplicateRouterFilter { listener: String },
```

Also update the existing `MultipleHttpFilters` variant with a doc-comment supersession note:

```rust
    /// Superseded by `EmptyHttpFilters` / `RouterNotTerminal` /
    /// `DuplicateRouterFilter` at 07.1; retained for ledger discipline
    /// per D-3.5 (typed-error API is grow-only). No code path
    /// constructs this variant after 07.1 Task 4.
    #[error("HCM listener has multiple http_filters (count: {count}); only single-Router chain supported prior to 07.1")]
    MultipleHttpFilters { count: usize },
```

- [ ] **Step 5: Add the `validate_http_filters` free function in `bootstrap.rs`**

Add to `crates/envoy-config/src/bootstrap.rs` (place near the existing `validate_hcm` function — same module scope so it sees `HttpFilter`, `HttpFilterTypedConfig`, etc.):

```rust
/// Validate the http_filters list of an HCM listener.
///
/// Enforces: (a) at least one filter, (b) exactly one Router, (c) Router
/// at the terminus, (d) name/typed_config consistency on every entry.
///
/// At 07.1 the only typed_config variant is `Router`; the
/// name/typed_config consistency check is currently dead-code-defensive
/// (the schema's `HttpFilterTypedConfig` enum is closed and serde's
/// `deny_unknown_fields` rejects unknown variants at parse time). The
/// check is retained so 07.2's HeaderMutation arm slots in without a
/// validator rewrite.
///
/// Replaces the pre-07.1 cardinality gate at lines 1335-1346 of this
/// file. Mirrors 05.2's `validate_h2_protocol_options` /
/// 06.3's `Http2ClusterFromHttp1Listener` listener-name-threaded
/// validator shape.
pub(crate) fn validate_http_filters(
    filters: &[crate::HttpFilter],
    listener_name: &str,
) -> Result<(), crate::ConfigError> {
    if filters.is_empty() {
        return Err(crate::ConfigError::EmptyHttpFilters {
            listener: listener_name.to_string(),
        });
    }

    let router_name = "envoy.filters.http.router";
    let last_index = filters.len() - 1;
    let mut router_positions: Vec<usize> = Vec::new();

    for (i, f) in filters.iter().enumerate() {
        // Per-filter name/typed_config consistency check.
        // At 07.1 the only typed_config is Router; the closed-enum match
        // is exhaustive. 07.2's HeaderMutation arm slots in below.
        match &f.typed_config {
            crate::HttpFilterTypedConfig::Router(_) => {
                if f.name != router_name {
                    return Err(crate::ConfigError::UnsupportedHttpFilter {
                        name: f.name.clone(),
                    });
                }
                router_positions.push(i);
            }
            // 07.2 adds: HttpFilterTypedConfig::HeaderMutation(_) =>
            //     if f.name != "envoy.filters.http.header_mutation" { ... }
        }
    }

    if router_positions.len() > 1 {
        return Err(crate::ConfigError::DuplicateRouterFilter {
            listener: listener_name.to_string(),
        });
    }
    if router_positions.is_empty() {
        // No Router at all — model as RouterNotTerminal with position
        // = last_index (the position Router should occupy).
        return Err(crate::ConfigError::RouterNotTerminal {
            listener: listener_name.to_string(),
            position: last_index,
        });
    }
    let router_position = router_positions[0];
    if router_position != last_index {
        return Err(crate::ConfigError::RouterNotTerminal {
            listener: listener_name.to_string(),
            position: router_position,
        });
    }
    Ok(())
}
```

- [ ] **Step 6: Replace the existing cardinality gate at the call site**

Locate lines 1335-1346 in `crates/envoy-config/src/bootstrap.rs` (inside `validate_hcm` or the inline HCM validation block). The current code reads (approximately):

```rust
match hcm.http_filters.len() {
    1 => {
        let f = &hcm.http_filters[0];
        if f.name != "envoy.filters.http.router" {
            return Err(crate::ConfigError::UnsupportedHttpFilter {
                name: f.name.clone(),
            });
        }
        // Router-only allowed at 07.0; any other shape rejected.
        match &f.typed_config {
            crate::HttpFilterTypedConfig::Router(_) => {}
        }
    }
    n => return Err(crate::ConfigError::MultipleHttpFilters { count: n }),
}
```

Replace this entire block with:

```rust
validate_http_filters(&hcm.http_filters, listener_name)?;
```

Verify `listener_name: &str` is in scope at this site. If `validate_hcm`'s signature is `fn validate_hcm(hcm: &mut HttpConnectionManagerConfig, clusters: &[Cluster]) -> Result<(), ConfigError>`, extend it to `fn validate_hcm(hcm: &mut HttpConnectionManagerConfig, clusters: &[Cluster], listener_name: &str) -> Result<(), ConfigError>`. Update all callers (likely just the outer `validate` function's listeners walk).

- [ ] **Step 7: Run tests to verify all 7 new tests PASS + the 12 fixture-parse tests stay GREEN**

Run in parallel:
- `cargo test -p envoy-config validate_http_filters 2>&1 | tail -20`
- `cargo test -p envoy-config parse_fixture 2>&1 | tail -20`

Expected:
```
running 7 tests
test bootstrap::tests::validate_http_filters_accepts_single_router ... ok
test bootstrap::tests::validate_http_filters_rejects_empty_list ... ok
test bootstrap::tests::validate_http_filters_rejects_duplicate_router ... ok
test bootstrap::tests::validate_http_filters_rejects_name_typed_config_mismatch ... ok
test bootstrap::tests::validate_http_filters_listener_name_propagates ... ok
test bootstrap::tests::validate_http_filters_duplicate_router_takes_precedence_over_router_not_terminal ... ok
test bootstrap::tests::validate_http_filters_accepts_existing_fixture_shape ... ok

test result: ok. 7 passed; 0 failed; 0 ignored
```

For `parse_fixture` tests: every existing fixture parser test stays green (all 12 fixtures declare `http_filters: [Router]` which passes both the old and new gates).

- [ ] **Step 8: Amend any existing tests that asserted `MultipleHttpFilters`**

Grep for usages: `grep -rn "MultipleHttpFilters" crates/envoy-config/src/`.

For every test that constructed a 2+-filter list and asserted `MultipleHttpFilters`, update the assertion to match one of the new variants (likely `DuplicateRouterFilter` if the test used 2x Router, or extend the test to use a non-Router-named filter and assert `RouterNotTerminal` — which is harder at 07.1 since there's no non-Router HttpFilterTypedConfig variant yet; defer such tests to 07.2 OR extend the test to use the `DuplicateRouterFilter` case).

Confirm `parse_bootstrap_rejects_multiple_http_filters` (or equivalent existing test) updates to either:
- assert `DuplicateRouterFilter` (if the test built 2x Router), OR
- defer with `#[ignore]` and a doc-comment pointing at 07.2 (if the test was deliberately testing a non-Router-name multi-filter case).

- [ ] **Step 9: Run workspace-wide checks**

Run in parallel:
- `cargo build --workspace --all-targets 2>&1 | tail -5`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -5`
- `cargo fmt --all -- --check 2>&1 | tail -5`
- `cargo test --workspace 2>&1 | tail -10`
- `cargo deny check 2>&1 | tail -5`

Expected: all clean. No new top-level Cargo deps. The `parse_bootstrap` fuzz corpus's existing seeds (which include single-Router HCM shapes) continue to parse + validate clean.

- [ ] **Step 10: Append Task 4 entry to PROGRESS.md + commit**

```bash
git add crates/envoy-config/src/ \
        docs/envoy-rust/phases/07.1-filter-framework-foundation/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 07.1: task 4 — envoy-config terminal-router validator + 3 new ConfigError variants

Replaces the pre-07.1 cardinality gate at `crates/envoy-config/src/bootstrap.rs`
(lines 1335-1346 — `match hcm.http_filters.len() { 1 => check-name; n =>
MultipleHttpFilters }`) with a new free function `validate_http_filters
(filters: &[HttpFilter], listener_name: &str) -> Result<(), ConfigError>`
that enforces (a) at least one filter, (b) exactly one Router, (c) Router
at terminus, (d) name/typed_config consistency.

3 new ConfigError variants: EmptyHttpFilters { listener }, RouterNotTerminal
{ listener, position }, DuplicateRouterFilter { listener }. The pre-existing
MultipleHttpFilters variant retained per signpost 13 ledger-discipline (no
longer constructed; doc-comment notes supersession). The pre-existing
UnsupportedHttpFilter continues firing on name/typed_config mismatch.

7 new unit tests; existing `parse_bootstrap` fuzz corpus exercises the
relaxed validator unchanged. All 12 existing fixtures declare
http_filters: [Router] and pass both old and new gates. listener_name
threaded through validate_hcm's signature per signpost 8 — mirrors
06.3's Http2ClusterFromHttp1Listener listener-name-threading.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: H1 HCM 5-writer-arm refactor (factor wire-write to unified site)

**Scope:** ~210 LoC. Pure refactor — NO filter invocation yet (Task 6 layers that on top). Lands the structural change at `crates/envoy-http1/src/hcm.rs::serve_connection`: introduces a `let mut outgoing: Http1Response;` declaration above the existing writer-arm match (per signpost 4); converts each of the 5 writer arms from inline-wire-write to `outgoing = <constructed_response>; upstream_host_for_log = ...`; lifts the unified `Http1Response::write_to(&outgoing, &mut downstream).await?` to a single factored site below the match. Factors the proxy-success arm's `write_proxied_response(...)` into `construct_proxied_response(...) -> Http1Response` (constructs the value without writing the wire) + reuses the existing factored site for the wire write. The refactor is verifiable by bit-equivalence to today's wire emission under the Router-only chain (all 12 in-process backstop tests + workspace tests + Docker-gated fixtures stay green).

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` — writer-arm refactor + unified-site lift.
- Modify: `crates/envoy-http1/src/router.rs` — extract `construct_proxied_response` from `write_proxied_response`.
- Modify (maybe): `crates/envoy-http1/src/hcm.rs::tests` — 3 new unit tests asserting per-arm `Http1Response` construction; existing tests stay green.

**Important pre-Task verification.** Before touching code, read:
- `crates/envoy-http1/src/hcm.rs` lines 246-560 to understand the current `serve_connection` shape (the writer-arm match, the `response_status_for_log` / `response_body_len` / `response_headers_for_log` / `upstream_host_for_log` locals, the per-class HCM counter site landed at 06.3, the access-log dispatch site landed at 06.2).
- `crates/envoy-http1/src/router.rs` lines 75-200 to understand `write_proxied_response`'s body (06.3 D15.3.c gave it a `cluster: &ClusterHandle` parameter for the `upstream_rq_total` / `upstream_rq_5xx` increments; preserve this).

The SPEC's quoted lines `378-516` for the 5 writer arms approximate the empirical lines (which may have drifted by ±10 lines since the SPEC was written). The executor confirms the exact arm boundaries by reading the file at task start.

- [ ] **Step 1: Read the current hcm.rs shape + identify the 5 writer arms + the unified factored site**

Run:
```bash
sed -n '246,560p' crates/envoy-http1/src/hcm.rs
```

Identify (and write into PROGRESS Task 5 entry):
- Line range of the `match outcome { ... }` block.
- The 5 arm matches (synth/direct_response; proxy success; proxy send-fail 502; proxy connect-fail 502; proxy no-endpoint 503).
- The unified factored site below the match (where 06.3's per-class HCM counter fires + 06.2's access-log dispatch fires).
- The wire-write call(s) inside each arm — at Task 5 these collapse into one wire-write at the unified site.

- [ ] **Step 2: Write 3 unit tests in `crates/envoy-http1/src/router.rs::tests` for the new `construct_proxied_response`**

Mirror the existing `write_proxied_response_increments_upstream_rq_total_on_200` /
`write_proxied_response_increments_upstream_rq_5xx_on_503` tests at `crates/envoy-http1/src/router.rs:330-370` (per the existing module structure). Append:

```rust
#[tokio::test]
async fn construct_proxied_response_returns_http1response_with_upstream_status_200() {
    use envoy_http1::codec::Response;
    let cluster = make_test_cluster_handle("test-cluster");
    let upstream = test_upstream_response_200_with_custom_header();
    // construct_proxied_response: takes upstream UpstreamResponse,
    // cluster handle, elapsed_ms; returns an Http1Response value
    // (NO wire write, NO downstream &mut writer).
    let response: Response = construct_proxied_response(&cluster, upstream, /* elapsed_ms = */ 1);
    assert_eq!(response.status, 200);
    // x-envoy-upstream-service-time injected by construct_proxied_response.
    assert!(response.headers.iter().any(|(n, v)| n.eq_ignore_ascii_case("x-envoy-upstream-service-time") && v == "1"));
    // content-length injected from body.len().
    let body_len = response.body.len().to_string();
    assert!(response.headers.iter().any(|(n, v)| n.eq_ignore_ascii_case("content-length") && v == &body_len));
}

#[tokio::test]
async fn construct_proxied_response_increments_upstream_rq_total() {
    let cluster = make_test_cluster_handle("test-cluster");
    let upstream = test_upstream_response_200_with_custom_header();
    let _response = construct_proxied_response(&cluster, upstream, 1);
    assert_eq!(cluster.upstream_rq_total().get(), 1);
    assert_eq!(cluster.upstream_rq_5xx().get(), 0);
}

#[tokio::test]
async fn construct_proxied_response_increments_upstream_rq_5xx_on_503() {
    let cluster = make_test_cluster_handle("test-cluster");
    let upstream = test_upstream_response_503();
    let _response = construct_proxied_response(&cluster, upstream, 5);
    assert_eq!(cluster.upstream_rq_total().get(), 1);
    assert_eq!(cluster.upstream_rq_5xx().get(), 1);
}
```

(The `make_test_cluster_handle` / `test_upstream_response_200_with_custom_header` / `test_upstream_response_503` helpers are reused from the existing tests at lines 330-370. The exact helper names may differ — check the existing tests at PLAN-write-time and reuse the existing names.)

- [ ] **Step 3: Run tests to verify FAIL with "construct_proxied_response not defined"**

Run: `cargo test -p envoy-http1 construct_proxied_response 2>&1 | tail -10`

Expected: FAIL with `error[E0425]: cannot find function 'construct_proxied_response' in this scope`.

- [ ] **Step 4: Factor `construct_proxied_response` out of `write_proxied_response` in `router.rs`**

Take the existing `write_proxied_response` body and split it:
- Everything that builds the `Http1Response` value (response status from upstream; content-length injection; x-envoy-upstream-service-time injection; cluster.upstream_rq_total + upstream_rq_5xx increments) → moves into `construct_proxied_response` returning `Http1Response`.
- The `Http1Response::write_to(&downstream, ...)` wire-write call → REMOVED from `write_proxied_response` (or `write_proxied_response` becomes a thin wrapper `construct_proxied_response(...).write_to(downstream).await`).

```rust
/// Construct the proxied response value WITHOUT writing it to the wire.
///
/// Mirrors the body of pre-07.1 `write_proxied_response` minus the
/// wire-write call. Used by the unified factored site at
/// `crates/envoy-http1/src/hcm.rs::serve_connection` after the
/// writer-arm match populates `outgoing`.
///
/// 06.3 D15.3.c: increments `cluster.upstream_rq_total` always, and
/// `cluster.upstream_rq_5xx` when `upstream_response.status / 100 == 5`.
pub fn construct_proxied_response(
    cluster: &envoy_cluster::ClusterHandle,
    upstream_response: envoy_http1::codec::Response,
    elapsed_ms: u128,
) -> envoy_http1::codec::Response {
    cluster.upstream_rq_total().inc();
    if upstream_response.status / 100 == 5 {
        cluster.upstream_rq_5xx().inc();
    }
    let mut headers = upstream_response.headers.clone();
    // Replace upstream's content-length (RFC 7230 §3.3.3 — proxy
    // policy: re-frame body bytes count rather than trust upstream's
    // header).
    headers.retain(|(n, _)| !n.eq_ignore_ascii_case("content-length"));
    headers.retain(|(n, _)| !n.eq_ignore_ascii_case("transfer-encoding"));
    headers.push((
        "content-length".to_string(),
        upstream_response.body.len().to_string(),
    ));
    headers.push((
        "x-envoy-upstream-service-time".to_string(),
        elapsed_ms.to_string(),
    ));
    envoy_http1::codec::Response {
        status: upstream_response.status,
        headers,
        body: upstream_response.body,
    }
}

/// Pre-07.1 helper: construct + write the proxied response in one call.
///
/// Retained at Task 5 as a thin wrapper for backward-compat with any
/// external caller; removable if grep shows no callers outside hcm.rs.
pub async fn write_proxied_response<W>(
    downstream: &mut W,
    cluster: &envoy_cluster::ClusterHandle,
    upstream_response: envoy_http1::codec::Response,
    elapsed_ms: u128,
    close: bool,
) -> Result<(), crate::Http1Error>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    let response = construct_proxied_response(cluster, upstream_response, elapsed_ms);
    envoy_http1::codec::Http1Response::write_to(&response, downstream, close).await
}
```

Verify no external (outside `crates/envoy-http1/src/`) callers of `write_proxied_response` exist:

```bash
grep -rn "write_proxied_response" --include="*.rs" | grep -v "^crates/envoy-http1/src/"
```

If empty: remove `write_proxied_response` outright (replace all uses inside `hcm.rs` with the `construct_proxied_response` + Step 5's unified wire-write call). If non-empty: keep `write_proxied_response` as the thin wrapper.

- [ ] **Step 5: Refactor the 5 writer arms in `crates/envoy-http1/src/hcm.rs::serve_connection`**

At the per-request loop body's writer-arm match, perform the following sequence of edits (preserving existing semantics for the access-log + per-class counter sites):

**(a) Declare `let mut outgoing: Http1Response;` immediately above the match block.** Place it where today's per-arm `response_status_for_log = ...; response_headers_for_log = ...; response_body_len = ...; upstream_host_for_log = ...` lines start.

**(b) Convert each writer arm:**

Arm 1 — Synth (direct_response):

```rust
// BEFORE (Task 4 / HEAD):
BuildOutcome::Synth(resp) => {
    response_status_for_log = resp.status;
    response_headers_for_log = resp.headers.clone();
    response_body_len = resp.body.len() as u64;
    upstream_host_for_log = None;
    Http1Response::write_to(&resp, &mut downstream, close).await?;
    if close { break; }
}

// AFTER (Task 5):
BuildOutcome::Synth(resp) => {
    outgoing = resp;
    upstream_host_for_log = None;
}
```

Arm 2 — Proxy success:

```rust
// BEFORE:
BuildOutcome::Proxy { upstream_response, cluster, elapsed_ms } => {
    crate::router::write_proxied_response(
        &mut downstream,
        &cluster,
        upstream_response,
        elapsed_ms,
        close,
    ).await?;
    response_status_for_log = /* read back from upstream_response */;
    // ...
}

// AFTER:
BuildOutcome::Proxy { upstream_response, cluster, elapsed_ms } => {
    outgoing = crate::router::construct_proxied_response(
        &cluster,
        upstream_response,
        elapsed_ms,
    );
    upstream_host_for_log = Some(cluster.upstream_host_string());
}
```

(The exact `upstream_host_for_log` source — `cluster.upstream_host()` vs a `SocketAddr` Display — comes from the existing 06.3 site; preserve verbatim.)

Arms 3-5 — Synth-502 (send-fail) / synth-502 (connect-fail) / synth-503 (no-endpoint):

```rust
// AFTER (mirror Arm 1's shape):
BuildOutcome::SynthSendFail502 => {
    let resp = build_synth_502_send_fail();  // existing helper
    outgoing = resp;
    upstream_host_for_log = None;
}
BuildOutcome::SynthConnectFail502 => {
    let resp = build_synth_502_connect_fail();  // existing helper
    outgoing = resp;
    upstream_host_for_log = None;
}
BuildOutcome::SynthNoEndpoint503 => {
    let resp = build_synth_503_no_endpoint();  // existing helper
    outgoing = resp;
    upstream_host_for_log = None;
}
```

(The exact `BuildOutcome` variant names and `build_synth_NNN` helper names come from existing hcm.rs code; preserve verbatim.)

**(c) Lift the wire-write to the unified factored site (below the match block):**

```rust
// AFTER the writer-arm match, BEFORE the per-class HCM counter site:

// (Task 6 will insert pipeline.encode_headers(&mut outgoing) here.)

// Re-populate per-arm-derived locals from `outgoing` for access-log /
// per-class counter / close-flag computation.
response_status_for_log = outgoing.status;
response_headers_for_log = outgoing.headers.clone();
response_body_len = outgoing.body.len() as u64;

// Existing 06.3 per-class HCM counter site (unchanged):
config.stats.downstream_rq_total.inc();
match response_status_for_log / 100 {
    2 => config.stats.downstream_rq_2xx.inc(),
    3 => config.stats.downstream_rq_3xx.inc(),
    4 => config.stats.downstream_rq_4xx.inc(),
    5 => config.stats.downstream_rq_5xx.inc(),
    _ => {}
}

// NEW — unified wire-write (lifted out of each arm):
Http1Response::write_to(&outgoing, &mut downstream, close).await?;

// Existing 06.2 access-log dispatch site (unchanged; reads
// response_status_for_log / response_headers_for_log / response_body_len /
// upstream_host_for_log populated above).
// ...

if close { break; }
```

**(d) Verify the close-flag bookkeeping is preserved.** The pre-07.1 arms each had a `if close { break; }` after the wire-write; the unified site preserves this exactly once below the unified wire-write. If any arm had pre-existing close-flag bookkeeping (e.g., the synth arms had `if close { break; }` but the proxy success arm had `if close { break; }` AFTER `write_proxied_response`), normalize: the unified site has one `if close { break; }` after the unified wire-write.

- [ ] **Step 6: Run tests to verify everything stays green (Task 5 is a pure refactor)**

Run in parallel:
- `cargo test -p envoy-http1 2>&1 | tail -20`
- `cargo test --workspace 2>&1 | tail -10`

Expected: all pre-existing envoy-http1 tests stay green + the 3 new `construct_proxied_response_*` tests PASS. **Critically:** the in-process backstop tests at `crates/envoy-bin/tests/http1_*.rs` + `crates/envoy-bin/tests/http2_*.rs` continue to pass — these exercise the actual serve_connection paths against real driver requests and would catch any regression in the wire-emitted bytes.

```
test result: ok. <N + 3> passed; 0 failed; 0 ignored
```

If ANY test fails, the refactor introduced a wire-emission regression. Most likely failure modes:
- Wrong order of operations at the unified site (e.g., wire-write before per-class counter increment instead of after).
- Missing `if close { break; }` after the unified wire-write.
- `upstream_host_for_log` populated wrong (e.g., not preserving `Some(cluster.upstream_host_string())` on the proxy-success arm).
- `response_status_for_log` etc. mis-typed (`u16` vs `u32`).

Diagnose by running specific failing tests with `--nocapture` and comparing wire output to pre-Task-5 baseline.

- [ ] **Step 7: Run workspace-wide checks + Docker-gated in-process surrogate**

Run in parallel:
- `cargo build --workspace --all-targets 2>&1 | tail -5`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -5`
- `cargo fmt --all -- --check 2>&1 | tail -5`
- `cargo test --workspace 2>&1 | tail -10`
- `cargo deny check 2>&1 | tail -5`

Expected: all clean.

**Bilateral attestation discipline (per architecture decision + 06.3 REVIEW I1 closure):** Task 5's PROGRESS entry MUST enumerate which test buckets ran with attestation:
- workspace tests: `cargo test --workspace` — PASS (count: N; commit at SHA)
- clippy/fmt/deny: clean
- in-process backstop tests for fixtures 0001-0012 at `crates/envoy-bin/tests/*.rs`: PASS (these are the in-process surrogate for the Docker-gated runs; Task 8 anchors the actual Docker-gated CI run)
- in-process H2 backstop tests at `crates/envoy-bin/tests/http2_*.rs`: PASS (Task 5 only touches H1 hcm; H2 surface unchanged — PASS expected)

- [ ] **Step 8: Append Task 5 entry to PROGRESS.md + commit**

```bash
git add crates/envoy-http1/src/ \
        docs/envoy-rust/phases/07.1-filter-framework-foundation/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 07.1: task 5 — H1 HCM 5-writer-arm refactor (factor wire-write to unified site)

Pure refactor — NO filter invocation yet (Task 6 layers that on top).

`crates/envoy-http1/src/hcm.rs::serve_connection` now declares
`let mut outgoing: Http1Response;` above the writer-arm match (per
signpost 4 + 5 — uninitialized let-then-assign). Each of the 5 writer
arms (synth direct_response; proxy success; proxy send-fail 502; proxy
connect-fail 502; proxy no-endpoint 503) populates `outgoing` and
`upstream_host_for_log` without writing the wire. Below the match:
unified `Http1Response::write_to(&outgoing, &mut downstream, close)`
fires once; the per-class HCM counter site (06.3) and access-log
dispatch site (06.2) read `outgoing` to populate the per-arm-derived
locals.

`crates/envoy-http1/src/router.rs` factors `construct_proxied_response`
out of `write_proxied_response` (same body minus the wire-write step;
returns Http1Response value). The cluster-side `upstream_rq_total` /
`upstream_rq_5xx` increments (06.3 D15.3.c) move into
`construct_proxied_response` so they fire once per construction
regardless of how the response is subsequently written. `write_proxied_response`
retained as thin wrapper (or removed if no callers outside hcm.rs).

Bit-equivalent wire emission verified by 12-fixture in-process backstop
+ workspace tests. Task 8 anchors the Docker-gated CI run.

3 new unit tests on construct_proxied_response. Pre-existing
write_proxied_response tests retained (or updated to call through the
thin wrapper).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: H1 HCM filter-chain decode/encode invocation

**Scope:** ~100 LoC. Layers filter invocation onto Task 5's refactor. Adds `pub filter_pipeline: Arc<FilterPipeline>` field to `HCMConfig`; adds `HCMConfig::from_config` wiring that calls `FilterPipeline::build_from_config`; adds `HCMConfigError::FilterPipeline(envoy_filter::FilterError)` variant; adds private `RequestPath` enum capturing `Match` (writer-arm path) vs `SynthFromDecode` (decode-side StopAndSend short-circuit); invokes `pipeline.decode_headers` after `parse_request` and before `build_response`; invokes `pipeline.encode_headers` at the unified factored site after the writer-arm match populates `outgoing` and before the wire write.

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` — HCMConfig field + from_config + RequestPath enum + decode invocation + encode invocation.
- Modify: `crates/envoy-http1/src/lib.rs` (or wherever `HCMConfigError` lives) — add `FilterPipeline(envoy_filter::FilterError)` variant via `#[from]`.
- Modify: `crates/envoy-http1/Cargo.toml` — add `envoy-filter = { path = "../envoy-filter" }` to `[dependencies]`.

**Per signpost 12 + parent-07 SPEC §6 Rule 2:** at 07.1 only tests 1 + 2 + 8 from 07.1 SPEC §3 Task 6 land. Tests 3-7 (filter-instrumented invocation semantics — request mutation visible to route-match, response mutation visible to wire, StopAndSend short-circuit) defer to 07.2 Task 5 (the first task that wires HeaderMutation as a non-Router filter).

- [ ] **Step 1: Add `envoy-filter` dependency to `crates/envoy-http1/Cargo.toml`**

Insert under `[dependencies]` (alphabetical between `envoy-config` and `envoy-tcp` if present, or as needed):

```toml
envoy-filter = { path = "../envoy-filter" }
```

Run `cargo build -p envoy-http1` to verify the dependency resolves.

- [ ] **Step 2: Add `HCMConfigError::FilterPipeline` variant**

Locate `HCMConfigError` (likely at `crates/envoy-http1/src/lib.rs` or `crates/envoy-http1/src/hcm.rs`). Append a new variant:

```rust
/// 07.1 Task 6: `FilterPipeline::build_from_config` rejected the
/// http_filters list (most likely empty list — but the validator at
/// `envoy_config::validate_http_filters` (Task 4) catches this earlier
/// at config-load time; this variant is defense-in-depth at HCMConfig
/// construction).
#[error("filter pipeline build failed: {0}")]
FilterPipeline(#[from] envoy_filter::FilterError),
```

- [ ] **Step 3: Add `filter_pipeline: Arc<FilterPipeline>` field to `HCMConfig`**

Edit the `HCMConfig` struct definition:

```rust
pub struct HCMConfig {
    // ... existing fields ...

    /// 07.1 Task 6: per-listener filter pipeline.
    ///
    /// Arc-shared at config-build time. Each per-request scope inside
    /// `serve_connection` clones into a working `FilterPipeline` via
    /// `(*config.filter_pipeline).clone()`. At 07.1 the per-request
    /// clone is effectively a no-op (Router is zero-state). 07.2's
    /// HeaderMutation per-stream cloning shares this structural shape.
    pub filter_pipeline: std::sync::Arc<envoy_filter::FilterPipeline>,
}
```

Update `HCMConfig::from_config` (or whichever constructor builds `HCMConfig` from `envoy_config::HttpConnectionManagerConfig`):

```rust
impl HCMConfig {
    pub fn from_config(
        cfg: &envoy_config::HttpConnectionManagerConfig,
        // ... other existing params ...
    ) -> Result<Self, HCMConfigError> {
        // ... existing setup ...

        let pipeline = envoy_filter::FilterPipeline::build_from_config(&cfg.http_filters)?;

        Ok(Self {
            // ... existing fields ...
            filter_pipeline: std::sync::Arc::new(pipeline),
        })
    }
}
```

Update all callers of `HCMConfig::from_config` (likely in `crates/envoy-bin/src/main.rs` and any test-construction helpers in `hcm.rs::tests`). Test helpers that build HCMConfig directly (bypassing `from_config`) need to populate the new field — likely:

```rust
fn test_hcm_config() -> HCMConfig {
    HCMConfig {
        // ... existing test fields ...
        filter_pipeline: std::sync::Arc::new(
            envoy_filter::FilterPipeline::build_from_config(&vec![
                envoy_config::HttpFilter {
                    name: "envoy.filters.http.router".to_string(),
                    typed_config: envoy_config::HttpFilterTypedConfig::Router(
                        envoy_config::RouterConfig {},
                    ),
                },
            ]).expect("single Router builds"),
        ),
    }
}
```

- [ ] **Step 4: Add the private `RequestPath` enum to hcm.rs**

Add near the top of `serve_connection` or at module scope (private):

```rust
/// Per-request dispatch path.
///
/// `Match` — the request passed through `pipeline.decode_headers` and
/// hit the writer-arm match via `build_response`.
/// `SynthFromDecode` — a decode-side filter short-circuited the request
/// with `StopAndSend`; the response goes directly to the unified site
/// without consulting the writer arms or `build_response`.
enum RequestPath {
    Match(BuildOutcome),
    SynthFromDecode(envoy_http1::codec::Response),
}
```

(Adjust `Response` import to match the existing hcm.rs import path; likely `crate::codec::Response` or `Response` if already imported.)

- [ ] **Step 5: Wire `decode_headers` invocation after `parse_request`**

At the per-request loop body in `serve_connection`, locate the existing call to `build_response(&config, &req, close)`. Restructure:

```rust
// BEFORE (Task 5 / HEAD):
let mut req = parse_request(/* ... */).await?;
let outcome = build_response(&config, &req, close);

// AFTER (Task 6):
let mut req = parse_request(/* ... */).await?;
let mut pipeline = (*config.filter_pipeline).clone();
let decode_decision = pipeline.decode_headers(&mut req);
let request_path = match decode_decision {
    envoy_filter::Decision::Continue => {
        RequestPath::Match(build_response(&config, &req, close))
    }
    envoy_filter::Decision::StopAndSend(resp) => {
        RequestPath::SynthFromDecode(resp)
    }
};
```

**Note on `mut req`.** `parse_request` currently returns `Request` by value (not `&mut`). Hoist to `let mut req = parse_request(...).await?;` so `pipeline.decode_headers(&mut req)` can mutate the headers list / path. The existing `build_response(&config, &req, close)` call accepts `&Request` so no change there.

- [ ] **Step 6: Wire `encode_headers` invocation at the unified site + plumb `RequestPath` through the writer-arm match**

At the unified factored site established by Task 5 (between the writer-arm match and the wire write), restructure:

```rust
// BEFORE (Task 5):
let mut outgoing: Http1Response;
match outcome {
    BuildOutcome::Synth(resp) => { outgoing = resp; upstream_host_for_log = None; }
    BuildOutcome::Proxy { upstream_response, cluster, elapsed_ms } => {
        outgoing = crate::router::construct_proxied_response(
            &cluster, upstream_response, elapsed_ms,
        );
        upstream_host_for_log = Some(cluster.upstream_host_string());
    }
    // ... synth-502/503 arms ...
}
// (per-class counter site + wire write + access-log dispatch follow)

// AFTER (Task 6):
let mut outgoing: Http1Response;
match request_path {
    RequestPath::Match(outcome) => match outcome {
        BuildOutcome::Synth(resp) => { outgoing = resp; upstream_host_for_log = None; }
        BuildOutcome::Proxy { upstream_response, cluster, elapsed_ms } => {
            outgoing = crate::router::construct_proxied_response(
                &cluster, upstream_response, elapsed_ms,
            );
            upstream_host_for_log = Some(cluster.upstream_host_string());
        }
        // ... synth-502/503 arms unchanged ...
    },
    RequestPath::SynthFromDecode(resp) => {
        outgoing = resp;
        upstream_host_for_log = None;
    }
}

// 07.1 Task 6 NEW: encode-side filter invocation.
if let envoy_filter::Decision::StopAndSend(replacement) =
    pipeline.encode_headers(&mut outgoing)
{
    outgoing = replacement;
}

// Re-populate per-arm-derived locals from `outgoing` AFTER encode-side
// filter invocation (so post-encode mutations reach the access log /
// per-class counter).
response_status_for_log = outgoing.status;
response_headers_for_log = outgoing.headers.clone();
response_body_len = outgoing.body.len() as u64;

// (per-class counter site + wire write + access-log dispatch unchanged
// from Task 5; they now read `response_status_for_log` etc. populated
// AFTER the encode-side filter invocation.)
```

**Iteration-protocol invariant.** `encode_headers` fires once per response, regardless of whether the decode side issued `StopAndSend`. This matches Envoy v1.33's semantic (encode runs on every response; the framework guarantees one encode pass). At 07.1 with Router-only, the encode call is a no-op; the structural shape is forward-compat for 07.2.

- [ ] **Step 7: Write 3 unit tests (the ones not deferred to 07.2 per signpost 12)**

Add to `crates/envoy-http1/src/hcm.rs::tests`:

```rust
#[test]
fn hcm_config_from_config_builds_filter_pipeline() {
    let envoy_cfg = envoy_config::HttpConnectionManagerConfig {
        // ... minimal valid config with http_filters: [Router] ...
        http_filters: vec![envoy_config::HttpFilter {
            name: "envoy.filters.http.router".to_string(),
            typed_config: envoy_config::HttpFilterTypedConfig::Router(
                envoy_config::RouterConfig {},
            ),
        }],
        // ... other required fields ...
    };
    let hcm_cfg = HCMConfig::from_config(&envoy_cfg /* + other params */)
        .expect("HCMConfig::from_config succeeds with single Router");
    // Verify the filter_pipeline was constructed.
    // (No public accessor to filters.len(); verify via Arc clone shape.)
    let _cloned = (*hcm_cfg.filter_pipeline).clone();
}

#[test]
fn hcm_config_from_config_errors_on_empty_http_filters() {
    let envoy_cfg = envoy_config::HttpConnectionManagerConfig {
        http_filters: Vec::new(),
        // ... other required fields ...
    };
    let result = HCMConfig::from_config(&envoy_cfg /* + other params */);
    match result {
        Err(HCMConfigError::FilterPipeline(envoy_filter::FilterError::EmptyChain)) => {}
        other => panic!(
            "expected FilterPipeline(EmptyChain), got {other:?}",
        ),
    }
}

#[tokio::test]
async fn serve_connection_regression_equivalent_under_router_only_chain() {
    // Regression-equivalence: drive a request through serve_connection
    // with a single-Router filter chain; assert the wire output is
    // byte-equivalent to the pre-Task-6 wire output.
    //
    // The existing in-process backstop tests at
    // `crates/envoy-bin/tests/http1_*.rs` already exercise this at the
    // integration-test level. This unit test is a sanity check at the
    // crate boundary.
    //
    // Setup: HCMConfig with http_filters: [Router], a direct_response route
    // returning 418 with body "hello".
    let hcm_cfg = test_hcm_config_with_direct_response_418_hello();
    let (client, server) = tokio::io::duplex(8192);
    let (mut client_read, mut client_write) = tokio::io::split(client);
    // Send a minimal GET request:
    use tokio::io::AsyncWriteExt;
    client_write.write_all(b"GET / HTTP/1.1\r\nhost: localhost\r\n\r\n").await.unwrap();
    client_write.shutdown().await.unwrap();
    // Drive serve_connection on the server side.
    let _ = serve_connection(server, std::sync::Arc::new(hcm_cfg), /* peer = */ "127.0.0.1:0".parse().unwrap()).await;
    // Read the wire output.
    use tokio::io::AsyncReadExt;
    let mut wire = Vec::new();
    client_read.read_to_end(&mut wire).await.unwrap();
    let wire_str = String::from_utf8_lossy(&wire);
    assert!(wire_str.starts_with("HTTP/1.1 418"));
    assert!(wire_str.contains("hello"));
}
```

**Note:** the exact `serve_connection` signature + `test_hcm_config_with_direct_response_418_hello` helper come from existing test patterns in `hcm.rs::tests`; the executor adapts at task-start.

Tests 3-7 (decode-side mutation visible to route-match; encode-side mutation visible to wire; StopAndSend short-circuit on both sides; access-log reflects post-encode headers) defer to 07.2 Task 5 per signpost 12.

- [ ] **Step 8: Run tests + workspace-wide checks**

Run in parallel:
- `cargo test -p envoy-http1 2>&1 | tail -20`
- `cargo test --workspace 2>&1 | tail -10`
- `cargo build --workspace --all-targets 2>&1 | tail -5`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -5`
- `cargo fmt --all -- --check 2>&1 | tail -5`

Expected: all clean. Pre-existing in-process backstop tests at `crates/envoy-bin/tests/http1_*.rs` continue passing (Router-only chain is regression-equivalent to pre-Task-6 behavior).

Test-bucket attestation in PROGRESS Task 6 entry:
- workspace tests: PASS (count: N; commit at SHA)
- clippy / fmt / deny: clean
- in-process backstop tests for fixtures 0001-0012: PASS (regression baseline preserved)

- [ ] **Step 9: Append Task 6 entry to PROGRESS.md + commit**

```bash
git add crates/envoy-http1/ \
        crates/envoy-bin/src/main.rs \
        docs/envoy-rust/phases/07.1-filter-framework-foundation/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 07.1: task 6 — H1 HCM filter-chain decode/encode invocation

Layers filter invocation onto Task 5's refactor.

`crates/envoy-http1/Cargo.toml` adds `envoy-filter` path-dep.
`HCMConfig` gains `filter_pipeline: Arc<FilterPipeline>` field.
`HCMConfig::from_config` calls `FilterPipeline::build_from_config` and
wraps in `Arc::new`. `HCMConfigError::FilterPipeline(envoy_filter::FilterError)`
variant via `#[from]`.

Private `RequestPath` enum at hcm.rs module scope captures the
`Match(BuildOutcome)` (writer-arm path) vs `SynthFromDecode(Response)`
(decode-side StopAndSend) dispatch. Per-request flow per signpost 14:
parse_request → clone pipeline → decode_headers → match Decision
(Continue → build_response → RequestPath::Match; StopAndSend →
RequestPath::SynthFromDecode) → writer-arm match populates `outgoing`
→ encode_headers (post-arm) → unified wire-write → access-log dispatch.

3 unit tests at 07.1 scope: HCMConfig::from_config success path,
HCMConfigError::FilterPipeline error path, regression-equivalence
under Router-only chain. Tests 3-7 (filter-instrumented invocation
semantics) deferred to 07.2 Task 5 per signpost 12; 07.1's
regression-equivalence at state-4 IS the no-behavior-regression test.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: H2 HCM `finalize_h2_stream` refactor + filter-chain invocation

**Scope:** ~170 LoC. Symmetric to Task 6 at H2: adds `pipeline: &mut FilterPipeline` parameter to `finalize_h2_stream`; wires `decode_headers` invocation in `handle_one_stream` after `http_to_envoy_request` translation and before `build_response`; wires `encode_headers` invocation inside `finalize_h2_stream` before the per-class HCM counter site + `send_envoy_response`. HCMConfig is type-aliased to `envoy_http1::HCMConfig` so the `filter_pipeline` field added by Task 6 is automatically present. All 3 callers of `finalize_h2_stream` inside `crates/envoy-http2/src/hcm.rs` update at this task.

**Files:**
- Modify: `crates/envoy-http2/Cargo.toml` — add `envoy-filter = { path = "../envoy-filter" }` to `[dependencies]` (if not already pulled transitively via `envoy-http1`).
- Modify: `crates/envoy-http2/src/hcm.rs` — add private `H2RequestPath` enum; wire decode_headers in `handle_one_stream`; add `pipeline: &mut FilterPipeline` param to `finalize_h2_stream`; wire encode_headers inside `finalize_h2_stream`; update all 3 callers.

**Per signpost 12:** at 07.1 only test 5 (regression-equivalence on existing H2 fixtures 0009 + 0010) lands. Tests 1-4 (filter-instrumented invocation semantics at H2) defer to 07.2 Task 5.

- [ ] **Step 1: Add `envoy-filter` dependency to `crates/envoy-http2/Cargo.toml`**

Insert under `[dependencies]` (alphabetical):

```toml
envoy-filter = { path = "../envoy-filter" }
```

Run `cargo build -p envoy-http2` to verify.

- [ ] **Step 2: Read the current `handle_one_stream` + `finalize_h2_stream` shape**

Run:
```bash
sed -n '88,180p' crates/envoy-http2/src/hcm.rs    # handle_one_stream
sed -n '360,440p' crates/envoy-http2/src/hcm.rs   # finalize_h2_stream
```

Identify:
- The `http_to_envoy_request(h2_req)` call site in `handle_one_stream` (the H2→envoy adapter from 05.2 D3).
- The `build_response(&config, &envoy_req, /* close = */ false)` call site at line ~127.
- The current signature of `finalize_h2_stream` (which parameters it takes today — likely `send_response`, `resp`, `hcm_stats`, `cluster_stats_opt`, `access_log_dispatch`, `start`, ...).
- All 3 call sites of `finalize_h2_stream` inside `hcm.rs` (the 3 H2 writer paths — synth direct_response; proxy success/H2-arm; synth-502/503 from build_response).
- The per-class HCM counter site inside `finalize_h2_stream` at line ~380 (landed at 06.3).
- The `send_envoy_response(send_response, resp).await?` wire-write call inside `finalize_h2_stream`.

- [ ] **Step 3: Add the private `H2RequestPath` enum**

Add near the top of `handle_one_stream` or at module scope:

```rust
/// Per-stream dispatch path (H2 mirror of H1's `RequestPath`).
///
/// `Match` — the H2 stream's translated request went through
/// `pipeline.decode_headers` and hit the writer-arm path via
/// `build_response`.
/// `SynthFromDecode` — a decode-side filter short-circuited the request
/// with `StopAndSend`; the response goes directly to `finalize_h2_stream`.
enum H2RequestPath {
    Match(BuildOutcome),
    SynthFromDecode(envoy_http1::codec::Response),
}
```

- [ ] **Step 4: Wire `decode_headers` invocation in `handle_one_stream`**

At the `handle_one_stream` body, locate the `http_to_envoy_request` call site (around line ~120) and the existing `build_response` call (line ~127). Restructure:

```rust
// BEFORE (Task 6 / HEAD):
let mut envoy_req = http_to_envoy_request(h2_req)?;
let outcome = build_response(&config, &envoy_req, /* close = */ false);
// (then dispatch to one of 3 paths that all call finalize_h2_stream)

// AFTER (Task 7):
let mut envoy_req = http_to_envoy_request(h2_req)?;
let mut pipeline = (*config.filter_pipeline).clone();
let decode_decision = pipeline.decode_headers(&mut envoy_req);
let request_path = match decode_decision {
    envoy_filter::Decision::Continue => {
        H2RequestPath::Match(build_response(&config, &envoy_req, /* close = */ false))
    }
    envoy_filter::Decision::StopAndSend(resp) => {
        H2RequestPath::SynthFromDecode(resp)
    }
};
```

- [ ] **Step 5: Add `pipeline: &mut FilterPipeline` parameter to `finalize_h2_stream`**

Update the function signature:

```rust
// BEFORE:
async fn finalize_h2_stream(
    send_response: h2::server::SendResponse</* H2 stream type */>,
    resp: envoy_http1::codec::Response,
    hcm_stats: &HCMStats,
    cluster_stats_opt: Option<&envoy_cluster::Cluster>,
    access_log_dispatch: AccessLogDispatch,
    start: std::time::Instant,
    // ... other existing params ...
) -> Result<(), Http2Error> {
    // ...
}

// AFTER (added pipeline: &mut FilterPipeline; resp is now `mut`):
async fn finalize_h2_stream(
    send_response: h2::server::SendResponse</* H2 stream type */>,
    pipeline: &mut envoy_filter::FilterPipeline,
    mut resp: envoy_http1::codec::Response,
    hcm_stats: &HCMStats,
    cluster_stats_opt: Option<&envoy_cluster::Cluster>,
    access_log_dispatch: AccessLogDispatch,
    start: std::time::Instant,
    // ... other existing params ...
) -> Result<(), Http2Error> {
    // 07.1 Task 7 NEW: encode-side filter invocation.
    if let envoy_filter::Decision::StopAndSend(replacement) =
        pipeline.encode_headers(&mut resp)
    {
        resp = replacement;
    }

    // Existing 06.3 per-class HCM counter site (now reads post-encode resp.status):
    hcm_stats.downstream_rq_total.inc();
    match resp.status / 100 {
        2 => hcm_stats.downstream_rq_2xx.inc(),
        3 => hcm_stats.downstream_rq_3xx.inc(),
        4 => hcm_stats.downstream_rq_4xx.inc(),
        5 => hcm_stats.downstream_rq_5xx.inc(),
        _ => {}
    }

    // Existing wire-write:
    send_envoy_response(send_response, resp).await?;

    // Existing 06.2 access-log dispatch site (unchanged):
    // ...
    Ok(())
}
```

- [ ] **Step 6: Update all 3 callers of `finalize_h2_stream`**

Find all 3 call sites:
```bash
grep -n "finalize_h2_stream(" crates/envoy-http2/src/hcm.rs
```

Each call site adds `&mut pipeline` as the second argument. Wrap the dispatch with the `request_path` match:

```rust
// BEFORE (3 paths each calling finalize_h2_stream directly with `resp`):
let resp = match outcome {
    BuildOutcome::Synth(r) => r,
    BuildOutcome::Proxy { /* ... H2-arm: builds proxy_resp inline ... */ } => proxy_resp,
    BuildOutcome::SynthSendFail502 | ... => /* synth */,
};
finalize_h2_stream(send_response, resp, &hcm_stats, /* ... */).await?;

// AFTER (request_path determines whether we ran build_response or short-circuited):
let resp = match request_path {
    H2RequestPath::Match(outcome) => match outcome {
        BuildOutcome::Synth(r) => r,
        BuildOutcome::Proxy { /* ... H2-arm builds proxy_resp inline ... */ } => proxy_resp,
        BuildOutcome::SynthSendFail502 | ... => /* synth */,
    },
    H2RequestPath::SynthFromDecode(r) => r,
};
finalize_h2_stream(send_response, &mut pipeline, resp, &hcm_stats, /* ... */).await?;
```

- [ ] **Step 7: Write 1 regression-equivalence unit test (test 5 from 07.1 SPEC §3 Task 7)**

Add to `crates/envoy-http2/src/hcm.rs::tests`:

```rust
#[tokio::test]
async fn h2_serve_regression_equivalent_under_router_only_chain() {
    // Regression-equivalence at the in-process backstop level: drive
    // an H2 request through the HCM with single-Router filter chain;
    // assert the response is byte-equivalent to pre-Task-7 wire
    // emission.
    //
    // The existing fixtures 0009 (H2 direct_response) and 0010 (H2
    // router-upstream) already exercise this at the in-process backstop
    // + Docker-gated level. This unit test is a sanity check at the
    // crate boundary.
    //
    // Setup mirrors `crates/envoy-bin/tests/http2_direct_response.rs`
    // (the 05.2 D9 in-process backstop test).
    //
    // ... test body ...
}
```

If the existing `crates/envoy-bin/tests/http2_direct_response.rs` + `crates/envoy-bin/tests/http2_router_upstream.rs` already cover this surface (they do), the additional unit test is OPTIONAL — the planner may skip it and rely on the integration-test surrogate. **Recommended posture (per signpost 12): SKIP** the new unit test; rely on the existing in-process backstop tests + workspace tests + Task 8's Docker-gated CI run as the regression-equivalence proof. PROGRESS Task 7 entry notes the test was deliberately skipped per signpost 12 + the existing coverage.

Tests 1-4 (filter-instrumented invocation semantics at H2) defer to 07.2 Task 5 per signpost 12.

- [ ] **Step 8: Run tests + workspace-wide checks**

Run in parallel:
- `cargo test -p envoy-http2 2>&1 | tail -10`
- `cargo test --workspace 2>&1 | tail -10`
- `cargo build --workspace --all-targets 2>&1 | tail -5`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -5`
- `cargo fmt --all -- --check 2>&1 | tail -5`

Expected: all clean. Pre-existing H2 tests (workspace tests + `crates/envoy-bin/tests/http2_*.rs` in-process backstops) continue passing under the Router-only chain (regression-equivalent to pre-Task-7).

Test-bucket attestation in PROGRESS Task 7 entry:
- workspace tests: PASS (count: N; commit at SHA)
- clippy / fmt / deny: clean
- in-process backstop tests at `crates/envoy-bin/tests/http2_direct_response.rs` + `crates/envoy-bin/tests/http2_router_upstream.rs`: PASS (regression baseline preserved)
- All 12 in-process surrogate tests for fixtures 0001-0012: PASS (covers BOTH H1 and H2 surfaces — Task 5/6/7 do not regress any wire emission under the Router-only chain)

- [ ] **Step 9: Append Task 7 entry to PROGRESS.md + commit**

```bash
git add crates/envoy-http2/ \
        docs/envoy-rust/phases/07.1-filter-framework-foundation/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 07.1: task 7 — H2 HCM finalize_h2_stream refactor + filter-chain invocation

Symmetric to Task 6 at H2.

`crates/envoy-http2/Cargo.toml` adds `envoy-filter` path-dep.

handle_one_stream wires decode-side filter invocation: after
`http_to_envoy_request(h2_req)` translates the H2 HEADERS frame to
envoy_http1::codec::Request, clones `(*config.filter_pipeline)` and
runs `pipeline.decode_headers(&mut envoy_req)`. On Continue: dispatch
to build_response per 05.2 D3. On StopAndSend(resp): bypass
build_response, feed resp directly to finalize_h2_stream via
H2RequestPath::SynthFromDecode.

finalize_h2_stream gains `pipeline: &mut FilterPipeline` parameter
(per signpost 6); changes `resp` from immutable to `mut`; inserts
`pipeline.encode_headers(&mut resp)` BEFORE the existing 06.3
per-class HCM counter site so the counter and the wire emission
both reflect post-encode response state. All 3 callers of
finalize_h2_stream inside hcm.rs update at this task.

Private `H2RequestPath { Match(BuildOutcome), SynthFromDecode(Response) }`
enum at module scope (parallel to H1's RequestPath per signpost 10 —
per-HCM separate types, NOT unified).

Regression-equivalence verified by workspace tests + in-process
backstops at `crates/envoy-bin/tests/http2_direct_response.rs` and
`http2_router_upstream.rs`. Tests 1-4 (filter-instrumented invocation
semantics at H2) deferred to 07.2 Task 5 per signpost 12.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: State-4 verification (12 fixtures simultaneously green)

**Scope:** ~30 LoC (PROGRESS-only). No code changes. Pushes the branch to GitHub at HEAD = Task 7's commit (or a Task-7-equivalent state); triggers the Docker-gated CI workflow; captures the run URL + run ID + conclusion + completion timestamp; verifies the §7.5 phase-done gate end-to-end.

**Files:**
- Modify: `docs/envoy-rust/phases/07.1-filter-framework-foundation/PROGRESS.md` — Task 8 state-4 evidence entry.

- [ ] **Step 1: Push the branch to GitHub**

```bash
git push origin HEAD
```

(Assumes a feature branch is in use; if working directly on `main` per the project's typical cadence, push to `main`. The CI workflow's trigger is determined by the project's `.github/workflows/ci.yml`; confirm at task-start.)

- [ ] **Step 2: Trigger / monitor the Docker-gated CI workflow**

Identify the latest CI run for HEAD:

```bash
gh run list --workflow=ci.yml --branch=main --limit=5
```

Capture:
- **Run URL:** `https://github.com/<org>/<repo>/actions/runs/<run_id>`
- **Run ID:** numeric.
- **HEAD SHA:** matches the Task 7 commit SHA.
- **Workflow file:** `ci.yml` (or equivalent).

If no run was auto-triggered by the push, manually trigger:

```bash
gh workflow run ci.yml --ref main
```

Then list to find the new run.

- [ ] **Step 3: Wait for completion + verify conclusion**

```bash
gh run watch <run_id>
```

Or poll until `status = completed`:

```bash
gh run view <run_id> --json status,conclusion,createdAt,updatedAt
```

Expected: `status = "completed"`, `conclusion = "success"`.

- [ ] **Step 4: Verify all 12 Docker-gated fixtures are green simultaneously**

The CI workflow has 12 Docker-gated jobs (one per fixture `0001-tcp-echo` through `0012-access-log-file-sink`) plus the h2spec conformance job + the parse_bootstrap fuzz job + the workspace tests + clippy + fmt + deny jobs. Confirm:

```bash
gh run view <run_id> --json jobs --jq '.jobs[] | {name: .name, conclusion: .conclusion}'
```

Expected output: every job's `conclusion` is `success`. If any job is `failure`, the state-4 gate has NOT held; the executor diagnoses with `superpowers:systematic-debugging` before claiming green.

- [ ] **Step 5: Append Task 8 state-4 evidence entry to PROGRESS.md**

```markdown
## Task 8 — State-4 phase-done gate evidence (12 fixtures simultaneously green)

### CI evidence anchor

- **CI run URL:** `https://github.com/<org>/<repo>/actions/runs/<run_id>`
- **Run ID:** `<run_id>`
- **HEAD SHA:** `<task-7-commit-sha>`
- **Workflow:** `ci.yml`
- **Conclusion:** `success`
- **Completed:** `<ISO-8601 UTC timestamp>`

### Test buckets enumerated (per parent SPEC §8 R-1 + 06.3 REVIEW I1 closure)

- **`cargo build --workspace --all-targets`** — PASS.
- **`cargo clippy --workspace --all-targets --all-features -- -D warnings`** — clean.
- **`cargo fmt --all -- --check`** — clean.
- **`cargo test --workspace`** — PASS (workspace test count: `<N>`).
- **`cargo deny check`** — clean (no new top-level Cargo deps; 07.1 added only the workspace-internal `envoy-filter` path-dep).
- **Docker-gated differential fixtures (12 total)** — all GREEN simultaneously at the same CI run:
  - `0001-tcp-echo` — PASS.
  - `0002-static-bootstrap-config` (or equivalent admin/ready) — PASS.
  - `0003-tcp-proxy` — PASS.
  - `0004-tls-downstream` — PASS.
  - `0005-tls-upstream` — PASS.
  - `0006-tls-sni` — PASS.
  - `0007-http1-direct-response` — PASS.
  - `0008-http1-router-upstream` — PASS.
  - `0009-http2-direct-response` — PASS.
  - `0010-http2-router-upstream` — PASS.
  - `0011-admin-stats-prometheus` — PASS.
  - `0012-access-log-file-sink` — PASS.
- **`tests/conformance/h2spec/`** — `<pass_rate>%` (≥95% gate held; `known-failures.txt` unchanged from 05.2 baseline).
- **`parse_bootstrap` fuzz target** — clean for the short-budget CI run (e.g., `-max_total_time=30s`); no new crash inputs added to corpus.

### §7.5 phase-done gate disposition

- **(a)** No new differential fixture — N/A for 07.1 (regression-equivalence is the surface).
- **(b)** Pre-existing fixtures: 12/12 green simultaneously — **PASS**.
- **(c)** Conformance suites: h2spec ≥95% — **PASS**.
- **(d)** Fuzz target: parse_bootstrap clean — **PASS**.
- **(e)** Workspace checks (build, clippy, fmt, test, deny) — **PASS**.
- **(f)** REVIEW.md approved — **PENDING** (state 5; lands at next session).
```

- [ ] **Step 6: Commit Task 8**

```bash
git add docs/envoy-rust/phases/07.1-filter-framework-foundation/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 07.1: task 8 — state-4 verification (12 fixtures simultaneously green)

CI run <run_id> at HEAD <task-7-commit-sha>, conclusion success,
completed <ISO-8601 UTC timestamp>. All 12 Docker-gated differential
fixtures (0001-tcp-echo through 0012-access-log-file-sink) green
simultaneously. h2spec ≥95% pass (known-failures.txt unchanged from
05.2 baseline). parse_bootstrap fuzz clean for short-budget run.
Workspace build / clippy / fmt / test / deny all clean.

Closes the §7.5 phase-done gate disposition for 07.1: (a) N/A
(no new fixture); (b) PASS (12/12 green simultaneously); (c) PASS
(h2spec ≥95%); (d) PASS (fuzz clean); (e) PASS (workspace clean);
(f) PENDING (REVIEW.md lands at state 5 / next session).

Closes 06.3 REVIEW I1 (verification-discipline gap — per-task PROGRESS
test-bucket attestation) by uniform attestation at Tasks 5/6/7 PROGRESS
entries + this comprehensive state-4 evidence anchor.

No code changes; PROGRESS-only.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 9: State-4 PROGRESS materialization + STATE advance to state-5-next

**Scope:** ~50 LoC docs only. Advances `STATE.md` from `07.1 state 3` to `07.1 state-4-reached / state-5-next`; next-skill `superpowers:requesting-code-review`. The 07.1 state-5 session reads the advanced STATE.md and runs `superpowers:requesting-code-review` per the state-machine.

**Files:**
- Modify: `docs/envoy-rust/STATE.md` — advance.
- Modify: `docs/envoy-rust/phases/07.1-filter-framework-foundation/PROGRESS.md` — Task 9 entry + a tidy state-4-reached / state-5-next summary block.

- [ ] **Step 1: Rewrite the "Active phase" + "Next expected skill" sections of `STATE.md`**

Update the "Active phase" block:
- **id:** `07.1` (unchanged).
- **slug:** `07.1-filter-framework-foundation` (unchanged).
- **directory:** `docs/envoy-rust/phases/07.1-filter-framework-foundation/` (exists with SPEC.md, PLAN.md, PROGRESS.md; REVIEW.md NOT yet present — lands at state 5).
- **status:** phase 07.1 lifecycle **state-4-reached / state-5-next** (verification complete; review pending). Phase-done gate evidence anchored at Task 8's commit `<SHA>` (CI run `<run_id>`, completed `<timestamp>`, conclusion `success`).
- ROADMAP row `07.1` remains `status: in-progress` (flips to `done` at the 07.1 state-6 close-out commit per the closing-sub-phase invariant — but 07.1 is NOT the closing sub-phase of parent-07, so the parent-07 row stays `in-progress` until 07.2 closes).

Update the "Next expected skill" block:

> Per the phase lifecycle state machine (`SKILL_ROUTING.md` line 39, verbatim from `BOOTSTRAP_PROMPT.md` §5 state 5): the next session — operating as the **07.1 state-5 review session** — invokes **`superpowers:requesting-code-review`** scoped to 07.1's surface. The reviewer reads `SPEC.md` + `PLAN.md` + `PROGRESS.md` (now landed end-to-end) + the per-task commits (Tasks 1-8) and produces `REVIEW.md` with verdict ∈ {Approved, Approved with M-track follow-ups, Issues}.
>
> If the verdict is Approved or Approved with M-track follow-ups: the next-next session enters state 6 (close-out) and invokes the state-6 commit cadence (advance STATE.md + flip ROADMAP row 07.1 to done + handoff to 07.2 state 1).
>
> If the verdict is Issues: the next-next session re-enters state 3 per BOOTSTRAP_PROMPT.md §5.2 (NOT state 4) — resuming implementation + TDD until REVIEW.md approves.

Update "Last commit" + "Last updated" fields to reflect Task 9's SHA + timestamp.

Add a "Phase-07.1 rollovers" subsection at the relevant location in STATE.md (after existing 06.3 / 06.2 / 06.1 / 05.x / 04.x rollovers sections; before any later-phase carryforward inventory):

```markdown
**Phase-07.1 rollovers** (per parent-07 SPEC §4 + 07.1 SPEC §4 inventory + Task 8's state-4 evidence):

- 06.3 REVIEW I1 (verification-discipline gap) — CLOSED at 07.1 per Tasks 5/6/7 PROGRESS test-bucket attestation + Task 8 comprehensive state-4 anchor.
- 04.1 REVIEW M5/M9 (Cargo.lock cadence ratification ADR) — carries forward unchanged (07.1 introduced no new top-level Cargo deps under recommended posture).
- Other carryforwards (06.2 M1/M2/M4/M5; 06.1 I2/M1/M4 to phase 08; 05.3 I2; 05.2 I1/I2/I3; 02.2 M1) — all out of scope for 07.1; carry forward unchanged.
```

- [ ] **Step 2: Append Task 9 entry to PROGRESS.md**

```markdown
## Task 9 — State-4 PROGRESS materialization + STATE.md advance

### Work summary

State-4 evidence materialized in clean PROGRESS Task 8 entry (Task 8
commit `<SHA>`). STATE.md advances `07.1` lifecycle state 3 →
state-4-reached / state-5-next. Next-skill
`superpowers:requesting-code-review` per BOOTSTRAP_PROMPT.md §5 state 5.

ROADMAP row 07.1 remains `in-progress` (flips to `done` at 07.1
state-6 close-out commit per ROADMAP-schema invariant; the parent-07
row stays `in-progress` until 07.2's state-6 commit per the
closing-sub-phase rule).

### Summary of 07.1 substantive surface (state-5 reviewer's quick-read)

- **Crates created:** 1 (`crates/envoy-filter/`; 4 modules; 13 unit tests).
- **Crates modified:** 3 (`envoy-config`, `envoy-http1`, `envoy-http2`).
- **Workspace members added:** 1 (`crates/envoy-filter`).
- **Cargo deps added:** 0 top-level (only workspace-internal path-dep
  on `envoy-filter` from `envoy-http1` + `envoy-http2`).
- **ConfigError variants added:** 3 (EmptyHttpFilters, RouterNotTerminal,
  DuplicateRouterFilter); MultipleHttpFilters retained per signpost 13.
- **HCMConfigError variants added:** 1 (FilterPipeline(envoy_filter::FilterError)).
- **Validator extension:** `validate_http_filters(filters, listener_name)`
  free function; replaces the pre-07.1 cardinality gate.
- **H1 refactor:** 5-writer-arm factoring at `serve_connection`; unified
  wire-write at the post-arm site; `construct_proxied_response` extracted
  from `write_proxied_response`.
- **H1 wiring:** `decode_headers` after `parse_request`; `encode_headers`
  at the unified factored site.
- **H2 refactor:** `finalize_h2_stream` gains `pipeline: &mut FilterPipeline`
  param; encode-side invocation inside `finalize_h2_stream`.
- **H2 wiring:** `decode_headers` at `handle_one_stream` after
  `http_to_envoy_request` translation.
- **Differential surface:** 12 pre-existing fixtures green simultaneously
  at CI run `<run_id>` (per Task 8 anchor).
- **Conformance:** h2spec ≥95% (carried forward from 05.2 baseline; 07.1
  engages no H2-framing surfaces).
- **DECISIONS.md ledger head:** **ADR-0030** (unchanged from 07.1 entry;
  no new ADRs landed under recommended posture per signpost 17).

### Total LoC delta across Tasks 1-9

| Task | Code LoC | Test LoC | Doc LoC | Total |
|---|---|---|---|---|
| 1 (envoy-filter scaffold + FilterError) | ~75 | ~25 | — | ~100 |
| 2 (FilterPipeline + Decision + iter) | ~70 | ~50 | — | ~120 |
| 3 (HttpFilterInstance + RouterTerminus) | ~90 | ~20 | — | ~110 |
| 4 (envoy-config validator + 3 variants) | ~150 | ~70 | — | ~220 |
| 5 (H1 HCM 5-writer-arm refactor) | ~130 | ~80 | — | ~210 |
| 6 (H1 HCM filter invocation) | ~70 | ~30 | — | ~100 |
| 7 (H2 HCM finalize_h2_stream + invocation) | ~140 | ~30 | — | ~170 |
| 8 (state-4 verification) | — | — | ~30 | ~30 |
| 9 (state-4 PROGRESS + STATE advance) | — | — | ~50 | ~50 |
| **Total substantive** | **~725** | **~305** | **~80** | **~1110** |

Plus PLAN.md narrative (~2500-3500 lines per 06.x precedent).

Against parent-07 SPEC §3 D1.1-D7.1 projection of ~900 LoC, actual
substantive surface lands at ~1110 LoC (~+23% drift; within parent-04.3 /
parent-05.3 ~+20% drift envelope). No nest-split required.

### Test-bucket attestation (final)

- `cargo test --workspace`: PASS at HEAD.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: clean.
- `cargo fmt --all -- --check`: clean.
- `cargo deny check`: clean.
- Docker-gated CI run `<run_id>`: all 12 fixtures green simultaneously
  + h2spec ≥95% + parse_bootstrap fuzz clean.
- envoy-filter unit test count: 13.
```

- [ ] **Step 3: Commit Task 9**

```bash
git add docs/envoy-rust/STATE.md \
        docs/envoy-rust/phases/07.1-filter-framework-foundation/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 07.1: task 9 — advance STATE.md to state-4-reached / state-5-next

STATE.md advances 07.1 lifecycle state 3 → state-4-reached /
state-5-next; next-skill superpowers:requesting-code-review.
PROGRESS.md gains Task 9 entry materializing the state-4 evidence
anchor (Task 8 commit + CI run details) and the substantive-surface
quick-read for the state-5 reviewer.

ROADMAP row 07.1 unchanged at in-progress (flips to done at 07.1
state-6 close-out per ROADMAP-schema invariant). Parent ROADMAP row
07 unchanged at in-progress (flips to done at 07.2's state-6 close-out
per the closing-sub-phase rule).

DECISIONS.md ledger head unchanged at ADR-0030. Recommended posture
(no foundations grants in phase 07) honored end-to-end across all
9 substantive tasks.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Self-review checklist (run before committing this state-2 PLAN.md)

Per the writing-plans skill's Self-Review section. The planner runs this checklist BEFORE the state-2 commit; the executor at state 3 may re-run as a sanity check.

**1. Spec coverage:** Skim 07.1 SPEC §3 D1.1-D7.1 + §5 14 signposts + §6 evidence shape + §7 ADR projection + §8 state-2 cadence. For every line, point to a task or architecture decision.

- D1.1 (envoy-filter crate + FilterPipeline + Decision + HttpFilterInstance + RouterTerminus + FilterError) → Tasks 1 + 2 + 3.
- D2.1 (H1 HCM writer-arm refactor + decode/encode invocation) → Tasks 5 + 6.
- D3.1 (H2 HCM finalize_h2_stream refactor + decode/encode invocation) → Task 7.
- D4.1 (envoy-config terminal-router validator + 3 new ConfigError variants) → Task 4.
- D5.1 (per-task PROGRESS test-bucket attestation) → Conventions block + Tasks 5/6/7 PROGRESS entries.
- D6.1 (state-4 phase-done gate verification) → Task 8.
- D7.1 (state-4-reached / state-5-next STATE advance) → Task 9.
- §5 signpost 1 (module decomposition mandatory) → architecture decision 1 + Tasks 1/2/3 split.
- §5 signpost 2 (StopAndSend scaffolded from day one) → architecture decision 2 + Task 2 Decision enum.
- §5 signpost 3 (Arc-clone shape) → architecture decision 3 + Task 6 HCMConfig field.
- §5 signpost 4 (outgoing local declaration) → architecture decision 4 + Task 5 refactor.
- §5 signpost 5 (let-then-assign discipline) → architecture decision 5 + Task 5 refactor.
- §5 signpost 6 (finalize_h2_stream parameter threading) → architecture decision 6 + Task 7.
- §5 signpost 7 (cross-crate dep direction) → architecture decision 7 + Task 1 Cargo.toml deps.
- §5 signpost 8 (validator listener_name threading) → architecture decision 8 + Task 4 validate_http_filters signature.
- §5 signpost 9 (Task-5-as-pure-refactor + Task-6-as-wiring) → architecture decision 9 + Tasks 5/6 split.
- §5 signpost 10 (RequestPath/H2RequestPath per-HCM) → architecture decision 10 + Tasks 6/7 enum declarations.
- §5 signpost 11 (no new fuzz target) → architecture decision 11 + Tech Stack note.
- §5 signpost 12 (test-only filter stub gating) → architecture decision 12 + Tasks 6/7 tests-deferred-to-07.2 notes.
- §5 signpost 13 (MultipleHttpFilters retention) → architecture decision 13 + Task 4 doc-comment supersession.
- §5 signpost 14 (serve_connection body shape) → architecture decision 14 + Task 6 wiring.
- §6 evidence shape (12 fixtures simultaneously green at CI run anchor) → Task 8 PROGRESS entry shape.
- §7 ADR projection (no new ADRs) → architecture decision 17 + state-2 commit msg.
- §8 state-2 cadence (PLAN.md + STATE.md ONLY at state-2) → State-2 commit section.

**No gaps found.**

**2. Placeholder scan:** Grep for red flags (`TBD`, `TODO`, `implement later`, `fill in details`, `similar to Task N`, etc.).

```bash
grep -nE "\b(TBD|TODO|implement later|fill in details|similar to Task)\b" docs/envoy-rust/phases/07.1-filter-framework-foundation/PLAN.md
```

Expected: no matches (the planner notes any matches found and resolves inline before state-2 commit).

**3. Type consistency:** Verify type names + method signatures + property names match across tasks.

- `FilterPipeline` (Task 2) + `FilterPipeline::build_from_config` / `decode_headers` / `encode_headers` (Task 2) + `Arc<FilterPipeline>` field (Task 6 + Task 7) → consistent.
- `Decision::{Continue, StopAndSend}` (Task 2) + matched in Tasks 6 + 7 → consistent.
- `HttpFilterInstance::Router(RouterTerminus)` (Task 3) + matched at Task 2 placeholder shape (which says `HttpFilterInstance::Router` no-payload; Task 3 corrects to payload-variant) → consistent across the Task 2 → Task 3 transition.
- `FilterError::{EmptyChain, RouterNotTerminal, DuplicateRouter, UnsupportedFilterType}` (Task 1) + matched at Tasks 2 (EmptyChain) + 6 (FilterPipeline error mapping via #[from]) → consistent.
- `ConfigError::{EmptyHttpFilters, RouterNotTerminal, DuplicateRouterFilter}` (Task 4) → all 3 variants carry `listener: String` field; `RouterNotTerminal` adds `position: usize` → consistent.
- `HCMConfigError::FilterPipeline(envoy_filter::FilterError)` (Task 6) via `#[from]` → consistent.
- `RequestPath::{Match(BuildOutcome), SynthFromDecode(Response)}` (Task 6) parallel to `H2RequestPath::{Match(BuildOutcome), SynthFromDecode(Response)}` (Task 7) → consistent shape (per-HCM separate types per signpost 10).
- `construct_proxied_response(&cluster, upstream_response, elapsed_ms)` signature (Task 5) → consumed at Task 5's H1 hcm refactor; signature stable.

**No inconsistencies found.**

If issues surface, fix them inline. Re-run the spec coverage check only if a fix changes task scope.

---
