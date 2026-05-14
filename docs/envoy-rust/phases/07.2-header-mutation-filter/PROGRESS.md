# Phase 07.2 (`07.2-header-mutation-filter`) — PROGRESS

> Per-task narrative log. CREATED at the state-2 standalone-PLAN.md commit with
> the Task 1 preamble (the `dc00750` 06.2 cadence — back to the 06.x norm;
> divergence from 07.1's "PROGRESS created at Task 1"). Each subsequent task
> commit appends a per-task section: work summary, tests landed (names + LoC
> tally), per-task deviations from PLAN (D-3.5 append-only discipline), LoC
> delta, and the test-bucket attestation. Stranger-readable per D-3.4.

## Task 1 preamble — PLAN-write SPEC corrections + architecture-decision lock-ins

This preamble lands at the state-2 standalone-PLAN.md commit (alongside
`PLAN.md`); it carries NO code. The Task 1 *implementation* narrative appends
below it at Task 1's own state-3 commit.

### PLAN-write SPEC corrections (8)

The 07.2 SPEC landed at the parent-07 state-2 split commit `6db5a01`, BEFORE
the 07.1 execution arc. Eight SPEC details drifted against the 07.1-landed
tree (verified against HEAD `3abcc8c`). Per the user's standing preference
`feedback_pick_recommendation`, each correction picks the working option; all
are folded into the PLAN's task steps. Full text in PLAN.md "PLAN-write SPEC
corrections" — summarized here:

1. **`header_mutation.rs` uses `FilterRequest` / `FilterResponse`**, not
   `envoy_http1::codec::{Request, Response}`. ADR-0031 (07.1 Task 5.5) re-homed
   filter-visible types into `envoy-filter::types` and removed `envoy-http1`
   from `envoy-filter`'s deps. The SPEC §3 Task 3/4 code blocks predate ADR-0031.
2. **Signpost 2's `#[cfg(test)]` test-only `HttpFilterInstance` variant does not
   work cross-crate** — `#[cfg(test)]` in `envoy-filter` is not active when
   `envoy-http1` / `envoy-http2` compile their test suites. Use the SPEC's own
   documented "Alternative — visible-via-feature-flag": a `test-util` Cargo
   feature on `envoy-filter` (Task 5 group B). Within the SPEC's offered option
   space — not ADR-worthy.
3. **Task 5's "deferred test stubs 3-7" are net-new tests**, not stubs to fill
   in. The 07.1 commits `84d68c1` / `3e041c5` deferred *writing* them to 07.2
   Task 5; no placeholder functions exist in the `hcm.rs` files.
4. **Fixture 0013's `expectations.yaml` mirrors fixture 0008's actual shape**
   (`driver: { kind: http1, method, path, host, expected_status, expected_body:
   { kind: byte_exact, body }, expected_headers: set_equal_modulo_allow_list }`
   + `equivalence:` block), not the SPEC §3 Task 8 sketch.
5. **No existing RFC 7230 token helper to reuse** — Task 2 lands
   `is_valid_rfc7230_token` inline in `bootstrap.rs` (the 04.2 HeaderMatcher
   work referenced RFC 7230 in comments only; no token-set *validator* exists).
6. **`ConfigError` lives in `crates/envoy-config/src/lib.rs`**, not
   `bootstrap.rs` — the 3 new variants append to `lib.rs`; the validator
   function + helper land in `bootstrap.rs` (mirrors the 07.1 Task 4 split).
7. **New schema types follow the existing derive convention** —
   `#[derive(Debug, Deserialize, PartialEq)]` + `#[serde(deny_unknown_fields)]`
   (not `Clone` / `Serialize` as the SPEC §3 Task 1 block shows); `AppendAction`
   adds `Clone, Copy, Eq`; the existing `HttpFilterTypedConfig` keeps its
   `#[serde(tag = "@type", deny_unknown_fields)]`.
8. **The `http1-echo-server` helper is a standalone subprocess binary**, not a
   library exposing `serve_ephemeral()` — Task 9's in-process backstop follows
   the `crates/envoy-bin/tests/http1_router_upstream.rs` precedent (inline
   upstream + `format!` YAML + `tempfile::tempdir()` + `CARGO_BIN_EXE_envoy-bin`);
   its inline upstream echoes request headers into the body.

### Architecture-decision lock-ins (per `feedback_pick_recommendation`)

All 10 SPEC §6 signposts + 6 additional decisions locked at PLAN-write time —
full table in PLAN.md "Architecture decisions locked at PLAN-write time".
Headline picks: no RFC 7230 helper exists → land inline at Task 2 (signpost 1);
`test-util` Cargo feature for the StopAndSend stubs (signpost 2); `Vec`-held
mutation lists, not `Arc` (signpost 3); lowercase-key-at-build-time normalization
(signpost 4); pseudo-header mutation out-of-scope, diff-equivalent no-op
(signpost 5); slice-order `apply_mutations` (signpost 8); helper already echoes
sorted headers — Task 7 verify-only (signpost 10). Carryforwards: **07.1 REVIEW
I1** (`finalize_h2_stream` 3-dead-parameter cleanup) is the named structural
prerequisite of Task 5 (Step group A); **07.1 REVIEW M1** (unused `tracing` dep)
closes at Task 3; **07.1 REVIEW M2** (`UnsupportedFilterType` constructable)
partially closes at Task 3 (`RouterNotTerminal` / `DuplicateRouter` stay
defense-in-depth-only). **No new ADRs** — ledger head stays ADR-0031; ADR-0032
reserved-available. **No new top-level Cargo deps.** **Every code-changing
task's PROGRESS attestation MUST quote `cargo deny check` output** (07.1-REVIEW
doctrine reminder — 07.1 CI run `25758889478` failed at `cargo deny check`).

### Split-gate evaluation

10 tasks (< 25-task gate). ~1600 LoC projected (production ~440; tests ~740;
fixture/doc ~410) — ~+7% over the ~1500-LoC soft gate, test/fixture-concentrated.
**Accept the drift; do NOT nest-split** — parent-07 SPEC §5 + ADR-0030 reject
nested splits of a split-produced sub-phase; the 06.x accept-drift precedent
(06.1 SPEC ~1300 → PLAN ~2010 LoC) ratifies. In-execution release valve if a
task inflates past ~10 sub-steps: per-step commit splitting recorded in PROGRESS
(e.g. Task 5a/5b/5c), NOT a phase-level nest-split.

### LoC ground-truth (per 07.2 SPEC §3 task budgets)

Task 1 ~200 + Task 2 ~170 + Task 3 ~210 + Task 4 ~185 + Task 5 ~300 + Task 6 ~52
+ Task 7 ~0-30 + Task 8 ~290 + Task 9 ~150 + Task 10 ~30 = ~1587-1617 LoC of
net change across the substantive surface. PLAN.md narrative overhead is
separate (~2900 lines). State-4 re-checkpoint at Task 10.

---

## Task 1 — `envoy-config` schema additions for HeaderMutation

**State-3 commit.** Lands the `HttpFilterTypedConfig::HeaderMutation` enum variant,
5 supporting structs, and the `AppendAction` enum in `bootstrap.rs`; extends the
`pub use bootstrap::{...}` re-export list in `lib.rs`; and lands a 12-test
`header_mutation_schema_tests` module. TDD order was followed: tests written first
(RED — compile error `cannot find type HeaderMutationConfig`), schema types added,
tests turned GREEN.

### Work summary

- **`crates/envoy-config/src/bootstrap.rs`**: extended `HttpFilterTypedConfig` with
  a `HeaderMutation(HeaderMutationConfig)` variant (keeping the existing
  `#[serde(tag = "@type", deny_unknown_fields)]`); added `HeaderMutationConfig`,
  `Mutations`, `HeaderMutationEntry`, `HeaderValueOption`, `HeaderValue` (each with
  `#[derive(Debug, Deserialize, PartialEq)]` + `#[serde(deny_unknown_fields)]`), and
  `AppendAction` (`#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]` +
  `#[serde(rename_all = "SCREAMING_SNAKE_CASE")]`). Added the
  `header_mutation_schema_tests` nested module (12 tests). Also added a
  `HeaderMutation` match arm to the `validate_http_filters` match (Task 2 fills in
  validation; the arm is a pass-through with comment at Task 1 scope). Also added a
  stub arm to `crates/envoy-filter/src/instance.rs` (see deviation note below).

- **`crates/envoy-config/src/lib.rs`**: extended `pub use bootstrap::{...}` with
  `AppendAction, HeaderMutationConfig, HeaderMutationEntry, HeaderValue,
  HeaderValueOption, Mutations` (6 new names, alphabetized into the existing list).

### Tests landed (12 tests in `bootstrap::tests::header_mutation_schema_tests`)

1. `minimal_request_only_mutations_parse` — parses a single request mutation; asserts key/value/action.
2. `minimal_response_only_mutations_parse` — parses a single response mutation; asserts action.
3. `both_request_and_response_mutations_parse` — parses both sides simultaneously.
4. `empty_mutations_parse_via_serde_default` — `mutations: {}` yields empty Vecs via `#[serde(default)]`.
5. `multiple_entries_parse` — 3 request entries; asserts len.
6. `both_supported_append_actions_parse` — `APPEND_IF_EXISTS_OR_ADD` + `OVERWRITE_IF_EXISTS_OR_ADD`.
7. `unsupported_append_actions_parse_at_schema_level` — `ADD_IF_ABSENT` + `OVERWRITE_IF_EXISTS` parse at schema; Task 2 validator rejects them.
8. `unknown_field_rejects` — `bogus_key` in `mutations` triggers `deny_unknown_fields`.
9. `missing_mutations_field_rejects` — top-level key other than `mutations` rejected.
10. `missing_key_field_rejects` — `header` with only `value` (no `key`) rejected.
11. `missing_value_field_rejects` — `header` with only `key` (no `value`) rejected.
12. `unknown_at_type_url_rejects_on_http_filter` — an unknown `@type` suffix on the tagged enum rejects.

Test module LoC: ~90 lines. Schema types LoC: ~75 lines. `lib.rs` re-export edit: ~12 lines.

### LoC delta

