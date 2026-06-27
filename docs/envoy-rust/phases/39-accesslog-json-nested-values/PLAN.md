# Phase 39 — `39-accesslog-json-nested-values` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. TDD per `superpowers:test-driven-development` on every task — failing test first, then minimal impl, then green, then commit.

**Goal:** Make the phase-38 `json_format` access-log encoder RECURSIVE — support nested JSON objects + lists (and `bool`/`null` literal leaves) as `google.protobuf.Struct` values, byte-equivalent to upstream Envoy v1.33.0.

**Architecture:** Replace the flat `json_format: Option<BTreeMap<String,String>>` config value type with a recursive `#[serde(untagged)]` enum `JsonFormatValue`, and widen the `CompiledJsonFormat` encoder to a recursive `CompiledJsonValue`. The phase-38 leaf helpers (`encode_json_value`/`encode_single_op`/`json_escape_into`/`quote*`) are reused VERBATIM at every recursion leaf; the new code is purely the `{…}`/`[…]` structural recursion + the `bool`/`null` arms. No new connection plumbing, request attribute, operator, crate, dependency, fuzz-target, or `ConfigError` variant.

**Tech Stack:** Rust (workspace), `serde`/`serde_yaml` (config), the hand-rolled `envoy-accesslog` command-operator engine, the `testcontainers` differential harness.

**§6.2 LOCKED FACTS (ADR-0094 — authored against these inline; no projections survive):**
- **§A** per-level key sorting = SORTED (UTF-8 byte order) at EVERY object level → `BTreeMap<String,_>` at each level.
- **§B** list element order = CONFIG order (NOT sorted) → `Vec<_>`.
- **§C** at-depth type inference = the SAME phase-38 per-leaf rule (numeric op→unquoted number, string op→quoted, absent→`null`, mixed/literal→quoted string with `-`) — reuse `encode_json_value` verbatim.
- **§D** non-string SCALAR leaves emit NATIVE TYPED: `bool`→`true`/`false`, `null`→`null` (byte-exact, IN); NUMERIC literals route through protobuf-`double` (`1000000`→`1e+06`, `1.5`→`"1.5"`) — a rabbit hole, **DEFERRED as CF-39-1** (envoy-rust boot-rejects a numeric literal `json_format` value).
- **§E** compact separators (no spaces) at every level; exactly ONE trailing `\n` on the whole top-level object; NO inter-element/inter-level `\n`.
- **§F** empty nested `{}`→`{}`, empty `[]`→`[]`; absent-operator leaf in a list → `null` in place.
- **§G** malformed operator in a NESTED leaf → BOOT-FATAL via the EXISTING `ConfigError::InvalidAccessLogFormat`; exactly-one-of (`AmbiguousLogFormat`) + empty-top-map (`{}\n`) UNCHANGED → NO new `ConfigError` variant.
- **§H** authoritative fixture-`0047` byte-exact line (CASE-1, captured live):
  `{"arequest":{"aaa":200,"method":"GET","zpath":"/"},"blist":["GET",200,null],"mtop":"code-200","zouter":"HTTP/1.1"}\n`

---

## File Structure

