# Phase 33 — `set_metadata` + dynamic-metadata store + `%DYNAMIC_METADATA%` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
> Every task is TDD: write the failing test FIRST, watch it fail, implement minimally, watch it pass, commit.

**Goal:** Land the smallest end-to-end, differentially-testable dynamic-metadata loop — a per-request dynamic-metadata
store, the `envoy.filters.http.set_metadata` HTTP filter (a static-value metadata emitter), and the
`%DYNAMIC_METADATA(namespace:key)%` access-log command-operator — proven by a byte-exact cross-proxy access-log line
(fixture `0041`), with all 40 pre-existing fixtures unchanged.

**Architecture:** A string-only `BTreeMap<String, BTreeMap<String, String>>` (namespace→key→value) is added as an
additive default-empty field on BOTH `envoy_filter::FilterRequest` and `envoy_accesslog::AccessLogRecord` (a plain std
type, NO new crate, NO shared `Value` enum). The `set_metadata` filter (the 11th `HttpFilterInstance` variant,
decode-side, `Continue`-only) merges its config-static value into `req.dynamic_metadata`. The two HCMs
(`envoy-http1`, `envoy-http2`) — the SOLE copy sites that already depend on both leaf crates — capture
`filter_req.dynamic_metadata` before `filter_req` is dropped and populate their (independent) `AccessLogRecord`
builds. A new `Op::DynamicMetadata { namespace, key }` in the phase-32 command-operator engine resolves
`record.dynamic_metadata.get(ns)?.get(key)`, absent → `-`.

**Tech Stack:** Rust (pinned toolchain, `#![forbid(unsafe_code)]` workspace-wide), `serde`/`serde_yaml`
(`deny_unknown_fields`), `thiserror`, the phase-32 `envoy-accesslog` command-operator engine, the phase-07 filter
framework, `tests/differential` (`Driver::Http1AccessLogByteExact` + `assert_access_log_lines_byte_identical`),
`cargo fuzz` (existing `accesslog_format_parse` + `parse_bootstrap` targets — NO new target).

---

## §A — Empirically-locked facts (the §6.2 ground truth; reconciled by ADR-0081)

Run LOCALLY at this PLAN-write against live `envoyproxy/envoy:v1.33.0` (build `b0f43d67…/1.33.0/RELEASE/BoringSSL`):
an H1 listener → `direct_response` 200, filter chain `[set_metadata, router]`, a file access-logger whose
`log_format.text_format_source.inline_string` carried `%DYNAMIC_METADATA(...)%` operators; ≥4 probes; bytes captured
with `od -c`. **ADR-0081 FIRES** — two MATERIAL divergences from the SPEC projection (the `@type` name and the
`:N`-truncation disposition), plus confirmations and three refinements. These facts are LOCKED; the tasks below
encode them. Do NOT re-derive.

**A1 — `set_metadata` config wire shape (MATERIAL DIVERGENCE on the `@type`).**
- The proto message is named **`Config`**, NOT `SetMetadata`. The SPEC's projected
  `@type = …filters.http.set_metadata.v3.SetMetadata` **DOES NOT EXIST** (Envoy: `could not find @type …SetMetadata`,
  boot-fatal). The correct URL is
  **`type.googleapis.com/envoy.extensions.filters.http.set_metadata.v3.Config`**.
- The modern wire form is the repeated `metadata: [{ metadata_namespace, value, allow_overwrite }]`, which boots
  CLEAN (zero warnings). The deprecated top-level form (`metadata_namespace` + `value` at `Config` top level) boots
  with deprecation WARNINGS. There is **no `{ key, value }` form** — `key` is rejected boot-fatal (`no such field`).
- The written value lands under the `metadata_namespace` string VERBATIM (confirmed via `/config_dump` and via a live
  `%DYNAMIC_METADATA(envoy.test:tier)%` read-back of `prod`).
