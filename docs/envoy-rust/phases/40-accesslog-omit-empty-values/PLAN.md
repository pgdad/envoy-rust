# Phase 40 — `40-accesslog-omit-empty-values` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax. TDD per `superpowers:test-driven-development` on every task — failing test first, then minimal impl, then green, then commit.

**Goal:** Add Envoy's `SubstitutionFormatString.omit_empty_values` boolean knob — when `true`, the command-operator engine renders an absent operator as the EMPTY STRING `""` instead of the `-` sentinel — byte-equivalent to upstream Envoy v1.33.0, for BOTH `text_format` and `json_format`.

**Architecture:** A `omit_empty_values: bool` field on `SubstitutionFormatString`, threaded as an `omit_empty: bool` parameter into the EXISTING `render_value_segments` (`crates/envoy-accesslog/src/command_operator.rs`), whose four `.unwrap_or("-")` absent-substitution sites become `.unwrap_or(if omit_empty {""} else {"-"})`. `CompiledFormat` (text) and `CompiledJsonFormat` (json) each carry the flag (set at construction) and pass it through their render. The json single-operator-typed path (`encode_single_op` → `null`/number/quoted) is UNCHANGED. No key-dropping (ADR-0096 §A).

**Tech Stack:** Rust workspace; `serde` (config); the hand-rolled `envoy-accesslog` command-operator engine; the `testcontainers` differential harness.

