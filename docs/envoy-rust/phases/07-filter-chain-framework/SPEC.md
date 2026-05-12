# Phase 07 — Filter chain framework: iteration protocol, per-route config, extension registry

- **Phase id:** `07`
- **Slug:** `07-filter-chain-framework`
- **Title:** HTTP filter-chain framework — iteration protocol over the already-buffered `envoy_http1::codec::Request` / `Response` shape; per-route `typed_per_filter_config` override map (scaffolded; no MVP filter consumes it); enum-based extension registry; new workspace member `crates/envoy-filter/` as the sole owner of filter-chain dispatch logic; first non-router pluggable filter (`envoy.filters.http.header_mutation`) covering both MVP iteration states bilaterally
- **Depends on:** `06` (observability foundations). Phase 06 ROADMAP row is `done` as of commit `b918f33` (the parent-06 state-6 close-out, which also flipped sub-phase row `06.3` from `in-progress` to `done` per the ROADMAP-schema invariant in §4.1 of `BOOTSTRAP_PROMPT.md`). Phase 07 enters `in-progress` at this state-1 close-out commit.
- **Seeded by:** `BOOTSTRAP_PROMPT.md` §8 row 07 — *"Filter chain framework: iteration protocol, per-route config, extension registry"* with the differential surface gate *"framework fixtures green; trivial pluggable filter covers all iteration states"*. Doctrine `D-3.2` lists *Filter chain engine (network + HTTP filter iteration protocol)* and *Every individual filter (network and HTTP)* on its **Must be written from scratch** list — neither a third-party filter-chain crate nor any individual filter is permitted as a direct dep. The framework, its iteration protocol, the per-route config wiring, and the `HeaderMutation` filter implementation are all hand-rolled atop the existing `tokio` + `bytes` + `serde` + `tracing` + `thiserror` foundations.
- **Differential surface when done:**
  - **Pre-existing fixtures:** `0001-tcp-echo` through `0012-access-log-file-sink` — all 12 stay green at the Docker-gated CI level. The parent-07 framework wiring (07.1) refactors both H1 and H2 HCM dispatch sites to walk the filter chain on every request, but with the Router-only chain configuration that every existing fixture uses, the wire behavior is regression-equivalent. The cardinality validator (currently rejecting `len != 1`) relaxes to "`len >= 1` AND `Router` is the last entry" at 07.1; all existing fixtures continue to declare exactly `[Router]`.
  - **New fixtures:** `tests/fixtures/0013-http-filter-header-mutation/` lands in 07.2 — exercises the `envoy.filters.http.header_mutation` filter in front of the Router on a `direct_response` route (decode-side) plus a router-proxied route to an Http1EchoBackend (encode-side + request-side echoed back through the body so the harness can assert the request stamp landed at the backend). Both proxies emit the response header `x-filter-response-stamp: phase-07`; both backends see `x-filter-stamp: phase-07`; differential equivalence is value-exact on both sides.
  - **Conformance suites unchanged:** `tests/conformance/h2spec/` continues at the **≥95% pass** gate landed in 05.2 D7 (99.31%). Phase 07's filter-chain framework engages no H2-framing surfaces — both H1 and H2 HCMs invoke the framework after request parsing and before response serialization, so framing remains the codec edge's responsibility.
- **Sub-phases:** **`07.1`, `07.2`** projected (codified at parent-07 state-2 via **ADR-0030** — see §7).

This SPEC is the design contract for the parent phase 07. It projects the split into two sub-phases by deliverable boundary (framework foundation + HCM wiring → first concrete pluggable filter + parent close). The 2-way split mirrors phase-02's precedent under ADR-0013 and phase-03's precedent under ADR-0017, and was selected over a 3-way split for the same coherence reason: the framework foundation + HCM integration is one architectural slice (07.1) that the first concrete filter (07.2) consumes — splitting the filter implementation away from its first consumer would leave 07.1 stranded with no behavior-changing fixture.

This SPEC is self-contained per doctrine D-3.4; a stranger reading only this file plus the stable doctrine documents (`MISSION.md`, `BEHAVIOR_CONTRACT.md`, `DECISIONS.md`, `BOOTSTRAP_PROMPT.md`) and the landed phase-06 surface (via `git log` and the in-tree workspace shape at HEAD `b918f33`) must be able to operate as the parent-07 state-2 session — landing **ADR-0030** (split decision), the two sub-phase SPECs, and the ROADMAP rows for `07.1`, `07.2`. Each sub-phase then enters its own state-1 brainstorm cadence with its own SPEC.

---

## 1. Goal and acceptance signal

**Goal.** Land an HTTP filter-chain framework that walks an ordered sequence of filters on both decode (request-arrival) and encode (response-write) sides, terminates at the existing `Router` filter, and supports the addition of new pluggable filters by adding one enum variant per filter type. Across both sub-phases, the architectural rule is **`envoy-filter` owns the iteration protocol and the filter runtime types; HCM consumers own the invocation site; `envoy-config` owns the parse-time schema**. The framework is hand-rolled per D-3.2 — no `async_trait`, no dyn-dispatch indirection, no factory pattern, no runtime extension registration. The MVP iteration protocol covers two states (decode_headers, encode_headers) on the already-buffered request/response shape that 04.1 + 05.2 established; body-streaming and trailer-iteration states defer to whichever phase first introduces a streaming-buffering refactor.

1. **Foundation crate + HCM integration** (sub-phase **07.1**). New workspace member `crates/envoy-filter/` (sole-dep-owner of filter-chain dispatch). Public surface: an `HttpFilterInstance` enum (the runtime form; one variant per filter type — Router-only at 07.1; HeaderMutation lands at 07.2), a `FilterPipeline` struct holding `Vec<HttpFilterInstance>` and the `decode_headers` / `encode_headers` methods, a `Decision` enum with `Continue` and `StopAndSend(Response)` variants (forward-compat scaffolding; no MVP filter emits `StopAndSend` in 07.x), and a `FilterPipeline::build_from_config(&[envoy_config::HttpFilter]) -> Result<Self, FilterError>` constructor. New typed-error enum `FilterError` (parse-time validation failures bubble up via `envoy_config::ConfigError`; runtime failures via `FilterError`; thiserror-typed). Crate root `lib.rs` carries `#![forbid(unsafe_code)]` per D-3.8. **HCM integration at both H1 and H2 sites**: `HCMConfig` gains a `filter_pipeline: Arc<FilterPipeline>` field (built once at `HCMConfig::from_config` time); H1's `serve_connection` per-request loop (at `crates/envoy-http1/src/hcm.rs::serve_connection`) and H2's `handle_one_stream` (at `crates/envoy-http2/src/hcm.rs::handle_one_stream`) both invoke `pipeline.decode_headers(&mut req)` immediately after request parsing and before `build_response`, and `pipeline.encode_headers(&mut resp)` at the single factored access-log dispatch site immediately after all 5 writer arms have populated `response_headers_for_log` (H1) or inside `finalize_h2_stream` (H2). **Validator relaxation in `envoy-config`**: the existing cardinality validator at `crates/envoy-config/src/bootstrap.rs::validate_hcm` (lines 1335-1347; currently rejecting `len != 1`) relaxes to "`len >= 1` AND last entry's `name == "envoy.filters.http.router"` AND no other entry's name is `"envoy.filters.http.router"`". New `ConfigError::RouterNotTerminal { listener: String, position: usize }` + `ConfigError::DuplicateRouterFilter { listener: String }` variants. **No new fixture in 07.1** — the framework is regression-equivalent under the existing Router-only chain. The state-4 phase-done gate verifies all 12 existing fixtures (0001-0012) stay green simultaneously, proving the framework wiring introduces no wire-behavior regression.