- **Modify** `crates/envoy-config/src/bootstrap.rs` — (a) add the recursive `pub enum JsonFormatValue` (`#[serde(untagged)]`); (b) change `SubstitutionFormatString.json_format` field type `Option<BTreeMap<String,String>>` → `Option<BTreeMap<String, JsonFormatValue>>` (`:708`); (c) make the access-log validator loop (`:4371` region, currently `map.values()` calling `parse_format`) RECURSE the tree, calling `parse_format` per `Format` leaf.
- **Modify** `crates/envoy-config/src/lib.rs` — re-export `JsonFormatValue` (next to `SubstitutionFormatString`/`DataSourceInline`).
- **Modify** `crates/envoy-accesslog/src/json_format.rs` — widen `CompiledJsonFormat` to hold a recursive `CompiledJsonValue` (top level stays `BTreeMap<String, CompiledJsonValue>`); `from_map` takes the recursive config map + an `&JsonFormatValue`-equivalent; recursive `render`. Reuse the leaf helpers verbatim. Fold M38-1 (shared `resolve_*_value` helper across text/JSON), M38-3 (empty value-string test), M38-4 (typed-path test gaps) where the leaf path is touched.
- **Modify** `crates/envoy-accesslog/src/lib.rs` — re-export `CompiledJsonValue` if it needs to be public for the from-config bridge (likely internal — confirm).
- **Modify** `crates/envoy-http1/src/hcm.rs:1269` — the `(None, Some(map))` arm `CompiledJsonFormat::from_map(map)` call: type ripples; no logic change expected. Confirm the H2 default site is untouched.
- **Create** `tests/fixtures/0047-accesslog-json-nested/` — `envoy.yaml` + `envoy-rust.yaml` (identical nested `json_format`) + `inputs/`/`expectations.yaml` + `README.md` (mirroring `0046-accesslog-json-format/`).
- **Create** `tests/differential/tests/access_log_json_nested.rs` — a new test file mirroring the phase-38 `tests/differential/tests/access_log_json_format.rs` (which calls `differential::run_fixture(...)` for `0046`); reuse `Driver::Http1WithAccessLog` + `AccessLogByteExactProbe` (whole-line byte-exact compare). NOT a `src/` registration.
- **Modify** `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — extend the "Access log field mapping" `json_format` subsection with the recursive shape + the §A–§H facts + CF-39-1.
- **Modify** the `parse_bootstrap` fuzz corpus — add a nested-`json_format` seed (verify it lands in `git ls-files`; the corpus dir is `*`-gitignored — needs a `!`-un-ignore line).

> Before starting: `git pull` is not needed (single-session). Read the CURRENT `crates/envoy-accesslog/src/json_format.rs`, `crates/envoy-config/src/bootstrap.rs:700-720` + `:4360-4400`, and `crates/envoy-http1/src/hcm.rs:1249-1275` — the line anchors above are from the phase-38 close state and may have drifted by a few lines.

---

### Task 1: Recursive `JsonFormatValue` config model (serde)

**Files:** Modify `crates/envoy-config/src/bootstrap.rs`, `crates/envoy-config/src/lib.rs`. Test: inline `#[cfg(test)]` in `bootstrap.rs`.

- [ ] **Step 1 — failing test.** Add tests asserting the recursive deserialization from YAML:
  - a string scalar → `JsonFormatValue::Format(s)`;
  - `true`/`false` → `JsonFormatValue::Bool(_)`; `null`/`~` → `JsonFormatValue::Null`;
  - a map → `JsonFormatValue::Object(BTreeMap)` (keys land sorted by `BTreeMap`);
  - a sequence → `JsonFormatValue::Array(Vec)` (order preserved);
  - **CF-39-1:** a NUMERIC scalar (`42`, `1.5`) → deserialization **ERRORS** (matches no untagged arm) — assert `serde_yaml::from_str::<SubstitutionFormatString>(…).is_err()` for a json_format with a numeric leaf, documenting the deferral.
```rust
// representative
let s: SubstitutionFormatString = serde_yaml::from_str(
    "json_format:\n  a: \"%PROTOCOL%\"\n  b: true\n  c: ~\n  d: { z: \"%REQ(:METHOD)%\", a: \"x\" }\n  e: [ \"%PROTOCOL%\", false ]\n",
).unwrap();
// assert variants + that d's BTreeMap iterates a,z (sorted) and e is order-preserved
assert!(serde_yaml::from_str::<SubstitutionFormatString>("json_format:\n  n: 42\n").is_err()); // CF-39-1
```
- [ ] **Step 2 — run, verify FAIL** (`JsonFormatValue` undefined / field type mismatch): `cargo test -p envoy-config json_format -- --nocapture`.
- [ ] **Step 3 — implement.** Add:
```rust
/// A `json_format` value (Envoy `google.protobuf.Struct` value). Recursive: a
/// command-operator format string, a `bool`/`null` literal, a nested object, or
/// a list. NUMERIC literals are NOT accepted (ADR-0094 §D / CF-39-1 — the
/// protobuf-double formatting is deferred).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum JsonFormatValue {
    Null,                                                   // YAML null/~
    Bool(bool),                                             // YAML true/false
    Format(String),                                         // YAML string → format string
    Array(Vec<JsonFormatValue>),                            // YAML sequence (ordered)
    Object(std::collections::BTreeMap<String, JsonFormatValue>), // YAML map (sorted keys)
}
```
  - Arm order matters for `#[serde(untagged)]`: `Null` before `Bool` before `Format` before `Array`/`Object`. Verify `serde_yaml` does NOT coerce a number into `Format(String)` (if it does, add a custom check / `deny`); the CF-39-1 test guards this.
  - Change the field: `pub json_format: Option<std::collections::BTreeMap<String, JsonFormatValue>>` (`:708`).
  - Re-export `JsonFormatValue` from `lib.rs`.
