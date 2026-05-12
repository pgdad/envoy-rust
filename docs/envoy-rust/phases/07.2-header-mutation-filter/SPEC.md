# Phase 07.2 — `envoy.filters.http.header_mutation` filter + fixture 0013 + parent-07 close-out

- **Phase id:** `07.2`
- **Slug:** `07.2-header-mutation-filter`
- **Title:** First concrete pluggable filter — `envoy.filters.http.header_mutation` (Envoy v1.33 has this as a real filter at `envoy.extensions.filters.http.header_mutation.v3.HeaderMutation`) — extending the 07.1-landed `HttpFilterInstance` enum with a `HeaderMutation(HeaderMutationFilter)` variant + envoy-config schema additions (`HttpFilterTypedConfig::HeaderMutation` variant; `HeaderMutationConfig` / `Mutations` / `HeaderMutationEntry` / `HeaderValueOption` / `HeaderValue` / `AppendAction` structs; all with `#[serde(deny_unknown_fields)]`) + validator extension (per-entry: `header.key` non-empty + RFC 7230 token-set; `append_action` in supported subset; new `ConfigError::UnsupportedHeaderMutationAppendAction` variant) + fuzz corpus extension (`hcm_header_mutation_filter.yaml` seed at `crates/envoy-config/fuzz/corpus/parse_bootstrap/`) + new differential fixture `tests/fixtures/0013-http-filter-header-mutation/` (HeaderMutation in front of Router on a Router-proxied route to an Http1EchoBackend; appends `x-filter-stamp: phase-07` on request_mutations + `x-filter-response-stamp: phase-07` on response_mutations; bilaterally asserted — the Http1EchoBackend echoes received request headers into the response body, proving both decode-side stamp landed at backend AND encode-side stamp landed at client) + in-process backstop at `crates/envoy-bin/tests/http_filter_header_mutation.rs` + parent-07 state-6 close-out.
- **Depends on:** `07.1` (`07.1-filter-framework-foundation` state-6 commit must have landed). The `HttpFilterInstance` enum + `FilterPipeline` types + H1/H2 HCM filter-chain wiring all come from 07.1; 07.2 extends them with a single new enum variant and the concrete filter behavior. Sub-phase ordering invariant per parent-07 SPEC §5 + ADR-0030 — 07.2 cannot start before 07.1's state-6 close-out commit.
- **Seeded by:** Parent-07 SPEC at `docs/envoy-rust/phases/07-filter-chain-framework/SPEC.md` §3 deliverables D8.2 through D15.2, codified at the parent-07 state-2 split commit (this sub-phase SPEC's landing commit) via **ADR-0030** (the parent-07 2-way split decision). The eight D8.2-D15.2 deliverables are decomposed into per-task PLAN-ready cadence in §3 below; the projection is ~800 LoC across ~10 tasks per parent SPEC §5.
- **Differential surface when done:**
  - **Pre-existing fixtures:** `0001-tcp-echo` through `0012-access-log-file-sink` — all 12 stay green at the Docker-gated CI level simultaneously. The HeaderMutation schema additions + filter runtime + validator extension introduce no behavioral change on any existing fixture (which declares only `[Router]` in `http_filters`). The 13-fixture-green simultaneous CI run at state-4 is the structural proof of regression-equivalence.
  - **New fixture:** `tests/fixtures/0013-http-filter-header-mutation/` lands at Task 8. Exercises HeaderMutation in front of Router on a Router-proxied route to an `Http1EchoBackend` cluster. Both proxies emit the response header `x-filter-response-stamp: phase-07`; both backends see `x-filter-stamp: phase-07` (echoed back into the response body via the Http1EchoBackend's existing body-echo shape from 04.3). Differential equivalence is value-exact on both the response-side stamp (asserted in `expected_headers`) and the request-side stamp (asserted in `expected_body` via the echoed body bytes).
  - **Conformance suites unchanged:** `tests/conformance/h2spec/` continues at the **≥95% pass** gate landed in 05.2 D7 (99.31%). The HeaderMutation filter manipulates only request/response headers; framing surfaces are untouched.
- **Parent-07 close-out:** the 07.2 state-6 phase-done commit ALSO flips parent ROADMAP row `07` from `in-progress` to `done` per the ROADMAP-schema invariant in `BOOTSTRAP_PROMPT.md` §4.1 ("the parent flips to `done` only after all sub-phases are `done`"). Mirrors phase-02's `f04e21a`-shape close-out (closing sub-phase 02.2 also closed parent-02), phase-03's `ca81226`-shape close-out (closing sub-phase 03.2 also closed parent-03), phase-04's `e626862`-shape close-out (closing sub-phase 04.3 also closed parent-04), phase-05's `82c26b8`-shape close-out (closing sub-phase 05.3 also closed parent-05), and phase-06's `b918f33`-shape close-out (closing sub-phase 06.3 also closed parent-06).

This SPEC is the design contract for sub-phase 07.2. It refines parent-07 SPEC §3 deliverables D8.2-D15.2 to per-task PLAN-ready cadence. Each task as projected here is one numbered task in the standalone PLAN.md that this sub-phase's own state-2 session lands. A stranger reading only this file plus the stable doctrine documents (`MISSION.md`, `BEHAVIOR_CONTRACT.md`, `DECISIONS.md`, `BOOTSTRAP_PROMPT.md`), the parent-07 SPEC, the sibling 07.1 SPEC, and the 07.1 state-6 close-out commit's tree must be able to operate as 07.2's state-1-and-onward sessions — landing the standalone PLAN.md, executing the per-task work end-to-end, materializing the state-4 evidence, reaching the state-6 close-out commit (which also closes parent-07).

---

## 1. Goal and acceptance signal

**Goal.** Land the first concrete pluggable filter (`envoy.filters.http.header_mutation`) end-to-end through the 07.1-established framework, and prove via differential fixture 0013 that the framework's iteration semantics produce wire-equivalent output to upstream Envoy on both decode (request-side stamp at backend) and encode (response-side stamp at client) iteration states. Close parent-07 at this sub-phase's state-6 commit.

The HeaderMutation filter mirrors Envoy v1.33's `envoy.extensions.filters.http.header_mutation.v3.HeaderMutation`. Its config schema (the wire form that operators write in YAML) is:

```yaml
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
```

Runtime behavior: `decode_headers` applies `request_mutations` to the request header list (`Vec<(String, String)>`); `encode_headers` applies `response_mutations` to the response header list. `AppendAction::APPEND_IF_EXISTS_OR_ADD` pushes a new entry (allowing duplicates per RFC 7230 §3.2.2's list-valued header semantic); `AppendAction::OVERWRITE_IF_EXISTS_OR_ADD` does a case-insensitive remove-then-push (removes every existing entry with the same key, then pushes the new entry once).

**Iteration order under the 07.1 framework:**

- Filter chain: `[HeaderMutation, Router]` (declaration order in YAML).
- `decode_headers` (declaration order): `HeaderMutation::decode_headers` fires first (mutates request headers); `Router::decode_headers` fires second (no-op terminus). The route-match runs AFTER decode_headers (per parent-07 SPEC §6 Rule 7), so the route matcher sees the mutated request.
- `encode_headers` (reverse declaration order): `Router::encode_headers` fires first (no-op terminus); `HeaderMutation::encode_headers` fires second (mutates response headers).

This matches Envoy v1.33's documented filter-chain semantics — the HeaderMutation filter's `response_mutations` apply AFTER the Router has populated the response, which models Envoy's "response stamping is a post-Router operation" semantic.

**Acceptance signal — `BOOTSTRAP_PROMPT.md` §7.5 phase-done gate, scoped to sub-phase 07.2's surface:**

- **(a)** The new differential fixture `tests/fixtures/0013-http-filter-header-mutation/` is green at the Docker-gated CI level.
- **(b)** All 12 pre-existing differential fixtures (`0001-tcp-echo` through `0012-access-log-file-sink`) are still green simultaneously at the Docker-gated CI level under the SAME CI run that's green on fixture 0013.
- **(c)** `tests/conformance/h2spec/` continues to pass at **≥95%** with `known-failures.txt` unchanged.
- **(d)** The existing `parse_bootstrap` fuzz target runs clean for its short-budget CI run (`cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30`) against the corpus extended in Task 6 (≥1 new HCM `http_filters` block with the HeaderMutation typed_config). No new fuzz target ships in 07.2.
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, and `cargo deny check` are all clean on the stable-toolchain CI job.
- **(f)** `REVIEW.md` is approved (`Approved` or `Approved with M-track follow-ups`).

The 07.2 state-6 phase-done commit advances:
- ROADMAP row `07.2` flips `planned` → `done`.
- ROADMAP row `07` flips `in-progress` → `done` (closing-sub-phase rule).
- STATE.md advances active phase from `07.2 state 5` → `08 state 1`; next-skill `superpowers:brainstorming` scoped to phase 08's `BOOTSTRAP_PROMPT.md` §8 row-08 charter (*"Minimum admin API (config_dump, stats, clusters, listeners, ready, server_info) + graceful drain"*).

---

## 2. Behavior-contract scope for sub-phase 07.2

Sub-phase 07.2 is the first sub-phase to ship a filter that mutates wire-emitted headers. The expected BEHAVIOR_CONTRACT.md updates are **none under the recommended posture**:

1. **`Header allow-list` — no new entries anticipated.** The HeaderMutation filter is deterministic on both proxies: identical config produces identical wire-level header mutations on both sides (declaration-order on decode; reverse-declaration on encode; per-entry `key/value` produces a byte-identical (`key`, `value`) pair on both sides). The fixture's `expected_headers: SetEqualModuloAllowList` rule (the existing 04.x-established shape) is satisfied without new allow-list entries — `x-filter-response-stamp: phase-07` lands identically on both proxies; the existing allow-list entries (`server`, `date`, `x-envoy-upstream-service-time`) continue to govern the proxy-injected headers. If empirical testing at the differential layer surfaces a header-emission divergence (e.g., Envoy emits a debug header that envoy-rust does not under the HeaderMutation chain), a new row lands in BEHAVIOR_CONTRACT.md at the relevant 07.2 task.

2. **`Stat-name mapping` — no new entries.** The HeaderMutation filter does NOT emit stats in MVP. Envoy's framework supports per-filter stats `http.<stat_prefix>.<filter_name>.<stat>`, but the 07.x scope does not engage filter-emitted stats. Future filters (rate-limit, ext_authz) will extend the table at their landing phases.

3. **`Access log field mapping` — no new tokens.** The HeaderMutation filter does NOT introduce filter-state or dynamic-metadata access-log tokens. `%FILTER_STATE%` / `%DYNAMIC_METADATA%` remain deferred per parent-07 SPEC §4.

4. **`Filter chain iteration` subsection — recommended deferred.** If 07.1's state-4 verification didn't land this subsection (per 07.1 SPEC §2), 07.2's bilateral fixture 0013 is the empirical proof that the iteration order (declaration on decode, reverse on encode) matches Envoy's. Recommended posture: defer the subsection — the fixture-green evidence is the implicit canonicalization.

5. **`xDS wire state machine` and `Timing tolerances` subsections — untouched.**

If a 07.2 task surfaces an empirical divergence (HeaderMutation's append behavior, the case-insensitive remove semantic in OVERWRITE_IF_EXISTS_OR_ADD, list-valued header behavior under multiple appends), the edit lands at that task's commit per the standard cadence; do not batch.

---

## 3. Deliverables (per-task PLAN-ready cadence)

The eight D8.2-D15.2 deliverables of parent SPEC §3 decompose into **10 numbered tasks** for the standalone PLAN.md. Recommended execution order: **1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9 → 10**.

### Task 1 — `envoy-config` schema additions for HeaderMutation (D8.2 part 1)

**Files modified:**
- `crates/envoy-config/src/bootstrap.rs` — at the `HttpFilterTypedConfig` enum (lines 442-447 today, HEAD `7337f2c`) and at the new supporting structs.

**Schema additions:**

```rust
// crates/envoy-config/src/bootstrap.rs

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "@type")]
pub enum HttpFilterTypedConfig {
    #[serde(rename = "type.googleapis.com/envoy.extensions.filters.http.router.v3.Router")]
    Router(RouterConfig),

    #[serde(rename = "type.googleapis.com/envoy.extensions.filters.http.header_mutation.v3.HeaderMutation")]
    HeaderMutation(HeaderMutationConfig),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeaderMutationConfig {
    pub mutations: Mutations,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mutations {
    #[serde(default)]
    pub request_mutations: Vec<HeaderMutationEntry>,
    #[serde(default)]
    pub response_mutations: Vec<HeaderMutationEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeaderMutationEntry {
    pub append: HeaderValueOption,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeaderValueOption {
    pub header: HeaderValue,
    pub append_action: AppendAction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeaderValue {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AppendAction {
    AppendIfExistsOrAdd,
    OverwriteIfExistsOrAdd,
    // ADD_IF_ABSENT, OVERWRITE_IF_EXISTS — deferred at 07.2.
    // Validator rejects at parse time (Task 2).
    AddIfAbsent,
    OverwriteIfExists,
}
```

**Note on the `AppendAction` enum and serde naming.** Envoy's wire form uses SCREAMING_SNAKE_CASE (`APPEND_IF_EXISTS_OR_ADD`, etc.). The `#[serde(rename_all = "SCREAMING_SNAKE_CASE")]` annotation handles the rename. The unsupported variants (`ADD_IF_ABSENT`, `OVERWRITE_IF_EXISTS`) are present in the enum so serde parses them (otherwise serde would emit a generic "unknown variant" error); Task 2's validator then rejects them with the typed `UnsupportedHeaderMutationAppendAction` variant.

**Tests** (unit; in `crates/envoy-config/src/bootstrap.rs` `#[cfg(test)] mod header_mutation_schema_tests`):

1. **Positive — minimal request-only mutations parse.** YAML with `mutations: { request_mutations: [{ append: { header: { key: x, value: y }, append_action: APPEND_IF_EXISTS_OR_ADD } }] }` parses to the expected struct.
2. **Positive — minimal response-only mutations parse.** Symmetric.
3. **Positive — both request and response mutations parse.**
4. **Positive — empty mutations parse.** `mutations: {}` parses as `Mutations { request_mutations: vec![], response_mutations: vec![] }` (via `#[serde(default)]`).
5. **Positive — multiple entries parse.** A request_mutations list with 3 entries parses correctly.
6. **Positive — both supported AppendAction variants parse.** APPEND_IF_EXISTS_OR_ADD + OVERWRITE_IF_EXISTS_OR_ADD.
7. **Positive — unsupported AppendAction variants parse at schema level.** ADD_IF_ABSENT + OVERWRITE_IF_EXISTS parse successfully at the schema layer (validator at Task 2 rejects them).
8. **Negative — unknown field rejects.** `mutations: { request_mutations: [...], unknown_key: value }` returns serde "unknown field" error per `#[serde(deny_unknown_fields)]`.
9. **Negative — missing `mutations` field rejects.** A HeaderMutationConfig YAML without `mutations:` returns a serde "missing field" error.
10. **Negative — missing `key` field rejects.** A HeaderValue without `key:` rejects.
11. **Negative — missing `value` field rejects.** A HeaderValue without `value:` rejects.
12. **Negative — unknown @type URL rejects.** `typed_config: { "@type": ".../HeaderMutation.unknown" }` rejects via serde's tagged-enum-on-unknown-tag.

**Code budget:** ~120 LoC schema (6 new structs + 1 new enum) + ~80 LoC unit tests = ~200 LoC.

**Dependencies:** None on 07.2 prior tasks (Task 1 can be the very first task of 07.2).

**Commit message:** `phase 07.2: task 1 — HttpFilterTypedConfig::HeaderMutation + supporting schema`.

### Task 2 — `envoy-config` validator extension for HeaderMutation (D8.2 part 2)

**Files modified:**
- `crates/envoy-config/src/bootstrap.rs` — extend `validate_http_filters` (the function landed in 07.1 Task 4) and add the new `ConfigError::UnsupportedHeaderMutationAppendAction` variant.

**New `ConfigError` variant** (append to the existing enum):

```rust
#[error("HCM listener {listener:?} HeaderMutation entry at position {position}: unsupported append_action {action}")]
UnsupportedHeaderMutationAppendAction {
    listener: String,
    position: usize,
    action: String,
},

#[error("HCM listener {listener:?} HeaderMutation entry at position {position}: empty header key")]
EmptyHeaderMutationKey { listener: String, position: usize },

#[error("HCM listener {listener:?} HeaderMutation entry at position {position}: invalid token in header key {key:?}")]
InvalidHeaderMutationKey { listener: String, position: usize, key: String },
```

**Validator extension at `validate_http_filters`** (the function that 07.1 Task 4 added):

```rust
// Inside validate_http_filters, in the per-filter loop:
match &f.typed_config {
    HttpFilterTypedConfig::Router(_) => { /* 07.1 existing arm */ }
    HttpFilterTypedConfig::HeaderMutation(cfg) => {
        // 07.1 retains the name-vs-typed_config consistency check below;
        // the validator continues to fire `UnsupportedHttpFilter` if a
        // HeaderMutation typed_config is paired with a wrong `name`.
        if f.name != "envoy.filters.http.header_mutation" {
            return Err(ConfigError::UnsupportedHttpFilter {
                name: f.name.clone(),
            });
        }
        validate_header_mutation_entries(
            &cfg.mutations.request_mutations,
            listener_name,
            i,  // overall position in the filter chain
        )?;
        validate_header_mutation_entries(
            &cfg.mutations.response_mutations,
            listener_name,
            i,
        )?;
    }
}
```

**New free function** `validate_header_mutation_entries`:

```rust
fn validate_header_mutation_entries(
    entries: &[HeaderMutationEntry],
    listener_name: &str,
    filter_chain_position: usize,
) -> Result<(), ConfigError> {
    for (entry_idx, entry) in entries.iter().enumerate() {
        if entry.append.header.key.is_empty() {
            return Err(ConfigError::EmptyHeaderMutationKey {
                listener: listener_name.to_string(),
                position: entry_idx,
            });
        }
        if !is_valid_rfc7230_token(&entry.append.header.key) {
            return Err(ConfigError::InvalidHeaderMutationKey {
                listener: listener_name.to_string(),
                position: entry_idx,
                key: entry.append.header.key.clone(),
            });
        }
        match entry.append.append_action {
            AppendAction::AppendIfExistsOrAdd | AppendAction::OverwriteIfExistsOrAdd => {
                // supported.
            }
            AppendAction::AddIfAbsent => {
                return Err(ConfigError::UnsupportedHeaderMutationAppendAction {
                    listener: listener_name.to_string(),
                    position: entry_idx,
                    action: "ADD_IF_ABSENT".to_string(),
                });
            }
            AppendAction::OverwriteIfExists => {
                return Err(ConfigError::UnsupportedHeaderMutationAppendAction {
                    listener: listener_name.to_string(),
                    position: entry_idx,
                    action: "OVERWRITE_IF_EXISTS".to_string(),
                });
            }
        }
    }
    Ok(())
}

fn is_valid_rfc7230_token(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| {
        // RFC 7230 §3.2.6 tchar definition.
        b.is_ascii_alphanumeric()
            || b"!#$%&'*+-.^_`|~".contains(&b)
    })
}
```

**Note on `is_valid_rfc7230_token` reuse.** The existing `crates/envoy-config/src/bootstrap.rs` may already have a `is_token_char` or similar helper from 04.2 D2's HeaderMatcher path (which validates header names per RFC 7230). The planner at 07.2 state-2 PLAN-writeup time reuses the existing helper if found, else lands the helper inline at Task 2. Recommended posture: search-and-reuse before adding a duplicate.

**Tests** (unit; extends the existing `bootstrap.rs` `#[cfg(test)] mod validate_http_filters_tests` and adds a new `#[cfg(test)] mod header_mutation_validator_tests`):

1. **Positive — HeaderMutation with all-supported entries passes.** A filter chain `[HeaderMutation, Router]` with `request_mutations` and `response_mutations` each containing 2 entries with `APPEND_IF_EXISTS_OR_ADD` / `OVERWRITE_IF_EXISTS_OR_ADD` validates clean.
2. **Negative — empty key rejects.** Entry with `header.key: ""` returns `Err(EmptyHeaderMutationKey { position: 0 })`.
3. **Negative — invalid token in key rejects.** Entry with `header.key: "x bad"` (space invalid per RFC 7230) returns `Err(InvalidHeaderMutationKey { position: 0, key: "x bad" })`.
4. **Negative — ADD_IF_ABSENT rejects.** Entry with `append_action: ADD_IF_ABSENT` returns `Err(UnsupportedHeaderMutationAppendAction { position: 0, action: "ADD_IF_ABSENT" })`.
5. **Negative — OVERWRITE_IF_EXISTS rejects.** Symmetric.
6. **Negative — Router-NOT-terminal still rejects under HeaderMutation chain.** A chain `[Router, HeaderMutation]` (Router first, HeaderMutation second) returns `Err(RouterNotTerminal { position: 0 })` per the 07.1 Task 4 validator. This re-verifies the 07.1 validator under a richer typed_config space.
7. **Negative — duplicate Router rejects under HeaderMutation chain.** A chain `[HeaderMutation, Router, Router]` returns `Err(DuplicateRouterFilter)` per the 07.1 Task 4 validator.
8. **Negative — name/typed_config mismatch rejects.** A HttpFilter with `name: "envoy.filters.http.fault"` but `typed_config: HeaderMutation(_)` returns `Err(UnsupportedHttpFilter { name: "envoy.filters.http.fault" })`.

**Code budget:** ~80 LoC validator extension (per-filter arm + helper function + token validator if not already present) + ~60 LoC unit tests + ~30 LoC new ConfigError variants = ~170 LoC.

**Dependencies:** Task 1 (schema types must exist).

**Commit message:** `phase 07.2: task 2 — HeaderMutation parse-time validator + 3 new ConfigError variants`.

### Task 3 — `envoy-filter` HeaderMutationFilter runtime + build_from_config arm (D9.2 part 1)

**Files modified:**
- `crates/envoy-filter/src/instance.rs` — extend `HttpFilterInstance` enum with `HeaderMutation` variant.
- `crates/envoy-filter/src/header_mutation.rs` — NEW; `HeaderMutationFilter` struct + builder.

**Files created:**
- `crates/envoy-filter/src/header_mutation.rs` — the runtime module for the HeaderMutation filter.

**`lib.rs`** gains a `pub mod header_mutation;` declaration.

**`instance.rs`** extends:

```rust
use crate::header_mutation::HeaderMutationFilter;

pub enum HttpFilterInstance {
    Router(RouterTerminus),
    HeaderMutation(HeaderMutationFilter),
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
            envoy_config::HttpFilterTypedConfig::HeaderMutation(cfg) => {
                Ok(HttpFilterInstance::HeaderMutation(
                    HeaderMutationFilter::build_from_config(cfg)?,
                ))
            }
        }
    }

    pub(crate) fn decode_headers(&mut self, req: &mut Request) -> Decision {
        match self {
            HttpFilterInstance::Router(r) => r.decode_headers(req),
            HttpFilterInstance::HeaderMutation(f) => f.decode_headers(req),
        }
    }

    pub(crate) fn encode_headers(&mut self, resp: &mut Response) -> Decision {
        match self {
            HttpFilterInstance::Router(r) => r.encode_headers(resp),
            HttpFilterInstance::HeaderMutation(f) => f.encode_headers(resp),
        }
    }
}
```

**`header_mutation.rs`** defines:

```rust
use envoy_http1::codec::{Request, Response};
use crate::error::FilterError;
use crate::pipeline::Decision;

#[derive(Debug, Clone)]
pub struct HeaderMutationFilter {
    request_mutations: Vec<RuntimeHeaderMutation>,
    response_mutations: Vec<RuntimeHeaderMutation>,
}

#[derive(Debug, Clone)]
struct RuntimeHeaderMutation {
    /// Header key, lowercased once at build time for case-insensitive
    /// matching on OVERWRITE semantics.
    key: String,
    value: String,
    action: RuntimeAppendAction,
}

#[derive(Debug, Clone, Copy)]
enum RuntimeAppendAction {
    Append,    // APPEND_IF_EXISTS_OR_ADD
    Overwrite, // OVERWRITE_IF_EXISTS_OR_ADD
}

impl HeaderMutationFilter {
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
        Ok(Self {
            request_mutations,
            response_mutations,
        })
    }

    pub(crate) fn decode_headers(&mut self, _req: &mut Request) -> Decision {
        // Task 4 lands the application; Task 3 stubs as Continue.
        Decision::Continue
    }

    pub(crate) fn encode_headers(&mut self, _resp: &mut Response) -> Decision {
        // Task 4 lands the application; Task 3 stubs as Continue.
        Decision::Continue
    }
}

fn map_entry(
    entry: &envoy_config::HeaderMutationEntry,
) -> Result<RuntimeHeaderMutation, FilterError> {
    let action = match entry.append.append_action {
        envoy_config::AppendAction::AppendIfExistsOrAdd => RuntimeAppendAction::Append,
        envoy_config::AppendAction::OverwriteIfExistsOrAdd => RuntimeAppendAction::Overwrite,
        envoy_config::AppendAction::AddIfAbsent | envoy_config::AppendAction::OverwriteIfExists => {
            // Validator (07.2 Task 2) rejects these earlier; defense-in-depth here.
            return Err(FilterError::UnsupportedFilterType {
                position: 0, // best effort; the actual position is known at build_from_config
                name: format!("AppendAction::{:?}", entry.append.append_action),
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

**Tests** (unit; in `header_mutation.rs` `#[cfg(test)] mod tests`):

1. **`build_from_config` on empty Mutations returns empty filter.** Asserts `filter.request_mutations.len() == 0` + `filter.response_mutations.len() == 0`.
2. **`build_from_config` on a single Append entry returns 1-entry runtime.** Asserts `filter.request_mutations[0].action` is `Append`; `key` is lowercased (`X-Foo` → `x-foo`); `value` is preserved.
3. **`build_from_config` on a single Overwrite entry returns 1-entry runtime.** Asserts `action == Overwrite`.
4. **`build_from_config` on unsupported AppendAction returns Err.** The defense-in-depth check fires (validator at Task 2 catches earlier).
5. **`HttpFilterInstance::build` on HeaderMutation typed_config produces a HeaderMutation variant.**
6. **`decode_headers` / `encode_headers` stubs return Continue at Task 3.** (Real semantics land at Task 4.)

**Code budget:** ~80 LoC runtime types + ~50 LoC builder + ~50 LoC unit tests + ~10 LoC instance.rs extension = ~190 LoC.

**Dependencies:** 07.1 Task 1 + Task 3 (envoy-filter scaffold + `HttpFilterInstance` enum present); 07.2 Task 1 (envoy-config schema types).

**Commit message:** `phase 07.2: task 3 — HeaderMutationFilter runtime types + builder`.

### Task 4 — `HeaderMutationFilter::decode_headers` + `encode_headers` semantics (D9.2 part 2)

**Files modified:**
- `crates/envoy-filter/src/header_mutation.rs` — replace the Task-3 stubs with the real iteration semantics.

**Decode-side semantics:**

```rust
pub(crate) fn decode_headers(&mut self, req: &mut Request) -> Decision {
    apply_mutations(&mut req.headers, &self.request_mutations);
    Decision::Continue
}
```

**Encode-side semantics:**

```rust
pub(crate) fn encode_headers(&mut self, resp: &mut Response) -> Decision {
    apply_mutations(&mut resp.headers, &self.response_mutations);
    Decision::Continue
}
```

**`apply_mutations` helper:**

```rust
fn apply_mutations(headers: &mut Vec<(String, String)>, mutations: &[RuntimeHeaderMutation]) {
    for mutation in mutations {
        match mutation.action {
            RuntimeAppendAction::Append => {
                // RFC 7230 §3.2.2: duplicate header names are permitted; semantics
                // are list-valued (comma-join on the receiver side).
                headers.push((mutation.key.clone(), mutation.value.clone()));
            }
            RuntimeAppendAction::Overwrite => {
                // Case-insensitive remove every existing entry with the same key,
                // then push the new entry once.
                let key_lower = &mutation.key; // already lowercased at build time
                headers.retain(|(k, _v)| k.to_ascii_lowercase() != *key_lower);
                headers.push((mutation.key.clone(), mutation.value.clone()));
            }
        }
    }
}
```

**Header normalization invariant.** The `Request.headers` / `Response.headers` vectors are `Vec<(String, String)>` in `envoy_http1::codec`. Per the 04.x convention, header names on the wire are RFC-7230-token-set and the codec normalizes them to lowercase at parse time. The HeaderMutation filter's `key` is lowercased once at build time (Task 3); on encode the wire-emitted header name matches whatever the codec re-serializes (the codec emits lowercase per HTTP/1.1 case-insensitive header-name semantics; this is regression-equivalent under the existing 04.x posture).

**Tests** (unit; in `header_mutation.rs` `#[cfg(test)] mod tests`):

1. **Append on absent key adds entry.** `request_mutations: [{ x-foo: bar, Append }]`; pre-state `req.headers: []`; post-state `req.headers: [("x-foo", "bar")]`.
2. **Append on present key adds duplicate.** Pre: `req.headers: [("x-foo", "original")]`; post: `req.headers: [("x-foo", "original"), ("x-foo", "bar")]`.
3. **Overwrite on absent key adds entry.** Pre: `[]`; post: `[("x-foo", "bar")]`.
4. **Overwrite on present key replaces.** Pre: `[("x-foo", "original")]`; post: `[("x-foo", "bar")]` — exactly one entry.
5. **Overwrite is case-insensitive on the existing entry.** Pre: `[("X-Foo", "original")]`; post: `[("x-foo", "bar")]` — case-folded match removes the existing entry.
6. **Multiple Append entries in order.** `request_mutations: [{x-a: 1, A}, {x-b: 2, A}, {x-a: 3, A}]`; post: `[("x-a", "1"), ("x-b", "2"), ("x-a", "3")]`.
7. **Multiple Overwrite entries in order.** Subsequent Overwrite replaces a prior Overwrite's entry.
8. **Mix of Append and Overwrite in order.**
9. **Empty mutations is no-op on decode.** Pre/post headers identical.
10. **Empty mutations is no-op on encode.** Symmetric.
11. **`decode_headers` returns `Continue` after applying mutations.**
12. **`encode_headers` returns `Continue` after applying mutations.**
13. **Round-trip via `FilterPipeline`.** Build a pipeline `[HeaderMutation, Router]` with `request_mutations: [{x-foo: bar, A}]`; call `pipeline.decode_headers(&mut req)`; assert `req.headers` carries `x-foo: bar`.
14. **Iteration-order on encode via `FilterPipeline`.** Build a pipeline `[HeaderMutation, Router]` with `response_mutations: [{x-resp: stamp, A}]`; call `pipeline.encode_headers(&mut resp)`; assert `resp.headers` carries `x-resp: stamp`. (Re-verifies that the framework's reverse-iteration on encode reaches HeaderMutation after Router's no-op.)

**Code budget:** ~30 LoC semantics + ~5 LoC apply_mutations helper + ~150 LoC unit tests (14 tests, ~10 LoC each) = ~185 LoC.

**Dependencies:** Task 3.

**Commit message:** `phase 07.2: task 4 — HeaderMutationFilter decode/encode iteration semantics`.

### Task 5 — H1 + H2 HCM integration tests for non-Router filters (D11.2 part 1; closes deferred 07.1 tests)

**Files modified:**
- `crates/envoy-http1/src/hcm.rs` — extend `#[cfg(test)] mod tests` with the 5 tests deferred at 07.1 Task 6 (test stubs 3-7).
- `crates/envoy-http2/src/hcm.rs` — extend `#[cfg(test)] mod tests` with the parallel 4 tests deferred at 07.1 Task 7.

**Tests (H1 side; in `crates/envoy-http1/src/hcm.rs`):**

1. **`decode_headers` fires before route-match.** Build an HCMConfig with `http_filters: [HeaderMutation { request_mutations: [{ :path, /bar, OverwriteIfExistsOrAdd }] }, Router]` and a route matching prefix `/bar` → direct_response 200. Drive a request with `path: /foo`; assert the response is the direct_response 200 (proves the route matcher saw `/bar` after the mutation). **Subtle:** `:path` is a pseudo-header in H2 but a request-line field in H1; this test uses an H1 codec, so the `path` mutation goes via `Request.path` (or via the headers vector if `:path` is stored there). The planner at 07.2 PLAN-writeup picks a non-pseudo header to mutate if the H1 codec doesn't carry `:path` in `headers`; e.g., mutate `:authority` or use `host:` rewriting via a different header. **Recommended:** mutate a regular header (`x-test-path-override`) and use a router-action that depends on that header (HeaderMatcher path matcher landed in 04.2). The test then becomes: a HeaderMatcher route matches on `x-test-path-override: /bar` → direct_response 200; the HeaderMutation adds that header on decode; the route matches.
2. **`encode_headers` fires after writer-arm response construction but before wire write.** HCMConfig with `http_filters: [HeaderMutation { response_mutations: [{ x-test-encode, ok, A }] }, Router]`; direct_response route. Drive a request; assert the wire output's response headers carry `x-test-encode: ok`.
3. **`StopAndSend` at decode skips route-match.** **Note:** at 07.2 no filter emits StopAndSend in MVP; this test requires a test-only filter stub gated `#[cfg(test)]` that always returns StopAndSend. The planner at 07.2 state-2 PLAN-writeup adds the test-only stub inside `instance.rs` `#[cfg(test)]` (e.g., `HttpFilterInstance::TestStopAndSend(...)` variant gated `#[cfg(test)]`) and writes the test against it. Validates that the framework's StopAndSend short-circuit on decode bypasses route-match.
4. **`StopAndSend` at encode substitutes the wire-emitted response.** Same `#[cfg(test)]` stub strategy; validates the encode-side StopAndSend path.
5. **Access-log reflects post-encode headers.** HCMConfig with both an access_log and HeaderMutation `response_mutations: [{ x-test, ok, A }]`. Drive a request; assert the access log line emitted captures the post-encode response state (the per-class HCM counter increment also sees post-encode `resp.status` — that's exercised by the per-class counter test below).

**Tests (H2 side; in `crates/envoy-http2/src/hcm.rs`):**

6. **`decode_headers` fires before route-match (H2).** Parallel to test 1 via the H2 codec.
7. **`encode_headers` fires before `send_envoy_response` (H2).** Parallel to test 2.
8. **`StopAndSend` at decode side (H2).** Parallel to test 3.
9. **`StopAndSend` at encode side (H2).** Parallel to test 4.

**Test-only filter stub** (for tests 3, 4, 8, 9). Add inside `crates/envoy-filter/src/instance.rs` gated `#[cfg(test)]`:

```rust
#[cfg(test)]
impl HttpFilterInstance {
    pub fn test_stop_and_send_on_decode(resp: Response) -> Self {
        // ... a test-only variant or a stub via PhantomData — the planner picks
        // the cleanest shape at PLAN-writeup time. One option: add a new
        // `TestStub(TestStub)` variant gated `#[cfg(test)]`.
    }
}
```

**Code budget:** ~150 LoC tests at H1 (~30 LoC × 5) + ~120 LoC tests at H2 (~30 LoC × 4) + ~20 LoC test-only stub = ~290 LoC. **Most of this is test code, not production code.**

**Dependencies:** Tasks 1-4 (HeaderMutation must be wired end-to-end for the in-process tests to drive a real chain).

**Commit message:** `phase 07.2: task 5 — H1+H2 HCM filter-chain in-process integration tests (closes deferred 07.1 tests)`.

### Task 6 — Fuzz corpus extension for HeaderMutation (D10.2)

**Files modified:**
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_header_mutation_filter.yaml` — NEW; minimal HCM with HeaderMutation + Router.
- `crates/envoy-config/fuzz/.gitignore` (if it uses an explicit allow-list) — add the new seed.
- `crates/envoy-config/tests/fuzz_corpus_*.rs` (the existing `fuzz_corpus_seeds_parse_or_reject_cleanly` test) — extend the seed array.

