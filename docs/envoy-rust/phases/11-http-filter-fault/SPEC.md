# Phase 11 (`11-http-filter-fault`) — SPEC

- **Phase id:** `11`
- **Slug:** `11-http-filter-fault`
- **Status before this SPEC lands:** _not yet in ROADMAP.md_ (per `docs/envoy-rust/ROADMAP.md` at HEAD `e24053e`, the phase-10 state-6 close-out commit; the "HTTP filters family" §9 heading exists with two concrete rows beneath it — phase 09 `local_ratelimit` `status: done` + phase 10 `rbac` `status: done`). **This SPEC's landing commit is the third concrete row added beneath the HTTP-filter-family heading**, with `status: planned`.
- **Charter source:** `BOOTSTRAP_PROMPT.md` §9 — *"HTTP filters family — header manipulation, cors, compression, **fault**, local+global rate limit, jwt_authn, rbac, ext_authz, ext_proc, oauth2, csrf, buffer, lua, wasm, adaptive concurrency, admission control, bandwidth limit."* This phase lands `envoy.filters.http.fault` narrowed to the **abort path** (operator-configured HTTP-status abort, optionally gated by request-header matchers, with deterministic 0%/100% percentage). Fractional-percentage abort, request/response **delay**, response-rate-limit, gRPC abort status, downstream-controlled faults, and `max_active_faults` all defer per §4 below.
- **Position in the project:** the **third post-MVP-trunk feature-family phase** and the **third concrete HTTP-filter-family phase** (after phase-09 `local_ratelimit` and phase-10 `rbac`). The MVP trunk 00→08 stands `done` as of commit `304ce98`; phase 09 stands `done` as of commit `518140c`; phase 10 stands `done` as of commit `e24053e`. Phase 11 amortizes the framework + helper investment of phases 07/09/10 (the `Decision::StopAndSend(FilterResponse)` production-path discipline; the ADR-0033 H1 HCM `decorate_filter_synth_response` helper; the 04.2 `HeaderMatcher` reuse; the per-filter `StatsRegistry` counter-wiring pattern) **and is the first HTTP-filter-family phase to exercise a short-circuiting filter on an HTTP/2 listener** — which is what makes it the named close site for the **09 REVIEW M2** H2 HCM filter-synth header-decoration gap.
- **depends-on:** `07` (the parent filter-chain framework). Phase 11 extends the 07.1-landed `envoy-filter::FilterPipeline` + `HttpFilterInstance` enum with a fifth production variant (after `Router` at 07.1, `HeaderMutation` at 07.2, `LocalRateLimit` at 09, `Rbac` at 10). **An implicit dependency on phase `05` (the HTTP/2 codec + `Driver::Http2` harness primitive) is load-bearing for the first time in the HTTP-filter family** — phase 11's differential fixture runs on an H2 listener. Implicit dependencies on 04.2 (`HeaderMatcher` reuse) and on 09 (the H1 HCM `decorate_filter_synth_response` helper landed via ADR-0033 Commit C, used as the structural template for the new H2 analogue) are not in the depends-on field per ROADMAP schema conventions (the schema captures only direct ROADMAP-row dependencies; cross-deliverable reuse is implicit). The 17-Docker-gated-fixture regression baseline established at phase-10 close (`0001-tcp-echo` through `0017-http-filter-rbac`) carries forward unchanged per `BOOTSTRAP_PROMPT.md` §7.5 (b).
- **Brainstorm narrative:** see the "Phase-11 state-1 brainstorm" subsection of `docs/envoy-rust/STATE.md` for the family-pick + filter-pick rationale with alternatives considered along the 5-dimension scoring framework.

---

## 1. Goal and acceptance signal

