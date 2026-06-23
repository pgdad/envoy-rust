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