- **Phase-33 models ONLY the modern repeated form** with `@type …v3.Config`. The deprecated top-level form is
  §2.2-DEFERRED ("non-chosen wire path"); `deny_unknown_fields` rejects its top-level `metadata_namespace`/`value`
  (envoy-rust is stricter — boot-fatal vs Envoy's warn — documented, not differentially exercised).

**A2 — `%DYNAMIC_METADATA(ARG)%` arg grammar (MATERIAL DIVERGENCE on `:N`).**
- The path separator is **`:`** (`%DYNAMIC_METADATA(envoy.test:tier)%` → `prod`). CONFIRMED.
- **`%DYNAMIC_METADATA(...):N%` (a `:N` length suffix) is BOOT-FATAL** in Envoy
  (`DYNAMIC_METADATA does not allow length to be specified.`, exit 1). The SPEC projected ":N composes
  unconditionally" with an `Op::DynamicMetadata { …, truncate: Option<usize> }` field — **WRONG**. The operator carries
  **NO `truncate` field**, and the parser MUST reject a trailing `:N` on this operator (boot-fatal).
- No-arg `%DYNAMIC_METADATA%` (no `(…)`) is BOOT-FATAL (`DYNAMIC_METADATA requires parameters`).
- Deeper nested paths (`envoy.test:nested:sub`) ARE accepted by Envoy (struct traversal). **The string-only MVP
  models ONLY the single-level two-segment `namespace:key`**; a 1-segment (whole-namespace) or 3+-segment (nested)
  arg → config-load-fatal in envoy-rust (the §2.2 nested-path deferral; stricter than Envoy; documented; NOT
  differentially exercised — the fixture uses only `ns:key`).

**A3 — Resolved-value byte form (THE key differential risk — CONFIRMED for scalars).**
- A scalar STRING leaf value (`prod`) renders **RAW, UNQUOTED `prod`** (`od -c` → `[ p r o d ]`, NOT `[ " p r o d " ]`).
  CONFIRMED — matches the SPEC projection. (Scalar number `7` → `7`; bool `true` → `true`.)
- **Refinement (out of MVP scope):** a STRUCT/object value renders as JSON WITH literal quotes
  (`%DYNAMIC_METADATA(envoy.test:nested)%` → `{"sub":"deepval"}`; whole-namespace → a sorted JSON object). The
  string-only single-level-leaf MVP never reaches this — it resolves only scalar-string leaves → raw unquoted. The
  JSON-composite rendering is the §2.2 non-string-Value deferral.

**A4 — Absent rendering (CONFIRMED).** Absent KEY (`%DYNAMIC_METADATA(envoy.test:missing)%`) and absent NAMESPACE
(`%DYNAMIC_METADATA(envoy.absent:k)%`) BOTH render a single dash **`-`** (`od -c` → `[ - ]`; never empty, never `{}`,
never `null`). Matches the SPEC projection and the existing engine's absent-sentinel.

**A5 — Config-validity disposition (CONFIRMED boot-fatal, three refinements).** Envoy is boot-fatal on
structurally-invalid config (bad `@type`, empty `metadata_namespace` — PGV `value length must be at least 1`,
unknown field, malformed operator, `:N` on DYNAMIC_METADATA, no-arg DYNAMIC_METADATA) — consistent with ADR-0049
"all config errors fatal at startup". Refinements: (a) a non-string SCALAR value (number/bool) is ACCEPTED by Envoy
(not fatal) — but the string-only MVP models `value` as `BTreeMap<String,String>`, so a non-string YAML scalar fails
serde deserialization → boot-fatal in envoy-rust (the §2.2 non-string-Value deferral boundary; documented; the fixture
uses string values only); (b) the `:N`-on-DYNAMIC_METADATA rejection is boot-fatal (A2); (c) the deprecated top-level
form is warn-not-fatal in Envoy but boot-fatal in envoy-rust (A1).

**A6 — Determinism (STRONG target FIRES).** 4 probes (`/a /b /c /d`) → exactly one unique access-log line (stable
md5, clean `$` line-ends). The `%DYNAMIC_METADATA%` render of a static-config value is a pure function of static config
(no host-address/clock terms), so both proxies emit a byte-identical line. The cross-proxy whole-line byte-exact
differential (`Driver::Http1AccessLogByteExact` + `assert_access_log_lines_byte_identical`, reused verbatim from
phase 32) is the STRONG target — no fallback needed.

**Disposition of the SPEC §3 open PLAN-write calls (now LOCKED):** §3.1 wire shape → `@type …v3.Config`, modern
repeated `metadata: [{metadata_namespace, value, allow_overwrite}]` form only (A1); §3.2 grammar → `:`-separator,
single-level two-segment only, NO `:N`, no-arg fatal (A2); §3.3 byte form → raw unquoted scalar string (A3); §3.4
config-validity → boot-fatal, ONE new `ConfigError::SetMetadataEmptyNamespace` + reuse `UnsupportedHttpFilter`
(name) + reuse `InvalidAccessLogFormat` (operator), serde rejects non-string value (A5); §3.5 store →
`BTreeMap<String, BTreeMap<String, String>>` declared independently on `FilterRequest` + `AccessLogRecord` (no shared
crate; the HCMs are the sole copy site — §4 reuse map); §3.6 fixture-0041 → `direct_response`, deterministic
`log_format` with `%DYNAMIC_METADATA%` present + absent probes (≥2), no timing operator (A6); §3.7 harness →
`Driver::Http1AccessLogByteExact` + `assert_access_log_lines_byte_identical` reused verbatim; §3.8 fuzz → existing
`accesslog_format_parse` (operator grammar) + `parse_bootstrap` (config) targets cover it, NO new target — add seeds;
§3.9 split → NOT fired (~12 tasks / ~900–1200 LoC; ADR-0082 reserved-but-UNFIRED).

---

## File structure (what each file is responsible for)

**New files:**
- `crates/envoy-filter/src/set_metadata.rs` — the `SetMetadataFilter` (struct + `new` + `decode_headers` merge +
  inert `encode_headers` + unit tests). The 11th filter, following the `cdn_loop.rs` add-a-decode-side-filter pattern.
- `tests/fixtures/0041-http-set-metadata-dynamic-metadata/{envoy.yaml,envoy-rust.yaml,expectations.yaml,README.md}` —
  the differential fixture (H1 listener, `[set_metadata, router]` chain, `%DYNAMIC_METADATA%`-bearing `log_format`,
  present-key + absent-key probes).
- `tests/differential/tests/set_metadata_dynamic_metadata.rs` — the Docker-gated differential test (mirrors
  `access_log_command_operators.rs`).

**Modified files (one responsibility each):**
- `crates/envoy-filter/src/types.rs` — add the additive `dynamic_metadata` field to `FilterRequest`.
- `crates/envoy-accesslog/src/record.rs` — add the additive `dynamic_metadata` field to `AccessLogRecord`.
- `crates/envoy-accesslog/src/command_operator.rs` — add `Op::DynamicMetadata`; the parse arm; the render arm; the
  M32-1 `enum Side`, M32-3 named-field diagnostics, M32-6 pre-alloc, M32-2 parser-strictness folds.
- `crates/envoy-config/src/bootstrap.rs` — `SetMetadataConfig` + `MetadataEntry` structs; the
  `HttpFilterTypedConfig::SetMetadata` variant; the `validate_http_filters` arm.
- `crates/envoy-config/src/lib.rs` — the `ConfigError::SetMetadataEmptyNamespace` variant + re-export.
- `crates/envoy-filter/src/instance.rs` — the `HttpFilterInstance::SetMetadata` variant + build/decode/encode/
  apply_route wirings.
- `crates/envoy-http1/src/hcm.rs` — capture `filter_req.dynamic_metadata` (~786) → the H1 record build (~1189);
  H1 in-process backstop.
- `crates/envoy-http2/src/hcm.rs` — capture `filter_req.dynamic_metadata` (~488) → the H2 record build (~888);
  H2 in-process backstop.
- `crates/envoy-accesslog/src/default_format.rs` — M32-4 (loop the default-equivalence oracle over multiple records).
- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — the "Access log field mapping" extension (the new operator + namespace/key
  resolution + raw-scalar byte form + absent `-`).
- `crates/envoy-accesslog/fuzz/corpus/accesslog_format_parse/` + `crates/envoy-config/fuzz/corpus/parse_bootstrap/` —
  new seed inputs.
- Plus the mechanical construction-site sweeps (33 `FilterRequest`, 13 `AccessLogRecord` literals — compiler-driven).

---

## Task 1: M32 command-operator refactors (Side enum + named diagnostics + pre-alloc + empty-alt strictness)

> Folds phase-32 carry-forwards M32-1 (`side: &'static str` → `enum Side`), M32-3 (`MalformedArgument` →
> named fields), M32-6 (`render` pre-alloc), M32-2 (empty-`?`-alt parser strictness). Pure refactor + one new strict
> rejection; the new `Op::DynamicMetadata` (Task 8) is the clean moment for `enum Side`. NO behavior change to any
> existing valid format. Run FIRST so Task 8 builds on the cleaned types.

**Files:**
- Modify: `crates/envoy-accesslog/src/command_operator.rs`

- [ ] **Step 1: Write failing tests** in the `command_operator.rs` test module:
  - `empty_alternate_is_error`: `parse_format("%REQ(:PATH?)%")` (a `?` with an empty alternate) → `Err`
    (`FormatParseError::MalformedArgument { .. }`). (M32-2: an empty alternate is malformed, not `alt: Some("")`.)
  - `truncate_zero_is_valid_and_empty`: `parse_format("%REQ(USER-AGENT):0%")` parses OK and renders `""` for a present
    value (`:0` = floor_char_boundary(0) = empty; total, no panic). (M32-2: pins `:0` semantics explicitly.)
- [ ] **Step 2: Run to verify they fail.** Run: `cargo test -p envoy-accesslog command_operator`
  Expected: the two new tests FAIL (empty alt currently parses to `alt: Some("")`; `:0` untested).
- [ ] **Step 3: Refactor + implement minimally.**
  - **M32-1 + M32-3:** replace `enum FormatParseError { … MalformedArgument(String, String), … UnsupportedHeader {
    side: &'static str, … } }` with a `#[derive(Debug, Clone, Copy, PartialEq)] pub enum Side { Req, Resp }` (with an
    `as_str(self) -> &'static str` returning `"REQ"`/`"RESP"`), `MalformedArgument { keyword: String, detail: String }`
    (named), and `UnsupportedHeader { side: Side, name: String, supported: String }`. Update `#[error(...)]` format
    strings to the named fields (use `{side}` via a `Display` for `Side` or `self.side.as_str()`). Thread `Side`
    through `parse_header_op(keyword, rest, side: Side)` and the two call sites (`"REQ"`/`"RESP"` → `Side::Req`/
    `Side::Resp`); update every `MalformedArgument(a, b)` construction to `MalformedArgument { keyword: a, detail: b }`.
  - **M32-2:** in `parse_header_op`, after `arg.split_once('?')`, reject an EMPTY alternate:
    `Some((n, a)) if a.is_empty() => return Err(MalformedArgument { keyword: keyword.into(), detail: "empty alternate
    after '?'".into() })`. Keep `:0` valid (`truncate_bytes` already totals via `floor_char_boundary`).
  - **M32-6:** give `CompiledFormat` a precomputed `literal_len: usize` (sum of `Segment::Literal` byte lengths,
    computed in `from_inline`/`Default`/the tuple path via a small helper) and change `render` to
    `String::with_capacity(self.literal_len + 64)` (literal bytes + a small operator allowance) instead of the fixed
    `256`. (If the private tuple `CompiledFormat(Vec<Segment>)` shape is load-bearing for tests, keep the tuple and
    compute `literal_len` on the fly at the top of `render` — either is acceptable; prefer the precompute.)
- [ ] **Step 4: Run to verify pass + no regression.** Run: `cargo test -p envoy-accesslog`
  Expected: ALL pass (the two new + every pre-existing command_operator/default_format/file_sink test).
  Run: `cargo build -p envoy-config` (the only out-of-crate consumer of `FormatParseError` via `parse_format` in
  `validate_access_logs` uses `e.to_string()` — unaffected by the field rename). Expected: clean.
- [ ] **Step 5: Commit.**
  ```bash
  git add crates/envoy-accesslog/src/command_operator.rs
  git commit -m "phase 33 T1: M32 command-operator folds (Side enum, named diagnostics, pre-alloc, empty-alt strictness)"
  ```

---

## Task 2: Add `dynamic_metadata` to `FilterRequest` (+ compiler-driven 33-site sweep)

**Files:**
- Modify: `crates/envoy-filter/src/types.rs` (the field)
- Modify (compiler-driven): the 33 `FilterRequest { … }` literal sites (2 production: `crates/envoy-http1/src/hcm.rs`,
  `crates/envoy-http2/src/hcm.rs`; 31 test sites across `crates/envoy-filter/src/{instance,pipeline,rbac,
  local_rate_limit,jwt_authn,header_mutation,fault,csrf,cors,cdn_loop,buffer,router,types}.rs`).

- [ ] **Step 1: Write the failing test** in `crates/envoy-filter/src/types.rs` test module:
  ```rust
  #[test]
  fn filter_request_dynamic_metadata_defaults_empty_and_is_writable() {
      use std::collections::BTreeMap;
      let mut r = FilterRequest {
          method: "GET".into(), path: "/".into(), headers: vec![], body: None,
          dynamic_metadata: BTreeMap::new(),
      };
      assert!(r.dynamic_metadata.is_empty());
      r.dynamic_metadata.entry("ns".into()).or_default().insert("k".into(), "v".into());
      assert_eq!(r.dynamic_metadata["ns"]["k"], "v");
  }
  ```
- [ ] **Step 2: Run to verify it fails (does not compile).** Run: `cargo test -p envoy-filter types::`
  Expected: FAIL — `FilterRequest` has no field `dynamic_metadata`.
- [ ] **Step 3: Add the field + sweep all sites.** In `types.rs`, add to `FilterRequest`:
  ```rust
  /// Per-request dynamic-metadata store (namespace → key → string value),
  /// written by `envoy.filters.http.set_metadata` (phase 33) and read by the
  /// HCM record-build into `AccessLogRecord.dynamic_metadata`. Default-empty;
  /// string-only (a non-string Value enum is the §2.2 deferral). A plain
  /// `std::collections::BTreeMap` — NO new crate, NO shared Value type.
  pub dynamic_metadata: std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
  ```
  Then build the workspace and add `dynamic_metadata: std::collections::BTreeMap::new()` (or `Default::default()`) to
  every flagged `FilterRequest { … }` literal. Use the compiler as the worklist:
  `cargo build -p envoy-filter --all-targets 2>&1 | grep "missing field"` and repeat until clean; then the two HCMs.
- [ ] **Step 4: Run to verify pass.** Run: `cargo test -p envoy-filter` then
  `cargo build -p envoy-http1 -p envoy-http2 --all-targets`. Expected: clean (behavior unchanged — empty default).
- [ ] **Step 5: Commit.**
  ```bash
  git add -A
  git commit -m "phase 33 T2: FilterRequest.dynamic_metadata additive field + construction-site sweep"
  ```

---

## Task 3: Add `dynamic_metadata` to `AccessLogRecord` (+ compiler-driven 13-site sweep)

**Files:**
- Modify: `crates/envoy-accesslog/src/record.rs` (the field)
- Modify (compiler-driven): the 13 `AccessLogRecord { … }` literal sites (2 production: `crates/envoy-http1/src/hcm.rs`,
  `crates/envoy-http2/src/hcm.rs`; 11 in-crate: `record.rs`, `command_operator.rs` `rec()`, `default_format.rs`
  `make_baseline_record()`, `file_sink.rs` tests).

- [ ] **Step 1: Write the failing test** in `record.rs` test module: extend `record_construction_full` (or add a new
  test) to set `dynamic_metadata: BTreeMap::new()` and assert it is empty; assert a populated record carries the value.
- [ ] **Step 2: Run to verify it fails (does not compile).** Run: `cargo test -p envoy-accesslog record::`
  Expected: FAIL — no field `dynamic_metadata`.
- [ ] **Step 3: Add the field + sweep.** In `record.rs`, add to `AccessLogRecord`:
  ```rust
  /// Per-request dynamic metadata (namespace → key → string value), copied
  /// from the pipeline's `FilterRequest.dynamic_metadata` at the HCM
  /// record-build site (H1 hcm.rs ~1189, H2 hcm.rs ~888). Rendered by the
  /// `%DYNAMIC_METADATA(namespace:key)%` command-operator (phase 33).
  pub dynamic_metadata: std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
  ```
  Sweep all 13 literals via the compiler worklist (`grep "missing field"`), adding `dynamic_metadata:
  std::collections::BTreeMap::new()`.
- [ ] **Step 4: Run to verify pass.** Run: `cargo test -p envoy-accesslog` then
  `cargo build -p envoy-http1 -p envoy-http2 --all-targets`. Expected: clean (all unchanged — empty default; `0012`
  default-format byte-identical because no operator reads the new field yet).
- [ ] **Step 5: Commit.**
  ```bash
  git add -A
  git commit -m "phase 33 T3: AccessLogRecord.dynamic_metadata additive field + construction-site sweep"
  ```

---

## Task 4: `SetMetadataConfig` schema + `HttpFilterTypedConfig::SetMetadata` variant

> §A1: `@type …v3.Config`, modern repeated `metadata: [{ metadata_namespace, value, allow_overwrite }]` form,
> string-only `value`.

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (the structs + the enum variant)
- Modify: `crates/envoy-config/src/lib.rs` (re-export `SetMetadataConfig` in the `pub use bootstrap::{…}` list)

- [ ] **Step 1: Write the failing test** in `bootstrap.rs` test module:
  ```rust
  #[test]
  fn parses_set_metadata_filter_modern_form() {
      let yaml = r#"
  name: envoy.filters.http.set_metadata
  typed_config:
    "@type": type.googleapis.com/envoy.extensions.filters.http.set_metadata.v3.Config
    metadata:
    - metadata_namespace: envoy.test
      value:
        tier: prod
  "#;
      let hf: HttpFilter = serde_yaml::from_str(yaml).expect("parses");
      match hf.typed_config {
          HttpFilterTypedConfig::SetMetadata(cfg) => {
              assert_eq!(cfg.metadata.len(), 1);
              assert_eq!(cfg.metadata[0].metadata_namespace, "envoy.test");
              assert_eq!(cfg.metadata[0].value["tier"], "prod");
              assert!(!cfg.metadata[0].allow_overwrite); // serde default false
          }
          other => panic!("expected SetMetadata, got {other:?}"),
      }
  }
  ```
  Plus `set_metadata_non_string_value_is_rejected`: a `value: { tier: 7 }` (number) → `serde_yaml::from_str::<HttpFilter>`
  returns `Err` (the string-only MVP boundary, §A5).
- [ ] **Step 2: Run to verify it fails.** Run: `cargo test -p envoy-config parses_set_metadata`
  Expected: FAIL — no `SetMetadata` variant / `SetMetadataConfig` type.
- [ ] **Step 3: Implement.** In `bootstrap.rs`, add (near `CdnLoopConfig`):
  ```rust
  /// `envoy.extensions.filters.http.set_metadata.v3.Config` (phase 33,
  /// §A1-LOCKED). The modern repeated `metadata` form. Each entry merges a flat
  /// string→string `value` map into the request's dynamic metadata under
  /// `metadata_namespace`. String-only MVP (non-string values rejected by serde
  /// — the §2.2 deferral). `allow_overwrite` (Envoy default false) governs
  /// whether existing keys in the namespace are overwritten.
  #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
  #[serde(deny_unknown_fields)]
  pub struct SetMetadataConfig {
      pub metadata: Vec<MetadataEntry>,
  }

  #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
  #[serde(deny_unknown_fields)]
  pub struct MetadataEntry {
      pub metadata_namespace: String,
      pub value: std::collections::BTreeMap<String, String>,
      #[serde(default)]
      pub allow_overwrite: bool,
  }
  ```
  Add the enum variant (after `CdnLoop`, keep the sorted-by-`@type` convention — `set_metadata` sorts after
  `router`/`rbac`; place it consistently with the existing ordering, which is grouping by add-order, so append):
  ```rust
  #[serde(rename = "type.googleapis.com/envoy.extensions.filters.http.set_metadata.v3.Config")]
  SetMetadata(SetMetadataConfig),
  ```
  Re-export `SetMetadataConfig, MetadataEntry` from `lib.rs`.
- [ ] **Step 4: Run to verify pass.** Run: `cargo test -p envoy-config parses_set_metadata set_metadata_non_string`
  Expected: PASS. Then `cargo build -p envoy-filter` — Expected: FAIL at `HttpFilterInstance::build`'s non-exhaustive
  match (the new variant is unhandled). **This is an EXPECTED, deliberate red window** closed by Task 7.
  **Sequencing rule (honor it):** run Tasks **4 → 5 → 6 → 7 contiguously**, and do NOT use `cargo build -p
  envoy-filter` (or `cargo test --workspace`) as a green gate until Task 7 lands the match arm. Each of T4/T5/T6 is
  green within ITS OWN crate (`cargo test -p envoy-config` for T4/T5, `cargo test -p envoy-filter set_metadata` for
  T6's standalone module test); only the cross-crate `envoy-filter` build is red, and only between T4 and T7. If the
  executing harness demands a workspace-green commit per task, fold T4+T5+T6+T7 into a single commit.
- [ ] **Step 5: Commit.**
  ```bash
  git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs
  git commit -m "phase 33 T4: SetMetadataConfig + HttpFilterTypedConfig::SetMetadata (@type ...v3.Config, modern form)"
  ```

---

## Task 5: `validate_http_filters` arm + `ConfigError::SetMetadataEmptyNamespace`

> §A5: empty `metadata_namespace` → boot-fatal; name mismatch → `UnsupportedHttpFilter`.

**Files:**
- Modify: `crates/envoy-config/src/lib.rs` (the new `ConfigError` variant)
- Modify: `crates/envoy-config/src/bootstrap.rs` (`validate_http_filters` arm + a `validate_set_metadata_config` helper)

- [ ] **Step 1: Write the failing tests** in `bootstrap.rs` test module:
  - `set_metadata_empty_namespace_is_fatal`: a `set_metadata` filter whose entry has `metadata_namespace: ""`,
    validated through the HCM/listener path → `Err(ConfigError::SetMetadataEmptyNamespace { .. })`.
  - `set_metadata_name_mismatch_is_unsupported`: a `SetMetadata` typed_config under the wrong filter `name` →
    `Err(ConfigError::UnsupportedHttpFilter { .. })`.
  (Model these on the existing `cdn_loop` validator tests; reuse the smallest validation entry-point those use.)
- [ ] **Step 2: Run to verify they fail.** Run: `cargo test -p envoy-config set_metadata_empty set_metadata_name`
  Expected: FAIL (no variant / no arm).
- [ ] **Step 3: Implement.** In `lib.rs` `ConfigError` (before the closing `}` near line 721):
  ```rust
  /// Phase 33 (§A5-LOCKED): a `set_metadata` filter entry has an empty
  /// `metadata_namespace`. Envoy rejects this boot-fatally (PGV: length ≥ 1);
  /// envoy-rust matches (ADR-0049 all-fatal). `listener` names the offending HCM.
  #[error("set_metadata filter on listener `{listener}` has an empty metadata_namespace; a non-empty namespace is required")]
  SetMetadataEmptyNamespace { listener: String },
  ```
  In `bootstrap.rs`, add the `validate_http_filters` arm (after the `CdnLoop` arm):
  ```rust
  crate::HttpFilterTypedConfig::SetMetadata(cfg) => {
      if f.name != "envoy.filters.http.set_metadata" {
          return Err(crate::ConfigError::UnsupportedHttpFilter { name: f.name.clone() });
      }
      validate_set_metadata_config(cfg, listener_name)?;
  }
  ```
  and the helper:
  ```rust
  fn validate_set_metadata_config(cfg: &crate::SetMetadataConfig, listener_name: &str)
      -> Result<(), crate::ConfigError> {
      for entry in &cfg.metadata {
          if entry.metadata_namespace.is_empty() {
              return Err(crate::ConfigError::SetMetadataEmptyNamespace {
                  listener: listener_name.to_string(),
              });
          }
      }
      Ok(())
  }
  ```
- [ ] **Step 4: Run to verify pass.** Run: `cargo test -p envoy-config set_metadata`
  Expected: PASS (all set_metadata validator + parse tests).
- [ ] **Step 5: Commit.**
  ```bash
  git add crates/envoy-config/src/lib.rs crates/envoy-config/src/bootstrap.rs
  git commit -m "phase 33 T5: validate_http_filters set_metadata arm + ConfigError::SetMetadataEmptyNamespace"
  ```

---

## Task 6: The `set_metadata` filter (`SetMetadataFilter`)

> The 11th filter, decode-side, `Continue`-only (observability plumbing, NEVER `StopAndSend`); encode inert.
> Follows `cdn_loop.rs` verbatim. Merges each config entry's `value` into `req.dynamic_metadata` under its
> `metadata_namespace`, honoring `allow_overwrite`.

**Files:**
- Create: `crates/envoy-filter/src/set_metadata.rs`
- Modify: `crates/envoy-filter/src/lib.rs` (add `mod set_metadata;` / `pub use` if the sibling modules are re-exported)

- [ ] **Step 1: Write failing tests** (in `set_metadata.rs` `#[cfg(test)]`):
  - `writes_value_under_namespace_and_continues`: a filter built from one entry (`envoy.test` → `{tier: prod}`,
    `allow_overwrite: false`) on an empty-metadata request → `Decision::Continue` AND
    `req.dynamic_metadata["envoy.test"]["tier"] == "prod"`.
  - `multi_namespace_multi_entry`: two entries writing two namespaces → both present.
  - `allow_overwrite_false_keeps_existing`: pre-seed `req.dynamic_metadata["envoy.test"]["tier"] = "stage"`; a filter
    entry with `allow_overwrite: false` writing `tier: prod` → keeps `stage`. With `allow_overwrite: true` → `prod`.
  - `encode_is_inert`: `encode_headers` → `Decision::Continue`, response untouched.
- [ ] **Step 2: Run to verify they fail.** Run: `cargo test -p envoy-filter set_metadata`
  Expected: FAIL — module does not exist.
- [ ] **Step 3: Implement** `set_metadata.rs`:
  ```rust
  //! The `envoy.filters.http.set_metadata` filter (phase 33; §A-LOCKED against
  //! envoyproxy/envoy:v1.33.0). Decode-side, Continue-only: merges each config
  //! entry's static string `value` map into `req.dynamic_metadata` under the
  //! entry's `metadata_namespace`, honoring `allow_overwrite`. Encode inert.
  use crate::pipeline::Decision;
  use crate::types::{FilterRequest, FilterResponse};

  #[derive(Debug, Clone)]
  pub struct SetMetadataFilter {
      metadata: Vec<envoy_config::MetadataEntry>,
  }
  impl SetMetadataFilter {
      pub(crate) fn new(cfg: &envoy_config::SetMetadataConfig) -> Self {
          Self { metadata: cfg.metadata.clone() }
      }
      pub(crate) fn decode_headers(&mut self, req: &mut FilterRequest) -> Decision {
          for entry in &self.metadata {
              let ns = req.dynamic_metadata.entry(entry.metadata_namespace.clone()).or_default();
              for (k, v) in &entry.value {
                  if entry.allow_overwrite || !ns.contains_key(k) {
                      ns.insert(k.clone(), v.clone());
                  }
              }
          }
          Decision::Continue
      }
      pub(crate) fn encode_headers(&mut self, _resp: &mut FilterResponse) -> Decision {
          Decision::Continue
      }
  }
  ```
  Wire `mod set_metadata;` into `lib.rs` consistently with the other filters.
- [ ] **Step 4: Run to verify pass.** Run: `cargo test -p envoy-filter set_metadata`
  Expected: PASS (the filter compiles standalone; the `instance.rs` match is closed in Task 7).
- [ ] **Step 5: Commit.**
  ```bash
  git add crates/envoy-filter/src/set_metadata.rs crates/envoy-filter/src/lib.rs
  git commit -m "phase 33 T6: SetMetadataFilter (decode-side Continue-only metadata emitter)"
  ```

---

## Task 7: `HttpFilterInstance::SetMetadata` wiring

> The 4 enum/dispatch wirings: variant, `build` arm, `decode_headers`/`encode_headers` arms, `apply_route_config`
> fall-through (no per-route config this phase). Closes the non-exhaustive match opened by Task 4.

**Files:**
- Modify: `crates/envoy-filter/src/instance.rs`

- [ ] **Step 1: Write the failing test** in `instance.rs` test module (model on `builds_cdn_loop_instance_and_dispatches`):
  `builds_set_metadata_instance_and_writes`: build an `HttpFilter` with `name: envoy.filters.http.set_metadata` and a
  `SetMetadata` typed_config (one entry `envoy.test`→`{tier: prod}`); `HttpFilterInstance::build(&hf, &registry,
  "ingress_http")` → `HttpFilterInstance::SetMetadata(_)`; `decode_headers` on an empty-metadata request → `Continue`
  AND `req.dynamic_metadata["envoy.test"]["tier"] == "prod"`; `encode_headers` → `Continue`.
- [ ] **Step 2: Run to verify it fails.** Run: `cargo test -p envoy-filter builds_set_metadata`
  Expected: FAIL — no `SetMetadata` arm (the match is non-exhaustive ⇒ compile error).
- [ ] **Step 3: Implement.** Add `use crate::set_metadata::SetMetadataFilter;`. Add the variant:
  `SetMetadata(SetMetadataFilter),` (with a doc comment mirroring `CdnLoop`). Add the `build` arm:
  ```rust
  envoy_config::HttpFilterTypedConfig::SetMetadata(cfg) => {
      Ok(HttpFilterInstance::SetMetadata(SetMetadataFilter::new(cfg)))
  }
  ```
  Add the `decode_headers`/`encode_headers` arms (`HttpFilterInstance::SetMetadata(f) => f.decode_headers(req)` /
  `f.encode_headers(resp_arg)`). Leave `apply_route_config` to the `_ => {}` fall-through (document set_metadata in the
  comment listing the no-per-route-config filters).
- [ ] **Step 4: Run to verify pass + the whole filter crate green.** Run: `cargo test -p envoy-filter`
  Expected: PASS (all filter tests, including the new one and every pre-existing). Then
  `cargo build -p envoy-http1 -p envoy-http2 --all-targets` — Expected: clean.
- [ ] **Step 5: Commit.**
  ```bash
  git add crates/envoy-filter/src/instance.rs
  git commit -m "phase 33 T7: HttpFilterInstance::SetMetadata wiring (11th filter variant)"
  ```

---

## Task 8: `Op::DynamicMetadata` parser + renderer

> §A2/A3/A4: `%DYNAMIC_METADATA(namespace:key)%` — single-level two-segment only; NO `:N` (boot-fatal); no-arg
> (boot-fatal); resolves to the raw unquoted scalar string; absent → `-`. NO `truncate` field.

**Files:**
- Modify: `crates/envoy-accesslog/src/command_operator.rs` (all BEHAVIOR_CONTRACT documentation is deferred to Task 12)

- [ ] **Step 1: Write failing tests** in the `command_operator.rs` test module:
  - `parses_dynamic_metadata`: `parse_format("%DYNAMIC_METADATA(envoy.test:tier)%")` →
    `vec![Segment::Op(Op::DynamicMetadata { namespace: "envoy.test".into(), key: "tier".into() })]`.
  - `renders_present_metadata_raw_unquoted`: a record with `dynamic_metadata["envoy.test"]["tier"] = "prod"` →
    render `%DYNAMIC_METADATA(envoy.test:tier)%` == `prod` (NO quotes).
  - `renders_absent_key_and_namespace_dash`: absent key → `-`; absent namespace → `-`.
  - `dynamic_metadata_rejects_truncation`: `parse_format("%DYNAMIC_METADATA(envoy.test:tier):2%")` → `Err`
    (`MalformedArgument { keyword: "DYNAMIC_METADATA", .. }`). (§A2 — Envoy boot-fatal on `:N`.)
  - `dynamic_metadata_requires_arg`: `parse_format("%DYNAMIC_METADATA%")` → `Err` (no-arg fatal).
  - `dynamic_metadata_rejects_single_and_nested_segments`: `%DYNAMIC_METADATA(envoy.test)%` (1 segment) AND
    `%DYNAMIC_METADATA(a:b:c)%` (3 segments) → `Err` (single-level MVP; §A2 nested deferral).
  - `dynamic_metadata_value_with_percent_is_literal_safe`: a value containing no `%` round-trips; (the operator value
    is rendered verbatim — confirm no re-parsing).
- [ ] **Step 2: Run to verify they fail.** Run: `cargo test -p envoy-accesslog dynamic_metadata`
  Expected: FAIL — no `DynamicMetadata` variant.
- [ ] **Step 3: Implement.**
  - Add the variant: `DynamicMetadata { namespace: String, key: String }` (NO `truncate` — §A2).
  - In `parse_operator`, add a `"DYNAMIC_METADATA"` keyword arm that calls a new `parse_dynamic_metadata_op(rest)`:
    requires `rest` (the `(arg)`) present (else `MalformedArgument { keyword, detail: "requires a (namespace:key)
    argument" }`); finds the closing `)`; requires the suffix after `)` to be EMPTY (else `MalformedArgument { keyword,
    detail: "does not accept a ':N' length suffix" }` — §A2); splits `arg` on `:` and requires EXACTLY two non-empty
    segments (else `MalformedArgument { keyword, detail: "requires exactly 'namespace:key'" }`). Namespace/key are NOT
    lowercased (metadata keys are case-sensitive, unlike header names). Returns
    `Op::DynamicMetadata { namespace, key }`.
  - In `render_op`, add: `Op::DynamicMetadata { namespace, key } => out.push_str(
    record.dynamic_metadata.get(namespace).and_then(|m| m.get(key)).map(String::as_str).unwrap_or("-"))`.
- [ ] **Step 4: Run to verify pass.** Run: `cargo test -p envoy-accesslog`
  Expected: PASS (all new + pre-existing). Then `cargo build -p envoy-config` (validate_access_logs reuses
  `parse_format` — the new operator now parses) — Expected: clean.
- [ ] **Step 5: Commit.**
  ```bash
  git add crates/envoy-accesslog/src/command_operator.rs
  git commit -m "phase 33 T8: Op::DynamicMetadata (single-level namespace:key, raw scalar, no truncation per §A2)"
  ```

---

## Task 9: H1 dynamic-metadata threading + H1 in-process backstop

> §2.1 item 1: capture `filter_req.dynamic_metadata` before `filter_req` is dropped (H1 hcm.rs ~786) and populate the
> H1 record build (~1189). `filter_req` is partially moved by the field write-backs (~783-786); capture the
> `dynamic_metadata` field into a local BEFORE the `match decode_decision` so it is available on both the `Continue`
> and `StopAndSend` branches.

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs`

- [ ] **Step 1: Write the failing test** — an H1 in-process backstop (in `hcm.rs` tests, model on the existing
  access-log wiring tests `hcm_config_with_access_log` / the `compiled_log_format` test): drive a `[set_metadata,
  router]` chain + an access logger whose `log_format` includes `%DYNAMIC_METADATA(envoy.test:tier)%` through the H1
  HCM with a `direct_response` route; scrape the written access-log line; assert it contains the rendered `prod` (and
  an absent-key probe renders `-`). (If a full HCM-drive harness test is heavy, the minimal failing test asserts the
  built `AccessLogRecord.dynamic_metadata` carries `{envoy.test:{tier:prod}}` after decode — whichever the existing H1
  access-log test scaffolding supports.)
- [ ] **Step 2: Run to verify it fails.** Run: `cargo test -p envoy-http1 <test name>`
  Expected: FAIL — the record's `dynamic_metadata` is empty (threading not yet wired).
- [ ] **Step 3: Implement.** At ~786 (right after the four field write-backs, BEFORE `match decode_decision`):
  ```rust
  // Phase 33: capture the pipeline's dynamic metadata before `filter_req` is
  // dropped, so the access-log record build below can render %DYNAMIC_METADATA%.
  let dynamic_metadata = filter_req.dynamic_metadata;
  ```
  (a full move of the remaining field; `filter_req` is already partially moved). At the record build (~1189), add
  `dynamic_metadata: dynamic_metadata.clone(),` to the `AccessLogRecord { … }` literal. (Clone because the build site
  is inside `if !config.access_log.is_empty()`; if the borrow checker permits a move — the local is not used again —
  move instead. Prefer move if single-use.)
- [ ] **Step 4: Run to verify pass.** Run: `cargo test -p envoy-http1`
  Expected: PASS (the backstop + all pre-existing H1 tests; behavior for chains WITHOUT set_metadata is unchanged —
  empty metadata → no `%DYNAMIC_METADATA%` in their formats).
- [ ] **Step 5: Commit.**
  ```bash
  git add crates/envoy-http1/src/hcm.rs
  git commit -m "phase 33 T9: H1 dynamic-metadata threading (capture-before-drop → record) + H1 backstop"
  ```

---

## Task 10: H2 dynamic-metadata threading + H2 in-process backstop

> §2.1 item 1 (the C-1 spec-review correction): H2 builds its OWN record (~888) and currently drops `filter_req` after
> `decode_headers` (~488), so `filter_req.dynamic_metadata` would be LOST without a symmetric capture. Because fixture
> 0041 is H1-only, the H2 threading is verified THIS phase by an H2 in-process backstop — NOT deferred (no §6.3 gap).

**Files:**
- Modify: `crates/envoy-http2/src/hcm.rs`

- [ ] **Step 1: Write the failing test** — an H2 in-process backstop (model on the H2 access-log wiring tests): drive
  a `[set_metadata, router]` chain + a `%DYNAMIC_METADATA(envoy.test:tier)%` logger through the H2 HCM; assert the
  built `AccessLogRecord.dynamic_metadata` carries `{envoy.test:{tier:prod}}` (and/or the rendered line contains
  `prod`; absent-key → `-`). This is the SOLE proof of the H2 path (no H2 fixture).
- [ ] **Step 2: Run to verify it fails.** Run: `cargo test -p envoy-http2 <test name>`
  Expected: FAIL — H2 record `dynamic_metadata` empty.
- [ ] **Step 3: Implement.** At ~488 (right after the four field write-backs, BEFORE `match decode_decision`):
  ```rust
  // Phase 33: capture dynamic metadata before `filter_req` is dropped (H2 builds
  // its own record at ~888 and does NOT inherit from H1 — spec C-1).
  let dynamic_metadata = filter_req.dynamic_metadata;
  ```
  At the H2 record build (~888), add `dynamic_metadata: dynamic_metadata.clone(),` (or move if single-use) to the
  `AccessLogRecord { … }` literal.
- [ ] **Step 4: Run to verify pass.** Run: `cargo test -p envoy-http2`
  Expected: PASS (backstop + all pre-existing H2 tests).
- [ ] **Step 5: Commit.**
  ```bash
  git add crates/envoy-http2/src/hcm.rs
  git commit -m "phase 33 T10: H2 dynamic-metadata threading (symmetric capture-before-drop → record) + H2 backstop"
  ```

---

## Task 11: Fixture `0041` differential (present-key + absent-key probes)

> §A6: the STRONG cross-proxy byte-exact target. `direct_response` (no upstream), deterministic `log_format` with
> `%DYNAMIC_METADATA%` present + absent operators, NO timing operator. The present+absent probe PAIR is the guard
> against an echo-the-config-literal (non-store-backed) implementation (the absent probe must resolve `-` from the
> SAME store path).

**Files:**
- Create: `tests/fixtures/0041-http-set-metadata-dynamic-metadata/envoy.yaml`
- Create: `tests/fixtures/0041-http-set-metadata-dynamic-metadata/envoy-rust.yaml` (identical to envoy.yaml)
- Create: `tests/fixtures/0041-http-set-metadata-dynamic-metadata/expectations.yaml`
- Create: `tests/fixtures/0041-http-set-metadata-dynamic-metadata/README.md`
- Create: `tests/differential/tests/set_metadata_dynamic_metadata.rs`

- [ ] **Step 1: Write the fixture + the differential test (the "failing test").** `envoy.yaml` (mirror 0040, swap the
  log_format + add the set_metadata filter; use distinct mount paths `/tmp/0041-envoy-mount/access.log` and
  `/tmp/0041-envoy-rust-mount/access.log`):
  ```yaml
  node: { id: envoy-rust-phase-33-fixture-0041, cluster: envoy-rust-phase-33 }
  admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
  static_resources:
    listeners:
      - name: http1_listener
        address: { socket_address: { address: 0.0.0.0, port_value: {{PORT}} } }
        filter_chains:
          - filters:
              - name: envoy.filters.network.http_connection_manager
                typed_config:
                  "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                  stat_prefix: ingress_http
                  codec_type: HTTP1
                  generate_request_id: false
                  access_log:
                    - name: envoy.access_loggers.file
                      typed_config:
                        "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                        path: /tmp/0041-envoy-mount/access.log
                        log_format:
                          text_format_source:
                            inline_string: "m=%REQ(:METHOD)% code=%RESPONSE_CODE% tier=%DYNAMIC_METADATA(envoy.test:tier)% missk=%DYNAMIC_METADATA(envoy.test:missing)% missns=%DYNAMIC_METADATA(envoy.absent:k)%\n"
                  route_config:
                    name: local_route
                    virtual_hosts:
                      - name: backend_vh
                        domains: ["*"]
                        routes:
                          - match: { prefix: "/" }
                            direct_response: { status: 200, body: { inline_string: "ok\n" } }
                  http_filters:
                    - name: envoy.filters.http.set_metadata
                      typed_config:
                        "@type": type.googleapis.com/envoy.extensions.filters.http.set_metadata.v3.Config
                        metadata:
                        - metadata_namespace: envoy.test
                          value: { tier: prod }
                    - name: envoy.filters.http.router
                      typed_config:
                        "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
    clusters: []
  ```
  `envoy-rust.yaml` = identical. `expectations.yaml` (mirror 0040's `http1_access_log_byte_exact`; present + absent
  rendered in ONE line; ≥2 probes — the absent operators always render `-`, the present renders `prod`):
  ```yaml
  driver:
    kind: http1_access_log_byte_exact
    expected_access_log_paths:
      envoy: /tmp/0041-envoy-mount/access.log
      envoy_rust: /tmp/0041-envoy-rust-mount/access.log
    probes:
      # Expected byte-identical line: m=GET code=200 tier=prod missk=- missns=-
      - { method: get, path: /a, host: envoy-rust.test }
      # Second probe (POST) proves the static metadata is request-independent.
      # Expected: m=POST code=200 tier=prod missk=- missns=-
      - { method: post, path: /b, host: envoy-rust.test, body: "x" }
  ```
  `README.md`: describe the fixture, the §A-locked facts it pins, and the present/absent guard. The differential test
  `set_metadata_dynamic_metadata.rs` mirrors `access_log_command_operators.rs` (calls `differential::run_fixture`).
- [ ] **Step 2: Run to verify it fails (or is Docker-gated).** Run: `cargo test -p differential set_metadata_dynamic_metadata -- --include-ignored`
  Expected on a Docker host: initially RED if any wiring is incomplete; GREEN once Tasks 4–10 are in. (This host:
  access-log file-scrape differentials ARE locally authoritative — no reload trigger; per memory
  `host-docker-desktop-virtiofs-no-inotify`, fixture 0041 runs locally. Confirm green LOCALLY at state-4.)
- [ ] **Step 3: (Implementation already done in Tasks 4–10.)** Adjust the `log_format` / probes only if the live run
  surfaces a byte mismatch; the §A facts predict a clean match.
- [ ] **Step 4: Run to verify pass.** Run the fixture; confirm both proxies' scraped lines are byte-identical
  (`assert_access_log_lines_byte_identical`). Run the full `cargo test -p differential` build to confirm no other
  fixture regressed at compile.
- [ ] **Step 5: Commit.**
  ```bash
  git add tests/fixtures/0041-http-set-metadata-dynamic-metadata tests/differential/tests/set_metadata_dynamic_metadata.rs
  git commit -m "phase 33 T11: fixture 0041 set_metadata + %DYNAMIC_METADATA% byte-exact differential (present+absent)"
  ```

---

## Task 12: BEHAVIOR_CONTRACT extension + fuzz seeds + M32-4/M32-5 folds

> Documents the new operator; adds fuzz seeds to the EXISTING targets (no new target — §A); folds M32-4 (loop the
> default-equivalence oracle) and M32-5 (delete the vestigial 0-byte payload.bin).

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (the "Access log field mapping" / phase-32 subsection)
- Create: a seed file under `crates/envoy-accesslog/fuzz/corpus/accesslog_format_parse/` (a format string with
  `%DYNAMIC_METADATA(envoy.test:tier)%`)
- Create: a seed file under `crates/envoy-config/fuzz/corpus/parse_bootstrap/` (a bootstrap with a `set_metadata`
  filter + a `%DYNAMIC_METADATA%`-bearing `log_format`)
- Modify: `crates/envoy-accesslog/src/default_format.rs` (M32-4)
- Delete: `tests/fixtures/0040-accesslog-command-operators/inputs/payload.bin` (M32-5)

- [ ] **Step 1: Write the failing test (M32-4).** In `default_format.rs`, replace the single-record
  `compiled_default_matches_legacy_concatenator` with a loop over ≥3 records (the baseline, a 5xx/router-proxy record
  with `upstream_host`+`upstream_service_time`+`response_code: 503`, and a UTF-8 `user_agent`/`authority` record),
  asserting `engine == format!("{legacy}\n")` for each. (Add the records; the assertion is the test.)
- [ ] **Step 2: Run to verify the loop covers + fails if regressed.** Run: `cargo test -p envoy-accesslog
  compiled_default_matches_legacy`. Expected: PASS (the engine ≡ legacy for every record — proves the new operator did
  not perturb the default format).
- [ ] **Step 3: Implement the docs + seeds + M32-5.**
  - **BEHAVIOR_CONTRACT.md:** under the phase-32 "Access log field mapping" section, add a phase-33 subsection
    documenting: the `%DYNAMIC_METADATA(namespace:key)%` operator (single-level, two-segment, `:`-separated,
    case-sensitive, NO `:N` truncation — boot-fatal, no-arg boot-fatal); the resolution
    `record.dynamic_metadata.get(ns)?.get(key)`; the RAW unquoted scalar-string byte form (§A3) and absent `-` (§A4);
    the deterministic classification (the static-config value is whole-line byte-exact cross-proxy — witness fixture
    0041); the §2.2 deferrals (non-string Values → JSON-quoted; nested paths; whole-namespace; the deprecated
    top-level `set_metadata` form; `:N`). Add a `set_metadata` config-shape note (`@type …v3.Config`, modern repeated
    form, string-only value, empty-namespace boot-fatal).
  - **Fuzz seeds:** drop the two seed files (raw bytes; the corpus dirs are content-addressed but new seeds may have any
    filename). Confirm `cargo +nightly fuzz run accesslog_format_parse -- -runs=0` and `… parse_bootstrap -- -runs=0`
    load the corpus (the §7.5 gate (d) short-budget run happens at state-4).
  - **M32-5:** `git rm tests/fixtures/0040-accesslog-command-operators/inputs/payload.bin` (the
    `Http1AccessLogByteExact` driver drives probes, not an input file — confirm the 0040 test still passes).
- [ ] **Step 4: Run to verify pass.** Run: `cargo test -p envoy-accesslog` and `cargo test -p differential
  access_log_command_operators -- --include-ignored` (0040 still green after the payload.bin removal). Expected: clean.
- [ ] **Step 5: Commit.**
  ```bash
  git add -A
  git commit -m "phase 33 T12: BEHAVIOR_CONTRACT %DYNAMIC_METADATA% + fuzz seeds + M32-4 oracle loop + M32-5 payload.bin"
  ```

---

## §7.5 acceptance gate (state-4 verification — previewed)

A phase-33 state-4 verification (the next-but-one session) must show ALL green:
- **(a)** fixture `0041` green (cross-proxy byte-identical access-log line; present `prod` + absent `-`).
- **(b)** all of `0001`–`0040` green (incl. `0012` UNCHANGED — the default-format byte-identical regression witness).
- **(c)** h2spec ≥95% (unchanged — no HTTP/2 codec change).
- **(d)** the EXISTING `accesslog_format_parse` + `parse_bootstrap` fuzz targets clean for the short-budget CI run
  (with the new seeds) — **NO new fuzz target** (§A §3.8). Per memory `new-fuzz-target-needs-a-ci-yml-step`: confirm
  NO ci.yml change is needed (both targets + steps already exist; only corpus seeds added).
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D
  warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check` all clean.
- **(f)** `REVIEW.md` approved.

`#![forbid(unsafe_code)]` holds (D-3.8). No new crate, no new dependency (D-3.2). Per the project memory
`envoy-rust-state4-ci-first-execution`: `cargo fmt --check` + the Docker differential first run at state-4 — budget
CI iteration; this host runs the 0041 access-log file-scrape differential locally-authoritatively (memory
`host-docker-desktop-virtiofs-no-inotify`).

---

_§A facts locked by **ADR-0081** (the §6.2 reconciliation). Scope locked by **ADR-0080**. ADR-0082 reserved (§6.1
split — NOT fired). The state-3 implementation is the next session (`superpowers:executing-plans` or
`superpowers:subagent-driven-development`)._
