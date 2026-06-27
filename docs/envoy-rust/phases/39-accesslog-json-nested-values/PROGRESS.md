# Phase 39 — `39-accesslog-json-nested-values` — PROGRESS (state-3 implementation)

Per-task TDD evidence (failing test → impl → green → commit). Locked facts: ADR-0094 §A–§H.

---

## T1 — Recursive `JsonFormatValue` config model (serde)

- RED: `cargo test -p envoy-config json_format_value` → `error[E0432]: unresolved import super::JsonFormatValue` (enum undefined).
- Impl: added `#[serde(untagged)] enum JsonFormatValue { Null, Bool(bool), Format(String), Array(Vec<_>), Object(BTreeMap<String,_>) }` in `bootstrap.rs`; changed `SubstitutionFormatString.json_format` field to `Option<BTreeMap<String, JsonFormatValue>>`; re-exported from `lib.rs`.
- GREEN: `cargo test -p envoy-config json_format_value` → 4 passed (incl. `numeric_literal_leaf_is_rejected` CF-39-1: `42`/`1.5` → `is_err()`; recursive variants + per-level sort + list order).

## T2 — Recursive config validator (per-leaf `parse_format`)

- RED context: field-type change forced the flat `map.values()` loop (calling `parse_format(&String)`) to break; replaced with `validate_json_format_value(&JsonFormatValue)` recursive helper.
- Impl: `fn validate_json_format_value` recurses `Object`/`Array`, calls `parse_format` per `Format` leaf → `InvalidAccessLogFormat`, `Bool`/`Null` no-ops. Exactly-one-of (`AmbiguousLogFormat`) + empty-top-map UNCHANGED. NO new `ConfigError` variant.
- GREEN: `cargo test -p envoy-config` → 531 passed; 0 failed. New tests: malformed op in nested object leaf / nested list leaf → `InvalidAccessLogFormat`; well-formed nested config validates Ok.
- Commit: `4dbde13 feat(config): recursive JsonFormatValue (nested json_format) + recurse validator per leaf [phase39 T1 T2]`

## T3 — Recursive `CompiledJsonValue` compile (`from_map`)

- RED: `cargo test -p envoy-accesslog json_format` → `cannot find type JsonValueInput` / `CompiledJsonValue` (undefined; old `from_map(&BTreeMap<String,String>)` signature).
- Impl: added `pub enum JsonValueInput` (accesslog-side MIRROR — NO envoy-config dep; caller maps at the bridge) + internal `enum CompiledJsonValue { Null, Bool, Leaf, Array, Object }`; `from_map` recurses via `CompiledJsonValue::compile` (Format→`Leaf(parse_format(s)?)`, Bool/Null verbatim, Object/Array recurse). Re-exported `JsonValueInput`. Updated the existing `file_sink.rs` test caller.
- GREEN: `cargo test -p envoy-accesslog` → 60 passed; 0 failed.
- Commit: `d89a68b feat(accesslog): recursive CompiledJsonValue compile + render [phase39 T3]`

## T4 — Recursive `render` (byte-exact core) + folded Minors

- The recursive `render_into` landed with T3 (needed for the existing render tests to compile); T4 adds the load-bearing byte-exact assertions.
- GREEN: `cargo test -p envoy-accesslog json_format` → 23 passed. Key tests:
  - `renders_authoritative_nested_fixture_line` = ADR-0094 §H byte-exact: `{"arequest":{"aaa":200,"method":"GET","zpath":"/"},"blist":["GET",200,null],"mtop":"code-200","zouter":"HTTP/1.1"}\n`.
  - `flat_round_trip_byte_unchanged_through_recursive_encoder` = the phase-38 0046 flat line byte-identical (depth-1==flat regression witness).
  - per-level sort (§A), list order preserved (§B), bool/null native-typed (§D), empty `{}`/`[]` (§F), absent-op-in-list→null (§F), depth-3, list-of-objects, nested key+value escaping.