- [ ] **Step 4 — run, verify PASS.** `cargo test -p envoy-config json_format`.
- [ ] **Step 5 — commit.** `feat(config): recursive JsonFormatValue (nested json_format) [phase39 T1]`

### Task 2: Recursive config validator (per-leaf `parse_format`)

**Files:** Modify `crates/envoy-config/src/bootstrap.rs` (the access-log validator, `:4371` region). Test: inline.

- [ ] **Step 1 — failing test.** A `json_format` with a malformed operator in a NESTED leaf must fail config validation with `ConfigError::InvalidAccessLogFormat` (ADR-0094 §G):
```rust
// a nested { a: { b: "%NOPE%" } } json_format → InvalidAccessLogFormat at config-load
assert!(matches!(validate(...), Err(ConfigError::InvalidAccessLogFormat { .. })));
// a well-formed nested config validates Ok; exactly-one-of + empty-top-map unchanged
```
- [ ] **Step 2 — run, verify FAIL** (validator only walks the flat `.values()`).
- [ ] **Step 3 — implement.** Replace the `for v in map.values() { parse_format(v)? }`-style loop with a recursive walk over `JsonFormatValue`: for each `Format(s)` leaf call `parse_format(s)` mapping the error to `InvalidAccessLogFormat`; `Object`/`Array` recurse; `Bool`/`Null` are no-ops. Keep the exactly-one-of (`AmbiguousLogFormat`) + empty-top-map acceptance UNCHANGED. (A small `fn validate_json_format_value(&JsonFormatValue) -> Result<(), ...>` helper.)
- [ ] **Step 4 — run, verify PASS.** `cargo test -p envoy-config`.
- [ ] **Step 5 — commit.** `feat(config): recurse json_format validator per leaf [phase39 T2]`

### Task 3: Recursive `CompiledJsonValue` compile (`from_map`)

**Files:** Modify `crates/envoy-accesslog/src/json_format.rs`. Test: inline.

- [ ] **Step 1 — failing test.** `CompiledJsonFormat::from_map` accepts the recursive config map and compiles every `Format` leaf via `parse_format` (returning the first error); `Bool`/`Null` carried verbatim:
```rust
// build a recursive config map (mirroring JsonFormatValue) and assert from_map Ok,
// and that a nested "%NOPE%" leaf → Err(FormatParseError)
```
- [ ] **Step 2 — run, verify FAIL.**
- [ ] **Step 3 — implement.** Add the recursive compiled type + change `CompiledJsonFormat` to wrap `BTreeMap<String, CompiledJsonValue>`:
```rust
#[derive(Debug, Clone, PartialEq)]
pub enum CompiledJsonValue {
    Null,
    Bool(bool),
    Leaf(Vec<Segment>),                                 // compiled format string
    Array(Vec<CompiledJsonValue>),
    Object(std::collections::BTreeMap<String, CompiledJsonValue>),
}
```
  - `from_map` walks a recursive map and builds `BTreeMap<String, CompiledJsonValue>` (a `compile_value(...) -> Result<CompiledJsonValue, FormatParseError>` helper: `Format(s)`→`Leaf(parse_format(s)?)`, `Bool`/`Null` mapped, `Object`/`Array` recurse).
  - **Dependency direction is FIXED, not a choice:** `envoy-config` depends on `envoy-accesslog` (one-directional; verified — `crates/envoy-config/Cargo.toml`). `JsonFormatValue` lives in `envoy-config`, so `from_map` **cannot** take `&JsonFormatValue` (that would force `envoy-accesslog`→`envoy-config`, a cycle). Therefore: introduce a small **accesslog-side mirror enum** (e.g. `JsonValueInput { Null, Bool(bool), Format(String), Array(Vec<…>), Object(BTreeMap<String, …>) }`) that `from_map` accepts, and author the `JsonFormatValue` → mirror mapping in `envoy-config`/`hcm` (the caller side). Do NOT add any `envoy-accesslog`→`envoy-config` dependency.
- [ ] **Step 4 — run, verify PASS.** `cargo test -p envoy-accesslog json_format`.
- [ ] **Step 5 — commit.** `feat(accesslog): recursive CompiledJsonValue compile [phase39 T3]`

### Task 4: Recursive `render` (the byte-exact core)

**Files:** Modify `crates/envoy-accesslog/src/json_format.rs`. Test: inline (the load-bearing unit tests).

