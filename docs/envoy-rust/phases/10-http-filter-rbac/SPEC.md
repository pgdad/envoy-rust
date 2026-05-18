# Phase 10 (`10-http-filter-rbac`) — SPEC

- **Phase id:** `10`
- **Slug:** `10-http-filter-rbac`
- **Status before this SPEC lands:** _not yet in ROADMAP.md_ (per `docs/envoy-rust/ROADMAP.md` at HEAD `518140c`, the phase-09 state-6 close-out commit; the "HTTP filters family" §9 heading exists with one concrete row beneath it — phase 09 `local_ratelimit`, `status: done`). **This SPEC's landing commit is the second concrete row added beneath the HTTP-filter-family heading**, with `status: planned`.
- **Charter source:** `BOOTSTRAP_PROMPT.md` §9 — *"HTTP filters family — header manipulation, cors, compression, fault, local+global rate limit, jwt_authn, **rbac**, ext_authz, ext_proc, oauth2, csrf, buffer, lua, wasm, adaptive concurrency, admission control, bandwidth limit."* This phase lands `envoy.filters.http.rbac` with a header-based Permission/Principal scope; IP/metadata/auth/url_path/SNI defer to subsequent RBAC phases per §4 below.
- **Position in the project:** the **second post-MVP-trunk feature-family phase** and the **second concrete HTTP-filter-family phase** (after phase-09 `local_ratelimit`). The MVP trunk 00→08 stands `done` as of commit `304ce98`; phase 09 stands `done` as of commit `518140c`. Phase 10 amortizes the framework + helper investment that phase 09 established (ADR-0033's `decorate_filter_synth_response` H1 HCM helper; the 4-counter stats-wiring pattern; the `Decision::StopAndSend(FilterResponse)` production-path discipline).
- **depends-on:** `07` (the parent filter-chain framework). Phase 10 extends the 07.1-landed `envoy-filter::FilterPipeline` + `HttpFilterInstance` enum with a fourth production variant (after `Router` at 07.1, `HeaderMutation` at 07.2, `LocalRateLimit` at 09). Implicit dependencies on 04.2 (`HeaderMatcher` + `StringMatcher` reuse) and on 09 (the H1 HCM `decorate_filter_synth_response` helper landed via ADR-0033 Commit C) are not in the depends-on field per ROADMAP schema conventions (the schema captures only direct ROADMAP-row dependencies; cross-deliverable reuse is implicit). The 16-Docker-gated-fixture regression baseline established at phase-09 close (`0001-tcp-echo` through `0016-http-filter-local-rate-limit`) carries forward unchanged per `BOOTSTRAP_PROMPT.md` §7.5 (b).
- **Brainstorm narrative:** see the "Phase-10 state-1 brainstorm" subsection of `docs/envoy-rust/STATE.md` for the family-pick + first-filter-pick rationale with alternatives considered along the 5-dimension scoring framework.

---

## 1. Goal and acceptance signal

Phase 10 lands the `envoy.filters.http.rbac` filter (per upstream Envoy v1.33's documented filter name; typed_config `@type = type.googleapis.com/envoy.extensions.filters.http.rbac.v3.RBAC`) as the **third concrete pluggable HTTP filter** in the 07.x-established framework (after HeaderMutation at 07.2 and LocalRateLimit at 09). The filter evaluates a declarative authorization policy tree against the decode-side request; on Deny-action match (or Allow-action no-match), the filter short-circuits via `Decision::StopAndSend(FilterResponse)` with status 403 + the standard HTTP/1.1 response headers via the ADR-0033 H1 HCM `decorate_filter_synth_response` helper.

The phase also closes **two phase-09 REVIEW carryforwards** at their named-owner sites:

- **09 REVIEW M2** (H2 HCM filter-synth header decoration gap; ADR-0033 Consequences misrepresents H2 as "naturally inherits") — closed via D5 (ADR-0033 Consequences amendment per the preferred close shape (a) — doc-amendment recording the H2 analogous gap as known-deferred until next HTTP-filter-family phase exercising filters on H2). Phase 10's fixture exercises H1 only (matching the 07.2 + 09 single-codec-fixture cadence); the M2 close at phase 10 is doc-amendment, not implementation.
- **09 REVIEW M3** (Task 7 in-process backstop subprocess discipline regression from 07.2/08.2 precedents — `std::process::Command` instead of `tokio::process::Command + kill_on_drop(true) + Stdio::null()`) — closed via D8.3 (phase-10's Task 7 in-process backstop adopts the 07.2/08.2 `tokio::process::Command` + `kill_on_drop(true)` + stdout `Stdio::null()` discipline directly; no regression).

**Differential surface added by phase 10:**

- **Fixture `0017-http-filter-rbac`** — bilateral assertion that both proxies, given an identical bootstrap with an RBAC filter configured as `action: ALLOW` with a single policy permitting requests bearing `x-rbac-pass: yes` AND any principal, produce the deterministic status sequence on a 4-probe burst: probe 1 (no header) → 403 (no policy matches under Allow → default Deny); probe 2 (`x-rbac-pass: yes`) → 200 (policy matches); probe 3 (`x-rbac-pass: no`) → 403; probe 4 (`x-rbac-pass: yes`) → 200. Asserts each 403 response carries the 5 standard HTTP/1.1 headers (`server`, `date`, `content-length`, `content-type`, `connection`) via the ADR-0033 H1 HCM helper. Reuses fixture 0007's minimal HCM + `direct_response` data-plane shape (matching the 07.2 + 09 precedent — focuses the bilateral assertion on the filter's authorization semantics, not on upstream proxy complexity).

**Acceptance signal (a)–(f), per `BOOTSTRAP_PROMPT.md` §7.5:**

- **(a)** Fixture `0017-http-filter-rbac` green at Docker-gated CI.
- **(b)** All **16 pre-existing differential fixtures** (`0001-tcp-echo` through `0016-http-filter-local-rate-limit`) **remain green simultaneously** at the same CI run (regression-equivalence per `BOOTSTRAP_PROMPT.md` §7.5 (b)).
- **(c)** `h2spec` continues at ≥95% (parent-05 baseline 99.31%; phase 10 engages no H2-framing surfaces — the filter operates on the post-codec `FilterRequest` / `FilterResponse` abstraction per the 07.1 ADR-0031 + 09 SPEC §5.8 precedent).
- **(d)** `parse_bootstrap` fuzz target clean for the short-budget CI run on the extended corpus (one new seed for the rbac bootstrap shape; corpus extends from 16 to 17 seeds).
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check` all clean.
- **(f)** `REVIEW.md` approved.

A **single CI run** must light up gates (a) through (e) **simultaneously** (continues the project precedent established at 06.1 / 07.x / 08.x / 09 — fixture inheritance is a regression vector).

---

## 2. Behavior-contract scope for phase 10

Phase 10 extends `docs/envoy-rust/BEHAVIOR_CONTRACT.md` with two authored additions, landed at the tasks where each is first empirically exercised (per the established 06.x / 07.x / 08.x / 09 doctrine — contract extensions land at empirical-engagement task time, NOT at PLAN-write time and NOT at state-1 SPEC time).

### 2.1 "Stat-name mapping" extension — 2 new rows

Two new counter rows under the `http_rbac.<stat_prefix>` namespace, mirroring upstream Envoy v1.33's documented stat tree (the `allowed`/`denied` pair for the primary rules; `shadow_allowed`/`shadow_denied` defer to whichever future phase first lands `shadow_rules` per §4 below):

| Stat name | Equivalence | Rationale |
|---|---|---|
| `http_rbac.<stat_prefix>.allowed` | value-exact | Counter; one increment per request allowed under the primary rules (whether by explicit Allow-action match OR by Deny-action no-match). Both proxies emit one increment per allowed request. |
| `http_rbac.<stat_prefix>.denied` | value-exact | Counter; one increment per request denied under the primary rules (whether by explicit Deny-action match OR by Allow-action no-match). Both proxies emit one increment per denied request (synchronously with the 403 `Decision::StopAndSend` emission). |

At phase-10 scope the two counters satisfy: `allowed + denied == total_requests_to_filter`. Each counter is incremented at its own fire site in `RbacFilter::decode_headers` — no derived computation; one source of truth per name.

The `<stat_prefix>` segment is sourced from the filter's `stat_prefix` field — actually upstream Envoy's `envoy.extensions.filters.http.rbac.v3.RBAC` HTTP filter config does NOT carry a `stat_prefix` field at the filter level; the prefix is computed from the parent HCM's `stat_prefix` via `http.<hcm_stat_prefix>.rbac.*` namespacing per upstream Envoy v1.33 source. **The state-2 PLAN-writer empirically verifies this against upstream Envoy v1.33 (Docker bootstrap + admin /stats scrape) before locking the namespace shape** — per the ADR-0033 process-gap awareness-only doctrine note (state-1 brainstorming should empirically verify upstream wire shapes for novel filter surfaces). If empirical verification reveals the namespace differs from the projection, the SPEC §2.1 revision lands via an ADR at PLAN-write time per D-3.5; the recommended state-1 SPEC projection is `http.<hcm_stat_prefix>.rbac.{allowed,denied}` (matching upstream Envoy's `http.<hcm_stat_prefix>.<filter_name>.*` general convention from 06.1).

### 2.2 "Header allow-list" extension — none required

The 403 response body shape + header decoration are determined by ADR-0033's revised contract:

- `RbacFilter::decode_headers` emits the 403 response with `body: Bytes::from_static(b"RBAC: access denied")` per upstream Envoy v1.33's source-hardcoded denial body (19 bytes; NO trailing newline — per **ADR-0034** ratifying the state-2 §6.2 empirical-verification finding that the original state-1 SPEC projection of `b"RBAC: access denied\n"` (20 bytes) was off by 1 byte; see DECISIONS.md ADR-0034 for the empirical evidence + revision narrative).
- The 5 standard HTTP/1.1 response headers (`server`, `date`, `content-length`, `content-type`, `connection`) are decorated onto every filter-synth 403 response by the existing H1 HCM helper `decorate_filter_synth_response` (landed at phase-09 ADR-0033 Commit C `ae2cef0` at `crates/envoy-http1/src/hcm.rs`; called from both `RequestPath::SynthFromDecode` and encode-side `Decision::StopAndSend(replacement)` writer-arm sites). The helper conditionally adds the standard headers if not already provided by the filter (case-insensitive name check); phase-10's RBAC filter emits 403 with no headers in `FilterResponse.headers`, so all 5 standard headers are decorated.

**No new Header allow-list row is needed.** The 04.1-landed `server` row + the 04.1-landed `date` row of `docs/envoy-rust/BEHAVIOR_CONTRACT.md`'s Header allow-list cover the cross-proxy implementation-identifying differences. The remaining 3 standard headers (`content-length`, `content-type`, `connection`) are value-exact across proxies under the deterministic fixture-0017 burst (4 sequential GET / probes; static `direct_response` body on Allow; static `"RBAC: access denied"` body on Deny per ADR-0034; sticky H1 close-on-response convention).

### 2.3 ADR-0033 Consequences amendment (closes 09 REVIEW M2)

D5 lands a ~10-15 LoC docs amendment to `docs/envoy-rust/DECISIONS.md` ADR-0033 Consequences §iii(c)-end (currently reads "the corresponding H2 HCM path naturally inherits via the shared Http1HCMConfig re-export"). The amendment replaces this with an explicit known-deferred record of the H2 analogous gap — naming the close site as "next HTTP-filter-family phase exercising filters on H2 (the H2 HCM `decorate_filter_synth_response` analogue lands as a ~50-70 LoC + 2-test follow-up at that phase)". This is the preferred close shape (a) per 09 REVIEW M2 disposition. The amendment is doc-only; no code change; no test re-run required.

Per `BOOTSTRAP_PROMPT.md` D-3.5 (ADRs are append-only; never edit a landed ADR), the amendment is technically a SUPERSEDING-NEAR-EDIT: it does NOT rewrite ADR-0033's Decision or Provenance sections; it only inserts a clarifying paragraph at the end of the Consequences §iii(c) bullet stating the H2 path is NOT covered by the H1 helper. **The state-2 PLAN-writer may choose to instead land a NEW ADR-0034 superseding ADR-0033's narrow Consequences claim** (the doctrinally-cleaner option). Recommended posture per §7 below: amend in-place (the amendment is a clarification, not a Decision change; the ADR's Decision + Provenance + Rationale stand verbatim).

---

## 3. Deliverables

Phase 10's scope is enumerated as deliverables `D1`–`D8` below. **The state-2 PLAN-writer organizes deliverables into tasks** (and evaluates the §6.1 split gate) — these are not 1:1 with tasks. Some deliverables compose into one task; some split across two. The deliverables are LISTED in roughly the order the PLAN-writer is expected to execute them, but the SPEC is not prescriptive about the order; only about the surface.

### D1 — `envoy-config` schema extension

At `crates/envoy-config/src/bootstrap.rs`, extend the existing `HttpFilterTypedConfig` enum (currently `Router`, `HeaderMutation`, `LocalRateLimit` variants per 07.2 + 09) with a fourth variant `Rbac(RbacConfig)`. The config struct shape mirrors upstream Envoy v1.33's `envoy.extensions.filters.http.rbac.v3.RBAC` (typed_config @type), narrowed to the minimum-viable surface for phase 10:

```rust
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RbacConfig {
    pub rules: Rules,                              // REQUIRED at phase 10
    // OPTIONAL — defers per §4
    //   shadow_rules: Rules
    //   shadow_rules_stat_prefix: String
    //   track_per_rule_stats: bool
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    #[serde(default = "default_action")]
    pub action: Action,                            // default Allow
    #[serde(default)]
    pub policies: BTreeMap<String, Policy>,        // deterministic iteration order
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum Action {
    Allow,
    Deny,
    // Log defers per §4
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub permissions: Vec<Permission>,              // REQUIRED non-empty
    pub principals: Vec<Principal>,                // REQUIRED non-empty
    // condition / checked_condition defer per §4
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub enum Permission {
    #[serde(rename = "any")]
    Any(bool),
    #[serde(rename = "header")]
    Header(HeaderMatcher),                         // reuses 04.2 HeaderMatcher
    #[serde(rename = "and_rules")]
    AndRules(PermissionSet),
    #[serde(rename = "or_rules")]
    OrRules(PermissionSet),
    #[serde(rename = "not_rule")]
    NotRule(Box<Permission>),
    // url_path, destination_ip, destination_port[_range], metadata,
    // requested_server_name[_matcher], uri_template defer per §4
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PermissionSet {
    pub rules: Vec<Permission>,                    // REQUIRED non-empty
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub enum Principal {
    #[serde(rename = "any")]
    Any(bool),
    #[serde(rename = "header")]
    Header(HeaderMatcher),                         // reuses 04.2 HeaderMatcher
    #[serde(rename = "and_ids")]
    AndIds(PrincipalSet),
    #[serde(rename = "or_ids")]
    OrIds(PrincipalSet),
    #[serde(rename = "not_id")]
    NotId(Box<Principal>),
    // authenticated, source_ip, direct_remote_ip, remote_ip, url_path,
    // metadata, filter_state defer per §4
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PrincipalSet {
    pub ids: Vec<Principal>,                       // REQUIRED non-empty
}
```

All struct shapes carry `#[serde(deny_unknown_fields)]` per the established envoy-config discipline (rejects forward-looking fields that envoy-rust does not yet support — e.g., `condition`, `metadata`, `source_ip`, `authenticated`). The `HeaderMatcher` reuses the existing 04.2-landed type directly (no schema duplication). The `policies` field uses `BTreeMap` for deterministic iteration order across both proxies (RBAC's policy-match semantic is short-circuit-on-first-match; deterministic iteration order ensures both proxies match the SAME policy first under multi-match scenarios — see §5.6 below).

The phase-10-deferred upstream-Envoy fields are each enumerated in §4 below; each is rejected by `deny_unknown_fields`.

### D2 — `envoy-config` validator extension

At `crates/envoy-config/src/bootstrap.rs::validate_http_filters`, extend the existing per-variant validator dispatch with an `Rbac` arm calling a new `validate_rbac_config(cfg) -> Result<(), ConfigError>` sub-validator. The validator checks:

- `rules.policies` is non-empty under both Allow and Deny actions (a no-policies RBAC config is operator error; both Allow-no-policies → deny-all and Deny-no-policies → allow-all are degenerate and rejected via `ConfigError::EmptyRbacPolicies`).
- For each `(policy_name, policy)` pair: `policy.permissions` is non-empty (`ConfigError::EmptyRbacPolicyPermissions { policy_name }`); `policy.principals` is non-empty (`ConfigError::EmptyRbacPolicyPrincipals { policy_name }`).
- For each `Permission::AndRules(set)` and `Permission::OrRules(set)`: `set.rules` is non-empty (`ConfigError::EmptyRbacPermissionSet { policy_name, permission_path }`); recursive descent into nested permissions.
- For each `Principal::AndIds(set)` and `Principal::OrIds(set)`: `set.ids` is non-empty (`ConfigError::EmptyRbacPrincipalSet { policy_name, principal_path }`); recursive descent into nested principals.
- Bounded recursion depth on Permission/Principal trees: reject trees deeper than `RBAC_TREE_MAX_DEPTH = 16` per defense-in-depth (`ConfigError::RbacTreeTooDeep { policy_name, depth }`). Matches typical RBAC policy depths in upstream-Envoy production configs (most are 1-4 levels; 16 is conservatively generous).

Five new `ConfigError` variants land at this site (`EmptyRbacPolicies`, `EmptyRbacPolicyPermissions`, `EmptyRbacPolicyPrincipals`, `EmptyRbacPermissionSet`, `EmptyRbacPrincipalSet`, `RbacTreeTooDeep` — possibly compressed to fewer if the PLAN-writer consolidates). Each has its own unit test cases for positive + negative parse paths. The validator is exercised by the existing fuzz target `parse_bootstrap` (the new fixture's bootstrap is seeded into the corpus per D8.2).

### D3 — `envoy-filter::RbacFilter` runtime + recursive tree-walk evaluator

New module `crates/envoy-filter/src/rbac.rs`. Hand-rolled per **D-3.2**'s *"Every individual filter ... Must be written from scratch"* doctrine + the 07.2 `header_mutation.rs` + 09 `local_rate_limit.rs` precedent. Module shape:

```rust
#![forbid(unsafe_code)]   // inherited from crate root

use std::sync::Arc;
use envoy_stats::{Counter, StatsRegistry};
use crate::error::FilterError;
use crate::pipeline::Decision;
use crate::types::{FilterRequest, FilterResponse};

/// The `envoy.filters.http.rbac` runtime filter.
#[derive(Debug, Clone)]
pub struct RbacFilter {
    action: RuntimeAction,
    policies: Arc<Vec<RuntimePolicy>>,
    allowed_counter: Arc<Counter>,
    denied_counter: Arc<Counter>,
}

#[derive(Debug, Clone, Copy)]
enum RuntimeAction { Allow, Deny }

#[derive(Debug)]
struct RuntimePolicy {
    name: String,                                // for debug logging
    permissions: Vec<RuntimePermission>,
    principals: Vec<RuntimePrincipal>,
}

#[derive(Debug)]
enum RuntimePermission {
    Any(bool),
    Header(envoy_config::HeaderMatcher),         // reuses 04.2 type
    AndRules(Vec<RuntimePermission>),
    OrRules(Vec<RuntimePermission>),
    NotRule(Box<RuntimePermission>),
}

#[derive(Debug)]
enum RuntimePrincipal {
    Any(bool),
    Header(envoy_config::HeaderMatcher),
    AndIds(Vec<RuntimePrincipal>),
    OrIds(Vec<RuntimePrincipal>),
    NotId(Box<RuntimePrincipal>),
}

impl RbacFilter {
    /// Lower an `envoy_config::RbacConfig` into the runtime filter +
    /// register the 2 stat counters against the StatsRegistry.
    pub(crate) fn build_from_config(
        cfg: &envoy_config::RbacConfig,
        registry: &Arc<StatsRegistry>,
        hcm_stat_prefix: &str,
    ) -> Result<Self, FilterError> { /* ... */ }

    pub(crate) fn decode_headers(&mut self, req: &mut FilterRequest) -> Decision {
        // For each policy in self.policies (BTreeMap iteration order):
        //   if eval_permissions(policy.permissions, req)
        //      && eval_principals(policy.principals, req)
        //      → policy matches
        //
        // Match decision (RBAC's allow-on-match-under-Allow-action semantic):
        //   action == Allow + any policy matches → ALLOW → Decision::Continue + inc allowed_counter
        //   action == Allow + no policy matches → DENY  → 403 StopAndSend + inc denied_counter
        //   action == Deny  + any policy matches → DENY  → 403 StopAndSend + inc denied_counter
        //   action == Deny  + no policy matches → ALLOW → Decision::Continue + inc allowed_counter
        unimplemented!()
    }

    pub(crate) fn encode_headers(&mut self, _resp: &mut FilterResponse) -> Decision {
        // Decode-only filter (per upstream Envoy semantic — RBAC operates on
        // request only).
        Decision::Continue
    }
}

fn eval_permission(p: &RuntimePermission, req: &FilterRequest) -> bool {
    match p {
        RuntimePermission::Any(b) => *b,
        RuntimePermission::Header(m) => match_header(m, &req.headers),
        RuntimePermission::AndRules(set) => set.iter().all(|p| eval_permission(p, req)),
        RuntimePermission::OrRules(set) => set.iter().any(|p| eval_permission(p, req)),
        RuntimePermission::NotRule(p) => !eval_permission(p, req),
    }
}

// Symmetric for eval_principal.
```

**Recursive tree-walk implementation note (signpost for the PLAN-writer + state-3 implementer):** the recursion is bounded by D2's parse-time depth check (`RBAC_TREE_MAX_DEPTH = 16`). The evaluator visits at most `O(depth * children_per_node)` nodes per request. For the phase-10 fixture (1 policy with 1 permission + 1 principal) the evaluator runs in O(1) per request. The recursion uses the stack directly (Rust's `match` recursion is naturally stack-bounded by the parse-time depth gate — defense-in-depth against malicious deeply-nested policies that bypass the validator).

**Async vs sync signature decision (signpost):** `decode_headers` is synchronous per the 07.1 framework (returns `Decision`, not `impl Future<Output = Decision>`). The recursive tree-walk evaluator is pure-compute (no I/O); sync is the natural shape. No async lift required.

**HeaderMatcher reuse (signpost):** the existing 04.2-landed `envoy_config::HeaderMatcher` + `StringMatcher` types are imported directly into `RuntimePermission::Header` and `RuntimePrincipal::Header`. No matcher logic is duplicated; the runtime calls the existing `HeaderMatcher::matches(headers: &[Header]) -> bool` method (or equivalent — the state-2 PLAN-writer verifies the exact method name and call shape at PLAN-write time).

### D4 — `HttpFilterInstance::Rbac` variant + dispatch

Extend `crates/envoy-filter/src/instance.rs::HttpFilterInstance` enum with a new variant `Rbac(RbacFilter)`. Extend the `build` dispatch (the new variant calls `RbacFilter::build_from_config(cfg, registry, hcm_stat_prefix)`) + `decode_headers` + `encode_headers` dispatch arms. New variant lands between `LocalRateLimit` and the `#[cfg(feature = "test-util")]` block (mirroring the 09 D4 placement precedent).

Re-export `RbacFilter` from `crates/envoy-filter/src/lib.rs::pub use rbac::RbacFilter;`.

**HCM `stat_prefix` threading widening (signpost):** the upstream-Envoy RBAC filter's stats namespace `http.<hcm_stat_prefix>.rbac.*` requires the HCM's `stat_prefix` to be threaded into `RbacFilter::build_from_config`. The 09 D4 widening threaded `&Arc<StatsRegistry>` through `FilterPipeline::build_from_config`; phase-10 widens further to thread `hcm_stat_prefix: &str` alongside. The H1 + H2 HCM filter-chain wiring sites already hold the `stat_prefix` field (per 06.1 envoy-config schema landing); passing it to `FilterPipeline::build_from_config` is one additional argument at two sites (H1 + H2). Mirrors 09's signature widening discipline.

### D5 — ADR-0033 Consequences amendment: closes 09 REVIEW M2

The 09 REVIEW M2 deferred to "next HTTP-filter-family phase exercising filters on H2" with **preferred close shape (a): doc amendment to ADR-0033 Consequences + PROGRESS Commit C forward-looking note**. Phase 10 IS that phase; D5 lands the close as a docs-only deliverable:

- **At `docs/envoy-rust/DECISIONS.md` ADR-0033 Consequences §iii(c)-end** (~line 697; the bullet currently reading "the corresponding H2 HCM path naturally inherits via the shared Http1HCMConfig re-export"): insert a clarifying paragraph immediately after that bullet recording the empirical H2 analogous gap — namely that `crates/envoy-http2/src/hcm.rs:373-378` (decode-side `H2RequestPath::SynthFromDecode`) and `crates/envoy-http2/src/hcm.rs:436-443` (encode-side `Decision::StopAndSend(replacement)`) both return the filter's response verbatim through `build_http_response` at `crates/envoy-http2/src/response.rs:29-50` which does NOT add `server`/`date`/`content-type`. The amendment names the close site: "next HTTP-filter-family phase exercising filters on H2 lands a `decorate_filter_synth_response_h2` analogue (~50-70 LoC + 2 tests; symmetric to the H1 helper landed at Commit C)". ~10-15 LoC docs edit.
- **At `docs/envoy-rust/phases/09-http-filter-local-rate-limit/PROGRESS.md`** (Commit C subsection): append a 1-sentence forward-looking note cross-referencing the DECISIONS.md amendment for the H2 deferral disposition.

D5 is mechanical docs-only (~12-15 LoC across 2 files). The PLAN-writer co-locates D5 with D7 (BEHAVIOR_CONTRACT extension) at the same task — both are docs-only contract authorship work and naturally compose. **The carryforward closure narrative for 09 REVIEW M2 attributes the close to the D5-landing task commit.**

**Per D-3.5 ADRs-are-append-only:** the amendment is a clarification of the existing Consequences narrative, NOT a Decision change. ADR-0033's Decision + Rationale + Provenance + Options-considered stand verbatim. The amendment adds a new paragraph immediately after the existing iii(c) bullet without rewriting the bullet's content. The state-2 PLAN-writer may alternatively choose to land a NEW ADR-0034 superseding ADR-0033's narrow iii(c) claim (the doctrinally-cleaner route — see §7 below); the recommended posture is in-place amendment (the change is a clarification, not a Decision shift).

### D6 — Stats wiring (2 counters per upstream-Envoy parity)

At `RbacFilter::build_from_config`, register two `Counter` handles against the `Arc<StatsRegistry>`:

- `format!("http.{hcm_stat_prefix}.rbac.allowed")` → `allowed_counter: Arc<Counter>`
- `format!("http.{hcm_stat_prefix}.rbac.denied")` → `denied_counter: Arc<Counter>`

**Namespace empirical verification signpost:** the `http.<hcm_stat_prefix>.rbac.{allowed,denied}` namespace shape is the recommended state-1 projection per the 06.1 stats convention. **The state-2 PLAN-writer empirically verifies the exact namespace against `envoyproxy/envoy:v1.33.0` + admin `/stats` scrape** before locking the namespace in PLAN lock-ins. If reality differs (e.g., upstream uses `http_rbac.<...>` like LocalRateLimit, OR uses per-policy stats under `track_per_rule_stats`), the SPEC §2.1 revision lands via an ADR at PLAN-write time per ADR-0033 process-gap doctrine.

Increment sites (all within `decode_headers`):

- `self.allowed_counter.inc()` — on RBAC ALLOW decision (computed per §5.6 below), before returning `Decision::Continue`.
- `self.denied_counter.inc()` — on RBAC DENY decision, before constructing the `Decision::StopAndSend(FilterResponse)` 403.

The 06.x stats convention applies: `StatsRegistry::register_counter` is idempotent for same-name re-registration. BEHAVIOR_CONTRACT extension (§2.1 above) lands at the same task commit as D6 per the 07.x cadence.

### D7 — BEHAVIOR_CONTRACT.md extension

Two extensions land at the task commits where they're empirically exercised (NOT at SPEC time):

- **D7.1 — `Stat-name mapping` 2 rows** land at the D6 stats-wiring task commit.
- **D7.2 — ADR-0033 Consequences amendment** lands at the D5 task commit (co-located with D5; see D5 above).

No new Header allow-list row needed per §2.2.

### D8 — Fixture + harness extension + fuzz seed + in-process backstop

- **D8.1 — Fixture `tests/fixtures/0017-http-filter-rbac/`.** Reuses fixture 0007's HCM + `direct_response` bootstrap shape so the bilateral assertion focuses on the filter, not on upstream proxy complexity. Bootstrap shape (sketch):

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
            - name: envoy.filters.http.rbac
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.http.rbac.v3.RBAC
                rules:
                  action: ALLOW
                  policies:
                    "pass_with_header":
                      permissions:
                      - any: true
                      principals:
                      - header:
                          name: x-rbac-pass
                          string_match: { exact: yes }
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

  Probe shape: `Driver::Http1ProbeList` (existing harness primitive from 04.2) with 4 sequential probes (`GET /`). Expected per-probe statuses: `[403, 200, 403, 200]` corresponding to (no header / `x-rbac-pass: yes` / `x-rbac-pass: no` / `x-rbac-pass: yes`). Asserts probes 1 and 3 (403 responses) carry the 5 standard HTTP/1.1 headers via `decorate_filter_synth_response` (set-equal-modulo-allow-list per the 04.1-landed `server` + `date` rows) + body `"RBAC: access denied"` (19 bytes; NO trailing newline per **ADR-0034**) byte-exact.

  Docker-gated wrapper at `tests/differential/tests/http_filter_rbac.rs` mirroring `tests/differential/tests/http_filter_local_rate_limit.rs` shape (the 09 precedent). One `#[tokio::test]` `http_filter_rbac_fixture` invoking `run_fixture("0017-http-filter-rbac").await`.

  **No new harness extensions required.** `Driver::Http1ProbeList` exists from 04.2 + carries forward through 09; `Http1Probe.expected_headers` allow-list discipline carries from 04.x + 09. The PLAN-writer verifies the harness's per-probe `request_headers` field supports the 4 distinct header sets needed (likely already present per 04.2's full-fan-out matcher fixtures).

- **D8.2 — Fuzz corpus seed.** New file `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_rbac_filter.yaml` containing the bootstrap shape above (or a minimal variant). Mirrors the 07.2 `hcm_header_mutation_filter.yaml` + 09 `hcm_local_rate_limit_filter.yaml` precedent. Extends the fuzz target's seed coverage from 16 to 17 entries (one per fixture's bootstrap shape). Includes the `crates/envoy-config/fuzz/.gitignore` allow-list extension AND the `crates/envoy-config/src/bootstrap.rs::tests::fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS-array extension (per the 09 Task 6 follow-up precedent — both files must be edited together).

- **D8.3 — In-process backstop.** New file `crates/envoy-bin/tests/http_filter_rbac.rs` mirroring `crates/envoy-bin/tests/http_filter_local_rate_limit.rs` (09 precedent) — **but with the 09 REVIEW M3 fix applied directly**: use `tokio::process::Command` (NOT `std::process::Command`); set `.kill_on_drop(true)` on the subprocess builder; route stdout to `Stdio::null()` (NOT `Stdio::piped()`) to avoid OS-pipe-buffer-fill deadlock. Stderr may stay `Stdio::piped()` per the 07.2/08.2 precedent (for diagnostic capture on test failure) AS LONG AS a concurrent drain reader is spawned (or `Stdio::null()` is used uniformly). This adoption closes 09 REVIEW M3 at the D8.3-landing task commit.

  Single `#[tokio::test]` exercising RBAC semantics in-process (no Docker). The test boots `envoy-bin` with a synthesized bootstrap (1 policy, allow-action, header-principal); issues 4 sequential `GET /` requests against the bound listener with varying `x-rbac-pass` header values; asserts the status sequence `[403, 200, 403, 200]` + body `"RBAC: access denied"` (19 bytes per ADR-0034) on 403 probes + body `"ok\n"` on 200 probes + presence of 5 standard HTTP/1.1 headers via `decorate_filter_synth_response` on 403 probes.

---

## 4. Out of scope (deferred non-goals)

Phase 10 explicitly does NOT land:

- **Per-route `typed_per_filter_config` for RBAC.** The 10 filter's config is sourced exclusively from the filter-chain-level entry. Per-route policy variation defers to whichever future HTTP-filter-family phase first needs it. CORS is the natural close site per upstream Envoy's per-route CorsPolicy pattern. Same deferral as phase 09 §4.
- **`Action::Log` enum variant.** Phase 10 ships only Allow + Deny actions. Log (which always permits the request but emits a log entry per policy match/no-match) defers to whichever later phase first needs the audit-only-no-enforcement semantic.
- **`shadow_rules`, `shadow_rules_stat_prefix`, `track_per_rule_stats` fields.** Phase 10 ships only the primary `rules` field. Shadow rules (which evaluate alongside primary rules but only LOG the would-be outcome; enable safe policy rollout) defer to a future RBAC enrichment phase. Per-rule stats (one allowed/denied pair per policy under `track_per_rule_stats: true`) defer to the same future phase. `shadow_allowed` + `shadow_denied` stat counters defer accordingly.
- **`condition` and `checked_condition` fields on `Policy` (CEL expressions).** CEL evaluator is a substantial primitive (~1500+ LoC for a minimum-viable subset). Defers indefinitely; whichever future RBAC enrichment phase first needs CEL-conditioned policies lands the evaluator via foundation grant (likely `cel-rust` or hand-roll).
- **`Permission::UrlPath` + `Principal::UrlPath` (URL path matchers).** Defers to whichever later RBAC phase first needs path-based policies. May reuse the 04.x `StringMatcher` directly if the upstream proto's `PathMatcher` is essentially a wrapped StringMatcher.
- **`Permission::DestinationIp`, `Permission::DestinationPort`, `Permission::DestinationPortRange`.** Requires IP/CIDR matcher primitives (not currently present in envoy-config — 04.x has socket addresses but no CIDR/range matching). Defers to whichever RBAC enrichment phase OR upstream-robustness phase first needs CIDR matching.
- **`Principal::SourceIp`, `Principal::DirectRemoteIp`, `Principal::RemoteIp`.** Requires the same IP/CIDR matcher primitive. Defers per the destination_ip deferral.
- **`Principal::Authenticated` (mTLS peer-cert principal).** Requires mTLS engagement + peer-cert attribution framework — both deferred per the standing 13-`x509-parser` carryforward (still open per STATE.md "Earlier-phase carryforwards"). Defers indefinitely; whichever later phase first needs mTLS attribution lands the framework.
- **`Permission::RequestedServerName`, `Permission::RequestedServerNameMatcher`.** Requires SNI-extraction at the filter layer (currently SNI is handled at the rustls TLS handshake layer per 03.x architectural choice; not propagated to the HCM filter). Defers to whichever phase first needs SNI-in-filter.
- **`Permission::UriTemplate`.** URI-template matching (RFC 6570-ish) is a substantial primitive. Defers indefinitely.
- **`Principal::Metadata`, `Permission::Metadata` (dynamic metadata matchers).** Defers until whichever phase first lands the dynamic-metadata framework (extension framework for filter-set/filter-read metadata).
- **`Principal::FilterState` (per-stream filter state matcher).** Defers until whichever phase first lands the per-stream filter-state framework.
- **Custom denial response body / headers.** Phase 10's 403 response body is the upstream-Envoy v1.33 source-hardcoded `"RBAC: access denied"` (19 bytes; no trailing newline per ADR-0034). Operator-configurable denial bodies defer indefinitely.
- **Custom HTTP status codes for denial.** Phase 10 emits 403 unconditionally. Whichever later RBAC enrichment phase first needs a configurable denial status lifts the validator constraint.
- **`envoy.filters.network.rbac` (network-layer RBAC).** Defers to the Network filters family per `BOOTSTRAP_PROMPT.md` §9.
- **H2 differential fixture coverage for RBAC.** Phase 10's fixture is H1 only per the established cadence. Per the D5 amendment (09 REVIEW M2 close), the next HTTP-filter-family phase exercising filters on H2 lands the H2 fixture + the H2 HCM `decorate_filter_synth_response_h2` analogue.

---

## 5. Architectural invariants

Phase 10 honors and extends the established cross-crate invariants:

### 5.1 Crate boundaries

- **`envoy-filter` stays sole-dep-owner of HTTP filter chain iteration.** All new variant + filter implementation land in `crates/envoy-filter/`. No new top-level crate is created. No new workspace member.
- **`RbacFilter` lives at `crates/envoy-filter/src/rbac.rs`.** Mirrors the 07.2 `header_mutation.rs` + 09 `local_rate_limit.rs` placement pattern (one module per concrete filter).
- **No new path-deps within `envoy-filter`.** The `envoy-stats` path-dep landed at phase 09; phase 10 reuses it. The `envoy-config` path-dep already exists. Phase 10 adds zero new workspace path-deps; the 04.1 REVIEW M5/M9 Cargo.lock cadence carries forward unchanged.

### 5.2 Hand-rolled tree-walk evaluator per D-3.2

The Permission/Principal tree-walk evaluator is hand-rolled per **D-3.2**'s *"Every individual filter ... Must be written from scratch"* doctrine. The implementation uses only **std-lib + bytes + envoy-config (HeaderMatcher reuse) + envoy-stats** — all D-3.2-permitted; all already pulled. Pure-compute recursive descent on `RuntimePermission` / `RuntimePrincipal` enums; no I/O; no async.

**Explicit non-grants:** no `cel-rust`, `cel-interpreter`, `serde_with`, `regex-automata` (beyond what `regex` already provides per ADR-0021), `ipnet`, `cidr` — none on D-3.2's permitted-foundations list; none required by the 10 scope (per the §4 deferrals on CEL conditions, IP/CIDR matchers, etc.). The state-3 implementer must NOT pull any of these (any pull forces a foundations-grant ADR per D-3.5).

### 5.3 No new top-level Cargo deps

The recommended no-foundations-grants posture per phase 09 + parent-08 + parent-07 SPEC §7 carries forward through phase 10. **If the state-3 implementer surfaces a genuine foundation need at execution time, a foundations-grant ADR lands per D-3.5 — see §7 for the conditional-ADR slots.**

### 5.4 Decode-only filter

`RbacFilter::encode_headers` is a no-op (returns `Decision::Continue` unconditionally). The filter operates only on the decode-side request flow. This matches upstream Envoy v1.33's documented `envoy.filters.http.rbac` semantic (the filter does not consult the response). The encode-side method exists on the `HttpFilterInstance` enum's dispatch arm per the 07.x framework symmetry, but never mutates the response.

### 5.5 Filter-chain config only (NOT per-route)

The sole config source is the filter-chain-level entry per the 07.2 `header_mutation` + 09 `local_rate_limit` precedent. Per-route `typed_per_filter_config` defers per §4 above. Whichever future HTTP-filter-family phase first needs per-route policy variation extends the 07.x framework with a per-route lookup primitive — the new primitive is the gating architectural change, NOT the filter that consumes it. Same posture as phase 09 §5.5.

### 5.6 RBAC decision semantic (cross-proxy deterministic)

The RBAC decision is the cross-proxy semantic invariant. Per upstream Envoy v1.33:

- **`action: ALLOW`**: if ANY policy's permissions AND principals all match the request, ALLOW. Otherwise DENY (default).
- **`action: DENY`**: if ANY policy's permissions AND principals all match the request, DENY. Otherwise ALLOW (default).

A policy "matches" iff (a) at least one permission in `policy.permissions` evaluates true on the request AND (b) at least one principal in `policy.principals` evaluates true. (Note: this is permissions-OR-list + principals-OR-list semantic at the top of each Policy; AND-list semantics live inside `Permission::AndRules` / `Principal::AndIds` set wrappers.)

**Policy iteration is deterministic across both proxies** because:
- `RbacConfig.rules.policies: BTreeMap<String, Policy>` — alphabetical iteration order.
- Per-policy permission iteration is `Vec` insertion order.
- Per-policy principal iteration is `Vec` insertion order.
- Short-circuit on first match within a policy (permissions: any true → match; principals: any true → match).
- Short-circuit on first matching policy (no need to evaluate remaining policies once the action verdict is determined).

The two stat counters are incremented EXACTLY ONCE per request (per the §2.1 functional dependency `allowed + denied == total_requests_to_filter`). No double-counting under any policy combination.

### 5.7 Statelessness across requests

The `RbacFilter` carries no per-request state. The policy tree is immutable post-`build_from_config`; the runtime tree-walk evaluator is pure-compute on the request alone. Per `Clone` of `HttpFilterInstance` (the `derive(Debug, Clone)` on the enum), each per-request pipeline-clone shares the underlying `Arc<Vec<RuntimePolicy>>` and the two `Arc<Counter>` handles — no clone cost beyond Arc reference bumps.

### 5.8 H1 + H2 symmetric (filter-layer only; H2 codec fixture deferred)

The filter operates on the codec-agnostic `FilterRequest` / `FilterResponse` abstraction per the 07.1 framework + ADR-0031. Both H1 and H2 HCM dispatch sites invoke `pipeline.decode_headers` at the established 07.x integration seam — no per-codec branching for phase 10. Fixture 0017 exercises H1 only (single fixture per the established cadence; H2 fixture coverage defers per the 09 REVIEW M2 disposition + D5 amendment); the in-process backstop D8.3 also exercises H1 only.

The 09 REVIEW M2 carryforward (H2 HCM filter-synth header decoration gap) is closed via D5 doc-amendment, NOT via implementation. Phase 10's RBAC filter, IF run on an H2 listener with filter-synth 403 responses, would surface the same standard-header omission documented in 09 REVIEW M2 — but phase 10 deliberately does not exercise that path bilaterally. The full H2 close (implementation of `decorate_filter_synth_response_h2`) defers to whichever future HTTP-filter-family phase first exercises filters on H2.

### 5.9 ADR-0033 H1 HCM helper reuse

Phase 10's 403 emission flows through the existing H1 HCM `decorate_filter_synth_response` helper landed at phase-09 ADR-0033 Commit C `ae2cef0`. The filter emits `FilterResponse { status: 403, reason: Some("Forbidden"), headers: vec![], body: Bytes::from_static(b"RBAC: access denied") }` (19 bytes per **ADR-0034** state-2 §6.2 empirical verification); the H1 HCM's `RequestPath::SynthFromDecode` arm + encode-side `Decision::StopAndSend` arm invoke the helper, which decorates the 5 standard HTTP/1.1 response headers onto the response before write. No new H1 HCM helper code lands at phase 10 — pure reuse of the 09-landed primitive. This is the first non-LocalRateLimit filter to exercise the helper; validates the helper's filter-agnostic design.

---

## 6. Implementation signposts for the planner

The state-2 PLAN-writer reads this section to drive PLAN structure.

### 6.1 Split-gate evaluation (read first)

Per `BOOTSTRAP_PROMPT.md` §6.1, the state-2 PLAN-write evaluates whether the PLAN exceeds ~25 numbered tasks OR ~1500 LoC. Phase 10's surface estimate at SPEC time:

- D1 — envoy-config schema (~200 LoC + ~150 LoC unit tests). ~1 task.
- D2 — envoy-config validator (~100 LoC + ~120 LoC unit tests). ~1 task or co-located with D1.
- D3 — RbacFilter runtime + recursive tree-walk evaluator (~280 LoC + ~250 LoC unit tests). ~1-2 tasks.
- D4 — HttpFilterInstance::Rbac variant + dispatch + HCM stat-prefix widening (~50 LoC + ~30 LoC tests). ~1 task.
- D5 — ADR-0033 Consequences amendment (~15 LoC docs). Co-located with D7 in 1 task.
- D6 — stats wiring (~50 LoC + ~50 LoC tests). Co-located with D3.
- D7 — BEHAVIOR_CONTRACT.md extensions (~20 LoC contract edits) + ADR-0033 amendment co-location. ~1 task with D5.
- D8.1 — fixture 0017 (~100 LoC YAML + ~40 LoC Docker-gated wrapper). ~1 task.
- D8.2 — fuzz corpus seed (~30 LoC YAML + 2 file edits). ~1 task.
- D8.3 — in-process backstop with 09 M3 fix (~180 LoC). ~1 task.
- State-4 verification + STATE-advance (~docs). ~1 task.

**SPEC-time projection: ~9-11 tasks; ~1100-1300 LoC** (production ~530, tests ~600, fixture/doc ~200). The phase is comfortably **under** the split-gate threshold on both dimensions. **Recommended posture: single-phase (no split).** State-2 PLAN-write lands a standalone `PLAN.md` per the 04.3 / 05.1 / 06.x / 07.x / 08.x / 09 cadence.

**If state-3 surfaces unexpected complexity** (e.g., the empirical-verification at PLAN-write reveals the upstream RBAC stats namespace differs materially from the projection, requiring schema rework): the in-execution release valve is per-step commit splitting recorded in PROGRESS (per the 06.x / 07.x / 08.x / 09 precedent), NOT a phase-level nest-split. Per parent-08 SPEC §6.1 alternative (vi).

### 6.2 Empirical verification at state-2 PLAN-write (process-gap awareness per ADR-0033)

Per ADR-0033's Provenance section + the awareness-only doctrine note in STATE.md "Phase-09 ADR ledger (final)": state-1 brainstorming should empirically verify upstream wire shapes for novel filter surfaces. **The state-2 PLAN-writer is the natural locus** for this verification, since PLAN-write is where lock-ins are baked in. The PLAN-writer's empirical-verification scope:

1. **Stats namespace shape**: run `envoyproxy/envoy:v1.33.0` with the §3 D8.1 canonical bootstrap; trigger both an allowed and a denied request; scrape `/stats` from the admin endpoint; record the exact stat names. Update SPEC §2.1 + D6 if the projection (`http.<hcm_stat_prefix>.rbac.{allowed,denied}`) is wrong.
2. **403 response body shape**: same Docker run; observe the 403 response body bytes on a denied request; record exact byte sequence. Update SPEC §2.2 + D8.1 if the projection (`"RBAC: access denied\n"`) is wrong.
3. **403 response header set**: same Docker run; observe the exact 4-5 response headers Envoy v1.33 emits on RBAC denial. Confirms compatibility with ADR-0033's `decorate_filter_synth_response` decoration set.

Each finding lands as a PLAN lock-in (the established 06.x / 07.x / 08.x / 09 PLAN-write cadence). If any finding differs materially from the SPEC projection, the lock-in records the divergence + the SPEC §X.Y revision via an inline ADR at PLAN-write time (mirrors the 05.1 / 05.4 / 09 Task-1-fixup ADR-inline precedent). **Recommended posture: empirically verify all 3 at PLAN-write; land any necessary corrections via inline ADRs at the state-2 PLAN-write commit** — avoid the phase-09 process gap that surfaced ADR-0033 only at Task 5 dispatch.

### 6.3 D5 (09 REVIEW M2 amendment) lands co-located with D7

D5 is mechanical docs-only (~12-15 LoC across DECISIONS.md + PROGRESS.md). D7 is also docs-only (~20 LoC BEHAVIOR_CONTRACT). Co-locating them in one task minimizes commit count and groups all docs-only contract authorship. **Recommended.**

### 6.4 D8.3 (09 REVIEW M3 fix adoption) — direct close at Task 7

The 09 REVIEW M3 disposition explicitly names "next HTTP-filter-family phase touching `crates/envoy-bin/tests/http_filter_*.rs` backstops" as the close site. Phase 10 IS that phase; D8.3 closes M3 directly by adopting the 07.2 + 08.2 `tokio::process::Command + kill_on_drop(true) + Stdio::null()` discipline from the start (NOT regressing to `std::process::Command` as phase 09's Task 7 did). The state-3 implementer reads 09 REVIEW M3 + the 07.2 + 08.2 backstop precedents before writing D8.3 — verifies the precedent shape via direct code-spot-check (per the awareness-only doctrine note in phase-09 REVIEW M3's Process note: "PROGRESS narratives that cite a precedent for an IMPORTANT-track finding should verify the precedent shape via direct code-spot-check before claiming 'same pattern'").

### 6.5 The 06.x stats convention

Per 06.x cadence: StatsRegistry registration at `RbacFilter::build_from_config` time. Per-filter-instance ownership of 2 Counter handles. Stat-name namespace `http.<hcm_stat_prefix>.rbac.{allowed,denied}` (subject to §6.2 empirical verification) matches upstream Envoy v1.33 parity exactly (no project-internal label divergence).

### 6.6 The 07.x BEHAVIOR_CONTRACT extension cadence

Per the established 06.x / 07.x / 08.x / 09 doctrine: contract extensions land at the TASK where each is first empirically exercised, NOT at PLAN-write time and NOT at state-1 SPEC time. For phase 10:

- The 2 Stat-name mapping rows land at the D6 task commit (where the 2 counters are first registered + first incremented in tests).
- The ADR-0033 Consequences amendment lands at the D5 task commit (co-located with D7).

### 6.7 Pre-state-4 fmt discipline (continues per 06.1 R-9)

Per-task PROGRESS sections quote `cargo fmt --all -- --check` at every PROGRESS-task close, NOT just at state-4. Carries forward from the 06.1 → 06.2 → 06.3 → 07.1 → 07.2 → 08.1 → 08.2 → 09 chain.

### 6.8 State-4 evidence-discipline (continues per 05.3 → 06.x → 07.x → 08.x → 09 chain)

Per-gate quoted evidence in PROGRESS at the state-4 verification task: real CI run URL + HEAD SHA + completion timestamp + per-gate quoted output (all 5 stable-toolchain gates + each Docker-gated fixture + h2spec_pass_rate_gate + parse_bootstrap fuzz iteration count).

### 6.9 Cargo.lock cadence

The phase-04.1 REVIEW M5/M9 (Cargo.lock cadence ratification ADR) carries forward unchanged through phase 10 — zero new top-level Cargo deps projected per §5.3 above. The `Cargo.lock` diff at the phase-10 reviewed range is expected to be empty (zero workspace-internal path-dep additions; `envoy-stats` already on `envoy-filter` from phase 09; `envoy-config` already on `envoy-filter` from earlier phases).

### 6.10 PROGRESS.md skeleton + Task 1 preamble land alongside PLAN.md at state-2

Per the 06.2 / 06.3 / 07.1 / 07.2 / 08.1 / 08.2 / 09 cadence. State-2 PLAN-write lands both `PLAN.md` + `PROGRESS.md` skeleton + Task 1 preamble in a single standalone pre-Task-1 commit.

### 6.11 Subagent-driven execution at state 3 (per `feedback_execution_style`)

The user's standing preference auto-memory `feedback_execution_style` ("default to subagent-driven-development; skip the two-option fork") applies at state 3. The state-2 PLAN-write organizes tasks for subagent-driven execution per the 06.x / 07.x / 08.x / 09 cadence (each task independent enough to dispatch in isolation; PROGRESS attestation per-task; in-phase recovery cadence if any task surfaces a code-quality-review-blocking finding). Per the phase-09 awareness-only doctrine note (cluster reviewer's process note on precedent-shape verification): subagents claiming "same pattern as previous phase" should verify the precedent shape via direct code-spot-check before the claim lands in PROGRESS.

---

## 7. ADR projection

**Recommended posture: NO new ADRs land in phase 10.** The work fits inside the existing permitted-foundations set per §5.2 + §5.3 above. The DECISIONS.md ledger head stays at **ADR-0033** through phase 10's state-1 (this) commit; the next-available number is **ADR-0034**.

Three conditional ADR slots stay reserved-available for state-2 / state-3 execution-time landing if reality forces them:

- **Conditional ADR-0034 (option A) — PLAN-write empirical-verification revision.** If the §6.2 empirical verification at state-2 PLAN-write reveals one or more of (stats namespace shape / 403 body bytes / 403 header set) materially differs from this SPEC's projection, ADR-0034 lands at the state-2 PLAN-write commit per the established 05.1 / 05.4 / 09 inline-ADR-at-Task-1 precedent. **Recommended posture: empirically verify all 3 at state-2 PLAN-write; if any differ, land ADR-0034 inline at the PLAN-write commit per the ADR-0033 process-gap-awareness doctrine.** This avoids the phase-09 process gap that surfaced ADR-0033 only at Task 5 subagent dispatch.

- **Conditional ADR-0034 (option B) — per-route filter config primitive.** If the state-2 PLAN-writer concludes that the per-route `typed_per_filter_config` deferral (per §4 + §5.5 above) warrants append-only durability NOW (rather than at whichever future filter actually needs per-route config), ADR-0034 lands at the state-2 PLAN-write commit recording the family-wide deferral pattern + the named close site. **Recommended posture: defer the ADR until a future filter actually needs per-route config (CORS is the natural close site per upstream Envoy's per-route CorsPolicy pattern); the 10 deferral is doctrinally clear without an ADR per phase 09's identical posture.**

- **Conditional ADR-0034 (option C) — foundations grant.** No grant projected. If state-3 surfaces a materially-worse-than-foundation result for the tree-walk evaluator (e.g., a need for `serde_with`, `ipnet`, or similar), ADR-0034 lands at the surfacing task. **Recommended posture per §5.2: no grant.** The std-lib + existing-workspace-internal deps are sufficient for the v1.33 documented RBAC surface narrowed to phase-10 scope.

- **Conditional ADR-0034 (option D) — D5 amendment shape.** If the state-2 PLAN-writer or state-3 implementer prefers landing a NEW ADR-0034 superseding ADR-0033's narrow iii(c) Consequences claim (rather than amending ADR-0033 in-place per D5's recommended shape), ADR-0034 lands at the D5 task commit. **Recommended posture per D5: amend in-place** (the change is a clarification, not a Decision shift; ADR-0033's Decision + Rationale + Provenance stand verbatim).

At most ONE of options A/B/C/D can land at any single commit (per D-3.5 sequential ADR numbering); if multiple fire, the second one becomes ADR-0035 etc. If none fire, the ledger stays at ADR-0033 through phase 10.

---

## 8. State-machine signposts for the phase-10 state-2 session

The next session (state 2) reads this section and acts.

- **Lifecycle state at session start:** State 2 (SPEC.md exists; PLAN.md does not).
- **Skill:** `superpowers:writing-plans` per `BOOTSTRAP_PROMPT.md` §5 state 2.
- **Output:** `docs/envoy-rust/phases/10-http-filter-rbac/PLAN.md` + `PROGRESS.md` skeleton + Task 1 preamble (standalone pre-Task-1 commit per the 04.3 / 05.1 / 06.x / 07.x / 08.x / 09 PLAN-write cadence).
- **Empirical verification at state 2 (per §6.2):** Run `envoyproxy/envoy:v1.33.0` Docker against the §3 D8.1 canonical bootstrap; verify (stats namespace shape / 403 body bytes / 403 header set) before locking PLAN lock-ins. If any differs from the SPEC projection, land an inline ADR-0034 at the state-2 PLAN-write commit per the 05.1 / 05.4 / 09 precedent.
- **Split-gate evaluation:** §6.1 above. **Recommended: single-phase (no split).** PLAN materializes ~9-11 tasks / ~1100-1300 LoC; well under the §6.1 ~25-task / ~1500-LoC gate.
- **ROADMAP row flip:** at state-2 PLAN-write, flip ROADMAP row `10` `planned` → `in-progress` (per the 08.1 / 08.2 / 09 precedent — new rows added at `planned`; flipped to `in-progress` at state-2 PLAN-write OR state-3 first task commit). The 10 row is added at THIS state-1 commit with `status: planned`.
- **D5 (09 REVIEW M2 amendment) ordering:** §6.3 above. **Recommended: co-located with D7 (BEHAVIOR_CONTRACT extension) in a single task.**
- **D8.3 (09 REVIEW M3 fix) discipline:** §6.4 above. **Required at Task 7 dispatch:** the in-process backstop uses `tokio::process::Command + kill_on_drop(true) + Stdio::null()` from the start; NOT `std::process::Command`. Direct code-spot-check the 07.2 + 08.2 precedents before writing.
- **Per-route filter config deferral ADR:** §7 option B above. **Recommended: defer the ADR.** If the PLAN-writer disagrees, ADR-0034 (option B) lands at state 2 alongside PLAN.md.
- **PLAN-time SPEC corrections:** the PLAN-writer reads this SPEC against HEAD `<state-1-commit-SHA>` and flags any drift (mechanical signature differences between the SPEC's projected types and the actual on-disk envoy-config/envoy-filter types — e.g., the exact `HeaderMatcher` method names, the exact `FilterRequest` field shape, the exact `Decision` enum variants). Per the 06.2 → 06.3 → 07.x → 08.x → 09 precedent ("7 PLAN-write SPEC corrections at 09 PLAN-write" pattern), corrections land in the PROGRESS Task 1 preamble.

---

## 9. Commit message format (for state 6 of the phase-10 lifecycle)

```
phase 10: envoy.filters.http.rbac + fixture 0017 + 09 REVIEW M2 + M3 close

<1-3 sentence summary>

Differential surface: fixture 0017-http-filter-rbac; all 17 Docker-gated fixtures (0001-0017) green simultaneously at CI run <ID> HEAD <SHA>.
Conformance: h2spec ≥95% gate held at parent-05 baseline; no H2-framing surfaces engaged.
```

If ADR(s) land, the bracketed list is appended to the title per `BOOTSTRAP_PROMPT.md` §5.3 (e.g., `... [ADR-0034]`). Per the recommended no-ADRs posture per §7, the bracketed list is omitted by default.

If phase 10 unexpectedly splits at state-2 into 10.1 + 10.2 (NOT recommended; see §6.1), the closing-sub-phase commit carries `[parent 10 done]` per the 07.2 / 08.2 / 09 closing-sub-phase precedent (though phase 09 was standalone; the closest split-precedent is 08.2's parent-08 close).

---

## 10. State-machine commit (this commit — phase-10 state-1 close-out)

This SPEC is the state-1 output. The state-1 close-out commit is **docs-only** and touches:

- **CREATE** `docs/envoy-rust/phases/10-http-filter-rbac/SPEC.md` (this file).
- **MODIFY** `docs/envoy-rust/ROADMAP.md` — **adds a new row** beneath the existing "HTTP filters family" §9 heading, immediately after the existing phase-09 row. Row format per the schema: `| id | title | depends-on | status | sub-phases | summary |`. New row content:
  ```
  | 10 | envoy.filters.http.rbac + fixture 0017 + 09 REVIEW M2 + M3 close | 07 | planned | — | fixture 0017-http-filter-rbac green; envoy-filter gains RbacFilter (hand-rolled recursive tree-walk evaluator + Allow/Deny actions + Any/Header/AndRules/OrRules/NotRule Permission + Any/Header/AndIds/OrIds/NotId Principal + decode-side StopAndSend with 403 + body "RBAC: access denied\n" via ADR-0033 H1 HCM decorate_filter_synth_response helper) + HttpFilterInstance::Rbac variant; envoy-config gains Rbac + Policy + Permission + Principal schema + ~5 new ConfigError variants; 09 REVIEW M2 closed (ADR-0033 Consequences amendment per preferred close shape (a)) + M3 closed (Task 7 backstop tokio::process::Command + kill_on_drop discipline) |
  ```
  The "HTTP filters family" heading itself stays unchanged; the new row joins beneath the existing phase-09 row per `BOOTSTRAP_PROMPT.md` §4.1 invariant 2 (append-only history; never delete rows). All other ROADMAP rows untouched.
- **MODIFY** `docs/envoy-rust/STATE.md` — advances "Active phase" pointer from `_none_ — awaiting next planning` to:
  - `id: 10`
  - `slug: 10-http-filter-rbac`
  - `directory: docs/envoy-rust/phases/10-http-filter-rbac/`
  - `status: phase 10 lifecycle state 1-complete / state-2-next (SPEC.md landed; PLAN.md does not exist)`
  
  Rewrites "Next expected skill" to `superpowers:writing-plans` scoped to this SPEC. Rewrites "Last commit" + "Last updated". Appends a new "Phase-10 state-1 brainstorm" subsection in Notes recording the family-pick + first-filter-pick rationale + alternatives considered + the 5-dimension scoring. Preserves all prior "Phase-NN rollovers" + "Phase-NN state-1 brainstorm" + "Phase-NN state-2 split" + "Phase-NN state-2 PLAN-write" + "Phase-NN state-3 execution arc" + "Phase-NN ADR ledger" subsections verbatim per D-3.5 (append-only) + D-3.4 (context isolation).

No code changes, no fixture changes, no Cargo.toml changes, no DECISIONS.md changes, no BEHAVIOR_CONTRACT.md changes. The DECISIONS.md ledger head stays at **ADR-0033**. ENVOY_TARGET.md + rust-toolchain.toml untouched (D-3.7 / D-3.9 unchanged).

**Commit message:**

```
phase 10: state-1 brainstorm — http-filter-rbac SPEC.md (HTTP-filter-family second phase; 09 REVIEW M2 + M3 named close sites)
```

Per the project precedent (phase-09 state-1 brainstorm commit `3025594` title shape — `phase 09: state-1 brainstorm — http-filter-local-rate-limit SPEC.md (HTTP-filter-family first phase; 07.2 REVIEW M1 named close site)`), state-1 brainstorm commit titles are descriptive with parenthesized scope summary. No `[ADR-NNNN]` brackets — no ADR lands at this commit.

**Predecessor:** `518140c` — phase 09 state-6 close-out (the most-recent commit; docs-only state-6 close-out per the standalone-phase invariant — phase 09 was standalone, NOT a sub-phase, so no parent-row close-out fold was needed).

**Origin/main:** `518140c`. Local + origin are in sync as of THIS state-1 brainstorm commit's prologue. After landing, the docs-only edits push to origin and the next CI run re-validates the docs-only edits compile cleanly through the 5 stable-toolchain gates + the parse_bootstrap fuzz target on the unchanged 16-seed corpus (predecessor docs-only CI runs took ~2-3m).

---

*End of SPEC. Phase 10 state-1 lifecycle complete on landing. The next session enters state 2 — writes PLAN.md per `superpowers:writing-plans`, performs the §6.2 empirical verification at PLAN-write, and evaluates the §6.1 split gate.*