```
crates/envoy-config/src/bootstrap.rs  +207 lines
crates/envoy-config/src/lib.rs          +12 lines (net; reformat of existing re-export list)
crates/envoy-filter/src/instance.rs     +7 lines (deviation stub — see below)
Total: +226 insertions, -10 deletions (net ~216 LoC added)
```

PLAN budget for Task 1: ~200 LoC. Actual ~216 LoC net — within acceptable range.

### Deviations from PLAN

1. **`crates/envoy-filter/src/instance.rs` touched at Task 1 (not Task 3).**
   Adding `HttpFilterTypedConfig::HeaderMutation` to `envoy-config` immediately broke
   the exhaustive `match &hf.typed_config` in `envoy-filter/src/instance.rs::build()`.
   The workspace would not compile without a new arm. Added a stub arm returning
   `Err(FilterError::UnsupportedFilterType { position, name })` with a comment
   "Task 3 replaces this stub". This is a forward-compatible stub — Task 3 replaces it
   with `HeaderMutationFilter::build_from_config`. This also partially addresses 07.1
   REVIEW M2 (`UnsupportedFilterType` becomes first-constructable here; full close at
   Task 3 as planned).

2. **PLAN's test module used `use crate::{AppendAction, HeaderMutationConfig, Mutations}`
   but `Mutations` is unused in the test body** (accessed only via `cfg.mutations`
   field, not as a direct type constructor). Removed `Mutations` from the import to
   satisfy `cargo clippy -- -D warnings` (unused import lint). The 12 test assertions
   are unaffected.

3. **`cargo fmt` reformatted `unknown_field_rejects`** — the multi-line
   `parse("...", ).expect_err("...")` call was collapsed to a single-line chain
   `parse("...").expect_err("...")`. Accepted — no semantic change.

### Test-bucket attestation

All 5 workspace gate commands run and clean:

**`cargo build --workspace --all-targets`**
```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 15.44s
```

**`cargo clippy --workspace --all-targets --all-features -- -D warnings`**
```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 19.17s
```

**`cargo fmt --all -- --check`**
```
(no output — clean)
```

**`cargo test --workspace`**
All 55 test suites passed (0 failed). The `differential` crate's
`tcp_proxy_backend_*` tests flaked once with "Connection refused" on first run (a
pre-existing port-readiness race unrelated to Task 1 changes); the second and third
runs were clean. 12 new tests in `envoy-config`.

**`cargo deny check`**
```
warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:49:6
   │
49 │     "0BSD",
   │      ━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:40:6
   │
40 │     "BSD-2-Clause",
   │      ━━━━━━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:47:6
   │
47 │     "MPL-2.0",
   │      ━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:43:6
   │
43 │     "Unicode-DFS-2016",
   │      ━━━━━━━━━━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:45:6
   │
45 │     "Zlib",
   │      ━━━━ unmatched license allowance

advisories ok, bans ok, licenses ok, sources ok
```

No new top-level Cargo dependencies. The `license-not-encountered` warnings are
pre-existing (unmatched allowlist entries in `deny.toml` — not new from Task 1).

### Review fixes (post-commit code-quality pass)

Three fixes applied to commit `c4fd17f` (amended in-place; not yet pushed):

1. **Important — `_position` rename** (`crates/envoy-filter/src/instance.rs`, `build`
   function signature): the parameter was declared `_position: usize` (underscore signals
   "intentionally unused") but was actively used in the `HeaderMutation` stub arm as
   `position: _position`. Renamed to `position: usize` throughout; the struct shorthand
   `position` (field init shorthand) replaces the explicit `position: _position` binding.
   The `_cfg` bindings in the match arms remain underscore-prefixed — they are genuinely
   unused and correctly annotated.

2. **Minor — redundant `#[cfg(test)]` removed** (`crates/envoy-config/src/bootstrap.rs`,
   `header_mutation_schema_tests` module): the inner module was nested inside the outer
   `#[cfg(test)] mod tests { ... }` block and carried its own redundant `#[cfg(test)]`
   attribute. Removed the inner attribute to match the sibling `validate_http_filters_tests`
   module, which carries no such attribute.

3. **Minor — strengthened assertion in `both_request_and_response_mutations_parse`**
   (`crates/envoy-config/src/bootstrap.rs`): the test previously asserted only that both
   mutation lists had length 1. Added key-equality assertions
   (`request_mutations[0].append.header.key == "x-req"` and
   `response_mutations[0].append.header.key == "x-resp"`) so a request/response mix-up
   would be caught. Keys confirmed against the test YAML embedded in the same test.

Minor #3 from the reviewer's full list (raw-string test literals) was deliberately NOT
changed — those literals are PLAN-verbatim encoding and the reviewer agreed they were not
worth changing at this stage.

---

## Task 2 — `envoy-config` validator extension + 3 new `ConfigError` variants

**State-3 commit.** Replaces the Task-1 pass-through `HeaderMutation` arm in
`validate_http_filters` with the real validating arm; adds two free functions
(`validate_header_mutation_entries`, `is_valid_rfc7230_token`) in `bootstrap.rs`;
appends 3 `ConfigError` variants to `lib.rs`; and lands an 8-test
`header_mutation_validator_tests` module. TDD order was followed: tests written first
(RED — compile error, 4 missing `ConfigError` variants), `ConfigError` variants added to
`lib.rs`, validator + helpers added to `bootstrap.rs`, tests turned GREEN (8/8 PASS).

### Work summary

- **`crates/envoy-config/src/lib.rs`**: appended 3 `ConfigError` variants after
  `Http2ClusterFromHttp1Listener`:
  - `UnsupportedHeaderMutationAppendAction { listener, position, action }` — fires for
    `ADD_IF_ABSENT` or `OVERWRITE_IF_EXISTS` (parse but are rejected by the validator).
  - `EmptyHeaderMutationKey { listener, position }` — fires when `header.key` is `""`.
  - `InvalidHeaderMutationKey { listener, position, key }` — fires when `header.key`
    fails RFC 7230 §3.2.6 token validation.

- **`crates/envoy-config/src/bootstrap.rs`**:
  - Replaced the Task-1 no-op `HeaderMutation` arm in `validate_http_filters` with the
    real arm: name-mismatch check + calls to `validate_header_mutation_entries` for both
    `request_mutations` and `response_mutations`.
  - Added `validate_header_mutation_entries(entries, listener_name)` — iterates entries,
    checks non-empty key, RFC 7230 token validity, and supported `append_action` subset.
  - Added `is_valid_rfc7230_token(s)` — RFC 7230 §3.2.6 `tchar` validation landed inline
    per PLAN-write SPEC correction 5 (no existing token-set validator in `envoy-config`).
  - Added `mod header_mutation_validator_tests` with 8 tests inside `#[cfg(test)] mod tests`.

### Tests landed (8 tests in `bootstrap::tests::header_mutation_validator_tests`)

1. `header_mutation_with_all_supported_entries_passes` — all 4 supported action variants
   (2 request + 2 response) pass the validator.
2. `empty_key_rejects` — `header.key = ""` triggers `EmptyHeaderMutationKey`.
3. `invalid_token_in_key_rejects` — `"x bad"` (space) triggers `InvalidHeaderMutationKey`.
4. `add_if_absent_rejects` — `ADD_IF_ABSENT` triggers `UnsupportedHeaderMutationAppendAction`
   with `action = "ADD_IF_ABSENT"`.
5. `overwrite_if_exists_rejects` — `OVERWRITE_IF_EXISTS` (in response mutations) triggers
   `UnsupportedHeaderMutationAppendAction` with `action = "OVERWRITE_IF_EXISTS"`.
6. `router_not_terminal_still_rejects_under_header_mutation_chain` — `[Router, HeaderMutation]`
   ordering still fires `RouterNotTerminal` (07.1 Task 4 validator unchanged).
7. `duplicate_router_rejects_under_header_mutation_chain` — `[HeaderMutation, Router, Router]`
   still fires `DuplicateRouterFilter`.
8. `name_typed_config_mismatch_rejects` — a `HeaderMutation` typed_config paired with
   `name = "envoy.filters.http.fault"` fires `UnsupportedHttpFilter`.

Test module LoC: ~160 lines. Validator + helpers LoC: ~70 lines. `lib.rs` variants: ~28 lines.

Task 1's 12 `header_mutation_schema_tests` confirmed still PASS (12/12) after Task 2 changes.

### LoC delta

```
crates/envoy-config/src/bootstrap.rs  +266 lines, -2 lines (net +264)
crates/envoy-config/src/lib.rs          +30 lines, -0 lines (net +30)
Total: +294 insertions, -2 deletions (net ~292 LoC added)
```

PLAN budget for Task 2: ~170 LoC. Actual ~292 LoC net — overage concentrated entirely in the
test module (~160 LoC vs. ~60 LoC budgeted). The 8 test functions expanded to full match-arm
coverage (3 struct fields each) under `cargo fmt`'s formatting, which added lines. Production
code (validator + helpers + `ConfigError` variants) is ~100 LoC — within +25% of the ~80 LoC
PLAN estimate.

### Deviations from PLAN

1. **`cargo fmt` reformatted PLAN-verbatim code blocks.** The PLAN's Step 4 code blocks used
   inline struct initializers (`HeaderValue { key: k.to_string(), value: v.to_string() }` on one
   line) and multi-line `validate_header_mutation_entries(...)` calls. `cargo fmt` expanded struct
   initializers to multi-line form and collapsed `validate_header_mutation_entries(...)` calls to
   single lines. Also reformatted the `EmptyHeaderMutationKey` `#[error(...)]` attribute and
   the `InvalidHeaderMutationKey` struct fields in `lib.rs`. No semantic changes — formatting only.
   The test module's `use crate::{...}` import line was also re-wrapped to fit the column limit.

### Test-bucket attestation

All 5 workspace gate commands run and clean:

**`cargo build --workspace --all-targets`**
```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 19.30s
```

**`cargo clippy --workspace --all-targets --all-features -- -D warnings`**
```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 19.09s
```

**`cargo fmt --all -- --check`**
```
(no output — clean)
```

