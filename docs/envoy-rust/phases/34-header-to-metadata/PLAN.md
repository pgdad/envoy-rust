# Phase 34 — `header_to_metadata` (request-header → dynamic-metadata) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
> Every task is TDD: write the failing test FIRST, watch it fail, implement minimally, watch it pass, commit.

**Goal:** Land the `envoy.filters.http.header_to_metadata` HTTP filter — a request-header-driven dynamic-metadata
emitter (the 12th `HttpFilterInstance` variant, decode-side, `Continue`-only) that extracts request-header values
into `req.dynamic_metadata`, proven by a byte-exact cross-proxy access-log line (fixture `0042`), with all 41
pre-existing fixtures unchanged.

**Architecture:** The filter REUSES the phase-33 dynamic-metadata store (`FilterRequest`/`AccessLogRecord.dynamic_metadata`),
the `%DYNAMIC_METADATA(namespace:key)%` operator, and the filter-agnostic H1/H2 capture-before-drop threading
UNCHANGED (no new infrastructure; `envoy-accesslog` is untouched). On `decode_headers`, the filter evaluates each
config `request_rule` against the request headers and merges the extracted **string** value into
`req.dynamic_metadata` under `metadata_namespace`→`key`. A `HeaderToMetadataConfig` (`@type …header_to_metadata.v3.Config`)
+ a `validate_http_filters` arm + a `ConfigError::HeaderToMetadataInvalidRule` provide config + boot-fatal validation.

**Tech Stack:** Rust (pinned toolchain, `#![forbid(unsafe_code)]` workspace-wide), `serde`/`serde_yaml`
(`deny_unknown_fields`), `thiserror`, the phase-33 dynamic-metadata store + `%DYNAMIC_METADATA%` operator (reused),
the phase-07 filter framework, `tests/differential` (`Driver::Http1AccessLogByteExact` +
`assert_access_log_lines_byte_identical`), `cargo fuzz` (existing `parse_bootstrap` target — NO new target).

---

## §A — Empirically-locked facts (the §6.2 ground truth; reconciled by ADR-0084)

Run LOCALLY at this PLAN-write against live `envoyproxy/envoy:v1.33.0` — an H1 `direct_response` listener, a
`[header_to_metadata, router]` chain, a file access logger whose `log_format.text_format_source.inline_string`
carried `%DYNAMIC_METADATA(...)%` operators; probes with/without request headers; bytes via `od -c`; config round-tripped
through `/config_dump`. **ADR-0084 FIRES** — **TWO MATERIAL divergences** from the SPEC projection (the default
`metadata_namespace`; the on_header_present static-value precedence), plus confirmations and refinements. These facts are
LOCKED; the tasks below encode them. Do NOT re-derive.

**A1 — config wire shape (CONFIRMED).** The proto message is **`Config`** (`@type =
type.googleapis.com/envoy.extensions.filters.http.header_to_metadata.v3.Config`). Top-level **`request_rules`** (a
list). Each rule `{ header, on_header_present, on_header_missing, cookie?, remove? }`; each action is a **`KeyValuePair
{ metadata_namespace, key, value, type, encode }`**. Confirmed via `/config_dump` round-trip. Unknown fields are
**boot-fatal** (A5e). **Phase-34 models the request-side, string-only subset:** `request_rules`, `header`,
`on_header_present`/`on_header_missing`, `KeyValuePair { metadata_namespace, key, value, type }`. The `cookie` /
`remove` rule fields + the `encode` action field are §2.2-DEFERRED — `deny_unknown_fields` rejects them boot-fatal
(envoy-rust stricter; documented; not differentially exercised).

**A2 — default `metadata_namespace` (MATERIAL DIVERGENCE).** When `metadata_namespace` is OMITTED from a
`KeyValuePair`, Envoy defaults it to the string **`envoy.filters.http.header_to_metadata`** (the filter's canonical
name), **NOT `envoy.lb`** as the SPEC projected. PROVEN: `on_header_present: { key: tier }` (no namespace) + `x-tier:
prod` → `%DYNAMIC_METADATA(envoy.filters.http.header_to_metadata:tier)%` renders `prod`, while
`%DYNAMIC_METADATA(envoy.lb:tier)%` renders `-`. **The MVP serde default for `metadata_namespace` is
`"envoy.filters.http.header_to_metadata"`.**

**A3 — on_header_present value precedence + byte form (MATERIAL DIVERGENCE + CONFIRMED bytes).**
- A present header writes the **RAW header value, UNQUOTED** (`od -c` → `p r o d`, NOT `" p r o d "`). CONFIRMED — the
  phase-33 `%DYNAMIC_METADATA%` operator render is reused verbatim (raw scalar string).
- **DIVERGENCE: when BOTH a static `value` AND the header are present, the static `value` WINS** (the header value is
  discarded). PROVEN: `on_header_present: { key: ovr, value: "override-val" }` + `x-ovr: from-header` → renders
  `override-val`. So `on_header_present` writes: the static `value` if configured, ELSE the header's value.
