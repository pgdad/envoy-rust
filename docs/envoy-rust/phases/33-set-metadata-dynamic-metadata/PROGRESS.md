# Phase 33 — Implementation Progress

## Task 1 — M32 command-operator folds

Folded the phase-32 carry-forwards M32-1, M32-2, M32-3, M32-6 into
`crates/envoy-accesslog/src/command_operator.rs`. Pure refactor + one new strict
rejection (empty alternate); no behavior change to any pre-existing valid format.

### What changed

- **M32-1 (`enum Side`):** added `#[derive(Debug, Clone, Copy, PartialEq)] pub enum Side { Req, Resp }`
  with `as_str(self) -> &'static str` (`"REQ"`/`"RESP"`) and a `Display` impl. Threaded `Side`
  through `parse_header_op(keyword, rest, side: Side)`; the two call sites pass `Side::Req`/`Side::Resp`.
  The internal `allow_list`/`Op` matches now match on `Side` instead of `&str`.
- **M32-3 (named diagnostics):** `MalformedArgument(String, String)` → `MalformedArgument { keyword: String, detail: String }`;
  `UnsupportedHeader.side` changed from `&'static str` to `Side`. Updated the `#[error(...)]` format
  strings (`{keyword}`/`{detail}`; `{side}` now renders via the `Display` impl). Updated every
  `MalformedArgument(a, b)` construction to the named form.
- **M32-2 (empty-alternate strictness):** in `parse_header_op`, after `arg.split_once('?')`, an empty
  alternate (`Some((_, a)) if a.is_empty()`) now returns
  `MalformedArgument { keyword, detail: "empty alternate after '?'" }`. `:0` truncation stays VALID
  (already totals via `floor_char_boundary`).
- **M32-6 (pre-alloc):** `render`'s `String::with_capacity(256)` replaced with a data-driven
  `String::with_capacity(literal_len + 64)`, where `literal_len` is the sum of `Segment::Literal` byte
  lengths.

### M32-6 option chosen

**Tuple-on-the-fly.** `CompiledFormat(pub(crate) Vec<Segment>)` is constructed directly as
`CompiledFormat(f)` by the in-crate tests, so the tuple shape was kept and `literal_len` is summed at
the top of `render` rather than precomputed on a named field. This is the least-churn option that keeps
all tests green (the plan explicitly permits it as an acceptable fallback).

### Tests added (TDD)

- `empty_alternate_is_error`: `parse_format("%REQ(:PATH?)%")` → `Err(MalformedArgument { .. })`.
  (Failed before the fix — parsed to `alt: Some("")`.)
- `truncate_zero_is_valid_and_empty`: `parse_format("%REQ(USER-AGENT):0%")` parses OK and renders `""`
  against a record with a present `user_agent`. (Already green pre-fix — pins `:0` semantics explicitly.)

### Verification

- `cargo test -p envoy-accesslog` → `test result: ok. 40 passed; 0 failed; 0 ignored` (the 2 new + every
  pre-existing command_operator/default_format/record/file_sink test). Doc-tests: 0 passed.
- `cargo build -p envoy-config` → clean (the only out-of-crate `FormatParseError` consumer,
  `validate_access_logs`, uses `parse_format(...).map_err(|e| e.to_string())` — unaffected by the field
  rename).

### Deviations from the plan

None. M32-6 took the plan-sanctioned tuple-on-the-fly fallback rather than a precomputed named field, to
keep the in-crate `CompiledFormat(f)` test constructors unchanged.

---

## Task 2 — FilterRequest.dynamic_metadata field + sweep

### Field added

- `crates/envoy-filter/src/types.rs`: added the additive, default-empty field to `FilterRequest`:
  ```rust
  pub dynamic_metadata: std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
  ```
  Per-request dynamic-metadata store (namespace → key → string value). String-only (non-string Value enum is
  the §2.2 deferral); plain `BTreeMap`, NO new crate, NO shared Value type. `PartialEq, Eq` derives still hold
  (the new `BTreeMap` field supports both). No logic reads it yet — behavior is unchanged.

### Construction-site sweep (compiler-driven)