2. **HeaderMutation filter + fixture 0013 + parent close** (sub-phase **07.2**). Extends `HttpFilterInstance` with a new variant `HeaderMutation(HeaderMutationFilter)`. The `HeaderMutationFilter` struct holds the parsed mutation lists (`request_mutations: Vec<HeaderMutation>`, `response_mutations: Vec<HeaderMutation>`) and applies them to the request/response headers in their respective iteration states. **Iteration semantics**: each `HeaderMutation` entry has an `append: HeaderValueOption { header: HeaderValue { key, value }, append_action: AppendAction }` shape (matching Envoy v1.33's `envoy.extensions.filters.http.header_mutation.v3.HeaderMutation` proto). `AppendAction` enum values supported in 07.2: `APPEND_IF_EXISTS_OR_ADD` (push to the headers Vec, allowing duplicates) and `OVERWRITE_IF_EXISTS_OR_ADD` (case-insensitive remove-then-push). Other AppendAction values (`ADD_IF_ABSENT`, `OVERWRITE_IF_EXISTS`) defer; the validator rejects them with `ConfigError::UnsupportedHeaderMutationAppendAction`. **Schema additions in `envoy-config`**: `HttpFilterTypedConfig` enum gains a `HeaderMutation(HeaderMutationConfig)` variant; new structs `HeaderMutationConfig { mutations: Mutations }`, `Mutations { request_mutations: Vec<HeaderMutationEntry>, response_mutations: Vec<HeaderMutationEntry> }`, `HeaderMutationEntry { append: HeaderValueOption }`, `HeaderValueOption { header: HeaderValue, append_action: AppendAction }`, `HeaderValue { key: String, value: String }`, `AppendAction` enum; all with `#[serde(deny_unknown_fields)]`. **Validator extension**: `validate_http_filters` walks each non-Router filter's `typed_config` and applies per-variant validation (HeaderMutation: each entry's `header.key` non-empty + RFC 7230 token-set per the existing `is_token_char` helper if available, else hand-rolled; `append_action` in the supported subset). **Differential harness extension**: reuses the existing `Driver::Http1` shape — no new `Driver::*` variant. `expected_headers: SetEqualModuloAllowList` asserts the response-side stamp lands on both proxies; `expected_body: ByteExact` asserts the Http1EchoBackend's body output (which echoes received headers as lines per the existing helper) matches across both proxies, proving the request-side stamp landed at the backend. **Fuzz seed**: `hcm_header_mutation_filter.yaml` added to `crates/envoy-config/fuzz/corpus/parse_bootstrap/`. **Fixture `0013-http-filter-header-mutation`** lands the bilateral assertion end-to-end. **Parent-07 close** at 07.2's state-6 commit per the `e626862` / `82c26b8` / `b918f33` closing-sub-phase precedent (the last sub-phase commit also flips parent row `07` from `in-progress` to `done`).

**Acceptance signal** — the phase-done gate from §7.5 of `BOOTSTRAP_PROMPT.md`, scoped to phase 07's full feature surface across both sub-phases:

- **(a)** the new differential fixture `tests/fixtures/0013-http-filter-header-mutation/` is green at the Docker-gated CI level;
- **(b)** the pre-existing differential fixtures `0001` through `0012` are all green at the Docker-gated CI level (no regression on any earlier surface; the framework wiring in 07.1 preserves every existing fixture's wire-equivalence against upstream Envoy);
- **(c)** the conformance suite `tests/conformance/h2spec/` continues to pass at **≥95%** with `known-failures.txt` unchanged (phase 07 engages no H2-framing surfaces; the framework invocation lives between the codec edge and the route-match site);
- **(d)** the existing fuzz target `parse_bootstrap` runs clean for its short-budget CI run (`cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30`) against the corpus extended in 07.2 (≥1 new HCM `http_filters` block with the HeaderMutation typed_config). No new fuzz target ships in phase 07;
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, and `cargo deny check` are all clean on the stable-toolchain CI job;
- **(f)** both sub-phase `REVIEW.md` verdicts are approved.

The parent-phase-done commit lands at the **last sub-phase's state-6 commit** (i.e., 07.2's phase-done commit also flips parent row `07` from `in-progress` to `done` — mirrors phase 04's `e626862` close-out, phase 05's `82c26b8` close-out, and phase 06's `b918f33` close-out).

---

## 2. Behavior-contract scope for phase 07

Phase 07 is the first phase to introduce a filter-chain-iteration surface to `docs/envoy-rust/BEHAVIOR_CONTRACT.md`. The expected updates are minimal because filter behavior is largely deterministic and symmetric between Envoy and envoy-rust:

1. **`Header allow-list` — no new entries anticipated for HeaderMutation.** The `HeaderMutationFilter` is deterministic on both proxies: identical config produces identical wire-level header mutations on both sides. The 07.2 fixture's `expected_headers` rule is `SetEqualModuloAllowList` (the existing 04.x-established shape), and the existing `HEADER_ALLOW_LIST` entries (`server`, `date`, `x-envoy-upstream-service-time`) are unaffected. If empirical testing at 07.2 surfaces a header-emission divergence (e.g., Envoy emits a `x-envoy-decorator-operation` debug header that envoy-rust does not), a new row lands in BEHAVIOR_CONTRACT.md at the relevant 07.2 task per the established 04.3 / 06.x posture.

2. **`Stat-name mapping` — no new stat entries anticipated.** Phase 07 does NOT add filter-emitted stats in MVP. Envoy's filter framework supports per-filter stats (`http.<stat_prefix>.<filter_name>.<stat>`), but the 07.x scope does not engage filter-emitted stats — neither the framework crate nor the HeaderMutation filter declares counters. Future filters (rate-limit, ext_authz) will introduce filter-emitted stats; those phases extend the table.

3. **`Access log field mapping` — no new tokens anticipated.** Phase 07 does NOT introduce filter-state or dynamic-metadata access-log tokens. `%FILTER_STATE%` and `%DYNAMIC_METADATA%` (per parent-06 SPEC §4) remain explicitly deferred; the framework crate does not provide filter-state or dynamic-metadata storage in MVP.

4. **New subsection `Filter chain iteration` MAY land at 07.1.** If the framework's iteration semantics need to be canonicalized in the behavior contract (e.g., "filters in the chain run in declaration order on decode; reverse declaration order on encode"), a new subsection lands in BEHAVIOR_CONTRACT.md at the 07.1 state-3 task that wires the iteration. Recommended posture: defer the subsection until empirical evidence (Envoy-vs-envoy-rust divergence on iteration order) demands it. The 07.1 fixture-regression evidence (all 12 existing fixtures stay green) is the implicit canonicalization.

5. **`xDS wire state machine` and `Timing tolerances` subsections — untouched.** Phase 07 does not engage xDS or timing-sensitive features.

---

## 3. Deliverables (organized by sub-phase)

This section enumerates the projected deliverables across the two sub-phases. Each sub-phase's own SPEC (written at parent-07 state-2 via the split commit per ADR-0030) will expand its own deliverables into the per-task PLAN cadence the project follows. Total LoC and task counts are first-order estimates; per phase-04.3's drift experience, the planner should expect ~+20% drift at execution time.

### Phase 07.1 — `envoy-filter` foundation + HCM filter-chain wiring (including H1 + H2 writer-arm refactor) + terminal-router validator (~900 LoC, ~9 tasks)

**D1.1 — New library crate `crates/envoy-filter/`.** Added to root `Cargo.toml` `[workspace] members`. Cargo deps: `bytes = "1"`, `thiserror = "2"`, `tracing = "0.1"`, `envoy-config = { path = "../envoy-config" }` (for parse-time `HttpFilter` consumption at build time), `envoy-http1 = { path = "../envoy-http1" }` (for the shared `codec::Request` / `codec::Response` value-types — the framework operates on these directly). **No new permitted-foundations grants** — the framework manipulates only `Vec<(String, String)>` headers, `Bytes` bodies, and primitives. Crate root `lib.rs` carries `#![forbid(unsafe_code)]` per D-3.8.

  **Module decomposition** (final shape decided at 07.1 SPEC writeup time; this is the projection):
  ```
  crates/envoy-filter/src/
    lib.rs        // crate root: #![forbid(unsafe_code)]; public re-exports
    pipeline.rs   // FilterPipeline + Decision + iteration loop
    instance.rs   // HttpFilterInstance enum (Router-only at 07.1; HeaderMutation at 07.2)
    router.rs     // RouterTerminus (no-op decode + no-op encode; the chain terminator)
    error.rs      // FilterError typed-error enum
  ```

  Public surface re-exported at `lib.rs`:
  ```rust
  pub mod pipeline;
  pub mod instance;
  pub mod error;
  pub use pipeline::{FilterPipeline, Decision};
  pub use instance::HttpFilterInstance;
  pub use error::FilterError;
  ```

  Iteration protocol:
  ```rust
  pub enum Decision {
      Continue,
      StopAndSend(envoy_http1::codec::Response),
  }

  pub struct FilterPipeline {
      filters: Vec<HttpFilterInstance>,
  }

  impl FilterPipeline {
      pub fn build_from_config(
          filters: &[envoy_config::HttpFilter],
      ) -> Result<Self, FilterError> { ... }

      pub fn decode_headers(&mut self, req: &mut envoy_http1::codec::Request) -> Decision { ... }
      pub fn encode_headers(&mut self, resp: &mut envoy_http1::codec::Response) -> Decision { ... }
  }
  ```

  ~250 LoC impl + ~200 LoC unit tests (pipeline-construction from Router-only config; decode iteration walks filters in declaration order; encode iteration walks filters in reverse declaration order — mirrors Envoy v1.33 semantics; `Continue → Continue → ... → Router::Continue` happy path; `StopAndSend` short-circuits the remaining filters and routes back through encode; FilterError typed-error coverage).

**D2.1 — H1 HCM integration (writer-arm refactor + filter invocation).** `envoy-http1::HCMConfig` gains an `Arc<FilterPipeline>` field constructed at `HCMConfig::from_config` time (parses the envoy-config `http_filters` list, builds the pipeline, wraps in `Arc`).

  **Decode-side invocation** is simple: at `serve_connection`'s per-request loop, immediately after `crate::codec::parse_request` returns Ok and immediately before `build_response`, invokes `pipeline.decode_headers(&mut req)`, and matches:
  - `Decision::Continue` → proceeds to `build_response` as today
  - `Decision::StopAndSend(resp)` → skips route-match and `build_response`; threads `resp` directly into the unified encode + dispatch site (described below)

  **Encode-side invocation requires factoring the wire-write out of the writer arms.** This is a structural refactor and the load-bearing complexity of 07.1. Today (per HEAD `b918f33`), each of the 5 writer arms at `crates/envoy-http1/src/hcm.rs:378-516` constructs a response value AND writes the wire inline:
  - synth arm (line 383): `Http1Response::write_to(&resp, &mut downstream).await?`
  - proxy-success arm (line 471): `router::write_proxied_response(&mut downstream, ..., upstream_response, ...).await?`
  - proxy send-fail-502 arm (line 484): synth-502 → `Http1Response::write_to`
  - proxy connect-fail-502 arm (line 499): synth-502 → `Http1Response::write_to`
  - proxy no-endpoint-503 arm (line 512): synth-503 → `Http1Response::write_to`

  Each arm also populates the `response_status_for_log` / `response_body_len` / `response_headers_for_log` / `upstream_host_for_log` locals before its wire-write call.

  07.1 refactors each arm to construct a response value WITHOUT writing the wire. The arms produce a single `OutgoingResponse` enum (or `Http1Response` value uniformly — the planner at 07.1 SPEC writeup picks the shape) and propagate it to the unified encode + dispatch site that today hosts the per-class counter increment (lines 527-533). The unified site runs:
  1. `pipeline.encode_headers(&mut outgoing_resp)` — applies filter mutations to the response. `Decision::StopAndSend(replacement_resp)` substitutes the response.
  2. Re-populate `response_status_for_log` / `response_body_len` / `response_headers_for_log` from the post-encode response (so access-log + per-class counter reflect the wire-emitted shape, not the pre-encode shape).
  3. Per-class HCM counter increment (existing 06.3 D15.3.a site; unchanged logic).
  4. Wire write — for synth/synth-502/synth-503/no-endpoint-503: `Http1Response::write_to(&outgoing_resp, &mut downstream).await?`; for proxy-success: the equivalent of `write_proxied_response` factored so the response value is taken by reference (the `cluster: &ClusterHandle` parameter remains for cluster-side stat wiring).
  5. Access-log build + dispatch (existing 06.2 site; consumes the post-encode `response_headers_for_log`).

  **Signpost 1 — Architectural shape of the unified site.** The planner at 07.1 SPEC writeup time picks between (a) a single `OutgoingResponse` enum with `{ Http1(Http1Response, bool /* close */), Proxied(UpstreamResponse, u128 /* elapsed_ms */) }` variants, OR (b) eagerly materialize the proxy-success arm's response into an `Http1Response` value at the arm itself (consuming the upstream-response bytes), so the unified site sees a uniform `Http1Response`. Option (b) is simpler at the cost of one buffer copy for the proxy-success body. **Recommended: option (b)** — the simplicity dominates; the buffer copy is acceptable per the parent-04.3 fully-buffered posture.

  **Signpost 2 — `write_proxied_response` factoring.** The existing `crates/envoy-http1/src/router.rs::write_proxied_response` mixes response construction with the wire write. 07.1 splits it into `construct_proxied_response(upstream, cluster, elapsed_ms, close) -> Http1Response` (or fills directly into the OutgoingResponse value of choice) + the wire-write at the unified site.

  **Signpost 3 — Pipeline mutability.** `FilterPipeline::decode_headers` / `encode_headers` take `&mut self` because future filter types may carry per-request state (e.g., a rate-limit filter accumulating bucket counters). HCMConfig holds `Arc<FilterPipelineTemplate>` (Cloneable); each per-request scope clones into a working `FilterPipeline`. Each `HttpFilterInstance` is a small Cloneable value (HeaderMutation: just two `Vec<RuntimeHeaderMutation>` references). Mirrors the access-log `Vec<Arc<FileSink>>` shape — Arc-shared at config, owned at request.

  **Signpost 4 — `StopAndSend` from decode side.** When decode_headers returns `StopAndSend(resp)`, route-match and the writer-arm match are skipped entirely; control jumps directly to the unified encode + dispatch site with the `StopAndSend` payload as the `OutgoingResponse`. encode_headers still runs (consistent semantics — encode iteration always fires on every response).

  **Signpost 5 — `upstream_host_for_log` only populated by proxy arm.** Per the existing pattern; decode_headers does not affect upstream selection (the Router does that during dispatch).

  ~230 LoC code (writer-arm refactor ~80 + filter invocation wiring ~70 + `write_proxied_response` factoring ~40 + new types ~40) + ~120 LoC unit tests (HCMConfig::from_config builds the pipeline; decode_headers fires before route-match; encode_headers fires after writer-arm response construction but before wire write; StopAndSend at decode skips route-match but still runs encode_headers; StopAndSend at encode substitutes the wire-emitted response; access-log reflects post-encode headers; regression-equivalence on Router-only chain).

**D3.1 — H2 HCM integration (symmetric writer-arm refactor at H2 sites).** Symmetric with D2.1 at `crates/envoy-http2/src/hcm.rs::handle_one_stream` + `finalize_h2_stream`. The H2 HCM consumes the same `HCMConfig` (per the cross-sub-phase rule established in 05.2 — HCMConfig is a type alias to `envoy_http1::hcm::HCMConfig`), so the framework field is automatically present.

  **Decode-side invocation**: after request translation via `http_to_envoy_request` (the 05.2 D3 adapter) returns `envoy_req`, `handle_one_stream` invokes `pipeline.decode_headers(&mut envoy_req)`. `Decision::Continue` proceeds to `build_response`; `Decision::StopAndSend(resp)` routes directly to `finalize_h2_stream` with the synthesized response.

  **Encode-side invocation requires factoring `send_envoy_response` out of the writer paths.** Today (per HEAD `b918f33`), H2's 3 writer paths each construct a response value AND route through `finalize_h2_stream` which calls `send_envoy_response(send_response, resp).await` — the wire-write happens inside `finalize_h2_stream` at line 378. 07.1 factors `finalize_h2_stream` to invoke `pipeline.encode_headers(&mut resp)` BEFORE `send_envoy_response`, then re-populates `response_status_for_log` / `response_headers_for_log` from the post-encode response. The per-class HCM counter increment (the 06.3 D15.3.a site at H2 lines 380-391) consumes the post-encode `response_status_for_log`. The H2 site's refactor is structurally simpler than H1's (only `finalize_h2_stream` itself is touched; the writer paths' early-returns already feed `finalize_h2_stream` with the synthesized response).

  ~120 LoC code (`finalize_h2_stream` extension ~50 + decode-side invocation in `handle_one_stream` ~30 + `StopAndSend` early-return wiring ~40) + ~80 LoC unit tests (parallel to D2.1 for H2; decode_headers fires before route-match; encode_headers fires before `send_envoy_response`; StopAndSend short-circuits both sides; regression-equivalence on Router-only chain on the existing 0009 + 0010 fixtures' surfaces).

**D4.1 — `envoy-config` validator relaxation.** `validate_hcm` at `crates/envoy-config/src/bootstrap.rs:1335-1347` relaxes from:
  ```rust
  match hcm.http_filters.len() {
      1 => { /* check name == router */ }
      n => return Err(ConfigError::MultipleHttpFilters { count: n }),
  }
  ```
  to:
  ```rust
  if hcm.http_filters.is_empty() {
      return Err(ConfigError::EmptyHttpFilters { listener: ... });
  }
  // Walk filters: at most one Router; Router must be at the terminus.
  // Validate per-filter typed_config invariants (per-variant; 07.1 only has Router so no-op).
  validate_http_filters(&hcm.http_filters, listener_name)?;
  ```
  New free function `validate_http_filters(filters: &[HttpFilter], listener_name: &str) -> Result<(), ConfigError>` walks the filter list and enforces: (a) at least one filter; (b) exactly one filter has `name == "envoy.filters.http.router"`; (c) the Router is the last entry. New `ConfigError` variants: `EmptyHttpFilters { listener: String }`, `RouterNotTerminal { listener: String, position: usize }`, `DuplicateRouterFilter { listener: String }`. The existing `MultipleHttpFilters` variant is deprecated but retained (no longer constructed; kept for ledger discipline; superseded-by note in the variant's doc comment). The existing `UnsupportedHttpFilter` continues firing on any non-Router filter at 07.1 (the schema's `HttpFilterTypedConfig` enum still has only the `Router` variant; 07.2 adds the `HeaderMutation` variant which relaxes this). ~60 LoC schema + ~80 LoC validator + ~8 unit tests (positive: single Router passes; positive: multi-filter with Router-last would also pass but no MVP filter at 07.1 to exercise; negative: empty rejects; negative: Router not last rejects; negative: duplicate Router rejects; negative: non-Router single-filter still rejects via `UnsupportedHttpFilter` at the typed_config schema level until 07.2).

**D5.1 — Cross-crate dep direction**. `envoy-filter` depends on `envoy-config` (for `HttpFilter` config struct) and `envoy-http1` (for `codec::Request` / `codec::Response`). `envoy-http1` and `envoy-http2` both depend on `envoy-filter` (for the framework runtime). This creates a stack `envoy-config → envoy-filter → envoy-http1, envoy-http2 → envoy-bin`. No cycles — `envoy-filter` does NOT depend back on `envoy-http1` for the HCM types; it only consumes `envoy-http1::codec` for value-types. **Signpost 3 — codec module is the seam**: if `envoy-filter` ends up needing types beyond `codec::Request` / `codec::Response`, the recommended path is to move those types to a smaller `envoy-codec` crate (out of scope for 07.1; flagged for whichever future phase first surfaces the pressure).

**D6.1 — Existing fixture regression evidence.** No new fixture in 07.1. The state-4 phase-done gate verifies all 12 existing fixtures (`0001-0012`) stay green simultaneously at the Docker-gated CI level. This is the framework's regression-equivalence proof.

**D7.1 (verification deliverable, no code).** State-4 phase-done verification per the `BOOTSTRAP_PROMPT.md` §7.5 gate, scoped to 07.1's surfaces. PROGRESS.md quotes the CI run URL + the 0001-0012 + h2spec results inline.

### Phase 07.2 — HeaderMutation filter + fixture 0013 + parent-07 close (~800 LoC, ~10 tasks)

**D8.2 — `envoy-config` schema additions for HeaderMutation.** `HttpFilterTypedConfig` enum gains a `HeaderMutation(HeaderMutationConfig)` variant at `crates/envoy-config/src/bootstrap.rs:442-447`:
  ```rust
  pub enum HttpFilterTypedConfig {
      #[serde(rename = "type.googleapis.com/envoy.extensions.filters.http.router.v3.Router")]
      Router(RouterConfig),
      #[serde(rename = "type.googleapis.com/envoy.extensions.filters.http.header_mutation.v3.HeaderMutation")]
      HeaderMutation(HeaderMutationConfig),
  }

  pub struct HeaderMutationConfig {
      pub mutations: Mutations,
  }
  pub struct Mutations {
      #[serde(default)] pub request_mutations: Vec<HeaderMutationEntry>,
      #[serde(default)] pub response_mutations: Vec<HeaderMutationEntry>,
  }
  pub struct HeaderMutationEntry {
      pub append: HeaderValueOption,
  }
  pub struct HeaderValueOption {
      pub header: HeaderValue,
      pub append_action: AppendAction,
  }
  pub struct HeaderValue { pub key: String, pub value: String }
  pub enum AppendAction {
      APPEND_IF_EXISTS_OR_ADD,
      OVERWRITE_IF_EXISTS_OR_ADD,
      // ADD_IF_ABSENT, OVERWRITE_IF_EXISTS — deferred; validator rejects.
  }
  ```
  All structs with `#[serde(deny_unknown_fields)]`. ~120 LoC schema + ~30 LoC validator extension at `validate_http_filters` (per-entry: `header.key` non-empty + RFC 7230 token-set; `append_action` in supported subset) + 12 unit tests (positive: minimal request-only / minimal response-only / both / empty-mutations / multiple-entries; negative: empty key / invalid token in key / unsupported append_action / unknown field).

**D9.2 — `envoy-filter` runtime additions for HeaderMutation.** Extends `HttpFilterInstance` enum:
  ```rust
  pub enum HttpFilterInstance {
      HeaderMutation(HeaderMutationFilter),
      Router(RouterTerminus),
  }

  pub struct HeaderMutationFilter {
      request_mutations: Vec<RuntimeHeaderMutation>,
      response_mutations: Vec<RuntimeHeaderMutation>,
  }
  struct RuntimeHeaderMutation {
      key: String,            // lowercased once at build time
      value: String,
      action: RuntimeAppendAction,
  }
  enum RuntimeAppendAction { Append, Overwrite }
  ```
  `FilterPipeline::build_from_config` gains a HeaderMutation arm mapping the schema struct to the runtime struct (lowercase keys once; map AppendAction variants to RuntimeAppendAction). `HeaderMutationFilter::decode_headers` applies `request_mutations` to the request headers (`Vec<(String, String)>`); `HeaderMutationFilter::encode_headers` applies `response_mutations` to the response headers. The `Append` action pushes a new entry (allowing duplicates per RFC 7230 §3.2.2 list-valued headers); `Overwrite` does a case-insensitive remove-then-push. ~180 LoC code + ~150 LoC unit tests (request-side append; response-side append; overwrite removes existing same-name; multiple mutations apply in order; case-insensitive remove; empty mutations is no-op; round-trip through FilterPipeline::decode_headers asserts the request was mutated).

**D10.2 — Fuzz corpus extension.** New seed `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_header_mutation_filter.yaml` — a minimal HCM with HeaderMutation + Router in the http_filters list. Add to the explicit `.gitignore` allow-list. The corpus-walk acceptance test (`fuzz_corpus_seeds_parse_or_reject_cleanly`) extends to include the new seed. ~40 LoC seed + 1 LoC gitignore + 1 LoC test array.

**D11.2 — Differential harness extension.** Reuses the existing `Driver::Http1` variant; no new variant needed. The fixture's `expected_headers` (a `Http1HeaderRule::SetEqualModuloAllowList`) asserts the response-side stamp lands on both proxies. The Http1EchoBackend (the 04.3 helper) echoes received request headers into its response body; the fixture's `expected_body: ByteExact` asserts the request-side stamp landed at the backend on both sides (the response body bytes match Envoy's because both proxies forward identical mutated request headers to the same backend). **No harness code changes** if the Http1EchoBackend's body-echo shape is already exercised at fixture 0008 (the 04.3 router-upstream fixture); if not, ~20 LoC of backend-helper extension to echo received headers as `key: value\n` lines into the response body.

**D12.2 — Fixture 0013.** `tests/fixtures/0013-http-filter-header-mutation/` — 5 files:
  - `envoy.yaml`: HCM with `http_filters: [HeaderMutation, Router]`; the HeaderMutation appends `x-filter-stamp: phase-07` on request_mutations and `x-filter-response-stamp: phase-07` on response_mutations; a single route to an Http1EchoBackend cluster.
  - `envoy-rust.yaml`: identical (modulo any per-side divergences already established for fixture 0008's STRICT_DNS / dns_lookup_family pattern).
  - `inputs/payload.bin`: 0-byte placeholder (per the 04.3 / 06.x convention for non-Http1Probe drivers).
  - `expectations.yaml`: `driver.kind: http1` with `expected_status: 200`, `expected_body: { kind: byte_exact }`, `expected_headers: { rule: set_equal_modulo_allow_list }`, plus a body-content assertion that `x-filter-stamp: phase-07` appears in the echoed body bytes (either via a new `BodyRule::ByteExactValue { value: String }` if the per-side body bytes are deterministic, or via the existing `BodyRule::ByteExact` if the bilateral assertion is sufficient).
  - `README.md`: explains the surface, the filter chain ordering, the per-side stamp assertions.
  - Docker-gated `tests/differential/tests/http_filter_header_mutation.rs` (sibling of `http1_router_upstream.rs`).
  
  ~150 LoC fixture YAML + ~30 LoC Docker-gated test.

**D13.2 — In-process backstop.** `crates/envoy-bin/tests/http_filter_header_mutation.rs` (sibling of phase 04/05/06's in-process backstops). Spawns envoy-bin against the fixture's `envoy-rust.yaml`, drives a `GET /` HTTP/1.1 request through `drive_http1`, asserts the response carries `x-filter-response-stamp: phase-07` and the response body contains `x-filter-stamp: phase-07`. ~120 LoC.

**D14.2 — Parent-07 state-6 close-out.** The 07.2 state-6 phase-done commit also flips parent ROADMAP row `07` from `in-progress` to `done` per the ROADMAP-schema invariant in `BOOTSTRAP_PROMPT.md` §4.1. Mirrors phase 04's `e626862`, phase 05's `82c26b8`, and phase 06's `b918f33` close-out shapes. STATE.md advances active phase from `07.2` lifecycle state 5 to phase `08` lifecycle state 1; next-skill `superpowers:brainstorming` scoped to phase 08's `BOOTSTRAP_PROMPT.md` §8 row-08 charter (*"Minimum admin API (config_dump, stats, clusters, listeners, ready, server_info) + graceful drain"*).

**D15.2 (verification deliverable, no code).** State-4 phase-done verification per the §7.5 gate, scoped to 07.2's surfaces + simultaneous green on all 0001-0013 fixtures.

---

## 4. Out of scope (deferred non-goals)

The following surfaces are **explicitly deferred** to later phases — phase 07 ships a minimal-viable filter-chain framework, not a comprehensive filter SDK.

- **Body-iteration states.** `decode_data` / `encode_data` / `decode_trailers` / `encode_trailers` defer. Both H1 and H2 HCMs are fully-buffered today (`Request.body: Option<Bytes>`, `Response.body: Bytes`); body iteration requires a streaming-buffering refactor that's its own phase. The MVP iteration protocol covers `decode_headers` + `encode_headers` only.
- **`StopAndSend` (local-reply) actively used by an MVP filter.** The `Decision::StopAndSend` variant is scaffolded from day one for forward-compat (the framework handles it; the HCM dispatches to the access-log site), but no 07.x filter emits it. Future filters (rate-limit, ext_authz, fault-injection) will exercise it.
- **Per-route `typed_per_filter_config` consumed by a filter.** The Route schema MAY gain a `typed_per_filter_config: BTreeMap<String, TypedConfig>` field at 07.1 (parse-and-validate scaffolding only) — but the planner at 07.1 SPEC writeup time can also defer the scaffolding entirely to keep 07.1 scope tight. **Recommended: defer to 07.2 or later** — adding the schema field without a consuming filter is premature scaffolding. The per-route override mechanism lands when the first filter needs it.
- **Buffered-decode / hold-traffic-while-async-call filters.** Filters that need to call out to a remote service (ext_authz, ext_proc) and hold the request while awaiting a response defer to whichever phase first surfaces them. These need an async-aware iteration shape (likely `async fn decode_headers`) that requires either `async_trait` or hand-rolled `Pin<Box<dyn Future>>` — both incur foundations-grant pressure (potential ADR-0031).
- **Stats-emitting filters.** Phase 07's MVP filters do not emit stats. Future filters with `http.<stat_prefix>.<filter_name>.<stat>` shapes extend BEHAVIOR_CONTRACT.md `Stat-name mapping` at their landing phase.
- **Filter-state machinery (`%FILTER_STATE%` access-log token).** Per-request filter-state storage (Envoy's `StreamFilterCallbacks::streamInfo()->filterState()`) defers indefinitely. No MVP filter populates filter state.
- **Dynamic metadata (`%DYNAMIC_METADATA%` access-log token).** Same disposition as filter-state.
- **HTTP filters beyond Router and HeaderMutation.** All listed in Mission §9 "HTTP filters family" (cors, compression, fault, local+global rate limit, jwt_authn, rbac, ext_authz, ext_proc, oauth2, csrf, buffer, lua, wasm, adaptive concurrency, admission control, bandwidth limit) defer to their respective family-phase entrances.
- **Network filters beyond TCP proxy and HCM.** Phase 02 + 04 established TCP proxy + HCM as the two network filters in scope. The full network-filters family (Mission §9) defers.
- **Wasm filter host.** Defers to its own multi-phase sub-project per Mission §9.
- **Filter-chain `is_optional` / version semantics.** Envoy's `HttpFilter.is_optional` (allows skipping unknown filters) defers; phase 07 rejects any unknown filter type via the schema's `HttpFilterTypedConfig` enum (`@type` URL not in the enum's variants).
- **Phase 06.3 REVIEW I1** (Task 11 fixup verification-discipline gap). Carries forward to phase 07 R-track signpost. The phase-07 PLAN/SPEC templates should signpost the discipline (any commit editing a fixture `expectations.yaml`'s value-side blocks must be Docker-gated-fixture-run before commit).
- **Phase 06.3 REVIEW I2** (synthetic 5xx backend + 4-class `pre_requests` deferred from 06.3 Task 11). Phase 07's scope does not introduce a synthetic backend; carries forward to upstream-robustness family.
- **Phase 06.2 REVIEW M1 / M2 / M4 / M5** — pre-existing harness / error-typing carryforwards. Not engaged by phase 07; carry forward unchanged.
- **Phase 06.1 REVIEW I2 / M1 / M4** — admin-handler defense-in-depth + DRAIN_BUDGET consolidation carryforwards. Carry forward to phase 08 (full admin endpoint surface).
- **Phase 05.3 REVIEW I2** (typed-error chain dissolution at H2 dispatch site via `format!("{e}")`). Not engaged by phase 07; carries forward.
- **Phase 05.2 REVIEW I1** (h2spec tarball SHA-256 verification in CI). Phase 07 does not edit `.github/workflows/ci.yml` under the recommended posture; carries forward unchanged.
- **Phase 04.1 REVIEW M5/M9** (Cargo.lock cadence ratification ADR). Phase 07 introduces no new top-level Cargo deps under the recommended posture; M5/M9 carries forward unchanged unless ADR-0031 actually lands at 07.x execution time.

---

## 5. Sub-phase split rationale (codified at parent-07 state-2 via ADR-0030)

**Why split.** The combined LoC estimate (~1700 LoC) and task estimate (~19 tasks) put parent-07 over the §6.1 LoC split-gate (~1500 LoC) by ~13%; task count is well under the 25-task cap. Per phase-04.3's drift experience (~+20% drift between brainstorm-time estimates and landed LoC), ~1700 × 1.2 = ~2040 LoC — well over the cap. The writer-arm refactor at H1 (D2.1) is the load-bearing complexity contributor: factoring wire-write out of each of the 5 writer arms is structurally invasive even though each arm is mechanically simple (the change is highly cross-cutting). Single-phase shipment risks mid-execution re-split (a §6.2 anti-pattern); a precautionary split keeps each sub-phase under the gate with healthy drift headroom.

**Why 2-way over 3-way.** A 3-way split (e.g., framework / HCM-integration / filter+fixture) would put each slice at ~500 LoC — too small to motivate a full state-machine cycle. The natural seam between the foundation crate + HCM wiring (architecturally one slice — the framework is incomplete without its consumer-side invocation) and the first concrete filter + fixture (the demonstration) is the 2-way split. Mirrors phase 02 (02.1: config-cluster scaffolding; 02.2: listener + TCP proxy filter as the consumer) and phase 03 (03.1: TLS foundation + downstream; 03.2: TLS upstream + SNI as the second consumer slice) precedents.

**Why this surface boundary.** The split groups the work by deliverable cohesion, not by code-area:
  - **07.1** delivers the **framework foundation + HCM integration** as one coherent slice. The framework crate (`envoy-filter`) and its consumer sites (H1 + H2 HCM) are architecturally inseparable — landing the crate without consumer wiring would leave it stranded with no behavior-equivalence proof, and landing the wiring without the crate is impossible. 07.1's differential surface is the regression-equivalence guarantee (all 12 existing fixtures stay green simultaneously).
  - **07.2** delivers the **first concrete pluggable filter (HeaderMutation) + fixture + parent close** as the demonstration slice. HeaderMutation is the first filter that the framework's iteration walks through with non-Router behavior. The fixture proves the framework's iteration semantics are equivalent to Envoy's on both decode (request-side stamp) and encode (response-side stamp) sides.

**Alternatives considered:**
  - **(i) Single phase** — rejected per LoC drift-headroom argument.
  - **(ii) Three-way split** (framework / HCM-integration / filter+fixture) — rejected; framework + HCM-integration are architecturally inseparable.
  - **(iii) Two-way split by code area** (envoy-config schema / runtime+wiring+filter) — rejected; mixes the framework foundation with the consumer-side filter in one sub-phase, leaving the schema sub-phase stranded.
  - **(iv) Two-way split with router-as-first-filter at 07.1 and HeaderMutation at 07.2** (decision) — keeps 07.1's framework foundation regression-equivalent and gives 07.2 the bilateral-fixture spotlight.

**Sub-phase ordering invariant.** Sub-phases ship strictly in order (07.1 → 07.2) — they cannot be parallelized because (a) 07.2's HeaderMutation depends on the framework's `HttpFilterInstance` enum + `FilterPipeline` types landed in 07.1, (b) 07.2's HCM-engagement of the encode-side stamp requires the encode-side iteration site landed in 07.1, (c) 07.2 closes the parent-07 ROADMAP row.

**ADR-0030 lands at parent-07 state-2** (writing-plans session) per the phase-04 / phase-05 / phase-06 precedent (ADR-0020 / ADR-0022 / ADR-0029 landed at their respective state-2 commits). ADR-0030 records the split decision; sub-phase SPECs land in the same state-2 commit.

---

## 6. Cross-sub-phase architectural invariants

These rules hold across both sub-phases; they are cross-cutting design contracts that any sub-phase's deliverables must respect.

**Rule 1 — `envoy-filter` is the sole workspace dep on filter-chain dispatch logic.** Phase 07 introduces no new permitted-foundations grants under the recommended posture (no `async_trait`, no `dyn-clone`, no `inventory`-style runtime registration). If a foundations grant becomes necessary at execution time (e.g., an async-aware iteration shape surfaces as essential), the planner lands an in-execution ADR-0031 narrowly scoped to the affected crate per D-3.5. Mirrors parent-06's posture toward conditional ADRs.

**Rule 2 — `envoy-filter` exports iteration primitives only; consumers invoke. `envoy-filter` does NOT know about HCM, listeners, clusters, or admin endpoints.** Filter invocation lives at the consumer side (`envoy-http1::serve_connection` invokes `pipeline.decode_headers` / `encode_headers`; `envoy-http2::handle_one_stream` does the same). The framework crate is a primitives library, not an integration layer. Mirrors parent-06 Rule 2 (envoy-stats exports primitives only; consumers register and increment).

**Rule 3 — `Router` is always the last entry in the filter chain.** Validator-enforced at parse time via `ConfigError::RouterNotTerminal`. Non-terminal Router rejects with the listener name + the Router's position. Mirrors Envoy v1.33's filter-chain doctrine: the Router filter terminates iteration and dispatches the request (or short-circuits to a synth response).

**Rule 4 — Filter chain runs once per H1 keep-alive request and once per H2 stream.** Per-request / per-stream scope alignment matches the 06.x stats / access-log dispatch precedent. The `FilterPipeline` is cloned from the HCMConfig-held template at per-request scope entry; mutations to per-request filter state are scoped to that request only.

**Rule 5 — Filter iteration is synchronous (non-async) on both decode and encode sides.** Both `decode_headers` and `encode_headers` are non-async methods. This avoids the `async_trait` foundations grant and matches the already-buffered request/response shape — by the time the framework runs, the request body is fully buffered (`Option<Bytes>`) and the response body is fully buffered (`Bytes`). Future async-aware filters (ext_authz, ext_proc) introduce an async iteration shape under their own ADR.

**Rule 6 — Iteration order is declaration order on decode, reverse declaration order on encode.** Matches Envoy v1.33's documented filter-chain semantics. With the chain `[HeaderMutation, Router]`, decode runs `HeaderMutation::decode_headers` then `Router::decode_headers` (terminus); encode runs `Router::encode_headers` (terminus) then `HeaderMutation::encode_headers`. The HeaderMutation filter's `response_mutations` apply AFTER the Router has populated the response (either synth or proxied), which is the expected semantic: response stamping is a post-Router operation.

**Rule 7 — `decode_headers` runs BEFORE route-match.** The HCM invokes `pipeline.decode_headers(&mut req)` immediately after request parsing and before `build_response` (which performs the VH-walk + route-walk). This lets filters mutate the request before route-match (e.g., a future header-rewriting filter can rewrite `:path` before the route matcher walks the VH). The Router's `decode_headers` is a no-op (it does not modify the request); route-match happens inside `build_response` regardless of the Router's filter-level participation. This is a phase-07 architectural choice — Envoy has a similar shape with the Router participating in decode-path filter iteration but not actually performing the match itself; the match happens implicitly via the `route` field of the StreamFilterCallbacks.

**Rule 8 — `encode_headers` runs AFTER the writer arm has populated the response but BEFORE the wire write.** The unified factored site at H1's `crates/envoy-http1/src/hcm.rs` (post-writer-arm match, pre-wire-write — requires the D2.1 writer-arm refactor) and H2's `finalize_h2_stream` (pre-`send_envoy_response`, requires the D3.1 finalize-refactor) is the encode-side invocation site. Response mutations apply to the response `Vec<(String, String)>` headers before the wire write. Access-log dispatch and per-class HCM counter increment both consume the post-encode response state (matching Envoy's semantic — access logs reflect the wire-emitted response; per-class counters bucket on the wire-emitted status). The writer-arm refactor at H1 is the load-bearing structural change of 07.1; each of the 5 H1 writer arms loses its inline wire write and instead produces a response value that the unified site consumes.

**Rule 9 — Phase 07 does NOT engage filter-state or dynamic-metadata machinery.** Per §4 above. The framework crate does not provide filter-state storage in MVP; future filters that need filter-state will introduce the machinery under their own ADR.

**Rule 10 — `envoy-filter` depends on `envoy-config` + `envoy-http1` only; HCM consumers depend on `envoy-filter`.** Crate dep graph stack: `envoy-config → envoy-filter → envoy-http1, envoy-http2 → envoy-bin`. No cycles. The `envoy-http1 → envoy-filter → envoy-http1` would-be cycle is avoided because `envoy-filter` consumes only `envoy-http1::codec` value-types, not the HCM types — and the `codec` module has no dependency on the `hcm` module within `envoy-http1`. If a future surface forces a wider dependency (e.g., access-log integration at the filter level), the recommended path is to factor a smaller `envoy-codec` crate out of `envoy-http1` rather than introduce a cycle. Flagged for whichever future phase first surfaces the pressure.

---

## 7. ADR projection

Phase 07's ADR ledger entrance state is **ADR-0029** (landed at parent-06 state-1+state-2 combined-recovery commit `1f7661a`; see DECISIONS.md tail). Phase 07 projects the following ADRs:

- **ADR-0030 (parent-07 split decision).** Lands at parent-07 state-2 (writing-plans session) alongside the two sub-phase SPECs and the new ROADMAP rows for 07.1 / 07.2. Records the 2-way split rationale per §5 above; mirrors ADR-0013 (phase-02 split), ADR-0017 (phase-03 split), ADR-0020 (phase-04 split), ADR-0022 (phase-05 split), ADR-0029 (phase-06 split) in shape and provenance discipline. Required.

- **Conditional ADR-0031 (foundations grant for `async_trait` or similar).** **Not pre-projected.** The recommended posture is no foundations grants in phase 07 (the framework's iteration is synchronous per Rule 5; the HeaderMutation filter manipulates only `Vec<(String, String)>` headers). If execution-time experience shows an async iteration shape is necessary (e.g., a filter type considered for 07.2 turns out to need an `.await`), an in-execution ADR per D-3.5 lands narrowly. The number `ADR-0031` stays available for whichever sub-phase first needs it.

- **Conditional ADR-0032 (Cargo.lock cadence ratification).** **Not pre-projected.** Phase-04.1 REVIEW M5/M9 carries forward unchanged unless ADR-0031 actually lands and forces a cadence pick. If ADR-0031 does not land, M5/M9 continues to phase 08.

- **Conditional ADR-0033+ (sub-phase-specific decisions).** Each sub-phase's brainstorm may surface unanticipated decisions worth ADR-shaped permanent records (e.g., the H2 HCM's per-stream pipeline-clone shape, the per-route override scaffolding decision, an iteration-order edge case). Numbers stay available; each sub-phase's SPEC §7 projects its own ADRs at its brainstorm time.

**ADR-renumbering provenance discipline.** If conditional ADRs do not land, their numbers stay available for later phases per the established ledger discipline (parent-04's ADR-0020 + ADR-0021 landed without renumbering; parent-05's ADR-0022 + ADR-0023 landed without renumbering; parent-06 landed only ADR-0029 with conditional ADR-0030 / ADR-0031 staying available for phase-07 — and phase-07's recommended posture stays available for phase 08+ if untouched).

---

## 8. State-machine signposts for the parent-07 state-2 session

The parent-07 state-2 session (the next session after this brainstorm; runs `superpowers:writing-plans`) operates per `SKILL_ROUTING.md` line 21: *"SPEC.md exists, PLAN.md does not → superpowers:writing-plans → output: PLAN.md → GATE: if PLAN.md > ~25 tasks OR > ~1500 LoC estimated → split into NN.1, NN.2, …; update ROADMAP + STATE; stop"*. Per the phase-04 (`1d9740d`) / phase-05 (`f1804a7`) / phase-06 (`1f7661a`) precedents, the parent-07 state-2 session lands:

1. **ADR-0030** (split decision) appended to `docs/envoy-rust/DECISIONS.md` per D-3.5 — parallel structure to ADR-0013 / ADR-0017 / ADR-0020 / ADR-0022 / ADR-0029.
2. **Two sub-phase SPECs** at `docs/envoy-rust/phases/07.1-filter-framework-foundation/SPEC.md` and `07.2-header-mutation-filter/SPEC.md`. Each sub-phase SPEC expands its own deliverables to per-task PLAN-ready cadence.
3. **Two new ROADMAP rows** (`07.1`, `07.2`) with `status: planned`.
4. **Parent ROADMAP row 07's `sub-phases` column** updated to `07.1, 07.2`. Row 07's `status` remains `in-progress` (it flipped to `in-progress` at this brainstorm's close-out commit).
5. **STATE.md** advanced to point at `07.1` lifecycle state 1 (next-skill `superpowers:brainstorming` scoped to 07.1).

The parent-07 state-2 session does **not** land per-sub-phase PLAN.md files — those land at each sub-phase's own state-2 sessions per the precedent. The parent-state-2 session writes only the parent-level split coordination artifacts.

**Sub-phase entry point.** After parent-07 state-2 lands, the next session enters phase 07.1 lifecycle state 1 — runs `superpowers:brainstorming` scoped to 07.1's surface, lands `07.1-filter-framework-foundation/SPEC.md` (the sub-phase SPEC, refining D1-D7 of this parent SPEC into per-deliverable detail), and the cycle continues.

**Execution invariants (unchanged from parent-04 / parent-05 / parent-06):**
- Sub-phases ship strictly in order. 07.2 cannot start before 07.1's state-6 close-out commit.
- Each sub-phase honors the phase-done gate from `BOOTSTRAP_PROMPT.md` §7.5 in full at its own state-4.
- Each sub-phase produces its own REVIEW.md at state-5 per `superpowers:requesting-code-review`.
- The parent-07 state-6 close-out happens at 07.2's state-6 commit (the last sub-phase's commit also flips parent row 07 to `done`), per the ROADMAP-schema invariant in `BOOTSTRAP_PROMPT.md` §4.1 and the established phase-04 / phase-05 / phase-06 close-out shape.
- **PLAN.md cadence**: per the 04.3 / 05.1 / 05.4 / 06.1 / 06.2 / 06.3 standardized posture, each sub-phase's PLAN.md lands as a standalone pre-Task-1 commit at the sub-phase's state-2 close. Phase 07's sub-phase PLAN-writers continue this cadence.
- **Execution skill at state 3**: per the user's standing preference (auto-memory `feedback_execution_style`), each sub-phase's state-3 execution uses `superpowers:subagent-driven-development`; do not present the inline-`executing-plans` fork at state-3 entry.
- **R-1 from 06.3 REVIEW**: any commit editing a fixture `expectations.yaml`'s value-side blocks (`value_exact` / `value_must_be_zero` / `value_present_only`) MUST be Docker-gated-fixture-run before commit; per-task PROGRESS should explicitly enumerate which test buckets ran (workspace tests, Docker-gated fixture, in-process backstop, fuzz). 07.2's fixture 0013 lands `expected_body` and `expected_headers` only (no value-side Prometheus-style assertion); the discipline still applies if the fixture grows to include a `BodyRule::ByteExactValue` literal assertion.

---

## 9. Commit message format

The final state-6 commit at parent-07 close (the 07.2 phase-done commit; mirrors phase 06's `b918f33`-shape) uses the standard format from `BOOTSTRAP_PROMPT.md` §5.3:

```
phase 07.2: <07.2 title> [parent 07 done] [ADR-NNNN, ...]

<summary — 1-3 sentences covering the 07.2 surface and the parent-07 close>

Differential surface: fixtures 0001-0013 green at the Docker-gated CI level; header-mutation filter bilaterally verified on both decode (request-side stamp at backend) and encode (response-side stamp at client) iteration states.
Conformance: h2spec ≥95% pass (carried forward from 05.2 baseline); no new conformance suites in phase 07.
```

The `[parent 07 done]` tag attaches to the 07.2 state-6 commit's title, mirroring phase 06.3's `b918f33` close-out. The bracketed ADR list enumerates ADRs landed across the parent-07 execution arc — at minimum ADR-0030 (split decision); plus any conditional ADRs that landed.

---

## 10. State-machine commit (this commit — parent-07 state-1 close-out)

This commit (the parent-07 state-1 brainstorm close-out) lands:

- This file (`docs/envoy-rust/phases/07-filter-chain-framework/SPEC.md`) — the parent-07 SPEC.
- `docs/envoy-rust/STATE.md` — advanced to point at phase 07 lifecycle state 2; next-skill `superpowers:writing-plans`.
- `docs/envoy-rust/ROADMAP.md` — row `07` flips `status: planned` → `status: in-progress` per the §4.1 invariant ("a phase enters `in-progress` only when STATE.md points at it" — STATE.md now points at phase 07 with state 2, so the row reflects that).

No code changes. No new ADRs at this commit (ADR-0030 lands at parent-07 state-2). DECISIONS.md ledger head remains ADR-0029.

The next session enters parent-07 state-2 — runs `superpowers:writing-plans` scoped to this parent SPEC, lands ADR-0030 + the two sub-phase SPECs + ROADMAP row updates per §8 above, and exits.