- [ ] **Step 1 — failing tests.** Assert byte-exact renders against the ADR-0094-captured lines:
  - **§H authoritative:** the CASE-1 nested line `{"arequest":{"aaa":200,"method":"GET","zpath":"/"},"blist":["GET",200,null],"mtop":"code-200","zouter":"HTTP/1.1"}\n` from the fixture record.
  - per-level sort (nested object with non-alpha keys → sorted output); list order preserved (NOT sorted); at-depth type inference (nested `%RESPONSE_CODE%`→`200`, nested absent→`null`); `bool`/`null` literal leaves (`true`/`null`); empty `{}`/`[]`; depth-3 (`{"d1":{"d2":{"d3":200}}}`); list-of-objects (CASE-5 `{"objlist":[{"k":"GET","z":"HTTP/1.1"}]}`); escaping a nested key + nested value (reuse the phase-38 escaping cases at depth).
  - **0046 flat round-trip byte-unchanged:** the phase-38 `renders_authoritative_fixture_line` line must render byte-identical through the recursive encoder (depth-1 == flat).
  - **M38-3 fold:** an empty value-string leaf (`""`) → `""`. **M38-4 fold:** `%REQ(...):N%` truncation / `?ALT` / non-zero `%DURATION%` / a control-char in a rendered value, exercised IN the nested path.
- [ ] **Step 2 — run, verify FAIL.**
- [ ] **Step 3 — implement** the recursive `render`:
```rust
fn render_value(out: &mut String, v: &CompiledJsonValue, r: &AccessLogRecord) {
    match v {
        CompiledJsonValue::Null => out.push_str("null"),
        CompiledJsonValue::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        CompiledJsonValue::Leaf(segs) => encode_json_value(out, segs, r), // phase-38 helper VERBATIM
        CompiledJsonValue::Array(items) => {
            out.push('[');
            for (i, it) in items.iter().enumerate() { if i>0 { out.push(','); } render_value(out, it, r); }
            out.push(']');
        }
        CompiledJsonValue::Object(m) => {
            out.push('{');
            for (i, (k, vv)) in m.iter().enumerate() {
                if i>0 { out.push(','); }
                out.push('"'); json_escape_into(out, k); out.push_str("\":");
                render_value(out, vv, r);
            }
            out.push('}');
        }
    }
}
// CompiledJsonFormat::render → render the top-level Object, then push '\n'.
```
  - **M38-1 fold (optional, if low-risk):** extract a shared `pub(crate) fn resolve_*_value` used by both the text `render_op` and `encode_single_op` to kill the duplicated resolve+truncate chain — only if it leaves the byte output identical (guard with the round-trip test).
- [ ] **Step 4 — run, verify PASS.** `cargo test -p envoy-accesslog`.
- [ ] **Step 5 — commit.** `feat(accesslog): recursive json_format render (byte-exact nested) [phase39 T4]`

### Task 5: HCM/config bridge wiring (type ripple)

**Files:** Modify `crates/envoy-http1/src/hcm.rs` (`:1269` json arm) + wherever the `JsonFormatValue`→accesslog bridge lives. Test: the existing `compiled_log_format_picks_json_arm` test (updated for the recursive map type).

- [ ] **Step 1 — failing/updated test.** Update `compiled_log_format_picks_json_arm` to build a recursive `json_format` map and assert `compiled_log_format` returns `LogFormat::Json(_)`; add a NESTED-map case. The text/default arms (`_picks_text_arm`, `_falls_back_to_default_when_absent`) stay byte-unchanged.
- [ ] **Step 2 — run, verify FAIL/compile-error** (type mismatch on the `from_map` arg).
- [ ] **Step 3 — implement.** Thread the recursive map through `compiled_log_format` → `CompiledJsonFormat::from_map`. Confirm the H2 default site (`LogFormat::Text(CompiledFormat::default())`) is untouched. No logic change beyond the type.
- [ ] **Step 4 — run, verify PASS.** `cargo test -p envoy-http1 compiled_log_format && cargo build --workspace --all-targets`.
- [ ] **Step 5 — commit.** `feat(hcm): wire recursive json_format map [phase39 T5]`

### Task 6: Fixture `0047-accesslog-json-nested` (byte-exact differential)

**Files:** Create `tests/fixtures/0047-accesslog-json-nested/{envoy.yaml,envoy-rust.yaml,expectations.yaml,README.md,inputs/}`; modify the differential wiring that registers `0046`.

