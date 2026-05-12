# Phase 07.1 — `envoy-filter` foundation + HCM filter-chain wiring + terminal-router validator

- **Phase id:** `07.1`
- **Slug:** `07.1-filter-framework-foundation`
- **Title:** New workspace member `crates/envoy-filter/` (sole-dep-owner of HTTP filter-chain dispatch logic; `HttpFilterInstance` enum Router-only at 07.1; `FilterPipeline` struct + `Decision::{Continue, StopAndSend(Response)}` enum + non-async `decode_headers` / `encode_headers` iteration methods) + HCM filter-chain wiring at both H1 (`crates/envoy-http1/src/hcm.rs::serve_connection` — load-bearing 5-writer-arm refactor so `encode_headers` runs AFTER the writer arm has constructed the response but BEFORE the wire write) and H2 (`crates/envoy-http2/src/hcm.rs::handle_one_stream` + `finalize_h2_stream` — symmetric refactor placing `encode_headers` before `send_envoy_response`) sites + terminal-router validator at `crates/envoy-config/src/bootstrap.rs::validate_hcm` (relaxes the existing `MultipleHttpFilters` cardinality gate at lines 1335-1347 from `len != 1` to `len >= 1 AND Router-last AND no-duplicate-Router`; new `ConfigError::EmptyHttpFilters` + `RouterNotTerminal { listener, position }` + `DuplicateRouterFilter { listener }` variants). **No new fixture** — the framework is regression-equivalent under the existing Router-only chain; the state-4 phase-done gate verifies all 12 existing fixtures (`0001-tcp-echo` through `0012-access-log-file-sink`) stay green simultaneously at the Docker-gated CI level.
- **Depends on:** `06` (parent ROADMAP row `06` `done` as of commit `b918f33` — the parent-06 close-out; sub-phase rows `06.1` / `06.2` / `06.3` all `done`). Phase 07.1's surface introduces no transitive constraints from earlier phases beyond what's already landed at HEAD; the 12-fixture regression baseline is the structural input.
- **Seeded by:** Parent-07 SPEC at `docs/envoy-rust/phases/07-filter-chain-framework/SPEC.md` §3 deliverables D1.1 through D7.1, codified at the parent-07 state-2 split commit (this sub-phase SPEC's landing commit) via **ADR-0030** (the parent-07 2-way split decision). The seven D1.1-D7.1 deliverables are decomposed into per-task PLAN-ready cadence in §3 below; the projection is ~900 LoC across ~9 tasks per parent SPEC §5.
- **Differential surface when done:**
  - **Pre-existing fixtures:** `0001-tcp-echo` through `0012-access-log-file-sink` — all 12 stay green at the Docker-gated CI level simultaneously. The framework wiring (decode + encode invocation on every HCM request; H1 + H2 writer-arm refactor) operates inside the Router-only chain that every existing fixture declares; the wire-emitted behavior is regression-equivalent against upstream Envoy because the Router filter is a no-op on the iteration sides (decode is a no-op pre-route-match; encode runs after the Router has populated the response from either direct_response or upstream-proxy). The cardinality validator relaxation (`len != 1` → `len >= 1 AND Router-last AND no-duplicate-Router`) is regression-equivalent for every existing fixture (all 12 declare exactly `[Router]`, which passes both the old and the new gate).
  - **New fixtures:** **None in 07.1.** The differential surface is regression-equivalence — proving the framework wiring does not alter wire-emitted behavior under the Router-only chain. Sub-phase 07.2 lands the first new fixture (`0013-http-filter-header-mutation`) that exercises a non-Router filter.
  - **Conformance suites unchanged:** `tests/conformance/h2spec/` continues at the **≥95% pass** gate landed in 05.2 D7 (99.31%). Phase 07.1 engages no H2-framing surfaces — the framework runs between the codec edge and the route-match site.

This SPEC is the design contract for sub-phase 07.1. It refines parent-07 SPEC §3 deliverables D1.1-D7.1 to per-task PLAN-ready cadence. Each task as projected here is one numbered task in the standalone PLAN.md that this sub-phase's own state-2 session lands per the 04.3 / 05.1 / 05.4 / 06.1 / 06.2 / 06.3 standardized standalone-PLAN cadence. A stranger reading only this file plus the stable doctrine documents (`MISSION.md`, `BEHAVIOR_CONTRACT.md`, `DECISIONS.md`, `BOOTSTRAP_PROMPT.md`) and the parent-07 SPEC must be able to operate as 07.1's state-1-and-onward sessions — landing the standalone PLAN.md, executing the per-task work end-to-end, materializing the state-4 evidence, reaching the state-6 close-out commit. Per doctrine D-3.4 (context isolation), this SPEC names every file path, every type, every error variant, every test, every CI gate it touches.

---

## 1. Goal and acceptance signal

**Goal.** Land the `envoy-filter` foundation crate and wire its iteration protocol into both H1 and H2 HCM dispatch sites so that future filter types can be added by adding one enum variant per type. The framework is hand-rolled per D-3.2 (D-3.2 lists *Filter chain engine (network + HTTP filter iteration protocol)* on its **Must be written from scratch** list — no `async_trait`, no `dyn`-dispatch indirection, no factory pattern, no runtime extension registration). The MVP iteration protocol covers two states (`decode_headers`, `encode_headers`) on the already-buffered request/response shape that 04.1 + 05.2 established. Iteration is synchronous (non-async) on both sides per parent-07 SPEC §6 Rule 5 — the framework consumes only `Vec<(String, String)>` headers and `Bytes` bodies, both of which are fully buffered before the framework runs.

The framework crate's public surface (re-exported at `lib.rs`):

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

pub enum HttpFilterInstance {
    Router(RouterTerminus),     // 07.1 ships Router only
    // HeaderMutation(HeaderMutationFilter) — added in 07.2
}
```

HCM integration: `envoy_http1::hcm::HCMConfig` gains an `Arc<FilterPipeline>` field constructed once at `HCMConfig::from_config` time (parses the envoy-config `http_filters` list, builds the pipeline, wraps in `Arc`). Both H1 (`crates/envoy-http1/src/hcm.rs::serve_connection`) and H2 (`crates/envoy-http2/src/hcm.rs::handle_one_stream` + `finalize_h2_stream`) call sites invoke `pipeline.decode_headers(&mut req)` immediately after request parsing (H1) or `http_to_envoy_request` translation (H2) and before `build_response`; and invoke `pipeline.encode_headers(&mut resp)` at the unified factored site (H1: after writer-arm match, before wire write — requires the 5-writer-arm refactor) or inside `finalize_h2_stream` (H2: before `send_envoy_response`).

Validator: `crates/envoy-config/src/bootstrap.rs::validate_hcm` at lines 1335-1347 relaxes from `match hcm.http_filters.len() { 1 => check-name-router; n => MultipleHttpFilters }` to a new free function `validate_http_filters(filters: &[HttpFilter], listener_name: &str) -> Result<(), ConfigError>` that enforces (a) at least one filter, (b) exactly one filter has `name == "envoy.filters.http.router"`, (c) the Router is the last entry. New `ConfigError` variants: `EmptyHttpFilters { listener: String }`, `RouterNotTerminal { listener: String, position: usize }`, `DuplicateRouterFilter { listener: String }`. The existing `MultipleHttpFilters { count: usize }` variant is retained but no longer constructed (kept for ledger discipline; doc-comment notes its supersession). The existing `UnsupportedHttpFilter` continues firing on any non-Router filter at 07.1 — the `HttpFilterTypedConfig` enum still has only the `Router` variant until 07.2 adds `HeaderMutation`.

**Acceptance signal — `BOOTSTRAP_PROMPT.md` §7.5 phase-done gate, scoped to sub-phase 07.1's surface:**

- **(a)** **No new differential fixture** lands in 07.1. The framework's regression-equivalence under the existing Router-only chain is the proof.
- **(b)** All 12 pre-existing differential fixtures (`0001-tcp-echo` through `0012-access-log-file-sink`) are green simultaneously at the Docker-gated CI level under a single CI run at HEAD = 07.1's state-4 evidence commit. This is the load-bearing structural guarantee — proves the framework wiring + writer-arm refactors introduce no wire-behavior regression at any layer (TCP / TLS / HTTP/1.1 / HTTP/2 / access-log / admin / stats).
- **(c)** `tests/conformance/h2spec/` continues to pass at **≥95%** with `known-failures.txt` unchanged. Phase 07.1 engages no H2-framing surfaces; the H2 `finalize_h2_stream` refactor moves `encode_headers` invocation in front of `send_envoy_response` but does not touch the codec edge.
- **(d)** The existing `parse_bootstrap` fuzz target runs clean for its short-budget CI run (`cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30`) — 07.1 introduces no new fuzz target and no new corpus seeds (the schema additions for HeaderMutation defer to 07.2; 07.1's validator-relaxation surface is exercised by the existing `MultipleHttpFilters` / `UnsupportedHttpFilter` corpus seeds).
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, and `cargo deny check` are all clean on the stable-toolchain CI job.
- **(f)** `REVIEW.md` is approved (`Approved` or `Approved with M-track follow-ups`).

The 07.1 state-6 phase-done commit advances the parent-07 ROADMAP row's sub-phase set (07.1's row flips `planned` → `done`; row 07's `status` stays `in-progress`); the parent-07 close lands at 07.2's state-6 commit per the closing-sub-phase invariant.

---

## 2. Behavior-contract scope for sub-phase 07.1

Sub-phase 07.1 is the first sub-phase to introduce a filter-chain-iteration surface to `docs/envoy-rust/BEHAVIOR_CONTRACT.md`. The expected updates are **none under the recommended posture**:

1. **`Header allow-list` — no new entries.** 07.1's framework wiring does not introduce any new response headers; the Router filter is a no-op on `encode_headers` (it does not modify the response value), so the wire-emitted headers under the Router-only chain are identical to today's pre-framework behavior. The existing entries (`server`, `date`, `x-envoy-upstream-service-time`) are unaffected.

2. **`Stat-name mapping` — no new entries.** 07.1's framework does NOT emit any stats. Future filters (rate-limit, ext_authz) will introduce filter-emitted stats `http.<stat_prefix>.<filter_name>.<stat>`; those phases extend the table.

3. **`Access log field mapping` — no new tokens.** 07.1 does NOT introduce filter-state or dynamic-metadata access-log tokens. `%FILTER_STATE%` and `%DYNAMIC_METADATA%` remain deferred per parent-07 SPEC §4.

4. **New subsection `Filter chain iteration` — recommended deferred.** If empirical evidence at state-4 verification surfaces an iteration-order divergence between Envoy and envoy-rust, a new subsection lands. The recommended posture is to defer this subsection — the 12-fixture-green simultaneous CI evidence is the implicit canonicalization that the framework's declared iteration order (declaration order on decode, reverse declaration order on encode per parent-07 SPEC §6 Rule 6) matches Envoy's. If a sub-phase reviewer flags a divergence at state 5, the 07.1 R-track posture is to land a new subsection at the review-fix commit.

5. **`xDS wire state machine` and `Timing tolerances` subsections — untouched.** Phase 07.1 does not engage xDS or timing-sensitive features.

If a sub-phase task discovers an empirical divergence that demands a BEHAVIOR_CONTRACT.md edit, the edit lands at that task's commit + the ADR landing flag (in-execution ADR per D-3.5) per the standard cadence; do not batch.

---

## 3. Deliverables (per-task PLAN-ready cadence)

The seven D1.1-D7.1 deliverables of parent SPEC §3 decompose into **9 numbered tasks** for the standalone PLAN.md. The recommended execution order is **1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9**; the dependencies between tasks are listed inline.

### Task 1 — Create `crates/envoy-filter/` scaffold + `FilterError` typed-error enum (D1.1 part 1)

**Files created:**
- `crates/envoy-filter/Cargo.toml` — new workspace member.
- `crates/envoy-filter/src/lib.rs` — crate root.
- `crates/envoy-filter/src/error.rs` — `FilterError` typed-error enum.

**Files modified:**
- `Cargo.toml` (workspace root) — append `crates/envoy-filter` to `[workspace] members`.

**Crate dependencies** (per `Cargo.toml`):
```toml
[dependencies]
bytes = "1"
thiserror = "2"
tracing = "0.1"
envoy-config = { path = "../envoy-config" }
envoy-http1 = { path = "../envoy-http1" }
```

No new permitted-foundations grants. Both `bytes` and `thiserror` are existing workspace foundations; `tracing` is on the permitted-foundations list. `envoy-config` and `envoy-http1` are existing workspace crates; the new crate depends on them via `path =` per the established workspace-member convention.

**`lib.rs`** opens with `#![forbid(unsafe_code)]` per D-3.8 (no ADR exemption requested; the framework manipulates only safe primitives). Re-exports the public surface:

```rust
#![forbid(unsafe_code)]

//! HTTP filter chain iteration protocol.
//!
//! Hand-rolled per D-3.2's "Must be written from scratch" doctrine for filter
//! chain engines. Synchronous (non-async) iteration on the already-buffered
//! request/response shape.

pub mod error;
pub mod instance;
pub mod pipeline;

pub use error::FilterError;
pub use instance::HttpFilterInstance;
pub use pipeline::{Decision, FilterPipeline};
```

**`error.rs`** defines `FilterError`:

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FilterError {
    #[error("filter chain is empty (must contain at least Router)")]
    EmptyChain,

    #[error("expected Router at terminus position {expected}, got filter named {actual:?} at position {position}")]
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
```

Most parse-time validation lives in `envoy-config::validate_http_filters` (Task 4); `FilterError` exists for the residual cases where the framework's `build_from_config` arm asserts an invariant the validator would also catch (defense-in-depth) plus future runtime errors (e.g., StopAndSend invariants).

**Tests** (unit; in `error.rs` `#[cfg(test)] mod tests`):

1. `FilterError::Display` is human-readable on each variant (one assertion per variant; ~5 lines per variant).
2. `FilterError` is `Send + Sync + 'static` (static assertion via `fn _assert_send_sync<T: Send + Sync + 'static>()` helper). Required so the error can flow through tokio task boundaries.

**Code budget:** ~100 LoC (Cargo.toml ~10 + lib.rs ~25 + error.rs ~40 + tests ~25).

**Commit message:** `phase 07.1: task 1 — envoy-filter crate scaffold + FilterError`.

### Task 2 — `FilterPipeline` + `Decision` enum + iteration-loop skeleton (D1.1 part 2)

**Files created:**
- `crates/envoy-filter/src/pipeline.rs` — `FilterPipeline` struct + `Decision` enum + iteration methods.

**Public surface:**

```rust
use envoy_http1::codec::{Request, Response};
use crate::error::FilterError;
use crate::instance::HttpFilterInstance;

pub enum Decision {
    Continue,
    StopAndSend(Response),
}

pub struct FilterPipeline {
    filters: Vec<HttpFilterInstance>,
}

impl FilterPipeline {
    /// Build a FilterPipeline from a parsed envoy-config HttpFilter list.
    ///
    /// Returns an error if the list is empty or contains an unsupported
    /// filter type. The parse-time validator at
    /// `envoy_config::validate_http_filters` performs the same checks earlier
    /// in the config-load path; this method's checks are defense-in-depth.
    pub fn build_from_config(
        filters: &[envoy_config::HttpFilter],
    ) -> Result<Self, FilterError> {
        let mut out = Vec::with_capacity(filters.len());
        for (position, hf) in filters.iter().enumerate() {
            out.push(HttpFilterInstance::build(hf, position)?);
        }
        Ok(Self { filters: out })
    }

    pub fn decode_headers(&mut self, req: &mut Request) -> Decision {
        for filter in self.filters.iter_mut() {
            match filter.decode_headers(req) {
                Decision::Continue => continue,
                Decision::StopAndSend(resp) => return Decision::StopAndSend(resp),
            }
        }
        Decision::Continue
    }

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
```

**Iteration-order rationale** (codified inline as a doc-comment on the impl block, referencing parent-07 SPEC §6 Rule 6):

- `decode_headers` walks `filters.iter_mut()` — **declaration order**.
- `encode_headers` walks `filters.iter_mut().rev()` — **reverse declaration order**.

This matches Envoy v1.33's documented filter-chain semantics.

**Tests** (unit; in `pipeline.rs` `#[cfg(test)] mod tests`):

1. `build_from_config` on an empty slice returns `Err(FilterError::EmptyChain)`.
2. `build_from_config` on a single `Router`-named entry with `RouterConfig` typed_config returns `Ok` with `filters.len() == 1`.
3. `decode_headers` on a `[Router]` pipeline returns `Decision::Continue` and does not mutate the request. (Router's `decode_headers` is the no-op terminus.)
4. `encode_headers` on a `[Router]` pipeline returns `Decision::Continue` and does not mutate the response. (Router's `encode_headers` is the no-op terminus.)
5. Iteration-order test using a hand-crafted `HttpFilterInstance` test-only variant (gate-marked `#[cfg(test)]`) that pushes its position onto a shared `Vec<usize>`: a pipeline of 3 test-filters + Router walks `[0, 1, 2]` on decode and `[3 (Router), 2, 1, 0]` on encode. (Tests the future-proofing of the reverse-iteration on encode; Router is a terminus, so on decode the iteration reaches Router last; on encode, the reverse order means Router fires first.)

**Important note on test-only variant.** Since `HttpFilterInstance` is an enum defined in a sibling module (`instance.rs`), the test-only variant must be added inside the enum behind `#[cfg(test)]` so the enum is non-exhaustively typed in tests. Alternative: the iteration-order test lives in `pipeline.rs` `#[cfg(test)] mod tests` and uses a private test-helper enum that mimics the iteration contract. The planner picks the cleaner shape at 07.1 state-2 PLAN-writeup time.

**Code budget:** ~120 LoC (pipeline.rs impl ~50 + Decision enum + struct ~20 + tests ~50).

**Dependencies:** Task 1 (FilterError types must exist).

**Commit message:** `phase 07.1: task 2 — FilterPipeline + Decision + iteration loop`.

### Task 3 — `HttpFilterInstance` enum (Router-only) + `RouterTerminus` (D1.1 part 3)

**Files created:**
- `crates/envoy-filter/src/instance.rs` — `HttpFilterInstance` enum + `HttpFilterInstance::build`.
- `crates/envoy-filter/src/router.rs` — `RouterTerminus` struct (the chain terminator; no-op on both iteration sides).

**`instance.rs`** defines:

```rust
use envoy_http1::codec::{Request, Response};
use crate::error::FilterError;
use crate::pipeline::Decision;
use crate::router::RouterTerminus;

pub enum HttpFilterInstance {
    Router(RouterTerminus),
}

impl HttpFilterInstance {
    pub(crate) fn build(
        hf: &envoy_config::HttpFilter,
        position: usize,
    ) -> Result<Self, FilterError> {
        match &hf.typed_config {
            envoy_config::HttpFilterTypedConfig::Router(_cfg) => {
                Ok(HttpFilterInstance::Router(RouterTerminus::new()))
            }
            // HeaderMutation arm lands in 07.2.
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
```

**`router.rs`** defines:

```rust
use envoy_http1::codec::{Request, Response};
use crate::pipeline::Decision;

/// The terminus of every filter chain.
///
/// `Router` is the filter that dispatches to the route's action (direct_response
/// or upstream proxy). At the filter-chain level it is a no-op on both
/// iteration sides — the actual dispatch happens inside the HCM's writer-arm
/// match after `pipeline.decode_headers` returns and route-match runs.
///
/// The validator guarantees Router is the last entry; on decode this means
/// `Router::decode_headers` runs after every other filter has had a chance to
/// mutate the request; on encode (reverse order) this means `Router::encode_headers`
/// runs FIRST among all filters, which models Envoy's semantic of "the Router
/// filter produces the response and other filters mutate it on the encode side".
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
```

**Tests** (unit; in `instance.rs` `#[cfg(test)] mod tests` and `router.rs` `#[cfg(test)] mod tests`):

1. `HttpFilterInstance::build` on a `Router`-typed HttpFilter returns `Ok(Router(_))`.
2. `RouterTerminus::decode_headers` returns `Continue` and does not mutate the request (assert request bytes-identical after the call).
3. `RouterTerminus::encode_headers` returns `Continue` and does not mutate the response.

**Code budget:** ~110 LoC (instance.rs ~50 + router.rs ~40 + tests ~20).

**Dependencies:** Tasks 1 (FilterError) + 2 (Decision).

**Commit message:** `phase 07.1: task 3 — HttpFilterInstance Router-only + RouterTerminus`.

### Task 4 — `envoy-config` validator relaxation (D4.1)

**Files modified:**
- `crates/envoy-config/src/bootstrap.rs` — at lines 1335-1347 (the existing `MultipleHttpFilters` cardinality gate) and at the `ConfigError` enum.

**Pre-state (today, HEAD `7337f2c`):**

```rust
// crates/envoy-config/src/bootstrap.rs, around line 1335-1347
match hcm.http_filters.len() {
    0 => return Err(ConfigError::MultipleHttpFilters { count: 0 }),
    1 => {
        let filter = &hcm.http_filters[0];
        if filter.name != "envoy.filters.http.router" {
            return Err(ConfigError::UnsupportedHttpFilter {
                name: filter.name.clone(),
            });
        }
    }
    n => return Err(ConfigError::MultipleHttpFilters { count: n }),
}
```

**Post-state (after 07.1 Task 4):**

```rust
// crates/envoy-config/src/bootstrap.rs, same call site
validate_http_filters(&hcm.http_filters, listener_name)?;
```

with a new free function below in the same file:

```rust
fn validate_http_filters(
    filters: &[HttpFilter],
    listener_name: &str,
) -> Result<(), ConfigError> {
    if filters.is_empty() {
        return Err(ConfigError::EmptyHttpFilters {
            listener: listener_name.to_string(),
        });
    }

    let router_name = "envoy.filters.http.router";
    let last_index = filters.len() - 1;
    let mut router_count = 0usize;
    let mut router_positions: Vec<usize> = Vec::new();
    for (i, f) in filters.iter().enumerate() {
        // Per-filter typed_config invariants are checked here.
        // At 07.1 the only allowed typed_config is Router (HeaderMutation lands in 07.2).
        match &f.typed_config {
            HttpFilterTypedConfig::Router(_) => {
                router_count += 1;
                router_positions.push(i);
            }
            // 07.2 adds HeaderMutation arm here (no validator gate at the variant level
            // beyond what 07.2 Task 8 introduces).
        }
        // The schema's HttpFilterTypedConfig enum is closed; serde's deny_unknown_fields
        // on the typed_config tag (the `@type` URL) rejects unknown typed_config types
        // at parse time, so UnsupportedHttpFilter at the *name*-only level is dead code
        // — kept as a safety net for the case where the `@type` happens to be
        // a HttpFilterTypedConfig variant but the `name` field disagrees with it
        // (operator typo). At 07.1 the only typed_config is Router; we check the
        // name matches the typed_config variant.
        if let HttpFilterTypedConfig::Router(_) = &f.typed_config {
            if f.name != router_name {
                return Err(ConfigError::UnsupportedHttpFilter {
                    name: f.name.clone(),
                });
            }
        }
    }

    if router_count == 0 {
        return Err(ConfigError::RouterNotTerminal {
            listener: listener_name.to_string(),
            position: last_index,
        });
    }
    if router_count > 1 {
        return Err(ConfigError::DuplicateRouterFilter {
            listener: listener_name.to_string(),
        });
    }
    // Exactly one Router; must be at the terminus.
    let router_position = router_positions[0];
    if router_position != last_index {
        return Err(ConfigError::RouterNotTerminal {
            listener: listener_name.to_string(),
            position: router_position,
        });
    }
    Ok(())
}
```

**New `ConfigError` variants** (append to the existing enum; per D-3.5 ADR-discipline this is a schema additive change and does not require an ADR — the existing `ConfigError` enum is a typed-error grow-only structure per the parent-04 / parent-05 / parent-06 precedents):

```rust
#[error("HCM listener {listener:?} has empty http_filters list (must contain at least Router)")]
EmptyHttpFilters { listener: String },

#[error("HCM listener {listener:?}: Router filter is not at the terminus (found at position {position})")]
RouterNotTerminal { listener: String, position: usize },

#[error("HCM listener {listener:?}: filter chain contains duplicate Router filter")]
DuplicateRouterFilter { listener: String },
```

The existing `MultipleHttpFilters` variant is retained (no longer constructed; its doc-comment is updated to note: *"Superseded by `EmptyHttpFilters` / `RouterNotTerminal` / `DuplicateRouterFilter` at 07.1; retained for ledger discipline per D-3.5. No code path constructs this variant after 07.1 Task 4."*). The existing `UnsupportedHttpFilter` variant is retained and continues firing on a `name`/`typed_config` mismatch.

**Tests** (unit; in `bootstrap.rs` `#[cfg(test)] mod validate_http_filters_tests`):

1. **Positive — single Router passes.** `validate_http_filters(&[HttpFilter { name: "envoy.filters.http.router", typed_config: Router(_) }], "listener0")` returns `Ok`.
2. **Negative — empty list rejects.** `validate_http_filters(&[], "listener0")` returns `Err(EmptyHttpFilters { listener: "listener0" })`.
3. **Negative — Router not last rejects.** Construct a 2-entry list with Router at index 0 and a (07.2-projected) HeaderMutation at index 1; expect `Err(RouterNotTerminal { listener: "listener0", position: 0 })`. **Note:** at 07.1 this requires a test-only HttpFilter with a HeaderMutation typed_config that doesn't exist yet — instead, the planner constructs a list with `Router` twice at indices 0 and 1; the second-Router case is the `DuplicateRouterFilter` arm, not the `RouterNotTerminal` arm. **Resolution at 07.1 PLAN-writeup time:** if HeaderMutation truly doesn't exist at 07.1 schema time, this test is gated `#[cfg(feature = "test-header-mutation-stub")]` or deferred to 07.2 Task 1; OR, the planner adds a test-only `HttpFilterTypedConfig::__TestStub` variant that's `#[cfg(test)]`. Recommended posture: defer the `RouterNotTerminal` positive-test-with-non-Router-at-tail case to 07.2 Task 1; at 07.1 the unit test for `RouterNotTerminal` uses the `DuplicateRouterFilter` short-circuit path (impossible to construct a non-Router non-empty list at 07.1 without HeaderMutation; the validator path is still exercised by the empty case and the duplicate case).
4. **Negative — duplicate Router rejects.** Construct a 2-Router list; expect `Err(DuplicateRouterFilter { listener: "listener0" })`.
5. **Negative — name/typed_config mismatch rejects.** Construct an HttpFilter with `name: "envoy.filters.http.fault"` but `typed_config: Router(_)`; expect `Err(UnsupportedHttpFilter { name: "envoy.filters.http.fault" })`. (This exercises the existing UnsupportedHttpFilter arm under the new validator.)
6. **Property — Router-only single-entry passes** (parametric over the listener name string).
7. **Property — empty rejects independently of listener name.**
8. **Property — duplicate-Router rejects on any pair of positions.**

**Pre-existing fixture re-verification.** All 12 existing fixtures declare `http_filters: [Router]` (single Router). Each must continue to parse + validate under the new validator. The state-4 phase-done gate confirms this end-to-end via the 12-fixture-green simultaneous CI run.

**Code budget:** ~60 LoC schema (3 new ConfigError variants) + ~80 LoC validator function + ~70 LoC unit tests + ~10 LoC at the call site = ~220 LoC.

**Dependencies:** None on prior 07.1 tasks (this task can execute before Tasks 1-3 by inverting the order — but the recommended order is 1 → 2 → 3 → 4 because the FilterPipeline's `build_from_config` invariants mirror the validator's; landing the validator after the FilterPipeline gives the planner a chance to validate the parallel logic by inspection).

**Commit message:** `phase 07.1: task 4 — envoy-config terminal-router validator + 3 new ConfigError variants`.

### Task 5 — H1 HCM 5-writer-arm refactor (D2.1 part 1; load-bearing structural change)

**Files modified:**
- `crates/envoy-http1/src/hcm.rs` — lines 378-516 (the existing 5 writer arms) plus the unified factored site at lines 527-533 (today: per-class counter increment + access-log dispatch).
- `crates/envoy-http1/src/router.rs` — `write_proxied_response` function gets factored.

**The five writer arms today** (HEAD `7337f2c`):

| Arm | Line | Action | Wire write |
|---|---|---|---|
| Synth (direct_response) | 383 | `Http1Response::write_to(&resp, &mut downstream).await?` | inline |
| Proxy success | 471 | `router::write_proxied_response(&mut downstream, ..., upstream_response, ...).await?` | inline |
| Proxy send-fail 502 | 484 | synth-502 → `Http1Response::write_to` | inline |
| Proxy connect-fail 502 | 499 | synth-502 → `Http1Response::write_to` | inline |
| Proxy no-endpoint 503 | 512 | synth-503 → `Http1Response::write_to` | inline |

Each arm populates the `response_status_for_log` / `response_body_len` / `response_headers_for_log` / `upstream_host_for_log` locals before its wire-write call. The unified factored site at lines 527-533 today increments per-class HCM counters then dispatches the access-log fire-and-forget.

**The refactor (per parent-07 SPEC §3 D2.1 Signpost 1).** Per the recommended posture (option b — eagerly materialize the proxy-success arm's response into an `Http1Response` value at the arm itself), each writer arm produces an `Http1Response` value WITHOUT writing the wire. The unified factored site then:

1. (Task 6 wires this) `pipeline.encode_headers(&mut outgoing)` — applies filter mutations to the response. `Decision::StopAndSend(replacement)` substitutes the response.
2. Re-populate `response_status_for_log` / `response_body_len` / `response_headers_for_log` from the post-encode response (so access-log + per-class counter reflect the wire-emitted shape).
3. Per-class HCM counter increment (existing 06.3 site; unchanged logic).
4. Wire write — `Http1Response::write_to(&outgoing, &mut downstream).await?`.
5. Access-log build + dispatch (existing 06.2 site).

**Task 5 lands ONLY the refactor itself** — Step (1) (filter invocation) is gated to Task 6. After Task 5, the unified site runs Steps (2)-(5) on the writer-arm-constructed `Http1Response` (Step 2 is a re-population that's identical to today's pre-write population since no filter mutated the response yet).

**Concretely** — each of the 5 writer arms changes from:

```rust
// Today (HEAD 7337f2c):
WriterOutcome::Synth(resp) => {
    response_status_for_log = resp.status;
    response_headers_for_log = resp.headers.clone();
    response_body_len = resp.body.len() as u64;
    upstream_host_for_log = None;
    Http1Response::write_to(&resp, &mut downstream).await?;
    // close-flag bookkeeping ...
}
```

to:

```rust
// After Task 5:
WriterOutcome::Synth(resp) => {
    outgoing = resp;
    upstream_host_for_log = None;
}
```

with the unified site below the arm-match running:

```rust
// At line ~527 (post-arm, pre-existing per-class counter site):

// Task 6 will insert: pipeline.encode_headers(&mut outgoing) here.

response_status_for_log = outgoing.status;
response_headers_for_log = outgoing.headers.clone();
response_body_len = outgoing.body.len() as u64;

// Existing 06.3 per-class HCM counter increment site (unchanged):
hcm_stats.downstream_rq_total.inc();
match outgoing.status / 100 {
    2 => hcm_stats.downstream_rq_2xx.inc(),
    // ...
}

// NEW (lifted out of each arm's inline call):
Http1Response::write_to(&outgoing, &mut downstream).await?;

// Existing 06.2 access-log dispatch site (unchanged).
```

**Proxy-success arm** is the most invasive case. Today (HEAD `7337f2c`) the arm calls `router::write_proxied_response(...)` which mixes response construction + wire write + the per-message-header content-length / x-envoy-upstream-service-time injection. Task 5 splits `write_proxied_response` into:

- `construct_proxied_response(upstream: UpstreamResponse, cluster: &ClusterHandle, elapsed_ms: u128) -> Http1Response` — builds the response value, including content-length, x-envoy-upstream-service-time, and cluster-side stat increments. Same signature minus the `&mut downstream` and `.await`.
- The existing `write_proxied_response` is removed (or kept as a thin wrapper that calls `construct_proxied_response` + `Http1Response::write_to` for any callers outside hcm.rs — at HEAD the function has no callers outside hcm.rs, so it's fully removable).

The proxy-success arm becomes:

```rust
WriterOutcome::ProxySuccess { upstream, cluster, elapsed_ms } => {
    outgoing = router::construct_proxied_response(upstream, &cluster, elapsed_ms);
    upstream_host_for_log = Some(cluster.upstream_host.to_string());
    // cluster.upstream_rq_total + upstream_rq_5xx already incremented inside
    // construct_proxied_response (preserved from write_proxied_response).
}
```

**`outgoing` local declaration.** Add at the top of the writer-arm scope: `let mut outgoing: Http1Response;`. Per the H2 parallel and the 06.2/06.3 declaration-discipline (let-then-assign on every arm), the local is uninitialized until the match populates it.

**Tests** (unit + integration; in `crates/envoy-http1/src/hcm.rs` `#[cfg(test)] mod tests`):

1. **Direct-response arm produces correct Http1Response** (synth arm). Construct a HCMConfig with a direct_response route returning status 418 with body `"hello"`; drive a request through `serve_connection`; assert the wire output's status line is `HTTP/1.1 418 ...` and body is `"hello"`. (Today this assertion passes via the inline wire write; after Task 5 it should still pass via the unified factored wire write.)
2. **Proxy-success arm produces correct Http1Response** (proxy arm). Use a mock UpstreamResponse with status 200 + custom headers; assert `construct_proxied_response` returns an `Http1Response` carrying the upstream status + headers + injected `content-length` + injected `x-envoy-upstream-service-time`.
3. **Synth-502 / synth-503 arms produce correct Http1Response.** One assertion per arm; each checks the synth response's status + body match upstream Envoy's documented synth shapes (carried forward unchanged from 04.3 + 05.3 baselines).
4. **All 12 pre-existing in-process backstop tests** (the workspace's existing `crates/envoy-bin/tests/*.rs` and per-crate tests against fixtures 0001-0012) continue to pass without modification. These are regression tests; the refactor preserves wire-emitted behavior.

**Workspace test bucket** — `cargo test --workspace` must stay green at Task 5's commit.

**Code budget:** ~80 LoC refactor at hcm.rs (5 arms × ~5 LoC reduction each + ~30 LoC unified-site additions) + ~40 LoC factoring at router.rs (`construct_proxied_response` extracted from `write_proxied_response`) + ~80 LoC unit tests + ~10 LoC `let outgoing` declaration + scope adjustments = ~210 LoC.

**Dependencies:** None on Tasks 1-4 (this task is a pure refactor; it does not yet invoke any filter machinery). Task 5 is a structural prerequisite for Task 6.

**Commit message:** `phase 07.1: task 5 — H1 HCM 5-writer-arm refactor (factor wire-write to unified site)`.

### Task 6 — H1 HCM filter invocation wiring (D2.1 part 2)

**Files modified:**
- `crates/envoy-http1/src/hcm.rs` — adds the `decode_headers` call site + the `encode_headers` call site + the HCMConfig field for the FilterPipeline.

**`HCMConfig` schema addition:**

```rust
pub struct HCMConfig {
    // ... existing fields ...
    pub filter_pipeline: Arc<FilterPipelineTemplate>,
}
```

where `FilterPipelineTemplate` is a Cloneable shape of `FilterPipeline` — at 07.1, since all filter instances are zero-state (Router is `RouterTerminus { _private: () }`), the template + per-request clone are identical. **Signpost — at 07.1, `FilterPipelineTemplate` can be `Arc<FilterPipeline>` directly with `clone()` returning a no-op per-instance clone** (since `RouterTerminus` is `Clone + Default`). The planner at 07.1 SPEC writeup decided this — the per-request clone is cheap because `Vec<HttpFilterInstance>` with single-Router-entry is `[RouterTerminus]` which is a 0-byte clone. When 07.2 introduces `HeaderMutationFilter` with `Vec<RuntimeHeaderMutation>` state, the clone shape becomes `Arc<Vec<RuntimeHeaderMutation>>`-shared (per-request reads, no per-request writes).

**Construction site** — `HCMConfig::from_config` (or wherever the HCMConfig is built today from envoy-config types):

```rust
pub fn from_config(
    cfg: &envoy_config::HttpConnectionManagerConfig,
) -> Result<Self, HCMConfigError> {
    // ... existing fields ...
    let pipeline = envoy_filter::FilterPipeline::build_from_config(&cfg.http_filters)
        .map_err(HCMConfigError::FilterPipeline)?;
    Ok(Self {
        // ... existing fields ...
        filter_pipeline: Arc::new(pipeline),
    })
}
```

New `HCMConfigError::FilterPipeline(envoy_filter::FilterError)` variant added inline (thiserror `#[from]`). The existing `HCMConfigError` enum is grow-only per the parent-04 / parent-05 precedents.

**Decode-side invocation** — at the per-request loop in `serve_connection`, immediately after `crate::codec::parse_request` returns Ok and immediately before `build_response`:

```rust
// Today (HEAD 7337f2c):
let mut req = parse_request(...).await?;
let route_outcome = build_response(&req, ...);

// After Task 6:
let mut req = parse_request(...).await?;
let mut pipeline = (*config.filter_pipeline).clone();
let decode_decision = pipeline.decode_headers(&mut req);
let outcome = match decode_decision {
    Decision::Continue => RequestPath::Match(build_response(&req, ...)),
    Decision::StopAndSend(resp) => RequestPath::SynthFromDecode(resp),
};
```

`RequestPath` is a new local enum (private to hcm.rs) that captures whether the request went through the writer-arm match (`Match`) or was short-circuited by a `StopAndSend` from decode (`SynthFromDecode`). The writer-arm match consumes only the `Match` variant; the `SynthFromDecode` variant feeds `outgoing` directly at the unified site.

**Encode-side invocation** — at the unified factored site (post-writer-arm, pre-wire-write — established by Task 5):

```rust
// After Task 6, post-writer-arm-match:
let mut outgoing = match outcome {
    RequestPath::Match(arm_result) => arm_result.outgoing,
    RequestPath::SynthFromDecode(resp) => resp,
};
// Task 6 adds:
let encode_decision = pipeline.encode_headers(&mut outgoing);
if let Decision::StopAndSend(replacement) = encode_decision {
    outgoing = replacement;
}
// ... existing post-encode logic from Task 5 ...
```

**Iteration-protocol invariant.** `encode_headers` always fires once per response, regardless of whether the decode side issued `StopAndSend`. This matches Envoy v1.33's semantic (encode runs on every response; the framework guarantees one encode pass).

**Tests** (unit + integration; in `crates/envoy-http1/src/hcm.rs` `#[cfg(test)] mod tests`):

1. **HCMConfig::from_config builds the pipeline.** Construct an `envoy_config::HttpConnectionManagerConfig` with `http_filters: [Router]`; assert `HCMConfig::from_config(&cfg)` returns `Ok(cfg)` with `cfg.filter_pipeline.filters.len() == 1`.
2. **HCMConfig::from_config errors on empty http_filters.** Construct a config with `http_filters: []`; assert `Err(HCMConfigError::FilterPipeline(FilterError::EmptyChain))`. **Note:** the envoy-config validator (Task 4) would catch this earlier at parse time; the test verifies the defense-in-depth.
3. **`decode_headers` fires before route-match.** Drive a request through `serve_connection` with a HCMConfig that includes a test-only filter (gated `#[cfg(test)]`) that mutates `req.path` from `/foo` to `/bar`; assert the route matcher saw `/bar`. **Note:** at 07.1 there's no test-only filter to instrument this; the assertion is implicit via the iteration-order test of Task 2 + the regression-equivalence proof at state-4.
4. **`encode_headers` fires after writer-arm response construction but before wire write.** Drive a request through `serve_connection` with a HCMConfig that includes a test-only filter (gated `#[cfg(test)]`) that mutates `resp.headers` to add `x-test-encode: ok` on the encode side; assert the wire output carries `x-test-encode: ok`. **Note:** same gating as test 3.
5. **`StopAndSend` at decode skips route-match.** Wire a test-only filter that emits `StopAndSend(synth_403)` on decode; assert the wire output is the synth 403 and the route matcher was NOT invoked. **Note:** same gating as test 3.
6. **`StopAndSend` at encode substitutes the wire-emitted response.** Wire a test-only filter that emits `StopAndSend(synth_500)` on encode; assert the wire output is the synth 500. **Note:** same gating as test 3.
7. **Access-log reflects post-encode headers.** Configure an HCMConfig with both an access_log and a test-only filter that adds `x-test-encode: ok` on encode; drive a request; assert the access log line's `RESP(X-TEST-ENCODE)` token captures `ok`. **Note:** same gating as test 3. **Resolution at 07.1 PLAN-writeup time:** since 07.1 ships only `Router` (no non-Router filter), tests 3-7 are gated to a 07.1-internal-only `#[cfg(test)]` `HttpFilterInstance::TestStub` variant OR deferred to 07.2 Task 5 (the first task that wires HeaderMutation). Recommended posture: **defer tests 3-7 to 07.2 Task 5** — the wire-equivalence proof at 07.1 state-4 (all 12 existing fixtures green simultaneously) IS the test of "no behavior regression," and the in-process backstop tests for tests 3-7 land naturally with the first non-Router filter at 07.2.
8. **Regression-equivalence under Router-only chain.** All 12 existing fixtures continue to pass at the in-process backstop level. This is a re-verification of the workspace test bucket at Task 6's commit.

**Workspace test bucket** — `cargo test --workspace` must stay green at Task 6's commit.

**Code budget:** ~70 LoC at hcm.rs (HCMConfig field + from_config wiring + decode invocation + encode invocation + RequestPath local enum + HCMConfigError variant) + ~30 LoC unit tests at 07.1 scope (tests 1, 2, 8; tests 3-7 deferred to 07.2) = ~100 LoC.

**Dependencies:** Tasks 1, 2, 3 (all framework types must exist); Task 4 (validator should be landed so the HCMConfig::from_config defense-in-depth check on top of the validator is meaningful); Task 5 (writer-arm refactor must be landed so the unified site exists for the encode invocation).

**Commit message:** `phase 07.1: task 6 — H1 HCM filter-chain decode/encode invocation`.

### Task 7 — H2 HCM filter invocation wiring + `finalize_h2_stream` refactor (D3.1)

**Files modified:**
- `crates/envoy-http2/src/hcm.rs` — `handle_one_stream` and `finalize_h2_stream`.

**HCMConfig consumption.** Per the 05.2 D3 contract, `envoy_http2::HCMConfig` is a type alias to `envoy_http1::hcm::HCMConfig`; the `filter_pipeline` field added in Task 6 is automatically present. The H2 HCM consumes it via the same `Arc<FilterPipeline>` clone-per-stream shape.

**Decode-side invocation** — at `handle_one_stream`, immediately after `http_to_envoy_request` (the 05.2 D3 adapter) translates the H2 stream's HEADERS frame into an `envoy_http1::codec::Request`:

```rust
// Today (HEAD 7337f2c):
let mut envoy_req = http_to_envoy_request(h2_req)?;
let outcome = build_response(&envoy_req, ...);

// After Task 7:
let mut envoy_req = http_to_envoy_request(h2_req)?;
let mut pipeline = (*config.filter_pipeline).clone();
let decode_decision = pipeline.decode_headers(&mut envoy_req);
let outcome = match decode_decision {
    Decision::Continue => H2RequestPath::Match(build_response(&envoy_req, ...)),
    Decision::StopAndSend(resp) => H2RequestPath::SynthFromDecode(resp),
};
```

(Parallel to the H1 `RequestPath` enum, the H2 site has its own `H2RequestPath` — separate types because the H2 writer paths feed different structural shapes downstream. The planner at 07.1 SPEC writeup time may unify these into a single `RequestPath` if the shape allows.)

**Encode-side invocation** — `finalize_h2_stream` refactor. Today (HEAD `7337f2c`), `finalize_h2_stream` at `crates/envoy-http2/src/hcm.rs` line ~378 is:

```rust
async fn finalize_h2_stream(
    send_response: h2::server::SendResponse<...>,
    resp: envoy_http1::codec::Response,
    hcm_stats: &HCMStats,
    cluster_stats_opt: Option<&Cluster>,
    access_log_dispatch: AccessLogDispatch,
    start: Instant,
    ...
) -> Result<(), Http2Error> {
    // [existing 06.3 per-class HCM counter increment site at lines 380-391]
    hcm_stats.downstream_rq_total.inc();
    match resp.status / 100 {
        2 => hcm_stats.downstream_rq_2xx.inc(),
        // ...
    }
    // Existing wire-write:
    send_envoy_response(send_response, resp).await?;
    // Existing 06.2 access-log dispatch.
    // ...
}
```

After Task 7:

```rust
async fn finalize_h2_stream(
    send_response: h2::server::SendResponse<...>,
    pipeline: &mut FilterPipeline,
    mut resp: envoy_http1::codec::Response,
    hcm_stats: &HCMStats,
    cluster_stats_opt: Option<&Cluster>,
    access_log_dispatch: AccessLogDispatch,
    start: Instant,
    ...
) -> Result<(), Http2Error> {
    // NEW: encode-side invocation.
    if let Decision::StopAndSend(replacement) = pipeline.encode_headers(&mut resp) {
        resp = replacement;
    }
    // Existing per-class counter site (now consumes post-encode resp.status):
    hcm_stats.downstream_rq_total.inc();
    match resp.status / 100 {
        2 => hcm_stats.downstream_rq_2xx.inc(),
        // ...
    }
    // Existing wire-write:
    send_envoy_response(send_response, resp).await?;
    // Existing access-log dispatch (now consumes post-encode resp.headers).
}
```

The `pipeline: &mut FilterPipeline` parameter is threaded from `handle_one_stream` per-stream scope. The per-class counter and access-log dispatch both move to consuming the post-encode response state (matching the H1 site's semantic).

**Tests** (unit + integration; in `crates/envoy-http2/src/hcm.rs` `#[cfg(test)] mod tests`):

1. **`decode_headers` fires before route-match (H2).** Parallel to H1 Task 6 test 3; deferred to 07.2 per the same rationale.
2. **`encode_headers` fires before `send_envoy_response` (H2).** Parallel to H1 Task 6 test 4; deferred to 07.2.
3. **`StopAndSend` at decode side (H2).** Parallel to H1 Task 6 test 5; deferred to 07.2.
4. **`StopAndSend` at encode side (H2).** Parallel to H1 Task 6 test 6; deferred to 07.2.
5. **Regression-equivalence on existing H2 fixtures (0009 + 0010).** Both fixtures' in-process backstops + Docker-gated fixtures continue green at Task 7's commit.

**Workspace test bucket** — `cargo test --workspace` must stay green at Task 7's commit.

**Code budget:** ~50 LoC at finalize_h2_stream refactor + ~30 LoC at handle_one_stream decode-side invocation + ~40 LoC StopAndSend early-return wiring + ~30 LoC unit tests (test 5 only at 07.1 scope) + ~20 LoC H2RequestPath enum = ~170 LoC.

**Dependencies:** Tasks 1-6 all required (framework types + validator + H1 wiring serve as the precedent shape for H2).

**Commit message:** `phase 07.1: task 7 — H2 HCM finalize_h2_stream refactor + filter-chain invocation`.

### Task 8 — 12-fixture regression-equivalence verification (D6.1)

**Files modified:**
- `docs/envoy-rust/phases/07.1-filter-framework-foundation/PROGRESS.md` — Task 8 entry.

**Action.** Push the branch to GitHub at HEAD = Task 7's commit (or a Task-7-equivalent state); trigger the Docker-gated CI workflow; capture the run URL + run ID + conclusion + completion timestamp; verify all 12 Docker-gated fixtures (`0001-tcp-echo` through `0012-access-log-file-sink`) are green simultaneously under the SAME CI run + the `h2spec` conformance suite passes at ≥95% with `known-failures.txt` unchanged + the `parse_bootstrap` fuzz target runs clean.

**Evidence quoting** — Task 8's PROGRESS entry quotes the CI run URL + run ID + conclusion + completion timestamp inline. Mirrors phase-06.3 Task 12 (commit `7cdc1a8`) and phase-06.1 Task 14 (commit `a5f795c`) precedents.

**Per-task PROGRESS test-bucket attestation** (per parent SPEC §8 R-1 + 06.3 REVIEW I1 carryforward): the PROGRESS entry enumerates which test buckets ran and their results:

```
- workspace tests: cargo test --workspace — PASS (count: <N>; commit at <SHA>)
- Docker-gated fixtures (12 total, 0001-0012): all green simultaneously per CI run <URL>
- h2spec conformance: <pass_rate>% (≥95% gate held; known-failures.txt unchanged)
- parse_bootstrap fuzz: clean (short-budget CI run at <duration>)
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean
- cargo fmt --all -- --check: clean
- cargo deny check: clean
```

**No code changes in Task 8** — only PROGRESS.md updates + the CI run trigger.

**Dependencies:** Task 7 (the H2 HCM wiring is the last code-changing task; Task 8 verifies the cumulative behavior).

**Commit message:** `phase 07.1: task 8 — state-4 verification (12 fixtures simultaneously green)`.

### Task 9 — Final state-4 PROGRESS materialization + state-machine advance (D7.1)

**Files modified:**
- `docs/envoy-rust/phases/07.1-filter-framework-foundation/PROGRESS.md` — final Task 9 entry plus a state-4-reached / state-5-next subsection.
- `docs/envoy-rust/STATE.md` — advance from `07.1` lifecycle state 3 (per parent-state-2 advance) to `07.1` lifecycle state 4 reached / state 5 next; next-skill `superpowers:requesting-code-review`.

**Action.** Materialize the §7.5 phase-done gate evidence captured at Task 8 into a clean PROGRESS sub-section that the state-5 reviewer reads. STATE.md advances active phase from `07.1 state 3` → `07.1 state 4-reached / state-5-next`.

This is the bracketed state-4-reached / state-5-next STATE advance commit per the parent-06 cadence (commit `42fc726` for 06.3). The next session enters 07.1 lifecycle state 5 (review) and runs `superpowers:requesting-code-review` per the state-machine.

**Code budget:** ~30 LoC PROGRESS + ~20 LoC STATE.md = ~50 LoC docs only.

**Dependencies:** Task 8 (the CI evidence anchor must be captured before STATE advance).

**Commit message:** `phase 07.1: task 9 — advance STATE.md to state-4-reached / state-5-next`.

### Code budget summary

| Task | Code LoC | Test LoC | Doc LoC | Total |
|---|---|---|---|---|
| 1 (envoy-filter scaffold + FilterError) | ~75 | ~25 | — | ~100 |
| 2 (FilterPipeline + Decision + iter) | ~70 | ~50 | — | ~120 |
| 3 (HttpFilterInstance + RouterTerminus) | ~90 | ~20 | — | ~110 |
| 4 (envoy-config validator relaxation) | ~150 | ~70 | — | ~220 |
| 5 (H1 HCM 5-writer-arm refactor) | ~130 | ~80 | — | ~210 |
| 6 (H1 HCM filter invocation wiring) | ~70 | ~30 | — | ~100 |
| 7 (H2 HCM finalize_h2_stream + invocation) | ~140 | ~30 | — | ~170 |
| 8 (12-fixture regression verification) | — | — | ~30 | ~30 |
| 9 (state-4 PROGRESS + STATE advance) | — | — | ~50 | ~50 |
| **Total** | **~725** | **~305** | **~80** | **~1110** |

Against parent SPEC §3 D1.1-D7.1 projection of ~900 LoC, the per-task decomposition projects ~1110 LoC (~+23% over the parent SPEC projection). This is within the parent-04.3 / parent-05.3 ~+20% drift envelope; if execution-time experience shows further inflation, the planner at 07.1 state-2 PLAN writeup may split Task 5 (the writer-arm refactor) into 5a + 5b along the proxy-success vs synth-arms boundary. Task count stays at 9 against parent SPEC's ~9 projection.

---

## 4. Out of scope (deferred non-goals for 07.1)

The following are explicitly deferred from sub-phase 07.1 per parent-07 SPEC §4 (the parent's out-of-scope list binds on each sub-phase). Items already covered by parent SPEC §4 are restated only where they intersect 07.1's surface.

- **Body-iteration states (`decode_data` / `encode_data`).** Defer per parent SPEC §4. The MVP iteration covers `decode_headers` + `encode_headers` only; H1 + H2 are fully buffered.
- **Trailer-iteration states (`decode_trailers` / `encode_trailers`).** Defer per parent SPEC §4.
- **Per-route `typed_per_filter_config`.** **Defer entirely from 07.1 per parent SPEC §4 recommended posture.** Adding the schema field without a consuming filter is premature scaffolding; the per-route override lands when the first filter that needs it lands (post-07.2).
- **HeaderMutation filter implementation.** Defer to 07.2.
- **Any non-Router filter.** Defer to 07.2 (HeaderMutation) or later phases.
- **`StopAndSend` actively used by an MVP filter.** The `Decision::StopAndSend` variant is scaffolded at Task 2 for forward-compat; no 07.x filter emits it. The framework's StopAndSend handling at H1 + H2 is unit-tested via test-only filter stubs (gated `#[cfg(test)]`) per Task 6 + Task 7's deferred tests 3-7 (those tests live with the first non-Router filter at 07.2).
- **Stats-emitting filters.** Defer. Phase 07 introduces no filter-emitted stats.
- **Filter-state machinery (`%FILTER_STATE%` access-log token).** Defer indefinitely per parent SPEC §4.
- **Dynamic metadata (`%DYNAMIC_METADATA%` access-log token).** Defer indefinitely.
- **Async-aware iteration shape.** Defer. Synchronous iteration per parent SPEC §6 Rule 5. Conditional ADR-0031 (foundations grant for `async_trait` or similar) is NOT projected at 07.1 — recommended posture is no foundations grants. Lands only if execution-time experience surfaces it as essential per D-3.5.
- **Phase-04.1 REVIEW M5/M9** (Cargo.lock cadence ratification ADR). Carries forward unchanged; 07.1 introduces no new top-level Cargo deps under recommended posture.
- **Phase-06.3 REVIEW I1** (Task 11 fixup verification-discipline gap). The 07.1 PROGRESS test-bucket attestation discipline at every code-changing task (Tasks 5, 6, 7) is the structural close — each commit's PROGRESS enumerates which test buckets ran with attestation.
- **Phase-06.3 REVIEW I2** (synthetic 5xx backend + 4-class `pre_requests`). 07.1 does not introduce a synthetic backend; carries forward to upstream-robustness family.
- **Phase-06.2 REVIEW M1 / M2 / M4 / M5** + **06.1 REVIEW I2 / M1 / M4** + **05.3 REVIEW I2** + **05.2 REVIEW I1 / I2 / I3** + **02.2 REVIEW M1** — all out of scope for 07.1; carry forward unchanged.

---

## 5. Implementation signposts for the planner

These are guidance notes the 07.1 state-2 PLAN.md writer should consult. Each signpost is a non-obvious shape decision codified in advance.

**Signpost 1 — Module decomposition is mandatory at Task 1.** The crate scaffold lands with `lib.rs` + `error.rs` only at Task 1; `pipeline.rs` and `instance.rs` + `router.rs` land at Tasks 2 + 3 respectively. Do NOT bundle all modules into Task 1 — the per-task scope discipline keeps each task at ~100-120 LoC.

**Signpost 2 — `Decision::StopAndSend` variant is scaffolded from day one (Task 2).** Even though no 07.1 filter emits it (Router is a no-op terminus), the variant lands at Task 2 so the framework's iteration loops have the structural shape for 07.2's HeaderMutation (and any future filter that needs short-circuit). Forward-compat scaffolding only — does not violate "Don't design for hypothetical future requirements" because the 07.2 surface lands in the immediately-following sub-phase and is concretely projected.

**Signpost 3 — Pipeline mutability + clone semantics.** `FilterPipeline::decode_headers` / `encode_headers` take `&mut self` because future filter types may carry per-request state. At 07.1 with Router-only, the per-request clone is effectively a no-op (Router is zero-state); but the clone shape is established Task 2 so 07.2's HeaderMutation per-stream cloning is structural rather than retrofitted. **HCMConfig holds `Arc<FilterPipeline>`** (Arc-shared at config time); each per-request scope at H1 (`serve_connection` per-request) and H2 (`handle_one_stream` per-stream) clones into a working `FilterPipeline` by dereferencing the Arc and calling `.clone()` on the inner. Mirrors the access-log `Vec<Arc<FileSink>>` shape from 06.2 — Arc-shared at config, owned at request.

**Signpost 4 — `outgoing` local at the H1 unified site.** The Task 5 refactor introduces `let mut outgoing: Http1Response;` declaration at the scope above the writer-arm match. Each arm assigns to `outgoing`. After the match, the unified site runs `pipeline.encode_headers(&mut outgoing)`. The `outgoing` shape is `Http1Response` because the proxy-success arm's `construct_proxied_response` returns this type (Task 5 factoring point).

**Signpost 5 — `let outgoing` declaration discipline.** Per the 06.2 + 06.3 declaration discipline (`let mut x;` form, not `let mut x = 0/Default::default()`), the `outgoing` local is uninitialized until the arm-match assigns it. This catches accidental fall-through (the compiler errors if any arm doesn't assign). Mirrors the H2 site's `let resp: envoy_http1::codec::Response;` discipline.

**Signpost 6 — H2 `finalize_h2_stream` parameter threading.** The new `pipeline: &mut FilterPipeline` parameter on `finalize_h2_stream` propagates from `handle_one_stream` per-stream scope. This is a function-signature change visible to any caller of `finalize_h2_stream`. At HEAD `7337f2c`, the function has callers only inside `crates/envoy-http2/src/hcm.rs` (the 3 H2 writer paths each call it). All 3 call sites update at Task 7.

**Signpost 7 — Cross-crate dep direction (per parent SPEC §6 Rule 10).** `envoy-filter` depends on `envoy-config` (for `HttpFilter` config struct) + `envoy-http1` (for `codec::Request` / `codec::Response` value-types — NOT HCM types). `envoy-http1` and `envoy-http2` both depend on `envoy-filter` (for the framework runtime). This creates a stack `envoy-config → envoy-filter → envoy-http1, envoy-http2 → envoy-bin`. **No cycles** — `envoy-filter` does NOT depend back on `envoy-http1` for the HCM types because the `codec` module has no dependency on the `hcm` module within `envoy-http1`. If a future surface forces a wider dependency (e.g., access-log integration at the filter level), the recommended path is to move `codec` into a smaller `envoy-codec` crate — flagged for whichever future phase first surfaces the pressure (out of scope for 07.1).

**Signpost 8 — Validator `listener_name` parameter.** `validate_http_filters(filters: &[HttpFilter], listener_name: &str)` requires a listener-name string for the error variants' `listener: String` field. The caller at `validate_hcm` already knows the listener name (it walks `listeners` at the level above); the planner threads the name through at the call site. Mirrors the existing `Http2ClusterFromHttp1Listener { listener: String, cluster: String }` variant from 06.3's listener-name-threading at the parse-time validator.

**Signpost 9 — Task-5-as-pure-refactor + Task-6-as-wiring split.** Task 5 lands the 5-writer-arm refactor ONLY (no filter invocation; `outgoing` is constructed, populated, written — but `encode_headers` is not yet invoked). Task 6 lands the filter-invocation wiring ON TOP of Task 5's refactor. This split is structurally important: Task 5's commit is verifiable by the in-process backstop tests + workspace tests + Docker-gated fixtures 0001-0012 (the regression-equivalence proof of the refactor alone); Task 6 then layers filter invocation onto a known-good base. Mirrors the 06.3 Task 4 + Task 5 split (per-class HCM counter writer-arm-extension + the access-log per-arm wiring; landed in separate commits).

**Signpost 10 — `RequestPath` / `H2RequestPath` private enums.** The local enum that captures `Match` (writer-arm path) vs `SynthFromDecode` (decode-side StopAndSend short-circuit) is private to each HCM module. The two are parallel but separate types because the writer-arm return shapes differ (H1 returns an `Http1Response` directly; H2 returns through `finalize_h2_stream` which threads upstream state). The planner at 07.1 state-2 PLAN writeup time decides whether to unify them at the framework level (then `envoy-filter` would export `pub enum RequestPath` for shared use) or leave them per-HCM. **Recommended posture: per-HCM** — the shapes diverge enough that unification adds abstraction without payoff at the 07.1 surface.

**Signpost 11 — No new fuzz target in 07.1.** Phase 07.1 introduces no new parser; the existing `parse_bootstrap` fuzz target continues to exercise the relaxed validator (the corpus extension for HeaderMutation lands in 07.2 Task 4). 07.1's state-4 fuzz-gate is satisfied by the existing corpus.

**Signpost 12 — Test-only filter stub gating.** The unit tests at Tasks 6 + 7 that need a non-Router filter to instrument decode-side / encode-side mutation are deferred to 07.2 Task 5 (the first task that wires HeaderMutation). This is a structural deferral per the test-coverage rationale at parent-07 SPEC §6 Rule 2 (envoy-filter exports primitives; consumers test invocation). 07.1's regression-equivalence proof at state-4 IS the no-behavior-regression test under the Router-only chain; the per-filter invocation semantics get exercised at 07.2 by HeaderMutation's bilateral fixture.

**Signpost 13 — Existing `MultipleHttpFilters` variant retention.** Task 4 retains the variant (no longer constructed) per the ledger-discipline of ConfigError as a grow-only typed-error structure. Renaming or removing landed variants is a breaking change to the typed-error API that consumers (envoy-bin, integration tests) may depend on. Doc-comment notes the supersession; existing tests that asserted the variant are updated to assert one of the new variants (the supersession is by-replacement, not by-removal).

**Signpost 14 — H1 `serve_connection` request-loop body shape.** Today the per-request loop body is roughly `parse_request → build_response → match writer-arm → write_wire → access_log`. After Task 6 it becomes `parse_request → clone_pipeline → decode_headers (→ short-circuit on StopAndSend) → build_response → match writer-arm → encode_headers → write_wire → access_log`. The clone-per-request shape is at the parse_request frontier; the early-return on StopAndSend short-circuits to the unified site directly.

---

## 6. Phase-done evidence shape

The state-4 phase-done gate evidence at Task 8 follows the parent-06.3 Task 12 (commit `7cdc1a8`) shape:

- **CI run URL** quoted inline (e.g., `https://github.com/<org>/<repo>/actions/runs/<run_id>`).
- **HEAD SHA** at the run target quoted inline.
- **Conclusion** (`success` required) quoted inline.
- **Completion timestamp** (ISO-8601 UTC) quoted inline.
- **Test buckets** enumerated with pass/fail status:
  - Workspace tests: `cargo test --workspace` — PASS (count: N)
  - Docker-gated fixtures: 12 total, all green simultaneously
  - h2spec conformance: pass rate ≥95%
  - parse_bootstrap fuzz: clean
  - cargo clippy, cargo fmt --check, cargo deny check: all clean

---

## 7. ADRs expected from this sub-phase

**No ADRs are pre-projected to land in 07.1** under the recommended posture per parent-07 SPEC §7. ADR-0030 (parent-07 split decision) landed at the parent-07 state-2 commit (this SPEC's landing commit). Conditional ADR-0031 (foundations grant for `async_trait` or similar) stays available — lands only if execution-time experience at any 07.1 task surfaces an async iteration need (e.g., a writer-arm refactor blocked by a Future-returning hook). The framework's synchronous non-async iteration shape per parent-07 SPEC §6 Rule 5 is designed to avoid `async_trait`; the synchronous shape works on `Vec<(String, String)>` headers + `Bytes` bodies (both fully buffered before the framework runs).

If an unanticipated ADR-shaped decision surfaces at execution time (e.g., the writer-arm refactor at Task 5 surfaces a non-obvious shape choice for the proxy-success arm's `construct_proxied_response` factoring; the H2 `finalize_h2_stream` parameter threading exposes a deeper refactor pressure on `envoy-http2`), the in-execution ADR lands inline at that task's commit per D-3.5, with a one-line note in the standalone PLAN.md flagging the ADR landing.

**DECISIONS.md ledger head at 07.1 entrance:** **ADR-0030** (landed at the parent-07 state-2 commit immediately before 07.1's first task). 07.1's ledger-head projection at state-6 close is **ADR-0030** (unchanged) under recommended posture.

---

## 8. State-machine signposts for 07.1's own state-2 session

07.1's state-2 session (the session immediately following parent-07's state-2 commit) operates per `SKILL_ROUTING.md` line 21 / `BOOTSTRAP_PROMPT.md` §5 state 2: *"SPEC.md exists, PLAN.md does not → superpowers:writing-plans → output: PLAN.md"*. Per the 04.3 / 05.1 / 05.4 / 06.1 / 06.2 / 06.3 standardized cadence (each sub-phase's standalone pre-Task-1 PLAN.md commit), 07.1's state-2 session lands:

1. **`docs/envoy-rust/phases/07.1-filter-framework-foundation/PLAN.md`** — the standalone PLAN.md, decomposing this SPEC's 9 tasks into per-step TDD checklists (test-first, then implementation, then verification, then commit). Mirrors 06.1's `505653d` (4126 lines, 14 tasks), 06.2's inline-`d65f04e` (sized similarly), and 06.3's `3a964cc` (sized similarly).
2. **STATE.md** advanced from `07.1` state 2 → `07.1` state 3 (PLAN.md exists, implementation begins next session); next-skill `superpowers:subagent-driven-development` per the user's standing preference (auto-memory `feedback_execution_style` — do not present the inline-`executing-plans` fork at state-3 entry).

The standalone PLAN.md commit is the ONLY artifact of 07.1's state-2 session per the "one state per session" doctrine (`BOOTSTRAP_PROMPT.md` §5.1). No code changes; no ADRs.

**Standalone PLAN.md commit message** (per 06.x precedent):

```
phase 07.1: state-2 standalone PLAN.md (9 tasks; ~1110 LoC projected)

<1-3 sentence summary covering the 9-task decomposition and the recommended
execution order; per-task LoC budget; references to parent-07 SPEC §3 + this
sub-phase SPEC §3.>
```

After 07.1 state-2 lands, the next session enters 07.1 state 3 — runs `superpowers:subagent-driven-development` scoped to 07.1's surface, dispatches Task 1 to the first subagent, executes per the standard cadence.

---

## 9. Final commit message format (for state 6 of the 07.1 lifecycle)

The 07.1 state-6 close-out commit uses the standard format from `BOOTSTRAP_PROMPT.md` §5.3:

```
phase 07.1: envoy-filter foundation + HCM filter-chain wiring + terminal-router validator

<1-3 sentence summary covering the framework crate, the HCM integration at H1+H2,
the H1 5-writer-arm refactor, the H2 finalize_h2_stream refactor, and the
terminal-router validator. Mention that no new fixture lands and that the
state-4 gate verified all 12 existing fixtures green simultaneously.>

Differential surface: fixtures 0001-tcp-echo through 0012-access-log-file-sink all green at the Docker-gated CI level simultaneously; no new fixture in this sub-phase.
Conformance: h2spec ≥95% pass (carried forward from 05.2 baseline; phase 07.1 engages no H2-framing surfaces).
```

If no ADRs land during 07.1 execution under the recommended posture, the title's bracketed `[ADR-NNNN]` tag is omitted. If conditional ADR-0031 lands, the title includes `[ADR-0031]`.

The 07.1 state-6 close-out does NOT flip parent ROADMAP row `07` to `done` — that happens at 07.2's state-6 commit per the ROADMAP-schema invariant (closing-sub-phase rule). 07.1's state-6 advances STATE.md to point at `07.2` lifecycle state 1 (next-skill `superpowers:brainstorming` scoped to 07.2's surface). Mirrors the 04.1 → 04.2 / 04.2 → 04.3 / 05.1 → 05.4 / 05.4 → 05.2 / 05.2 → 05.3 / 06.1 → 06.2 / 06.2 → 06.3 inter-sub-phase advance pattern.

---

## 10. State-machine commit (the parent-07 state-2 split commit; this SPEC's landing commit)

This SPEC lands at the parent-07 state-2 split commit alongside:

- **ADR-0030** (parent-07 split decision; appended to `docs/envoy-rust/DECISIONS.md` per D-3.5).
- **Sibling sub-phase SPEC** at `docs/envoy-rust/phases/07.2-header-mutation-filter/SPEC.md`.
- **ROADMAP.md** — new rows for `07.1` + `07.2` with `status: planned`; parent row `07`'s `sub-phases` column updates from `—` to `07.1, 07.2`; row `07`'s `status` stays `in-progress`.
- **STATE.md** advanced to point at `07.1` lifecycle state 1 (next-skill `superpowers:brainstorming` scoped to 07.1's surface).

No code changes. No fixture changes. Doc-only commit.

Mirrors phase-05 state-2 commit `f1804a7` shape and phase-04 state-2 commit `1d9740d` shape. Per `BOOTSTRAP_PROMPT.md` §5.1 ("one state per session; do not chain states"), the parent-07 state-2 commit is the ONLY artifact this session lands; the next session (07.1 state 1) reads the advanced STATE.md and runs `superpowers:brainstorming` scoped to 07.1's surface.
