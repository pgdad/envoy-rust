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