Phase 11 lands the `envoy.filters.http.fault` filter (per upstream Envoy v1.33's documented filter name; typed_config `@type = type.googleapis.com/envoy.extensions.filters.http.fault.v3.HTTPFault`) as the **fourth concrete pluggable HTTP filter** in the 07.x-established framework (after HeaderMutation at 07.2, LocalRateLimit at 09, and Rbac at 10). The filter, when configured with an `abort` block, optionally gates on a set of request-header matchers; on a gated request that the percentage selects, the filter short-circuits via `Decision::StopAndSend(FilterResponse)` with the operator-configured HTTP status + the abort body, decorated with the standard response headers via the HCM filter-synth decoration helpers.

The phase **closes the most-named open carryforward, 09 REVIEW M2**, at its named-owner site:

- **09 REVIEW M2** (the H2 HCM filter-synth header-decoration gap). Phase 10 closed M2's *documentation* arm (ADR-0033 Consequences amendment per close shape (a)) but explicitly deferred the *implementation* arm: per the amendment at `docs/envoy-rust/DECISIONS.md:699`, the H2 writer path at `crates/envoy-http2/src/hcm.rs` returns the filter response verbatim through `build_http_response` at `crates/envoy-http2/src/response.rs:29-50`, which does NOT add `server`/`date`/`content-type`. The amendment named the close site verbatim: *"next HTTP-filter-family phase exercising filters bilaterally on H2 (the H2 HCM `decorate_filter_synth_response_h2` analogue lands as a ~50-70 LoC + 2-test follow-up at that phase)."* **Phase 11 IS that phase.** D6 lands the `decorate_filter_synth_response_h2` analogue and wires it into both H2 synth writer-arm sites; fixture 0018 runs the fault filter on an H2 listener, exercising the H2 filter-synth path bilaterally and proving the decoration end-to-end.

**Differential surface added by phase 11:**

- **Fixture `0018-http-filter-fault`** — bilateral assertion that both proxies, given an identical bootstrap with a fault filter configured as `abort: { http_status: 503, percentage: { numerator: 100, denominator: HUNDRED } }` gated by a single request-header matcher (`x-fault: abort`), produce the deterministic per-probe status sequence on a 4-probe burst over an **HTTP/2** listener: probe 1 (`x-fault: abort`) → 503 (gate matches, 100% → abort); probe 2 (no `x-fault` header) → 200 (gate does not match → pass-through to `direct_response`); probe 3 (`x-fault: abort`) → 503; probe 4 (no header) → 200. Asserts each 503 response carries the standard HTTP/2 response headers (`server`, `date`, `content-length`, `content-type`; **NOT** `connection` — that name is an H2-forbidden hop-by-hop header stripped by `build_http_response`) via the new `decorate_filter_synth_response_h2` helper, plus the abort body byte-exact. The exact body bytes + header set + stat namespace are **empirically verified at state-2 PLAN-write per §6.2** (the phase-10-ratified verify-at-PLAN-write process improvement).

**Acceptance signal (a)–(f), per `BOOTSTRAP_PROMPT.md` §7.5:**

- **(a)** Fixture `0018-http-filter-fault` green at Docker-gated CI.
- **(b)** All **17 pre-existing differential fixtures** (`0001-tcp-echo` through `0017-http-filter-rbac`) **remain green simultaneously** at the same CI run (regression-equivalence per `BOOTSTRAP_PROMPT.md` §7.5 (b)).
- **(c)** `h2spec` continues at ≥95% (parent-05 baseline 99.31%). **Phase 11 touches the H2 HCM filter-synth writer path** (the new `decorate_filter_synth_response_h2` decoration) — the state-4 verification re-runs `h2spec` and confirms the decoration introduces no framing regression (the decoration adds response *headers*, not frame-structure changes; the helper runs before `build_http_response` translates to the `http::Response<()>` head).
- **(d)** `parse_bootstrap` fuzz target clean for the short-budget CI run on the extended corpus (one new seed for the fault bootstrap shape; corpus extends from 17 to 18 seeds).
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check` all clean.
- **(f)** `REVIEW.md` approved.

A **single CI run** must light up gates (a) through (e) **simultaneously** (continues the project precedent established at 06.1 / 07.x / 08.x / 09 / 10 — fixture inheritance is a regression vector).

---

## 2. Behavior-contract scope for phase 11

Phase 11 extends `docs/envoy-rust/BEHAVIOR_CONTRACT.md` with authored additions, landed at the tasks where each is first empirically exercised (per the established 06.x / 07.x / 08.x / 09 / 10 doctrine — contract extensions land at empirical-engagement task time, NOT at PLAN-write time and NOT at state-1 SPEC time).

### 2.1 "Stat-name mapping" extension — 1 new row (projected; §6.2-verified)

One new counter row under the fault filter's stat namespace, mirroring upstream Envoy v1.33's documented stat tree. Upstream's `envoy.filters.http.fault` emits `<prefix>.fault.aborts_injected`, `<prefix>.fault.delays_injected`, `<prefix>.fault.response_rl_injected`, `<prefix>.fault.active_faults` (gauge), `<prefix>.fault.faults_overflow`. At phase-11 abort-only scope, only the abort counter is wired (the delay/response-rate-limit/active-faults/overflow stats defer alongside their features per §4):

| Stat name | Equivalence | Rationale |
|---|---|---|
| `http.<hcm_stat_prefix>.fault.aborts_injected` | value-exact | Counter; one increment per request the filter aborts (gate matches AND percentage selects). Both proxies emit one increment per aborted request, synchronously with the abort `Decision::StopAndSend` emission. |

**Namespace empirical-verification signpost:** the `http.<hcm_stat_prefix>.fault.aborts_injected` namespace shape is the recommended state-1 projection per the 06.1 stats convention + the upstream `http.<hcm_stat_prefix>.<filter>.*` pattern confirmed at phase 10. **The state-2 PLAN-writer empirically verifies the exact namespace against `envoyproxy/envoy:v1.33.0` + admin `/stats` scrape** before locking the namespace shape (per §6.2). If reality differs (e.g. upstream uses a top-level `<configured_prefix>.fault.*` rooted at a filter-level stat prefix rather than the HCM prefix), the SPEC §2.1 + D7 revision lands via an inline ADR at PLAN-write time per D-3.5. Note: upstream fault exposes no `stat_prefix` field on the v3 `HTTPFault` message; the prefix derives from the parent HCM's `stat_prefix` (same as RBAC at phase 10).

The differential fixture 0018 does **not** scrape fault stats (it asserts only the 4-probe HTTP status sequence + the 503 body + the decorated header set); the single counter is exercised by the in-process backstop (D8.3) + unit tests (D4). This mirrors phase 10's posture (the RBAC fixture did not scrape RBAC stats either).

### 2.2 "Header allow-list" extension — none required

The abort response body shape + header decoration are determined by the HCM filter-synth decoration helpers:

- `FaultFilter::decode_headers` emits the abort response with `body: Bytes::from_static(<abort body bytes>)` per upstream Envoy v1.33's source-hardcoded abort body. **The exact body bytes are §6.2-verified at state-2 PLAN-write** (the recommended state-1 projection is `"fault filter abort"` — but the phase-10 experience (the RBAC body was off by 1 byte) mandates empirical verification before locking; if the projection is wrong, an inline ADR lands at PLAN-write per the phase-10 ADR-0034 precedent).
- The standard response headers are decorated onto every filter-synth abort response by the HCM helpers: on **H1**, by the existing `decorate_filter_synth_response` helper (`crates/envoy-http1/src/hcm.rs:968`; adds `content-length` always + `server`/`date`/`content-type`/`connection` only-if-missing); on **H2**, by the **new** `decorate_filter_synth_response_h2` helper (D6; adds `content-length` always + `server`/`date`/`content-type` only-if-missing; **no `connection`** — that name is an H2-forbidden hop-by-hop header stripped by `build_http_response` per RFC 7540 §8.1.2.2).

**No new Header allow-list row is needed.** The 04.1-landed `server` row + `date` row of `docs/envoy-rust/BEHAVIOR_CONTRACT.md`'s Header allow-list cover the cross-proxy implementation-identifying differences. The remaining standard headers (`content-length`, `content-type`) are value-exact across proxies under the deterministic fixture-0018 burst (static abort body on abort; static `direct_response` body on pass-through). The `:status` H2 pseudo-header is value-exact (503 / 200). **The exact decorated header set on H2 abort responses is §6.2-verified** — the recommended projection is `{server, date, content-length, content-type}` (4 headers, no `connection`); if upstream H2 emits a different set, the SPEC §2.2 + D6 + D8.1 revision lands via inline ADR at PLAN-write.

### 2.3 No DECISIONS.md amendment required at SPEC time

Phase 10's D5 already landed the 09 REVIEW M2 *documentation* close (the ADR-0033 Consequences amendment at `docs/envoy-rust/DECISIONS.md:699` naming this phase as the implementation close site). Phase 11's D6 lands the *implementation* close (the H2 decoration helper). The phase-11 PROGRESS narrative attributes the M2 implementation close to the D6-landing task commit and cross-references the phase-10 D5 amendment + the phase-09 PROGRESS Commit C forward-reference. **No new ADR is required to close M2** — the carryforward's named close shape is "implementation of the H2 analogue," which is ordinary deliverable work, not a decision. (If state-3 surfaces an unexpected wire-shape divergence for the H2 abort response, an inline ADR lands per §6.2 / §7.)

---

## 3. Deliverables

Phase 11's scope is enumerated as deliverables `D1`–`D8` below. **The state-2 PLAN-writer organizes deliverables into tasks** (and evaluates the §6.1 split gate) — these are not 1:1 with tasks. The deliverables are LISTED in roughly the order the PLAN-writer is expected to execute them, but the SPEC is not prescriptive about the order; only about the surface.

### D1 — `envoy-config` schema extension

At `crates/envoy-config/src/bootstrap.rs`, extend the existing `HttpFilterTypedConfig` enum (currently `Router`, `HeaderMutation`, `LocalRateLimit`, `Rbac` variants per 07.2 + 09 + 10) with a fifth variant `Fault(FaultConfig)` (typed_config `@type = type.googleapis.com/envoy.extensions.filters.http.fault.v3.HTTPFault`). The config struct shape mirrors upstream Envoy v1.33's `envoy.extensions.filters.http.fault.v3.HTTPFault`, narrowed to the minimum-viable abort surface for phase 11:

```rust
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FaultConfig {
    pub abort: FaultAbort,                          // REQUIRED at phase 11
    #[serde(default)]
    pub headers: Vec<HeaderMatcher>,                // OPTIONAL gate; reuses 04.2 HeaderMatcher
    // OPTIONAL — all defer per §4:
    //   delay: FaultDelay
    //   response_rate_limit: FaultRateLimit
    //   max_active_faults: u32
    //   abort_grpc_status / downstream-controlled fault headers
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FaultAbort {
    pub http_status: u16,                           // REQUIRED at phase 11 (1xx-5xx)
    pub percentage: FractionalPercent,              // REQUIRED at phase 11
    // grpc_status, header_abort defer per §4
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FractionalPercent {
    pub numerator: u32,
    #[serde(default = "default_denominator")]
    pub denominator: DenominatorType,               // default HUNDRED
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DenominatorType {
    Hundred,                                         // "HUNDRED"  → 100
    TenThousand,                                     // "TEN_THOUSAND" → 10_000
    Million,                                         // "MILLION" → 1_000_000
}
```

All struct shapes carry `#[serde(deny_unknown_fields)]` per the established envoy-config discipline (rejects forward-looking fields envoy-rust does not yet support — e.g. `delay`, `response_rate_limit`, `grpc_status`, `header_abort`, `max_active_faults`). The `HeaderMatcher` reuses the existing 04.2-landed type directly (no schema duplication; same reuse as RBAC's `Permission::Header`/`Principal::Header` at phase 10). `FractionalPercent` + `DenominatorType` are **new** envoy-config types (no percent type exists in envoy-config yet, confirmed at SPEC time) and are authored to be reusable by future filters that take a `FractionalPercent` (local/global ratelimit `filter_enabled`/`filter_enforced`, RBAC shadow rules, etc.).

The phase-11-deferred upstream-Envoy fields are each enumerated in §4 below; each is rejected by `deny_unknown_fields`.

### D2 — `envoy-config` validator extension

At `crates/envoy-config/src/bootstrap.rs::validate_http_filters`, extend the existing per-variant validator dispatch with a `Fault` arm calling a new `validate_fault_config(cfg) -> Result<(), ConfigError>` sub-validator. The validator checks:

- `abort.http_status` is a syntactically valid HTTP status code (100–599; `ConfigError::InvalidFaultAbortStatus { status }`). Mirrors the `http::StatusCode::from_u16` acceptance band; the production runtime additionally relies on the codec's status validation.
- `abort.percentage.numerator <= denominator_value(abort.percentage.denominator)` (a numerator exceeding the denominator is operator error; `ConfigError::FaultPercentageOutOfRange { numerator, denominator }`).
- **`abort.percentage` is deterministic at phase-11 scope: `numerator` must be either `0` OR equal to `denominator_value(denominator)` (i.e. 0% or 100%).** Fractional percentages (`0 < numerator < denominator`) are rejected with `ConfigError::UnsupportedFractionalFaultPercentage { numerator, denominator }` per the §4 deferral. **Rationale (load-bearing — see §5.6):** a fractional per-request abort decision is non-differential-testable (the per-request "abort vs pass" outcome depends on a random draw that cannot be matched across two independent proxies — the differential contract §7.2 does not compare probabilistic outcomes); deterministic 0%/100% IS differential-testable. Deferring fractional keeps phase 11 entirely inside the differential contract and avoids landing untested (un-differential-verifiable) code per `BOOTSTRAP_PROMPT.md` §6.3.
- Each entry of `headers` (if present) is a structurally valid `HeaderMatcher` (delegates to the existing 04.2 `HeaderMatcher` parse-time validation; no new check beyond the deserialize).

Two-to-three new `ConfigError` variants land at this site (`InvalidFaultAbortStatus`, `FaultPercentageOutOfRange`, `UnsupportedFractionalFaultPercentage` — possibly consolidated if the PLAN-writer prefers). Each carries `listener: String` per the established envoy-config error-context discipline. Each has its own unit test cases for positive + negative parse paths. The validator is exercised by the existing fuzz target `parse_bootstrap` (the new fixture's bootstrap is seeded into the corpus per D8.2).

The `envoy.filters.http.fault` filter name is **currently in the unsupported-filter reject list** at `crates/envoy-filter/src/error.rs:51` (it produces an `UnsupportedHttpFilter` error today). D1 + D2 + D3 + D5 collectively move it from rejected to supported; the PLAN-writer removes/updates the `error.rs:51` reject entry + its associated test (`error.rs:57`) at the appropriate task.

### D3 — `FractionalPercent` deterministic evaluation helper

A small pure-compute helper (likely on the `FractionalPercent` type in envoy-config, or a thin wrapper in `envoy-filter`) that, at phase-11 deterministic scope, answers "does this percentage select?" — returning `true` iff `numerator == denominator_value(denominator)` (100%), `false` iff `numerator == 0` (0%). Because the validator (D2) rejects fractional percentages, the runtime evaluation is a pure boolean with **no per-request randomness and no PRNG** at phase-11 scope.

**Explicit non-deliverable (signpost):** the hand-rolled per-request PRNG that a *fractional* percentage would require is **NOT** landed at phase 11 (it defers with fractional percentage per §4). This is a deliberate scope cut to (a) keep the phase inside the differential contract per §6.3 and (b) keep the phase under the §6.1 split gate. The future fault-enrichment phase that lands fractional percentage hand-rolls the PRNG per D-3.2 (no `rand` foundations grant; written-from-scratch per the token-bucket / tree-walk-evaluator precedent) and opts into statistical-distribution unit testing per §7.2.

### D4 — `envoy-filter::FaultFilter` runtime

New module `crates/envoy-filter/src/fault.rs`. Hand-rolled per **D-3.2**'s *"Every individual filter ... Must be written from scratch"* doctrine + the 07.2 `header_mutation.rs` + 09 `local_rate_limit.rs` + 10 `rbac.rs` precedent (one module per concrete filter). Module shape:

```rust
#![forbid(unsafe_code)]   // inherited from crate root

use std::sync::Arc;
use bytes::Bytes;
use envoy_stats::{Counter, StatsRegistry};
use crate::error::FilterError;
use crate::pipeline::Decision;
use crate::types::{FilterRequest, FilterResponse};

/// The `envoy.filters.http.fault` runtime filter (abort path).
#[derive(Debug, Clone)]
pub struct FaultFilter {
    abort_status: u16,
    abort_selects: bool,                            // true iff percentage == 100% (D3)
    header_gate: Vec<envoy_config::HeaderMatcher>,  // empty ⇒ always gated-in
    aborts_injected: Arc<Counter>,
}

impl FaultFilter {
    /// Lower an `envoy_config::FaultConfig` into the runtime filter +
    /// register the abort counter against the StatsRegistry.
    pub(crate) fn build_from_config(
        cfg: &envoy_config::FaultConfig,
        registry: &Arc<StatsRegistry>,
        hcm_stat_prefix: &str,
    ) -> Result<Self, FilterError> { /* ... */ }

    pub(crate) fn decode_headers(&mut self, req: &mut FilterRequest) -> Decision {
        // Gate: header_gate empty ⇒ matches; else ALL matchers must match.
        // Select: abort_selects (100%) ⇒ abort; 0% ⇒ pass.
        // On abort: inc aborts_injected; return Decision::StopAndSend(
        //   FilterResponse { status: self.abort_status, reason: None,
        //   headers: vec![], body: Bytes::from_static(<abort body §6.2>) }).
        // Else: Decision::Continue.
        unimplemented!()
    }

    pub(crate) fn encode_headers(&mut self, _resp: &mut FilterResponse) -> Decision {
        // Decode-only filter at phase-11 scope (response-rate-limit defers).
        Decision::Continue
    }
}

fn header_gate_matches(gate: &[envoy_config::HeaderMatcher], req: &FilterRequest) -> bool {
    gate.iter().all(|m| m.matches(&req.headers))   // all-must-match per upstream
}
```

**Gate semantic (signpost):** upstream Envoy's `HTTPFault.headers` is a repeated `HeaderMatcher`; a fault is injected only for requests matching **all** the listed matchers (AND semantics). An empty/absent `headers` list means the fault applies to all requests (no gate). This matches `header_gate_matches` above (`Iterator::all` over an empty slice returns `true`).

**HeaderMatcher reuse (signpost):** the existing 04.2-landed `envoy_config::HeaderMatcher::matches(&[(String, String)]) -> bool` (verified at phase 10 PLAN-write: the method takes `&[(String, String)]`, NOT `&[Header]`; `FilterRequest::headers: Vec<(String, String)>` matches directly — see phase-10 PLAN §1 SPEC correction #1). No matcher logic is duplicated.

**Async vs sync signature (signpost):** `decode_headers` is synchronous per the 07.1 framework (the abort decision is pure-compute; no I/O). The deferred *delay* fault would be the first filter requiring an async-sleep lift — explicitly out of scope per §4.

### D5 — `HttpFilterInstance::Fault` variant + dispatch

Extend `crates/envoy-filter/src/instance.rs::HttpFilterInstance` enum with a new variant `Fault(FaultFilter)`. Extend the `build` dispatch (the new variant calls `FaultFilter::build_from_config(cfg, registry, hcm_stat_prefix)`) + `decode_headers` + `encode_headers` dispatch arms. New variant lands between `Rbac` and the `#[cfg(feature = "test-util")]` block (mirroring the 09 + 10 variant-placement precedent). Re-export `FaultFilter` from `crates/envoy-filter/src/lib.rs::pub use fault::FaultFilter;`.

**HCM `stat_prefix` threading (signpost):** the fault stats namespace `http.<hcm_stat_prefix>.fault.aborts_injected` requires the HCM's `stat_prefix` threaded into `FaultFilter::build_from_config`. Phase 10 already widened `FilterPipeline::build_from_config` → `HttpFilterInstance::build` to thread `(filters, &registry, hcm_stat_prefix: &str)` (phase-10 PLAN lock-in #15 / SPEC correction #5). **Phase 11 reuses that 3-arg shape unchanged — no further signature widening is needed** (the third arg already exists from phase 10). The `Fault` build arm passes `hcm_stat_prefix` straight through.

### D6 — H2 HCM `decorate_filter_synth_response_h2` helper (closes 09 REVIEW M2 implementation arm)

This is the phase's **carryforward-closure centerpiece**. Add a new helper symmetric to the H1 `decorate_filter_synth_response` (`crates/envoy-http1/src/hcm.rs:968`), in the envoy-http2 crate (recommended site: `crates/envoy-http2/src/hcm.rs`, or a small free function in `crates/envoy-http2/src/response.rs` adjacent to `build_http_response`). The helper operates on the `envoy_http1::Response` (the filter-synth response struct shared across codecs) **before** it is handed to `build_http_response`:

```rust
/// Decorate a filter-synth H2 response with the standard response headers
/// (symmetric to H1's `decorate_filter_synth_response`, minus `connection`
/// which is an H2-forbidden hop-by-hop header stripped by build_http_response).
fn decorate_filter_synth_response_h2(resp: &mut envoy_http1::Response) {
    // content-length: always derived from body.len(); overwrite if present.
    // server / date / content-type: add only-if-missing.
    // NO connection header (H2-forbidden per RFC 7540 §8.1.2.2).
}
```

Wire it into **both** H2 synth writer-arm sites:

- **Decode-side** `H2RequestPath::SynthFromDecode(r)` at `crates/envoy-http2/src/hcm.rs:373` — the arm reached when a decode-side filter returns `Decision::StopAndSend(filter_resp)` (constructed at `hcm.rs:176-177`). The fault filter's abort takes this path.
- **Encode-side** `Decision::StopAndSend(replacement)` at `crates/envoy-http2/src/hcm.rs:436` — the arm reached when an encode-side filter substitutes the response. (No phase-11 filter takes this path — fault abort is decode-side — but the helper is wired symmetrically per the H1 precedent, which decorates at both `hcm.rs:598` and `hcm.rs:636`, so future encode-side-short-circuiting H2 filters inherit the decoration for free.)

**Header-set parity signpost:** the H1 helper adds 4 standard names (`server`, `date`, `content-type`, `connection`) only-if-missing + `content-length` always. The H2 helper adds 3 (`server`, `date`, `content-type`) only-if-missing + `content-length` always — **dropping `connection`** because `build_http_response` (`response.rs:36-40`) strips H2-forbidden hop-by-hop headers including `connection`. The PLAN-writer **§6.2-verifies the exact header set upstream Envoy v1.33 emits on an H2 abort response** (recommended projection: `{server, date, content-length, content-type}`) before locking D6.

**~50-70 LoC + 2 tests** per the 09 REVIEW M2 named estimate (the phase-10 D5 amendment quoted this number). 2 unit tests mirror the H1 helper's tests (`crates/envoy-http1/src/hcm.rs:1405-1452`): one asserting all standard headers are added when absent; one asserting existing headers are preserved (only-if-missing) + `content-length` is overwritten from `body.len()` + `connection` is NOT added.

### D7 — Stats wiring (1 counter per upstream-Envoy parity) + BEHAVIOR_CONTRACT extension

At `FaultFilter::build_from_config`, register one `Counter` handle against the `Arc<StatsRegistry>`:

- `format!("http.{hcm_stat_prefix}.fault.aborts_injected")` → `aborts_injected: Arc<Counter>` (§6.2-verified namespace).

Increment site (within `decode_headers`): `self.aborts_injected.inc()` on the abort decision, before constructing the `Decision::StopAndSend(FilterResponse)`. The 06.x stats convention applies: `StatsRegistry::register_counter` is idempotent for same-name re-registration.

**D7.1 — `Stat-name mapping` 1 row** (§2.1) lands at the D7 stats-wiring task commit (where the counter is first registered + incremented in tests), per the 06.x / 07.x / 08.x / 09 / 10 cadence. No new Header allow-list row needed per §2.2.

### D8 — Fixture + harness extension + fuzz seed + in-process backstop

- **D8.1 — Fixture `tests/fixtures/0018-http-filter-fault/`.** Runs the fault filter on an **HTTP/2** listener (the deliberate inverse of the 09 + 10 H1-only fixtures — phase 11 is the H2-exercising phase per §1). Bootstrap shape mirrors fixture 0009's H2 HCM + `direct_response` data-plane surface (`codec_type: HTTP2`) so the bilateral assertion focuses on the filter + the H2 decoration, not on upstream proxy complexity. Bootstrap sketch:

  ```yaml
  static_resources:
    listeners:
    - name: ingress_http2
      address: { socket_address: { address: 0.0.0.0, port_value: 10000 } }
      filter_chains:
      - filters:
        - name: envoy.filters.network.http_connection_manager
          typed_config:
            "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
            stat_prefix: ingress_http
            codec_type: HTTP2
            http_filters:
            - name: envoy.filters.http.fault
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.http.fault.v3.HTTPFault
                abort:
                  http_status: 503
                  percentage: { numerator: 100, denominator: HUNDRED }
                headers:
                - name: x-fault
                  string_match: { exact: abort }
            - name: envoy.filters.http.router
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
            route_config:
              virtual_hosts:
              - name: default
                domains: ["*"]
                routes:
                - match: { prefix: "/" }
                  direct_response: { status: 200, body: { inline_string: "ok\n" } }
  ```

  Probe shape: a 4-probe burst over H2. Expected per-probe statuses: `[503, 200, 503, 200]` corresponding to (`x-fault: abort` / no header / `x-fault: abort` / no header). Asserts probes 1 and 3 (503 responses) carry the standard H2 response headers (`server`, `date`, `content-length`, `content-type`; no `connection`) via `decorate_filter_synth_response_h2` (set-equal-modulo-allow-list per the 04.1-landed `server` + `date` rows) + the abort body (§6.2-verified bytes) byte-exact.

  **Harness signpost (PLAN-write decision):** the existing `Driver::Http2` (`tests/differential/src/lib.rs:113`) drives an H2 exchange; the existing `Driver::Http1ProbeList` (`lib.rs:81`) drives a multi-probe burst with per-probe request headers + per-probe expected status/headers/body. Phase 11 needs a **multi-probe H2** driver. **Recommended: add `Driver::Http2ProbeList`** mirroring `Http1ProbeList`'s shape (per-probe `request_headers` + `expected_status` + `expected_headers` allow-list + `expected_body`) but driving over the H2 client path — a modest, well-bounded harness extension (~60-100 LoC, reusing the `Http2` driver's H2-client machinery + the `Http1Probe` per-probe assertion structure). The state-2 PLAN-writer confirms the exact `Driver::Http2` internals at PLAN-write and decides between (a) `Driver::Http2ProbeList` (recommended — gives both abort + pass-through arms bilaterally in one fixture, mirroring the 10 `[403,200,403,200]` pattern) or (b) a single-probe `Driver::Http2` abort assertion + the pass-through arm covered by the in-process backstop only (smaller, but weaker bilateral coverage). Recommended: (a).

  Docker-gated wrapper at `tests/differential/tests/http_filter_fault.rs` mirroring `tests/differential/tests/http_filter_rbac.rs` shape (the 10 precedent). One `#[tokio::test]` `http_filter_fault_fixture` invoking `run_fixture("0018-http-filter-fault").await`.

- **D8.2 — Fuzz corpus seed.** New file `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_fault_filter.yaml` containing the bootstrap shape above (or a minimal variant). Mirrors the 07.2 `hcm_header_mutation_filter.yaml` + 09 `hcm_local_rate_limit_filter.yaml` + 10 `hcm_rbac_filter.yaml` precedent. Extends the fuzz target's seed coverage from 17 to 18 entries (one per fixture's bootstrap shape). Includes the `crates/envoy-config/fuzz/.gitignore` allow-list extension AND the `crates/envoy-config/src/bootstrap.rs::tests::fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS-array extension (per the 09 + 10 Task 6 follow-up precedent — both files edited together).

- **D8.3 — In-process backstop.** New file `crates/envoy-bin/tests/http_filter_fault.rs` mirroring `crates/envoy-bin/tests/http_filter_rbac.rs` (10 precedent) — **with the 09 REVIEW M3 subprocess discipline already baked in** (`tokio::process::Command` + `.kill_on_drop(true)` + stdout `Stdio::null()` + stderr `Stdio::piped()`; M3 was CLOSED at phase 10's Task 7 `dd95673` and the discipline is now the standing pattern). The state-3 implementer reads the phase-10 `http_filter_rbac.rs:180-186` backstop as the precedent shape via direct code-spot-check before writing (per the standing precedent-verification discipline note).

  Single `#[tokio::test]` exercising fault abort semantics in-process (no Docker). **Recommended: exercise the H1 path in-process** (the in-process server boots an H1 listener; the H1 `decorate_filter_synth_response` helper already exists, so this gives cheap H1-codec coverage of the abort semantics to complement the H2 differential fixture — both codecs covered across the two test tiers). The test boots `envoy-bin` with a synthesized bootstrap (fault abort 100%, header-gated `x-fault: abort`); issues 4 sequential `GET /` requests with varying `x-fault` header values; asserts the status sequence `[503, 200, 503, 200]` + the abort body (§6.2 bytes) on 503 probes + body `"ok\n"` on 200 probes + presence of the standard HTTP/1.1 headers on 503 probes. (Per the 10 M1 carryforward lesson, the PLAN-writer should EITHER include the per-probe standard-header presence assertion on the 503 probes OR disclose its omission explicitly in PROGRESS — see §6.4.)

---

## 4. Out of scope (deferred non-goals)

Phase 11 explicitly does NOT land:

- **Fractional-percentage abort (`0 < numerator < denominator`).** Phase 11 supports deterministic 0%/100% only; the validator (D2) rejects fractional with `ConfigError::UnsupportedFractionalFaultPercentage`. **Rationale:** a per-request probabilistic abort decision is non-differential-testable (the differential contract §7.2 does not compare probabilistic per-request outcomes across two independent proxies). Fractional defers to a future fault-enrichment phase that hand-rolls a PRNG per D-3.2 + opts into statistical-distribution unit testing per §7.2 (the differential fixture would still pin a deterministic percentage). This is the natural next fault increment.
- **Request/response delay fault (`HTTPFault.delay`).** Delay's observable effect is added latency; per §7.2 the differential contract does NOT compare timing by default. Delay also requires the first async-sleep lift in the filter decode path (a framework extension). Defers to whichever future phase opts into latency bounds per §7.2 + lands the async-delay framework primitive.
- **Response rate-limit fault (`HTTPFault.response_rate_limit`).** Requires a body-streaming/throttling framework extension (larger than the filter). Defers.
- **gRPC abort status (`FaultAbort.grpc_status`).** Requires gRPC-response engagement at the filter layer; defers to the gRPC family or a later fault-enrichment phase.
- **Downstream-controlled faults (`x-envoy-fault-abort-request`, `x-envoy-fault-delay-request`, `header_abort`, `header_delay`).** Phase 11 uses operator-configured abort + `headers`-match GATING only (distinct surface from downstream-header-CONTROLLED fault injection). Downstream-controlled faults defer.
- **`max_active_faults` + `active_faults` gauge + `faults_overflow` counter.** Concurrency-limit accounting requires per-stream active-fault tracking; not exercised by the deterministic fixture. Defers with the concurrency surface.
- **Runtime-keyed percentage/enablement overrides.** Upstream fault supports `runtime` keys (e.g. `fault.http.abort.abort_percent`) consulted from the RTDS runtime layer. The runtime layer is unimplemented (Runtime + hot restart family per §9). Defers.
- **Per-route `typed_per_filter_config` for fault.** The filter's config is sourced exclusively from the filter-chain-level entry. Per-route fault variation defers to whichever future HTTP-filter-family phase first needs per-route config (CORS is the natural close site per upstream Envoy's per-route pattern — same family-wide deferral as phases 09 + 10).
- **Custom abort body / headers.** Phase 11's abort body is upstream-Envoy v1.33's source-hardcoded abort body (§6.2-verified). Operator-configurable abort bodies are not an upstream fault feature; no deferral needed.
- **H1 differential fixture for fault.** Phase 11's differential fixture is H2 (the H2-exercising phase per §1). The H1 abort path is covered by the in-process backstop (D8.3) + unit tests (D4); the H1 `decorate_filter_synth_response` helper is already validated bilaterally by fixtures 0016 (09) + 0017 (10). A standalone H1 fault differential fixture is not needed (the filter is codec-agnostic; the H1 decoration path is already proven).

---

## 5. Architectural invariants

Phase 11 honors and extends the established cross-crate invariants:

### 5.1 Crate boundaries

- **`envoy-filter` stays sole-dep-owner of HTTP filter-chain iteration.** All new variant + filter implementation land in `crates/envoy-filter/`. No new top-level crate; no new workspace member.
- **`FaultFilter` lives at `crates/envoy-filter/src/fault.rs`.** Mirrors the 07.2 `header_mutation.rs` + 09 `local_rate_limit.rs` + 10 `rbac.rs` placement pattern (one module per concrete filter).
- **`FractionalPercent` + `DenominatorType` live in `crates/envoy-config/`** (alongside the other shared config types). Authored to be reusable by future filters (local/global ratelimit `filter_enabled`/`filter_enforced`, RBAC shadow rules) — this is the only genuinely *new shared* schema type phase 11 introduces.
- **The new H2 decoration helper lives in `crates/envoy-http2/`** (the crate that owns the H2 HCM writer path). No new path-dep is introduced — `envoy-http2` already depends on `envoy-http1` (for the shared `Response` type) + `envoy-filter`.
- **No new path-deps within `envoy-filter`.** `envoy-stats` + `envoy-config` path-deps already exist (phases 06.1 + 09 + earlier). Phase 11 adds zero new workspace path-deps; the 04.1 REVIEW M5/M9 Cargo.lock cadence carries forward unchanged.

### 5.2 Hand-rolled filter per D-3.2

The fault filter is hand-rolled per **D-3.2**'s *"Every individual filter ... Must be written from scratch"* doctrine. The implementation uses only **std-lib + bytes + envoy-config (HeaderMatcher + FractionalPercent) + envoy-stats** — all D-3.2-permitted; all already pulled. Pure-compute gate + select; no I/O; no async.

**Explicit non-grants:** no `rand` (the fractional-percentage PRNG defers per §4 + §5.6; when it lands, it is hand-rolled per D-3.2, not granted), no `ipnet`/`cidr`, no `regex` beyond what ADR-0021 already provides. None on D-3.2's permitted-foundations list beyond what is already pulled; none required by the phase-11 scope. The state-3 implementer must NOT pull any new top-level crate (any pull forces a foundations-grant ADR per D-3.5).

### 5.3 No new top-level Cargo deps

The recommended no-foundations-grants posture per phases 09 + 10 + parent-08 + parent-07 SPEC §7 carries forward through phase 11. **If the state-3 implementer surfaces a genuine foundation need at execution time, a foundations-grant ADR lands per D-3.5 — see §7 for the conditional-ADR slots.**

### 5.4 Decode-side abort filter (encode no-op at phase-11 scope)

`FaultFilter::encode_headers` is a no-op (`Decision::Continue`) at phase-11 scope — the abort path is decode-side. (Upstream fault's response-rate-limit IS encode-side, but defers per §4.) The encode-side method exists on the `HttpFilterInstance` dispatch arm per the 07.x framework symmetry.

### 5.5 Filter-chain config only (NOT per-route)

The sole config source is the filter-chain-level entry per the 07.2 + 09 + 10 precedent. Per-route `typed_per_filter_config` defers per §4. Whichever future HTTP-filter-family phase first needs per-route variation extends the 07.x framework with a per-route lookup primitive — the new primitive is the gating architectural change, NOT the filter that consumes it. Same posture as phases 09 + 10.

### 5.6 Fault decision semantic (cross-proxy deterministic)

The fault decision is the cross-proxy semantic invariant. Per upstream Envoy v1.33, with `abort` configured:

- **Gate:** if `headers` is non-empty, the fault applies only to requests matching **all** the listed `HeaderMatcher`s (AND semantics); an empty/absent `headers` list applies the fault to all requests.
- **Select:** for a gated-in request, abort iff the `percentage` selects. At phase-11 deterministic scope, `percentage` is 0% (never select) or 100% (always select).
- **Abort:** on select, short-circuit with `Decision::StopAndSend(FilterResponse { status: abort_status, .. })` + increment `aborts_injected`. Otherwise `Decision::Continue` (the request proceeds to the next filter / route).

**Determinism across both proxies** holds because (a) the gate is a pure function of the request headers (04.2 `HeaderMatcher`, already cross-proxy deterministic via fixtures 0007/0017), and (b) the select is deterministic at 0%/100% (no random draw). Fractional percentage is **excluded** precisely because it would break (b) — the per-request random draw differs across two independent proxies, making the abort/pass outcome non-comparable under the differential contract §7.2. This is why D2 rejects fractional (see §4 rationale).

The `aborts_injected` counter is incremented EXACTLY ONCE per aborted request (one increment per `StopAndSend` emission; never on pass-through). No double-counting.

### 5.7 Statelessness across requests

The `FaultFilter` carries no per-request state at phase-11 scope (no `max_active_faults` accounting — that defers per §4). The gate matchers + abort status + select boolean are immutable post-`build_from_config`. Per `Clone` of `HttpFilterInstance`, each per-request pipeline-clone shares the `Arc<Counter>` handle + clones the (small) gate `Vec` + scalar fields — negligible clone cost.

### 5.8 H1 + H2 symmetric (filter-layer codec-agnostic; H2 writer-path decoration NEW)

The filter operates on the codec-agnostic `FilterRequest` / `FilterResponse` abstraction per the 07.1 framework + ADR-0031. Both H1 and H2 HCM dispatch sites invoke `pipeline.decode_headers` at the established 07.x integration seam — no per-codec branching in the filter. **The codec asymmetry phase 11 closes is in the HCM *writer* path, not the filter:** the H1 writer path decorates filter-synth responses (since 09 ADR-0033 Commit C); the H2 writer path did NOT (the 09 REVIEW M2 gap). D6 lands the H2 `decorate_filter_synth_response_h2` analogue so both codecs decorate filter-synth responses symmetrically. **After phase 11, the H1 + H2 HCM filter-synth writer paths are at parity** — closing the 09 REVIEW M2 implementation arm.

### 5.9 HCM filter-synth decoration-helper reuse (H1) + parity (H2)

Phase 11's H1 abort emission (in-process backstop + any H1 deployment) flows through the existing `decorate_filter_synth_response` helper (09 ADR-0033 Commit C `ae2cef0`) unchanged — pure reuse. Phase 11's H2 abort emission (the differential fixture) flows through the **new** `decorate_filter_synth_response_h2` helper (D6). The fault filter emits `FilterResponse { status: abort_status, reason: None, headers: vec![], body: Bytes::from_static(<§6.2 bytes>) }`; the per-codec HCM writer arm decorates the standard headers before write. This is the **first filter to exercise the H2 filter-synth writer path bilaterally** — validating the H2 decoration helper end-to-end (the same way phase 10's RBAC was the first non-LocalRateLimit consumer of the H1 helper).

---

## 6. Implementation signposts for the planner

The state-2 PLAN-writer reads this section to drive PLAN structure.

### 6.1 Split-gate evaluation (read first)

Per `BOOTSTRAP_PROMPT.md` §6.1, the state-2 PLAN-write evaluates whether the PLAN exceeds ~25 numbered tasks OR ~1500 LoC. Phase 11's surface estimate at SPEC time:

- D1 — envoy-config schema (FaultConfig + FaultAbort + FractionalPercent + DenominatorType) (~150 LoC + ~140 LoC unit tests). ~1 task.
- D2 — envoy-config validator (~90 LoC + ~120 LoC unit tests). ~1 task or co-located with D1.
- D3 — FractionalPercent deterministic eval helper (~20 LoC + ~30 LoC tests). Co-located with D1 or D4.
- D4 — FaultFilter runtime + gate/select + abort (~190 LoC + ~180 LoC unit tests). ~1 task.
- D5 — HttpFilterInstance::Fault variant + dispatch + error.rs reject-list removal (~50 LoC + ~30 LoC tests). ~1 task.
- D6 — H2 `decorate_filter_synth_response_h2` helper + 2 writer-arm wirings (~65 LoC + ~80 LoC tests). ~1 task. **(closes 09 REVIEW M2 impl)**
- D7 — stats wiring (1 counter) + BEHAVIOR_CONTRACT 1 row (~40 LoC + ~40 LoC tests). Co-located with D4.
- D8.1 — fixture 0018 (H2) + Docker-gated wrapper + `Driver::Http2ProbeList` harness extension (~120 LoC YAML + ~60 LoC wrapper + ~90 LoC harness). ~1-2 tasks.
- D8.2 — fuzz corpus seed (~30 LoC YAML + 2 file edits). ~1 task.
- D8.3 — in-process backstop (~190 LoC). ~1 task.
- State-4 verification + STATE-advance (~docs). ~1 task.

**SPEC-time projection: ~10-12 tasks; ~1350-1500 LoC** (production ~620, tests ~700, fixture/harness/doc ~280). The phase is at the **upper-tractable band but at-or-under** the §6.1 ~1500-LoC gate; task count comfortably under ~25. **Recommended posture: single-phase (no split).** State-2 PLAN-write lands a standalone `PLAN.md` per the 04.3 / 05.1 / 06.x / 07.x / 08.x / 09 / 10 cadence.

**Release valve (if state-3 drifts over the gate):** the `Driver::Http2ProbeList` harness extension (D8.1) is the single largest discretionary LoC item. If the PLAN materializes clearly over ~1600 LoC, the PLAN-writer's preferred trim is to use option (b) in D8.1 (single-probe `Driver::Http2` abort assertion + pass-through covered by the backstop) rather than the new `Http2ProbeList` driver — saving ~90 LoC while still closing 09 M2 (the H2 abort path is still exercised bilaterally; only the pass-through arm loses H2 differential coverage). This is the recommended trim before any nest-split. Per parent-08 SPEC §6.1 alternative (vi), in-execution drift is recorded in PROGRESS, NOT nest-split.

### 6.2 Empirical verification at state-2 PLAN-write (process-improvement ratified at phase 10)

Per the phase-10 SPEC §6.2 process-improvement (ratified in the phase-10 ADR ledger as the standard for novel filter surfaces with empirically-discoverable wire-level projections) + the ADR-0033 process-gap-awareness doctrine: **the state-2 PLAN-writer empirically verifies the upstream wire shapes BEFORE locking PLAN lock-ins.** Phase 11's empirical-verification scope (run `envoyproxy/envoy:v1.33.0` Docker against the §3 D8.1 canonical bootstrap on an H2 listener; drive aborted + pass-through requests with an H2 client — e.g. `curl --http2-prior-knowledge -i` or `nghttp`):

1. **Stats namespace shape**: scrape `/stats` post-abort; record the exact fault stat names. Update SPEC §2.1 + D7 if the projection (`http.<hcm_stat_prefix>.fault.aborts_injected`) is wrong.
2. **Abort response body bytes**: observe the abort response body bytes on a gated-in 503; record the exact byte sequence (hex-dump). Update SPEC §2.2 + D4 + D8.1 if the projection (`"fault filter abort"`) is wrong. **The phase-10 experience (RBAC body off by 1 byte) makes this verification load-bearing — do NOT assume the projection.**
3. **Abort response header set (over H2)**: observe the exact response headers Envoy v1.33 emits on an H2 fault-abort response; record the set. Confirms the D6 `decorate_filter_synth_response_h2` target set (recommended projection `{server, date, content-length, content-type}` — no `connection`).

Each finding lands as a PLAN lock-in. **If any finding differs materially from the SPEC projection, the lock-in records the divergence + the SPEC §X.Y revision via an inline ADR at the state-2 PLAN-write commit** (mirrors the phase-10 ADR-0034 inline-at-PLAN-write precedent). The next-available ADR number is **ADR-0035** (ledger head is ADR-0034 at phase-10 close). **Recommended posture: empirically verify all 3 at PLAN-write; land any necessary correction via inline ADR-0035 at the state-2 commit.**

### 6.3 09 REVIEW M2 implementation close (D6) is the carryforward centerpiece

D6 closes the 09 REVIEW M2 *implementation* arm (the H2 decoration helper). Phase 10's D5 closed the *documentation* arm (the ADR-0033 Consequences amendment). The PLAN-writer reads the phase-10 D5 amendment (`docs/envoy-rust/DECISIONS.md:699`) + the phase-09 PROGRESS Commit C forward-reference + the H1 helper (`crates/envoy-http1/src/hcm.rs:968`) via direct code-spot-check before writing D6 (the standing precedent-verification discipline). The PROGRESS narrative for the D6 task attributes the M2 implementation close + cross-references the phase-10 D5 amendment. **After D6 lands, the 09 → 10 → 11 M2 chain ENDS.**

### 6.4 In-process backstop header-presence assertion (heeds the phase-10 M1 lesson)

Phase 10's REVIEW M1 flagged that the RBAC backstop omitted the per-probe standard-header presence assertion on the 403 probes without disclosing the omission as a deviation. Phase 11's D8.3 SHOULD either (a) include the per-probe standard-header presence assertion on the 503 probes, OR (b) explicitly disclose the omission in PROGRESS with the redundancy-with-Docker-wrapper rationale. **Recommended: (a)** — the small assertion cost closes the M1-style gap proactively for the fault backstop.

### 6.5 The 06.x stats convention

Per 06.x cadence: StatsRegistry registration at `FaultFilter::build_from_config` time; per-filter-instance ownership of the Counter handle; namespace `http.<hcm_stat_prefix>.fault.aborts_injected` (§6.2-verified) matches upstream Envoy v1.33 parity exactly (no project-internal label divergence).

### 6.6 The 07.x BEHAVIOR_CONTRACT extension cadence

Per the established 06.x / 07.x / 08.x / 09 / 10 doctrine: contract extensions land at the TASK where each is first empirically exercised, NOT at PLAN-write time and NOT at state-1 SPEC time. For phase 11: the 1 Stat-name mapping row lands at the D7 task commit.

### 6.7 Pre-state-4 fmt discipline (continues per 06.1 R-9)

Per-task PROGRESS sections quote `cargo fmt --all -- --check` at every PROGRESS-task close, NOT just at state-4. Carries forward from the 06.1 → … → 10 chain.

### 6.8 State-4 evidence-discipline (continues per 05.3 → … → 10 chain)

Per-gate quoted evidence in PROGRESS at the state-4 verification task: real CI run URL + HEAD SHA + completion timestamp + per-gate quoted output (all 5 stable-toolchain gates + each Docker-gated fixture + h2spec_pass_rate_gate + parse_bootstrap fuzz iteration count). **Phase 11 touches the H2 writer path — the state-4 verification explicitly re-confirms h2spec ≥95% (the decoration must not regress framing).**

### 6.9 Cargo.lock cadence

The phase-04.1 REVIEW M5/M9 (Cargo.lock cadence ratification ADR) carries forward unchanged through phase 11 — zero new top-level Cargo deps projected per §5.3. The `Cargo.lock` diff at the phase-11 reviewed range is expected to be empty (zero workspace-internal path-dep additions; all needed path-deps already wired).

### 6.10 PROGRESS.md skeleton + Task 1 preamble land alongside PLAN.md at state-2

Per the 06.2 / … / 10 cadence. State-2 PLAN-write lands `PLAN.md` + `PROGRESS.md` skeleton + Task 1 preamble in a single standalone pre-Task-1 commit.

### 6.11 Subagent-driven execution at state 3 (per `feedback_execution_style`)

The user's standing preference auto-memory `feedback_execution_style` ("default to subagent-driven-development; skip the two-option fork") applies at state 3. The state-2 PLAN-write organizes tasks for subagent-driven execution per the 06.x / … / 10 cadence (each task independent enough to dispatch in isolation; PROGRESS attestation per-task; in-phase recovery cadence if any task surfaces a code-quality-review-blocking finding). Subagents claiming "same pattern as previous phase" verify the precedent shape via direct code-spot-check before the claim lands in PROGRESS.

---

## 7. ADR projection

**Recommended posture: NO new ADRs land in phase 11.** The work fits inside the existing permitted-foundations set per §5.2 + §5.3 above. The DECISIONS.md ledger head stays at **ADR-0034** through phase 11's state-1 (this) commit; the next-available number is **ADR-0035**.

Conditional ADR slots stay reserved-available for state-2 / state-3 execution-time landing if reality forces them:

- **Conditional ADR-0035 (option A) — PLAN-write empirical-verification revision.** If the §6.2 empirical verification at state-2 PLAN-write reveals one or more of (fault stats namespace / abort body bytes / H2 abort header set) materially differs from this SPEC's projection, ADR-0035 lands at the state-2 PLAN-write commit per the phase-10 ADR-0034 inline-at-PLAN-write precedent. **Recommended posture: empirically verify all 3 at state-2 PLAN-write; if any differ, land ADR-0035 inline at the PLAN-write commit.** Given the phase-10 RBAC body was off by 1 byte, the abort-body verification is the most likely to fire.

- **Conditional ADR-0035 (option B) — `FractionalPercent` reusability / fractional-deferral durability.** If the state-2 PLAN-writer concludes the fractional-percentage deferral (per §4 + §5.6) warrants append-only durability NOW (rather than at whichever future phase lands fractional), ADR-0035 lands recording the deferral + the named close site + the new shared `FractionalPercent` type's contract. **Recommended posture: defer the ADR** — the deferral is doctrinally clear without an ADR (the §4 + §5.6 rationale is self-contained), per phases 09 + 10's identical per-route-deferral posture.

- **Conditional ADR-0035 (option C) — foundations grant.** No grant projected. If state-3 surfaces a materially-worse-than-foundation result, ADR-0035 lands at the surfacing task. **Recommended posture per §5.2: no grant.** Std-lib + existing workspace-internal deps suffice for the abort-only fault surface narrowed to phase-11 scope.

- **Conditional ADR-0035 (option D) — `Driver::Http2ProbeList` harness primitive durability.** If the PLAN-writer judges the new H2 multi-probe harness driver to be a load-bearing primitive warranting an ADR (it is the first multi-probe H2 differential driver), ADR-0035 lands recording it. **Recommended posture: no ADR** — harness extensions have landed without ADRs throughout the project (e.g. `Driver::Http1ProbeList` at 04.2, `Driver::AdminScrape` at 06.1/08.1); the `Http2ProbeList` extension follows the same no-ADR cadence.

At most ONE of options A/B/C/D can land at any single commit (per D-3.5 sequential ADR numbering); if multiple fire, the second becomes ADR-0036 etc. If none fire, the ledger stays at ADR-0034 through phase 11.

---

## 8. State-machine signposts for the phase-11 state-2 session

The next session (state 2) reads this section and acts.

- **Lifecycle state at session start:** State 2 (SPEC.md exists; PLAN.md does not).
- **Skill:** `superpowers:writing-plans` per `BOOTSTRAP_PROMPT.md` §5 state 2.
- **Output:** `docs/envoy-rust/phases/11-http-filter-fault/PLAN.md` + `PROGRESS.md` skeleton + Task 1 preamble (standalone pre-Task-1 commit per the 04.3 / 05.1 / 06.x / 07.x / 08.x / 09 / 10 PLAN-write cadence).
- **Empirical verification at state 2 (per §6.2):** Run `envoyproxy/envoy:v1.33.0` Docker against the §3 D8.1 canonical bootstrap on an H2 listener; verify (fault stats namespace / abort body bytes / H2 abort header set) before locking PLAN lock-ins. If any differs from the SPEC projection, land an inline ADR-0035 at the state-2 PLAN-write commit per the phase-10 ADR-0034 precedent.
- **Split-gate evaluation:** §6.1 above. **Recommended: single-phase (no split).** PLAN materializes ~10-12 tasks / ~1350-1500 LoC; at-or-under the §6.1 gate. Release valve (D8.1 harness trim) per §6.1 if state-3 drifts over.
- **09 REVIEW M2 implementation close (D6):** §6.3 above. The carryforward centerpiece. Read the phase-10 D5 amendment + the H1 helper via direct code-spot-check before writing D6.
- **In-process backstop header assertion (D8.3):** §6.4 above. **Recommended: include the per-probe standard-header presence assertion on the 503 probes** (heeds the phase-10 M1 lesson).
- **Fractional-percentage deferral:** §4 + §5.6 + §7 option B. **Recommended: defer (validator rejects fractional; no ADR).**
- **PLAN-time SPEC corrections:** the PLAN-writer reads this SPEC against HEAD `<state-1-commit-SHA>` and flags any drift (mechanical signature differences between the SPEC's projected types and the actual on-disk envoy-config/envoy-filter/envoy-http2 types — e.g. the exact `HeaderMatcher::matches` signature [already confirmed `&[(String,String)]` at phase-10 PLAN-write], the exact `FilterRequest`/`FilterResponse` field shapes, the exact `Decision` enum variants, the exact `H2RequestPath` arm names + line numbers, the exact `build_http_response` signature). Per the 06.2 → … → 10 precedent ("N PLAN-write SPEC corrections" pattern), corrections land in the PROGRESS Task 1 preamble.

---

## 9. Commit message format (for state 6 of the phase-11 lifecycle)

```
phase 11: envoy.filters.http.fault (abort) + fixture 0018 + 09 REVIEW M2 impl close [H2 HCM decorate_filter_synth_response_h2]

<1-3 sentence summary>

Differential surface: fixture 0018-http-filter-fault (H2); all 18 Docker-gated fixtures (0001-0018) green simultaneously at CI run <ID> HEAD <SHA>.
Conformance: h2spec ≥95% gate held at parent-05 baseline; H2 writer-path decoration introduces no framing regression.
```

If ADR(s) land, the bracketed list is appended to the title per `BOOTSTRAP_PROMPT.md` §5.3 (e.g. `... [ADR-0035]`). Per the recommended no-ADRs posture per §7, the bracketed list is omitted by default (the §6.2 abort-body verification is the most likely ADR-0035 trigger).

If phase 11 unexpectedly splits at state-2 into 11.1 + 11.2 (NOT recommended; see §6.1), the closing-sub-phase commit carries `[parent 11 done]` per the 07.2 / 08.2 closing-sub-phase precedent.

---

## 10. State-machine commit (this commit — phase-11 state-1 close-out)

This SPEC is the state-1 output. The state-1 close-out commit is **docs-only** and touches:

- **CREATE** `docs/envoy-rust/phases/11-http-filter-fault/SPEC.md` (this file).
- **MODIFY** `docs/envoy-rust/ROADMAP.md` — **adds a new row** beneath the existing "HTTP filters family" §9 heading, immediately after the existing phase-10 row. Row format per the schema: `| id | title | depends-on | status | sub-phases | summary |`. New row content:
  ```
  | 11 | envoy.filters.http.fault (abort) + fixture 0018 + 09 REVIEW M2 impl close | 07 | planned | — | fixture 0018-http-filter-fault green (H2 listener); envoy-filter gains FaultFilter (operator-configured HTTP-status abort + optional header-match gate reusing 04.2 HeaderMatcher + deterministic 0%/100% percentage + decode-side StopAndSend) + HttpFilterInstance::Fault variant; envoy-config gains FaultConfig + FaultAbort + FractionalPercent + DenominatorType schema + ~3 new ConfigError variants (fractional percentage rejected per deterministic-only scope); envoy-http2 gains decorate_filter_synth_response_h2 helper closing the 09 REVIEW M2 H2 HCM decoration gap (implementation arm; first HTTP-filter exercised on H2) |
  ```
  The "HTTP filters family" heading itself stays unchanged; the new row joins beneath the existing phase-10 row per `BOOTSTRAP_PROMPT.md` §4.1 invariant 2 (append-only history; never delete rows). All other ROADMAP rows untouched.
- **MODIFY** `docs/envoy-rust/STATE.md` — advances "Active phase" pointer from `_none_ — awaiting next planning` to:
  - `id: 11`
  - `slug: 11-http-filter-fault`
  - `directory: docs/envoy-rust/phases/11-http-filter-fault/`
  - `status: phase 11 lifecycle state 1-complete / state-2-next (SPEC.md landed; PLAN.md does not exist)`

  Rewrites "Next expected skill" to `superpowers:writing-plans` scoped to this SPEC. Rewrites "Last commit" + "Last updated". Appends a new "Phase-11 state-1 brainstorm" subsection in Notes recording the family-pick + filter-pick rationale + alternatives considered + the 5-dimension scoring. Preserves all prior "Phase-NN rollovers" + "Phase-NN state-1 brainstorm" + "Phase-NN state-2 PLAN-write" + "Phase-NN state-3 execution arc" + "Phase-NN ADR ledger" subsections verbatim per D-3.5 (append-only) + D-3.4 (context isolation).

No code changes, no fixture changes, no Cargo.toml changes, no DECISIONS.md changes, no BEHAVIOR_CONTRACT.md changes. The DECISIONS.md ledger head stays at **ADR-0034**. ENVOY_TARGET.md + rust-toolchain.toml untouched (D-3.7 / D-3.9 unchanged).

**Commit message:**

```
phase 11: state-1 brainstorm — http-filter-fault SPEC.md (HTTP-filter-family third phase; 09 REVIEW M2 H2-decoration impl close site)
```

Per the project precedent (phase-10 state-1 brainstorm commit `c73f44f` title shape — `phase 10: state-1 brainstorm — http-filter-rbac SPEC.md (HTTP-filter-family second phase; 09 REVIEW M2 + M3 named close sites)`), state-1 brainstorm commit titles are descriptive with a parenthesized scope summary. No `[ADR-NNNN]` brackets — no ADR lands at this commit.

**Predecessor:** `e24053e` — phase 10 state-6 close-out (the most-recent commit; docs-only state-6 close-out per the standalone-phase invariant — phase 10 was standalone, NOT a sub-phase).

**Origin/main:** `e24053e`. Local + origin are in sync as of THIS state-1 brainstorm commit's prologue. After landing, the docs-only edits push to origin and the next CI run re-validates the docs-only edits compile cleanly through the 5 stable-toolchain gates + the parse_bootstrap fuzz target on the unchanged 17-seed corpus (predecessor docs-only CI runs took ~2-3m).

---

*End of SPEC. Phase 11 state-1 lifecycle complete on landing. The next session enters state 2 — writes PLAN.md per `superpowers:writing-plans`, performs the §6.2 empirical verification at PLAN-write (H2 listener; fault stats namespace + abort body bytes + H2 abort header set), and evaluates the §6.1 split gate.*