**§6.2 LOCKED FACTS (ADR-0096 — authored against these; the SPEC's "drop-empty" framing is VOID):**
- **§A** `omit_empty_values` does NOT drop keys/entries — every key always emits.
- **§B** it SWAPS the absent-operator `-` sentinel for `""` in the command-operator MULTI-SEGMENT render (`render_value_segments`), for BOTH `text_format` AND `json_format`.
- **§C** single-operator-TYPED `json_format` values are UNAFFECTED (`encode_single_op`: absent→`null`, unchanged); the swap applies ONLY to the multi-segment/mixed-string leaf path.
- **§D** the swap applies RECURSIVELY (nested objects + lists); single-op nulls at depth stay `null`.
- **§E** all-single-absent → keys survive as `null` (not dropped, not `{}`); `omit_empty_values` is a plain `bool` (`deny_unknown_fields`); NO new `ConfigError` variant.

---

## File Structure
- **Modify** `crates/envoy-config/src/bootstrap.rs` — add `omit_empty_values: bool` (`#[serde(default)]`) to `SubstitutionFormatString` (`:704-709`).
- **Modify** `crates/envoy-accesslog/src/command_operator.rs` — `render_value_segments` (`:431`) gains an `omit_empty: bool` param AND threads it to **`render_op` (`:462`)** — the four `.unwrap_or("-")` sites (`:479`/`:486`/`:496`/`:508`) live in `render_op`, NOT directly in `render_value_segments` (which calls `render_op` per `Segment::Op`); so BOTH signatures change. The sites become `.unwrap_or(if omit_empty {""} else {"-"})`. `CompiledFormat` (`:~412`) gains an `omit_empty: bool` field; `CompiledFormat::render` (`:419`) passes it; `from_inline`/`Default` set it (`from_inline` default `false`; a setter or a `from_inline_with_omit`). `render_value_segments` has exactly 2 callers — `CompiledFormat::render` (`:420`) and `encode_json_value` (`json_format.rs:174`); `from_inline` has additional callers (`hcm.rs` tests, `file_sink.rs:318`) that keep `from_inline`'s signature + default omit=`false`, so they are unaffected.
- **Modify** `crates/envoy-accesslog/src/json_format.rs` — `CompiledJsonFormat` (`:107`) gains an `omit_empty: bool` field (set in `from_map`); `render_into` (`:71`) / `encode_json_value` (`:170`) thread `omit_empty` to `render_value_segments`. `encode_single_op` UNCHANGED.
- **Modify** `crates/envoy-accesslog/src/file_sink.rs` / `log_format.rs` / `crates/envoy-http1/src/hcm.rs` — `compiled_log_format` reads `omit_empty_values` from the config `SubstitutionFormatString` and sets it on the compiled `CompiledFormat`/`CompiledJsonFormat`.
- **Create** `tests/fixtures/0048-accesslog-omit-empty/` (`envoy.yaml`+`envoy-rust.yaml`+`expectations.yaml`+`README.md`) + `tests/differential/tests/access_log_omit_empty.rs` (mirror `access_log_json_nested.rs`).
- **Modify** the `parse_bootstrap` fuzz corpus (a `omit_empty_values: true` seed, distinct filename + `!`-un-ignore line) + `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (a `omit_empty_values` subsection).

> Before starting: read the CURRENT `command_operator.rs:410-515` (the `CompiledFormat` + `render_value_segments` + the `.unwrap_or("-")` sites), `json_format.rs:30-130` (the recursive render), and `hcm.rs:1249-1311` (`compiled_log_format` + the bridge). Line anchors are from the phase-39 close and may have drifted a few lines.

---

### Task 1: `omit_empty_values` config field
**Files:** Modify `crates/envoy-config/src/bootstrap.rs`. Test: inline.
- [ ] **Step 1 — failing test.** Assert serde round-trips `omit_empty_values`:
```rust
let s: SubstitutionFormatString = serde_yaml::from_str(
    "omit_empty_values: true\njson_format:\n  a: \"%PROTOCOL%\"\n").unwrap();
assert!(s.omit_empty_values);
let d: SubstitutionFormatString = serde_yaml::from_str("json_format:\n  a: \"x\"\n").unwrap();
assert!(!d.omit_empty_values); // default false
```
- [ ] **Step 2 — run, verify FAIL** (no field). `cargo test -p envoy-config omit_empty`.
- [ ] **Step 3 — implement.** Add `#[serde(default)] pub omit_empty_values: bool,` to `SubstitutionFormatString`. `deny_unknown_fields` retained; the exactly-one-of validator UNCHANGED (the bool composes with either arm).
- [ ] **Step 4 — run, verify PASS.**
- [ ] **Step 5 — commit.** `feat(config): SubstitutionFormatString.omit_empty_values field [phase40 T1]`

### Task 2: thread `omit_empty` into `render_value_segments` + the text `CompiledFormat` (§B)
**Files:** Modify `crates/envoy-accesslog/src/command_operator.rs`. Test: inline.
- [ ] **Step 1 — failing test.** The sentinel swap on the TEXT path:
```rust
// "m=%REQ(:METHOD)% up=%UPSTREAM_HOST% x=%REQ(X-ABSENT)%" on a record with no upstream/xff:
//   omit=false → "m=GET up=- x=-"   ;  omit=true → "m=GET up= x="   (ADR-0096 §B / CASE-3)
let segs = parse_format("up=%UPSTREAM_HOST% x=%REQ(X-FORWARDED-FOR)%").unwrap();
assert_eq!(render_value_segments(&segs, &rec_no_upstream(), false), "up=- x=-");
assert_eq!(render_value_segments(&segs, &rec_no_upstream(), true),  "up= x=");
```
- [ ] **Step 2 — run, verify FAIL** (signature/behavior).
- [ ] **Step 3 — implement.** Thread `omit_empty: bool` through `render_value_segments(segments, record, omit_empty)` → `render_op(op, record, omit_empty)` (the `.unwrap_or("-")` sites at `:479`/`:486`/`:496`/`:508` are in `render_op`); replace each `.unwrap_or("-")` with `.unwrap_or(if omit_empty { "" } else { "-" })` (bind `let empty_or_dash = if omit_empty {""} else {"-"};` once). `CompiledFormat` gains `omit_empty: bool`; `render` passes `self.omit_empty`. `from_inline` sets `false` (add a setter `with_omit_empty(bool)` or a second constructor); `Default` → `false`. **Both `render_value_segments` callers must pass the new param in THIS task to keep `envoy-accesslog` compiling (avoid a T2→T3 intermediate compile-red):** update `CompiledFormat::render` (`:420`) AND the json caller `encode_json_value` (`json_format.rs:174`) — at `:174` pass a literal `false` placeholder for now (T3 replaces it with the real threaded flag). So T2 leaves the json path byte-unchanged (omit hard-coded false) and the crate green; T3 wires the real json flag.
- [ ] **Step 4 — run, verify PASS** + all existing command_operator tests green (default-off path byte-unchanged). `cargo test -p envoy-accesslog`.
- [ ] **Step 5 — commit.** `feat(accesslog): omit_empty sentinel swap in render_value_segments + text CompiledFormat [phase40 T2]`

### Task 3: thread `omit_empty` into the `json_format` render (§B/§C/§D)
**Files:** Modify `crates/envoy-accesslog/src/json_format.rs`. Test: inline.
- [ ] **Step 1 — failing tests.**
```rust
// §B multi-segment leaf: "pre-%REQ(X-ABSENT)%" → omit=false "pre--" ; omit=true "pre-"
// §C single-op carve-out: "%REQ(X-ABSENT)%" → null under BOTH (encode_single_op untouched)
// §D recursive: {nested:{mixed:"v=%REQ(X-ABSENT)%", single:"%REQ(X-ABSENT)%"}, arr:["a=%REQ(X-ABSENT)%","%REQ(X-ABSENT)%"]}
//   omit=true → {"arr":["a=",null],"nested":{"mixed":"v=","single":null}}   (ADR-0096 §D / CASE-4)
// default-off round-trip: omit=false renders byte-identical to the phase-39 output
```
- [ ] **Step 2 — run, verify FAIL.**
- [ ] **Step 3 — implement.** `CompiledJsonFormat` gains `omit_empty: bool` (set in `from_map`, default `false`); `render`/`render_into` thread it; `encode_json_value(out, segments, record, omit_empty)` passes it to `render_value_segments` for the multi-segment branch. `encode_single_op` is UNCHANGED (§C — a single absent op stays `null`). Confirm the dependency direction is untouched (no new dep).
- [ ] **Step 4 — run, verify PASS** + the phase-38/39 json tests green (default-off byte-unchanged).
- [ ] **Step 5 — commit.** `feat(accesslog): omit_empty in recursive json_format render (single-op null untouched) [phase40 T3]`

### Task 4: wire `omit_empty_values` from config → compiled format
**Files:** Modify `crates/envoy-http1/src/hcm.rs` (`compiled_log_format`) + `log_format.rs`/`file_sink.rs` as needed. Test: the existing `compiled_log_format_*` tests.
- [ ] **Step 1 — failing/updated test.** `compiled_log_format` sets `omit_empty` on the compiled format from `s.omit_empty_values` (both the text and json arms); assert a config with `omit_empty_values: true` produces a compiled format that renders with the swap. The default/absent path is byte-unchanged.
- [ ] **Step 2 — run, verify FAIL** (flag not threaded).
- [ ] **Step 3 — implement.** In `compiled_log_format`, read `s.omit_empty_values` and set it on the `CompiledFormat`/`CompiledJsonFormat` before wrapping in `LogFormat`. The H2 default site (`CompiledFormat::default()`, omit=false) is unchanged.
- [ ] **Step 4 — run, verify PASS** + `cargo build --workspace --all-targets`.
- [ ] **Step 5 — commit.** `feat(hcm): wire omit_empty_values into the compiled log format [phase40 T4]`

### Task 5: fixture `0048-accesslog-omit-empty` (byte-exact sentinel-swap differential)
**Files:** Create `tests/fixtures/0048-accesslog-omit-empty/*` + `tests/differential/tests/access_log_omit_empty.rs`.
- [ ] **Step 1 — failing test.** Wire the `0048` case (reuse `Driver::Http1WithAccessLog` + `AccessLogByteExactProbe`, whole-line byte-exact). The config: a `json_format` (or text) with a mixed/multi-segment absent-operator value (e.g. `mixed: "up=%UPSTREAM_HOST%"` on a `direct_response` route) + `omit_empty_values: true` → the value is `"up="` (swap) not `"up=-"`. Capture the exact line live first (run the recon config) to lock the expected bytes.
- [ ] **Step 2 — run, verify FAIL.** Rebuild `cargo build -p envoy-bin` first (the differential runs `target/debug/envoy-bin` — [[differential-harness-uses-debug-envoy-bin]]).
- [ ] **Step 3 — implement.** Author the paired configs (identical). Keep the line deterministic. A flag-off control sub-case (same config, no `omit_empty_values`) → the `-` sentinel, also byte-exact.
- [ ] **Step 4 — run, verify PASS** (cross-proxy byte-identical) + `0001`-`0047` unaffected (run new fixture in isolation; host false-REDs are CI-authoritative).
- [ ] **Step 5 — commit.** `test(differential): fixture 0048 omit_empty_values byte-exact [phase40 T5]`

### Task 6: fuzz seed + BEHAVIOR_CONTRACT
**Files:** the `parse_bootstrap` corpus + `docs/envoy-rust/BEHAVIOR_CONTRACT.md`.
- [ ] **Step 1.** Add a `omit_empty_values: true` `parse_bootstrap` seed (distinct filename + `!`-un-ignore line; verify `git ls-files`). NO new fuzz target.
- [ ] **Step 2.** Extend the BEHAVIOR_CONTRACT "Access log field mapping" with a `omit_empty_values` subsection (§A–§E: sentinel swap, NOT key-drop; both formats; single-op carve-out; recursive; no new ConfigError variant).
- [ ] **Step 3 — run the local gate:** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check`.
- [ ] **Step 4 — commit.** `docs(accesslog): BEHAVIOR_CONTRACT omit_empty_values + fuzz seed [phase40 T6]`

---

## Acceptance (the §7.5 phase-done gate, re-run at state-4)
(a) fixture `0048` green (byte-identical sentinel-swap line + the flag-off control) + (b) all `0001`-`0047` green (default-off regression witnesses) + (c) h2spec ≥95% (unchanged) + (d) `parse_bootstrap` + `accesslog_format_parse` fuzz clean (with the `omit_empty_values` seed) — NO new target + (e) build/clippy/fmt/test/deny clean + (f) `REVIEW.md` approved. `#![forbid(unsafe_code)]` holds; NO new crate/dependency; NO new `ConfigError` variant.

## Notes for the executor
- The SPEC's "drop empty KEYS" language is VOID (ADR-0096 §A) — `omit_empty_values` is a SENTINEL SWAP (`-`→`""`), not a key filter. Implement §A–§E exactly.
- `encode_single_op` MUST NOT change (§C — a single absent op stays `null`, NOT `""`).
- Default-off byte-preservation is the load-bearing regression proof — all `0001`-`0047` stay green.
- M39-1/M39-2 (phase-39 carry-forwards) are ADJACENT (the json encoder is touched); fold the M39-1 doc-pointer if the render-pass edits `JsonValueInput`'s site (cheap), else leave live.

---

_Scope locked by **ADR-0095**; §6.2 facts locked by **ADR-0096** (the recon FIRES — sentinel swap, NOT key-drop). The §6.1 split does NOT fire (**ADR-0097 reserved-but-unfired**). The state-3 implementation is the next session (`superpowers:executing-plans` / `subagent-driven-development`)._
