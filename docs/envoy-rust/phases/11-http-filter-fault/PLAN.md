# Phase 11 (`11-http-filter-fault`) — PLAN

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development`
> per `feedback_execution_style` auto-memory and per the established 06.x / 07.x / 08.x /
> 09 / 10 cadence. Tasks 1-8 implement the phase per `SPEC.md`. Steps use `- [ ]` checkbox
> syntax for tracking.

**Goal.** Land the `envoy.filters.http.fault` filter (abort path) as the fourth concrete
pluggable HTTP filter in the 07.x-established framework (after HeaderMutation at 07.2,
LocalRateLimit at 09, Rbac at 10): a hand-rolled decode-side abort filter (per D-3.2's
*"Every individual filter ... Must be written from scratch"* doctrine) that, on a request
matching its optional header gate (AND-semantics over a `Vec<HeaderMatcher>`) when the
deterministic percentage selects (0% or 100% only), short-circuits via
`Decision::StopAndSend` with the operator-configured HTTP status + body
`"fault filter abort"` (18 bytes, §6.2-verified) + the standard response headers via the
HCM filter-synth decoration helpers. **Close the most-named open carryforward, 09 REVIEW
M2 (implementation arm), at deliverable D6** — land a new
`decorate_filter_synth_response_h2` helper symmetric to the H1
`decorate_filter_synth_response` (`crates/envoy-http1/src/hcm.rs:968`), wired into both H2
HCM synth writer-arm sites, so the H1 + H2 HCM filter-synth writer paths reach parity.
Fixture 0018 runs the fault filter on an **HTTP/2** listener, exercising the H2
filter-synth path bilaterally — the first HTTP-filter-family phase to do so.

**Architecture.** The `FaultFilter` lives at `crates/envoy-filter/src/fault.rs` (new
module; mirrors the 07.2 `header_mutation.rs` + 09 `local_rate_limit.rs` + 10 `rbac.rs`
placement). Runtime state is 2 scalars (`abort_status: u16` + `abort_selects: bool`) + a
`Vec<HeaderMatcher>` gate + 1 `Arc<Counter>` handle. The select boolean is computed once
at `build_from_config` time from the (validator-guaranteed-deterministic) percentage — no
per-request randomness, no PRNG (fractional percentage defers per SPEC §4 + §5.6). The
gate uses the existing 04.2-landed `envoy_config::HeaderMatcher::matches(&[(String,
String)]) -> bool` directly — no matcher logic duplicated. Filter-chain integration
extends `HttpFilterInstance` with a fifth production variant `Fault(FaultFilter)` and
**reuses the phase-10-threaded 3-arg `(filters, &registry, hcm_stat_prefix: &str)`
`build_from_config` / `build` signatures unchanged** (no signature widening this phase).
The new `FractionalPercent` + `DenominatorType` config types live in `crates/envoy-config`
(the only genuinely new *shared* schema types phase 11 introduces; authored to be reusable
by future filters that take a `FractionalPercent`). The H2 decoration helper
`decorate_filter_synth_response_h2` lives in `crates/envoy-http2/src/response.rs` (adjacent
to `build_http_response`) and runs on the `envoy_http1::Response` *before*
`build_http_response` translates it to the `http::Response<()>` head — it adds
`content-length` always + `server`/`date`/`content-type` only-if-missing, and (unlike H1)
**no `connection`** (an H2-forbidden hop-by-hop header stripped by `build_http_response`
per `H2_FORBIDDEN_HOP_BY_HOP`). The differential fixture 0018 reuses a new
`Driver::Http2ProbeList { probes: Vec<Http1Probe> }` harness driver mirroring
`Driver::Http1ProbeList`'s per-probe assertion cascade but calling `drive_http2`.

**Tech Stack.** Zero new top-level Cargo deps. Zero new workspace path-deps
(`envoy-stats` already on `envoy-filter` from phase 09; `envoy-config` already; `envoy-http2`
already depends on `envoy-http1` + `envoy-filter`). Permitted-foundations primitives used:
`std::sync::Arc`, `Vec`, `bytes::Bytes`. **No `rand`** — the fractional-percentage PRNG
defers per SPEC §4 + §5.6 (when it lands, it is hand-rolled per D-3.2, not granted). None
of `rand` / `ipnet` / `cidr` / `regex`-beyond-ADR-0021 is on D-3.2's permitted-foundations
list; none is required by the abort-only fault surface narrowed to phase-11 scope. The
differential harness gains one new `Driver::Http2ProbeList` variant (~70-90 LoC; mirrors
`Http1ProbeList` + reuses `drive_http2` + the existing `Http1Probe` struct) — a harness
extension that lands without an ADR per the established `Driver::Http1ProbeList` (04.2) /
`Driver::AdminScrape` (06.1/08.1) no-ADR cadence.

---

## 1. PLAN-write SPEC corrections

Per the 06.2 / 06.3 / 07.x / 08.x / 09 / 10 precedent (phase-10 landed 7 PLAN-write SPEC
corrections at `55abc61`), the PLAN-writer reads SPEC §3 surfaces against HEAD `1370aaa`
and flags mechanical signature drift between projected types and actual on-disk types.
Corrections land in execution at the named task and in the PROGRESS.md Task 1 preamble.

1. **The `envoy.filters.http.fault` filter name is NOT in any "reject list" at
   `crates/envoy-filter/src/error.rs:51`.** SPEC §3 D2 states *"The `envoy.filters.http.fault`
   filter name is currently in the unsupported-filter reject list at
   `crates/envoy-filter/src/error.rs:51` ... the PLAN-writer removes/updates the
   `error.rs:51` reject entry + its associated test (`error.rs:57`)."* **This is incorrect.**
   Verified at HEAD `1370aaa`: `error.rs:51` + `:57` are a **test fixture string** inside
   `FilterError::tests::display_router_not_terminal_includes_position_and_name`, which uses
   `"envoy.filters.http.fault"` as an arbitrary example non-Router filter name to assert the
   `RouterNotTerminal` Display contains the name. It is NOT a reject list. The *actual*
   rejection mechanism is serde: `HttpFilterTypedConfig` carries `#[serde(tag = "@type",
   deny_unknown_fields)]` (`bootstrap.rs:443`), so a fault `@type`
   (`type.googleapis.com/envoy.extensions.filters.http.fault.v3.HTTPFault`) currently fails
   deserialization with an *unknown-variant* serde error (a `ConfigError::Parse`-class
   error, not `UnsupportedHttpFilter`). **Action: D5 does NOT touch `error.rs`.** Adding the
   `Fault(FaultConfig)` variant (D1, Task 1) + the `validate_http_filters` dispatch arm (D2,
   Task 1) is the *entire* "rejected → supported" move. The `error.rs:57` test stays
   verbatim — it remains a valid `RouterNotTerminal` Display test (using fault as an
   example of a non-terminal filter is still correct; the test is unrelated to whether fault
   is a supported filter).

2. **`ConfigError` enum lives in `crates/envoy-config/src/lib.rs`, NOT
   `crates/envoy-config/src/bootstrap.rs`** (same correction as phase-09 PLAN §1 item 1 +
   phase-10 PLAN §1 item 2). The validator function `validate_http_filters` IS in
   `bootstrap.rs` (line 1863 at HEAD `1370aaa`). **Action at Task 1:** the 3 new fault
   `ConfigError` variants land in `lib.rs`; the new `FaultConfig` / `FaultAbort` /
   `FractionalPercent` / `DenominatorType` schema items + the `Fault` `HttpFilterTypedConfig`
   variant + the `validate_fault_config` sub-validator + the `validate_http_filters` Fault
   dispatch arm land in `bootstrap.rs`. (Whether the new schema structs live in `bootstrap.rs`
   or a sibling module is the implementer's call; phase-10 placed RBAC schema in
   `bootstrap.rs`, so phase 11 follows suit for the fault schema. `FractionalPercent` +
   `DenominatorType` are general shared types — place them adjacent to the fault schema in
   `bootstrap.rs` and re-export from `lib.rs` so future filters can use them.)

3. **`HeaderMatcher::matches` takes `&[(String, String)]` — RE-CONFIRMED at HEAD `1370aaa`.**
   `crates/envoy-config/src/matcher.rs:19`: `pub fn matches(&self, headers: &[(String,
   String)]) -> bool`. This matches `envoy_filter::types::FilterRequest::headers: Vec<(String,
   String)>` directly (`crates/envoy-filter/src/types.rs:31`) — no adapter needed. **Action
   at Task 2:** `header_gate_matches` calls `m.matches(&req.headers)` directly. (SPEC §3 D4
   already records this; the re-confirmation closes the SPEC §8 "verify the exact signature"
   signpost.)

4. **HCM `stat_prefix` threading already exists — NO signature widening this phase.** SPEC §5
   D5 asserts phase-10 widened `FilterPipeline::build_from_config` →
   `HttpFilterInstance::build` to the 3-arg `(filters, &registry, hcm_stat_prefix: &str)`
   shape. RE-CONFIRMED at HEAD `1370aaa`: `pipeline.rs:40-50`
   (`build_from_config(filters, registry, hcm_stat_prefix: &str)` → calls
   `HttpFilterInstance::build(hf, registry, hcm_stat_prefix)`) + `instance.rs:73-76`
   (`build(typed_config, registry, hcm_stat_prefix: &str)`). **Phase 11 reuses both
   unchanged.** The `Fault` build arm passes `hcm_stat_prefix` straight through to
   `FaultFilter::build_from_config`. Unlike phase 10 (which widened the H1 HCM call site at
   `hcm.rs:185`), **phase 11 widens NO HCM call-site signature** — the 3-arg shape is
   already in place.

5. **`FilterResponse` field shapes — RE-CONFIRMED at HEAD `1370aaa`.**
   `crates/envoy-filter/src/types.rs:43`: `pub struct FilterResponse { pub status: u16, pub
   reason: Option<&'static str>, pub headers: Vec<(String, String)>, pub body: Bytes }`. The
   abort `FilterResponse` is `{ status: self.abort_status, reason: None, headers: vec![],
   body: Bytes::from_static(FAULT_ABORT_BODY) }` — `reason` is `Option<&'static str>` so
   `None` is correct (NOT `Option<String>`). `Decision::{Continue, StopAndSend(FilterResponse)}`
   at `pipeline.rs:12-14`. No drift.

6. **H2 HCM writer-arm sites + line numbers — RE-CONFIRMED at HEAD `1370aaa`.** Decode-side
   `H2RequestPath::SynthFromDecode(r) => { ... r }` at `crates/envoy-http2/src/hcm.rs:373`
   (the `Response` is constructed at `hcm.rs:176-182` in the decode-side
   `Decision::StopAndSend(filter_resp)` arm). Encode-side `Decision::StopAndSend(replacement)
   => { resp = Response { ... } }` at `crates/envoy-http2/src/hcm.rs:436`. Both currently
   return/construct the filter response verbatim WITHOUT decoration. `build_http_response(resp:
   &Response) -> Result<HttpResponse<()>, Http2Error>` at `crates/envoy-http2/src/response.rs:29`
   strips `H2_FORBIDDEN_HOP_BY_HOP` (which contains `"connection"` per `lib.rs:34-41`) and
   drops the reason phrase. **Action at Task 4:** the new helper decorates the `Response`
   before it reaches `build_http_response` at both arm sites.

7. **All 3 SPEC §6.2 empirical-verification projections MATCH — no inline ADR-0035.** The
   PLAN-writer performed the §6.2 verification at THIS state-2 commit against
   `envoyproxy/envoy:v1.33.0` Docker on an HTTP/2 listener (full evidence in PROGRESS Task 1
   preamble). All 3 findings match the SPEC projections exactly: (a) stats namespace
   `http.ingress_http.fault.aborts_injected` (= SPEC §2.1 `http.<hcm_stat_prefix>.fault.aborts_injected`);
   (b) abort body `"fault filter abort"` = 18 bytes, hex `66 61 75 6c 74 20 66 69 6c 74 65 72
   20 61 62 6f 72 74` (= SPEC §2.2 projection exactly — **no off-by-one** this time, unlike
   phase-10 RBAC); (c) H2 abort header set `{server, content-length, content-type, date}`, no
   `connection` (= SPEC §2.2 + D6 projection exactly). **No SPEC revision needed; no ADR
   triggered. DECISIONS.md ledger head stays at ADR-0034.** This is the recommended posture
   per SPEC §7 option A ("verify all 3; land ADR only if any differ" — none differ).

---

## 2. Architecture decisions locked at PLAN-write time

Per `feedback_pick_recommendation` ("always pick the recommended option; do not ask"), the
following decisions are locked at this commit. PROGRESS.md Task 1 preamble references these
by `#NN` for in-execution lookup. The lock-in density mirrors the phase-10 PLAN §2 table (46
lock-ins at `55abc61`); phase 11's lower count reflects the sharply-scoped abort-only surface
(no recursive-tree-walk, no token-bucket atomicity surface).

| # | Signpost | Decision | Rationale |
|---|---|---|---|
| 1 | Module placement | New module `crates/envoy-filter/src/fault.rs`; new `pub mod fault;` in `lib.rs` between `error` and `header_mutation` (alphabetical); new re-export `pub use fault::FaultFilter;` after `pub use error::FilterError;` (alphabetical). | Mirrors 07.2 / 09 / 10 (one module per concrete filter). |
| 2 | No new path-deps | `envoy-filter/Cargo.toml` `[dependencies]` block unchanged — `envoy-stats` + `envoy-config` already present. `envoy-http2/Cargo.toml` unchanged (already depends on `envoy-http1` + `envoy-filter`). **Zero Cargo manifest edits.** | SPEC §5.1 + §5.3. |
| 3 | FaultConfig schema | `#[derive(Debug, Clone, Deserialize, PartialEq)] #[serde(deny_unknown_fields)] pub struct FaultConfig { pub abort: FaultAbort, #[serde(default)] pub headers: Vec<HeaderMatcher> }`. The phase-11-deferred fields (`delay`, `response_rate_limit`, `max_active_faults`, downstream-controlled fault headers) are NOT modeled; `deny_unknown_fields` rejects them. | SPEC §3 D1 + §4. |
| 4 | FaultAbort schema | `#[derive(Debug, Clone, Deserialize, PartialEq)] #[serde(deny_unknown_fields)] pub struct FaultAbort { pub http_status: u16, pub percentage: FractionalPercent }`. `grpc_status` + `header_abort` NOT modeled per SPEC §4. | SPEC §3 D1 + §4. |
| 5 | FractionalPercent schema | `#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)] #[serde(deny_unknown_fields)] pub struct FractionalPercent { pub numerator: u32, #[serde(default = "default_denominator")] pub denominator: DenominatorType }`. `Copy` is cheap (2 scalars) and convenient for the eval helper. | SPEC §3 D1. New shared type. |
| 6 | DenominatorType schema | `#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)] #[serde(rename_all = "SCREAMING_SNAKE_CASE")] pub enum DenominatorType { Hundred, TenThousand, Million }`. Maps `"HUNDRED"`→100, `"TEN_THOUSAND"`→10_000, `"MILLION"`→1_000_000. **No `deny_unknown_fields` on a fieldless enum** (it is meaningless / rejected by serde for unit-variant enums; `rename_all` handles the wire names). | SPEC §3 D1. |
| 7 | default_denominator helper | `fn default_denominator() -> DenominatorType { DenominatorType::Hundred }` at module scope in `bootstrap.rs`. | SPEC §3 D1 (`#[serde(default = "default_denominator")]` → default HUNDRED). |
| 8 | DenominatorType::value | `impl DenominatorType { pub fn value(self) -> u32 { match self { Hundred => 100, TenThousand => 10_000, Million => 1_000_000 } } }`. Pure-compute; used by D2 validator + D3 eval helper. | SPEC §3 D2 + D3. |
| 9 | FractionalPercent::selects_deterministic (D3) | `impl FractionalPercent { pub fn selects_deterministic(&self) -> bool { self.numerator == self.denominator.value() } }`. Returns `true` at 100% (`numerator == denominator.value()`), `false` at 0% (`numerator == 0`). The validator (D2) guarantees `numerator ∈ {0, denominator.value()}`, so this is a pure boolean — **no PRNG, no per-request randomness**. | SPEC §3 D3 + §5.6. |
| 10 | HttpFilterTypedConfig variant | `Fault(FaultConfig)` — fifth variant after `Router` + `HeaderMutation` + `LocalRateLimit` + `Rbac`. `@type` rename `"type.googleapis.com/envoy.extensions.filters.http.fault.v3.HTTPFault"`. | SPEC §3 D1. Mirrors the Rbac variant's rename pattern. |
| 11 | 3 new ConfigError variants | `InvalidFaultAbortStatus { listener: String, status: u16 }`, `FaultPercentageOutOfRange { listener: String, numerator: u32, denominator: u32 }`, `UnsupportedFractionalFaultPercentage { listener: String, numerator: u32, denominator: u32 }`. Land in `crates/envoy-config/src/lib.rs` alongside existing filter variants. Each carries `listener: String` per the envoy-config error-context discipline. | SPEC §3 D2 (SPEC says "two-to-three"; PLAN locks 3 — one per failure class for precise diagnostics). |
| 12 | validate_fault_config sub-validator | New private fn `fn validate_fault_config(cfg: &crate::FaultConfig, listener_name: &str) -> Result<(), crate::ConfigError>` in `bootstrap.rs`, alongside `validate_rbac_config` / `validate_local_rate_limit_config`. Checks (in order): (1) `abort.http_status ∈ 100..=599` else `InvalidFaultAbortStatus`; (2) `numerator <= denominator.value()` else `FaultPercentageOutOfRange`; (3) `numerator ∈ {0, denominator.value()}` else `UnsupportedFractionalFaultPercentage`. No iteration over `headers` (the 04.2 `HeaderMatcher` has no parse-time validation beyond deserialize per SPEC §3 D2). | SPEC §3 D2 + §5.6. **Check order matters:** the out-of-range check precedes the fractional check so `numerator > denominator` reports `FaultPercentageOutOfRange` (operator typo), not the fractional rejection. |
| 13 | Validator dispatch arm | At `validate_http_filters` (`bootstrap.rs:1863`), the `match &f.typed_config` block gains a fifth arm AFTER the `Rbac` arm (closing at `:1911`): `HttpFilterTypedConfig::Fault(cfg) => { if f.name != "envoy.filters.http.fault" { return Err(crate::ConfigError::UnsupportedHttpFilter { name: f.name.clone() }); } validate_fault_config(cfg, listener_name)?; }`. Mirrors the Rbac dispatch arm shape exactly. The terminal-router check stays unchanged. | SPEC §3 D2. |
| 14 | FaultFilter struct shape | `#[derive(Debug, Clone)] pub struct FaultFilter { abort_status: u16, abort_selects: bool, header_gate: Vec<envoy_config::HeaderMatcher>, aborts_injected: Arc<Counter> }`. `Clone` is cheap (2 scalars + a small Vec clone + 1 Arc ref-bump). | SPEC §3 D4 + §5.7. |
| 15 | FaultFilter::build_from_config | `pub(crate) fn build_from_config(cfg: &envoy_config::FaultConfig, registry: &Arc<StatsRegistry>, hcm_stat_prefix: &str) -> Result<Self, FilterError>`. Computes `abort_selects = cfg.abort.percentage.selects_deterministic()`; clones `cfg.headers` into `header_gate`; registers the abort counter via `registry.register_counter(&format!("http.{hcm_stat_prefix}.fault.aborts_injected"))`. 3-arg shape reused from phase 10 (lock-in / SPEC correction #4). | SPEC §3 D4 + D5 + D7. |
| 16 | FAULT_ABORT_BODY const | `const FAULT_ABORT_BODY: &[u8] = b"fault filter abort";` (18 bytes; hex `66 61 75 6c 74 20 66 69 6c 74 65 72 20 61 62 6f 72 74`) at module scope in `fault.rs`. §6.2-verified — matches upstream Envoy v1.33 exactly (no off-by-one; SPEC correction #7). | SPEC §2.2 + §6.2 empirical verification. |
| 17 | decode_headers shape | `pub(crate) fn decode_headers(&mut self, req: &mut FilterRequest) -> Decision`. If `header_gate_matches(&self.header_gate, req) && self.abort_selects`: inc `aborts_injected`, return `Decision::StopAndSend(FilterResponse { status: self.abort_status, reason: None, headers: vec![], body: Bytes::from_static(FAULT_ABORT_BODY) })`. Else `Decision::Continue`. Counter increments EXACTLY ONCE per aborted request, at the abort site (one source of truth); never on pass-through. | SPEC §3 D4 + §5.6 + §6.5. |
| 18 | encode_headers shape | `pub(crate) fn encode_headers(&mut self, _resp: &mut FilterResponse) -> Decision { Decision::Continue }`. No-op. | SPEC §5.4 — decode-only filter at phase-11 scope; encode-side method exists for framework symmetry. |
| 19 | header_gate_matches helper | `fn header_gate_matches(gate: &[envoy_config::HeaderMatcher], req: &FilterRequest) -> bool { gate.iter().all(|m| m.matches(&req.headers)) }`. Empty gate ⇒ `Iterator::all` over empty slice returns `true` (no gate ⇒ fault applies to all requests). All listed matchers must match (AND semantics) per upstream. | SPEC §3 D4 + §5.6. |
| 20 | HttpFilterInstance::Fault variant | `Fault(FaultFilter)` — new production variant between `Rbac` (`instance.rs:37`) and the `#[cfg(feature = "test-util")]` block (`instance.rs:42`). `use crate::fault::FaultFilter;` added alongside the existing filter `use`s. The 2 test-util variants are preserved verbatim. | SPEC §3 D5. |
| 21 | HttpFilterInstance build/dispatch arms | `build` arm: `envoy_config::HttpFilterTypedConfig::Fault(cfg) => Ok(HttpFilterInstance::Fault(FaultFilter::build_from_config(cfg, registry, hcm_stat_prefix)?)),` (mirrors the Rbac arm at `instance.rs:90-91`). `decode_headers` arm: `HttpFilterInstance::Fault(f) => f.decode_headers(req),`. `encode_headers` arm: `HttpFilterInstance::Fault(f) => f.encode_headers(resp),`. | SPEC §3 D5. |
| 22 | decorate_filter_synth_response_h2 (D6) | New free fn in `crates/envoy-http2/src/response.rs` (adjacent to `build_http_response`): `pub(crate) fn decorate_filter_synth_response_h2(resp: &mut envoy_http1::Response)`. Adds `content-length` always (overwrite from `resp.body.len()`); adds `server`/`date`/`content-type` only-if-missing. **NO `connection`** + **NO `close` parameter** (unlike the H1 helper) — `connection` is H2-forbidden + stripped by `build_http_response`. Uses the same defaults as the existing H2/H1 synth path (`server: envoy-rust`, `content-type: text/plain`, IMF-fixdate `date`). | SPEC §3 D6 + §2.2 + §6.2 (verified header set `{server, content-length, content-type, date}`). |
| 23 | D6 default-value sourcing | The H2 helper must source the same `server` / `content-type` / `date` defaults the H1 helper uses. The H1 helper uses `DEFAULT_SERVER_NAME` (`"envoy-rust"`), `DEFAULT_CONTENT_TYPE` (`"text/plain"`), `now_imf_fixdate()` (`crates/envoy-http1/src/hcm.rs`). The implementer reuses whatever the envoy-http2 crate already exposes for these (the H2 HCM already emits these on non-synth responses); if envoy-http2 lacks an IMF-fixdate helper, reuse the envoy-http1 one via the existing path-dep (it is already a dependency). The Task 4 implementer spot-checks the existing H2 response-header emission to source the exact constants — do NOT hardcode divergent strings. | SPEC §3 D6; D-3.3 (match upstream wire shape — both proxies' fixed strings live behind the BEHAVIOR_CONTRACT `server` allow-list row). |
| 24 | D6 wiring sites | Wire `decorate_filter_synth_response_h2(&mut r)` into the decode-side `H2RequestPath::SynthFromDecode(r)` arm at `crates/envoy-http2/src/hcm.rs:373` (decorate `r` before it is returned/sent) AND `decorate_filter_synth_response_h2(&mut resp)` into the encode-side `Decision::StopAndSend(replacement)` arm at `:436` (decorate the constructed `resp` Response). No phase-11 filter takes the encode-side path (fault is decode-side), but the helper is wired symmetrically per the H1 precedent (which decorates at both `hcm.rs:598` + `:636`) so future encode-side-short-circuiting H2 filters inherit decoration for free. | SPEC §3 D6 + §5.8. |
| 25 | D6 2 unit tests | 2 tests in `crates/envoy-http2/src/response.rs` (or `hcm.rs` if the helper sits there) mirroring the H1 helper tests (`crates/envoy-http1/src/hcm.rs:1405-1452`): (1) `decorate_h2_adds_standard_headers_when_filter_provides_none` — asserts all 4 H2 standard headers added (`server`, `date`, `content-length`, `content-type`) + content-length == body.len() + **`connection` is NOT added**; (2) `decorate_h2_preserves_filter_headers_and_overwrites_content_length` — filter provides `server: my-proxy` + stale `content-length: 10` + an extra non-standard header; asserts server preserved, content-length overwritten to body.len(), date/content-type added, extra header preserved, **`connection` absent**. | SPEC §3 D6 ("2 tests"). |
| 26 | D7 counter registration | At `build_from_config`, register one counter: `registry.register_counter(&format!("http.{hcm_stat_prefix}.fault.aborts_injected"))` → `aborts_injected: Arc<Counter>`. Idempotent re-registration is fine (06.x StatsRegistry contract). | SPEC §3 D7 + §6.5 + §6.2 (namespace verified `http.ingress_http.fault.aborts_injected`). |
| 27 | D7.1 BEHAVIOR_CONTRACT row landing | 1 new "Stat-name mapping" row lands at Task 2 (D4 + D7 co-located) commit per SPEC §6.6 + the 06.x / 07.x / 08.x / 09 / 10 doctrine (contract extensions land at the task where first empirically exercised). New section header `**11 entries (Fault filter):**` after the existing `**10 entries (RBAC filter):**` rows. Row content per SPEC §2.1 verbatim. No new Header allow-list row per SPEC §2.2 (the 4-header set is value-exact across both proxies under the deterministic burst; the 04.1-landed `server` + `date` rows cover implementation-identifying divergences). | SPEC §2.1 + §6.6. |
| 28 | D8.1 fixture 0018 shape | `tests/fixtures/0018-http-filter-fault/`: bootstrap HCM (`codec_type: HTTP2`) + `envoy.filters.http.fault` (abort `http_status: 503`, `percentage: { numerator: 100, denominator: HUNDRED }`, `headers: [- name: x-fault, string_match: { exact: abort }]`) + `envoy.filters.http.router` + `direct_response: { status: 200, body: { inline_string: "ok\n" } }`. Mirrors fixture 0009's H2 HCM + direct_response data-plane shape (per SPEC §3 D8.1). | SPEC §3 D8.1. |
| 29 | D8.1 harness driver (recommended option (a)) | Add `Driver::Http2ProbeList { probes: Vec<Http1Probe> }` to `tests/differential/src/lib.rs` — a new variant mirroring `Driver::Http1ProbeList { probes }` but driving each probe over H2 via the existing `drive_http2`. The `Http1Probe` struct is codec-agnostic (request shape + per-probe expectations) and is reused directly. The match arm mirrors the `Http1ProbeList` per-probe assertion cascade (`lib.rs:2136`) verbatim, swapping `drive_http1` → `drive_http2`. ~70-90 LoC. **Recommended per SPEC §6.1 + §8 — gives both abort + pass-through arms bilaterally over H2 in one fixture (the `[503,200,503,200]` pattern mirroring phase-10's `[403,200,403,200]`).** | SPEC §3 D8.1 + §6.1 (option (a) recommended). |
| 30 | D8.1 probe list | `Driver::Http2ProbeList` with 4 sequential probes (each `GET /`, `host: envoy-rust.test`). Per-probe `extra_headers`: `[[("x-fault","abort")], [], [("x-fault","abort")], []]`. Expected per-probe statuses: `[503, 200, 503, 200]`. `expected_body` byte-exact: `"fault filter abort"` on probes 1 + 3 (503); `"ok\n"` on probes 2 + 4 (200). `expected_headers: set_equal_modulo_allow_list` on all 4 probes (asserts the 503 probes carry the standard H2 headers via `decorate_filter_synth_response_h2`). | SPEC §3 D8.1 + §2.2. |
| 31 | D8.1 `drive_http2` GET-only constraint | `drive_http2` (`lib.rs:1282`) carries a `debug_assert!(matches!(method, Http1Method::Get))` — GET-only. All 4 fixture-0018 probes are GET, so no `drive_http2` widening is needed. The `Http2ProbeList` match arm passes `&probe.method` through; the debug-assert holds. | SPEC §3 D8.1 (harness internals verified at PLAN-write). |
| 32 | D8.1 Docker-gated wrapper | `tests/differential/tests/http_filter_fault.rs` mirroring `tests/differential/tests/http_filter_rbac.rs` (the 10 precedent). One `#[tokio::test]` `http_filter_fault_fixture` invoking `run_fixture("0018-http-filter-fault").await`. | SPEC §3 D8.1. |
| 33 | D8.2 fuzz corpus seed | New file `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_fault_filter.yaml` mirroring fixture 0018's bootstrap shape. Extends seed count 17 → 18. Includes `crates/envoy-config/fuzz/.gitignore` allow-list extension AND the `crates/envoy-config/src/bootstrap.rs::tests::fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS-array extension (BOTH files edited in the SAME commit per the 09 + 10 Task 6 follow-up lesson — NOT a follow-up commit). | SPEC §3 D8.2 + 09/10 Task 6 lesson. |
| 34 | D8.3 in-process backstop (H1 path) | New file `crates/envoy-bin/tests/http_filter_fault.rs` using `tokio::process::Command + .kill_on_drop(true) + stdout: Stdio::null() + stderr: Stdio::piped()` (the 09 REVIEW M3 discipline, CLOSED at phase-10 Task 7 `dd95673`, now the standing pattern). **Recommended: exercise the H1 path in-process** (boot an H1 listener; the H1 `decorate_filter_synth_response` helper already exists) — gives cheap H1-codec coverage of the abort semantics complementing the H2 differential fixture. Single `#[tokio::test]` issuing 4 sequential GET probes with varying `x-fault` values; asserts status sequence `[503, 200, 503, 200]` + body `"fault filter abort"` on 503 probes + `"ok\n"` on 200 probes + **per-probe standard-header presence on the 503 probes** (heeds the 10 M1 lesson per SPEC §6.4 — recommended option (a)). | SPEC §3 D8.3 + §6.4. |
| 35 | D8.3 direct code-spot-check precedent | The Task 7 implementer reads `crates/envoy-bin/tests/http_filter_rbac.rs` (the 10 backstop; `kill_on_drop` + Stdio discipline) directly via `Read` before writing — verifies the precedent subprocess shape by direct code-spot-check, NOT by relying on the prior phase's PROGRESS claim. Per the standing precedent-verification discipline. | SPEC §6.4 + SPEC §6.11. |
| 36 | 09 REVIEW M2 implementation close (D6) | D6 (Task 4) closes the 09 REVIEW M2 *implementation* arm. The Task 4 PROGRESS subsection attributes the M2 implementation close + cross-references the phase-10 D5 amendment (`docs/envoy-rust/DECISIONS.md:699`) + the phase-09 PROGRESS Commit C forward-reference. **After D6 lands, the 09 → 10 → 11 M2 chain ENDS. No new ADR required** — the close shape is ordinary implementation deliverable work, not a decision (SPEC §2.3). | SPEC §6.3 + §2.3. |
| 37 | ADR landings | **NO ADR lands in phase 11 by default.** All 3 §6.2 projections match (SPEC correction #7); no foundations grant projected; the fractional-deferral durability ADR defers per SPEC §7 option B (the §4 + §5.6 rationale is self-contained); the `Driver::Http2ProbeList` harness primitive lands without an ADR per the established harness-extension no-ADR cadence (SPEC §7 option D). **DECISIONS.md ledger head stays at ADR-0034 through phase 11.** Next available number stays ADR-0035 for a future phase. | SPEC §7 (all 4 options DEFER per recommended posture). |
| 38 | Split-gate verdict | Single-phase, no split. PLAN materializes **8 tasks / ~1500-1620 LoC projected** (production ~600, tests ~720, fixture/harness/doc ~300). Task count comfortably under `BOOTSTRAP_PROMPT.md` §6.1 ~25-task gate; LoC at/marginally-over the soft ~1500-LoC gate (same posture as phase-10's ~1525). Accept the projection per the soft gate. **Release valve (if state-3 drifts past ~1600):** SPEC §6.1's preferred trim is D8.1 option (b) — single-probe `Driver::Http2` abort assertion + pass-through covered by the backstop only — saving ~90 LoC while still closing 09 M2 (the H2 abort path stays bilaterally exercised). Do NOT nest-split per parent-08 SPEC §6.1 alternative (vi). | SPEC §6.1 + §8 split-gate signpost. |
| 39 | Subagent-driven execution | State-3 dispatches each task to a fresh subagent per `feedback_execution_style` auto-memory + the 06.x / 07.x / 08.x / 09 / 10 cadence. Two-stage review per task (spec-compliance + code-quality). | Auto-memory + project precedent. |
| 40 | PROGRESS.md skeleton + Task 1 preamble | Land alongside PLAN.md at this state-2 commit per the 06.2 / 06.3 / 07.x / 08.x / 09 / 10 cadence. The Task 1 preamble records the §6.2 empirical-verification findings + the 7 PLAN-write SPEC corrections + the architecture-decision lock-ins grouped summary + the carryforward dispositions. | Project precedent. |
| 41 | Cargo.lock cadence | Phase-04.1 REVIEW M5/M9 ratification ADR carries forward unchanged. Zero new top-level Cargo deps; zero new workspace path-deps. `Cargo.lock` diff at the phase-11 reviewed range is expected to be EMPTY. | SPEC §6.9. |
| 42 | `#![forbid(unsafe_code)]` posture | The new `fault.rs` module inherits from the `crates/envoy-filter/src/lib.rs` crate root attribute; the D6 helper inherits from `crates/envoy-http2/src/lib.rs`. No `unsafe`; no per-module override. | SPEC §5.2; D-3.8. |

---

## 3. LoC drift posture / split-gate evaluation

Per SPEC §6.1, the SPEC-time projection was ~10-12 tasks / ~1350-1500 LoC. The PLAN
materializes **8 tasks** (the deliverable co-locations recommended by SPEC §6.1 collapse the
12-deliverable surface into 8 tasks):

| Task | Production LoC | Test LoC | Fixture/harness/doc LoC | Total |
|---|---|---|---|---|
| 1 — D1 schema + D2 validator + D3 eval helper (co-located) | ~170 | ~180 | ~5 | ~355 |
| 2 — D4 FaultFilter runtime + D7 stats + D7.1 1 contract row | ~120 | ~190 | ~15 | ~325 |
| 3 — D5 HttpFilterInstance::Fault variant + dispatch | ~30 | ~60 | 0 | ~90 |
| 4 — D6 H2 decorate helper + 2 wirings + 2 tests (closes 09 M2) | ~65 | ~90 | ~10 | ~165 |
| 5 — D8.1 fixture 0018 + Driver::Http2ProbeList + Docker wrapper | ~90 | ~25 | ~130 | ~245 |
| 6 — D8.2 fuzz seed + SUCCESS-array | ~5 | ~5 | ~45 | ~55 |
| 7 — D8.3 in-process backstop (H1; with header assertion) | 0 | ~195 | 0 | ~195 |
| 8 — state-4 verification + STATE advance | 0 | 0 | ~90 | ~90 |
| **TOTAL** | **~480** | **~745** | **~295** | **~1520** |

**Task count: 8.** Comfortably under §6.1's ~25-task gate. **LoC: ~1520.** Marginally **at**
§6.1's ~1500-LoC soft gate (+1.3%) — the same posture as phase-10's ~1525 single-phase
landing. Test-heavy concentration (~49% of LoC) is consistent with the mature 06.x → 10
cadence.

**Decision: single-phase; no split.** The ~20-LoC "overrun" (1.3%) is well within PLAN-time
projection uncertainty + the §6.1 gate is a SOFT gate per `BOOTSTRAP_PROMPT.md` §6.1 prose
("~1500 lines of code of net change" — approximate, not hard). Accept up to ~+15% empirical
drift at state-3. **Release valve if a single task's empirical drift pushes the phase past
~1600 LoC:** D8.1 option (b) trim per lock-in #38 (single-probe `Driver::Http2` + backstop
pass-through), recorded in PROGRESS — NOT a phase-level nest-split per parent-08 SPEC §6.1
alternative (vi).

---

## 4. Task summary

| # | Title | Files touched | Carryforwards / notes |
|---|---|---|---|
| 1 | D1 envoy-config schema + D2 validator + D3 eval helper (co-located) | `crates/envoy-config/src/lib.rs` (3 new `ConfigError` variants + re-exports for `FaultConfig`, `FaultAbort`, `FractionalPercent`, `DenominatorType`); `crates/envoy-config/src/bootstrap.rs` (new `HttpFilterTypedConfig::Fault` variant + 4 new schema items + `default_denominator` + `DenominatorType::value` + `FractionalPercent::selects_deterministic` + `validate_http_filters` Fault arm + `validate_fault_config` + unit tests) | None engaged. **SPEC correction #1: no `error.rs` edit.** |
| 2 | D4 FaultFilter runtime + D7 stats wiring + D7.1 1 BEHAVIOR_CONTRACT row | `crates/envoy-filter/src/fault.rs` (NEW: `FaultFilter` struct + `build_from_config` + `decode_headers` + `encode_headers` + `header_gate_matches` + `FAULT_ABORT_BODY` + unit tests); `crates/envoy-filter/src/lib.rs` (`pub mod fault;` + `pub use fault::FaultFilter;`); `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (1 new "Stat-name mapping" row under new `**11 entries (Fault filter):**` header) | None engaged. |
| 3 | D5 HttpFilterInstance::Fault variant + dispatch | `crates/envoy-filter/src/instance.rs` (`use crate::fault::FaultFilter;` + new `Fault(FaultFilter)` variant + build/decode/encode dispatch arms) | None engaged. **No `error.rs` edit per SPEC correction #1.** |
| 4 | D6 H2 `decorate_filter_synth_response_h2` + 2 writer-arm wirings + 2 tests | `crates/envoy-http2/src/response.rs` (NEW helper + 2 unit tests); `crates/envoy-http2/src/hcm.rs` (2 wirings: decode-side `SynthFromDecode` arm `:373` + encode-side `StopAndSend` arm `:436`) | **CLOSES 09 REVIEW M2 (implementation arm)** at the named close site. After this task, the 09 → 10 → 11 M2 chain ENDS. |
| 5 | D8.1 fixture 0018 + `Driver::Http2ProbeList` harness + Docker wrapper | `tests/fixtures/0018-http-filter-fault/` (NEW: `envoy.yaml`, `envoy-rust.yaml`, `expectations.yaml`, `README.md`); `tests/differential/src/lib.rs` (new `Driver::Http2ProbeList` variant + match arm + a round-trip parse test); `tests/differential/tests/http_filter_fault.rs` (NEW Docker-gated wrapper) | None engaged. |
| 6 | D8.2 fuzz corpus seed | `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_fault_filter.yaml` (NEW); `crates/envoy-config/fuzz/.gitignore` (1-line allow-list extension); `crates/envoy-config/src/bootstrap.rs` (extend `fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS-array — same-commit edit per the 09/10 lesson) | None engaged. |
| 7 | D8.3 in-process backstop (H1; with per-probe header assertion) | `crates/envoy-bin/tests/http_filter_fault.rs` (NEW: `tokio::process::Command + .kill_on_drop(true) + Stdio` discipline; 4-probe `[503,200,503,200]` sequence + body + 503-probe header presence per §6.4 option (a)) | Heeds 10 REVIEW M1 lesson proactively (SPEC §6.4). |
| 8 | state-4 phase-done verification + STATE advance to state-5-next | `docs/envoy-rust/phases/11-http-filter-fault/PROGRESS.md` (state-4 evidence anchor: 18-fixture green simultaneously + per-gate quoted output + CI run URL + HEAD SHA + completion timestamp + **h2spec ≥95% re-confirmation** per SPEC §6.8); `docs/envoy-rust/STATE.md` (Active phase status → state 4-complete / state-5-next; Next expected skill → `superpowers:requesting-code-review`) | Materializes state-4 evidence per `BOOTSTRAP_PROMPT.md` §7.5 (a)-(e). |

**Dependency chain:**
- Task 1 has no in-phase deps.
- Task 2 depends on Task 1 (`FaultConfig` + `FractionalPercent::selects_deterministic`).
- Task 3 depends on Task 2 (consumes `FaultFilter::build_from_config` + `decode_headers`).
- Task 4 (D6 H2 helper) is **independent of Tasks 1-3** (it decorates the H2 HCM writer
  path; it does not reference the fault filter). It can be dispatched any time after Task 0,
  but is sequenced at Task 4 so the fixture (Task 5) lands after both the filter wiring
  (Task 3) and the H2 decoration (Task 4) are in place.
- Task 5 depends on Tasks 1-4 (the full pipeline must compile + the H2 decoration must work
  before the fixture exercises the decorated 503 over H2).
- Task 6 depends on Task 1 (config parsing must accept the fault schema for the fuzz seed).
- Task 7 depends on Tasks 1-3 (the in-process backstop boots envoy-bin against the full
  pipeline; the H1 path needs the filter wired but not the H2 helper).
- Task 8 depends on all prior tasks (verification anchor).

**Task ordering for state-3 dispatch:** 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8. Tasks 4/6/7 are
pairwise independent of each other given Tasks 1-3; the established cadence prefers
sequential single-task dispatch (each subagent reads the prior task's PROGRESS append for
context).

---

## 5. Conventions

**TDD shape per task:** Write the failing tests FIRST (one or more `- [ ]` steps); run them
and verify they fail; implement; run again and verify they pass; run the 5 stable-toolchain
gates (`cargo fmt --all -- --check` + `cargo clippy --workspace --all-targets --all-features
-- -D warnings` + `cargo build --workspace --all-targets` + `cargo test --workspace` +
`cargo deny check`); append to PROGRESS.md; commit.

**Commit message format per task:** `phase 11: task NN — <short description>` matching the
06.x / 07.x / 08.x / 09 / 10 precedent. Final state-6 commit per SPEC §9 +
`BOOTSTRAP_PROMPT.md` §5.3: `phase 11: envoy.filters.http.fault (abort) + fixture 0018 + 09
REVIEW M2 impl close [H2 HCM decorate_filter_synth_response_h2]`. No `[ADR-NNNN]` bracket is
projected (no ADR lands per lock-in #37); if any ADR unexpectedly lands in state-3, the
bracket is appended per `BOOTSTRAP_PROMPT.md` §5.3.

**PROGRESS cadence per task:** Append a new `### Task N — <name>` subsection with: work
summary (3-5 paragraphs); tests landed (bulleted list); per-task deviations from PLAN
(numbered list, often empty); LoC delta (table); 5-gate test-bucket attestation (5
subsections, one per gate, each with PASS/FAIL + exit code + verbatim output where the gate
produces visible diff vs prior task).

**Per-task fmt discipline:** Every task closes by running `cargo fmt --all -- --check`. If
drift is observed, run `cargo fmt --all` first (mutating step) and re-stage before commit.
Carries the 06.1 R-9 discipline forward (SPEC §6.7).

**Error-handling convention:** All new error variants are `thiserror::Error` derives on the
existing `ConfigError` enum (envoy-config) and `FilterError` enum (envoy-filter). `anyhow` is
forbidden in library crates per D-3.2.

**Precedent-verification discipline (SPEC §6.11):** Subagents claiming "same pattern as
previous phase" verify the precedent shape via direct code-spot-check (`Read` tool) before
the claim lands in PROGRESS — never rely on a prior phase's PROGRESS narrative claim.

---

## 6. State-2 commit (this commit)

This commit is **docs-only** and touches 4 files (NO ADR; NO SPEC edit — all 3 §6.2
projections matched, SPEC correction #7):

- **CREATE** `docs/envoy-rust/phases/11-http-filter-fault/PLAN.md` (this file).
- **CREATE** `docs/envoy-rust/phases/11-http-filter-fault/PROGRESS.md` (skeleton + Task 1 preamble).
- **MODIFY** `docs/envoy-rust/ROADMAP.md` — flip row `11` `status: planned` → `status: in-progress`. Earlier rows unchanged.
- **MODIFY** `docs/envoy-rust/STATE.md` — Active phase status; Next expected skill; Last commit; Last updated; new "Phase-11 state-2 PLAN-write" subsection in Notes.

**Commit message:**

```
phase 11: state-2 standalone PLAN.md
```

Mirrors `55abc61` (phase-10 state-2 PLAN-write `phase 10: state-2 standalone PLAN.md
[ADR-0034]`) shape precedent — minus the `[ADR-NNNN]` bracket, since phase 11's §6.2
verification surfaced no revision (SPEC correction #7; ledger stays at ADR-0034).

No production code changes; no test changes; no fixture changes; no Cargo.toml / Cargo.lock
changes; no DECISIONS.md change (ledger head stays at ADR-0034); no BEHAVIOR_CONTRACT.md
change (the 1 stat-name mapping row lands at Task 2 commit per the 06.x → 10 doctrine —
contract extensions land at empirical-engagement task time, NOT at PLAN-write time); no
SPEC.md change (all 3 §6.2 projections matched); no ENVOY_TARGET.md / rust-toolchain.toml
change (D-3.7 / D-3.9 unchanged).

---

## Task 1: D1 envoy-config schema + D2 validator + D3 eval helper (co-located)

**Goal.** Extend `crates/envoy-config` with the fault schema (4 new schema items + 1 new
`HttpFilterTypedConfig` variant) + the deterministic-percentage eval helper + the validator
dispatch arm + sub-validator (3 new `ConfigError` variants). This is the parse-time gate
that catches misconfigured fault bootstraps — including the **fractional-percentage
rejection** that keeps phase 11 inside the differential contract (SPEC §5.6) — before they
reach the filter runtime.

**Files:**
- Modify: `crates/envoy-config/src/lib.rs` (add 3 `ConfigError` variants + re-exports for `FaultConfig`, `FaultAbort`, `FractionalPercent`, `DenominatorType`).
- Modify: `crates/envoy-config/src/bootstrap.rs` (add `HttpFilterTypedConfig::Fault` variant + 4 schema items + `default_denominator` + `DenominatorType::value` + `FractionalPercent::selects_deterministic` + extend `validate_http_filters` + `validate_fault_config` + unit tests).

### Steps

- [ ] **Step 1: Write the failing schema-deserialization + validator unit tests.**

Add to `crates/envoy-config/src/bootstrap.rs` at the bottom of the existing `#[cfg(test)] mod
tests { ... }` block (after the `rbac_tests` submodule from phase 10):

```rust
mod fault_tests {
    use super::*;
    use crate::{
        ConfigError, DenominatorType, FaultAbort, FaultConfig, FractionalPercent,
        HttpFilter, HttpFilterTypedConfig,
    };

    fn router_filter() -> HttpFilter {
        HttpFilter {
            name: "envoy.filters.http.router".to_string(),
            typed_config: HttpFilterTypedConfig::Router(crate::RouterConfig {}),
        }
    }

    // ── schema deserialization ───────────────────────────────────────────────

    #[test]
    fn fault_config_parses_full_abort_with_header_gate() {
        let yaml = r#"
abort:
  http_status: 503
  percentage: { numerator: 100, denominator: HUNDRED }
headers:
- name: x-fault
  string_match: { exact: abort }
"#;
        let cfg: FaultConfig = serde_yaml::from_str(yaml).expect("parses");
        assert_eq!(cfg.abort.http_status, 503);
        assert_eq!(cfg.abort.percentage.numerator, 100);
        assert_eq!(cfg.abort.percentage.denominator, DenominatorType::Hundred);
        assert_eq!(cfg.headers.len(), 1);
    }

    #[test]
    fn fault_config_denominator_defaults_to_hundred() {
        let yaml = r#"
abort:
  http_status: 503
  percentage: { numerator: 0 }
"#;
        let cfg: FaultConfig = serde_yaml::from_str(yaml).expect("parses");
        assert_eq!(cfg.abort.percentage.denominator, DenominatorType::Hundred);
        assert!(cfg.headers.is_empty());
    }

    #[test]
    fn fault_config_rejects_unknown_field() {
        let yaml = r#"
abort:
  http_status: 503
  percentage: { numerator: 100 }
delay: { fixed_delay: 5s }
"#;
        let err = serde_yaml::from_str::<FaultConfig>(yaml).unwrap_err();
        assert!(format!("{err}").contains("delay"), "err: {err}");
    }

    #[test]
    fn denominator_type_value_maps_correctly() {
        assert_eq!(DenominatorType::Hundred.value(), 100);
        assert_eq!(DenominatorType::TenThousand.value(), 10_000);
        assert_eq!(DenominatorType::Million.value(), 1_000_000);
    }

    #[test]
    fn fractional_percent_selects_deterministic() {
        let p100 = FractionalPercent { numerator: 100, denominator: DenominatorType::Hundred };
        let p0 = FractionalPercent { numerator: 0, denominator: DenominatorType::Hundred };
        let p_full_million =
            FractionalPercent { numerator: 1_000_000, denominator: DenominatorType::Million };
        assert!(p100.selects_deterministic());
        assert!(!p0.selects_deterministic());
        assert!(p_full_million.selects_deterministic());
    }

    // ── validator: positive ──────────────────────────────────────────────────

    #[test]
    fn validate_accepts_fault_abort_100_percent() {
        let fault = HttpFilter {
            name: "envoy.filters.http.fault".to_string(),
            typed_config: HttpFilterTypedConfig::Fault(FaultConfig {
                abort: FaultAbort {
                    http_status: 503,
                    percentage: FractionalPercent {
                        numerator: 100,
                        denominator: DenominatorType::Hundred,
                    },
                },
                headers: vec![],
            }),
        };
        assert!(validate_http_filters(&[fault, router_filter()], "ingress").is_ok());
    }

    #[test]
    fn validate_accepts_fault_abort_0_percent() {
        let fault = HttpFilter {
            name: "envoy.filters.http.fault".to_string(),
            typed_config: HttpFilterTypedConfig::Fault(FaultConfig {
                abort: FaultAbort {
                    http_status: 503,
                    percentage: FractionalPercent {
                        numerator: 0,
                        denominator: DenominatorType::Hundred,
                    },
                },
                headers: vec![],
            }),
        };
        assert!(validate_http_filters(&[fault, router_filter()], "ingress").is_ok());
    }

    // ── validator: negative ──────────────────────────────────────────────────

    #[test]
    fn validate_rejects_invalid_abort_status() {
        let fault = HttpFilter {
            name: "envoy.filters.http.fault".to_string(),
            typed_config: HttpFilterTypedConfig::Fault(FaultConfig {
                abort: FaultAbort {
                    http_status: 999,
                    percentage: FractionalPercent {
                        numerator: 100,
                        denominator: DenominatorType::Hundred,
                    },
                },
                headers: vec![],
            }),
        };
        let err = validate_http_filters(&[fault, router_filter()], "ingress").unwrap_err();
        assert!(
            matches!(err, ConfigError::InvalidFaultAbortStatus { status: 999, .. }),
            "err: {err:?}"
        );
    }

    #[test]
    fn validate_rejects_percentage_out_of_range() {
        let fault = HttpFilter {
            name: "envoy.filters.http.fault".to_string(),
            typed_config: HttpFilterTypedConfig::Fault(FaultConfig {
                abort: FaultAbort {
                    http_status: 503,
                    percentage: FractionalPercent {
                        numerator: 200,
                        denominator: DenominatorType::Hundred,
                    },
                },
                headers: vec![],
            }),
        };
        let err = validate_http_filters(&[fault, router_filter()], "ingress").unwrap_err();
        assert!(
            matches!(err, ConfigError::FaultPercentageOutOfRange { numerator: 200, denominator: 100, .. }),
            "err: {err:?}"
        );
    }

    #[test]
    fn validate_rejects_fractional_percentage() {
        let fault = HttpFilter {
            name: "envoy.filters.http.fault".to_string(),
            typed_config: HttpFilterTypedConfig::Fault(FaultConfig {
                abort: FaultAbort {
                    http_status: 503,
                    percentage: FractionalPercent {
                        numerator: 50,
                        denominator: DenominatorType::Hundred,
                    },
                },
                headers: vec![],
            }),
        };
        let err = validate_http_filters(&[fault, router_filter()], "ingress").unwrap_err();
        assert!(
            matches!(err, ConfigError::UnsupportedFractionalFaultPercentage { numerator: 50, denominator: 100, .. }),
            "err: {err:?}"
        );
    }

    #[test]
    fn validate_rejects_name_typed_config_mismatch() {
        let fault = HttpFilter {
            name: "envoy.filters.http.WRONG".to_string(),
            typed_config: HttpFilterTypedConfig::Fault(FaultConfig {
                abort: FaultAbort {
                    http_status: 503,
                    percentage: FractionalPercent {
                        numerator: 100,
                        denominator: DenominatorType::Hundred,
                    },
                },
                headers: vec![],
            }),
        };
        let err = validate_http_filters(&[fault, router_filter()], "ingress").unwrap_err();
        assert!(matches!(err, ConfigError::UnsupportedHttpFilter { .. }), "err: {err:?}");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail (compile error — types not defined yet).**

Run: `cargo test -p envoy-config fault_tests 2>&1 | tail -20`
Expected: FAIL — compile errors (`cannot find type FaultConfig`, `no variant Fault`, `no variant InvalidFaultAbortStatus`, etc.).

- [ ] **Step 3: Add the 3 new `ConfigError` variants to `crates/envoy-config/src/lib.rs`.**

In the `ConfigError` enum (alongside the existing RBAC / LocalRateLimit variants), add:

```rust
    /// Phase 11: fault filter `abort.http_status` outside the syntactic HTTP
    /// status band (100..=599).
    #[error("listener {listener:?}: fault abort http_status {status} is not a valid HTTP status code (must be 100-599)")]
    InvalidFaultAbortStatus { listener: String, status: u16 },

    /// Phase 11: fault filter `abort.percentage.numerator` exceeds its denominator.
    #[error("listener {listener:?}: fault abort percentage numerator {numerator} exceeds denominator {denominator}")]
    FaultPercentageOutOfRange {
        listener: String,
        numerator: u32,
        denominator: u32,
    },

    /// Phase 11: fault filter fractional percentage (0 < numerator < denominator)
    /// is not supported — phase-11 scope is deterministic 0%/100% only (a
    /// fractional per-request abort is non-differential-testable per the
    /// differential contract; SPEC §4 + §5.6).
    #[error("listener {listener:?}: fault abort fractional percentage {numerator}/{denominator} is unsupported (deterministic 0% or 100% only)")]
    UnsupportedFractionalFaultPercentage {
        listener: String,
        numerator: u32,
        denominator: u32,
    },
```

- [ ] **Step 4: Add the schema items + helpers to `crates/envoy-config/src/bootstrap.rs`.**

Add (near the existing filter schema items; `FractionalPercent` + `DenominatorType` are
general shared types — place them where they read cleanly, e.g. just before the
`HttpFilterTypedConfig` enum):

```rust
/// `envoy.extensions.filters.http.fault.v3.HTTPFault` config (abort path).
/// Phase 11 supports the abort block + optional header-match gate; delay,
/// response_rate_limit, max_active_faults, and downstream-controlled faults
/// all defer per phase-11 SPEC §4 (rejected by `deny_unknown_fields`).
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FaultConfig {
    pub abort: FaultAbort,
    #[serde(default)]
    pub headers: Vec<HeaderMatcher>,
}

/// `envoy.extensions.filters.http.fault.v3.FaultAbort` (abort block).
/// `grpc_status` + `header_abort` defer per SPEC §4.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FaultAbort {
    pub http_status: u16,
    pub percentage: FractionalPercent,
}

/// `envoy.type.v3.FractionalPercent`. A general shared config type (the first
/// percent type in envoy-config); authored to be reusable by future filters
/// that take a fractional percentage.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FractionalPercent {
    pub numerator: u32,
    #[serde(default = "default_denominator")]
    pub denominator: DenominatorType,
}

impl FractionalPercent {
    /// Phase-11 deterministic select: `true` iff 100% (`numerator ==
    /// denominator.value()`), `false` iff 0% (`numerator == 0`). The validator
    /// (`validate_fault_config`) guarantees `numerator ∈ {0, denominator.value()}`,
    /// so this is a pure boolean — no per-request randomness, no PRNG. Fractional
    /// percentage defers per SPEC §4 + §5.6.
    pub fn selects_deterministic(&self) -> bool {
        self.numerator == self.denominator.value()
    }
}

/// `envoy.type.v3.FractionalPercent.DenominatorType`.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DenominatorType {
    Hundred,
    TenThousand,
    Million,
}

impl DenominatorType {
    /// The integer denominator this variant represents.
    pub fn value(self) -> u32 {
        match self {
            DenominatorType::Hundred => 100,
            DenominatorType::TenThousand => 10_000,
            DenominatorType::Million => 1_000_000,
        }
    }
}

fn default_denominator() -> DenominatorType {
    DenominatorType::Hundred
}
```

Then add the `Fault` variant to the `HttpFilterTypedConfig` enum (after the `Rbac` variant
at `bootstrap.rs:459`):

```rust
    #[serde(rename = "type.googleapis.com/envoy.extensions.filters.http.fault.v3.HTTPFault")]
    Fault(FaultConfig),
```

- [ ] **Step 5: Add the `validate_fault_config` sub-validator + the dispatch arm to `bootstrap.rs`.**

Add the sub-validator (alongside `validate_rbac_config`):

```rust
/// Phase 11: validate the fault filter config. Rejects invalid abort status
/// codes, out-of-range percentages, and (per phase-11 deterministic-only scope)
/// fractional percentages. The optional `headers` gate reuses the 04.2
/// `HeaderMatcher` (no parse-time validation beyond deserialize).
fn validate_fault_config(
    cfg: &crate::FaultConfig,
    listener_name: &str,
) -> Result<(), crate::ConfigError> {
    if !(100..=599).contains(&cfg.abort.http_status) {
        return Err(crate::ConfigError::InvalidFaultAbortStatus {
            listener: listener_name.to_string(),
            status: cfg.abort.http_status,
        });
    }
    let denominator = cfg.abort.percentage.denominator.value();
    let numerator = cfg.abort.percentage.numerator;
    // Out-of-range check FIRST: numerator > denominator is an operator typo,
    // reported distinctly from the fractional rejection.
    if numerator > denominator {
        return Err(crate::ConfigError::FaultPercentageOutOfRange {
            listener: listener_name.to_string(),
            numerator,
            denominator,
        });
    }
    // Deterministic-only: numerator must be 0 (0%) or == denominator (100%).
    if numerator != 0 && numerator != denominator {
        return Err(crate::ConfigError::UnsupportedFractionalFaultPercentage {
            listener: listener_name.to_string(),
            numerator,
            denominator,
        });
    }
    Ok(())
}
```

Then add the dispatch arm to `validate_http_filters` (after the `Rbac` arm closing at
`bootstrap.rs:1911`):

```rust
            crate::HttpFilterTypedConfig::Fault(cfg) => {
                if f.name != "envoy.filters.http.fault" {
                    return Err(crate::ConfigError::UnsupportedHttpFilter {
                        name: f.name.clone(),
                    });
                }
                validate_fault_config(cfg, listener_name)?;
            }
```

- [ ] **Step 6: Add the 4 re-exports to `crates/envoy-config/src/lib.rs`.**

In the `pub use bootstrap::{...}` re-export block (alongside the existing filter-config
re-exports), add `DenominatorType, FaultAbort, FaultConfig, FractionalPercent` (preserve
alphabetical order if the existing block is alphabetized).

- [ ] **Step 7: Run the tests to verify they pass.**

Run: `cargo test -p envoy-config fault_tests 2>&1 | tail -20`
Expected: PASS — all `fault_tests` green.

- [ ] **Step 8: Run the 5 stable-toolchain gates.**

Run:
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-targets
cargo test --workspace
cargo deny check
```
Expected: all clean. (If `cargo fmt --all -- --check` reports drift, run `cargo fmt --all` first, then re-stage.)

- [ ] **Step 9: Append the Task 1 PROGRESS subsection + commit.**

Append `### Task 1 — D1 envoy-config schema + D2 validator + D3 eval helper` to PROGRESS.md
(work summary + tests landed + deviations + LoC delta + 5-gate attestation), then:
```bash
git add crates/envoy-config/src/lib.rs crates/envoy-config/src/bootstrap.rs docs/envoy-rust/phases/11-http-filter-fault/PROGRESS.md
git commit -m "phase 11: task 1 — D1 envoy-config schema + D2 validator + D3 eval helper"
```

---

## Task 2: D4 FaultFilter runtime + D7 stats wiring + D7.1 BEHAVIOR_CONTRACT row

**Goal.** Land the hand-rolled `FaultFilter` runtime at `crates/envoy-filter/src/fault.rs`:
the gate-then-select-then-abort decode path + the `aborts_injected` counter wiring + the 1
BEHAVIOR_CONTRACT "Stat-name mapping" row.

**Files:**
- Create: `crates/envoy-filter/src/fault.rs`.
- Modify: `crates/envoy-filter/src/lib.rs` (`pub mod fault;` + `pub use fault::FaultFilter;`).
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (1 new "Stat-name mapping" row).

### Steps

- [ ] **Step 1: Write the failing `FaultFilter` unit tests.**

Create `crates/envoy-filter/src/fault.rs` with a `#[cfg(test)] mod tests` block FIRST (the
struct/impl follow in Step 3). Tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use envoy_config::{DenominatorType, FaultAbort, FaultConfig, FractionalPercent};
    use envoy_stats::StatsRegistry;
    use std::sync::Arc;

    fn cfg(numerator: u32, headers: Vec<envoy_config::HeaderMatcher>) -> FaultConfig {
        FaultConfig {
            abort: FaultAbort {
                http_status: 503,
                percentage: FractionalPercent { numerator, denominator: DenominatorType::Hundred },
            },
            headers,
        }
    }

    fn header_matcher_exact(name: &str, value: &str) -> envoy_config::HeaderMatcher {
        // Parse a HeaderMatcher from YAML so the test exercises the real 04.2 type.
        let yaml = format!("name: {name}\nstring_match: {{ exact: {value} }}\n");
        serde_yaml::from_str(&yaml).expect("HeaderMatcher parses")
    }

    fn req(headers: Vec<(String, String)>) -> FilterRequest {
        FilterRequest { headers, ..Default::default() }
    }

    #[test]
    fn abort_100_percent_no_gate_aborts_every_request() {
        let registry = Arc::new(StatsRegistry::new());
        let mut f = FaultFilter::build_from_config(&cfg(100, vec![]), &registry, "ingress_http").unwrap();
        let mut r = req(vec![]);
        match f.decode_headers(&mut r) {
            Decision::StopAndSend(resp) => {
                assert_eq!(resp.status, 503);
                assert_eq!(resp.body.as_ref(), b"fault filter abort");
                assert_eq!(resp.body.len(), 18);
                assert!(resp.headers.is_empty(), "filter adds no headers; HCM decorates");
            }
            Decision::Continue => panic!("expected abort"),
        }
    }

    #[test]
    fn abort_0_percent_never_aborts() {
        let registry = Arc::new(StatsRegistry::new());
        let mut f = FaultFilter::build_from_config(&cfg(0, vec![]), &registry, "ingress_http").unwrap();
        let mut r = req(vec![]);
        assert!(matches!(f.decode_headers(&mut r), Decision::Continue));
    }

    #[test]
    fn header_gate_match_aborts_miss_passes() {
        let registry = Arc::new(StatsRegistry::new());
        let gate = vec![header_matcher_exact("x-fault", "abort")];
        let mut f = FaultFilter::build_from_config(&cfg(100, gate), &registry, "ingress_http").unwrap();

        // Gate matches → abort.
        let mut r_match = req(vec![("x-fault".to_string(), "abort".to_string())]);
        assert!(matches!(f.decode_headers(&mut r_match), Decision::StopAndSend(_)));

        // Gate misses (no header) → pass.
        let mut r_miss = req(vec![]);
        assert!(matches!(f.decode_headers(&mut r_miss), Decision::Continue));
    }

    #[test]
    fn aborts_injected_counter_increments_once_per_abort_only() {
        let registry = Arc::new(StatsRegistry::new());
        let mut f = FaultFilter::build_from_config(&cfg(100, vec![]), &registry, "ingress_http").unwrap();
        let _ = f.decode_headers(&mut req(vec![]));
        let _ = f.decode_headers(&mut req(vec![]));
        let counter = registry
            .counter_value("http.ingress_http.fault.aborts_injected")
            .expect("counter registered");
        assert_eq!(counter, 2, "one increment per abort, never on pass");
    }

    #[test]
    fn encode_headers_is_noop() {
        let registry = Arc::new(StatsRegistry::new());
        let mut f = FaultFilter::build_from_config(&cfg(100, vec![]), &registry, "ingress_http").unwrap();
        let mut resp = FilterResponse {
            status: 200,
            reason: None,
            headers: vec![],
            body: bytes::Bytes::new(),
        };
        assert!(matches!(f.encode_headers(&mut resp), Decision::Continue));
    }
}
```

> **Implementer note:** the test uses `registry.counter_value(name)` + `StatsRegistry::new()`
> + `FilterRequest { .., ..Default::default() }`. Verify these exact API shapes via direct
> spot-check of `crates/envoy-stats/src/lib.rs` and `crates/envoy-filter/src/types.rs` before
> running — if the stats registry exposes a different read accessor (e.g. `value(name)` or a
> snapshot map) or `FilterRequest` lacks a `Default`, adjust the test helpers to the actual
> API (the phase-09 + phase-10 filter tests are the precedent for the exact accessor name).

- [ ] **Step 2: Run the tests to verify they fail.**

Run: `cargo test -p envoy-filter fault 2>&1 | tail -20`
Expected: FAIL — `cannot find FaultFilter` / module not declared.

- [ ] **Step 3: Implement `FaultFilter` at the top of `crates/envoy-filter/src/fault.rs`.**

```rust
//! The `envoy.filters.http.fault` runtime filter — abort path (phase 11).
//!
//! Decode-side filter: on a request matching the optional header gate (AND
//! semantics over a `Vec<HeaderMatcher>`) when the deterministic percentage
//! selects (0%/100% only at phase-11 scope), short-circuits via
//! `Decision::StopAndSend` with the operator-configured HTTP status + the
//! source-hardcoded abort body. The standard response headers are decorated by
//! the HCM filter-synth decoration helpers (H1: `decorate_filter_synth_response`;
//! H2: `decorate_filter_synth_response_h2`, phase-11 D6).

use std::sync::Arc;

use bytes::Bytes;
use envoy_stats::{Counter, StatsRegistry};

use crate::error::FilterError;
use crate::pipeline::Decision;
use crate::types::{FilterRequest, FilterResponse};

/// Upstream Envoy v1.33's source-hardcoded fault-abort body (18 bytes;
/// §6.2-verified at phase-11 state-2 PLAN-write against `envoyproxy/envoy:v1.33.0`).
const FAULT_ABORT_BODY: &[u8] = b"fault filter abort";

/// The `envoy.filters.http.fault` runtime filter (abort path).
#[derive(Debug, Clone)]
pub struct FaultFilter {
    abort_status: u16,
    /// `true` iff the percentage is 100% (per `FractionalPercent::selects_deterministic`);
    /// computed once at build time — no per-request randomness.
    abort_selects: bool,
    /// Optional gate; empty ⇒ the fault applies to all requests.
    header_gate: Vec<envoy_config::HeaderMatcher>,
    aborts_injected: Arc<Counter>,
}

impl FaultFilter {
    /// Lower an `envoy_config::FaultConfig` into the runtime filter + register
    /// the abort counter under `http.{hcm_stat_prefix}.fault.aborts_injected`.
    pub(crate) fn build_from_config(
        cfg: &envoy_config::FaultConfig,
        registry: &Arc<StatsRegistry>,
        hcm_stat_prefix: &str,
    ) -> Result<Self, FilterError> {
        let aborts_injected =
            registry.register_counter(&format!("http.{hcm_stat_prefix}.fault.aborts_injected"));
        Ok(Self {
            abort_status: cfg.abort.http_status,
            abort_selects: cfg.abort.percentage.selects_deterministic(),
            header_gate: cfg.headers.clone(),
            aborts_injected,
        })
    }

    pub(crate) fn decode_headers(&mut self, req: &mut FilterRequest) -> Decision {
        if header_gate_matches(&self.header_gate, req) && self.abort_selects {
            self.aborts_injected.inc();
            return Decision::StopAndSend(FilterResponse {
                status: self.abort_status,
                reason: None,
                headers: vec![],
                body: Bytes::from_static(FAULT_ABORT_BODY),
            });
        }
        Decision::Continue
    }

    pub(crate) fn encode_headers(&mut self, _resp: &mut FilterResponse) -> Decision {
        // Decode-only filter at phase-11 scope (response-rate-limit defers).
        Decision::Continue
    }
}

/// All listed matchers must match (AND semantics) per upstream. An empty gate
/// returns `true` (`Iterator::all` over an empty slice) — no gate ⇒ all requests.
fn header_gate_matches(gate: &[envoy_config::HeaderMatcher], req: &FilterRequest) -> bool {
    gate.iter().all(|m| m.matches(&req.headers))
}
```

> **Implementer note:** verify `registry.register_counter(&str) -> Arc<Counter>` + `Counter::inc()`
> against `crates/envoy-stats/src/lib.rs` (the phase-09 `local_rate_limit.rs` + phase-10
> `rbac.rs` are the precedent — `RbacFilter` uses the same `register_counter` + `Arc<Counter>`
> shape per phase-10 PLAN lock-in #15). Adjust if the accessor differs.

- [ ] **Step 4: Declare the module + re-export in `crates/envoy-filter/src/lib.rs`.**

Add `pub mod fault;` between `pub mod error;` and `pub mod header_mutation;` (alphabetical).
Add `pub use fault::FaultFilter;` after `pub use error::FilterError;` (alphabetical).

- [ ] **Step 5: Run the tests to verify they pass.**

Run: `cargo test -p envoy-filter fault 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 6: Add the BEHAVIOR_CONTRACT.md "Stat-name mapping" row.**

In `docs/envoy-rust/BEHAVIOR_CONTRACT.md`, after the existing `**10 entries (RBAC filter):**`
rows (ending at the `rbac.denied` row), add:

```markdown
**11 entries (Fault filter):**

| Stat name | Equivalence | Rationale |
|---|---|---|
| `http.<hcm_stat_prefix>.fault.aborts_injected` | value-exact | Counter; one increment per request the filter aborts (the header gate matches AND the deterministic percentage selects at 100%). Both proxies emit one increment per aborted request at the abort decision site in `FaultFilter::decode_headers` (synchronously, before constructing the `Decision::StopAndSend(FilterResponse)` abort). Never increments on pass-through (gate miss OR 0% percentage). Upstream Envoy v1.33 emits the same name at the `http.<hcm_stat_prefix>.fault.*` namespace per the §6.2 empirical verification at phase-11 state-2 PLAN-write (`http.ingress_http.fault.aborts_injected: 4` after 4 aborts). The `<hcm_stat_prefix>` is sourced from the parent HCM's `stat_prefix` (the fault filter has no `stat_prefix` field of its own — same threading as RBAC at phase 10). |
```

- [ ] **Step 7: Run the 5 stable-toolchain gates** (as in Task 1 Step 8). Expected: all clean.

- [ ] **Step 8: Append the Task 2 PROGRESS subsection + commit.**

```bash
git add crates/envoy-filter/src/fault.rs crates/envoy-filter/src/lib.rs docs/envoy-rust/BEHAVIOR_CONTRACT.md docs/envoy-rust/phases/11-http-filter-fault/PROGRESS.md
git commit -m "phase 11: task 2 — D4 FaultFilter runtime + D7 stats + D7.1 contract row"
```

---

## Task 3: D5 HttpFilterInstance::Fault variant + dispatch

**Goal.** Wire the `FaultFilter` into the filter-chain framework via a fifth
`HttpFilterInstance` production variant + the build/decode/encode dispatch arms. Per SPEC
correction #1, **no `error.rs` edit** — the fault filter was never in a reject list; adding
the variant + the Task 1 validator arm is the entire "rejected → supported" move.

**Files:**
- Modify: `crates/envoy-filter/src/instance.rs` (`use` + variant + 3 dispatch arms).

### Steps

- [ ] **Step 1: Write the failing integration test.**

Add to the `#[cfg(test)] mod tests` block in `crates/envoy-filter/src/instance.rs` (or
`pipeline.rs` if that is where the build-from-config integration tests live — spot-check
which file holds the existing `build_from_config_with_single_router_succeeds`-style tests):

```rust
    #[test]
    fn build_from_config_wires_fault_then_router() {
        use envoy_config::{
            DenominatorType, FaultAbort, FaultConfig, FractionalPercent, HttpFilter,
            HttpFilterTypedConfig,
        };
        let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());
        let filters = vec![
            HttpFilter {
                name: "envoy.filters.http.fault".to_string(),
                typed_config: HttpFilterTypedConfig::Fault(FaultConfig {
                    abort: FaultAbort {
                        http_status: 503,
                        percentage: FractionalPercent {
                            numerator: 100,
                            denominator: DenominatorType::Hundred,
                        },
                    },
                    headers: vec![],
                }),
            },
            HttpFilter {
                name: "envoy.filters.http.router".to_string(),
                typed_config: HttpFilterTypedConfig::Router(envoy_config::RouterConfig {}),
            },
        ];
        let pipeline = crate::FilterPipeline::build_from_config(&filters, &registry, "ingress_http")
            .expect("builds");
        // Decode a request with no gate → 100% abort → StopAndSend(503).
        let mut req = crate::FilterRequest::default();
        match pipeline.clone().decode_headers(&mut req) {
            crate::Decision::StopAndSend(resp) => assert_eq!(resp.status, 503),
            crate::Decision::Continue => panic!("expected fault abort"),
        }
    }
```

> **Implementer note:** verify `FilterPipeline::decode_headers` is callable on a built
> pipeline + the exact mutability/clone shape (the phase-09/10 pipeline tests are the
> precedent). Adjust the call shape to match the existing integration-test style.

- [ ] **Step 2: Run to verify it fails.**

Run: `cargo test -p envoy-filter build_from_config_wires_fault 2>&1 | tail -20`
Expected: FAIL — `no variant Fault` / non-exhaustive match.

- [ ] **Step 3: Add the `use` + variant + dispatch arms to `instance.rs`.**

Add `use crate::fault::FaultFilter;` alongside the existing filter `use`s (`instance.rs:21-24`).

Add the variant between `Rbac(RbacFilter)` (`instance.rs:37`) and the `#[cfg(feature =
"test-util")]` block (`instance.rs:42`):

```rust
    Fault(FaultFilter),
```

Add the build dispatch arm (after the `Rbac` arm at `instance.rs:90-92`):

```rust
            envoy_config::HttpFilterTypedConfig::Fault(cfg) => Ok(HttpFilterInstance::Fault(
                FaultFilter::build_from_config(cfg, registry, hcm_stat_prefix)?,
            )),
```

Add the decode dispatch arm (after `HttpFilterInstance::Rbac(f) => f.decode_headers(req),` at
`instance.rs:101`):

```rust
            HttpFilterInstance::Fault(f) => f.decode_headers(req),
```

Add the encode dispatch arm (in the `encode_headers` match, after the `Rbac` arm — spot-check
the exact line; mirror the decode arm shape):

```rust
            HttpFilterInstance::Fault(f) => f.encode_headers(resp),
```

- [ ] **Step 4: Run to verify it passes.**

Run: `cargo test -p envoy-filter build_from_config_wires_fault 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 5: Run the 5 stable-toolchain gates.** Expected: all clean.

- [ ] **Step 6: Append the Task 3 PROGRESS subsection + commit.**

```bash
git add crates/envoy-filter/src/instance.rs docs/envoy-rust/phases/11-http-filter-fault/PROGRESS.md
git commit -m "phase 11: task 3 — D5 HttpFilterInstance::Fault variant + dispatch"
```

---

## Task 4: D6 H2 `decorate_filter_synth_response_h2` helper + 2 wirings + 2 tests (closes 09 REVIEW M2 implementation arm)

**Goal.** Land the H2 HCM filter-synth decoration helper symmetric to the H1
`decorate_filter_synth_response` (`crates/envoy-http1/src/hcm.rs:968`), and wire it into both
H2 synth writer-arm sites. **This is the phase's carryforward-closure centerpiece** — it
closes the 09 REVIEW M2 *implementation* arm (phase 10's D5 closed the documentation arm).
After this task, the 09 → 10 → 11 M2 chain ENDS. No new ADR is required (SPEC §2.3 — the
close shape is ordinary deliverable work).

**Read before writing (precedent-verification discipline, lock-in #35):** the H1 helper
`decorate_filter_synth_response` at `crates/envoy-http1/src/hcm.rs:968-1005` + its 2 unit
tests at `:1405-1452` + the phase-10 D5 amendment at `docs/envoy-rust/DECISIONS.md:699`.

**Files:**
- Modify: `crates/envoy-http2/src/response.rs` (new `decorate_filter_synth_response_h2` helper + 2 unit tests).
- Modify: `crates/envoy-http2/src/hcm.rs` (2 wirings at `:373` decode-side + `:436` encode-side).

### Steps

- [ ] **Step 1: Write the failing 2 unit tests in `crates/envoy-http2/src/response.rs`.**

Add to the `#[cfg(test)] mod tests` block (mirroring the H1 helper tests at
`crates/envoy-http1/src/hcm.rs:1405-1452`):

```rust
    #[test]
    fn decorate_h2_adds_standard_headers_when_filter_provides_none() {
        let mut resp = envoy_http1::Response {
            status: 503,
            reason: None,
            headers: Vec::new(),
            body: bytes::Bytes::from_static(b"fault filter abort"),
        };
        super::decorate_filter_synth_response_h2(&mut resp);
        let name = |n: &str| -> Option<&str> {
            resp.headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(n))
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(name("content-length"), Some("18"));
        assert_eq!(name("server"), Some("envoy-rust"));
        assert_eq!(name("content-type"), Some("text/plain"));
        let date = name("date").expect("date header added");
        assert!(!date.is_empty(), "date empty: {date:?}");
        // H2: NO connection header (H2-forbidden hop-by-hop).
        assert!(name("connection").is_none(), "connection must NOT be added on H2");
        // 4 standard headers; no more, no fewer (filter contributed 0).
        assert_eq!(resp.headers.len(), 4, "headers: {:?}", resp.headers);
    }

    #[test]
    fn decorate_h2_preserves_filter_headers_and_overwrites_content_length() {
        let mut resp = envoy_http1::Response {
            status: 503,
            reason: None,
            headers: vec![
                ("server".to_string(), "my-proxy".to_string()),
                ("content-length".to_string(), "10".to_string()),
                ("x-fault-policy".to_string(), "phase-11".to_string()),
            ],
            body: bytes::Bytes::from_static(b"fault filter abort"),
        };
        super::decorate_filter_synth_response_h2(&mut resp);
        let name = |n: &str| -> Option<String> {
            resp.headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(n))
                .map(|(_, v)| v.clone())
        };
        // Filter's server wins (only-if-missing for server).
        assert_eq!(name("server").as_deref(), Some("my-proxy"));
        // content-length always overwritten to body.len() = 18.
        assert_eq!(name("content-length").as_deref(), Some("18"));
        // date + content-type added (filter didn't provide).
        assert!(name("date").is_some());
        assert_eq!(name("content-type").as_deref(), Some("text/plain"));
        // Non-standard header preserved verbatim.
        assert_eq!(name("x-fault-policy").as_deref(), Some("phase-11"));
        // Still no connection.
        assert!(name("connection").is_none());
    }
```

> **Implementer note:** confirm the exact `envoy_http1::Response` struct path + field names
> (`status`/`reason`/`headers`/`body`) and the default `server` / `content-type` string
> constants envoy-http2 emits. If envoy-http2 re-exports or aliases the `Response` type
> differently, adjust the test's type path. The default strings MUST match what the H1 helper
> uses (`"envoy-rust"`, `"text/plain"`) — both proxies' fixed `server` string lives behind the
> BEHAVIOR_CONTRACT `server` allow-list row, so envoy-rust's H1 and H2 paths must agree.

- [ ] **Step 2: Run to verify the tests fail.**

Run: `cargo test -p envoy-http2 decorate_h2 2>&1 | tail -20`
Expected: FAIL — `cannot find function decorate_filter_synth_response_h2`.

- [ ] **Step 3: Implement the helper in `crates/envoy-http2/src/response.rs`.**

Add adjacent to `build_http_response` (sourcing the default constants from whatever
envoy-http2 already uses for non-synth response headers; reuse the envoy-http1 helpers via
the existing path-dep if envoy-http2 has no local equivalent):

```rust
/// Decorate a filter-synth H2 response with the standard response headers,
/// symmetric to H1's `decorate_filter_synth_response` (`crates/envoy-http1/src/hcm.rs:968`)
/// — minus `connection`, which is an H2-forbidden hop-by-hop header stripped by
/// `build_http_response` per `H2_FORBIDDEN_HOP_BY_HOP` (RFC 7540 §8.1.2.2).
///
/// Adds `content-length` always (overwritten from `resp.body.len()`); adds
/// `server` / `date` / `content-type` only-if-missing (a filter that sets its
/// own value wins). Closes the 09 REVIEW M2 implementation arm (phase 11 D6):
/// the H1 writer path has decorated filter-synth responses since 09 ADR-0033
/// Commit C; this brings the H2 writer path to parity.
pub(crate) fn decorate_filter_synth_response_h2(resp: &mut Response) {
    // content-length: always derived from body.len(); overwrite if present.
    let cl_value = resp.body.len().to_string();
    let mut cl_set = false;
    for (k, v) in resp.headers.iter_mut() {
        if k.eq_ignore_ascii_case("content-length") {
            *v = cl_value.clone();
            cl_set = true;
            break;
        }
    }
    if !cl_set {
        resp.headers.push(("content-length".to_string(), cl_value));
    }
    // server / date / content-type: add only-if-missing. NO connection (H2-forbidden).
    let standards: [(&str, String); 3] = [
        ("server", DEFAULT_SERVER_NAME.to_string()),
        ("date", now_imf_fixdate()),
        ("content-type", DEFAULT_CONTENT_TYPE.to_string()),
    ];
    for (name, value) in standards {
        if !resp.headers.iter().any(|(k, _)| k.eq_ignore_ascii_case(name)) {
            resp.headers.push((name.to_string(), value));
        }
    }
}
```

> **Implementer note:** `DEFAULT_SERVER_NAME`, `DEFAULT_CONTENT_TYPE`, and `now_imf_fixdate()`
> are the H1 helper's sources (`crates/envoy-http1/src/hcm.rs`). Source the identical values in
> envoy-http2 — either by reusing the envoy-http1 exports (path-dep already exists) or by
> reusing whatever envoy-http2 already uses to stamp these on non-synth responses. Do NOT
> introduce divergent string literals. If `Response` is `envoy_http1::Response` re-exported,
> the `resp: &mut Response` parameter type follows the crate's existing convention.

- [ ] **Step 4: Run to verify the tests pass.**

Run: `cargo test -p envoy-http2 decorate_h2 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 5: Wire the helper into both H2 HCM synth writer-arm sites in `crates/envoy-http2/src/hcm.rs`.**

**Decode-side** — the `H2RequestPath::SynthFromDecode(r)` arm at `hcm.rs:373`. Decorate `r`
before it is returned/sent. Change:

```rust
        H2RequestPath::SynthFromDecode(r) => {
            // 07.1 Task 7: decode-side filter short-circuit. Unreachable
            // under the Router-only 07.1 chain; lit by 07.2's HeaderMutation.
            // `upstream_host_for_log_h2` stays None (no proxy attempt).
            r
        }
```
to:
```rust
        H2RequestPath::SynthFromDecode(mut r) => {
            // 07.1 Task 7: decode-side filter short-circuit. Phase 11 D6:
            // decorate the filter-synth response with the standard H2 response
            // headers (closes 09 REVIEW M2 implementation arm).
            // `upstream_host_for_log_h2` stays None (no proxy attempt).
            crate::response::decorate_filter_synth_response_h2(&mut r);
            r
        }
```

**Encode-side** — the `Decision::StopAndSend(replacement)` arm at `hcm.rs:436`. Decorate the
constructed `resp` before it is sent. Change:

```rust
        envoy_filter::Decision::StopAndSend(replacement) => {
            resp = Response {
                status: replacement.status,
                reason: replacement.reason,
                headers: replacement.headers,
                body: replacement.body,
            };
        }
```
to:
```rust
        envoy_filter::Decision::StopAndSend(replacement) => {
            resp = Response {
                status: replacement.status,
                reason: replacement.reason,
                headers: replacement.headers,
                body: replacement.body,
            };
            // Phase 11 D6: decorate the encode-side filter-synth replacement with
            // the standard H2 response headers (symmetric to the H1 helper's
            // encode-side wiring at hcm.rs:636). No phase-11 filter takes this
            // path, but future encode-side-short-circuiting H2 filters inherit it.
            crate::response::decorate_filter_synth_response_h2(&mut resp);
        }
```

> **Implementer note:** confirm the `crate::response::` path resolves (the helper is
> `pub(crate)`); confirm `resp` is the local `Response` binding at the encode-side site.
> Adjust the comment line-number references if they have drifted.

- [ ] **Step 6: Run the 5 stable-toolchain gates.** Expected: all clean. (The decode-side
  change makes `SynthFromDecode(mut r)` mutable — ensure no `unused_mut` clippy warning by
  confirming the decorate call uses `&mut r`.)

- [ ] **Step 7: Append the Task 4 PROGRESS subsection (attributing the 09 REVIEW M2 implementation close + cross-referencing the phase-10 D5 amendment at `DECISIONS.md:699` + the phase-09 PROGRESS Commit C forward-reference) + commit.**

```bash
git add crates/envoy-http2/src/response.rs crates/envoy-http2/src/hcm.rs docs/envoy-rust/phases/11-http-filter-fault/PROGRESS.md
git commit -m "phase 11: task 4 — D6 H2 decorate_filter_synth_response_h2 + 2 wirings [closes 09 REVIEW M2 impl]"
```

---

## Task 5: D8.1 fixture 0018 + `Driver::Http2ProbeList` harness + Docker-gated wrapper

**Goal.** Land the differential fixture `0018-http-filter-fault` on an HTTP/2 listener + the
new `Driver::Http2ProbeList` harness driver (recommended option (a) per SPEC §6.1) + the
Docker-gated wrapper. Asserts the `[503, 200, 503, 200]` per-probe sequence + the abort body
+ the decorated H2 header set bilaterally.

**Read before writing:** `tests/differential/src/lib.rs` — the `Driver::Http1ProbeList`
variant (`:81`) + its match arm (`:2136`) + the `Http1Probe` struct (`:619`) + `drive_http2`
(`:1282`); and `tests/differential/tests/http_filter_rbac.rs` (the 10 wrapper precedent) +
`tests/fixtures/0017-http-filter-rbac/` (the 10 fixture precedent — `envoy.yaml`,
`envoy-rust.yaml`, `expectations.yaml`, `README.md` shapes).

**Files:**
- Modify: `tests/differential/src/lib.rs` (new `Driver::Http2ProbeList` variant + match arm + parse round-trip test).
- Create: `tests/fixtures/0018-http-filter-fault/{envoy.yaml,envoy-rust.yaml,expectations.yaml,README.md}`.
- Create: `tests/differential/tests/http_filter_fault.rs` (Docker-gated wrapper).

### Steps

- [ ] **Step 1: Add the `Driver::Http2ProbeList` variant + match arm + a parse round-trip test (failing).**

Add the variant to the `Driver` enum (after `Driver::Http2`):

```rust
    /// 11 NEW: drive a sequence of HTTP/2 probes against a single listener
    /// address. Each probe runs an independent H2 request/response cycle and
    /// applies the per-probe equivalence cascade. Mirrors `Http1ProbeList`
    /// (04.2) but drives over H2 via `drive_http2`. The `Http1Probe` struct is
    /// codec-agnostic (request shape + per-probe expectations) and is reused
    /// directly. Per phase-11 SPEC §3 D8.1.
    Http2ProbeList {
        probes: Vec<Http1Probe>,
    },
```

Add the match arm. The cleanest implementation mirrors the `Driver::Http1ProbeList` arm
(`lib.rs:2136`) verbatim, swapping `drive_http1` → `drive_http2`. To avoid duplicating the
~80-line per-probe assertion cascade, factor the cascade into a shared helper
`assert_probe_equivalence(probe, upstream_resp, subject_resp, expectations)` called by BOTH
the `Http1ProbeList` and `Http2ProbeList` arms; OR (simpler, lower-risk) copy the cascade
into the new arm. The implementer picks based on the existing code's factoring; if the
`Http1ProbeList` cascade is already a self-contained block, extracting a helper is preferred
(DRY). Sketch (copy variant):

```rust
        Driver::Http2ProbeList { probes } => {
            for probe in probes {
                let upstream_resp = drive_http2(
                    upstream_addr,
                    &probe.method,
                    &probe.path,
                    &probe.host,
                    &probe.extra_headers,
                )
                .await
                .with_context(|| format!("upstream envoy http2 drive (probe {})", probe.name))?;
                let subject_resp = drive_http2(
                    subject_addr,
                    &probe.method,
                    &probe.path,
                    &probe.host,
                    &probe.extra_headers,
                )
                .await
                .with_context(|| format!("envoy-rust http2 drive (probe {})", probe.name))?;
                // ── per-probe equivalence cascade: copy verbatim from the
                //    Driver::Http1ProbeList arm (status exact + per-probe
                //    expected_status / expected_body / expected_headers). ──
                // <same cascade as lib.rs:2160-onwards, parameterized on probe>
            }
        }
```

> **Implementer note:** the `Http1ProbeList` cascade (`lib.rs:2160`+) handles `response_status:
> exact`, per-probe `expected_status`, `expected_body` (byte-exact), and `expected_headers`
> (set-equal-modulo-allow-list). Replicate it exactly for `Http2ProbeList`. Also add
> `Driver::Http2ProbeList { .. }` to any exhaustive `match` over `Driver` elsewhere in
> `lib.rs` (e.g. the listener-protocol classifier near `:1650-1652` that groups
> `Http1ProbeList`/`Http2` — `Http2ProbeList` is H2 like `Http2`).

Add a parse round-trip test (mirroring the `Http1ProbeList` parse test at `lib.rs:3797`):

```rust
    #[test]
    fn http2_probe_list_round_trips_from_yaml() {
        let yaml = r#"
driver:
  kind: http2_probe_list
  probes:
  - name: abort
    method: GET
    path: /
    host: envoy-rust.test
    extra_headers: [["x-fault", "abort"]]
    expected_status: 503
"#;
        let e: FixtureExpectations = serde_yaml::from_str(yaml).expect("parses");
        let Driver::Http2ProbeList { probes } = e.driver else {
            panic!("expected Http2ProbeList");
        };
        assert_eq!(probes.len(), 1);
        assert_eq!(probes[0].expected_status, Some(503));
    }
```

> **Implementer note:** the exact `kind`/serde tag for `Driver` variants + the
> `FixtureExpectations` type name come from the existing `Http1ProbeList` round-trip test —
> match that test's shape exactly.

- [ ] **Step 2: Run to verify the harness test fails.**

Run: `cargo test -p differential http2_probe_list 2>&1 | tail -20`
Expected: FAIL — `no variant Http2ProbeList`.

- [ ] **Step 3: (implement the variant + arm per Step 1), then run to verify pass.**

Run: `cargo test -p differential http2_probe_list 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 4: Create the fixture `tests/fixtures/0018-http-filter-fault/envoy.yaml`** (reference Envoy config; H2 listener; mirrors fixture 0009's H2 HCM shape):

```yaml
# Phase 11: fault filter (abort) on an HTTP/2 listener.
#   http_filters: [envoy.filters.http.fault, envoy.filters.http.router]
# The fault filter aborts (503 + body "fault filter abort") any request
# carrying `x-fault: abort`; other requests pass through to the
# direct_response 200. Empirical 503 response shape (verified at phase-11
# state-2 PLAN-write via Docker run of envoyproxy/envoy:v1.33.0 on an H2
# listener):
#   - status 503
#   - body bytes "fault filter abort" (18 bytes; hex 66 61 75 6c 74 20 66 69
#     6c 74 65 72 20 61 62 6f 72 74)
#   - 4 standard H2 response headers: {server, content-length, content-type,
#     date}  (NO connection — H2-forbidden hop-by-hop)
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
            name: local
            virtual_hosts:
            - name: default
              domains: ["*"]
              routes:
              - match: { prefix: "/" }
                direct_response: { status: 200, body: { inline_string: "ok\n" } }
```

- [ ] **Step 5: Create `envoy-rust.yaml`** — identical to `envoy.yaml` (no documented
  divergence; per the §7.1 fixture convention, initially identical). Copy the file verbatim.

- [ ] **Step 6: Create `expectations.yaml`** (the `Http2ProbeList` driver + per-probe
  cascade; mirrors `0017-http-filter-rbac/expectations.yaml` shape, swapping the driver
  kind + the header/status/body values):

```yaml
# Phase 11: 4-probe H2 burst against the fault filter chain.
#   probe 1 (x-fault: abort) → 503 abort
#   probe 2 (no header)      → 200 pass-through (direct_response)
#   probe 3 (x-fault: abort) → 503 abort
#   probe 4 (no header)      → 200 pass-through
equivalence:
  response_status: exact
  response_headers: set_equal_modulo_allow_list
  response_body: byte_exact
driver:
  kind: http2_probe_list
  probes:
  - name: abort-1
    method: GET
    path: /
    host: envoy-rust.test
    extra_headers: [["x-fault", "abort"]]
    expected_status: 503
    expected_body: { kind: byte_exact, body: "fault filter abort" }
  - name: pass-1
    method: GET
    path: /
    host: envoy-rust.test
    expected_status: 200
    expected_body: { kind: byte_exact, body: "ok\n" }
  - name: abort-2
    method: GET
    path: /
    host: envoy-rust.test
    extra_headers: [["x-fault", "abort"]]
    expected_status: 503
    expected_body: { kind: byte_exact, body: "fault filter abort" }
  - name: pass-2
    method: GET
    path: /
    host: envoy-rust.test
    expected_status: 200
    expected_body: { kind: byte_exact, body: "ok\n" }
```

> **Implementer note:** the EXACT YAML keys for `expected_body` / `expected_status` /
> `extra_headers` + the top-level `equivalence` block + `driver.kind` tag MUST match the
> on-disk `Http1Probe` / `FixtureExpectations` serde shape (read `0017-http-filter-rbac/expectations.yaml`
> + the `Http1Probe` struct's `#[serde(...)]` attributes). Adjust keys/casing to match. The
> `expected_headers` allow-list (the `server` + `date` rows) is the default per the harness;
> add an explicit per-probe `expected_headers` block only if the 0017 fixture does so.

- [ ] **Step 7: Create `README.md`** documenting the fixture (mirror `0017-http-filter-rbac/README.md`):
  the H2 listener; the fault abort + header gate; the `[503,200,503,200]` expected sequence;
  the §6.2-verified body + header set; and (load-bearing) the note that **this fixture is the
  first HTTP-filter-family fixture on an H2 listener, exercising the H2 filter-synth writer
  path bilaterally — validating the phase-11 D6 `decorate_filter_synth_response_h2` helper
  end-to-end (closing 09 REVIEW M2)**.

- [ ] **Step 8: Create the Docker-gated wrapper `tests/differential/tests/http_filter_fault.rs`** (mirror `http_filter_rbac.rs`):

```rust
//! Docker-gated differential test for fixture 0018-http-filter-fault.
//! Runs the fault filter on an HTTP/2 listener; asserts the [503,200,503,200]
//! per-probe burst + the abort body + the decorated H2 header set bilaterally.

mod common;

#[tokio::test]
async fn http_filter_fault_fixture() {
    common::run_fixture("0018-http-filter-fault").await;
}
```

> **Implementer note:** match `http_filter_rbac.rs`'s exact module-include + `run_fixture`
> invocation shape (the `mod common;` / harness-entry convention may differ — copy it verbatim
> from the 0017 wrapper).

- [ ] **Step 9: Run the workspace build + the Docker-gated fixture (locally, if Docker available).**

Run: `cargo test -p differential http_filter_fault_fixture -- --ignored 2>&1 | tail -30`
(Docker-gated tests are typically `#[ignore]`-gated or feature-gated; match the 0017
convention. If the wrapper is not ignore-gated, run it directly.)
Expected: PASS — 4 probes `[503,200,503,200]` bilaterally green; abort body + header set
match.

- [ ] **Step 10: Run the 5 stable-toolchain gates.** Expected: all clean.

- [ ] **Step 11: Append the Task 5 PROGRESS subsection + commit.**

```bash
git add tests/differential/src/lib.rs tests/fixtures/0018-http-filter-fault/ tests/differential/tests/http_filter_fault.rs docs/envoy-rust/phases/11-http-filter-fault/PROGRESS.md
git commit -m "phase 11: task 5 — D8.1 fixture 0018 + Driver::Http2ProbeList harness + Docker wrapper"
```

---

## Task 6: D8.2 fuzz corpus seed

**Goal.** Add the fault bootstrap shape as a `parse_bootstrap` fuzz corpus seed (extends seed
count 17 → 18) + the `.gitignore` allow-list entry + the in-source SUCCESS-array extension —
all in the SAME commit (per the 09 + 10 Task 6 follow-up lesson; lock-in #33).

**Read before writing:** `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_rbac_filter.yaml`
(the 10 seed precedent) + `crates/envoy-config/fuzz/.gitignore` + the
`fuzz_corpus_seeds_parse_or_reject_cleanly` test's SUCCESS array in
`crates/envoy-config/src/bootstrap.rs`.

**Files:**
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_fault_filter.yaml`.
- Modify: `crates/envoy-config/fuzz/.gitignore` (1-line allow-list extension).
- Modify: `crates/envoy-config/src/bootstrap.rs` (extend the `fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS array).

### Steps

- [ ] **Step 1: Create `hcm_fault_filter.yaml`** — a complete valid bootstrap with the fault
  filter (wrap the fixture-0018 HCM in a full bootstrap with a listener address; mirror
  `hcm_rbac_filter.yaml`'s structure exactly, swapping the RBAC filter block for the fault
  block):

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
            name: local
            virtual_hosts:
            - name: default
              domains: ["*"]
              routes:
              - match: { prefix: "/" }
                direct_response: { status: 200, body: { inline_string: "ok\n" } }
```

> **Implementer note:** match `hcm_rbac_filter.yaml`'s exact top-level structure (it may
> include `admin:` / `clusters:` blocks the fixture omits; the fuzz seed is a full bootstrap,
> not a fixture). Copy that structure and swap only the filter block.

- [ ] **Step 2: Add the `.gitignore` allow-list entry** in `crates/envoy-config/fuzz/.gitignore`
  (mirror the existing `!corpus/parse_bootstrap/hcm_rbac_filter.yaml`-style entries):

```gitignore
!corpus/parse_bootstrap/hcm_fault_filter.yaml
```

- [ ] **Step 3: Extend the SUCCESS array in `fuzz_corpus_seeds_parse_or_reject_cleanly`.**

In `crates/envoy-config/src/bootstrap.rs`, find the test `fuzz_corpus_seeds_parse_or_reject_cleanly`
and add `"hcm_fault_filter.yaml"` to its list of seed filenames expected to parse cleanly
(the SUCCESS array), preserving the existing ordering/format.

- [ ] **Step 4: Run the corpus test to verify it passes.**

Run: `cargo test -p envoy-config fuzz_corpus_seeds_parse_or_reject_cleanly 2>&1 | tail -20`
Expected: PASS — the new seed parses cleanly.

- [ ] **Step 5: (Optional, if `cargo fuzz` + nightly available) short-budget fuzz run.**

Run: `cargo +nightly fuzz run parse_bootstrap -- -runs=100000 2>&1 | tail -10` (or the
project's documented short-budget invocation). Expected: no crashes. (CI runs the
short-budget fuzz on push; this local step is confirmatory only.)

- [ ] **Step 6: Run the 5 stable-toolchain gates.** Expected: all clean.

- [ ] **Step 7: Append the Task 6 PROGRESS subsection + commit.**

```bash
git add crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_fault_filter.yaml crates/envoy-config/fuzz/.gitignore crates/envoy-config/src/bootstrap.rs docs/envoy-rust/phases/11-http-filter-fault/PROGRESS.md
git commit -m "phase 11: task 6 — D8.2 fuzz corpus seed hcm_fault_filter.yaml"
```

---

## Task 7: D8.3 in-process backstop (H1 path; with per-probe header assertion)

**Goal.** Land an in-process backstop that boots `envoy-bin` with a synthesized fault
bootstrap on an **H1** listener and asserts the abort semantics over 4 sequential GET probes.
Per SPEC §6.4 + the 10 REVIEW M1 lesson, the backstop **includes the per-probe standard-header
presence assertion on the 503 probes** (recommended option (a)).

**Read before writing (lock-in #35):** `crates/envoy-bin/tests/http_filter_rbac.rs` (the 10
backstop — `tokio::process::Command + .kill_on_drop(true) + stdout: Stdio::null() + stderr:
Stdio::piped()` discipline; the bootstrap-synthesis + probe-loop shape).

**Files:**
- Create: `crates/envoy-bin/tests/http_filter_fault.rs`.

### Steps

- [ ] **Step 1: Write the backstop test** (mirror `http_filter_rbac.rs` structure: synthesize
  an H1 bootstrap with the fault filter [abort 503, 100%, header-gated `x-fault: abort`] +
  router + direct_response 200; spawn `envoy-bin` via `tokio::process::Command` with
  `.kill_on_drop(true)` + `stdout(Stdio::null())` + `stderr(Stdio::piped())`; wait for the
  listener; issue 4 sequential GET probes with `x-fault` values `[abort, <none>, abort,
  <none>]`):

```rust
//! In-process backstop for the fault filter (abort path), exercised over an
//! HTTP/1.1 listener. Complements the H2 differential fixture 0018 — both
//! codecs covered across the two test tiers. Boots envoy-bin as a subprocess
//! with kill_on_drop discipline (09 REVIEW M3 pattern, standing since 10 Task 7).

// <module setup mirroring crates/envoy-bin/tests/http_filter_rbac.rs:
//  - synth bootstrap string (H1 HCM + fault filter + router + direct_response)
//  - spawn envoy-bin (tokio::process::Command, kill_on_drop(true), Stdio)
//  - wait-for-listener helper
//  - a small H1 GET helper returning (status, headers, body)>

#[tokio::test]
async fn http_filter_fault_in_process_backstop() {
    // ... boot envoy-bin on an ephemeral H1 listener with the fault bootstrap ...

    // Probe sequence: [abort, pass, abort, pass].
    let probes: [(Option<&str>, u16, &str); 4] = [
        (Some("abort"), 503, "fault filter abort"),
        (None, 200, "ok\n"),
        (Some("abort"), 503, "fault filter abort"),
        (None, 200, "ok\n"),
    ];
    for (i, (fault_header, expected_status, expected_body)) in probes.iter().enumerate() {
        let (status, headers, body) = http1_get(addr, "/", *fault_header).await;
        assert_eq!(status, *expected_status, "probe {i}: status");
        assert_eq!(body, *expected_body, "probe {i}: body");
        if *expected_status == 503 {
            // 10 REVIEW M1 lesson (SPEC §6.4 option (a)): assert the standard
            // HTTP/1.1 headers are present on the abort response.
            for h in ["server", "date", "content-length", "content-type", "connection"] {
                assert!(
                    headers.iter().any(|(k, _)| k.eq_ignore_ascii_case(h)),
                    "probe {i}: missing standard header {h:?} on 503; headers: {headers:?}"
                );
            }
        }
    }
}
```

> **Implementer note:** the H1 abort response carries 5 standard headers (incl. `connection`,
> since H1 keeps it — the H1 `decorate_filter_synth_response` adds all 5). This is the H1
> path, NOT the H2 path (the H2 fixture asserts 4 headers without `connection`). Source the
> exact bootstrap-synthesis + subprocess-spawn + listener-wait helpers from
> `http_filter_rbac.rs` — copy its structure verbatim and swap the RBAC filter block for the
> fault block + the `x-rbac-pass` header for `x-fault` + the 403 body for the 503 abort body.

- [ ] **Step 2: Run the backstop test.**

Run: `cargo test -p envoy-bin http_filter_fault_in_process_backstop 2>&1 | tail -30`
Expected: PASS — `[503,200,503,200]` + bodies + 503-probe header presence all green.

- [ ] **Step 3: Run the 5 stable-toolchain gates.** Expected: all clean.

- [ ] **Step 4: Append the Task 7 PROGRESS subsection + commit.**

```bash
git add crates/envoy-bin/tests/http_filter_fault.rs docs/envoy-rust/phases/11-http-filter-fault/PROGRESS.md
git commit -m "phase 11: task 7 — D8.3 in-process backstop (H1; with 503-probe header assertion)"
```

---

## Task 8: state-4 phase-done verification + STATE advance to state-5-next

**Goal.** Run the full `BOOTSTRAP_PROMPT.md` §7.5 (a)-(e) phase-done gate, capture the
evidence anchor (CI run URL + HEAD SHA + completion timestamp + per-gate quoted output),
**re-confirm h2spec ≥95%** (phase 11 touched the H2 writer path — SPEC §6.8), and advance
STATE.md to state-5-next.

**Files:**
- Modify: `docs/envoy-rust/phases/11-http-filter-fault/PROGRESS.md` (state-4 evidence anchor at the Task 8 subsection).
- Modify: `docs/envoy-rust/STATE.md` (Active phase status → state 4-complete / state-5-next; Next expected skill → `superpowers:requesting-code-review`; Last commit; Last updated; new Notes subsection).

### Steps

- [ ] **Step 1: Run the 5 stable-toolchain gates locally + capture output.**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-targets
cargo test --workspace
cargo deny check
```
Expected: all clean. Capture the `cargo test --workspace` summary line (test count) for the
PROGRESS evidence anchor.

- [ ] **Step 2: Run the Docker-gated differential suite (all 18 fixtures) + h2spec locally if Docker available,** OR rely on the CI run triggered by the push. Capture:
  - fixture `0018-http-filter-fault` green
  - all 18 fixtures (`0001-tcp-echo` through `0018-http-filter-fault`) green SIMULTANEOUSLY
  - **h2spec ≥95%** (re-confirm the H2 writer-path decoration introduces no framing regression)
  - `parse_bootstrap` fuzz clean on the 18-seed corpus

- [ ] **Step 3: Push + capture the CI evidence anchor.**

```bash
git push
```
Then poll the CI run for the pushed HEAD until both jobs (`build + test + lint` +
`fuzz (parse_bootstrap, 30s)`) report `success`. Capture the CI run ID/URL + HEAD SHA +
completion timestamp + per-job wall time.

- [ ] **Step 4: Write the state-4 evidence anchor into the PROGRESS Task 8 subsection** —
  per-gate quoted output for all 5 stable-toolchain gates + each Docker-gated fixture +
  `h2spec_pass_rate_gate` + `parse_bootstrap` fuzz iteration count + the CI run URL + HEAD SHA
  + completion timestamp (per the 05.3 → … → 10 state-4 evidence-discipline cadence; SPEC §6.8).

- [ ] **Step 5: Advance STATE.md** — Active phase status → `phase 11 lifecycle state
  4-complete / state-5-next (PLAN.md + PROGRESS.md + state-4 evidence landed; REVIEW.md
  pending)`; Next expected skill → `superpowers:requesting-code-review` scoped to the reviewed
  range `<state-2 PLAN-write SHA>..<this Task 8 HEAD>`; Last commit; Last updated; append a new
  Notes subsection recording the phase-11 state-3 execution arc. **Preserve all prior
  subsections verbatim per D-3.5 + D-3.4.**

- [ ] **Step 6: Commit (docs-only state-4 anchor + STATE advance).**

```bash
git add docs/envoy-rust/phases/11-http-filter-fault/PROGRESS.md docs/envoy-rust/STATE.md
git commit -m "phase 11: task 8 — state-4 phase-done verification + STATE advance to state-5-next"
git push
```

- [ ] **Step 7: Exit cleanly.** The state-4 evidence anchor + STATE advance ends the state-3
  execution arc. The next session enters state 5 — writes `REVIEW.md` per
  `superpowers:requesting-code-review`.

---

## Self-Review (PLAN-writer's fresh-eyes pass against the SPEC)

**1. Spec coverage:** D1 (Task 1), D2 (Task 1), D3 (Task 1), D4 (Task 2), D5 (Task 3), D6
(Task 4 — closes 09 M2), D7 + D7.1 (Task 2), D8.1 (Task 5), D8.2 (Task 6), D8.3 (Task 7),
state-4 (Task 8). All 8 deliverables + the 3 sub-deliverables (D7.1, D8.1/2/3) map to a task.
The §6.2 empirical verification is performed at THIS state-2 commit (PROGRESS Task 1
preamble). The §6.1 split-gate is evaluated (§3 above: single-phase). The §4 deferrals
(fractional, delay, rate-limit, grpc-status, downstream-controlled, max_active_faults,
runtime overrides, per-route config) are honored by `deny_unknown_fields` (D1) + the
fractional-rejection validator (D2). No gap.

**2. Placeholder scan:** No "TBD" / "implement later" / "add error handling" placeholders.
Each code step shows the actual code. The "Implementer note" blocks flag exact API
shapes to spot-check (stats accessor name, `Response` type path, default-string sources,
serde keys) — these are verification directives, not placeholders; the surrounding code is
complete and the notes name the precedent file to check.

**3. Type consistency:** `FaultConfig`/`FaultAbort`/`FractionalPercent`/`DenominatorType`,
`selects_deterministic()`, `DenominatorType::value()`, `default_denominator()`,
`FaultFilter`/`build_from_config`/`decode_headers`/`encode_headers`/`header_gate_matches`,
`FAULT_ABORT_BODY` (`b"fault filter abort"`, 18 bytes), `decorate_filter_synth_response_h2`,
`Driver::Http2ProbeList { probes: Vec<Http1Probe> }`, the 3 `ConfigError` variants
(`InvalidFaultAbortStatus`/`FaultPercentageOutOfRange`/`UnsupportedFractionalFaultPercentage`),
and the stat name `http.{hcm_stat_prefix}.fault.aborts_injected` are used consistently across
Tasks 1-8. The abort `FilterResponse` uses `reason: None` (matching `Option<&'static str>`).
The 3-arg `build_from_config(cfg, registry, hcm_stat_prefix)` signature is reused unchanged
from phase 10. No signature drift.