- Updated **22** `FilterRequest { … }` literal construction sites by adding
  `dynamic_metadata: std::collections::BTreeMap::new()` (the types.rs test site uses the imported `BTreeMap::new()`):
  - 2 production: `crates/envoy-http1/src/hcm.rs` (775), `crates/envoy-http2/src/hcm.rs` (475).
  - 20 test sites across `crates/envoy-filter/src/{types(1), pipeline(3), router(1), rbac(1), cdn_loop(1),
    header_mutation(1), jwt_authn(1), csrf(1), instance(6), local_rate_limit(1), fault(1), cors(1), buffer(1)}.rs`.
- **Count note:** the PLAN's "33 sites" figure counted all lines containing the `FilterRequest {` token,
  which includes 11 non-literal `fn … -> FilterRequest {` return-type/signature lines that need no field. The
  actual literal-construction-site count is **22**. The compiler is authoritative: `cargo build --all-targets`
  is clean for all three crates, proving every literal that needs the field has it.

### Verification

- `cargo test -p envoy-filter` → `test result: ok. 171 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`
  (includes the new `types::tests::filter_request_dynamic_metadata_defaults_empty_and_is_writable ... ok`).
- `cargo build -p envoy-http1 -p envoy-http2 --all-targets` → clean (`Finished dev profile`).

### Deviations from the plan

