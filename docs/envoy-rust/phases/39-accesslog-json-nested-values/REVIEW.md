# Phase 39 — `39-accesslog-json-nested-values` — REVIEW

> **Lifecycle state 5 (code-review output).** Routed via `superpowers:requesting-code-review`;
> performed by a fresh `superpowers:code-reviewer` subagent with precisely-crafted context (the
> implementation diff + SPEC + PLAN + ADR-0094 §A–§H — NOT session history). Reviews the phase-39
> implementation (commit range `4dbde13`..`f2e4767`, diff `8270a97..f2e4767`) against `SPEC.md`,
> `PLAN.md`, and the empirically-locked ADR-0094 §A–§H wire-shape facts.

## Verdict: **APPROVE** — 0 Critical / 0 Important / 2 new Minor (M39-1, M39-2; both carry-forward, non-blocking)

The implementation faithfully realizes ADR-0094 §A–§H and the 7-task PLAN. The reviewer independently
verified the full diff, ran `cargo test -p envoy-accesslog -p envoy-config -p envoy-http1` (72 / 531 / 132
green), `cargo clippy` on all three crates (clean), and confirmed `Cargo.lock`/`Cargo.toml` byte-unchanged
(no new dependency). High-quality, doctrine-compliant work.

## Doctrine compliance (all verified GREEN)
- **No new dependency / crate / fuzz target** — zero changes to `Cargo.lock` + the three `Cargo.toml`; the
  fuzz seed reuses the EXISTING `parse_bootstrap` target (`crates/envoy-config/fuzz/corpus/parse_bootstrap/
  json_format_nested.yaml`, tracked, un-ignored at `fuzz/.gitignore:50`).
- **`#![forbid(unsafe_code)]`** present in all three crate roots; no `unsafe` introduced.
- **No new `ConfigError` variant** — the recursive validator reuses `InvalidAccessLogFormat`
  (`bootstrap.rs:4435-4458`).
- **Crate dependency direction preserved** — `crates/envoy-accesslog/Cargo.toml` has NO `envoy-config` dep;
  the bridge uses the accesslog-side mirror `JsonValueInput` (`json_format.rs:11-25`) with the
  `JsonFormatValue`→`JsonValueInput` map caller-side in `hcm.rs:1289-1311`. No cycle.
- **Leaf helpers genuinely reused, not duplicated** — single definitions of `encode_json_value`/
  `encode_single_op`/`json_escape_into`/`quote`/`quote_opt` (`json_format.rs:147-242`); the recursive
  `Leaf` arm calls `encode_json_value` verbatim.

## §A–§H wire-shape conformance (all verified, each with a dedicated unit test)
- **§A** per-level key sorting (`BTreeMap` each level) — `per_level_keys_sorted_independently`.
- **§B** list = config order (`Vec`, not sorted) — `list_order_preserved_not_sorted`.
- **§C** at-depth type inference reuses `encode_json_value` — `depth_three_nesting` + the §H test.
- **§D** `bool`/`null` native-typed (`bool_and_null_literal_leaves_native_typed`); numeric literal
  BOOT-REJECTED via the untagged arm-order (no `Number` arm) — `numeric_literal_leaf_is_rejected`
  (`42` and `1.5` both `is_err()`).
- **§E** compact separators + exactly one top-level `\n` (only `CompiledJsonFormat::render` appends `"}\n"`).
- **§F** empty `{}`/`[]` + in-list absent→`null` — `empty_nested_object_and_list`,
  `absent_operator_leaf_in_list_is_null`.
- **§G** nested malformed op → `InvalidAccessLogFormat`
  (`malformed_operator_in_nested_object_leaf_is_invalid_format` + `_nested_list_leaf_`); exactly-one-of +
  empty-top-map untouched.
