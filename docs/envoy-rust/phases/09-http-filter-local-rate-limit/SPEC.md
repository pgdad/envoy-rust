# Phase 09 (`09-http-filter-local-rate-limit`) — SPEC

- **Phase id:** `09`
- **Slug:** `09-http-filter-local-rate-limit`
- **Status before this SPEC lands:** _not yet in ROADMAP.md_ (per `docs/envoy-rust/ROADMAP.md` at HEAD `304ce98`, the parent-08 / MVP-trunk close-out commit; the "HTTP filters family" §9 heading exists but has no concrete row beneath it). **This SPEC's landing commit is the first concrete row added beneath the HTTP-filter-family heading**, with `status: planned`.
- **Charter source:** `BOOTSTRAP_PROMPT.md` §9 — *"HTTP filters family — header manipulation, cors, compression, fault, **local+global rate limit**, jwt_authn, rbac, ext_authz, ext_proc, oauth2, csrf, buffer, lua, wasm, adaptive concurrency, admission control, bandwidth limit."* This phase lands the **local** half of "local+global rate limit"; the global half (gRPC rate-limit-service client; descriptor-based per-route matching) defers to a future HTTP-filter-family phase that engages gRPC client primitives.
- **Position in the project:** the **first post-MVP-trunk feature-family phase**. The MVP trunk 00→08 stands `done` as of commit `304ce98`; per `BOOTSTRAP_PROMPT.md` §9 the project transitions to feature-family expansion. Phase 09 is the cadence-setter for the HTTP-filter-family arc (and indirectly for all subsequent feature-family arcs).
- **depends-on:** `07` (the parent filter-chain framework). Phase 09 extends the 07.1-landed `envoy-filter::FilterPipeline` + `HttpFilterInstance` enum with a third production variant (after `Router` at 07.1 and `HeaderMutation` at 07.2). The 15-Docker-gated-fixture regression baseline established at parent-08 close (`0001-tcp-echo` through `0015-admin-drain-listeners`) carries forward unchanged per `BOOTSTRAP_PROMPT.md` §7.5 (b).
- **Brainstorm narrative:** see the "Phase-09 state-1 brainstorm" subsection of `docs/envoy-rust/STATE.md` for the family-pick + first-filter-pick rationale with alternatives considered.

---

## 1. Goal and acceptance signal