None on substance. The field and the mechanical sweep are exactly as specified; only the site count differs
(22 actual literals vs. the PLAN's 33 token-occurrence estimate, as noted above).

## Task 3 — AccessLogRecord.dynamic_metadata field + sweep

Added the additive, default-empty field to `AccessLogRecord` (`crates/envoy-accesslog/src/record.rs`):

```rust
pub dynamic_metadata: std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
```

Per-request dynamic metadata (namespace → key → string value), copied from `FilterRequest.dynamic_metadata`
at the HCM record-build site. Rendered by the future `%DYNAMIC_METADATA(namespace:key)%` operator. The struct
still has NO `Default` impl — every construction site sets the field explicitly.

**TDD:** new test `record_dynamic_metadata_defaults_empty_and_carries_values` in `record.rs` — constructs an
empty-metadata record (asserts `dynamic_metadata.is_empty()`) and a populated one
(`{"envoy.test": {"tier": "prod"}}`, asserts `record.dynamic_metadata["envoy.test"]["tier"] == "prod"`).
Verified RED (no field `dynamic_metadata`, E0560/E0609) before adding the field.

**Construction-site sweep (compiler-driven) — 8 literal sites updated:**
- Production (3): `crates/envoy-http1/src/hcm.rs` (H1 record build ~1190 + the `make_test_record` literal ~1752),
  `crates/envoy-http2/src/hcm.rs` (H2 record build ~889). Each set to `std::collections::BTreeMap::new()`
  (default-empty; no operator reads it yet — wiring of `filter_req.dynamic_metadata` is Task 8).
- In-crate (5): `command_operator.rs` `rec()`, `default_format.rs` `make_baseline_record()`, `file_sink.rs`
  `make_record()`, and the two pre-existing `record.rs` test literals (`record_construction_full`,
  `record_clone_is_deep_for_strings`).

(The PLAN's "~13 literal sites" was a token-occurrence estimate; the compiler worklist found 8 true literals
needing the field — the rest were `fn … -> AccessLogRecord {` signature lines.)

**Verification:**
- `cargo test -p envoy-accesslog` → `test result: ok. 41 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`
- `cargo build -p envoy-http1 -p envoy-http2 --all-targets` → clean (no errors/warnings)
- `cargo fmt -p envoy-accesslog -p envoy-http1 -p envoy-http2 --check` → clean
- Behavior unchanged (default-empty); fixture 0012 stays byte-identical (no operator reads the field).

---

## Tasks 4+5 — SetMetadataConfig + variant + validator + ConfigError

**Task 4 — config schema + enum variant (`crates/envoy-config/src/bootstrap.rs`, `…/src/lib.rs`):**
- Added `SetMetadataConfig { metadata: Vec<MetadataEntry> }` and
  `MetadataEntry { metadata_namespace: String, value: BTreeMap<String, String>, allow_overwrite: bool }`
  near `CdnLoopConfig`. Both `#[serde(deny_unknown_fields)]`; `value` is string→string (a non-string YAML
  scalar fails serde deserialization — the §A1/§A5 string-only MVP boundary). `allow_overwrite` is
  `#[serde(default)]` (Envoy default false).
- Appended the `HttpFilterTypedConfig::SetMetadata(SetMetadataConfig)` variant after `CdnLoop`, tagged
  `@type = type.googleapis.com/envoy.extensions.filters.http.set_metadata.v3.Config` (§A1-LOCKED — the
  proto message is `Config`, NOT `SetMetadata`).
- Re-exported `MetadataEntry`, `SetMetadataConfig` from `lib.rs` (alphabetical `pub use bootstrap::{…}` list;
  cargo fmt re-wrapped the list).
- Tests (`bootstrap::set_metadata_config_tests`): `parses_set_metadata_filter_modern_form` (modern repeated
  form → variant + field asserts) and `set_metadata_non_string_value_is_rejected` (`value: { tier: 7 }` → `Err`).

**Task 5 — validator + ConfigError (`…/src/lib.rs`, `…/src/bootstrap.rs`):**
- Added `ConfigError::SetMetadataEmptyNamespace { listener: String }` (§A5-LOCKED, boot-fatal,
  ADR-0049 all-fatal) after the cdn_loop variants.
- Added the `validate_http_filters` `SetMetadata` arm (name-mismatch → `UnsupportedHttpFilter`, else
  `validate_set_metadata_config`) and the `validate_set_metadata_config` helper (empty
  `metadata_namespace` → `SetMetadataEmptyNamespace`), modeled on the `CdnLoop` arm / `validate_cdn_loop_config`.
- Tests (same module): `set_metadata_empty_namespace_is_fatal` and `set_metadata_name_mismatch_is_unsupported`,
  both driven through the full `parse_bootstrap(&yaml)` validation entry-point (the same entry-point the
  existing cdn_loop validator tests use).

**Verification:**
- `cargo test -p envoy-config` → `test result: ok. 486 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`
  (the 4 new + every pre-existing).
- `cargo fmt -p envoy-config` → clean.
- NOTE: `envoy-filter` is intentionally RED (`HttpFilterInstance::build` non-exhaustive match on the new
  `SetMetadata` variant) until the instance-wiring task (Task 7). Per the PLAN sequencing rule, the gate for
  Tasks 4+5 is `cargo test -p envoy-config` only — NOT a workspace build.

## Tasks 6+7 — SetMetadataFilter + HttpFilterInstance wiring

**Task 6 — the filter (`crates/envoy-filter/src/set_metadata.rs`, new):**
- Added `SetMetadataFilter { metadata: Vec<MetadataEntry> }` with `new(&SetMetadataConfig)`,
  `decode_headers` (merges each entry's `value` into `req.dynamic_metadata[ns]`, honoring
  `allow_overwrite`), and an inert `encode_headers`. Decode-side, **Continue-ONLY** — NEVER
  `StopAndSend` (observability plumbing; §A-LOCKED). Follows the `cdn_loop.rs` add-a-decode-side-filter
  precedent.
- Wired `pub mod set_metadata;` + `pub use set_metadata::SetMetadataFilter;` into `lib.rs` (sibling pattern).
- Tests (`set_metadata::tests`): `writes_value_under_namespace_and_continues`, `multi_namespace_multi_entry`,
  `allow_overwrite_false_keeps_existing`, `allow_overwrite_true_overwrites`, `encode_is_inert`.

**Task 7 — instance wiring (`crates/envoy-filter/src/instance.rs`):**
- Added `use crate::set_metadata::SetMetadataFilter;`, the `SetMetadata(SetMetadataFilter)` enum variant
  (doc comment mirroring `CdnLoop`), the `build` arm (`SetMetadata(cfg) => …SetMetadataFilter::new(cfg)`),
  and the `decode_headers` / `encode_headers` dispatch arms. `apply_route_config` uses the existing
  `_ => {}` fall-through (no per-route config); updated the no-per-route-config comment to list `SetMetadata`.
- Test (`instance::tests`): `builds_set_metadata_instance_and_writes` — builds via `HttpFilterInstance::build`,
  matches `SetMetadata(_)`, decode → `Continue` + `dynamic_metadata["envoy.test"]["tier"]=="prod"`, encode → `Continue`.

**RED WINDOW CLOSED:** the `HttpFilterInstance::build` non-exhaustive match opened by Task 4's
`HttpFilterTypedConfig::SetMetadata` variant is now exhaustive.

**Verification:**
- `cargo test -p envoy-filter` → `test result: ok. 177 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`
  (the new set_metadata + instance tests + every pre-existing).
- `cargo build -p envoy-http1 -p envoy-http2 --all-targets` → clean.
- `cargo build -p envoy-bin` → clean (binary compiles with the new variant).
- `cargo fmt -p envoy-filter --check` → clean. `#![forbid(unsafe_code)]` holds.
- Continue-only honored: no `StopAndSend` anywhere in `set_metadata.rs`.

## Task 8 — `Op::DynamicMetadata` (`crates/envoy-accesslog/src/command_operator.rs`)

- Added the `Op::DynamicMetadata { namespace: String, key: String }` variant — **NO `truncate` field**
  (§A2-LOCKED: a `:N` length suffix is boot-fatal in Envoy, so the operator carries no length).
- `parse_operator`: new `"DYNAMIC_METADATA"` arm → `parse_dynamic_metadata_op(rest)`, mirroring
  `parse_header_op`'s paren/`)`-suffix handling but rejecting `:N` and NOT lowercasing. Parse rejections
  (all `FormatParseError::MalformedArgument { keyword: "DYNAMIC_METADATA", .. }`):
  - no `(...)` argument (no-arg `%DYNAMIC_METADATA%`) → "requires a (namespace:key) argument";
  - any non-empty suffix after `)` (a trailing `:N`) → "does not accept a ':N' length suffix";
  - not exactly two non-empty `:`-separated segments (1-segment whole-namespace `(ns)` or 3+-segment
    nested `(a:b:c)`) → "requires exactly 'namespace:key'".
  namespace/key are CASE-SENSITIVE (stored verbatim, not lowercased — unlike REQ/RESP header names).
- `render_op`: `record.dynamic_metadata.get(namespace).and_then(|m| m.get(key)).map(String::as_str)
  .unwrap_or("-")` — a present scalar string renders RAW, UNQUOTED (§A3, e.g. `prod` not `"prod"`);
  an absent namespace OR an absent key renders the single dash `-` (§A4).

**Tests (`command_operator::tests`):** `parses_dynamic_metadata`, `renders_present_metadata_raw_unquoted`,
`renders_absent_key_and_namespace_dash`, `dynamic_metadata_rejects_truncation`, `dynamic_metadata_requires_arg`,
`dynamic_metadata_rejects_single_and_nested_segments`, `dynamic_metadata_is_case_sensitive`.

**Verification:**
- `cargo test -p envoy-accesslog` → `test result: ok. 48 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`
  (the new operator tests + every pre-existing command_operator/default_format/record/file_sink test).
- `cargo build -p envoy-config` → clean (`validate_access_logs` reuses `parse_format`; the new operator now
  parses through it without error).
- `cargo fmt -p envoy-accesslog` applied. `#![forbid(unsafe_code)]` holds. No `truncate` field on the variant;
  `:N` / no-arg / non-two-segment args are all rejected.

## Tasks 9+10 — H1+H2 dynamic-metadata threading + backstops

**Goal:** thread the per-request dynamic-metadata store from the filter pipeline into the access-log record at the
TWO independent HCM record-build sites, so `%DYNAMIC_METADATA(ns:key)%` renders the `set_metadata`-written value
instead of `-`. Each site needs a symmetric capture-before-drop: both HCMs build a `FilterRequest` (`filter_req`),
run `pipeline.decode_headers`, write back ONLY `method`/`path`/`headers`/`body` to the codec-native request, then
drop the rest of `filter_req` — so `filter_req.dynamic_metadata` would be LOST without an explicit capture.

**Dual capture-before-drop sites:**
- **H1 (`crates/envoy-http1/src/hcm.rs`):** right after the 4-field write-back (`req.body = filter_req.body;`) and
  BEFORE the `match decode_decision`, `let dynamic_metadata = filter_req.dynamic_metadata;` (a full move of the
  remaining field — `filter_req` is already partially moved by the four write-backs, so this compiles cleanly).
  The record build inside `if !config.access_log.is_empty()` sets `dynamic_metadata: dynamic_metadata.clone()`
  (clone because the local is captured unconditionally but consumed inside the `if`).
- **H2 (`crates/envoy-http2/src/hcm.rs`):** the H2 record build lives in a SEPARATE function
  (`finalize_h2_stream`), not inline. So the capture `let dynamic_metadata = filter_req.dynamic_metadata;` (same
  partial-move pattern, after the 4-field write-back, before `match decode_decision`) is threaded as a NEW owned
  parameter `dynamic_metadata: BTreeMap<String, BTreeMap<String, String>>` to `finalize_h2_stream` (single call
  site updated). The record build moves it in directly (`dynamic_metadata,` shorthand — single-use). H2 builds its
  OWN record and does NOT inherit from H1 (spec C-1), so this is the SOLE proof of the H2 path (fixture 0041 is
  H1-only).

Both partial-move captures compile — confirmed. Chains WITHOUT `set_metadata` are unchanged (empty metadata →
their formats don't use `%DYNAMIC_METADATA%`); every pre-existing H1/H2 test stays green.

**Backstop approach — END-TO-END log-scrape for BOTH crates** (preferred over the record-field assertion; exercises
the operator render too):
- **H1 (`h1_dynamic_metadata_threads_into_access_log`):** builds a `[set_metadata, router]` pipeline
  (`set_metadata_router_pipeline()` helper writes `envoy.test`→`{tier: prod}`), an `HCMConfig` with a `FileSink`
  whose `log_format` is `%DYNAMIC_METADATA(envoy.test:tier)% / %DYNAMIC_METADATA(envoy.test:missing)%` against a
  `direct_response` 200 route; drives one H1 GET via the existing `drive` helper; scrapes the written line and
  asserts it equals `prod / -\n` (present key `prod`, absent key `-`).
- **H2 (`h2_dynamic_metadata_threads_into_access_log`):** builds the inner `Http1HCMConfig` via `from_config` from
  a `HttpConnectionManagerConfig` carrying the same `[set_metadata, router]` filter chain + a FileAccessLog with the
  same `log_format`; drives one real H2 GET via the `h2::client` + `spawn_h2_hcm` harness; scrapes the line and
  asserts `prod / -\n`. Both watched FAIL first (line `- / -\n` — present key renders `-` because the threading
  isn't wired), then PASS after the capture.

**Verification:**
- `cargo test -p envoy-http1` → `test result: ok. 128 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`
  (incl. the new H1 backstop).
- `cargo test -p envoy-http2` → `test result: ok. 73 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out`
  (incl. the new H2 backstop).
- `cargo fmt --all -- --check` clean. `#![forbid(unsafe_code)]` holds. Only `crates/envoy-http1/src/hcm.rs` and
  `crates/envoy-http2/src/hcm.rs` touched.

---

## Task 11 — fixture 0041 differential

**Files created:**
- `tests/fixtures/0041-http-set-metadata-dynamic-metadata/envoy.yaml` — H1 `direct_response` 200 `ok\n` listener;
  filter chain `[set_metadata, router]` where `set_metadata` writes `envoy.test`→`{tier: prod}` via the modern
  repeated form `metadata: [{ metadata_namespace: envoy.test, value: { tier: prod } }]` under
  `@type …set_metadata.v3.Config` (§A1); a `file` access-logger whose `log_format.text_format_source.inline_string`
  is `"m=%REQ(:METHOD)% code=%RESPONSE_CODE% tier=%DYNAMIC_METADATA(envoy.test:tier)% missk=%DYNAMIC_METADATA(envoy.test:missing)% missns=%DYNAMIC_METADATA(envoy.absent:k)%\n"`; mount `/tmp/0041-envoy-mount/access.log`;
  `0.0.0.0` bind + admin (port 0) + `generate_request_id: false`. `{{PORT}}` substituted as in 0040.
- `tests/fixtures/0041-http-set-metadata-dynamic-metadata/envoy-rust.yaml` — identical EXCEPT the 0040 per-side
  convention: `127.0.0.1` bind, no admin block, no `generate_request_id`, mount `/tmp/0041-envoy-rust-mount/access.log`.
- `tests/fixtures/0041-http-set-metadata-dynamic-metadata/expectations.yaml` — `kind: http1_access_log_byte_exact`;
  `expected_access_log_paths` for both proxies; 2 probes: probe 1 `{ method: get, path: /a, host: envoy-rust.test }`
  (expected `m=GET code=200 tier=prod missk=- missns=-`), probe 2
  `{ method: post, path: /b, host: envoy-rust.test, body: "x" }` (expected `m=POST code=200 tier=prod missk=- missns=-`).
- `tests/fixtures/0041-http-set-metadata-dynamic-metadata/README.md` — pins §A1 (`@type …v3.Config`, modern repeated
  form, no `:N`), §A3 (raw unquoted `prod`), §A4 (absent `-`), §A6 determinism; documents the present+absent probe
  pair as the anti-echo (echo-the-config-literal) guard.
- `tests/differential/tests/set_metadata_dynamic_metadata.rs` — mirrors `access_log_command_operators.rs`; test fn
  `set_metadata_dynamic_metadata` calls `differential::run_fixture` on the `0041-…` dir (Docker-availability is
  guarded inside `run_fixture`, NOT `#[ignore]`-gated — same as 0040).

**The present + absent probe pair:** each line carries ONE present read (`tier=prod`) and TWO absent reads
(`missk=-`, `missns=-`); the absent KEY (`envoy.test:missing`) + absent NAMESPACE (`envoy.absent:k`) both resolve `-`
through the SAME store path that yields `prod` for the present key — proving the implementation is store-backed, not
an echo-the-configured-literal.

**Verification:**
- **Compile:** `cargo build -p differential --tests` → clean (the new test file compiles).
- **Config-parse smoke (envoy-rust ACCEPTS):** `envoy_config::parse_bootstrap` on `envoy-rust.yaml` (with `{{PORT}}`
  substituted) → `Ok` ("envoy-rust ACCEPTS fixture 0041 envoy-rust.yaml"). Throwaway example used + removed; not
  committed.
- **Local differential — RAN BYTE-IDENTICAL:** `cargo test -p differential set_metadata_dynamic_metadata --
  --include-ignored` → `test result: ok. 1 passed`. Docker present + `envoyproxy/envoy:v1.33.0` image present; the
  access-log file-scrape differential is locally authoritative on this host (memory
  `host-docker-desktop-virtiofs-no-inotify`). Cross-proxy whole-line byte-exact assertion GREEN for both probes
  (present `tier=prod` + absent `missk=- missns=-`). NOTE: the first run hit a STALE `envoy-bin` (pre-T8 binary →
  `unknown access-log operator keyword 'DYNAMIC_METADATA'`); after `cargo build -p envoy-bin` the run is green — the
  source `command_operator.rs` carries the `DYNAMIC_METADATA` arm (T8). Test-started v1.33.0 container auto-cleaned by
  the harness.

---

## Task 12 — BEHAVIOR_CONTRACT + fuzz seeds + M32-4/M32-5

Documents the new operator; folds the two M32 carry-forwards; seeds the EXISTING fuzz targets (NO new target — §A
§3.8, no ci.yml change needed).

**(A) M32-4 — looped the default-equivalence oracle.** `crates/envoy-accesslog/src/default_format.rs`: replaced the
single-record `compiled_default_matches_legacy_concatenator` with a loop over THREE records — (1) the baseline
`make_baseline_record()` direct_response record (fixture-0012 surface), (2) a 5xx/router-proxy record
(`response_code: 503`, `upstream_host: Some("127.0.0.1:8080")`, `upstream_service_time: Some(2ms)`), (3) a UTF-8
record (`user_agent: "Mözillá/5.0 — café"`, `authority: "héllo.example"`). Each asserts
`engine == format!("{legacy}\n")`. Every record carries an (empty) `dynamic_metadata` field via
`make_baseline_record`; since the DEFAULT format does NOT reference `%DYNAMIC_METADATA%`, the engine≡legacy
equivalence holding for every record PROVES the phase-33 operator did not perturb the default format. Coverage-widening
of an existing passing test — stayed GREEN.

**(B) M32-5 — deleted the vestigial 0-byte `payload.bin`.** `git rm
tests/fixtures/0040-accesslog-command-operators/inputs/payload.bin` (it was 0 bytes; the `Http1AccessLogByteExact`
driver drives probes, NOT an input file). Confirmed nothing references it: a `grep -rn payload.bin` over `tests/` +
`crates/` hits ONLY the unrelated tcp_echo fixtures 0001/0005/0006 and the `tcp_echo` driver in
`tests/differential/src/lib.rs` — never 0040.

**(C) Fuzz seeds (existing targets, NO new target).**
- `crates/envoy-accesslog/fuzz/corpus/accesslog_format_parse/dynamic_metadata.txt` — a format string exercising the
  new operator: `m=%REQ(:METHOD)% tier=%DYNAMIC_METADATA(envoy.test:tier)% miss=%DYNAMIC_METADATA(envoy.absent:k)%\n`
  (present + absent-namespace reads).
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_set_metadata_dynamic_metadata.yaml` — a minimal valid bootstrap
  (concrete `port_value: 10000`, no `{{PORT}}`) with a `set_metadata` filter (`@type …v3.Config`, modern
  `metadata: [{ metadata_namespace: envoy.test, value: { tier: prod } }]`) + a router + a file access logger whose
  `log_format` uses `%DYNAMIC_METADATA(envoy.test:tier)%`. Modeled on fixture 0041's `envoy-rust.yaml`.
- **Both seeds parse** via the targets' entry points (throwaway integration tests used + removed; NOT committed):
  `envoy_accesslog::parse_format(seed)` → `Ok`; `envoy_config::parse_bootstrap(seed)` → `Ok`. No full
  `cargo +nightly fuzz` build run here — that is the state-4 §7.5 gate (d).

**(D) BEHAVIOR_CONTRACT.md extension.** Added a `### Phase 33 (ADR-0081): the %DYNAMIC_METADATA% operator +
set_metadata` subsection AFTER the phase-32 subsection in the "Access log field mapping" section. Documents: the
`%DYNAMIC_METADATA(namespace:key)%` operator (single-level, two-segment, `:`-separated, CASE-SENSITIVE, NO `:N`
truncation — boot-fatal; no-arg boot-fatal; 1-seg/3+-seg boot-fatal); the resolution
`record.dynamic_metadata.get(ns)?.get(key)`; the RAW UNQUOTED scalar-string byte form (§A3) + absent `-` (§A4); the
deterministic cross-proxy classification (witness fixture 0041, present+absent probe pair guards against
echo-the-literal); the §2.2 deferrals (non-string Values → JSON-quoted; nested paths; whole-namespace; deprecated
top-level form; `:N`); and the `set_metadata` config-shape note (`@type …v3.Config`, modern repeated form, string-only
`value`, empty-namespace boot-fatal `ConfigError::SetMetadataEmptyNamespace`).

**Verification:**
- `cargo fmt -p envoy-accesslog` → clean (only `default_format.rs` changed in code).
- `cargo test -p envoy-accesslog` → `test result: ok. 48 passed; 0 failed; 0 ignored` (incl. the looped
  `compiled_default_matches_legacy_concatenator`).
- `cargo test -p differential access_log_command_operators -- --include-ignored` → `test result: ok. 1 passed`
  (10.63s) — fixture 0040 stays GREEN after the `payload.bin` removal (Docker + `envoyproxy/envoy:v1.33.0` present;
  locally authoritative per memory `host-docker-desktop-virtiofs-no-inotify`).