- **§H** the byte-exact line — fixture `0047` paired configs identical; `renders_authoritative_nested_
  fixture_line` asserts exactly `{"arequest":{"aaa":200,"method":"GET","zpath":"/"},"blist":["GET",200,
  null],"mtop":"code-200","zouter":"HTTP/1.1"}\n`.

## Claimed folds (verified accurate)
- **M38-3** (empty value-string `""`) — `empty_value_string_leaf_renders_empty_quoted` (flat + nested). Genuine.
- **M38-4** (`:N`/`?ALT`/`%DURATION%`/control-char in the nested path) — `m38_4_typed_path_gaps_in_nested_
  position` exercises all four inside a nested object. Genuine.
- **M38-1** ("folded-equivalent") — accurate, not a dodge: `encode_single_op` and the text `render_op`
  both call the shared `command_operator::{resolve_req,resolve_resp,truncate_bytes}`; the resolve+truncate
  chain is already factored, so no further extraction was needed.

## Test quality (strong)
Fixture `0047` asserts the §H byte-exact line cross-proxy via the auto-discovered
`tests/differential/tests/access_log_json_nested.rs` (same pattern as phase-38's `access_log_json_format.rs`).
Fixture `0046` (flat) preserved untouched + the `flat_round_trip_byte_unchanged_through_recursive_encoder`
regression witness re-asserts the phase-38 flat line byte-identical through the recursive encoder. The HCM
bridge has flat + nested end-to-end tests.

## Findings

**Critical:** none.

**Important:** none.

**Minor (new carry-forwards; NONE blocks):**
- **M39-1** (mirror-enum sync) — `JsonFormatValue` (`envoy-config`) and `JsonValueInput` (`envoy-accesslog`)
  are structurally-identical mirror enums whose 1:1 map is hand-maintained in
  `hcm.rs::json_format_value_to_input`. This is the CORRECT way to avoid the dep cycle (no fix wanted now),
  but the three-way duplication (config enum / accesslog mirror / manual bridge) is a future-maintenance
  trap: adding a variant requires editing all three with no compiler-enforced cross-crate totality. A
  one-line `// keep in sync with envoy-config::JsonFormatValue` doc-pointer on `JsonValueInput` would
  harden it. Maintainability-only; fold into the next phase that touches the `json_format` config/encoder.
- **M39-2** (unbounded recursion depth) — `CompiledJsonValue::compile` + `render_into` recurse with no
  depth cap. Config is operator-authored + boot-time-only (NOT attacker-controlled at runtime), so a
  pathologically deep `json_format` could stack-overflow at BOOT (boot-fatal-by-crash, not a security
  issue; matches upstream Envoy's own protobuf recursion behavior). Acceptable for the MVP. Note it as a
  known edge IF a future phase ever accepts `json_format` from a DYNAMIC (xDS/RDS) source. Carry-forward.

## Strengths
- ADR-0094 §A–§H reproduced verbatim in code comments, `expectations.yaml`, and `BEHAVIOR_CONTRACT.md` —
  exceptional traceability from empirical recon → implementation → test.
- CF-39-1 (numeric-literal deferral) realized as a TYPE-LEVEL rejection (no `Number` untagged arm) rather
  than a runtime check, and positively tested on both int + float.
- The dep-direction constraint handled exactly as the PLAN prescribed (accesslog-side mirror + caller-side
  bridge), rationale documented at all three sites.
- Per-task TDD discipline visible in commit pairing; every §A–§H fact has a dedicated named unit test.

---

_Reviewed at state-5. **APPROVE** (0 Critical / 0 Important / 2 new Minor M39-1/M39-2 carried forward,
non-blocking). The §7.5 (a)-(e) gate was GREEN at state-4 (authoritative CI `28295620744` @ `fffc297`
`completed/success`). With (f) `REVIEW.md` APPROVE, the full §7.5 (a)-(f) gate is COMPLETE → the next
session is the state-6 phase-close (flip ROADMAP row `39` → `done`, advance STATE to awaiting-next-planning)._