- [ ] **Step 1 — failing test.** Wire the `0047` case into the differential harness reusing `Driver::Http1WithAccessLog` + `AccessLogByteExactProbe` (whole-line byte-exact compare), with the §H nested `json_format` (a `direct_response` 200 route; the bare `GET /` probe). Expected byte-exact line = §H.
- [ ] **Step 2 — run, verify FAIL** (envoy-rust not yet emitting / fixture absent). NOTE: the differential runs `target/debug/envoy-bin` — rebuild it (`cargo build -p envoy-bin`) before running, or it REDs with a stale `unknown field`/encoder ([[differential-harness-uses-debug-envoy-bin]]).
- [ ] **Step 3 — implement.** Author the paired configs (identical nested `json_format`); ensure the envoy-rust config parses + renders byte-exact. Keep the line in the deterministic subset (no timing/id operators). MAY add a `bool`/`null` literal leaf to exercise §D in the byte-exact line.
- [ ] **Step 4 — run, verify PASS** (cross-proxy byte-identical). `cargo test -p differential <0047 case>` — and confirm `0046` + all `0001`-`0045` stay green. (Local host false-REDs per the recalled memory notes are CI-authoritative; run the new case in isolation first.)
- [ ] **Step 5 — commit.** `test(differential): fixture 0047 nested json_format byte-exact [phase39 T6]`

### Task 7: Fuzz seed + BEHAVIOR_CONTRACT + carry-forward bookkeeping

**Files:** Modify the `parse_bootstrap` fuzz corpus (+ its `.gitignore` `!`-un-ignore line); `docs/envoy-rust/BEHAVIOR_CONTRACT.md`.

- [ ] **Step 1.** Add a nested-`json_format` `parse_bootstrap` corpus seed under a DISTINCT filename (e.g. `json_format_nested.yaml` — NOT the existing phase-38 `json_format_logger.yaml`) with its OWN `!`-un-ignore line in the fuzz `.gitignore`; verify it is tracked (`git ls-files | grep json_format_nested` — the corpus dir is `*`-gitignored, [[fuzz-corpus-seed-gitignored-by-default]]). NO new fuzz target ([[new-fuzz-target-needs-a-ci-yml-step]] — confirm none added).
- [ ] **Step 2.** Extend the BEHAVIOR_CONTRACT "Access log field mapping" `json_format` subsection with the recursive shape (§A–§H) + CF-39-1 (numeric literal leaves deferred).
- [ ] **Step 3.** Confirm the carry-forward bookkeeping: CF-39-1 is recorded; M38-1/M38-3/M38-4 folded (or noted why not) at T4; M38-2 + the RBAC Minors stay live.
- [ ] **Step 4 — run the full §7.5 gate** (state-4 will re-run, but smoke it here): `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check`.
- [ ] **Step 5 — commit.** `docs(accesslog): BEHAVIOR_CONTRACT recursive json_format + fuzz seed [phase39 T7]`

---

## Acceptance (the §7.5 phase-done gate, re-run at state-4)

(a) fixture `0047` green (cross-proxy byte-identical NESTED JSON line, §H) + (b) all `0001`–`0046` green (incl. `0046` flat-JSON byte-identical — the recursion-refactor regression witness) + (c) h2spec ≥95% (unchanged) + (d) `parse_bootstrap` + `accesslog_format_parse` clean for the short-budget CI run (with the nested seed) — NO new fuzz target + (e) build/clippy/fmt/test/deny all clean + (f) `REVIEW.md` approved. `#![forbid(unsafe_code)]` holds; NO new crate/dependency; NO new `ConfigError` variant.

## Notes for the executor

- **Crate dependency direction:** `envoy-accesslog` must NOT depend on `envoy-config` (check `Cargo.toml`). The `JsonFormatValue` (config) → `CompiledJsonValue` (accesslog) bridge therefore lives on the `envoy-config`/`hcm` side (the caller maps the config enum into whatever `CompiledJsonFormat::from_map` accepts). Decide at T3 whether `from_map` takes the config `JsonFormatValue` (requires the dep) or an accesslog-side mirror (no dep — preferred; mirror the small enum and map at the call site).
- **CF-39-1 divergence** (envoy-rust rejects a numeric-literal `json_format` value that Envoy accepts) is documented + intentional; keep the fixture/backstop free of numeric literals.
- One state per session (§5.1): this PLAN is the state-2 deliverable; state-3 implements it.

---

_Scope locked by **ADR-0093**; §6.2 facts locked by **ADR-0094** (the recon FIRES — §D scalar refinement + CF-39-1). The §6.1 split does NOT fire (**ADR-0095 reserved-but-unfired**). The state-3 implementation is the next session (`superpowers:executing-plans` / `subagent-driven-development`)._