- `type` defaults to **STRING** (a `type: NUMBER` with a non-numeric header silently DROPS the key → renders `-`). The
  MVP models ONLY `type: STRING` / `type` absent; a `type: NUMBER | PROTOBUF_VALUE` is the §2.2 non-string-Value
  deferral → config-fatal in envoy-rust (stricter; documented; the fixture omits `type`).

**A4 — on_header_missing semantics (CONFIRMED + refinements).**
- `on_header_missing` **REQUIRES a `value`** (there is no header to read) — an `on_header_missing` with no `value` is
  **boot-fatal** (A5d). When the header is absent + `on_header_missing: { key, value: default-tier }` → renders
  `default-tier` (raw, unquoted).
- **Present-but-EMPTY header → key UNSET** (Envoy treats an empty header value as NEITHER present NOR missing → renders
  `-`). So `on_header_present` fires only when the header is present with a **non-empty** value; `on_header_missing`
  fires only when the header is **fully absent**.
- Header absent + NO `on_header_missing` → key unset → renders the phase-33 absent sentinel `-`. CONFIRMED.

**A5 — config-validity disposition (ALL boot-fatal, exit 1; ADR-0049 all-fatal).** Envoy fails boot on every malformed
case; envoy-rust matches (boot-fatal). The MVP enforces (a)-(d) in `validate_http_filters`; (e) falls out of
`deny_unknown_fields`:
- (a) empty `header: ""` → fatal (Envoy: "One of Cookie or Header option needs to be specified");
- (b) empty `key: ""` in an action → fatal (Envoy PGV: "value length must be at least 1 characters");
- (c) a rule with NEITHER `on_header_present` NOR `on_header_missing` → fatal (Envoy: "rule for header '…' has neither
  `on_header_present` nor `on_header_missing` set");
- (d) an `on_header_missing` with no `value` → fatal (Envoy: "Cannot specify on_header_missing rule with an empty value");
- (e) an unknown field → fatal (serde `deny_unknown_fields`).

**A6 — determinism (STRONG target FIRES).** 50 identical requests → exactly ONE unique access-log render line. The
extracted value is a pure function of the (fixed) request + static config (no host-address/clock terms), so both proxies
emit a byte-identical line. The cross-proxy whole-line byte-exact differential
(`Driver::Http1AccessLogByteExact` + `assert_access_log_lines_byte_identical`, reused verbatim) is the STRONG target — no
fallback. **Note:** the file access logger buffers ~1s — the fixture/harness must flush (the existing 0040/0041
file-scrape path already handles this; reuse it).

**Disposition of the SPEC §3 open PLAN-write calls (now LOCKED):** §3.1 wire shape → `@type …v3.Config`, `request_rules`
list, `KeyValuePair { metadata_namespace, key, value, type }`, request-side string-only subset (A1); §3.1 default
namespace → **`envoy.filters.http.header_to_metadata`** (A2, DIVERGENCE); §3.2 value precedence → **static `value` wins**,
raw unquoted bytes (A3, DIVERGENCE); §3.3 on_header_missing → requires `value`, present-but-empty → unset, absent → `-`
(A4); §3.4 config-validity → boot-fatal, ONE new `ConfigError::HeaderToMetadataInvalidRule` + reuse `UnsupportedHttpFilter`
(name) + serde `deny_unknown_fields` (A5); §3.5/§3.6 fixture-0042 → `direct_response`, header-present + header-missing
probes via `AccessLogByteExactProbe.extra_headers` (A6); §3.7 harness → reused verbatim; §3.8 fuzz → existing
`parse_bootstrap` covers it (NO new target — add a seed); §3.9 split → NOT fired (~7 tasks; the §6.1 split projected NOT
to fire).

---

## File structure (what each file is responsible for)

**New files:**
- `crates/envoy-filter/src/header_to_metadata.rs` — the `HeaderToMetadataFilter` (struct + `new` + `decode_headers`
  extraction + inert `encode_headers` + unit tests). The 12th filter, following the `set_metadata.rs`
  add-a-decode-side-filter pattern.
- `tests/fixtures/0042-http-header-to-metadata/{envoy.yaml,envoy-rust.yaml,expectations.yaml,README.md}` — the
  differential fixture (H1 `direct_response`, `[header_to_metadata, router]` chain, `%DYNAMIC_METADATA%`-bearing
  `log_format`, header-present + header-missing probes).
- `tests/differential/tests/header_to_metadata.rs` — the Docker-gated differential test (mirrors
  `set_metadata_dynamic_metadata.rs`).

**Modified files (one responsibility each):**
- `crates/envoy-config/src/bootstrap.rs` — `HeaderToMetadataConfig` + `Rule` + `KeyValuePair` + `MetadataType` structs;
  the `HttpFilterTypedConfig::HeaderToMetadata` variant; the `validate_http_filters` arm + a
  `validate_header_to_metadata_config` helper.
- `crates/envoy-config/src/lib.rs` — the `ConfigError::HeaderToMetadataInvalidRule` variant + re-exports.
- `crates/envoy-filter/src/instance.rs` — the `HttpFilterInstance::HeaderToMetadata` variant + build/decode/encode/
  apply_route wirings (the 12th variant).
- `crates/envoy-filter/src/lib.rs` — `pub mod header_to_metadata;` + `pub use`.
- `crates/envoy-http1/src/hcm.rs` — an H1 in-process end-to-end backstop test (the threading is UNCHANGED).
- `crates/envoy-http2/src/hcm.rs` — an H2 in-process backstop test (proving the filter-agnostic thread carries
  header_to_metadata output too).
- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — the "HTTP filters" + "Access log field mapping" extension.
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/` — a new seed (a bootstrap with a `header_to_metadata` filter +
  a `%DYNAMIC_METADATA%`-bearing `log_format`).
- Plus the mechanical construction-site touches as the compiler flags them.

---

## Task 1: `HeaderToMetadataConfig` schema + `HttpFilterTypedConfig::HeaderToMetadata` variant

> §A1/A2: `@type …header_to_metadata.v3.Config`; `request_rules`; `KeyValuePair { metadata_namespace (serde default
> `envoy.filters.http.header_to_metadata`), key, value: Option<String>, type: MetadataType (default String) }`;
> string-only, request-side subset; `deny_unknown_fields` rejects `cookie`/`remove`/`encode` + non-`STRING` `type`.

**Files:** Modify `crates/envoy-config/src/bootstrap.rs` (structs + enum variant); `crates/envoy-config/src/lib.rs` (re-exports).

- [ ] **Step 1: Write the failing test** in `bootstrap.rs` test module:
  ```rust
  #[test]
  fn parses_header_to_metadata_filter() {
      let yaml = r#"
  name: envoy.filters.http.header_to_metadata
  typed_config:
    "@type": type.googleapis.com/envoy.extensions.filters.http.header_to_metadata.v3.Config
    request_rules:
    - header: x-tier
      on_header_present:
        metadata_namespace: envoy.lb
        key: tier
      on_header_missing:
        metadata_namespace: envoy.lb
        key: tier
        value: default-tier
  "#;
      let hf: HttpFilter = serde_yaml::from_str(yaml).expect("parses");
      match hf.typed_config {
          HttpFilterTypedConfig::HeaderToMetadata(cfg) => {
              assert_eq!(cfg.request_rules.len(), 1);
              let r = &cfg.request_rules[0];
              assert_eq!(r.header, "x-tier");
              let p = r.on_header_present.as_ref().unwrap();
              assert_eq!(p.metadata_namespace, "envoy.lb");
              assert_eq!(p.key, "tier");
              assert!(p.value.is_none());
              assert_eq!(r.on_header_missing.as_ref().unwrap().value.as_deref(), Some("default-tier"));
          }
          other => panic!("expected HeaderToMetadata, got {other:?}"),
      }
  }
  ```
  Plus `header_to_metadata_default_namespace_is_filter_name`: a `KeyValuePair` with NO `metadata_namespace` →
  `cfg.request_rules[0].on_header_present.unwrap().metadata_namespace == "envoy.filters.http.header_to_metadata"` (§A2).
  Plus `header_to_metadata_rejects_unknown_rule_field`: a rule with `remove: true` (the deferred field) →
  `serde_yaml::from_str::<HttpFilter>` returns `Err` (§A1/A5e, `deny_unknown_fields`).
- [ ] **Step 2: Run to verify it fails.** `cargo test -p envoy-config parses_header_to_metadata header_to_metadata_default header_to_metadata_rejects`
  Expected: FAIL — no `HeaderToMetadata` variant / types.
- [ ] **Step 3: Implement.** In `bootstrap.rs`, add (near `SetMetadataConfig`):
  ```rust
  /// `envoy.extensions.filters.http.header_to_metadata.v3.Config` (phase 34, §A1-LOCKED).
  /// Request-side, string-only subset: each rule extracts a request header into dynamic
  /// metadata. `cookie`/`remove`/`encode`/non-STRING `type` are §2.2-deferred → rejected
  /// by `deny_unknown_fields` / the `MetadataType` enum (boot-fatal, stricter than Envoy).
  #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
  #[serde(deny_unknown_fields)]
  pub struct HeaderToMetadataConfig {
      pub request_rules: Vec<HeaderToMetadataRule>,
  }

  #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
  #[serde(deny_unknown_fields)]
  pub struct HeaderToMetadataRule {
      pub header: String,
      #[serde(default)]
      pub on_header_present: Option<HeaderToMetadataKeyValue>,
      #[serde(default)]
      pub on_header_missing: Option<HeaderToMetadataKeyValue>,
  }

  #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
  #[serde(deny_unknown_fields)]
  pub struct HeaderToMetadataKeyValue {
      #[serde(default = "default_h2m_namespace")]
      pub metadata_namespace: String,
      pub key: String,
      #[serde(default)]
      pub value: Option<String>,
      #[serde(default)]
      pub r#type: HeaderToMetadataType,
  }

  fn default_h2m_namespace() -> String { "envoy.filters.http.header_to_metadata".to_string() }

  /// §A3: MVP models only STRING. A `type: NUMBER | PROTOBUF_VALUE` (the §2.2 non-string-Value
  /// deferral) fails deserialization → boot-fatal (stricter than Envoy).
  #[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
  #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
  pub enum HeaderToMetadataType {
      #[default]
      String,
  }
  ```
  (NOTE: confirm the `key` field is REQUIRED — no `#[serde(default)]` — so an action without `key` fails serde; A5b's
  empty-`key` case is caught by the validator in Task 2.) Append the enum variant (after `SetMetadata`):
  ```rust
  #[serde(rename = "type.googleapis.com/envoy.extensions.filters.http.header_to_metadata.v3.Config")]
  HeaderToMetadata(HeaderToMetadataConfig),
  ```
  Re-export the new types from `lib.rs` (alphabetical `pub use bootstrap::{…}`).