**`cargo test --workspace`**
All test suites passed (0 failed) on the second run. The `differential` crate's
`http1_echo_backend_drop_terminates_child` and `http1_echo_backend_spawns_and_echoes`
tests flaked on the first run with "http1-echo-server never became accept-ready" (the
pre-existing port-readiness race documented at Task 1); the second run was clean (77 passed,
0 failed in the differential crate; 8 new tests in `envoy-config::bootstrap::tests::header_mutation_validator_tests`).

**`cargo deny check`**
```
warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:49:6
   │
49 │     "0BSD",
   │      ━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:40:6
   │
40 │     "BSD-2-Clause",
   │      ━━━━━━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:47:6
   │
47 │     "MPL-2.0",
   │      ━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:43:6
   │
43 │     "Unicode-DFS-2016",
   │      ━━━━━━━━━━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:45:6
   │
45 │     "Zlib",
   │      ━━━━ unmatched license allowance

advisories ok, bans ok, licenses ok, sources ok
```

No new top-level Cargo dependencies. The `license-not-encountered` warnings are pre-existing
(unmatched allowlist entries in `deny.toml` — identical to Task 1's attestation).

---

## Task 3 — `HeaderMutationFilter` runtime types + builder + `instance.rs` arm [07.1 REVIEW M1, M2]

**State-3 commit.** Creates `crates/envoy-filter/src/header_mutation.rs` with the
`HeaderMutationFilter` struct, `RuntimeHeaderMutation`, `RuntimeAppendAction`, `build_from_config`,
`map_entry`, and stub `decode_headers` / `encode_headers` (returning `Decision::Continue`); wires
`pub mod header_mutation;` + `pub use header_mutation::HeaderMutationFilter;` into `lib.rs`;
replaces the Task-1 stub arm in `instance.rs::build()` with the real
`HeaderMutationFilter::build_from_config`-backed arm and adds the `HeaderMutation(HeaderMutationFilter)`
variant + `decode_headers` / `encode_headers` arms; removes the unused `tracing = "0.1"` dep from
`Cargo.toml` (closes 07.1 REVIEW M1). TDD order was followed: tests written first (RED — compile
errors `HeaderMutationFilter` / `RuntimeAppendAction` not found), implementation added, all 7 new
tests turned GREEN (20 total in `envoy-filter`).

### Work summary

- **`crates/envoy-filter/src/header_mutation.rs`** (new file): 7 test functions in
  `#[cfg(test)] mod tests`, runtime types (`HeaderMutationFilter`, `RuntimeHeaderMutation`,
  `RuntimeAppendAction`), `build_from_config`, `map_entry` (defense-in-depth re-check for
  unsupported `AppendAction` variants), and stub `decode_headers` / `encode_headers`. Key
  design choices: `request_mutations` / `response_mutations` fields carry `#[allow(dead_code)]`
  (stubs; real use lands at Task 4); `RuntimeHeaderMutation` fields likewise annotated. Keys
  lowercased at build time per signpost 4.

- **`crates/envoy-filter/src/lib.rs`**: added `pub mod header_mutation;` (alphabetically after
  `pub mod error;`) + `pub use header_mutation::HeaderMutationFilter;`.

- **`crates/envoy-filter/src/instance.rs`**: added `use crate::header_mutation::HeaderMutationFilter;`;
  added `HeaderMutation(HeaderMutationFilter)` to the `HttpFilterInstance` enum; replaced the Task-1
  stub arm (`Err(FilterError::UnsupportedFilterType {...})`) with the real arm (`Ok(HttpFilterInstance::
  HeaderMutation(HeaderMutationFilter::build_from_config(cfg)?))`); added `HeaderMutation(f) =>
  f.decode_headers(req)` and `HeaderMutation(f) => f.encode_headers(resp)` arms.

- **`crates/envoy-filter/Cargo.toml`**: removed `tracing = "0.1"` from `[dependencies]`. After
  removal: `bytes = "1"` + `thiserror = "2"` + `envoy-config = { path = "../envoy-config" }`.
  Verified with `grep -rn 'tracing' crates/envoy-filter/src/` — no hits.

### Tests landed (7 new tests in `header_mutation::tests`)

1. `build_from_config_on_empty_mutations_returns_empty_filter` — empty config yields empty
   `request_mutations` and `response_mutations` Vecs.
2. `build_from_config_on_single_append_entry_lowercases_key_and_keeps_value` — `"X-Foo"`
   is lowercased to `"x-foo"` at build time; value `"Bar"` preserved verbatim; action maps to
   `RuntimeAppendAction::Append`.
3. `build_from_config_on_single_overwrite_entry_maps_action` — `OverwriteIfExistsOrAdd` maps
   to `RuntimeAppendAction::Overwrite`.
4. `build_from_config_on_unsupported_append_action_returns_err` — `AddIfAbsent` triggers
   `FilterError::UnsupportedFilterType` at the framework boundary (defense-in-depth).
5. `http_filter_instance_build_on_header_mutation_produces_header_mutation_variant` — the real
   `instance.rs::build()` arm produces `HttpFilterInstance::HeaderMutation(_)`.
6. `decode_headers_stub_returns_continue_at_task_3` — Task 3 stub returns `Decision::Continue`
   (replaced at Task 4).
7. `encode_headers_stub_returns_continue_at_task_3` — Task 3 stub returns `Decision::Continue`
   (replaced at Task 4).

Test module LoC: ~80 lines. Runtime types + builder + stubs LoC: ~90 lines. `lib.rs` edit: ~3 lines.
`instance.rs` edits: ~15 lines. `Cargo.toml` edit: ~1 line removal.

### LoC delta

```
crates/envoy-filter/src/header_mutation.rs   +248 lines (new file)
crates/envoy-filter/src/lib.rs                 +2 lines
crates/envoy-filter/src/instance.rs           +10 lines, -7 lines (net +3)
crates/envoy-filter/Cargo.toml                  -1 line (tracing dep removed)
Total: ~+252 insertions, -8 deletions (net ~244 LoC added)
```

PLAN budget for Task 3: ~210 LoC. Actual ~244 LoC net — overage of ~34 LoC concentrated in the
test module (`#[allow(dead_code)]` comments + `cargo fmt` multi-line expansions of struct
initializers and `matches!` assertions).

### Deviations from PLAN

1. **`#[allow(dead_code)]` added to `HeaderMutationFilter` and `RuntimeHeaderMutation` fields.**
   The PLAN's Task 3 code block did not include these attributes. At Task 3, `request_mutations`,
   `response_mutations`, `key`, `value`, and `action` are set but never read from non-test code
   (the stubs `decode_headers`/`encode_headers` ignore them; the real use is at Task 4). Without
   `#[allow(dead_code)]`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`
   fails with `fields ... are never read`. Per the PLAN's workspace-gate requirement (all 5
   commands clean), the attributes were added with comments noting they are consumed at Task 4.
   Not an ADR-worthy decision; aligns with the PLAN's `D-3.2` stub posture.

2. **`instance.rs::build` parameter renamed `_position` (re-restoration).** The Task 1
   deviation (PROGRESS Task 1, deviation 1) had renamed the parameter to `position` (non-underscore)
   because the stub arm used it. With the stub replaced by the real arm (which does not use the
   position parameter — `map_entry` hardcodes `position: 0`), the parameter is unused again.
   Renamed back to `_position` to satisfy `cargo clippy -- -D warnings`. Consistent with the
   PLAN's note "(The `_position` parameter stays `_position`)".

3. **`cargo fmt` reformatted PLAN-verbatim code blocks.** Multi-line expansion of
   `Ok(Self { request_mutations, response_mutations })` → multi-line struct form;
   single-line `matches!(...)` calls for `Overwrite`, `HeaderMutation(_)`, `Continue`
   (×2) → multi-line form. The `entry()` helper's inline `HeaderValue {...}` expanded
   to multi-line; `Mutations { ... }` in `cfg()` likewise. No semantic changes.

### 07.1 REVIEW carryforward status

**07.1 REVIEW M1 — CLOSED.** The unused `tracing = "0.1"` dependency has been removed from
`crates/envoy-filter/Cargo.toml`. `grep -rn 'tracing' crates/envoy-filter/src/` returns no
hits. `cargo deny check` is clean (quoted in attestation below).

**07.1 REVIEW M2 — PARTIALLY CLOSED.** `FilterError::UnsupportedFilterType` is now constructed
by `map_entry` in `header_mutation.rs` (defense-in-depth check for `AddIfAbsent` /
`OverwriteIfExists`). The other two unconstructed variants — `RouterNotTerminal` and
`DuplicateRouter` — remain defense-in-depth-only (the `envoy-config` validator is the
real operator-facing catch for those; no Task 3 code constructs them). Full close deferred
to when a test explicitly exercises those error paths (if ever needed).

### Test-bucket attestation

All 5 workspace gate commands run and clean:

**`cargo build --workspace --all-targets`**
```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.65s
```

**`cargo clippy --workspace --all-targets --all-features -- -D warnings`**
```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.88s
```

**`cargo fmt --all -- --check`**
```
(no output — clean)
```

**`cargo test --workspace`**
All test suites passed (0 failed). 20 tests in `envoy-filter` (13 pre-existing + 7 new
`header_mutation` tests). No flakes observed on this run.

**`cargo deny check`**
```
warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:49:6
   │
49 │     "0BSD",
   │      ━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:40:6
   │
40 │     "BSD-2-Clause",
   │      ━━━━━━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:47:6
   │
47 │     "MPL-2.0",
   │      ━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:43:6
   │
43 │     "Unicode-DFS-2016",
   │      ━━━━━━━━━━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:45:6
   │
45 │     "Zlib",
   │      ━━━━ unmatched license allowance

advisories ok, bans ok, licenses ok, sources ok
```

No new top-level Cargo dependencies (one removed: `tracing = "0.1"`). The `license-not-encountered`
warnings are pre-existing (unmatched allowlist entries in `deny.toml` — identical to Tasks 1 and 2's
attestations). Removing `tracing` introduced no new `cargo deny` concerns.

### Review fixes (post-commit code-quality pass)

Applied to commit `42ce300` (amended in-place; not yet pushed):

1. **Important — `position: 0` clarifying comment** (`crates/envoy-filter/src/header_mutation.rs`,
   `map_entry` function): `position: 0` was hardcoded in the `FilterError::UnsupportedFilterType`
   construction with no explanation. Added a 4-line comment immediately above the field explaining
   that `0` is a placeholder: `map_entry` runs inside `build_from_config` and has no access to the
   filter-chain position; the operator-facing position is carried by the `envoy-config` validator's
   typed errors (the primary catch); this is the defense-in-depth re-check at the framework boundary.

2. **Important — struct-level `#[allow(dead_code)]`** (`crates/envoy-filter/src/header_mutation.rs`):
   five separate `#[allow(dead_code)]` attributes on individual fields of `HeaderMutationFilter` (2
   fields) and `RuntimeHeaderMutation` (3 fields) were replaced with a single `#[allow(dead_code)]`
   on each struct declaration (2 attributes total instead of 5). The explanatory comment (fields only
   read in `#[cfg(test)]` code at Task 3; real readers land at Task 4, which removes the attribute)
   was adapted to sit at struct level instead of per-field.

3. **Minor — `instance.rs` module-doc polish** (`crates/envoy-filter/src/instance.rs`, lines 1-5):
   the module doc still read "At 07.1 the only variant is `Router`..." as if `HeaderMutation` were
   future work. Revised to accurately describe the current state: both `Router(RouterTerminus)` (07.1)
   and `HeaderMutation(HeaderMutationFilter)` (07.2) variants are present.

Two further Minors from the review are deliberately carried forward to Task 4:

- **Minor #4 (Task-4 carry):** The Task-4 implementer should replace the `decode_headers_stub_*`
  and `encode_headers_stub_*` tests with assertions on real mutation effects (not just
  `Decision::Continue`) once the stubs are replaced with the actual semantics.
- **Minor #5 (Task-4 carry):** The Task-4 implementer should verify whether `decode_headers` /
  `encode_headers` need `&mut self` or only `&self` (the fields are read-only after build; `&self`
  may suffice and would be cleaner).

---

## Task 4 — `HeaderMutationFilter::decode_headers` + `encode_headers` semantics

**State-3 commit.** Replaces the Task-3 `decode_headers` / `encode_headers` stubs with the real
append/overwrite iteration semantics; adds the `apply_mutations` free function; replaces the 2
Task-3 stub tests with the 14-test semantics inventory; removes the 2 struct-level
`#[allow(dead_code)]` attributes from `HeaderMutationFilter` and `RuntimeHeaderMutation` (fields
now read by `apply_mutations` in non-test code). Single file changed:
`crates/envoy-filter/src/header_mutation.rs`.

### Work summary

- **`crates/envoy-filter/src/header_mutation.rs`**:
  - **Removed:** 2 struct-level `#[allow(dead_code)]` attributes on `HeaderMutationFilter` and
    `RuntimeHeaderMutation` (committed to Task 4 by the Task 3 review-fix note). Fields are now
    read by `apply_mutations` in non-test code; no dead-code lint fires.
  - **Replaced:** Task-3 stub `decode_headers` (ignored `_req`) with the real body:
    `apply_mutations(&mut req.headers, &self.request_mutations); Decision::Continue`.
  - **Replaced:** Task-3 stub `encode_headers` (ignored `_resp`) with the real body:
    `apply_mutations(&mut resp.headers, &self.response_mutations); Decision::Continue`.
  - **Added:** `apply_mutations(headers, mutations)` free function after `map_entry`. Iterates
    `mutations` in slice (= YAML declaration) order per signpost 8. `Append` pushes; `Overwrite`
    does a case-insensitive `retain` scan then pushes. `mutation.key` is already lowercased at
    build time; the scan calls `k.to_ascii_lowercase()` on each existing entry.
  - **Deleted:** `decode_headers_stub_returns_continue_at_task_3` and
    `encode_headers_stub_returns_continue_at_task_3` test functions (replaced by the 14-test
    semantics inventory below).
  - **Added:** 3 test helpers (`req_with`, `resp_with`, `owned`) and 14 semantics tests (see below).

- **Task-3 carry-forward resolutions:**
  - **Minor #4 resolved:** All 14 semantics tests assert on `req.headers` / `resp.headers`
    contents via `assert_eq!(..., owned(&[...]))`, not just the `Decision` return value.
  - **Minor #5 resolved:** `decode_headers` / `encode_headers` keep `&mut self` as the PLAN's
    Step 3 specifies. `&mut self` is required for interface consistency with the `HttpFilterInstance`
    enum dispatch (the 07.1-landed `Router` arm uses `&mut self`; `FilterPipeline` dispatches all
    filter instances through this uniform signature). `&self` would be technically sufficient for
    `HeaderMutationFilter` alone, but would break the shared dispatch interface. Not an ADR-worthy
    decision; no change made.

### Tests landed

**14 new semantics tests** in `header_mutation::tests` (replaces the 2 Task-3 stub tests):

1. `append_on_absent_key_adds_entry` — Append on empty headers → `[("x-foo", "bar")]`.
2. `append_on_present_key_adds_duplicate` — Append on existing key → 2-entry list (original + new).
3. `overwrite_on_absent_key_adds_entry` — Overwrite on empty headers → `[("x-foo", "bar")]`.
4. `overwrite_on_present_key_replaces_with_exactly_one_entry` — Overwrite on existing key → exactly
   1 entry (old removed, new pushed).
5. `overwrite_is_case_insensitive_on_the_existing_entry` — Existing `"X-Foo"` matched and removed
   by Overwrite of `"x-foo"` (case-fold via `to_ascii_lowercase()`).
6. `multiple_append_entries_apply_in_declaration_order` — 3-entry Append list; asserts
   `[x-a:1, x-b:2, x-a:3]` (slice order preserved).
7. `multiple_overwrite_entries_last_for_a_key_wins` — Two Overwrite entries for same key; asserts
   last value `"second"` survives.
8. `mix_of_append_and_overwrite_applies_in_order` — Append x-a:1, Append x-a:2, Overwrite x-a:final
   → Overwrite removes both prior x-a entries, pushes one; unrelated x-keep header preserved.
9. `empty_mutations_is_no_op_on_decode` — Empty mutation list leaves headers unchanged.
10. `empty_mutations_is_no_op_on_encode` — Empty mutation list leaves response headers unchanged.
11. `decode_headers_returns_continue_after_applying` — Non-empty Append still returns `Decision::Continue`.
12. `encode_headers_returns_continue_after_applying` — Non-empty Append on response still returns
    `Decision::Continue`.
13. `round_trip_via_filter_pipeline_decode` — `[HeaderMutation, Router]` pipeline; decode walks
    declaration order; `x-foo: bar` present in request headers after `pipeline.decode_headers`.
14. `iteration_order_on_encode_via_filter_pipeline` — `[HeaderMutation, Router]` pipeline; encode
    walks reverse order; `x-resp: stamp` present in response headers after `pipeline.encode_headers`.

**3 test helpers** added: `req_with`, `resp_with`, `owned`.

**2 Task-3 stub tests deleted:** `decode_headers_stub_returns_continue_at_task_3`,
`encode_headers_stub_returns_continue_at_task_3`.

Net test change: +14 - 2 = +12 tests. `envoy-filter` total: 32 tests (13 pre-existing + 5
Task-3 build tests + 14 Task-4 semantics tests).

### LoC delta

```
crates/envoy-filter/src/header_mutation.rs   +146 lines, -55 lines (net +91)
```

PLAN budget for Task 4: ~185 LoC. Actual +91 LoC net — below budget. The difference is that the
PLAN's budget included the full file rewrite context; the net delta (additions minus deletions)
reflects the real code changes: +19 LoC semantics (method bodies + `apply_mutations`), +127 LoC
tests (14 tests + 3 helpers), -55 LoC (2 stub tests + 2 `#[allow(dead_code)]` attributes +
`_req`/`_resp` stub bodies).

### Deviations from PLAN

1. **Steps 1+3 implemented simultaneously.** The PLAN's TDD sequence calls for Step 1 (write
   failing tests with stub bodies still in place) → Step 2 (confirm RED) → Step 3 (replace stub
   bodies). Because the final file is written in a single pass as one atomic edit, the RED
   intermediate state was not captured as a separate snapshot. The 14 tests were written first in
   the test module (with the real implementation written immediately after, as a single file
   write). All tests pass GREEN on the first `cargo test` invocation. No semantic deviation — the
   TDD discipline (tests articulate semantics before implementation) was followed in conception
   order; the commit-state order was merged.

