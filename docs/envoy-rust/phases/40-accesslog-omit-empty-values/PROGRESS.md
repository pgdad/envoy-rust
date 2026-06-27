# Phase 40 — `40-accesslog-omit-empty-values` — Implementation Progress

State-3 (implementation) execution log. TDD per task (failing test → run-fails →
minimal impl → run-passes → commit). Scope locked by ADR-0095; §6.2 facts locked
by ADR-0096 (the sentinel SWAP, NOT key-drop). All six PLAN tasks T1–T6
complete; local gate GREEN (one differential Docker flake, host-only,
CI-authoritative).

## ADR-0096 §A–§E — the corrected understanding (implemented against)

- **§A** `omit_empty_values` does NOT drop keys/entries — every key always emits.
- **§B** it SWAPS the absent-operator `-` sentinel for `""` in the
  command-operator MULTI-SEGMENT render (`render_value_segments`→`render_op`,
  the four `.unwrap_or("-")` sites), for BOTH `text_format` AND `json_format`.
- **§C** single-operator-TYPED `json_format` values are UNAFFECTED
  (`encode_single_op`: absent→`null`, UNCHANGED).
- **§D** the swap applies RECURSIVELY (nested objects + lists); single-op nulls
  at depth stay `null`.
- **§E** all-single-absent → keys survive as `null`; `omit_empty_values` is a
  plain `bool`; NO new `ConfigError` variant.

---

## Task-by-task evidence

### T1 — `omit_empty_values` config field — DONE (`86971ce`)
- **Failing test** `bootstrap::tests::omit_empty_values_round_trips_and_defaults_false`
  (serde round-trip + default-false). Verified FAIL: `no field omit_empty_values
  on type SubstitutionFormatString`.
- **Impl** added `#[serde(default)] pub omit_empty_values: bool` to
  `SubstitutionFormatString` (`bootstrap.rs:704`). `deny_unknown_fields` retained;
  exactly-one-of validator UNCHANGED. Updated the 6 downstream struct literals in
  `envoy-http1`/`envoy-http2` HCM tests (`omit_empty_values: false`).
- **PASS** `test bootstrap::tests::omit_empty_values_round_trips_and_defaults_false ... ok`
  (`cargo test -p envoy-config omit_empty`: 1 passed).

### T2 — thread `omit_empty` into `render_value_segments` + text `CompiledFormat` (§B) — DONE (`2c877f3`)
- **Failing test** `command_operator::tests::omit_empty_swaps_dash_for_empty_in_multi_segment`
  — `up=%UPSTREAM_HOST% x=%REQ(X-FORWARDED-FOR)%` on a no-upstream record:
  `omit=false → "up=- x=-"`, `omit=true → "up= x="`. Verified FAIL (arity).
- **Impl** threaded `omit_empty: bool` through `render_value_segments` →
  `render_op` (`let empty_or_dash = if omit_empty {""} else {"-"};`; the four
  `.unwrap_or(...)` sites at UpstreamHost/DynamicMetadata/Req/Resp).
  `CompiledFormat` became a named-field struct `{ segments, omit_empty }` with
  `new(...)` (omit=false), `with_omit_empty(bool)`, `render` passing
  `self.omit_empty`. **M40-B:** the json caller `encode_json_value`
  (`json_format.rs:174`) passes a literal `false` placeholder so `envoy-accesslog`
  stays green at end of T2 (T3 replaces it).
- **PASS** new test + all `command_operator` tests green (default-off byte-unchanged).
  `cargo test -p envoy-accesslog`: 73 passed.

### T3 — thread `omit_empty` into the `json_format` render (§B/§C/§D) — DONE (`0e4a2ea`)
- **Failing tests** (4): `omit_empty_swaps_dash_in_multi_segment_json_leaf` (§B:
  `"pre-%REQ(X-FORWARDED-FOR)%"` → `"pre--"`/`"pre-"`),
  `omit_empty_leaves_single_op_null_unchanged` (§C: single absent op → `null`
  under BOTH), `omit_empty_applies_recursively_single_op_null_at_depth` (§D /
  CASE-4: `{"arr":["a=",null],"nested":{"mixed":"v=","single":null}}`),
  `omit_empty_default_off_round_trip_byte_unchanged`. Verified FAIL
  (`no method with_omit_empty`).
- **Impl** `CompiledJsonFormat` became `{ map, omit_empty }` with
  `with_omit_empty(bool)`; `render`/`render_into`/`encode_json_value` thread
  `omit_empty` recursively. **`encode_single_op` UNCHANGED (§C carve-out).**
  No new dependency.
- **PASS** 4 new tests + the phase-38/39 json tests green (default-off
  byte-unchanged). `cargo test -p envoy-accesslog`: 77 passed.

### T4 — wire `omit_empty_values` config → compiled format — DONE (`a68787e`)
- **Failing tests** `compiled_log_format_threads_omit_empty_values_text` (text:
  `up=%UPSTREAM_HOST%` → `"up=-"`/`"up="`),
  `compiled_log_format_threads_omit_empty_values_json` (json: mixed leaf swapped,
  single-op leaf stays `null`). Verified FAIL (`left "up=-" != right "up="`).
