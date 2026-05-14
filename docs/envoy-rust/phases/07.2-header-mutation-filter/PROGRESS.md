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