- [ ] **Step 4: Run to verify pass.** `cargo test -p envoy-config parses_header_to_metadata header_to_metadata_default header_to_metadata_rejects` → PASS.
  Then `cargo build -p envoy-filter` → **EXPECTED FAIL** at `HttpFilterInstance::build`'s non-exhaustive match (closed in
  Task 4). **Sequencing rule:** run Tasks 1→2→3→4 contiguously; the cross-crate `envoy-filter` build is red only between
  T1 and T4. Gate T1/T2 on `cargo test -p envoy-config`, T3 on `cargo test -p envoy-filter header_to_metadata` (the
  standalone module test). If the harness demands a workspace-green commit per task, fold T1+T2+T3+T4.
- [ ] **Step 5: Commit.** `git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs && git commit -m "phase 34 T1: HeaderToMetadataConfig + HttpFilterTypedConfig::HeaderToMetadata (@type ...v3.Config)"`

---

## Task 2: `validate_http_filters` arm + `ConfigError::HeaderToMetadataInvalidRule`

> §A5: each rule — `header` non-empty; ≥1 of present/missing; each action `key` non-empty; `on_header_missing` MUST have
> `value`. All boot-fatal (ADR-0049). Name mismatch → `UnsupportedHttpFilter`.

**Files:** Modify `crates/envoy-config/src/lib.rs` (the `ConfigError` variant); `crates/envoy-config/src/bootstrap.rs`
(the `validate_http_filters` arm + a `validate_header_to_metadata_config` helper).