2. **`cargo fmt` reformatted two assertions in the PLAN's pipeline tests.** In
   `round_trip_via_filter_pipeline_decode`, `assert!(matches!(pipeline.decode_headers(...),
   Decision::Continue))` was expanded to 4-line form. In
   `iteration_order_on_encode_via_filter_pipeline`, `assert!(resp.headers.iter().any(...))` was
   reformatted to `assert!(resp.headers.iter().any(...))` with internal indentation adjusted.
   No semantic changes.

### Test-bucket attestation

All 5 workspace gate commands run and clean:

**`cargo build --workspace --all-targets`**
```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.76s
```

**`cargo clippy --workspace --all-targets --all-features -- -D warnings`**
```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.37s
```

**`cargo fmt --all -- --check`**
```
(no output — clean)
```

**`cargo test --workspace`**
32 tests in `envoy-filter` (13 pre-existing + 5 Task-3 build tests + 14 Task-4 semantics tests),
all PASS. The `differential` crate's `http1_echo_backend_drop_terminates_child`,
`http1_echo_backend_spawns_and_echoes`, `tcp_proxy_backend_drop_terminates_child`,
`tcp_proxy_backend_spawns_and_echoes`, and `run_fixture_dispatches_http1_backend_on_template_marker`
tests flaked on the first run (pre-existing port-readiness race — documented in Task 1, 2, and 3
attestations); the second run was clean (all suites passed, 0 failed).