**Seed contents** (`hcm_header_mutation_filter.yaml`; minimal positive case):

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

The seed parses + validates cleanly per the new validator (Task 2) — no clusters declared because the route uses direct_response 200; this is the minimal positive case that exercises the new schema arm.

**Existing fuzz target** (`crates/envoy-config/fuzz/fuzz_targets/parse_bootstrap.rs`) is unchanged; the new seed is auto-discovered by the corpus walker at fuzz-run time.

**`fuzz_corpus_seeds_parse_or_reject_cleanly` test** extension — add `"hcm_header_mutation_filter.yaml"` to the seed array. Test asserts the seed parses without panic (positive case → `Ok`).

**Tests** (workspace-level; the existing fuzz_corpus walker test asserts the new seed parses successfully). No new test file.

**Code budget:** ~50 LoC fuzz seed YAML + ~1 LoC test array entry + ~1 LoC gitignore entry = ~52 LoC.

**Dependencies:** Task 1 + Task 2 (schema + validator must accept the seed).

**Commit message:** `phase 07.2: task 6 — fuzz corpus seed for HeaderMutation HCM`.

### Task 7 — Differential harness backend-helper extension (D11.2 part 2)

**Files modified:**
- `tests/helpers/http1-echo-server/src/lib.rs` (or wherever the existing Http1EchoBackend lives) — verify it echoes received request headers into the response body as `key: value\n` lines; extend if necessary.