- [ ] **Step 1: Write the failing tests** in `bootstrap.rs` test module (model on the `set_metadata` validator tests,
  driven through the full `parse_bootstrap(&yaml)` entry-point):
  - `header_to_metadata_empty_header_is_fatal`: a rule with `header: ""` → `Err(ConfigError::HeaderToMetadataInvalidRule { .. })`.
  - `header_to_metadata_no_action_is_fatal`: a rule with neither `on_header_present` nor `on_header_missing` → `Err(…InvalidRule)`.
  - `header_to_metadata_empty_key_is_fatal`: an `on_header_present` with `key: ""` → `Err(…InvalidRule)`.
  - `header_to_metadata_missing_without_value_is_fatal`: an `on_header_missing` with no `value` → `Err(…InvalidRule)`.
  - `header_to_metadata_name_mismatch_is_unsupported`: a `HeaderToMetadata` typed_config under the wrong filter `name`
    → `Err(ConfigError::UnsupportedHttpFilter { .. })`.
- [ ] **Step 2: Run to verify they fail.** `cargo test -p envoy-config header_to_metadata_empty header_to_metadata_no_action header_to_metadata_missing header_to_metadata_name` → FAIL.
- [ ] **Step 3: Implement.** In `lib.rs` `ConfigError` (near `SetMetadataEmptyNamespace`):
  ```rust
  /// Phase 34 (§A5-LOCKED): a `header_to_metadata` rule is malformed (empty header, no action,
  /// empty key, or an on_header_missing with no value). Envoy rejects these boot-fatally; envoy-rust
  /// matches (ADR-0049). `listener` names the offending HCM; `detail` the specific violation.
  #[error("header_to_metadata filter on listener `{listener}` has an invalid rule: {detail}")]
  HeaderToMetadataInvalidRule { listener: String, detail: String },
  ```
  In `bootstrap.rs`, add the `validate_http_filters` arm (after the `SetMetadata` arm):
  ```rust
  crate::HttpFilterTypedConfig::HeaderToMetadata(cfg) => {
      if f.name != "envoy.filters.http.header_to_metadata" {
          return Err(crate::ConfigError::UnsupportedHttpFilter { name: f.name.clone() });
      }
      validate_header_to_metadata_config(cfg, listener_name)?;
  }
  ```
  and the helper (§A5 (a)-(d)):
  ```rust
  fn validate_header_to_metadata_config(cfg: &crate::HeaderToMetadataConfig, listener: &str)
      -> Result<(), crate::ConfigError> {
      let bad = |detail: String| crate::ConfigError::HeaderToMetadataInvalidRule { listener: listener.to_string(), detail };
      for rule in &cfg.request_rules {
          if rule.header.is_empty() {
              return Err(bad("a rule has an empty `header`".into()));
          }
          if rule.on_header_present.is_none() && rule.on_header_missing.is_none() {
              return Err(bad(format!("rule for header `{}` has neither on_header_present nor on_header_missing", rule.header)));
          }
          for kv in [rule.on_header_present.as_ref(), rule.on_header_missing.as_ref()].into_iter().flatten() {
              if kv.key.is_empty() {
                  return Err(bad(format!("rule for header `{}` has an empty `key`", rule.header)));
              }
          }
          if let Some(miss) = &rule.on_header_missing {
              if miss.value.is_none() {
                  return Err(bad(format!("on_header_missing for header `{}` requires a `value`", rule.header)));
              }
          }
      }
      Ok(())
  }
  ```