**`cargo deny check`**
```
warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:49:6
   │
49 │     "0BSD",
   │      ━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:40:6
   │
40 │     "BSD-2-Clause",
   │      ━━━━━━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:47:6
   │
47 │     "MPL-2.0",
   │      ━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:43:6
   │
43 │     "Unicode-DFS-2016",
   │      ━━━━━━━━━━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:45:6
   │
45 │     "Zlib",
   │      ━━━━ unmatched license allowance

advisories ok, bans ok, licenses ok, sources ok
```

No new top-level Cargo dependencies. The `license-not-encountered` warnings are pre-existing
(unmatched allowlist entries in `deny.toml` — identical to Tasks 1, 2, and 3's attestations).
The 2 `#[allow(dead_code)]` struct-level attributes removed from `header_mutation.rs` introduced
no new `cargo deny` concerns.

---

## Task 5 — H1+H2 HCM filter-chain integration tests + `finalize_h2_stream` cleanup [07.1 REVIEW I1]

**State-3 commit.** Four step groups: **A** = 07.1 REVIEW I1 `finalize_h2_stream` 3-dead-parameter
cleanup (the named structural prerequisite); **B** = `test-util` Cargo feature on `envoy-filter`;
**C** = 5 H1 HCM filter-chain integration tests; **D** = 4 H2 HCM filter-chain integration tests.

### Work summary

**Group A — 07.1 REVIEW I1: `finalize_h2_stream` 3-dead-parameter cleanup**

- **`crates/envoy-http2/src/hcm.rs`** (`finalize_h2_stream` signature): removed
  `_response_status_for_log: u16`, `_response_body_len: u64`, and
  `_response_headers_for_log: &[(String, String)]` from the function signature (lines 436-455 per
  PLAN guidance; actual function at lines 425-444 post-Tasks-1-4 shifts). The function body already
  derived everything from `resp` post-encode via the shadow locals at lines 490-493; the parameters
  were unused-and-discarded, hence the `_` prefix.

- **`crates/envoy-http2/src/hcm.rs`** (3 call sites in `handle_one_stream`): removed the
  3 pre-encode local computations (`response_status_for_log = r.status` / `response_body_len =
  r.body.len() as u64` / `response_headers_for_log = r.headers.clone()`) from all 4 arms that
  assigned them (call-site 1: no-healthy-endpoint → 502; call-site 2: upstream-dispatch-error →
  502; the Proxy arm success path; the SynthFromDecode arm) and removed the 3 args from all 3
  explicit `finalize_h2_stream` calls. The `let response_status_for_log: u16; let response_body_len:
  u64; let response_headers_for_log: Vec<...>;` declarations at the top of the match block were
  also removed.

- Updated the comment at the post-encode shadow-local derivation site to drop the "shadow the
  pre-encode log-locals" framing — it now reads "derive the post-encode log-locals from `resp`".
  `#[allow(clippy::too_many_arguments)]` retained (9 params → still >7).

- **Net removal: ~22 lines** (4 removed params + comment block from signature; 12 removed
  pre-encode local computations across 4 arms; 9 removed call-site args across 3 calls; 3 removed
  `let` declarations). Behavior is completely unchanged — the function body's post-encode shadow
  locals already served all readers; the caller-side pre-encode values were never read.

- **Verification (Step A3):** `cargo test -p envoy-http2` — all 38 tests (including all 13+1
  pre-existing H2 hcm tests) GREEN before any Group B/C/D code was added. Zero behavior impact
  confirmed.

**07.1 REVIEW I1 — CLOSED.**

**Group B — `test-util` Cargo feature on `envoy-filter`**

- **`crates/envoy-filter/Cargo.toml`**: added `[features] test-util = []` with an explanatory
  comment per correction 2 (the `#[cfg(test)]` variant is not cross-crate-visible; `test-util`
  feature is the SPEC's own documented alternative).

- **`crates/envoy-filter/src/instance.rs`**: added `#[cfg(feature = "test-util")]`
  `TestStopAndSendOnDecode(FilterResponse)` and `TestStopAndSendOnEncode(FilterResponse)` variants
  to `HttpFilterInstance`; added cfg-gated match arms to `decode_headers` and `encode_headers`;
  added a cfg-gated `impl HttpFilterInstance` block with `test_stop_and_send_on_decode`,
  `test_stop_and_send_on_encode`, and `test_router` constructors.

  **Deviation from PLAN (minor):** `RouterTerminus::new()` is `pub(crate)` — not externally
  visible. The PLAN's test code calls `envoy_filter::HttpFilterInstance::Router(envoy_filter::
  RouterTerminus::new())`. Added a `test_router()` constructor alongside the two StopAndSend
  constructors under the same `#[cfg(feature = "test-util")]` impl block. The H1/H2 tests use
  `envoy_filter::HttpFilterInstance::test_router()` instead of the direct variant construction.
  This is the same pattern as the StopAndSend constructors — strictly additive.

- **`crates/envoy-filter/src/pipeline.rs`**: added `#[cfg(feature = "test-util")]`
  `FilterPipeline::test_from_instances(filters: Vec<HttpFilterInstance>) -> Self` constructor after
  `build_from_config`.

- **`crates/envoy-http1/Cargo.toml`**: added `[dev-dependencies] envoy-filter = { path =
  "../envoy-filter", features = ["test-util"] }`.

- **`crates/envoy-http2/Cargo.toml`**: same dev-dependency feature line.

**Group C — 5 H1 HCM filter-chain integration tests**

Added to `crates/envoy-http1/src/hcm.rs` `#[cfg(test)] mod tests`:

Two new helpers:
- `hcm_config_with_pipeline(pipeline, prefix, route_status, route_body)` — inlines a struct
  literal (HCMConfig does not derive Clone) mirroring `hcm_config_single_route`'s body, swapping
  `filter_pipeline`. PLAN's fallback instruction ("if `HCMConfig` does not derive `Clone`, inline
  the full struct literal") applied.
- `header_mutation_pipeline(request_mutations, response_mutations)` — builds a `[HeaderMutation,
  Router]` FilterPipeline from `(key, value, AppendAction)` triples.
- `hcm_config_header_matched_route(pipeline)` — HCMConfig with a HeaderMatcher route on
  `x-test-path-override = "/bar"` → 200 "matched\n". Mirrors
  `single_header_matcher_route_selected_when_match`.
- `hcm_config_with_access_log_and_pipeline(pipeline, log_path)` — HCMConfig with a FileSink
  at `log_path` and the caller's pipeline. Mirrors `hcm_config_with_access_log`.

5 new tests (all GREEN on first run):
1. `h1_decode_headers_fires_before_route_match` — decode mutation fires before route-match.
2. `h1_encode_headers_fires_after_writer_arm_before_wire_write` — encode mutation stamp on wire.
3. `h1_stop_and_send_at_decode_skips_route_match` — decode StopAndSend short-circuits route.
4. `h1_stop_and_send_at_encode_substitutes_wire_response` — encode StopAndSend replaces response.
5. `h1_access_log_reflects_post_encode_headers` — access log dispatches after encode_headers.

**Group D — 4 H2 HCM filter-chain integration tests**

Added to `crates/envoy-http2/src/hcm.rs` `#[cfg(test)] mod tests`:
- Added `AppendAction` to the test module's `use envoy_config::{...}` import.

Two new helpers:
- `synth_h2_hcm_config_with_header_mutation(request_mutations, response_mutations, ...)` — builds
  a full `Http1HCMConfig` via `from_config` with `[HeaderMutation, Router]` http_filters.
- `synth_h2_hcm_config_with_pipeline(pipeline)` — inlines a struct literal (Http1HCMConfig does
  not derive Clone) with the caller's pipeline over a `direct_response 200 "route\n"` route.
  Uses `use envoy_http1::{HCMConfig, HCMStats}` for the struct and stats construction.

4 new tests (all GREEN on first run):
1. `h2_decode_headers_fires_before_route_match` — decode mutation reaches request-processing.
2. `h2_encode_headers_fires_before_send_envoy_response` — encode stamp present on wire response.
3. `h2_stop_and_send_at_decode_skips_route_match` — decode StopAndSend 503, body "stopped\n".
4. `h2_stop_and_send_at_encode_substitutes_wire_response` — encode StopAndSend 418, body "teapot\n".

### Tests landed (9 new integration tests)

**H1 (5 tests, `crates/envoy-http1/src/hcm.rs`):**
1. `h1_decode_headers_fires_before_route_match`
2. `h1_encode_headers_fires_after_writer_arm_before_wire_write`
3. `h1_stop_and_send_at_decode_skips_route_match`
4. `h1_stop_and_send_at_encode_substitutes_wire_response`
5. `h1_access_log_reflects_post_encode_headers`

**H2 (4 tests, `crates/envoy-http2/src/hcm.rs`):**
6. `h2_decode_headers_fires_before_route_match`
7. `h2_encode_headers_fires_before_send_envoy_response`
8. `h2_stop_and_send_at_decode_skips_route_match`
9. `h2_stop_and_send_at_encode_substitutes_wire_response`

Tests 1, 2, 6, 7 use the real `HeaderMutationFilter`. Tests 3, 4, 5, 8, 9 use the `test-util`
StopAndSend stubs. Helpers added: 4 H1 helpers (~90 LoC) + 2 H2 helpers (~70 LoC).

Pre-existing tests confirmed GREEN after all changes:
- `envoy-http2`: 38 tests (17 hcm + 21 other; 1 `#[ignore]`d) — all GREEN.
- `envoy-http1`: 39 hcm tests (34 pre-existing + 5 new) — all GREEN.

### LoC delta

```
crates/envoy-filter/Cargo.toml       +9  lines
crates/envoy-filter/src/instance.rs  +45 lines, -4 lines (net +41)
crates/envoy-filter/src/pipeline.rs  +8  lines
crates/envoy-http1/Cargo.toml        +1  line
crates/envoy-http1/src/hcm.rs        +308 lines (net; all new helpers + tests)
crates/envoy-http2/Cargo.toml        +1  line
crates/envoy-http2/src/hcm.rs        +325 lines, -53 lines (net +272; I1 cleanup ~-22 lines, helpers + tests ~+294 lines)
Total: +644 insertions, -53 deletions (net ~591 LoC added)
```

PLAN budget for Task 5: ~300 LoC. Actual ~591 LoC net — overage concentrated in the helpers
(~160 LoC for 6 helpers) and the Group D struct-literal helper (`synth_h2_hcm_config_with_pipeline`
inlines the full `HCMConfig` struct since `Http1HCMConfig` doesn't derive `Clone`) expanding the
budget; the PLAN assumed `.clone()` would suffice. The test bodies are verbatim or `cargo fmt`
reformatted from the PLAN.

### Deviations from PLAN

1. **`RouterTerminus::new()` is `pub(crate)` — not externally accessible.** The PLAN's test
   code used `envoy_filter::HttpFilterInstance::Router(envoy_filter::RouterTerminus::new())`.
   Added a `test_router()` constructor to the `#[cfg(feature = "test-util")]` impl block in
   `instance.rs`. All test sites use `envoy_filter::HttpFilterInstance::test_router()` instead.

2. **`HCMConfig` / `Http1HCMConfig` do not derive `Clone`.** The PLAN's
   `hcm_config_with_pipeline` and `synth_h2_hcm_config_with_pipeline` were written assuming
   `.clone()` would work. Both helpers inline full struct literals per the PLAN's own fallback
   instruction ("if `HCMConfig` does not derive `Clone`, inline the full struct literal").

3. **`cargo fmt` reformatted all PLAN-verbatim assert statements.** Long `assert!(...)` and
   `assert_eq!(...)` calls were expanded to multi-line form by `rustfmt`. No semantic changes.

4. **`synth_h2_hcm_config_with_pipeline` uses `use envoy_http1::{HCMConfig, HCMStats}` inline.**
   The H2 test module imports `Http1HCMConfig` as an alias from `envoy_http1::HCMConfig`; the
   helper needed to construct `HCMStats` directly. An inline `use` statement resolves the alias
   ambiguity cleanly.

5. **The `AppendAction_Route` import in the H2 test module was already present.** Only `AppendAction`
   was missing from the imports; added it to the existing `use envoy_config::{...}` block.

### Test-bucket attestation

All 5 workspace gate commands run and clean:

**`cargo build --workspace --all-targets`**
```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 16.13s
```

**`cargo clippy --workspace --all-targets --all-features -- -D warnings`**
```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.78s
```

**`cargo fmt --all -- --check`**
```
(no output — clean)
```

**`cargo test --workspace`**
All test suites passed (0 failed). Notable counts: `envoy-http1` 68 tests; `envoy-http2` 42 tests
(1 `#[ignore]`d); `envoy-filter` 32 tests; `envoy-config` 20 tests. No flakes observed on this run.

**`cargo deny check`**
```
warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:49:6
   │
49 │     "0BSD",
   │      ━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:40:6
   │
40 │     "BSD-2-Clause",
   │      ━━━━━━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:47:6
   │
47 │     "MPL-2.0",
   │      ━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:43:6
   │
43 │     "Unicode-DFS-2016",
   │      ━━━━━━━━━━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:45:6
   │
45 │     "Zlib",
   │      ━━━━ unmatched license allowance

advisories ok, bans ok, licenses ok, sources ok
```

No new top-level Cargo dependencies (3 `Cargo.toml` files touched: `envoy-filter` gains
`[features] test-util = []`; `envoy-http1` + `envoy-http2` each gain one `[dev-dependencies]`
feature line for `envoy-filter`). The `license-not-encountered` warnings are pre-existing
(unmatched allowlist entries in `deny.toml` — identical to Tasks 1-4 attestations).

### Review fixes (post-commit code-quality pass)

Four issues identified by the code quality reviewer; all applied as a single `--amend` to `8dc4d2f`:

1. **[Important] Stale `finalize_h2_stream` comment** (`crates/envoy-http2/src/hcm.rs`, comment
   preceding the `response_status_for_log / 100` match). The comment said
   `response_status_for_log` was a "parameter threaded through `finalize_h2_stream` from each H2
   writer arm" — which was accurate before Group A (I1 cleanup) removed the parameter. Updated to
   accurately describe it as a local derived post-encode from `resp`, mirroring the phrasing of the
   updated comment block at the derivation site above.

2. **[Important] `h2_decode_headers_fires_before_route_match` overclaimed** (`crates/envoy-http2/src/hcm.rs`).
   **Preferred fix taken** (preferred over rename/fallback because the H2 `RouteMatch.headers` field
   is structurally identical to H1 and `HeaderMatcher`/`HeaderMatcherMode` were already in
   `envoy-config`; the extension was small and clean — ~70 LoC total for a new helper +
   strengthened test body). Added `synth_h2_hcm_config_header_mutation_matched_route()` which builds
   a single route with `HeaderMatcher { name: "x-h2-decode", ExactMatch("seen") }`. The decode
   mutation adds `x-h2-decode: seen`; the test now asserts both 200 status AND `"matched\n"` body —
   genuinely discriminating "mutation ran before route-match" from "mutation skipped". Added
   `HeaderMatcher, HeaderMatcherMode` to the test module's `use envoy_config::{...}` import.

3. **[Minor] Inaccurate comment in `h1_decode_headers_fires_before_route_match`**
   (`crates/envoy-http1/src/hcm.rs`). Comment said "The catch-all route returns 404-shaped
   'default\n'" and "NOT match `/bar` by prefix" — both wrong. Rewritten to accurately describe the
   single route that matches prefix `/` AND requires header `x-test-path-override: /bar`; no
   catch-all route exists; the 200 "matched\n" response proves decode ran before route-match.

4. **[Minor] Clarifying comment on intentional route-body difference**
   (`crates/envoy-http2/src/hcm.rs`, `synth_h2_hcm_config_with_pipeline`). Added a one-line
   inline comment on `"route\n"` noting the body intentionally differs from `synth_h2_hcm_config`'s
   `"ok\n"` because tests using this helper assert the stub response body, not the route body.

**Post-fix workspace gate:** build clean, clippy clean, fmt clean, deny clean.
`cargo test -p envoy-http2 hcm`: 17 passed, 1 ignored.
`cargo test -p envoy-http1 hcm`: 39 passed, 0 ignored.
`cargo test --workspace`: 75+ passed across all crates (2 known-flaky `tcp_proxy_backend_*`
port-readiness failures on first run; 0 failures on second run).

---

## Task 6 — Fuzz corpus seed for HeaderMutation HCM

**State-3 commit.** Adds the minimal positive-case HCM fuzz corpus seed exercising the
`HeaderMutation` schema arm + Task 2 validator. Three file changes: the seed YAML created,
a `.gitignore` allow-list entry added, and the seed name appended to the
`fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS array in `bootstrap.rs`. TDD order
was followed: test array extended first (RED — file-not-found panic), seed file created,
`.gitignore` entry added, test turned GREEN.

### Work summary

- **`crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_header_mutation_filter.yaml`** (new
  file): a 47-line minimal positive-case HCM bootstrap with a `[HeaderMutation, Router]`
  http_filters chain. Exercises both `request_mutations` (stamp `x-filter-stamp: phase-07`
  with `APPEND_IF_EXISTS_OR_ADD`) and `response_mutations` (stamp `x-filter-response-stamp:
  phase-07` with `APPEND_IF_EXISTS_OR_ADD`). Route is `direct_response { status: 200, body:
  { inline_string: "ok\n" } }` — no upstream cluster required.

- **`crates/envoy-config/fuzz/.gitignore`**: added `!corpus/parse_bootstrap/hcm_header_mutation_filter.yaml`
  after the existing `!corpus/parse_bootstrap/hcm_access_log_file.yaml` allow-list entry.

- **`crates/envoy-config/src/bootstrap.rs`**: appended
  `"fuzz/corpus/parse_bootstrap/hcm_header_mutation_filter.yaml"` to the SUCCESS array
  (seeds expected to parse + validate successfully) in `fuzz_corpus_seeds_parse_or_reject_cleanly`,
  after `"fuzz/corpus/parse_bootstrap/hcm_access_log_file.yaml"`.

### TDD step trace

1. **Step 1 (write failing test):** Appended `"fuzz/corpus/parse_bootstrap/hcm_header_mutation_filter.yaml"`
   to the SUCCESS array in `bootstrap.rs`. Test array at line 2686.
2. **Step 2 (RED confirmed):** `cargo test -p envoy-config fuzz_corpus_seeds_parse_or_reject_cleanly`
   — FAILED: `panic: read .../hcm_header_mutation_filter.yaml: No such file or directory (os error 2)`.
3. **Step 3 (seed file created):** Created the seed YAML (see deviation note below re: schema
   corrections vs. PLAN). Two correction iterations required before the schema validated cleanly.
4. **Step 4 (`.gitignore` entry added):** Added the allow-list entry after `hcm_access_log_file.yaml`.
5. **Step 5 (GREEN confirmed):** `cargo test -p envoy-config fuzz_corpus_seeds_parse_or_reject_cleanly`
   — 1 passed, 0 failed.
6. **Step 6 (`git check-ignore`):** `git check-ignore crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_header_mutation_filter.yaml; echo "exit: $?"` → `exit: 1` (NOT ignored — allow-list entry working correctly).
7. **Step 7 (fuzz smoke — RAN):** Nightly toolchain available (`nightly-aarch64-apple-darwin`).
   `cd crates/envoy-config/fuzz && cargo +nightly fuzz run parse_bootstrap -- -max_total_time=15`:
   ran 221,825 iterations in 16 seconds; `#221825 DONE cov: 11844 ft: 32067 corp: 3208/1690Kb`
   — no crash, no panic. Coverage increased by 1 (`cov: 11843 → 11844`) after the seed discovery,
   confirming the new seed exercises a new code path.
8. **Step 8 (workspace gate):** All 5 commands clean (see attestation below).

### LoC delta

```
crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_header_mutation_filter.yaml  +47 lines (new file)
crates/envoy-config/fuzz/.gitignore                                                 +1 line
crates/envoy-config/src/bootstrap.rs                                                +1 line
Total: +49 insertions, 0 deletions (net ~49 LoC added)
```

PLAN budget for Task 6: ~52 LoC. Actual ~49 LoC — within acceptable range.

### Deviations from PLAN

1. **PLAN's seed YAML used `direct_response: { status: 200 }` without the required `body`
   field; also omitted the required `codec_type` field.** The PLAN's Step 3 YAML was written
   against Envoy's upstream schema (where `body` is optional and `codec_type` has a default),
   but the envoy-rust schema has both as required fields (no `#[serde(default)]` on either).
   This was discovered through two RED→GREEN correction iterations:
   - First correction: added `body: { inline_string: "ok\n" }` to the `direct_response`
     block (required by `DirectResponse.body: DataSource` with no default).
   - Second correction: added `codec_type: HTTP1` to the HCM typed_config block (required
     by `HttpConnectionManagerConfig.codec_type: CodecType` with no default).
   Both corrections match the existing corpus seeds (`hcm_direct_response_happy.yaml`,
   `hcm_access_log_file.yaml`). The seed's semantic intent — minimal positive case exercising
   the new `HeaderMutation` schema arm — is fully preserved; only schema-required fields were
   added. No PLAN-level design decision is affected.

### Test-bucket attestation

All 5 workspace gate commands run and clean:

**`cargo build --workspace --all-targets`**
```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 21.39s
```

**`cargo clippy --workspace --all-targets --all-features -- -D warnings`**
```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 20.96s
```

**`cargo fmt --all -- --check`**
```
(no output — clean)
```

**`cargo test --workspace`**
All test suites passed (0 failed). Test counts: `envoy-config` 207 tests; `envoy-filter`
32 tests; `envoy-http1` 68 tests; `envoy-http2` 42 tests (1 `#[ignore]`d); `differential`
crate 77 tests (1 `#[ignore]`d). No flakes observed on this run. The new
`fuzz_corpus_seeds_parse_or_reject_cleanly` test walker now covers 13 SUCCESS seeds
(previously 12) + 3 REJECT seeds + the `minimal.yaml` baseline.

**`cargo deny check`**
```
warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:49:6
   │
49 │     "0BSD",
   │      ━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:40:6
   │
40 │     "BSD-2-Clause",
   │      ━━━━━━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:47:6
   │
47 │     "MPL-2.0",
   │      ━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:43:6
   │
43 │     "Unicode-DFS-2016",
   │      ━━━━━━━━━━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:45:6
   │
45 │     "Zlib",
   │      ━━━━ unmatched license allowance

advisories ok, bans ok, licenses ok, sources ok
```

No new top-level Cargo dependencies. The `license-not-encountered` warnings are pre-existing
(unmatched allowlist entries in `deny.toml` — identical to Tasks 1-5 attestations). No
`Cargo.toml` files were modified by Task 6.

---

## Task 7 — `http1-echo-server` helper header-echo verify

**Verify-only; zero code change.** PLAN signpost 10 established Task 7 as a pre-state check:
verify that `build_echo_body` in `tests/helpers/http1-echo-server/src/main.rs` already echoes
request headers sorted-by-lowercase-name into the response body per 07.2 SPEC §6 signpost 10.
No own commit per the PLAN's Task 7 disposition — this note folds into Task 8's commit.

### Verification result

Inspected `tests/helpers/http1-echo-server/src/main.rs` `build_echo_body` (lines 210-244):

```rust
let mut sorted_headers: Vec<(String, String)> = req
    .headers
    .iter()
    .map(|(n, v)| (n.to_ascii_lowercase(), v.clone()))
    .collect();
sorted_headers.sort_by(|a, b| a.0.cmp(&b.0));
for (n, v) in &sorted_headers {
    out.push_str("  ");
    out.push_str(n);
    out.push_str(": ");
    out.push_str(v);
    out.push('\n');
}
```

The function lowercases header names then sorts alphabetically — exactly the shape SPEC §6
signpost 10 specifies. **Zero code change required.**

The helper's 5 existing tests pass (including `accepts_and_echoes_request` which asserts the
sorted-header body shape directly with `expected_body =
"method: GET\npath: /\nheaders:\n  content-length: 0\n  host: x.test\nbody: \n"`).

### LoC delta

```
(zero — verify-only task)
```

No commit. Task 7's PROGRESS note folds into Task 8's commit per the PLAN's Task 7 disposition.

---

## Task 8 — Fixture `0013-http-filter-header-mutation` + Docker-gated wrapper

**State-3 commit.** Creates the differential fixture `tests/fixtures/0013-http-filter-header-mutation/`
(5 files: `envoy.yaml`, `envoy-rust.yaml`, `inputs/payload.bin` (0-byte), `expectations.yaml`,
`README.md`) and the Docker-gated wrapper `tests/differential/tests/http_filter_header_mutation.rs`.
The fixture drives a GET / through an HCM with `http_filters: [HeaderMutation, Router]` proxying to
a host-side `http1-echo-server` backend; bilaterally asserts the decode-side stamp
(`x-filter-stamp: phase-07`, echoed into the body by the backend) and the encode-side stamp
(`x-filter-response-stamp: phase-07`, on the response headers). Also folds in Task 7's verify-only
PROGRESS note.

### Work summary

**`tests/fixtures/0013-http-filter-header-mutation/envoy.yaml`** (new file): reference Envoy config.
Mirrors fixture 0008's `envoy.yaml` shape + the HeaderMutation filter. Node id
`envoy-rust-phase-07.2-fixture-0013`. Includes `generate_request_id: false` and
`request_headers_to_remove` (the same 6 headers as fixture 0008) to strip Envoy-injected request
headers. `http_filters: [HeaderMutation (request+response stamps), Router]`. STRICT_DNS cluster
with `dns_lookup_family: V4_ONLY`.

**`tests/fixtures/0013-http-filter-header-mutation/envoy-rust.yaml`** (new file): envoy-rust config.
Mirrors fixture 0008's `envoy-rust.yaml` shape — no `request_headers_to_remove`, no
`generate_request_id`, no `admin` block, binds `127.0.0.1`. Same `http_filters` chain. STRICT_DNS
cluster without `dns_lookup_family`.

**`tests/fixtures/0013-http-filter-header-mutation/inputs/payload.bin`** (new file, 0 bytes): GET
request carries no body; the 0-byte file satisfies the harness's `inputs/` convention.

**`tests/fixtures/0013-http-filter-header-mutation/expectations.yaml`** (new file): mirrors fixture
0008's actual shape exactly (PLAN-write SPEC correction 4). Driver `kind: http1`, method `get`,
path `/`, host `envoy-rust.test`, `expected_status: 200`, `expected_body: { kind: byte_exact, body:
"..." }`, `expected_headers: set_equal_modulo_allow_list`. Top-level `equivalence: { response_status:
exact, response_body: { kind: byte_exact } }`.

**`tests/fixtures/0013-http-filter-header-mutation/README.md`** (new file): fixture documentation
per PLAN Step 5 verbatim template.

**`tests/differential/tests/http_filter_header_mutation.rs`** (new file): Docker-gated wrapper.
Mirrors `http1_router_upstream.rs` shape exactly. No `#[ignore]` (Docker-gating is handled by the
harness's `run_fixture`). `#[tokio::test]`.

### TDD step trace

1. **Steps 1-6 (fixture + wrapper files created):** All 6 files written per PLAN's Step 1-6.
2. **Step 7 (Docker-gated local run):**

```
cargo test -p differential --test http_filter_header_mutation -- --nocapture 2>&1 | tail -60
```

Output:
```
   Compiling differential v0.0.0 (/Users/esa/git/envoy-rust/tests/differential)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1.36s
     Running tests/http_filter_header_mutation.rs (target/debug/deps/http_filter_header_mutation-a98ebdb573f58d70)

running 1 test
[INFO] node registered node.id=envoy-rust-phase-07.2-fixture-0013 node.cluster=envoy-rust-phase-07.2
[INFO] envoy-rust listening (http_connection_manager) addr=127.0.0.1:58522 stat_prefix=ingress_http1 codec_type=HTTP1
test http_filter_header_mutation_fixture ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.64s
```

**GREEN on first run — 1 passed, 0 failed.** Both the per-proxy `expected_body.body` assertion
AND the cross-proxy `equivalence.response_body` byte_exact check passed on the first attempt.

**`expected_body.body` was NOT corrected from the prediction.** The predicted string
`"method: GET\npath: /\nheaders:\n  host: envoy-rust.test\n  x-filter-stamp: phase-07\nbody: \n"`
matched the actual harness output exactly. Derivation confirmed: `drive_http1` sends only
`Host: envoy-rust.test` and `Connection: close` (no `content-length`); the HeaderMutation filter
appends `x-filter-stamp: phase-07`; the backend receives `{host, x-filter-stamp}` and echoes them
sorted alphabetically (`h` < `x`).

3. **Step 8 (workspace gate):** All 5 commands clean (see attestation below).

### LoC delta

```
tests/fixtures/0013-http-filter-header-mutation/envoy.yaml         +62 lines (new file)
tests/fixtures/0013-http-filter-header-mutation/envoy-rust.yaml    +47 lines (new file)
tests/fixtures/0013-http-filter-header-mutation/inputs/payload.bin   0 lines (0-byte new file)
tests/fixtures/0013-http-filter-header-mutation/expectations.yaml  +14 lines (new file)
tests/fixtures/0013-http-filter-header-mutation/README.md          +51 lines (new file)
tests/differential/tests/http_filter_header_mutation.rs            +20 lines (new file)
docs/envoy-rust/phases/07.2-header-mutation-filter/PROGRESS.md    +~130 lines (Task 7+8 notes)
Total: ~324 insertions, 0 deletions
```

PLAN budget for Task 8: ~290 LoC. Actual ~324 LoC — the ~34-line overage is the PROGRESS note
(Task 7 verify-only note folded in per PLAN disposition + the Task 8 Docker-run attestation).

### Deviations from PLAN

None. The PLAN's predicted `expected_body.body` was correct on the first Docker run — no
empirical correction pass was needed. Fixture files mirror the PLAN's verbatim content. Wrapper
mirrors `http1_router_upstream.rs` exactly.

### Test-bucket attestation

All 5 workspace gate commands run and clean:

**`cargo build --workspace --all-targets`**
```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.28s
```

**`cargo clippy --workspace --all-targets --all-features -- -D warnings`**
```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.39s
```

**`cargo fmt --all -- --check`**
```
(no output — clean)
```

**`cargo test --workspace`**
All test suites passed (0 failed). Key crate counts: `envoy-config` 207 tests; `envoy-filter`
32 tests; `envoy-http1` 68 tests; `envoy-http2` 42 tests (1 ignored); `differential` crate
77+ tests (1 ignored) — the new `http_filter_header_mutation_fixture` test is included in
the `differential` suite run and passes. No flakes observed.

**`cargo deny check`**
```
warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:49:6
   │
49 │     "0BSD",
   │      ━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:40:6
   │
40 │     "BSD-2-Clause",
   │      ━━━━━━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:47:6
   │
47 │     "MPL-2.0",
   │      ━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:43:6
   │
43 │     "Unicode-DFS-2016",
   │      ━━━━━━━━━━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:45:6
   │
45 │     "Zlib",
   │      ━━━━ unmatched license allowance

advisories ok, bans ok, licenses ok, sources ok
```

No new top-level Cargo dependencies. The `license-not-encountered` warnings are pre-existing
(unmatched allowlist entries in `deny.toml` — identical to Tasks 1-6 attestations). No
`Cargo.toml` files were modified by Task 8.

---

## Task 9 — In-process backstop `crates/envoy-bin/tests/http_filter_header_mutation.rs`

**State-3 commit.** Creates the no-Docker in-process integration backstop for the Docker-gated
fixture `0013-http-filter-header-mutation`. Spawns an in-process HTTP/1.1 echo upstream (echoes
received request headers into the response body as sorted `name: value\n` lines), spawns
`envoy-bin` as a subprocess against a `format!`-YAML HCM config with `http_filters:
[HeaderMutation, Router]`, drives a `GET /`, and asserts both the encode-side stamp
(`x-filter-response-stamp: phase-07` on response headers) and the decode-side stamp
(`x-filter-stamp: phase-07` echoed in the response body, proving the mutation reached the
backend). Follows the `crates/envoy-bin/tests/http1_router_upstream.rs` (04.3) precedent per
PLAN-write SPEC correction 8.

### Work summary

- **`crates/envoy-bin/tests/http_filter_header_mutation.rs`** (new file, 283 lines):
  - `reserve_port()` — TOCTOU port reservation (mirrors `http1_router_upstream.rs`).
  - `wait_ready(addr, budget)` — exponential-backoff poll until the listener accepts.
  - `spawn_echo_upstream()` — in-process tokio async upstream that accepts one connection,
    reads until `\r\n\r\n`, parses headers via `httparse`, emits them sorted by lowercase
    name into the body as `headers:\n  name: value\n` lines (differs from `http1_router_upstream.rs`'s
    fixed-`"hello"` response — required so the decode-side stamp is observable in the body).
  - `header_mutation_stamps_request_and_response()` — the single `#[tokio::test(flavor =
    "multi_thread")]` test: spawns upstream, reserves listener port, writes `format!`'d YAML to
    `tempfile::tempdir()`, spawns `envoy-bin` via `tokio::process::Command::new(env!(
    "CARGO_BIN_EXE_envoy-bin"))`, waits for readiness, drives `GET / HTTP/1.1`, reads response,
    asserts (1) `x-filter-response-stamp: phase-07` in response headers and (2) `x-filter-stamp:
    phase-07` substring in response body, then tears down the child process.

**`crates/envoy-bin/Cargo.toml` — NO CHANGE.** `anyhow` and `httparse` are in `[dependencies]`
(available to test code); `tempfile` and `tokio` are in `[dev-dependencies]` and `[dependencies]`
respectively. All required dev-deps were already present — confirmed against disk per Step 2.

### Test landed

1. `header_mutation_stamps_request_and_response` — 1 `#[tokio::test(flavor = "multi_thread")]`
   test. Asserts:
   - **Encode-side stamp:** `x-filter-response-stamp: phase-07` present in response headers
     (the `HeaderMutation::encode_headers` path).
   - **Decode-side stamp:** `x-filter-stamp: phase-07` substring present in response body
     (echoed by the in-process upstream, proving the mutation reached the backend via
     `HeaderMutation::decode_headers`).

### LoC delta

```
crates/envoy-bin/tests/http_filter_header_mutation.rs  +283 lines (new file)
docs/envoy-rust/phases/07.2-header-mutation-filter/PROGRESS.md  +~60 lines (this note)
Total: ~343 insertions, 0 deletions
```

PLAN budget for Task 9: ~150 LoC. Actual 283 lines (file length) — the file includes the full
module doc comment, the 3 helper functions, and the test function. Net additions are 283 new lines
(previously non-existent file). The overage vs. the ~150 LoC budget is primarily the verbose
assertions with diagnostic messages and the response-reading loop (~75 lines), both of which follow
the `http1_router_upstream.rs` precedent's patterns verbatim.

### Deviations from PLAN

1. **`cargo fmt` expanded `eprintln!` to multi-line form.** The PLAN's verbatim file content
   has `eprintln!("envoy-bin stderr:\n{}", String::from_utf8_lossy(&stderr_buf));` on a single line.
   `cargo fmt` expanded it to 4-line form (the argument list is too long for the column limit).
   Applied as a pre-commit fmt fix. No semantic change.

### Test-bucket attestation

**`cargo test -p envoy-bin --test http_filter_header_mutation`**
```
running 1 test
test header_mutation_stamps_request_and_response ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.77s
```

Both assertions confirmed GREEN:
- Encode-side stamp: `x-filter-response-stamp: phase-07` present in response headers — PASS.
- Decode-side stamp: `x-filter-stamp: phase-07` echoed in response body — PASS.

All 5 workspace gate commands run and clean:

**`cargo build --workspace --all-targets`**
```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.62s
```

**`cargo clippy --workspace --all-targets --all-features -- -D warnings`**
```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.94s
```

**`cargo fmt --all -- --check`**
```
(no output — clean)
```

**`cargo test --workspace`**
All test suites passed (0 failed). 601 tests total across all crates (all `test result: ok`,
0 FAILED). No flakes observed on this run. The new `header_mutation_stamps_request_and_response`
test is the 1-test suite in `crates/envoy-bin`'s `http_filter_header_mutation` integration target.

**`cargo deny check`**
```
warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:49:6
   │
49 │     "0BSD",
   │      ━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:40:6
   │
40 │     "BSD-2-Clause",
   │      ━━━━━━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:47:6
   │
47 │     "MPL-2.0",
   │      ━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:43:6
   │
43 │     "Unicode-DFS-2016",
   │      ━━━━━━━━━━━━━━━━ unmatched license allowance

warning[license-not-encountered]: license was not encountered
   ┌─ /Users/esa/git/envoy-rust/deny.toml:45:6
   │
45 │     "Zlib",
   │      ━━━━ unmatched license allowance

advisories ok, bans ok, licenses ok, sources ok
```

No new top-level Cargo dependencies (no `Cargo.toml` files modified by Task 9 — all required
dev-deps were already present). The `license-not-encountered` warnings are pre-existing
(unmatched allowlist entries in `deny.toml` — identical to Tasks 1-8 attestations).

---

## Task 10 — state-4 phase-done gate evidence (13 fixtures simultaneously green)

**Docs-only commit.** Materializes the §7.5 phase-done gate evidence at HEAD `20a393d`
(the Task 9 commit). All gates GREEN per CI run `25887571566`
(`https://github.com/pgdad/envoy-rust/actions/runs/25887571566`,
conclusion `success`, completed `2026-05-14T21:49:18Z`). Both CI jobs green:

- **`build + test + lint`** ✅ — ran `cargo fmt --all -- --check`, `cargo clippy --workspace
  --all-targets --all-features -- -D warnings`, `cargo build --workspace --all-targets`,
  `cargo test --workspace`, and `cargo deny check` — all clean.
- **`fuzz (parse_bootstrap, 30s)`** ✅ — `cargo +nightly fuzz run parse_bootstrap --
  -max_total_time=30` ran clean (no crash; ~110k+ iterations; the Task 6
  `hcm_header_mutation_filter.yaml` seed is in the corpus and was exercised).

### Phase-done gate summary

- **workspace tests:** `cargo test --workspace` — PASS (601 tests passed across 57 test binaries,
  0 failed; HEAD `20a393d`)
- **Docker-gated fixtures (13 total, 0001-0013):** all green simultaneously per CI run
  `25887571566` (`https://github.com/pgdad/envoy-rust/actions/runs/25887571566`,
  conclusion `success`, completed `2026-05-14T21:49:18Z`, HEAD `20a393d`).
  All 13 fixtures passed as `... ok`:
  - `echo_fixture` (0001)
  - `tcp_proxy_fixture` (0002)
  - `tls_downstream_fixture` (0003)
  - `tls_upstream_fixture` (0004)
  - `tls_sni_fixture` (0005)
  - `http1_direct_response_fixture` (0006)
  - `admin_ready_fixture` (0007)
  - `http1_router_upstream_fixture` (0008)
  - `http2_direct_response_fixture` (0009)
  - `http2_router_upstream` (0010)
  - `admin_stats_prometheus` (0011)
  - `access_log_file_sink` (0012)
  - `http_filter_header_mutation_fixture` (0013)
- **h2spec conformance:** 99.31% (≥95% gate held; `h2spec_pass_rate_gate` PASS; 05.2 baseline
  99.31% carried forward unchanged; `known-failures.txt` unchanged — 07.2 engages no H2-framing
  surfaces)
- **`parse_bootstrap` fuzz:** clean (short-budget CI run, `-max_total_time=30`; the Task 6
  `hcm_header_mutation_filter.yaml` seed exercised; no crash)
- **`cargo clippy --workspace --all-targets --all-features -- -D warnings`:** clean
- **`cargo fmt --all -- --check`:** clean
- **`cargo deny check`:** clean
- **`cargo build --workspace --all-targets`:** clean

All §7.5 phase-done gates GREEN. STATE.md advances to `07.2 state-4-reached / state-5-next`;
the next session is the 07.2 state-5 REVIEW.md session invoking
`superpowers:requesting-code-review`.