**Pre-state check.** The 04.3 Http1EchoBackend helper at HEAD `7337f2c` echoes received request headers into the response body as part of its existing shape (the helper was used by fixture 0008 to assert request-side equivalence). If the echo shape is already present, **no code change in Task 7** — the assertion shape is reused as-is.

If the echo shape is absent (the helper echoes only body bytes, not headers), Task 7 extends the helper to echo headers per the line format `name: value\n` plus the existing body. The echo emit order is sorted-by-name (deterministic) so the differential body-byte comparison is reliable across runs.

**Tests** (in the helper crate's own `#[cfg(test)]`):

1. Helper response body contains the request's header set as `name: value\n` lines.
2. Helper response body's header echo is sorted by header name (deterministic).

**Code budget:** ~0-30 LoC depending on the helper's pre-state. The planner at 07.2 state-2 PLAN-writeup time grep-checks the helper before estimating.

**Dependencies:** None on 07.2 prior tasks (helper-only change).

**Commit message:** `phase 07.2: task 7 — http1-echo-server helper: verify/extend header echo shape` (or omitted if the helper is already correct).

### Task 8 — Fixture 0013 (D12.2)

**Files created:**
- `tests/fixtures/0013-http-filter-header-mutation/envoy.yaml` — reference Envoy config.
- `tests/fixtures/0013-http-filter-header-mutation/envoy-rust.yaml` — envoy-rust config.
- `tests/fixtures/0013-http-filter-header-mutation/inputs/payload.bin` — 0-byte placeholder.
- `tests/fixtures/0013-http-filter-header-mutation/expectations.yaml` — differential assertions.
- `tests/fixtures/0013-http-filter-header-mutation/README.md` — fixture documentation.
- `tests/differential/tests/http_filter_header_mutation.rs` — Docker-gated wrapper.

**`envoy.yaml`** (reference config; ~100 LoC):

```yaml
admin: { ... }  # standard admin block from prior fixtures
static_resources:
  listeners:
    - name: listener0
      address: { socket_address: { address: 0.0.0.0, port_value: 10000 } }
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
      type: STRICT_DNS  # per 05.1 ADR-0023 + 05.4 ADR-0024
      dns_lookup_family: V4_ONLY  # per 05.4 ADR-0025
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: host.docker.internal  # per 04.3 ADR-0019
                      port_value: {{HTTP1_BACKEND_PORT}}
```

**`envoy-rust.yaml`** is identical modulo any per-side divergence already established for fixture 0008's STRICT_DNS / dns_lookup_family pattern (e.g., envoy-rust may use `127.0.0.1` directly if `host.docker.internal` is the Docker-gated-only pattern). The planner at 07.2 PLAN-writeup time mirrors fixture 0008's `envoy-rust.yaml` shape verbatim for the cluster/endpoint pattern.

**`expectations.yaml`** (~30 LoC):

```yaml
driver:
  kind: http1
  request:
    method: GET
    path: /
    headers: []
    body: ""
  expected_status: 200
  expected_body:
    kind: byte_exact
  expected_headers:
    rule: set_equal_modulo_allow_list
```

The `expected_body: byte_exact` assertion compares the response body byte-for-byte across both proxies. Since the Http1EchoBackend echoes received request headers into the body (per Task 7), and both proxies forward identical mutated request headers to the same backend (the HeaderMutation filter is deterministic on both sides), the body bytes are identical. The presence of `x-filter-stamp: phase-07` in the echoed body proves the decode-side stamp landed at the backend on both proxies.

The `expected_headers: set_equal_modulo_allow_list` assertion compares the response headers as a set, allow-listing the existing `server` / `date` / `x-envoy-upstream-service-time` entries. The presence of `x-filter-response-stamp: phase-07` in both proxies' responses is automatic (both proxies apply the response_mutations identically).

**`README.md`** (~30 LoC) explains the fixture's surface:

```markdown
# Fixture 0013 — `envoy.filters.http.header_mutation` end-to-end

This fixture exercises the HeaderMutation HTTP filter in front of the Router
on a Router-proxied route to an Http1EchoBackend cluster.

## Filter chain

  http_filters:
    - HeaderMutation (request_mutations + response_mutations)
    - Router (terminus)

## Assertions

- **Request-side stamp at backend** (decode_headers).
  The HeaderMutation adds `x-filter-stamp: phase-07` on the request.
  The Http1EchoBackend echoes received request headers into the response
  body as `name: value\n` lines (per the 04.3 helper shape; verified at
  07.2 Task 7). The `expected_body: byte_exact` assertion confirms both
  proxies forwarded the same stamped request to the backend.

- **Response-side stamp at client** (encode_headers).
  The HeaderMutation adds `x-filter-response-stamp: phase-07` on the
  response. The `expected_headers: set_equal_modulo_allow_list`
  assertion confirms both proxies emitted the stamp.

## Per-side divergence

`envoy-rust.yaml` uses STRICT_DNS / dns_lookup_family / V4_ONLY per
the 05.1 ADR-0023 + 05.4 ADR-0024 + 05.4 ADR-0025 pattern (mirrors
fixture 0008's `envoy-rust.yaml` shape).
```

**Docker-gated wrapper** (`tests/differential/tests/http_filter_header_mutation.rs`, ~30 LoC) — mirrors `tests/differential/tests/http1_router_upstream.rs`:

```rust
use differential::run_fixture;

#[tokio::test]
#[ignore = "docker-gated"]
async fn http_filter_header_mutation() {
    run_fixture("0013-http-filter-header-mutation").await;
}
```

(The `run_fixture` helper is the existing 04.3+ shape; no helper extension needed if the harness already supports the `Http1EchoBackend` + `expected_body: byte_exact` combination at fixture 0008's level.)

**Code budget:** ~100 LoC envoy.yaml + ~100 LoC envoy-rust.yaml + ~30 LoC expectations.yaml + ~30 LoC README.md + ~30 LoC Docker-gated wrapper = ~290 LoC of fixture material. **The fixture YAML is data; the wrapper is the only Rust code.**

**Dependencies:** Tasks 1-7 (schema + validator + runtime + helper all required).

**Commit message:** `phase 07.2: task 8 — fixture 0013-http-filter-header-mutation (bilateral assertion)`.

### Task 9 — In-process backstop (D13.2)

**Files created:**
- `crates/envoy-bin/tests/http_filter_header_mutation.rs` — in-process integration test.

**Test contents** (~120 LoC; mirrors the 04.3 / 05.3 / 06.2 in-process backstop precedents):

```rust
// crates/envoy-bin/tests/http_filter_header_mutation.rs

use tempfile::TempDir;
// ... existing imports per the prior in-process backstops ...

#[tokio::test]
async fn http_filter_header_mutation_in_process() {
    // Start an Http1EchoBackend on an ephemeral port.
    let backend = http1_echo_server::serve_ephemeral().await;

    // Render envoy-rust.yaml with the ephemeral port.
    let tmpdir = TempDir::new().unwrap();
    let cfg_path = tmpdir.path().join("envoy-rust.yaml");
    let cfg = render_fixture_config(
        "tests/fixtures/0013-http-filter-header-mutation/envoy-rust.yaml",
        &[("{{HTTP1_BACKEND_PORT}}", &backend.port().to_string())],
    );
    fs::write(&cfg_path, cfg).unwrap();

    // Spawn envoy-bin as a subprocess.
    let envoy_rust = spawn_envoy_bin(&cfg_path).await;

    // Drive a GET / through drive_http1.
    let resp = drive_http1(envoy_rust.listener_addr(), "GET", "/", &[], b"").await;

    // Assertions.
    assert_eq!(resp.status, 200);
    assert!(
        resp.headers.iter().any(|(k, v)| k == "x-filter-response-stamp" && v == "phase-07"),
        "expected response-side stamp; got headers: {:?}", resp.headers,
    );
    assert!(
        resp.body.windows(b"x-filter-stamp: phase-07".len())
            .any(|w| w == b"x-filter-stamp: phase-07"),
        "expected request-side stamp echoed in body; got body: {:?}", String::from_utf8_lossy(&resp.body),
    );
}
```

**Tests** — the test file IS the test; runs via `cargo test -p envoy-bin --test http_filter_header_mutation`.

**Code budget:** ~120 LoC.

**Dependencies:** Tasks 1-8 (everything must be wired; the fixture's `envoy-rust.yaml` is reused).

**Commit message:** `phase 07.2: task 9 — in-process backstop for HeaderMutation`.

### Task 10 — State-4 phase-done verification + STATE advance + parent-07 close (D14.2 + D15.2)

**Files modified:**
- `docs/envoy-rust/phases/07.2-header-mutation-filter/PROGRESS.md` — Task 10 entry.
- `docs/envoy-rust/STATE.md` — advance from `07.2` lifecycle state 3 → `07.2` state 4-reached / state-5-next; then (at state-6 close) → phase `08` lifecycle state 1.
- `docs/envoy-rust/ROADMAP.md` — flip row `07.2` `planned` → `done`; flip row `07` `in-progress` → `done` (closing-sub-phase rule).

**Note on the 2-commit-vs-1-commit cadence.** The state-4 evidence anchor + state-5 REVIEW + state-6 close form a 3-commit chain per the parent-06 / parent-05 cadence:
- **State-4 evidence anchor commit:** PROGRESS Task 10 quotes the CI run URL + 13-fixture-green-simultaneous evidence + h2spec + clippy/fmt/deny gates.
- **State-5 STATE advance commit:** STATE.md advances active phase to `07.2 state 5`; next-skill `superpowers:requesting-code-review`.
- **State-5 REVIEW.md commit:** `superpowers:requesting-code-review` lands `REVIEW.md` with a verdict.
- **State-6 close-out commit:** STATE.md advances to `08 state 1`; ROADMAP rows `07.2` + `07` flip to `done`; commit title `phase 07.2: <title> [parent 07 done] [ADR-NNNN, ...]`.

Task 10 covers the state-4 evidence anchor + the state-5 STATE advance; the state-5 REVIEW.md is a separate session running `superpowers:requesting-code-review`; the state-6 close-out is a separate session/commit after REVIEW approval.

**Per-task PROGRESS test-bucket attestation** at Task 10 (per parent SPEC §8 R-1):

```
- workspace tests: cargo test --workspace — PASS (count: <N>; commit at <SHA>)
- Docker-gated fixtures (13 total, 0001-0013): all green simultaneously per CI run <URL>
- h2spec conformance: <pass_rate>% (≥95% gate held; known-failures.txt unchanged)
- parse_bootstrap fuzz: clean (short-budget CI run at <duration>; new HeaderMutation seed exercised)
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean
- cargo fmt --all -- --check: clean
- cargo deny check: clean
```

**Parent-07 close at state-6.** When the state-6 close-out commit lands (after REVIEW approval):

- ROADMAP row `07.2`: `status: planned` → `status: done`.
- ROADMAP row `07`: `status: in-progress` → `status: done`. (Closing-sub-phase rule per `BOOTSTRAP_PROMPT.md` §4.1 invariant 2.)
- STATE.md advances active phase from `07.2 state 5` → `08 state 1`; next-skill `superpowers:brainstorming` scoped to phase 08's `BOOTSTRAP_PROMPT.md` §8 row-08 charter.
- DECISIONS.md ledger head: unchanged unless conditional ADRs landed during 07.2 execution under recommended posture.

**Code budget:** ~30 LoC PROGRESS + ~30 LoC STATE.md + ~5 LoC ROADMAP.md (row updates) at state-6 = ~65 LoC docs only across the 2-3 commits.

**Dependencies:** Tasks 1-9 (all code must be landed; the CI evidence anchors the cumulative behavior).

**Commit message (state-4 evidence anchor):** `phase 07.2: task 10 — state-4 verification (13 fixtures simultaneously green)`.

**Commit message (state-6 close-out):**

```
phase 07.2: envoy.filters.http.header_mutation + fixture 0013 [parent 07 done]

<1-3 sentence summary covering the HeaderMutation filter end-to-end, the
fixture 0013 bilateral assertion, and the parent-07 close-out.>

Differential surface: fixtures 0001-tcp-echo through 0013-http-filter-header-mutation all green at the Docker-gated CI level simultaneously; HeaderMutation filter bilaterally verified on both decode (request-side stamp at backend via Http1EchoBackend body echo) and encode (response-side stamp at client) iteration states.
Conformance: h2spec ≥95% pass (carried forward from 05.2 baseline; phase 07.2 engages no H2-framing surfaces).
```

If any conditional ADR landed during 07.2 (e.g., ADR-0031 foundations grant; ADR-0032 Cargo.lock cadence; or an in-execution ADR per D-3.5), the title's bracketed `[ADR-NNNN]` enumeration follows.

### Code budget summary

| Task | Code LoC | Test LoC | Fixture/Doc LoC | Total |
|---|---|---|---|---|
| 1 (HttpFilterTypedConfig::HeaderMutation schema) | ~120 | ~80 | — | ~200 |
| 2 (validator extension + 3 ConfigError variants) | ~110 | ~60 | — | ~170 |
| 3 (HeaderMutationFilter runtime types + builder) | ~140 | ~50 | — | ~190 |
| 4 (decode/encode semantics + apply_mutations) | ~35 | ~150 | — | ~185 |
| 5 (H1+H2 in-process integration tests) | ~20 | ~270 | — | ~290 |
| 6 (fuzz corpus extension) | — | — | ~52 | ~52 |
| 7 (helper verify/extend) | ~0-30 | ~10 | — | ~10-40 |
| 8 (fixture 0013) | — | — | ~290 | ~290 |
| 9 (in-process backstop) | — | ~120 | — | ~120 |
| 10 (state-4 + state-5 + state-6 docs) | — | — | ~65 | ~65 |
| **Total** | **~430-460** | **~740** | **~407** | **~1577-1607** |

Against parent SPEC §3 D8.2-D15.2 projection of ~800 LoC, the per-task decomposition projects ~1600 LoC (~+100% over the parent SPEC projection). **The over-projection is concentrated in test code** (~740 LoC tests + ~407 LoC fixture/doc) — production code stays at ~430-460 LoC, well within the parent's estimate. The state-2 PLAN-writeup planner should:

1. Verify the test-code projection is real (the 14-test breakdown at Task 4 is concrete; the 9-test breakdown at Task 5 is concrete). If the planner finds redundancy in the test inventory at PLAN-writeup time, prune.
2. Verify the fixture YAML projection (~290 LoC at Task 8) — the YAML files are reasonably dense; the README.md is the largest free variable. If the README.md grows large, that's expected per the 04.3 / 05.3 / 06.x README precedent.

Task count stays at 10 against parent SPEC's ~10 projection.

---

## 4. Out of scope (deferred non-goals for 07.2)

The following are explicitly deferred from sub-phase 07.2 per parent-07 SPEC §4 (the parent's out-of-scope list binds on each sub-phase). Items already covered by parent SPEC §4 are restated only where they intersect 07.2's surface.

- **`AppendAction::ADD_IF_ABSENT` + `AppendAction::OVERWRITE_IF_EXISTS`.** Defer; parsed at the schema level (the enum variants are present so serde doesn't emit a generic "unknown variant" error) but rejected at the validator level via `ConfigError::UnsupportedHeaderMutationAppendAction`. Future phases extend the validator's supported subset.
- **Per-route `typed_per_filter_config` overrides for HeaderMutation.** Defer per parent SPEC §4. HeaderMutation has no per-route knob in 07.2 — `request_mutations` and `response_mutations` are listener-level only (one chain per HCM). Per-route override scaffolding lands at the first phase that surfaces it.
- **Body-iteration / trailer-iteration states.** Defer per parent SPEC §4.
- **HeaderMutation `query_parameter_mutations`.** Envoy v1.33's HeaderMutation also supports `query_parameter_mutations`. Defer. envoy-rust's URL parsing surface (path + query) does not currently expose a query parameter mutation API; this is a future phase.
- **Filter-emitted stats.** Defer. HeaderMutation does not emit stats in MVP per parent SPEC §4.
- **Filter-state machinery + dynamic metadata.** Defer indefinitely.
- **Async-aware iteration shape.** Defer. HeaderMutation manipulates only `Vec<(String, String)>` headers; synchronous iteration suffices. Conditional ADR-0031 (foundations grant for `async_trait` or similar) is NOT projected.
- **`%FILTER_STATE%` / `%DYNAMIC_METADATA%` access-log tokens.** Defer indefinitely.
- **HTTP filters beyond HeaderMutation.** Defer per parent SPEC §4 ("HTTP filters family"). 07.x ships Router (07.1) + HeaderMutation (07.2) only; the broader family (cors, compression, fault, local+global rate limit, jwt_authn, rbac, ext_authz, ext_proc, oauth2, csrf, buffer, lua, wasm, adaptive concurrency, admission control, bandwidth limit) lands at later phases.
- **Phase-04.1 REVIEW M5/M9** (Cargo.lock cadence ratification ADR). Carries forward unchanged; 07.2 introduces no new top-level Cargo deps under recommended posture.
- **Phase-06.3 REVIEW I1** (Task 11 fixup verification-discipline gap). 07.2 PROGRESS test-bucket attestation discipline at every code-changing task closes this structurally. Note specifically: Task 8 lands fixture 0013 with a `expected_body: byte_exact` value-side block. Per parent SPEC §8 R-1, the Task 8 commit MUST be Docker-gated-fixture-run before commit (the planner triggers a CI run + waits for green before landing the Task 8 commit; OR runs the fixture locally if Docker is available; OR if neither, the Task 9 in-process backstop catches a behavioral regression before state-4).
- **Other carryforwards** (06.3 REVIEW I2; 06.2 REVIEW M1/M2/M4/M5; 06.1 REVIEW I2/M1/M4; 05.3 REVIEW I2; 05.2 REVIEW I1/I2/I3; 02.2 REVIEW M1) — all out of scope; carry forward unchanged.

---

## 5. Parent-07 close-out posture (Task 10 detail)

The 07.2 state-6 phase-done commit is the parent-07 close-out per the closing-sub-phase invariant. Concretely, at the state-6 commit:

1. **ROADMAP.md row `07.2`** flips `planned` → `done` (this sub-phase done).
2. **ROADMAP.md row `07`** flips `in-progress` → `done` (parent flipped because all sub-phases done; `07.1` flipped at its own state-6).
3. **STATE.md** is rewritten to:
   - Active phase: `08` (`08-admin-api-and-drain` per `BOOTSTRAP_PROMPT.md` §8 row 08).
   - Active phase lifecycle state: `0` (phase not yet in lifecycle state 1; ROADMAP row `08` is `status: planned`).
   - Next-skill: `superpowers:brainstorming` scoped to phase 08's row-08 charter.
   - Brainstorm-summary blocks from 07.1 and 07.2: replaced by the parent-07 close-out summary (mirrors 06.3's STATE.md replacement of 06.1 + 06.2 + 06.3 summary blocks at the parent-06 close commit).
   - "Last commit" section: points at the 07.2 state-6 close-out commit SHA.
   - "Last updated" timestamp: ISO-8601 UTC at commit time.
4. **DECISIONS.md ledger head**: unchanged unless conditional ADRs landed (ADR-0030 is the head at parent-07 close under recommended posture). If ADR-0031 / ADR-0032 / inline-execution-ADRs landed, the head advances accordingly.
5. **BEHAVIOR_CONTRACT.md**: unchanged under recommended posture. New rows / subsections land at the task where empirical evidence demanded them (per parent SPEC §2 + 07.2 §2).

This mirrors phase-04's `e626862` (closing-sub-phase 04.3 close + parent-04 close), phase-05's `82c26b8` (closing-sub-phase 05.3 close + parent-05 close), and phase-06's `b918f33` (closing-sub-phase 06.3 close + parent-06 close) shapes.

The state-6 close-out commit's title carries both the sub-phase title and the `[parent 07 done]` tag plus any ADRs landed during the execution arc:

```
phase 07.2: envoy.filters.http.header_mutation + fixture 0013 [parent 07 done] [ADR-0030, ...]
```

ADR-0030 (parent-07 split decision) landed at parent-07 state-2 (the predecessor of the 07.1 state-1 brainstorm session). The state-6 close-out title's ADR enumeration includes ADR-0030 + any conditional ADRs that actually landed during 07.x execution.

---

## 6. Implementation signposts for the planner

**Signpost 1 — `is_valid_rfc7230_token` reuse.** Search `crates/envoy-config/src/bootstrap.rs` for existing token-validator helpers from 04.2's HeaderMatcher path before adding a new helper. The 04.2 D2 work added RFC 7230 validation; if the helper exists, reuse; if it's private to a different module, expose it (or duplicate inline if exposing creates a cross-cut). Recommended posture: search-and-reuse before adding a duplicate.

**Signpost 2 — Test-only `HttpFilterInstance` variant for StopAndSend tests.** Tests 3, 4, 8, 9 at Task 5 require a filter that always emits StopAndSend. Add inside `crates/envoy-filter/src/instance.rs` gated `#[cfg(test)]`:

```rust
#[cfg(test)]
#[derive(Clone)]
pub enum HttpFilterInstance {
    Router(RouterTerminus),
    HeaderMutation(HeaderMutationFilter),
    TestStopAndSendOnDecode(envoy_http1::codec::Response),
    TestStopAndSendOnEncode(envoy_http1::codec::Response),
}
```

The `#[cfg(test)]` enum-redefinition pattern allows the variant to exist in tests without polluting the production enum. The match arms in `decode_headers` / `encode_headers` are also `#[cfg(test)]`-gated.

**Alternative — visible-via-feature-flag.** If the planner prefers not to redefine the enum, an alternative is a Cargo feature `test-stubs` that gates the test-only variant. Recommended: stick with `#[cfg(test)]` — Cargo features for test-only types add complexity without payoff.

**Signpost 3 — `HeaderMutationFilter` clone shape.** The runtime struct holds `Vec<RuntimeHeaderMutation>` directly (not `Arc<Vec<RuntimeHeaderMutation>>`). The per-request clone at the H1/H2 HCM site copies the Vec. For 07.2's typical fixture (2-4 mutations across request + response), the clone is cheap (~30 bytes per entry). If a future deployment lands a HeaderMutation with hundreds of entries, the planner refactors to `Arc<Vec<...>>` at that time. Per YAGNI, the Arc-wrap is not pre-projected.

**Signpost 4 — Header normalization on Append vs Overwrite.** The `key` in `RuntimeHeaderMutation` is lowercased at build time (Task 3). On Append, the key is pushed onto `headers` as-is (lowercase). On Overwrite, the search uses the lowercase key against `to_ascii_lowercase()` of each existing entry. Both produce wire-consistent results when the codec re-serializes to lowercase. **Pitfall to avoid:** do NOT preserve the operator's original case (`X-Foo` vs `x-foo`) on the wire — the 04.x codec normalizes to lowercase, and the differential harness's `set_equal_modulo_allow_list` rule is case-sensitive on the comparison side. Mirroring Envoy's behavior: Envoy normalizes to lowercase on the H2 side (per RFC 7540 §8.1.2 — pseudo-header names are lowercase; regular header names are lowercase by convention) and preserves case on H1 (RFC 7230 §3.2.2 — case-insensitive). envoy-rust normalizes to lowercase on both sides per the 04.x posture; this is regression-equivalent on the fixture-level differential.

**Signpost 5 — `:authority` and pseudo-headers in HeaderMutation.** Envoy v1.33's HeaderMutation can mutate the `:path` / `:method` / `:authority` pseudo-headers (per the proto's `HeaderValue.key` field accepting `:`-prefixed names). 07.2's MVP scope does NOT enforce a `:`-prefix-blocklist — operators may attempt to mutate `:path` if their proxy stack supports it. envoy-rust's H1 codec stores `:method` and `:path` in `Request.method` / `Request.path` fields rather than in the `headers` Vec; mutations targeting `:method` or `:path` keys land in `headers` as a regular entry with the literal `:method` / `:path` key, which is wire-equivalent to "no effect" on the H1 codec but visible on the H2 codec. **Recommended posture:** document this as out-of-scope at 07.2 SPEC §4 and leave the behavior diff-equivalent (both proxies treat pseudo-header mutations as data-plane no-ops on the H1 side at least). If empirical fixture-running surfaces a divergence, land a follow-up at 07.2 state 5 or carry forward to phase 08+.

**Signpost 6 — Fuzz corpus seed location.** The corpus directory at `crates/envoy-config/fuzz/corpus/parse_bootstrap/` has explicit `.gitignore` allow-listing per the 06.3 / 05.2 / 05.3 pattern. The Task 6 seed lands in this directory with a `.gitignore` allow-list entry.

**Signpost 7 — Per-task PROGRESS test-bucket attestation.** Every code-changing task (Tasks 1-9) MUST include in its PROGRESS entry the workspace-test-bucket result. Task 5 + Task 9 MUST additionally run the in-process integration tests (the H1/H2 backstops). Task 8 MUST be Docker-gated-fixture-run before commit OR if Docker is unavailable, the Task 8 commit defers to a CI-run-then-commit cadence (push branch, wait for CI green, then land the commit message). This is the parent SPEC §8 R-1 discipline closing 06.3 REVIEW I1.

**Signpost 8 — `apply_mutations` ordering.** The function iterates `mutations` in slice order. For Append, this means the order of entries in the YAML config matches the order entries are pushed onto `headers` (last-pushed wins on Overwrite). For Overwrite, the last entry with a given key wins (since each Overwrite removes prior entries with the same key). This matches Envoy v1.33's documented HeaderMutation semantics.

**Signpost 9 — Helper bind shape.** The Http1EchoBackend helper binds `0.0.0.0` per 05.4 ADR-0024. The fixture 0013's `envoy-rust.yaml` cluster uses `127.0.0.1` or `host.docker.internal` per the fixture-side convention (matches fixture 0008's shape). The planner verifies the helper's bind-address matches the cluster's endpoint-address per the 05.4 hardening posture.

**Signpost 10 — Echoed body order.** The Http1EchoBackend's body echo (verified at Task 7) emits headers sorted by name. Without sort, the echo order would be non-deterministic across runs (HashMap iteration order); with sort, the body bytes are run-to-run identical, which is what `expected_body: byte_exact` requires.

---

## 7. ADRs expected from this sub-phase

**No ADRs are pre-projected to land in 07.2** under the recommended posture per parent-07 SPEC §7. ADR-0030 (parent-07 split decision) landed at the parent-07 state-2 commit. Conditional ADRs:

- **Conditional ADR-0031 (foundations grant for `async_trait` or similar).** Not pre-projected. The HeaderMutation filter is synchronous (manipulates `Vec<(String, String)>` directly); no async iteration is needed. If a 07.2 task surfaces an async requirement (extremely unlikely under the recommended posture), an in-execution ADR per D-3.5 lands inline.

- **Conditional ADR-0032 (Cargo.lock cadence ratification).** Conditional on ADR-0031 actually landing. Phase-04.1 REVIEW M5/M9 carries forward unchanged if ADR-0031 does not land.

- **Conditional ADR-0033+ (sub-phase-specific decisions).** Each 07.2 task may surface unanticipated decisions worth an ADR. Recommended candidates:
  - **The per-route `typed_per_filter_config` scaffolding decision.** Parent SPEC §4 + 07.1 SPEC §4 recommended deferring per-route scaffolding entirely. If 07.2 needs to consume per-route overrides for HeaderMutation (it doesn't under the recommended posture), an ADR lands.
  - **Pseudo-header mutation semantics.** If empirical fixture-running surfaces a divergence between Envoy and envoy-rust on `:authority` / `:path` mutations, an ADR codifies the posture.
  - **The case-insensitive Overwrite semantic.** Mirrors Envoy v1.33 but is not strictly RFC-mandated. If a reviewer flags it as needing codification, an ADR lands.

**DECISIONS.md ledger head at 07.2 entrance:** **ADR-0030** (landed at parent-07 state-2). At 07.2 state-6 close under recommended posture: **ADR-0030** (unchanged). If conditional ADRs landed during 07.2 execution: ledger head advances accordingly; the state-6 commit message enumerates them in the bracketed `[ADR-NNNN]` list.

---

## 8. State-machine signposts for 07.2's own state-2 session

07.2's state-2 session (the session immediately following 07.1's state-6 close-out + the 07.2 state-1 brainstorm) operates per `SKILL_ROUTING.md` line 21. Per the 04.3 / 05.1 / 05.4 / 06.1 / 06.2 / 06.3 standardized standalone-PLAN cadence, 07.2's state-2 session lands:

1. **`docs/envoy-rust/phases/07.2-header-mutation-filter/PLAN.md`** — the standalone PLAN.md, decomposing this SPEC's 10 tasks into per-step TDD checklists. Mirrors 06.3's `3a964cc` shape.
2. **STATE.md** advanced from `07.2 state 2` → `07.2 state 3`; next-skill `superpowers:subagent-driven-development` per the user's standing preference.

The standalone PLAN.md commit is the ONLY artifact of 07.2's state-2 session per the "one state per session" doctrine. No code changes; no ADRs.

**Standalone PLAN.md commit message** (per 06.x precedent):

```
phase 07.2: state-2 standalone PLAN.md (10 tasks; ~1600 LoC projected)

<1-3 sentence summary covering the 10-task decomposition, the recommended
execution order, and the parent-07 close-out at Task 10's state-6.>
```

After 07.2 state-2 lands, the next session enters 07.2 state 3 — runs `superpowers:subagent-driven-development` scoped to 07.2's surface, dispatches Task 1 to the first subagent.

---

## 9. Commit message format (for state 6 of the 07.2 lifecycle and parent-07 close)

The 07.2 state-6 close-out commit uses the standard format from `BOOTSTRAP_PROMPT.md` §5.3 with the `[parent 07 done]` tag attached (mirrors phase-06's `b918f33`):

```
phase 07.2: envoy.filters.http.header_mutation + fixture 0013 [parent 07 done] [ADR-NNNN, ...]

<1-3 sentence summary covering the HeaderMutation filter end-to-end, the
fixture 0013 bilateral assertion, and the parent-07 close-out. Highlights
the wire-emitted equivalence of the request-side stamp at backend (via
Http1EchoBackend body echo) and the response-side stamp at client.>

Differential surface: fixtures 0001-tcp-echo through 0013-http-filter-header-mutation all green at the Docker-gated CI level simultaneously; HeaderMutation filter bilaterally verified on both decode and encode iteration states under the Router-terminated chain.
Conformance: h2spec ≥95% pass (carried forward from 05.2 baseline; phase 07.2 engages no H2-framing surfaces).
```

The `[parent 07 done]` tag attaches to the commit title per the closing-sub-phase convention. The bracketed ADR list enumerates ADRs landed across the parent-07 execution arc — at minimum **ADR-0030** (split decision); plus any conditional ADRs that landed during 07.x execution. If no conditional ADRs landed, the bracketed list reads `[ADR-0030]`.

---

## 10. State-machine commit (the parent-07 state-2 split commit; this SPEC's landing commit)

This SPEC lands at the parent-07 state-2 split commit alongside:

- **ADR-0030** (parent-07 split decision; appended to `docs/envoy-rust/DECISIONS.md` per D-3.5).
- **Sibling sub-phase SPEC** at `docs/envoy-rust/phases/07.1-filter-framework-foundation/SPEC.md`.
- **ROADMAP.md** — new rows for `07.1` + `07.2` with `status: planned`; parent row `07`'s `sub-phases` column updates from `—` to `07.1, 07.2`; row `07`'s `status` stays `in-progress`.
- **STATE.md** advanced to point at `07.1` lifecycle state 1 (next-skill `superpowers:brainstorming` scoped to 07.1's surface).

No code changes. No fixture changes. Doc-only commit.

Mirrors phase-05 state-2 commit `f1804a7` shape and phase-04 state-2 commit `1d9740d` shape. Per `BOOTSTRAP_PROMPT.md` §5.1 ("one state per session; do not chain states"), the parent-07 state-2 commit is the ONLY artifact this session lands; 07.2 does not begin until 07.1's state-6 close-out lands (per the sub-phase ordering invariant in parent-07 SPEC §5).