- [ ] **Step 4: Run to verify pass.** `cargo test -p envoy-config header_to_metadata` → PASS (all parse + validator tests).
- [ ] **Step 5: Commit.** `git add crates/envoy-config/src/lib.rs crates/envoy-config/src/bootstrap.rs && git commit -m "phase 34 T2: validate_http_filters header_to_metadata arm + ConfigError::HeaderToMetadataInvalidRule"`

---

## Task 3: The `HeaderToMetadataFilter`

> §A2/A3/A4: decode-side, `Continue`-only; for each rule — header present (non-empty) → write the static `value` (if set,
> §A3 precedence) ELSE the header value; header absent → on_header_missing `value`; present-but-empty → nothing.
> Header lookup is case-insensitive. Encode inert. Follows `set_metadata.rs` verbatim.

**Files:** Create `crates/envoy-filter/src/header_to_metadata.rs`; modify `crates/envoy-filter/src/lib.rs`.

- [ ] **Step 1: Write failing tests** (`header_to_metadata.rs` `#[cfg(test)]`):
  - `present_writes_header_value_and_continues`: a filter from `header: x-tier, on_header_present: {ns: envoy.lb, key: tier}`
    on a request with header `("x-tier","prod")` → `Continue` AND `req.dynamic_metadata["envoy.lb"]["tier"] == "prod"`.
  - `present_static_value_overrides_header` (§A3): `on_header_present: {key: tier, value: forced}` + header `x-tier: prod`
    → writes `forced` (not `prod`).
  - `missing_writes_fallback`: header ABSENT + `on_header_missing: {key: tier, value: dflt}` → writes `dflt`.
  - `present_but_empty_writes_nothing` (§A4): header `("x-tier","")` (empty) → neither action fires → namespace/key UNSET.
  - `case_insensitive_header_match`: config `header: X-Tier`, request header `("x-tier","prod")` → matches → writes `prod`.
  - `multi_rule_composes`: two rules → two namespaces written.
  - `encode_is_inert`: `encode_headers` → `Continue`, response untouched.
- [ ] **Step 2: Run to verify they fail.** `cargo test -p envoy-filter header_to_metadata` → FAIL (module absent).
- [ ] **Step 3: Implement** `header_to_metadata.rs`:
  ```rust
  //! The `envoy.filters.http.header_to_metadata` filter (phase 34; §A-LOCKED against
  //! envoyproxy/envoy:v1.33.0). Decode-side, Continue-only: for each request_rule, extract the
  //! matched request header's value (or the rule's static `value` override — §A3 precedence) into
  //! `req.dynamic_metadata[namespace][key]`; an absent header applies on_header_missing's static
  //! `value`; a present-but-empty header writes nothing (§A4). Encode inert.
  use crate::pipeline::Decision;
  use crate::types::{FilterRequest, FilterResponse};

  #[derive(Debug, Clone)]
  pub struct HeaderToMetadataFilter {
      rules: Vec<envoy_config::HeaderToMetadataRule>,
  }
  impl HeaderToMetadataFilter {
      pub(crate) fn new(cfg: &envoy_config::HeaderToMetadataConfig) -> Self {
          Self { rules: cfg.request_rules.clone() }
      }
      pub(crate) fn decode_headers(&mut self, req: &mut FilterRequest) -> Decision {
          for rule in &self.rules {
              // Case-insensitive header lookup (HTTP header names are case-insensitive).
              let found = req.headers.iter()
                  .find(|(n, _)| n.eq_ignore_ascii_case(&rule.header))
                  .map(|(_, v)| v.clone());
              let action = match found.as_deref() {
                  Some(v) if !v.is_empty() => rule.on_header_present.as_ref().map(|kv| (kv, v.to_string())),
                  Some(_) => None, // present-but-empty → nothing (§A4)
                  None => rule.on_header_missing.as_ref().map(|kv| {
                      // validated: on_header_missing.value is Some
                      (kv, kv.value.clone().unwrap_or_default())
                  }),
              };
              if let Some((kv, header_value)) = action {
                  // §A3: static `value` wins over the header value.
                  let to_write = kv.value.clone().unwrap_or(header_value);
                  req.dynamic_metadata
                      .entry(kv.metadata_namespace.clone()).or_default()
                      .insert(kv.key.clone(), to_write);
              }
          }
          Decision::Continue
      }
      pub(crate) fn encode_headers(&mut self, _resp: &mut FilterResponse) -> Decision {
          Decision::Continue
      }
  }
  ```
  (Adjust `req.headers` access to the actual `FilterRequest.headers` shape — confirm it is `Vec<(String, String)>`;
  follow how `header_mutation.rs` / `rbac.rs` read request headers. The §A3 precedence is encoded by
  `kv.value.clone().unwrap_or(header_value)` — static value wins, header value is the fallback.) Wire `pub mod
  header_to_metadata;` + `pub use header_to_metadata::HeaderToMetadataFilter;` into `lib.rs`.