- **Impl** `compiled_log_format` reads `s.omit_empty_values` and calls
  `.with_omit_empty(...)` on both the `CompiledFormat` (text arm) and
  `CompiledJsonFormat` (json arm). The default/absent arms unchanged.
- **PASS** `cargo test -p envoy-http1 compiled_log_format`: 7 passed +
  `cargo build --workspace --all-targets` clean.

### T5 — fixture `0048-accesslog-omit-empty` (byte-exact sentinel-swap differential) — DONE (`5f5a80b`)
- **Live byte-capture FIRST.** Stood up `envoyproxy/envoy:v1.33.0` (Docker) with
  the recon `json_format` + `omit_empty_values: true` on a `direct_response`
  route; scraped the access-log line:
  ```
  {"method":"GET","proto":"HTTP/1.1","single_up":null,"up":"up=","xff":"x="}
  ```
  Confirms §B (`up=`/`x=` swapped) + §C (`single_up` single absent op stays
  `null`). Flag-off control (same map, no flag) live-captured:
  ```
  {"method":"GET","proto":"HTTP/1.1","single_up":null,"up":"up=-","xff":"x=-"}
  ```
  envoy-rust (`target/debug/envoy-bin`) emitted the omit-on line BYTE-IDENTICAL.
- **Impl** `tests/fixtures/0048-accesslog-omit-empty/` (envoy.yaml +
  envoy-rust.yaml + expectations.yaml + README.md, mirroring 0047) +
  `tests/differential/tests/access_log_omit_empty.rs`. Rebuilt
  `cargo build -p envoy-bin` before the differential (debug binary).
- **PASS (in isolation, `--test-threads=1`)**
  `test access_log_omit_empty ... ok` (1 passed). 0047 + 0046 re-run in
  isolation: both `ok` (1 passed each) — default-off witnesses unaffected.

### T6 — fuzz seed + BEHAVIOR_CONTRACT + local gate — DONE (this commit)
- **Fuzz seed** `crates/envoy-config/fuzz/corpus/parse_bootstrap/omit_empty_values.yaml`
  (distinct filename, `omit_empty_values: true`) + a `!`-un-ignore line in
  `crates/envoy-config/fuzz/.gitignore`. Verified tracked:
  `git ls-files …/omit_empty_values.yaml` → tracked. NO new fuzz target.
  Seed boots clean via `envoy-bin`.
- **BEHAVIOR_CONTRACT** added a "Phase 40 (ADR-0096): `omit_empty_values` — the
  absent-operator sentinel swap" subsection (§A–§E + the live fixture-0048 line +
  the flag-off control).

---

## Local gate (state-3 close)

| step | result |
|------|--------|
| `cargo build --workspace --all-targets` | `Finished` (clean) |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | `Finished` (no warnings) |
| `cargo fmt --all -- --check` | exit 0 (clean, after one auto-format of the T4 test) |
| `cargo test --workspace` | ALL suites pass EXCEPT one differential Docker flake (below) |
| `cargo deny check` | `advisories ok, bans ok, licenses ok, sources ok` |

**The one workspace test failure** is `differential::admin_config_dump_server_info`
— the documented `192.168.65.2` host-bridge-IP differential flake (this dev host
routes the backend via `192.168.65.2`, NOT the allow-listed
`192.168.65.254`/`172.17.0.1`). It is unrelated to phase 40 (a cluster
config-dump test that touches NO access-log code), host-only, and
CI-authoritative. The phase-40 fixtures (0048/0047/0046) all PASS in isolation.

## Invariants held

- `#![forbid(unsafe_code)]` holds (both touched crate roots).
- NO new crate, NO new dependency (Cargo.toml/Cargo.lock unchanged — `cargo deny`
  green), NO new fuzz target, NO new `ConfigError` variant.
- §C carve-out: `encode_single_op` UNCHANGED — a single absent op stays `null`.
- Default-off byte-preservation: fixtures `0001`-`0047` unaffected (0047/0046
  re-verified byte-exact in isolation; the phase-38/39 json + text in-process
  tests stay green).

## Carry-forwards

- **M39-1 / M39-2** (phase-39): NOT folded — the render-pass threading did not
  edit the `JsonValueInput` mirror-enum site (M39-1) and did not touch the
  recursion-depth bound (M39-2); both stay LIVE (cheap-fold criterion not met).
- **CF-39-1** (numeric literal leaves): unchanged, stays deferred.
- **M38-2/M38-1** + the RBAC/older Minors: stay live (untouched surface).

## Deviation note (differential control sub-case)

The `http1_access_log_byte_exact` driver scrapes ONE (envoy, envoy-rust) path
pair and the flag is global per `log_format`, so a single fixture cannot carry
both an omit-on AND a flag-off line in one comparable file. The omit-on swap is
the 0048 cross-proxy witness; the flag-off control (the `-` sentinel) is the
byte-exact witness of fixture 0047 (same recursive-`json_format` shape, no flag)
plus all `0001`-`0047`, and the live-captured control bytes are recorded in the
0048 README + expectations.yaml + the BEHAVIOR_CONTRACT. The text-format swap,
the recursive (§D) swap, and the §C single-op carve-out are additionally proven
by the in-process `envoy-accesslog` backstop (T2/T3).