- **M38-3 folded:** `empty_value_string_leaf_renders_empty_quoted` (`""`→`""`, flat + nested).
- **M38-4 folded:** `m38_4_typed_path_gaps_in_nested_position` — `:N` truncation, `?ALT`, non-zero `%DURATION%`, control-char escaping all exercised IN the nested object path.
- **M38-1:** the resolve+truncate chain is ALREADY shared — `encode_single_op` (JSON path) and `render_op` (text path) both call the same `command_operator::{resolve_req,resolve_resp,truncate_bytes}` helpers. No further extraction done (it would be cosmetic with byte-output regression risk; the PLAN marked it optional/round-trip-guarded). Round-trip guard test present (`flat_round_trip_byte_unchanged…`). M38-1 considered folded-equivalent / no-op.
- Commit: `70be3be feat(accesslog): recursive json_format render byte-exact nested + fold M38-3/M38-4 [phase39 T4]`

## T5 — HCM/config bridge wiring (type ripple)

- RED: `cargo test -p envoy-http1 compiled_log_format` → `error[E0308]: expected &BTreeMap<String, JsonValueInput>, found &BTreeMap<String, JsonFormatValue>`.
- Impl: added `json_format_value_to_input` 1:1 bridge in `hcm.rs` (caller-side mapping; dependency direction forbids envoy-accesslog seeing envoy-config). H2 default site untouched.
- GREEN: `cargo test -p envoy-http1 compiled_log_format` → 5 passed (incl. new `compiled_log_format_picks_json_arm_nested` rendering `{"list":["HTTP/1.1",true,null],"obj":{"code":200,"method":"GET"}}\n`). `cargo build --workspace --all-targets` → Finished.
- Commit: `54319ea feat(hcm): wire recursive json_format map via JsonFormatValue->JsonValueInput bridge [phase39 T5]`

## T6 — Fixture 0047 nested json_format (byte-exact differential)

- Created `tests/fixtures/0047-accesslog-json-nested/{envoy.yaml,envoy-rust.yaml,expectations.yaml,README.md}` (mirror 0046; §H nested json_format) + `tests/differential/tests/access_log_json_nested.rs`.
- Rebuilt debug `envoy-bin` first (harness uses target/debug/envoy-bin).
- GREEN (ISOLATION, --test-threads=1): `cargo test -p differential --test access_log_json_nested` → `test access_log_json_nested ... ok` (1 passed) vs `envoyproxy/envoy:v1.33.0`. Byte-identical §H line confirmed cross-proxy.
- Regression: `cargo test -p differential --test access_log_json_format` (0046 flat) → 1 passed.
- Commit: `f8d6ec6 test(differential): fixture 0047 nested json_format byte-exact [phase39 T6]`

## T7 — Fuzz seed + BEHAVIOR_CONTRACT + bookkeeping

- Seed: `crates/envoy-config/fuzz/corpus/parse_bootstrap/json_format_nested.yaml` (nested object + list + bool/null + empty {}/[] leaves) with its OWN `!`-un-ignore line. `git ls-files | grep json_format_nested` → TRACKED. `cargo fuzz run parse_bootstrap <seed> -- -runs=1` → executed clean (no crash). Verified the seed parses to Ok (recursive validator success path). NO new fuzz target.
- BEHAVIOR_CONTRACT: added "Phase 39 (ADR-0094): the RECURSIVE (nested) json_format encoder" subsection (§A–§H + CF-39-1) + supersession-chain pointer.
- Carry-forwards: **CF-39-1** (numeric literal leaves) recorded. **M38-1** (shared resolve — already shared), **M38-3**, **M38-4** folded at T4. M38-2 + RBAC/older Minors stay live un-folded.

## Final local gate

- `cargo fmt --all -- --check` → exit 0 (clean, after one reformat of the lib.rs re-export line).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` → `Finished` exit 0 (no warnings).
- `cargo build --workspace --all-targets` → `Finished` exit 0.
- `cargo test --workspace --exclude differential` → ALL green, 0 failed. Key crates: `envoy-config` 531 passed; `envoy-accesslog` 72 passed; `envoy-http1` 132 passed; `envoy-filter` 208; `envoy-cluster` 160; (1 pre-existing `envoy-http2` test ignored — unrelated).
- Differential (Docker-gated): `0047-accesslog-json-nested` GREEN in isolation (`--test-threads=1`) vs `envoyproxy/envoy:v1.33.0`; `0046-accesslog-json-format` (flat regression witness) GREEN. The full differential suite was NOT run end-to-end locally (Docker-Desktop host false-REDs under parallel load per project memory; CI authoritative) — the two phase-relevant fixtures verified directly.
- `cargo deny check` → advisories ok, bans ok, licenses ok, sources ok (no new dependency; only pre-existing unmatched-license-allowance warnings).