- [ ] **Step 4: Run to verify pass.** `cargo test -p envoy-filter header_to_metadata` → PASS (the filter compiles
  standalone; the `instance.rs` match is closed in Task 4).
- [ ] **Step 5: Commit.** `git add crates/envoy-filter/src/header_to_metadata.rs crates/envoy-filter/src/lib.rs && git commit -m "phase 34 T3: HeaderToMetadataFilter (decode-side request-header → dynamic-metadata extraction)"`

---

## Task 4: `HttpFilterInstance::HeaderToMetadata` wiring (the 12th variant)

> The 4 enum/dispatch wirings: variant, `build` arm, `decode_headers`/`encode_headers` arms, `apply_route_config`
> fall-through (no per-route config this phase). Closes the non-exhaustive match opened by Task 1.

**Files:** Modify `crates/envoy-filter/src/instance.rs`.

- [ ] **Step 1: Write the failing test** in `instance.rs` test module (model on `builds_set_metadata_instance_and_writes`):
  `builds_header_to_metadata_instance_and_writes`: build an `HttpFilter` (`name: envoy.filters.http.header_to_metadata`,
  a `HeaderToMetadata` typed_config: `header: x-tier, on_header_present: {ns: envoy.lb, key: tier}`); `HttpFilterInstance::build`
  → `HttpFilterInstance::HeaderToMetadata(_)`; `decode_headers` on a request with `x-tier: prod` → `Continue` AND
  `req.dynamic_metadata["envoy.lb"]["tier"] == "prod"`; `encode_headers` → `Continue`.
- [ ] **Step 2: Run to verify it fails.** `cargo test -p envoy-filter builds_header_to_metadata` → FAIL (non-exhaustive match).
- [ ] **Step 3: Implement.** Add `use crate::header_to_metadata::HeaderToMetadataFilter;`, the variant
  `HeaderToMetadata(HeaderToMetadataFilter),` (doc comment mirroring `SetMetadata`), the `build` arm
  (`HttpFilterTypedConfig::HeaderToMetadata(cfg) => Ok(HttpFilterInstance::HeaderToMetadata(HeaderToMetadataFilter::new(cfg)))`),
  the `decode_headers`/`encode_headers` dispatch arms, and (NO new `apply_route_config` arm — add `HeaderToMetadata`
  to the existing no-per-route-config comment list at the `apply_route_config` fall-through, per the `set_metadata`
  precedent; do NOT add a literal `_ => {}` arm).
- [ ] **Step 4: Run to verify pass + whole crate green.** `cargo test -p envoy-filter` → PASS. Then
  `cargo build -p envoy-http1 -p envoy-http2 -p envoy-bin --all-targets` → clean (RED WINDOW CLOSED).
- [ ] **Step 5: Commit.** `git add crates/envoy-filter/src/instance.rs && git commit -m "phase 34 T4: HttpFilterInstance::HeaderToMetadata wiring (12th filter variant)"`

---

## Task 5: H1 + H2 in-process end-to-end backstops

> The threading is UNCHANGED from phase 33 (filter-agnostic — proven there). These backstops prove header_to_metadata's
> output threads to the record + renders, on BOTH codecs (fixture 0042 is H1-only). Model on the phase-33
> `h1_dynamic_metadata_threads_into_access_log` / `h2_...` backstops verbatim, swapping the filter chain.

**Files:** Modify `crates/envoy-http1/src/hcm.rs`; `crates/envoy-http2/src/hcm.rs`.

- [ ] **Step 1: Write the failing tests.**
  - H1 (`h1_header_to_metadata_threads_into_access_log`): build a `[header_to_metadata, router]` pipeline (a helper writing
    `x-tier`→`envoy.lb:tier`), an `HCMConfig` with a `FileSink` whose `log_format` is
    `%DYNAMIC_METADATA(envoy.lb:tier)% / %DYNAMIC_METADATA(envoy.lb:missing)%` against a `direct_response` 200 route;
    drive one H1 GET with header `x-tier: prod`; scrape the line; assert it equals `prod / -\n`.
  - H2 (`h2_header_to_metadata_threads_into_access_log`): the same chain + log_format over the H2 HCM harness
    (`h2::client` + `spawn_h2_hcm`); drive one real H2 GET with `x-tier: prod`; assert `prod / -\n`.
  (If a present + missing-in-one-line assertion is awkward, split into two probes; the key is present→`prod`, absent→`-`.)
