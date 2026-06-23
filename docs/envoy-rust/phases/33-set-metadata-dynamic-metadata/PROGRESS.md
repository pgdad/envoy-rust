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