Phase 09 lands the `envoy.filters.http.local_ratelimit` filter (per upstream Envoy v1.33's documented filter name; typed_config `@type = type.googleapis.com/envoy.extensions.filters.http.local_ratelimit.v3.LocalRateLimit`) as the **second concrete pluggable HTTP filter** in the 07.x-established framework. The filter rate-limits decode-side requests against a per-instance token bucket; on rate-limit-hit it short-circuits via `Decision::StopAndSend(FilterResponse)` with status 429 + the `x-envoy-ratelimited: true` response header.

The phase also closes phase **07.2 REVIEW M1** (severed `position` plumbing in `HttpFilterInstance::build`) at its named-owner site — adding `HttpFilterInstance::LocalRateLimit` is precisely the "non-HeaderMutation `HttpFilterInstance` variant" the 07.2 disposition called out as the close-opportunity. Per 07.2's preferred fix ("delete dead plumbing"), the `_position: usize` parameter is dropped from `HttpFilterInstance::build` and the `.enumerate()` is removed from `FilterPipeline::build_from_config`.

**Differential surface added by phase 09:**

- **Fixture `0016-http-filter-local-rate-limit`** — bilateral assertion that both proxies, given an identical bootstrap with `token_bucket: { max_tokens: 3, tokens_per_fill: 0, fill_interval: 60s }` and a 5-request burst against a `direct_response` route, produce the deterministic status sequence `[200, 200, 200, 429, 429]` (first 3 requests consume the initial 3 tokens; requests 4-5 hit the rate limit because no refill arrives within the burst window). Asserts each 429 response carries the `x-envoy-ratelimited: true` response header. Reuses fixture 0007's minimal HCM + `direct_response` data-plane shape so the assertion focuses on the filter's rate-limit semantics, not on upstream proxy complexity.

**Acceptance signal (a)–(f), per `BOOTSTRAP_PROMPT.md` §7.5:**

- **(a)** Fixture `0016-http-filter-local-rate-limit` green at Docker-gated CI.
- **(b)** All **15 pre-existing differential fixtures** (`0001-tcp-echo` through `0015-admin-drain-listeners`) **remain green simultaneously** at the same CI run (regression-equivalence per `BOOTSTRAP_PROMPT.md` §7.5 (b)).
- **(c)** `h2spec` continues at ≥95% (parent-05 baseline 99.31%; phase 09 engages no H2-framing surfaces — the filter operates on the post-codec `FilterRequest` / `FilterResponse` abstraction).
- **(d)** `parse_bootstrap` fuzz target clean for the short-budget CI run on the extended corpus (one new seed for the local-ratelimit bootstrap shape).
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check` all clean.
- **(f)** `REVIEW.md` approved.

A **single CI run** must light up gates (a) through (e) **simultaneously** (continues the project precedent established at 06.1 / 07.x / 08.x — fixture inheritance is a regression vector).

---

## 2. Behavior-contract scope for phase 09

Phase 09 extends `docs/envoy-rust/BEHAVIOR_CONTRACT.md` with two authored additions, landed at the tasks where each is first empirically exercised (per the established 06.x / 07.x / 08.x doctrine — contract extensions land at empirical-engagement task time, NOT at PLAN-write time).

### 2.1 "Stat-name mapping" extension — 4 new rows

Four new counter rows under the `http_local_rate_limit.<stat_prefix>` namespace, mirroring upstream Envoy v1.33's documented stat tree:

| Stat name | Equivalence | Rationale |
|---|---|---|
| `http_local_rate_limit.<stat_prefix>.enabled` | value-exact | Counter; one increment per decode-side filter invocation when the filter is enabled. At phase-09 scope `filter_enabled` defaults to always-on (100%); per upstream Envoy parity `enabled` increments unconditionally on every `decode_headers` call. Both proxies emit one increment per request reaching the filter. |
| `http_local_rate_limit.<stat_prefix>.ok` | value-exact | Counter; one increment per `try_acquire` success (token consumed; request allowed to continue). Both proxies emit one increment per under-limit request. |
| `http_local_rate_limit.<stat_prefix>.rate_limited` | value-exact | Counter; one increment per `try_acquire` failure (no tokens available; request would-be-rate-limited). At phase-09 scope `filter_enforced` defaults to always-on (100%) so `rate_limited` counts coincide with `enforced` — but the upstream-Envoy semantic distinguishes "would-be-rate-limited" (`rate_limited`) from "actually-rate-limited" (`enforced`). Both proxies emit one increment per over-limit request. |
| `http_local_rate_limit.<stat_prefix>.enforced` | value-exact | Counter; one increment per request actually rate-limited (429 response emitted via `Decision::StopAndSend`). At phase-09 scope `enforced == rate_limited` because `filter_enforced` defaults to always-on; the two stat names track for upstream-Envoy parity. When a future phase lands runtime-fractional-percent `filter_enforced` overrides, the two counters diverge. Both proxies emit one increment per 429 emission. |

At phase-09 scope the four counters satisfy: `enabled == ok + rate_limited` and `enforced == rate_limited`. The functional dependency is real but the four stats are landed independently to match upstream Envoy v1.33's stat tree exactly. Each counter is incremented at its own fire site in `LocalRateLimitFilter::decode_headers` — no derived computation; one source of truth per name.

The `<stat_prefix>` segment is sourced from the filter config's `stat_prefix` field (required; rejected at parse time if absent or empty). This mirrors 06.1's `http.<stat_prefix>.downstream_rq_*` namespacing pattern.

### 2.2 "Header allow-list" extension — none required per ADR-0033

> **Revised at state 3 per ADR-0033** (the original state-1 row claiming
> upstream Envoy v1.33 auto-injects `x-envoy-ratelimited` on local-ratelimit
> 429 responses was empirically discovered to be incorrect at Task 5
> dispatch). See `docs/envoy-rust/DECISIONS.md` ADR-0033 for the full
> empirical evidence + the 4-commit corrective fixup sequence.

**Empirical upstream Envoy v1.33 behavior** (verified at Task 5 dispatch
against `envoyproxy/envoy:v1.33.0` with canonical bootstrap
`max_tokens: 3, tokens_per_fill: 3, fill_interval: 60s, status: { code: 429 }, filter_enabled: 100%, filter_enforced: 100%`):

- **429 response header set:** `content-length`, `content-type`, `date`,
  `server` — the four standard HTTP/1.1 response headers. **`x-envoy-ratelimited`
  is NOT emitted** by upstream's `envoy.filters.http.local_ratelimit`. That
  header is emitted only by the GLOBAL ratelimit filter
  (`envoy.filters.http.ratelimit`) and by router-side response-flag handling
  on the `RateLimited` response flag — neither of which the local_ratelimit
  filter sets or triggers in v1.33.
- **429 response body:** the source-hardcoded byte string `"local_rate_limited"`
  (18 bytes; `content-length: 18`). The upstream proto
  `envoy.extensions.filters.http.local_ratelimit.v3.LocalRateLimit` has no
  configurable `response_body` field.

**envoy-rust upstream-parity disposition per ADR-0033:**

- `LocalRateLimitFilter::decode_headers` does NOT inject `x-envoy-ratelimited`
  (revised at Commit B per ADR-0033; original Task 3 lock-in #13's injection
  is voided).
- `LocalRateLimitFilter::decode_headers` emits body
  `Bytes::from_static(b"local_rate_limited")` (18 bytes; revised at Commit B
  per ADR-0033; original Task 3 lock-in #13's `Bytes::new()` is voided).
- The 5 standard HTTP/1.1 response headers (`server`, `date`, `content-length`,
  `content-type`, `connection`) are decorated onto every filter-synth
  response by a new H1 HCM helper `decorate_filter_synth_response` (added at
  Commit C per ADR-0033; called from both `RequestPath::SynthFromDecode` and
  `RequestPath::SynthFromEncode` writer-arm sites; symmetric to `synth_status`
  at `crates/envoy-http1/src/hcm.rs:866-887`).

**No new Header allow-list row is needed.** The 04.1-landed `server` row +
the 04.1-landed `date` row of `docs/envoy-rust/BEHAVIOR_CONTRACT.md`'s
Header allow-list cover the cross-proxy implementation-identifying differences
(envoy-rust emits `server: envoy-rust`; upstream emits `server: envoy`;
both values are implementation-identifying; wall-clock `date` divergence
across the two proxies is the standard timing-non-determinism allowance).
The remaining 3 standard headers (`content-length`, `content-type`,
`connection`) are value-exact across proxies under the deterministic
fixture-0016 burst (5 sequential GET / probes; static `direct_response`
body; sticky H1 close-on-response convention).

**`response_headers_to_add` operator-configurable header plumbing is
preserved** by ADR-0033. Operators may still configure per-instance
response headers via the `response_headers_to_add` field on
`LocalRateLimitConfig`; these append to the synth response's header list
alongside the 5 standard headers from `decorate_filter_synth_response`.

No new BEHAVIOR_CONTRACT subsection is needed — the existing structure
(Header allow-list + Stat-name mapping + Equivalence matrix) covers the
surface. The 07.x-landed `Response status` row (exact) and `Response body`
row (byte-exact for deterministic handlers) cover the rest of the filter's
wire-level observables; the `Response body` row's `byte-exact for
deterministic handlers` clause directly anchors the bilateral
`"local_rate_limited"` body assertion on 429 probes.

---

## 3. Deliverables

Phase 09's scope is enumerated as deliverables `D1`–`D8` below. **The state-2 PLAN-writer organizes deliverables into tasks** (and evaluates the §6.1 split gate) — these are not 1:1 with tasks. Some deliverables compose into one task; some split across two. The deliverables are LISTED in roughly the order the PLAN-writer is expected to execute them, but the SPEC is not prescriptive about the order; only about the surface.

### D1 — `envoy-config` schema extension

At `crates/envoy-config/src/bootstrap.rs`, extend the existing `HttpFilterTypedConfig` enum (currently `Router`, `HeaderMutation` variants per 07.2) with a third variant `LocalRateLimit(LocalRateLimitFilterConfig)`. The config struct shape mirrors upstream Envoy v1.33's `envoy.extensions.filters.http.local_ratelimit.v3.LocalRateLimit` (typed_config @type), narrowed to the minimum-viable surface for phase 09:

```rust
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LocalRateLimitFilterConfig {
    pub stat_prefix: String,                  // REQUIRED; non-empty
    pub token_bucket: TokenBucket,            // REQUIRED
    // OPTIONAL — defaults; full per-route + runtime-fractional + descriptors defer per §4
    #[serde(default)]
    pub response_headers_to_add: Vec<HeaderValueOption>,
    #[serde(default = "default_status_code")]
    pub status: HttpStatusCode,               // default 429
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TokenBucket {
    pub max_tokens: u32,                      // REQUIRED; must be > 0
    pub tokens_per_fill: u32,                 // REQUIRED; may be 0 (no refill)
    pub fill_interval: serde_yaml::Value,     // REQUIRED; Duration shape "60s" / "100ms"
    //                                           parsed via humantime-style helper at validate-time
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HttpStatusCode {
    pub code: u16,                            // REQUIRED; phase-09 accepts 429 only
}

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
```

All struct shapes carry `#[serde(deny_unknown_fields)]` per the established envoy-config discipline (rejects forward-looking fields that envoy-rust does not yet support). The `fill_interval` is parsed as `serde_yaml::Value` and converted to `std::time::Duration` at validate-time via a hand-rolled "humantime-style" duration parser (or via the existing `serde_yaml::Value::as_str()` + manual `parse` if the PLAN-writer settles on it); avoids the `humantime-serde` foundations grant.

The four upstream-Envoy fields explicitly **deferred** (each rejected by `deny_unknown_fields`; each enumerated in §4 below): `descriptors`, `local_rate_limit_per_downstream_connection`, `filter_enabled`, `filter_enforced`, `request_headers_to_add_when_not_enforced`.

### D2 — `envoy-config` validator extension

At `crates/envoy-config/src/bootstrap.rs::validate_http_filters`, extend the existing per-variant validator dispatch (currently handling `Router` + `HeaderMutation` per 07.2) with a `LocalRateLimit` arm calling a new `validate_local_rate_limit_config(cfg) -> Result<(), ConfigError>` sub-validator. The validator checks:

- `stat_prefix` is non-empty (otherwise `ConfigError::EmptyLocalRateLimitStatPrefix`).
- `token_bucket.max_tokens > 0` (otherwise `ConfigError::TokenBucketMaxTokensMustBePositive`).
- `token_bucket.fill_interval` parses to a `Duration` and is `> Duration::ZERO` (otherwise `ConfigError::InvalidTokenBucketFillInterval { message: String }`).
- `status.code == 429` (otherwise `ConfigError::UnsupportedLocalRateLimitStatusCode { code: u16 }`; phase 09 accepts only 429 — non-429 status defers).

Four new `ConfigError` variants land at this site (`EmptyLocalRateLimitStatPrefix`, `TokenBucketMaxTokensMustBePositive`, `InvalidTokenBucketFillInterval`, `UnsupportedLocalRateLimitStatusCode`). Each has its own unit test cases for positive + negative parse paths. The validator is exercised by the existing fuzz target `parse_bootstrap` (the new fixture's bootstrap is seeded into the corpus per D8.2).

### D3 — `envoy-filter::LocalRateLimitFilter` runtime + hand-rolled token bucket

New module `crates/envoy-filter/src/local_rate_limit.rs`. Hand-rolled per **D-3.2**'s *"Every individual filter ... Must be written from scratch"* doctrine + the 07.2 `header_mutation.rs` precedent. Module shape:

```rust
#![forbid(unsafe_code)]   // inherited from crate root

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use envoy_stats::{Counter, StatsRegistry};
use crate::error::FilterError;
use crate::pipeline::Decision;
use crate::types::{FilterRequest, FilterResponse};

/// Hand-rolled token-bucket primitive. `AtomicU64` for the live token count;
/// `Mutex<Instant>` for the last-fill timestamp. Lazy fill: tokens computed
/// at `try_acquire` time, NOT via a background refill task. Per §5.2 below.
#[derive(Debug)]
struct TokenBucket {
    max_tokens: u64,
    tokens_per_fill: u64,
    fill_interval: Duration,
    state: Arc<TokenBucketState>,
}

#[derive(Debug)]
struct TokenBucketState {
    tokens: AtomicU64,
    last_fill_instant: Mutex<Instant>,
}

impl TokenBucket {
    fn new(max_tokens: u64, tokens_per_fill: u64, fill_interval: Duration) -> Self { /* ... */ }

    /// Returns `true` if a token was successfully consumed; `false` if the
    /// bucket is empty (post-refill).
    async fn try_acquire(&self) -> bool { /* lazy fill + compare_exchange consume */ }
}

/// The `envoy.filters.http.local_ratelimit` runtime filter.
#[derive(Debug, Clone)]
pub struct LocalRateLimitFilter {
    stat_prefix: String,
    bucket: Arc<TokenBucket>,
    response_headers_to_add: Vec<(String, String)>,
    enabled_counter: Arc<Counter>,
    ok_counter: Arc<Counter>,
    rate_limited_counter: Arc<Counter>,
    enforced_counter: Arc<Counter>,
}

impl LocalRateLimitFilter {
    /// Lower an `envoy_config::LocalRateLimitFilterConfig` into the runtime
    /// filter + register the 4 stat counters against the StatsRegistry.
    pub(crate) fn build_from_config(
        cfg: &envoy_config::LocalRateLimitFilterConfig,
        registry: &Arc<StatsRegistry>,
    ) -> Result<Self, FilterError> { /* ... */ }

    pub(crate) fn decode_headers(&mut self, _req: &mut FilterRequest) -> Decision {
        // Inc `enabled` unconditionally.
        // Block on `bucket.try_acquire().await`:
        //   - on success: inc `ok`; return Decision::Continue.
        //   - on failure: inc `rate_limited`; inc `enforced`;
        //     return Decision::StopAndSend(FilterResponse{
        //       status: 429,
        //       reason: Some("Too Many Requests"),
        //       headers: [("x-envoy-ratelimited", "true")]
        //                ++ self.response_headers_to_add,
        //       body: Bytes::new(),
        //     }).
        unimplemented!()
    }

    pub(crate) fn encode_headers(&mut self, _resp: &mut FilterResponse) -> Decision {
        // Decode-only filter (per upstream Envoy semantic).
        Decision::Continue
    }
}
```

**Token-bucket implementation note (signpost for the PLAN-writer + state-3 implementer):** the lazy-fill calculation must use `compare_exchange` to atomically (a) read the current token count, (b) compute the post-refill count via `(current + elapsed_intervals * tokens_per_fill).min(max_tokens)`, (c) decrement by 1, and (d) update the `last_fill_instant` to the post-refill timestamp. Naive multi-step read-then-write loses tokens under concurrency. The state-3 implementer should mirror the 08.2 `DrainState::drain()` two-sequenced-CAS shape (commit `c1c9604`'s Task 1 + the `fddabd2` fixup for the TOCTOU lesson). A 3-thread concurrent torture test (`token_bucket_atomic_compare_exchange_under_concurrency`) is REQUIRED per §6.3 below.

**Async vs sync signature decision (signpost):** `decode_headers` is currently synchronous per the 07.1 framework (returns `Decision`, not `impl Future<Output = Decision>`). The token bucket's `Mutex<Instant>` lock acquisition could be either `tokio::sync::Mutex` (async, requires the framework to go async) or `std::sync::Mutex` (sync, blocks on contention but contention is rare under the lazy-fill model). **Recommended:** `std::sync::Mutex` (or better: `parking_lot::Mutex` if D-3.2 permits — defer; use std). Sync is consistent with the 07.1 framework's synchronous-iteration design (per `crates/envoy-filter/src/lib.rs` doc-comment). The state-3 implementer settles this empirically — if `std::sync::Mutex` contention is observable in unit tests, switch to `tokio::sync::Mutex` and make the framework async (an architectural shift — invokes ADR-0033 per §7 below). **Recommended posture: sync.**

### D4 — `HttpFilterInstance::LocalRateLimit` variant + dispatch

Extend `crates/envoy-filter/src/instance.rs::HttpFilterInstance` enum with a new variant `LocalRateLimit(LocalRateLimitFilter)`. Extend the `build` dispatch + `decode_headers` + `encode_headers` dispatch arms.

Re-export `LocalRateLimitFilter` from `crates/envoy-filter/src/lib.rs::pub use local_rate_limit::LocalRateLimitFilter;`.

Since `LocalRateLimitFilter::build_from_config` now requires an `&Arc<StatsRegistry>` parameter (per D6), the `HttpFilterInstance::build` signature widens to accept the registry. The PLAN-writer settles whether to thread the registry through `FilterPipeline::build_from_config` as a new parameter OR to construct the registry-aware filter at a higher layer (HCM-side) and pass it to the pipeline pre-constructed. **Recommended:** thread the registry through `build_from_config` as a new parameter (additive; analogous to how 08.1's `Bootstrap`+`ClusterManager` threading widened `AdminHandler::new`). The H1 + H2 HCM filter-chain wiring sites (`crates/envoy-http1/src/hcm.rs::serve_connection` + `crates/envoy-http2/src/hcm.rs::handle_one_stream`) already hold an `Arc<StatsRegistry>` (per 06.x stats wiring) — passing it to `FilterPipeline::build_from_config` is one new argument at two sites.

### D5 — 07.2 REVIEW M1 closure: severed `position` plumbing

The 07.2 REVIEW M1 deferred to "whichever future HTTP-filter-family phase next adds a non-HeaderMutation `HttpFilterInstance` variant" with the preferred fix being "drop the dead `_position` parameter from `build` (option b — consistent with the 07.1 REVIEW I1 'delete dead plumbing' posture)". Phase 09 IS that phase; D5 lands the close:

- At `crates/envoy-filter/src/instance.rs::HttpFilterInstance::build` (currently `pub(crate) fn build(hf: &envoy_config::HttpFilter, _position: usize) -> Result<Self, FilterError>`): drop the `_position: usize` parameter. New signature: `pub(crate) fn build(hf: &envoy_config::HttpFilter, registry: &Arc<StatsRegistry>) -> Result<Self, FilterError>` (per D4 the `registry` parameter is added at the same site).
- At `crates/envoy-filter/src/pipeline.rs::FilterPipeline::build_from_config` (currently `for (position, hf) in filters.iter().enumerate() { out.push(HttpFilterInstance::build(hf, position)?); }`): drop the `.enumerate()` and the `position` arg. New shape: `for hf in filters.iter() { out.push(HttpFilterInstance::build(hf, registry)?); }`.
- At `crates/envoy-filter/src/instance.rs::tests::build_router_succeeds` (currently `let instance = HttpFilterInstance::build(&hf, 0).expect(...)`): drop the trailing `0` arg + add the `registry` arg.

The hardcoded `position: 0` at `crates/envoy-filter/src/header_mutation.rs::map_entry`'s `FilterError::UnsupportedFilterType { position: 0, ... }` defense-in-depth construction is LEFT AS-IS — it's a within-HeaderMutation entry-index encoding, not a filter-chain position, and the validator at `envoy-config::validate_header_mutation_entries` catches the real case at parse time. The hardcode is semantically slightly-wrong but unreachable in normal operation; preserving it avoids touching `header_mutation.rs` in phase 09 (which is doctrinally cleaner — minimum-touch the established 07.2 surface).

D5 is mechanical (~5-10 LoC across 3 sites). The PLAN-writer co-locates D5 with D4 in a single task (or lands D5 first as a Task-1 preamble before D4). Either ordering works; the carryforward closure narrative in PROGRESS attributes M1 to the D5-landing task commit.

### D6 — Stats wiring (4 new counters per upstream-Envoy parity)

At `LocalRateLimitFilter::build_from_config`, register four `Counter` handles against the `Arc<StatsRegistry>`:

- `format!("http_local_rate_limit.{stat_prefix}.enabled")` → `enabled_counter: Arc<Counter>`
- `format!("http_local_rate_limit.{stat_prefix}.ok")` → `ok_counter: Arc<Counter>`
- `format!("http_local_rate_limit.{stat_prefix}.rate_limited")` → `rate_limited_counter: Arc<Counter>`
- `format!("http_local_rate_limit.{stat_prefix}.enforced")` → `enforced_counter: Arc<Counter>`

Increment sites (all within `decode_headers`):

- `self.enabled_counter.inc()` — unconditional, at function prologue.
- `self.ok_counter.inc()` — on `try_acquire` success, before returning `Decision::Continue`.
- `self.rate_limited_counter.inc()` AND `self.enforced_counter.inc()` — on `try_acquire` failure, before constructing the `Decision::StopAndSend(FilterResponse)`.

The 06.x stats convention applies: `StatsRegistry::register_counter` is idempotent for same-name re-registration, so multiple filter instances sharing the same `stat_prefix` (a misconfiguration but not validator-rejected at phase 09) increment the same underlying counter — correct under both proxies' shared-counter semantics. BEHAVIOR_CONTRACT extension (§2.1 above) lands at the same task commit as D6 per the 07.x cadence.

### D7 — BEHAVIOR_CONTRACT.md extension

Two extensions land at the task commits where they're empirically exercised (NOT at SPEC time):

- **D7.1 — `Stat-name mapping` 4 rows** land at the D6 stats-wiring task commit.
- **D7.2 — `Header allow-list` 1 row (`x-envoy-ratelimited`)** lands at the D8.1 fixture-0016 task commit (the empirical site exercising the synthetic-emit).

### D8 — Fixture + harness extension + fuzz seed + in-process backstop

- **D8.1 — Fixture `tests/fixtures/0016-http-filter-local-rate-limit/`.** Reuses fixture 0007's HCM + `direct_response` bootstrap shape so the bilateral assertion focuses on the filter, not on upstream proxy complexity. Bootstrap shape:

  ```yaml
  static_resources:
    listeners:
    - name: ingress_http
      address: { socket_address: { address: 0.0.0.0, port_value: 10000 } }
      filter_chains:
      - filters:
        - name: envoy.filters.network.http_connection_manager
          typed_config:
            "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
            stat_prefix: ingress_http
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
            route_config:
              virtual_hosts:
              - name: default
                domains: ["*"]
                routes:
                - match: { prefix: "/" }
                  direct_response: { status: 200, body: { inline_string: "ok\n" } }
  ```

  Probe shape: `Driver::Http1ProbeList` with 5 sequential probes (each `GET /` with no body). Expected per-probe statuses: `[200, 200, 200, 429, 429]`. Asserts probes 4-5 carry `x-envoy-ratelimited: true` response header (via `Http1Probe.expected_headers` set-equal-modulo-allow-list).

  Docker-gated wrapper at `tests/differential/tests/http_filter_local_rate_limit.rs` mirroring `tests/differential/tests/http_filter_header_mutation.rs` shape (the 07.2 precedent). One `#[tokio::test]` `http_filter_local_rate_limit_fixture` invoking `run_fixture("0016-http-filter-local-rate-limit").await`.

  **No new harness extensions required.** `Driver::Http1ProbeList` exists from 04.2 (`tests/differential/src/lib.rs:613` per cross-reference); `expected_headers` allow-list discipline carries from 04.x. The PLAN-writer verifies `Http1Probe` carries `expected_headers: Option<Vec<HeaderRule>>` and extends if not.

- **D8.2 — Fuzz corpus seed.** New file `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_local_rate_limit_filter.yaml` containing the bootstrap shape above. Mirrors the 07.2 `hcm_header_mutation_filter.yaml` precedent. Extends the fuzz target's seed coverage from 15 to 16 entries (one per fixture's bootstrap shape).

- **D8.3 — In-process backstop.** New file `crates/envoy-bin/tests/http_filter_local_rate_limit.rs` mirroring `crates/envoy-bin/tests/http_filter_header_mutation.rs` (07.2 precedent). Single `#[tokio::test]` exercising the rate-limit semantics in-process (no Docker). The test boots `envoy-bin` with a synthesized bootstrap (`max_tokens: 2`); issues 4 sequential `GET /` requests against the bound listener; asserts the status sequence `[200, 200, 429, 429]` + the `x-envoy-ratelimited: true` header on the 429 responses.

---

## 4. Out of scope (deferred non-goals)

Phase 09 explicitly does NOT land:

- **Per-route `typed_per_filter_config` for LocalRateLimit.** The 09 filter's config is sourced exclusively from the filter-chain-level entry. Per-route policy variation defers to whichever future HTTP-filter-family phase first needs it. CORS is the natural close site per upstream Envoy's per-route CorsPolicy pattern. **Recommended posture per §5.5: this deferral may warrant a state-2 ADR if the PLAN-writer concludes it establishes a project-wide pattern for the HTTP-filter-family arc; recommended ADR-0033 slot per §7.**
- **`descriptors` (per-route action matching + gRPC ratelimit service integration).** Defers to **global rate limit** (a future HTTP-filter-family phase that engages the gRPC client + RLS protocol).
- **`local_rate_limit_per_downstream_connection: true`.** The per-connection scope is a distinct primitive (one bucket per downstream TCP connection, not one bucket per filter instance). Defers to a future HTTP-filter-family phase that engages per-connection state.
- **Runtime fractional-percent overrides (`filter_enabled`, `filter_enforced`).** Defers to whichever phase first surfaces runtime-control needs (likely the RTDS subsection of the xDS family). At phase 09 the filter is always-enabled-and-always-enforced (the 100% / 100% case).
- **Custom HTTP status codes (`status: { code: <non-429> }`).** Phase 09's validator rejects `status.code != 429`. Whichever later HTTP-filter-family phase first needs a non-429 status code (e.g., `503 Service Unavailable` for circuit-breaker-shaped rejection) lifts the validator constraint.
- **`response_headers_to_add` and `request_headers_to_add_when_not_enforced` field-set richness.** Phase 09's `response_headers_to_add` accepts the upstream-Envoy `HeaderValueOption` shape but the field's effect at phase 09 is appended-on-rate-limited-responses-only (no encode-side mutation). `request_headers_to_add_when_not_enforced` is parse-and-reject per `deny_unknown_fields` because envoy-rust always enforces — the field is meaningless under the 09 always-on posture.
- **Token-bucket background-refill task.** Replaced by lazy fill calculation at decision time. Aligns with the project's "no spawn per filter instance" doctrine (per the 06.x / 07.x stats-wiring posture).
- **`x-envoy-ratelimit-reset` response header.** Deprecated in upstream Envoy v1.33 anyway; defers indefinitely.
- **Probabilistic randomness primitives (`rand` crate; `getrandom` foundations grant).** No need for this filter — the always-100% `filter_enabled` / `filter_enforced` posture eliminates the per-request random sampling that probabilistic-percentage overrides would need. The first HTTP-filter-family phase that needs random sampling (e.g., a `fault` filter with non-100% percentage) lands the `rand` foundations grant via ADR.
- **Global rate limit filter (`envoy.filters.http.ratelimit`).** Defers to a future HTTP-filter-family phase per the §1 charter scoping ("the local half ... the global half defers").
- **`envoy.filters.network.local_ratelimit` (network-layer variant).** Defers to the Network filters family per `BOOTSTRAP_PROMPT.md` §9.
- **Per-route + per-listener typed `local_rate_limit` overrides via `typed_per_filter_config` AT the route level.** Same deferral as the per-route `typed_per_filter_config` above.
- **Live ROADMAP-row mutation by rate-limit hits.** ROADMAP is project-doc-only; the runtime rate-limit observability lives in stats per D6 + BEHAVIOR_CONTRACT per §2.1.

---

## 5. Architectural invariants

Phase 09 honors and extends the established cross-crate invariants:

### 5.1 Crate boundaries

- **`envoy-filter` stays sole-dep-owner of HTTP filter chain iteration.** All new variant + filter implementation land in `crates/envoy-filter/`. No new top-level crate is created. No new workspace member.
- **`LocalRateLimitFilter` lives at `crates/envoy-filter/src/local_rate_limit.rs`.** Mirrors the 07.2 `header_mutation.rs` placement pattern (one module per concrete filter).
- **No new path-deps within `envoy-filter`.** The crate's `Cargo.toml` adds `envoy-stats = { path = "../envoy-stats" }` (the stats counter primitive needs to be importable inside the filter) — this is a workspace-internal path-dep only, NOT a new top-level Cargo dep. The 04.1 REVIEW M5/M9 Cargo.lock cadence carries forward unchanged.

### 5.2 Hand-rolled token bucket per D-3.2

The token-bucket primitive is hand-rolled per **D-3.2**'s *"Every individual filter ... Must be written from scratch"* doctrine + the broader "stats subsystem" / "access log formatters and sinks" / "admin API" hand-roll posture honored end-to-end through the MVP trunk. The implementation uses only **std-lib + tokio + bytes** (all D-3.2-permitted; all already pulled): `std::sync::atomic::AtomicU64` for the live token count; `std::sync::Mutex<std::time::Instant>` for the last-fill timestamp; `std::time::Duration` for the fill-interval; `bytes::Bytes::new()` for the synthetic 429 response body.

**Explicit non-grants:** no `governor`, `ratelimit`, `nonzero-ext`, `parking_lot`, `humantime`, `humantime-serde`, `rand`, `getrandom` — none on D-3.2's permitted-foundations list; none required by the 09 scope. The state-3 implementer must NOT pull any of these (any pull forces a foundations-grant ADR per D-3.5).

### 5.3 No new top-level Cargo deps

The recommended no-foundations-grants posture per parent-07 / parent-08 SPEC §7 carries forward through phase 09. **If the state-3 implementer surfaces a genuine foundation need at execution time, a foundations-grant ADR lands per D-3.5 — see §7 for the conditional-ADR slots.**

### 5.4 Decode-only filter

`LocalRateLimitFilter::encode_headers` is a no-op (returns `Decision::Continue` unconditionally). The filter operates only on the decode-side request flow. This matches upstream Envoy v1.33's documented `envoy.filters.http.local_ratelimit` semantic (the filter does not consult the response). The encode-side method exists on the `HttpFilterInstance` enum's dispatch arm per the 07.x framework symmetry, but never mutates the response.

### 5.5 Filter-chain config only (NOT per-route)

The sole config source is the filter-chain-level entry per the 07.2 `header_mutation` precedent. Per-route `typed_per_filter_config` defers per §4 above. Whichever future HTTP-filter-family phase first needs per-route policy variation extends the 07.x framework with a per-route lookup primitive — the new primitive is the gating architectural change, NOT the filter that consumes it. The 09 SPEC explicitly does not invent the per-route lookup primitive; the validator rejects per-route LocalRateLimit configs at parse time via `deny_unknown_fields` on the route-level `typed_per_filter_config` field (which doesn't exist on the envoy-config Route struct at the 09 baseline anyway).

### 5.6 Always-enabled, always-enforced

`filter_enabled` and `filter_enforced` runtime overrides default to always-on (the 100% / 100% case). Both fields are parse-and-reject per `deny_unknown_fields` on `LocalRateLimitFilterConfig` (envoy-rust doesn't model the fields at all at the 09 scope; they're not in the struct shape per D1). Runtime-fractional-percent overrides defer per §4 above.

### 5.7 Statelessness across requests

The `LocalRateLimitFilter` carries no per-request state (the token bucket lives inside `Arc<TokenBucketState>` shared across all filter invocations on the same filter-chain instance). Per `Clone` of `HttpFilterInstance` (the `derive(Debug, Clone)` on the enum), each per-request pipeline-clone shares the underlying `Arc<TokenBucketState>` — the bucket's atomic state is the single source of truth across all clones. This matches the 07.2 `HeaderMutationFilter` clone-semantics (which holds `Vec<RuntimeHeaderMutation>` directly; rate-limit is different because the bucket MUST be shared across clones for correct rate-limiting semantics).

### 5.8 H1 + H2 symmetric

The filter operates on the codec-agnostic `FilterRequest` / `FilterResponse` abstraction per the 07.1 framework. Both H1 and H2 HCM dispatch sites (`crates/envoy-http1/src/hcm.rs::serve_connection`, `crates/envoy-http2/src/hcm.rs::handle_one_stream`) invoke `pipeline.decode_headers` at the established 07.x integration seam — no per-codec branching for phase 09. Fixture 0016 exercises H1 only (single fixture per the established cadence; H2 coverage from `Driver::Http2` extends naturally in future); the in-process backstop D8.3 also exercises H1 only.

---

## 6. Implementation signposts for the planner

The state-2 PLAN-writer reads this section to drive PLAN structure.

### 6.1 Split-gate evaluation (read first)

Per `BOOTSTRAP_PROMPT.md` §6.1, the state-2 PLAN-write evaluates whether the PLAN exceeds ~25 numbered tasks OR ~1500 LoC. Phase 09's surface estimate at SPEC time:

- D1 — envoy-config schema (~150 LoC + ~100 LoC unit tests). ~1 task.
- D2 — envoy-config validator (~50 LoC + ~80 LoC unit tests). ~1 task or co-located with D1.
- D3 — token-bucket primitive + LocalRateLimitFilter runtime (~250 LoC + ~200 LoC unit tests including the concurrency torture test). ~1-2 tasks.
- D4 — HttpFilterInstance::LocalRateLimit variant + dispatch (~30 LoC + ~30 LoC tests). ~1 task.
- D5 — 07.2 REVIEW M1 closure (~10 LoC + ~5 LoC test edits). Co-located with D4 in 1 task.
- D6 — stats wiring (~40 LoC + ~50 LoC tests). Co-located with D3.
- D7 — BEHAVIOR_CONTRACT.md extensions (~20 LoC contract edits). Co-located with D6 + D8.1.
- D8.1 — fixture 0016 (~80 LoC YAML + ~30 LoC Docker-gated wrapper). ~1 task.
- D8.2 — fuzz corpus seed (~30 LoC YAML). ~1 task.
- D8.3 — in-process backstop (~150 LoC). ~1 task.
- State-4 verification + STATE-advance (~docs). ~1 task.

**SPEC-time projection: ~10-13 tasks; ~900-1100 LoC** (production ~430, tests ~530, fixture/doc ~150). The phase is comfortably **under** the split-gate threshold on both dimensions. **Recommended posture: single-phase (no split).** State-2 PLAN-write lands a standalone `PLAN.md` per the 04.3 / 05.1 / 06.x / 07.x / 08.x cadence.

**If state-3 surfaces unexpected complexity:** the in-execution release valve is per-step commit splitting recorded in PROGRESS (per the 06.x / 07.x / 08.x precedent), NOT a phase-level nest-split. Per parent-08 SPEC §6.1 alternative (vi).

### 6.2 D5 (07.2 REVIEW M1 closure) lands AT or BEFORE D4

D5 is mechanical (~5-10 LoC); D4 adds the variant that surfaces M1 as eligible-to-close. Two ordering options:

- **(a) Co-located (recommended).** Single task lands `HttpFilterInstance::LocalRateLimit` variant + drops `_position` parameter + drops `.enumerate()`. Combined edit is ~30-40 LoC.
- **(b) Sequenced (D5 first, then D4).** Two atomic edits. Slightly more PROGRESS narrative but each commit is smaller.

Either ordering works. **Recommended: option (a).**

### 6.3 Token-bucket concurrency torture test is REQUIRED

The 08.2 Task 1 + Task 1 fixup precedent (`fddabd2` TOCTOU race in `DrainState::drain_signal`) establishes the project's most-Critical-fix-to-date pattern: concurrent atomic-state primitives MUST land with a deterministic concurrent-execution torture test that would fail under the pre-fix shape. For phase 09's token bucket:

- **REQUIRED test: `token_bucket_atomic_compare_exchange_under_concurrency`.** Spawn N tokio tasks (recommend N=8) each calling `try_acquire` in a tight loop M times (recommend M=10_000). Assert the sum of `true` returns across all tasks equals `min(N*M, max_tokens)` (initial fill, no refill). Verify under `Ordering::AcqRel` semantics that no token-double-count occurs.

The state-3 implementer co-locates this test with the token-bucket primitive in `local_rate_limit.rs::tests`. Per the 06.1 8-thread × 10_000-inc Counter torture test precedent (verifying `Ordering::Relaxed` soundness on Counter), the LocalRateLimit torture test is the analogous primitive-validation pattern.

### 6.4 The 06.x stats convention

Per 06.x cadence: StatsRegistry registration at `LocalRateLimitFilter::build_from_config` time. Per-filter-instance ownership of 4 Counter handles. Stat-name namespace `http_local_rate_limit.<stat_prefix>` matches upstream Envoy v1.33 parity exactly (no project-internal label divergence; the BEHAVIOR_CONTRACT.md row dispositions are all `value-exact`).

### 6.5 The 07.x BEHAVIOR_CONTRACT extension cadence

Per the established 06.x / 07.x / 08.x doctrine: contract extensions land at the TASK where each is first empirically exercised, NOT at PLAN-write time and NOT at state-1 SPEC time. For phase 09:

- The 4 Stat-name mapping rows land at the D6 task commit (where the 4 counters are first registered + first incremented in tests).
- The 1 Header allow-list row (`x-envoy-ratelimited`) lands at the D8.1 task commit (where the fixture first asserts the header on a 429 response).

### 6.6 Pre-state-4 fmt discipline (continues per 06.1 R-9)

Per-task PROGRESS sections quote `cargo fmt --all -- --check` at every PROGRESS-task close, NOT just at state-4. Carries forward from the 06.1 → 06.2 → 06.3 → 07.1 → 07.2 → 08.1 → 08.2 chain.

### 6.7 State-4 evidence-discipline (continues per 05.3 → 06.x → 07.x → 08.x chain)

Per-gate quoted evidence in PROGRESS at the state-4 verification task: real CI run URL + HEAD SHA + completion timestamp + per-gate quoted output (all 5 stable-toolchain gates + each Docker-gated fixture + h2spec_pass_rate_gate + parse_bootstrap fuzz iteration count).

### 6.8 Cargo.lock cadence

The phase-04.1 REVIEW M5/M9 (Cargo.lock cadence ratification ADR) carries forward unchanged through phase 09 if no new top-level Cargo deps are added (the recommended posture per §5.3 above). The Cargo.lock diff at the phase-09 reviewed range is expected to be minimal (~5-10 lines for the `envoy-stats` path-dep registration on `envoy-filter`; possibly +1-2 lines for transitive crates already in the workspace). If a foundations-grant ADR fires (conditional ADR-0034 per §7), the cadence pick is forced and must land alongside.

### 6.9 PROGRESS.md skeleton + Task 1 preamble land alongside PLAN.md at state-2

Per the 06.2 / 06.3 / 07.1 / 07.2 / 08.1 / 08.2 cadence (divergence from the 06.1 "PROGRESS created at Task 1" pattern, which the project rationalized away at 06.2). State-2 PLAN-write lands both `PLAN.md` + `PROGRESS.md` skeleton + Task 1 preamble in a single standalone pre-Task-1 commit.

### 6.10 Subagent-driven execution at state 3 (per `feedback_execution_style`)

The user's standing preference auto-memory `feedback_execution_style` ("default to subagent-driven-development; skip the two-option fork") applies at state 3. The state-2 PLAN-write organizes tasks for subagent-driven execution per the 06.x / 07.x / 08.x cadence (each task independent enough to dispatch in isolation; PROGRESS attestation per-task; in-phase recovery cadence if any task surfaces a code-quality-review-blocking finding).

---

## 7. ADR projection

**Recommended posture: NO new ADRs land in phase 09.** The work fits inside the existing permitted-foundations set per §5.2 + §5.3 above. The DECISIONS.md ledger head stays at **ADR-0032** through phase 09's state-1 (this) commit; the next-available number is **ADR-0033**.

Two conditional ADR slots stay reserved-available for state-2 / state-3 execution-time landing if reality forces them:

- **Conditional ADR-0033 — per-route filter config deferral.** If the state-2 PLAN-writer or state-3 implementer concludes that the per-route `typed_per_filter_config` deferral (per §4 + §5.5 above) warrants append-only durability (because it establishes a project-wide pattern for the HTTP-filter-family arc — every filter-family phase from 09 onward inherits the deferral until one needs per-route config), ADR-0033 lands at the surfacing commit. **Recommended posture: defer the ADR until a future filter actually needs per-route config (CORS is the natural close site per upstream Envoy's per-route CorsPolicy pattern); the 09 deferral is doctrinally clear without an ADR.** If the PLAN-writer disagrees and lands ADR-0033 at state 2, the ADR text records the family-wide deferral pattern + the named close site.

- **Conditional ADR-0034 — foundations grant (only if the token-bucket primitive forces one).** Lands at the task where the state-3 implementer surfaces a materially-worse-than-foundation result for the token-bucket primitive (e.g., a need for `governor`, `ratelimit`, `parking_lot`, `humantime`, or `humantime-serde`). **Recommended posture per §5.2: no grant.** The hand-rolled implementation should be sufficient; the `std::time::Duration` parse for `fill_interval` is a hand-rolled match on `"<N>s"` / `"<N>ms"` / `"<N>us"` (covers the upstream Envoy v1.33 documented Duration shapes — extend if reality surfaces a needed shape).

If both conditional ADRs land in lex-then-execution order (per-route deferral first at state-2; foundations grant later at state-3), the ledger advances `ADR-0032 → ADR-0033 → ADR-0034`. If only one lands, it takes the next-available number (ADR-0033). If neither lands, the ledger stays at ADR-0032.

---

## 8. State-machine signposts for the phase-09 state-2 session

The next session (state 2) reads this section and acts.

- **Lifecycle state at session start:** State 2 (SPEC.md exists; PLAN.md does not).
- **Skill:** `superpowers:writing-plans` per `BOOTSTRAP_PROMPT.md` §5 state 2.
- **Output:** `docs/envoy-rust/phases/09-http-filter-local-rate-limit/PLAN.md` + `PROGRESS.md` skeleton + Task 1 preamble (standalone pre-Task-1 commit per the 04.3 / 05.1 / 06.x / 07.x / 08.x PLAN-write cadence).
- **Split-gate evaluation:** §6.1 above. **Recommended: single-phase (no split).** PLAN materializes ~10-13 tasks / ~900-1100 LoC; well under the §6.1 ~25-task / ~1500-LoC gate.
- **ROADMAP row flip:** at state-2 PLAN-write, flip ROADMAP row `09` `planned` → `in-progress` (per the 08.1 / 08.2 sub-phase precedent — new rows added at `planned`; flipped to `in-progress` at state-2 PLAN-write or state-3 first task commit). The 09 row was added at THIS state-1 commit with `status: planned`.
- **D5 (07.2 REVIEW M1 closure) ordering:** §6.2 above. **Recommended: co-located with D4 in a single task.**
- **Per-route filter config deferral ADR:** §7 above. **Recommended: defer the ADR.** If the PLAN-writer disagrees, ADR-0033 lands at state 2 alongside PLAN.md (the most-natural state-2 ADR-landing site per the parent-08 SPEC §10 precedent).
- **PLAN-time SPEC corrections:** the PLAN-writer reads this SPEC against HEAD `<state-1-commit-SHA>` and flags any drift (mechanical signature differences between the SPEC's projected types and the actual on-disk envoy-config/envoy-filter types). Per the 06.2 → 06.3 → 07.x → 08.x precedent ("8 PLAN-write SPEC corrections at 08.2 PLAN-write" pattern), corrections land in the PROGRESS Task 1 preamble.

---

## 9. Commit message format (for state 6 of the phase-09 lifecycle)

```
phase 09: envoy.filters.http.local_ratelimit + fixture 0016 + 07.2 REVIEW M1 close

<1-3 sentence summary>

Differential surface: fixture 0016-http-filter-local-rate-limit; all 16 Docker-gated fixtures (0001-0016) green simultaneously at CI run <ID> HEAD <SHA>.
Conformance: h2spec ≥95% gate held at parent-05 baseline; no H2-framing surfaces engaged.
```

If ADR(s) land, the bracketed list is appended to the title per `BOOTSTRAP_PROMPT.md` §5.3 (e.g., `... [ADR-0033]`). Per the recommended no-ADRs posture per §7, the bracketed list is omitted by default.

If phase 09 unexpectedly splits at state-2 into 09.1 + 09.2 (NOT recommended; see §6.1), the closing-sub-phase commit carries `[parent 09 done]` per the 07.2 / 08.2 closing-sub-phase precedent.

---

## 10. State-machine commit (this commit — phase-09 state-1 close-out)

This SPEC is the state-1 output. The state-1 close-out commit is **docs-only** and touches:

- **CREATE** `docs/envoy-rust/phases/09-http-filter-local-rate-limit/SPEC.md` (this file).
- **MODIFY** `docs/envoy-rust/ROADMAP.md` — **adds a new row** beneath the existing "HTTP filters family" §9 heading. Row format per the schema: `| id | title | depends-on | status | sub-phases | summary |`. New row content:
  ```
  | 09 | envoy.filters.http.local_ratelimit + fixture 0016 + 07.2 REVIEW M1 close | 07 | planned | — | fixture 0016-http-filter-local-rate-limit green; envoy-filter gains LocalRateLimitFilter (hand-rolled token bucket + decode-side StopAndSend with 429 + x-envoy-ratelimited) + HttpFilterInstance::LocalRateLimit variant; envoy-config gains LocalRateLimit + TokenBucket schema + 4 new ConfigError variants; 07.2 REVIEW M1 closed (severed `position` plumbing deleted) |
  ```
  The "HTTP filters family" heading itself stays unchanged; the new row joins beneath it as the family's first concrete row per `BOOTSTRAP_PROMPT.md` §4.1 invariant 2 (append-only history; never delete rows). All other ROADMAP rows untouched.
- **MODIFY** `docs/envoy-rust/STATE.md` — advances "Active phase" pointer from `_none_ — awaiting next planning` (the MVP-trunk-complete state) to:
  - `id: 09`
  - `slug: 09-http-filter-local-rate-limit`
  - `directory: docs/envoy-rust/phases/09-http-filter-local-rate-limit/`
  - `status: phase 09 lifecycle state 1-complete / state-2-next (SPEC.md landed; PLAN.md does not exist)`
  
  Rewrites "Next expected skill" to `superpowers:writing-plans` scoped to this SPEC. Rewrites "Last commit" + "Last updated". Appends a new "Phase-09 state-1 brainstorm" subsection in Notes recording the family-pick + first-filter-pick rationale + alternatives considered. Preserves all prior "Phase-NN rollovers" + "Phase-NN state-1 brainstorm" + "Phase-NN state-2 split" + "Phase-NN state-2 PLAN-write" subsections verbatim per D-3.5 (append-only) + D-3.4 (context isolation).

No code changes, no fixture changes, no Cargo.toml changes, no DECISIONS.md changes, no BEHAVIOR_CONTRACT.md changes. The DECISIONS.md ledger head stays at **ADR-0032**. ENVOY_TARGET.md + rust-toolchain.toml untouched (D-3.7 / D-3.9 unchanged).

**Commit message:**

```
phase 09: state-1 brainstorm — http-filter-local-rate-limit SPEC.md (HTTP-filter-family first phase; 07.2 REVIEW M1 named close site)
```

Per the project precedent (parent-08 state-1 brainstorm commit `0202e38` title shape — `phase 08: state-1 brainstorm — admin-api-and-drain SPEC.md (07 carryforward closures + endpoint-triggered drain scope)`), state-1 brainstorm commit titles are descriptive with parenthesized scope summary. No `[ADR-NNNN]` brackets — no ADR lands at this commit.

**Predecessor:** `304ce98` — phase 08.2 state-6 phase-done close-out (ALSO the parent-08 close-out AND the MVP-trunk close-out per the closing-sub-phase invariant + the `BOOTSTRAP_PROMPT.md` §8 seeded-MVP-trunk invariant).

**Origin/main:** `304ce98`. Local + origin are in sync as of THIS state-1 brainstorm commit's prologue. After landing, the docs-only edits push to origin and the next CI run re-validates the docs-only edits compile cleanly through the 5 stable-toolchain gates (predecessor docs-only CI runs took ~2-3m).

---

*End of SPEC. Phase 09 state-1 lifecycle complete on landing. The next session enters state 2 — writes PLAN.md per `superpowers:writing-plans` and evaluates the §6.1 split gate.*