- [ ] **Step 2: Run to verify they fail.** `cargo test -p envoy-http1 h1_header_to_metadata` + `cargo test -p envoy-http2 h2_header_to_metadata`
  Expected: FAIL FIRST — actually, since the threading is UNCHANGED and the filter (Task 3) already writes the metadata,
  these may PASS immediately (the wiring is complete after T4). That is ACCEPTABLE — they are regression backstops, not
  red-green drivers for new threading. If they pass on first run, that CONFIRMS the reuse claim (the threading carries
  header_to_metadata's output with no new plumbing). Note this in PROGRESS.md.
- [ ] **Step 3: (No implementation — the threading is reused unchanged.)** If a backstop fails, debug whether the filter
  (T3) or the chain helper is wrong — NOT the threading.
- [ ] **Step 4: Run to verify pass.** `cargo test -p envoy-http1` + `cargo test -p envoy-http2` → PASS (the backstops +
  all pre-existing tests).
- [ ] **Step 5: Commit.** `git add crates/envoy-http1/src/hcm.rs crates/envoy-http2/src/hcm.rs && git commit -m "phase 34 T5: H1+H2 in-process backstops (header_to_metadata threads to access-log record)"`

---

## Task 6: Fixture `0042` differential (header-present + header-missing probes)

> §A6: the STRONG cross-proxy byte-exact target. `direct_response`, deterministic `log_format` with `%DYNAMIC_METADATA%`,
> NO timing operator. The present + missing probe PAIR guards the store round-trip (the missing probe resolves `-` from the
> SAME path that yields the extracted value for the present probe — not an echo).

**Files:** Create the 4 fixture files + `tests/differential/tests/header_to_metadata.rs`.

- [ ] **Step 1: Write the fixture + the differential test (the "failing test").** `envoy.yaml` (mirror 0041; swap the
  filter; distinct mount paths `/tmp/0042-envoy-mount/access.log` / `/tmp/0042-envoy-rust-mount/access.log`): an H1
  `direct_response` 200 listener; filter chain `[header_to_metadata, router]` where the rule extracts `x-tier` →
  `envoy.lb:tier` (`on_header_present: {metadata_namespace: envoy.lb, key: tier}`, `on_header_missing: {metadata_namespace:
  envoy.lb, key: tier, value: "missing"}`); a file logger whose `log_format.text_format_source.inline_string` is
  **(NOTE: quote the fallback `value: "missing"` — a BARE word like `none`/`null`/`~` risks YAML parsing as null →
  `value: None` → the A5d "on_header_missing requires value" validator → boot-fatal. Use an explicit quoted string.)**
  `"m=%REQ(:METHOD)% tier=%DYNAMIC_METADATA(envoy.lb:tier)% missns=%DYNAMIC_METADATA(envoy.absent:k)%\n"`; `0.0.0.0` bind +
  admin (port 0) + `generate_request_id: false`. `envoy-rust.yaml` = the 0041 per-side convention (`127.0.0.1`, no admin).
  `expectations.yaml` (`kind: http1_access_log_byte_exact`; both proxies' paths; **2 probes via `extra_headers`**):
  ```yaml
  probes:
    # Expected: m=GET tier=prod missns=-
    - { method: get, path: /a, host: envoy-rust.test, extra_headers: [["x-tier", "prod"]] }
    # Header ABSENT → on_header_missing → tier=missing. Expected: m=GET tier=missing missns=-
    - { method: get, path: /b, host: envoy-rust.test }
  ```
  (Confirm the exact `extra_headers` probe field name against `tests/differential/src/lib.rs` `AccessLogByteExactProbe`.)
  `README.md`: pin §A1 (`@type …v3.Config`), §A2 (default namespace — but the fixture sets `envoy.lb` explicitly), §A3
  (raw unquoted; static-value precedence), §A4 (on_header_missing fallback + absent `-`), §A6 determinism; document the
  present/missing probe pair as the anti-echo guard. The test `header_to_metadata.rs` mirrors
  `set_metadata_dynamic_metadata.rs` (calls `differential::run_fixture` on the `0042-…` dir).
- [ ] **Step 2: Run to verify (Docker-gated).** `cargo test -p differential header_to_metadata -- --include-ignored`
  Expected on this Docker host: GREEN once Tasks 1–4 are in (access-log file-scrape is locally authoritative per memory
  `host-docker-desktop-virtiofs-no-inotify`). REBUILD `envoy-bin` first (`cargo build -p envoy-bin`) — a stale binary
  fails with `unknown filter` (the phase-33 T11 lesson).
- [ ] **Step 3: (Implementation already done in Tasks 1–4.)** Adjust the `log_format`/probes only if a live byte
  mismatch surfaces; the §A facts predict a clean match.
- [ ] **Step 4: Run to verify pass.** Confirm both proxies' scraped lines byte-identical for both probes. Run
  `cargo build -p differential --tests` (no other fixture regressed at compile).
- [ ] **Step 5: Commit.** `git add tests/fixtures/0042-http-header-to-metadata tests/differential/tests/header_to_metadata.rs && git commit -m "phase 34 T6: fixture 0042 header_to_metadata byte-exact differential (present+missing)"`

---

## Task 7: BEHAVIOR_CONTRACT extension + `parse_bootstrap` fuzz seed

> Document the new filter + the §A facts; add a fuzz seed to the EXISTING `parse_bootstrap` target (NO new target — §A §3.8).

**Files:** Modify `docs/envoy-rust/BEHAVIOR_CONTRACT.md`; create a seed under
`crates/envoy-config/fuzz/corpus/parse_bootstrap/`.

- [ ] **Step 1: (No code test — doc + seed task.)** Verify the seed parses: a throwaway
  `envoy_config::parse_bootstrap(seed)` → `Ok` (used + removed; NOT committed).
- [ ] **Step 2: Implement the docs + seed.**
  - **BEHAVIOR_CONTRACT.md:** under the "HTTP filters" + "Access log field mapping" sections, add a phase-34 subsection
    documenting the `header_to_metadata` filter: the `request_rules`/`KeyValuePair` config shape (`@type …v3.Config`,
    request-side string-only subset); the **default namespace `envoy.filters.http.header_to_metadata`** (§A2); the
    on_header_present **static-value-wins** precedence + RAW unquoted byte form (§A3); the on_header_missing
    requires-value + present-but-empty→unset + absent→`-` semantics (§A4); the boot-fatal config-validity (§A5); and the
    §2.2 deferrals (`response_rules`, typed values, `encode: BASE64`, `regex_value_rewrite`, `remove`, per-route).
  - **Fuzz seed:** `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_header_to_metadata.yaml` — a minimal valid
    bootstrap (concrete `port_value`, no `{{PORT}}`) with a `header_to_metadata` filter (`@type …v3.Config`, one
    request_rule) + a router + a file logger with a `%DYNAMIC_METADATA(envoy.lb:tier)%` `log_format`. Modeled on fixture
    0042's `envoy-rust.yaml`. (NO new fuzz target; NO ci.yml change — memory `new-fuzz-target-needs-a-ci-yml-step`
    satisfied; confirm by inspection.)
- [ ] **Step 3: Run to verify.** `cargo build -p envoy-config` (the seed is data, not code — confirm the crate still
  builds). The §7.5 gate (d) short-budget fuzz run happens at state-4.
- [ ] **Step 4: (No test run beyond the build.)** Optionally `cargo +nightly fuzz run parse_bootstrap -- -runs=0` loads the corpus.
- [ ] **Step 5: Commit.** `git add docs/envoy-rust/BEHAVIOR_CONTRACT.md crates/envoy-config/fuzz/corpus/parse_bootstrap/ && git commit -m "phase 34 T7: BEHAVIOR_CONTRACT header_to_metadata + parse_bootstrap fuzz seed"`

---

## §7.5 acceptance gate (state-4 verification — previewed)

A phase-34 state-4 verification (the next-but-one session) must show ALL green:
- **(a)** fixture `0042` green (cross-proxy byte-identical access-log line; header-present extracted value +
  header-missing fallback + absent `-`).
- **(b)** all of `0001`–`0041` green (incl. `0012` default-format + `0041` set_metadata UNCHANGED — the
  regression-equivalence witnesses).
- **(c)** h2spec ≥95% (unchanged — no HTTP/2 codec change). **NOTE (memory `h2spec-3-5-2-preface-host-sensitive`):**
  the §3.5/2 known-failure false-REDs LOCALLY; CI is authoritative.
- **(d)** the EXISTING `parse_bootstrap` (+ `accesslog_format_parse`, unchanged) fuzz targets clean for the short-budget
  CI run (with the new seed) — **NO new fuzz target** (§A §3.8); confirm NO ci.yml change.
- **(e)** `cargo build --workspace --all-targets` / `cargo clippy --workspace --all-targets --all-features -- -D
  warnings` / `cargo fmt --all -- --check` / `cargo test --workspace` / `cargo deny check` all clean. **Per memory
  `envoy-rust-state4-ci-first-execution`: state-4 is CI's first real execution — budget a possible cross-crate clippy
  nit / fmt drift; fix in-place.** **PUSH per-session from state-3 onward + confirm CI green** (do NOT let code commits
  accumulate unpushed — the phase-33 lesson).
- **(f)** `REVIEW.md` approved.

`#![forbid(unsafe_code)]` holds (D-3.8). No new crate, no new dependency (D-3.2). The backend-routing/upstream/admin-dump
differential fixtures false-RED LOCALLY (Docker bridge IP `192.168.65.2`); CI is authoritative (memories
`differential-host-bridge-ip-192-168-65-2`, `host-docker-desktop-virtiofs-no-inotify`).

---

_§A facts locked by **ADR-0084** (the §6.2 reconciliation). Scope locked by **ADR-0083**. The §6.1 split did NOT fire.
The state-3 implementation is the next session (`superpowers:subagent-driven-development`)._
