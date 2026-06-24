# Phase 35 — `35-rbac-dynamic-metadata` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
> Every task is TDD: write the failing test FIRST, watch it fail, implement minimally, watch it pass, commit.

**Goal:** Extend the EXISTING phase-10 `envoy.filters.http.rbac` filter with a `metadata` Permission/Principal
condition — a string-only `MetadataMatcher` that reads `req.dynamic_metadata` written mid-chain by an upstream
producer filter (phase-34 `header_to_metadata`) — landing the FIRST dynamic-metadata CONSUMER, proven by a
byte-exact cross-proxy verdict (fixture `0043`: `X-Tier: prod`→`200`+`ok\n` / `dev`/absent→`403`+`RBAC: access
denied`), with all 42 pre-existing fixtures unchanged.

**Architecture:** ADDS NO new `HttpFilterInstance` variant (RBAC is the phase-10 variant) and NO new infrastructure.
Config: a `Metadata(MetadataMatcher)` arm on BOTH the `Permission` and `Principal` hand-rolled "exactly one map key"
`Deserialize` visitors (`crates/envoy-config/src/bootstrap.rs`) + a `MetadataMatcher { filter, path:
Vec<MetadataPathSegment{ key }>, value: ValueMatcher }` struct trio with a string-only `ValueMatcher` enum (only
variant `StringMatch(StringMatcher)`, reusing the 04.x `StringMatcher` verbatim), all `deny_unknown_fields`. Runtime:
`RuntimePermission::Metadata`/`RuntimePrincipal::Metadata` variants (`crates/envoy-filter/src/rbac.rs`) lowered by the
existing `lower_permission`/`lower_principal`, plus an `eval` arm reading
`req.dynamic_metadata.get(&m.filter).and_then(|ns| ns.get(&m.path[0].key)).is_some_and(|v| m.value.matches(v))`. The
RBAC decision matrix + the `403` + `b"RBAC: access denied"` local reply + the stats + the decode pipeline are
UNCHANGED. The load-bearing mechanism is the decode pipeline's shared `&mut FilterRequest` threading
(`pipeline.rs:77`): a consumer filter reads what an earlier producer wrote IN THE SAME PASS, so the fixture chain
order `[header_to_metadata, rbac, router]` (producer before consumer) is REQUIRED.

**Tech Stack:** Rust (pinned toolchain, `#![forbid(unsafe_code)]` workspace-wide), `serde`/`serde_yaml`
(hand-rolled visitors + `deny_unknown_fields`), `thiserror`, the phase-10 RBAC engine + the 04.x `StringMatcher`
(reused), the phase-33 dynamic-metadata store + the phase-34 `header_to_metadata` producer + the decode-pipeline
threading (reused UNCHANGED), `tests/differential` (the fixture-`0017` `http1_probe_list` status+body comparator
with the `extra_headers` probe), `cargo fuzz` (existing `parse_bootstrap` target — NO new target).

---

## §A — Empirically-locked facts (the §6.2 ground truth; reconciled by ADR-0086)

Run LOCALLY at this PLAN-write against live `envoyproxy/envoy:v1.33.0` (the ENVOY_TARGET pin `sha256:56da5afd…`) —
an H1 `direct_response` listener, a `[header_to_metadata, rbac, router]` chain with an `action: ALLOW` single-policy
`metadata` Permission/Principal; `curl` probes with/without `X-Tier`; bytes via `od -c`; config round-tripped through
`/config_dump`; malformed variants via `--mode validate`; live re-probes of the Principal placement, a custom
namespace, a reversed chain, and a `prefix` matcher. **ADR-0086 FIRES** — **THREE MATERIAL divergences** from the SPEC
projection (the deprecated-but-functional `metadata` field; the multi-segment-path acceptance; the non-`string_match`
value acceptance), plus the headline confirmations. These facts are LOCKED; the tasks below encode them. Do NOT
re-derive.

**A1 — wire shape (CONFIRMED).** A `metadata` Permission/Principal entry is
`{ metadata: { filter: <string>, path: [{ key: <string> }, …], value: { string_match: <StringMatcher> } } }`. The
field names `filter` / `path` / `key` / `value` / `string_match` round-trip VERBATIM through `/config_dump`
(snake_case). The matcher is accepted under BOTH `permissions[]` and `principals[]` (identical shape; `prod`→200 /
`dev`→403 under each placement). The `value` field is **REQUIRED** (Envoy: `MetadataMatcherValidationError.Value:
value is required` → boot-fatal when omitted). The MVP models `MetadataMatcher { filter: String, path:
Vec<MetadataPathSegment{ key: String }>, value: ValueMatcher }` + a string-only `ValueMatcher` enum (only variant
`StringMatch(StringMatcher)`, reusing the 04.x `StringMatcher` verbatim), all `#[serde(deny_unknown_fields)]`.

**A2 — `filter`→namespace correspondence (CONFIRMED).** `MetadataMatcher.filter` is matched against the
dynamic-metadata namespace key (the store's outer `BTreeMap` key — what `header_to_metadata`'s `metadata_namespace`
writes). The phase-34 default producer namespace **`envoy.filters.http.header_to_metadata`** (ADR-0084) is matchable;
a custom `metadata_namespace` (tested `my.custom.ns`) is matchable by an equal `filter`. **Producer-before-consumer
chain order is REQUIRED:** a reversed `[rbac, header_to_metadata, router]` chain evaluates RBAC against EMPTY metadata
→ `X-Tier: prod` gets `403` (no match under ALLOW). **Fixture 0043 MUST order `[header_to_metadata, rbac, router]`.**

**A3 — match semantics + verdicts (CONFIRMED byte-exact, = phase-10/ADR-0034 ground truth).**
- `X-Tier: prod` → store `tier=prod` → match → ALLOW → **`200` + `ok\n`** (3 bytes, the `direct_response` body).
- `X-Tier: dev` → `tier=dev` → no match → DENY → **`403` + `RBAC: access denied`** (19 bytes, NO trailing newline,
  `od -c` confirmed; the phase-10/ADR-0034 body, reused verbatim).
- Absent `X-Tier` → key UNSET → no match → `403`+19B. Present-but-empty `X-Tier;` → `header_to_metadata` writes
  nothing (§A4 phase-34) → key UNSET → no match → `403`+19B.
- **StringMatcher modes:** the FULL 04.x set flows through (`exact` AND `prefix` BOTH confirmed live: `prefix: pro` →
  `prod` 200 / `dev` 403). **REUSE the `StringMatcher` verbatim — do NOT restrict the MVP to `exact`.**
- **Runtime eval:** `req.dynamic_metadata.get(&m.filter).and_then(|ns| ns.get(&m.path[0].key)).is_some_and(|v|
  m.value.matches(v))` — absent namespace OR absent key → `false` (no match).

**A4 — config-validity (CONFIRMED boot-fatal where Envoy rejects; ADR-0049):**
- empty `filter: ""` → Envoy `MetadataMatcherValidationError.Filter: value length must be at least 1 characters`.
- missing `filter` → same constraint (boot-fatal).
- empty `path: []` → Envoy `MetadataMatcherValidationError.Path: value must contain at least 1 item(s)`.
- missing `value` → Envoy `MetadataMatcherValidationError.Value: value is required`.

envoy-rust matches via ONE new `ConfigError::RbacMetadataMatcherInvalid { listener, policy_name, path, detail }`
(checked in the `Metadata` arm of `validate_permission_tree`/`validate_principal_tree` — empty `filter` + the §A5
path-length≠1 reject) + serde (`value` is a required non-`Option` field → missing → serde error; `path`/`filter` are
required fields → missing → serde error; `deny_unknown_fields` on the structs).

**A5 — MATERIAL DIVERGENCE: multi-segment `path`.** Envoy ACCEPTS a multi-segment `path: [{ key: tier }, { key: sub }]`
(the SPEC projected boot-fatal). The flat string-only store (`BTreeMap<String, BTreeMap<String, String>>`) cannot
resolve a nested path, so **envoy-rust MVP is STRICTER: a `path` whose length ≠ 1 is BOOT-FATAL**
(`ConfigError::RbacMetadataMatcherInvalid`, detail `"metadata matcher path must have exactly one segment"`) — a
documented stricter-than-Envoy divergence (the phase-34 `cookie`/typed-values precedent). The nested path is the §2.2
deferral (rides the future structured-Value generalization).

**A6 — MATERIAL DIVERGENCE: non-`string_match` `value`.** Envoy ACCEPTS the full `ValueMatcher` oneof (`present_match`,
`null_match`, `double_match`, `bool_match`, `list_match`, `or_match`) — tested `present_match: true` → accepted. The
string-only MVP `ValueMatcher` rejects any non-`string_match` key BOOT-FATAL via its hand-rolled "exactly one key"
visitor (`unknown_field` → serde error) — a documented stricter-than-Envoy divergence (the phase-34 non-`STRING`
`type` precedent). `present_match` is the cheapest §2.2 additive follow-up.

**A7 — MATERIAL DIVERGENCE: deprecation (NON-DIFFERENTIAL).** `envoy.config.rbac.v3.Permission.metadata` AND
`.Principal.metadata` are DEPRECATED in v1.33.0 — Envoy boots with a `warning` ("Using deprecated option … will be
removed from Envoy soon") but the fields are FULLY FUNCTIONAL and accepted at the pin (both verdicts correct). The
warning is stderr-only → **NON-DIFFERENTIAL** (no response / access-log / stats impact; envoy-rust simply does not
emit it). Land the matcher as-is at the pin; the future pin-refresh phase (D-3.7) inherits the flag that a later
Envoy may remove the field.

---

## File structure (what each file is responsible for)

- **`crates/envoy-config/src/bootstrap.rs`** — the config schema: the `MetadataMatcher`/`MetadataPathSegment`/
  `ValueMatcher` structs + the `ValueMatcher` hand-rolled `Deserialize`; the `Metadata(MetadataMatcher)` arm on the
  `Permission` and `Principal` enums + their visitors' `"metadata"` key + `KEYS` lists; the `Metadata` arm on
  `validate_permission_tree`/`validate_principal_tree`.
- **`crates/envoy-config/src/lib.rs`** — the `ConfigError::RbacMetadataMatcherInvalid` variant; the re-exports of
  `MetadataMatcher`/`MetadataPathSegment`/`ValueMatcher`.
- **`crates/envoy-config/src/matcher.rs`** — `impl ValueMatcher { pub fn matches(&self, value: &str) -> bool }`
  (delegates to the inner `StringMatcher::matches`).
- **`crates/envoy-filter/src/rbac.rs`** — the `RuntimePermission::Metadata`/`RuntimePrincipal::Metadata` variants +
  the `eval` arms (a shared `eval_metadata` helper) + the `lower_permission`/`lower_principal` arms; the in-process
  backstop tests.
- **`tests/fixtures/0043-http-rbac-dynamic-metadata/`** — the 4 fixture files (`envoy.yaml`, `envoy-rust.yaml`,
  `expectations.yaml`, `README.md`).
- **`tests/differential/tests/rbac_dynamic_metadata.rs`** — the differential test entry (mirrors the fixture-0017 RBAC
  test; `differential::run_fixture` on the `0043-…` dir).
- **`docs/envoy-rust/BEHAVIOR_CONTRACT.md`** — the phase-35 "HTTP filters" subsection.
- **`crates/envoy-config/fuzz/corpus/parse_bootstrap/`** — one seed exercising a `[header_to_metadata, rbac]`
  `metadata`-matcher chain (NO new fuzz target).

**Sequencing note (cross-crate red window).** Task 1 adds the `Permission::Metadata`/`Principal::Metadata` config
variants; `crates/envoy-filter`'s `lower_permission`/`lower_principal`/`eval_*` matches become non-exhaustive until
Task 3 closes them. Run Tasks 1→2→3 contiguously; `cargo build -p envoy-filter` is red only between T1 and T3. Gate
T1/T2 on `cargo test -p envoy-config`, T3 on `cargo test -p envoy-filter rbac`. If the harness demands a
workspace-green commit per task, fold T1+T2+T3.

---

## Task 1: Config schema — the `MetadataMatcher` trio + the `Permission`/`Principal` `"metadata"` visitor arms

> §A1/A6: `metadata: { filter, path: [{ key }], value: { string_match: <StringMatcher> } }`; `value` required;
> string-only `ValueMatcher` (hand-rolled "exactly one key" visitor rejects `present_match` etc. boot-fatal); accepted
> under BOTH `permissions` and `principals`; all `deny_unknown_fields`.

**Files:** Modify `crates/envoy-config/src/bootstrap.rs` (structs + `ValueMatcher` Deserialize + the two enum variants
+ the two visitor arms + `KEYS`); `crates/envoy-config/src/lib.rs` (re-exports).

- [ ] **Step 1: Write the failing tests** in `bootstrap.rs` test module:
  ```rust
  #[test]
  fn parses_rbac_metadata_permission() {
      let yaml = r#"
  rules:
    action: ALLOW
    policies:
      tier_prod:
        permissions:
          - metadata:
              filter: envoy.filters.http.header_to_metadata
              path:
                - key: tier
              value:
                string_match: { exact: "prod" }
        principals:
          - any: true
  "#;
      let cfg: RbacConfig = serde_yaml::from_str(yaml).expect("parses");
      let policy = &cfg.rules.policies["tier_prod"];
      match &policy.permissions[0] {
          Permission::Metadata(m) => {
              assert_eq!(m.filter, "envoy.filters.http.header_to_metadata");
              assert_eq!(m.path.len(), 1);
              assert_eq!(m.path[0].key, "tier");
              assert!(m.value.matches("prod"));   // ValueMatcher::matches (added in matcher.rs — see Step 3)
              assert!(!m.value.matches("dev"));
          }
          other => panic!("expected Metadata, got {other:?}"),
      }
  }
  ```
  Plus `parses_rbac_metadata_principal`: the same matcher under `principals:` → `Principal::Metadata(m)` (§A1, accepted
  under both). Plus `rbac_metadata_rejects_present_match_value`: a `value: { present_match: true }` →
  `serde_yaml::from_str::<RbacConfig>` returns `Err` (§A6 — the string-only `ValueMatcher` visitor rejects the unknown
  key). Plus `rbac_metadata_rejects_unknown_field`: a `metadata` with `invert: true` (the deferred field) → `Err`
  (`deny_unknown_fields`).
- [ ] **Step 2: Run to verify they fail.** `cargo test -p envoy-config parses_rbac_metadata rbac_metadata_rejects`
  Expected: FAIL — no `Metadata` variant / `MetadataMatcher` / `ValueMatcher` types.
- [ ] **Step 3: Implement.** In `bootstrap.rs`, add (near `RbacConfig`/`Permission`):
  ```rust
  /// RBAC `metadata` matcher (phase 35, §A1-LOCKED). Reads a single-segment dynamic-metadata
  /// path. `filter` is the namespace (the producer's `metadata_namespace`, §A2). The MVP models
  /// a single `path` segment + a string-only `value` (§A5/A6: multi-segment path + non-string_match
  /// value are stricter-than-Envoy boot-fatal). `value` is REQUIRED (§A4).
  #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
  #[serde(deny_unknown_fields)]
  pub struct MetadataMatcher {
      pub filter: String,
      pub path: Vec<MetadataPathSegment>,
      pub value: ValueMatcher,
  }

  #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
  #[serde(deny_unknown_fields)]
  pub struct MetadataPathSegment {
      pub key: String,
  }

  /// Envoy `type.matcher.v3.ValueMatcher` (string-only MVP, §A6). The hand-rolled "exactly one
  /// key" Deserialize accepts ONLY `string_match`; any other oneof key (`present_match`, …) →
  /// `unknown_field` → boot-fatal (stricter than Envoy, which accepts them). Mirrors the
  /// `Permission`/`StringMatcher` visitor template.
  #[derive(Debug, Clone, PartialEq, Serialize)]
  pub enum ValueMatcher {
      #[serde(rename = "string_match")]
      StringMatch(StringMatcher),
  }

  impl<'de> serde::Deserialize<'de> for ValueMatcher {
      fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
      where D: serde::Deserializer<'de> {
          use serde::de::{Error, MapAccess, Visitor};
          use std::fmt;
          const KEYS: &[&str] = &["string_match"];
          struct V;
          impl<'de> Visitor<'de> for V {
              type Value = ValueMatcher;
              fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                  write!(f, "a ValueMatcher map with exactly one of {KEYS:?} as key")
              }
              fn visit_map<M>(self, mut map: M) -> Result<ValueMatcher, M::Error>
              where M: MapAccess<'de> {
                  let key: String = map.next_key()?
                      .ok_or_else(|| M::Error::custom("ValueMatcher: expected one map key, got none"))?;
                  let value = match key.as_str() {
                      "string_match" => ValueMatcher::StringMatch(map.next_value::<StringMatcher>()?),
                      other => return Err(M::Error::unknown_field(other, KEYS)),
                  };
                  if map.next_key::<String>()?.is_some() {
                      return Err(M::Error::custom("ValueMatcher: expected exactly one map key, got more"));
                  }
                  Ok(value)
              }
          }
          deserializer.deserialize_map(V)
      }
  }
  ```
  Add the `Metadata` variant to the `Permission` enum (after `NotRule`) with `#[serde(rename = "metadata")]
  Metadata(MetadataMatcher)`, add `"metadata"` to its visitor's `KEYS` const, and add the arm to its `visit_map`:
  ```rust
  "metadata" => Permission::Metadata(map.next_value::<MetadataMatcher>()?),
  ```
  Do the SAME for the `Principal` enum (variant `Metadata(MetadataMatcher)`, `KEYS` += `"metadata"`, arm
  `"metadata" => Principal::Metadata(map.next_value::<MetadataMatcher>()?)`). Re-export the three new types from
  `lib.rs` (`pub use bootstrap::{MetadataMatcher, MetadataPathSegment, ValueMatcher, …}`, keep alphabetical).
  (The `ValueMatcher::matches` method lands in Task 1 Step 3b below — it is needed by the Step 1 test.)
- [ ] **Step 3b: Add `ValueMatcher::matches`** to `crates/envoy-config/src/matcher.rs`:
  ```rust
  use crate::bootstrap::ValueMatcher;
  impl ValueMatcher {
      /// Returns true iff the resolved metadata value matches. String-only MVP: delegates to the
      /// inner `StringMatcher::matches` (phase 35, §A3).
      pub fn matches(&self, value: &str) -> bool {
          match self {
              ValueMatcher::StringMatch(sm) => sm.matches(value),
          }
      }
  }
  ```
- [ ] **Step 4: Run to verify pass.** `cargo test -p envoy-config parses_rbac_metadata rbac_metadata_rejects` → PASS.
  Then `cargo build -p envoy-filter` → **EXPECTED FAIL** (non-exhaustive `lower_permission`/`eval_permission` matches —
  closed in Task 3). See the sequencing note.
- [ ] **Step 5: Commit.** `git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs crates/envoy-config/src/matcher.rs && git commit -m "phase 35 T1: RBAC metadata MetadataMatcher/ValueMatcher schema + Permission/Principal visitor arms [ADR-0086]"`

---

## Task 2: Validator arm + `ConfigError::RbacMetadataMatcherInvalid`

> §A4/A5: empty `filter` → boot-fatal; `path.len() != 1` → boot-fatal (stricter than Envoy, which accepts
> multi-segment). Checked in the `Metadata` arm of `validate_permission_tree`/`validate_principal_tree`.

**Files:** Modify `crates/envoy-config/src/lib.rs` (the `ConfigError` variant); `crates/envoy-config/src/bootstrap.rs`
(the `Metadata` arm on both tree validators).

- [ ] **Step 1: Write the failing tests** in `bootstrap.rs` test module, driven through the full `parse_bootstrap(&yaml)`
  entry-point (model on the existing `EmptyRbacPermissionSet`/`RbacTreeTooDeep` validator tests — a minimal HCM bootstrap
  with one `rbac` filter + a router):
  - `rbac_metadata_empty_filter_is_fatal`: a `metadata` Permission with `filter: ""` →
    `Err(ConfigError::RbacMetadataMatcherInvalid { .. })`.
  - `rbac_metadata_multi_segment_path_is_fatal`: a `path: [{ key: tier }, { key: sub }]` → `Err(…RbacMetadataMatcherInvalid)`
    (§A5 — stricter than Envoy).
  - `rbac_metadata_empty_path_is_fatal`: a `path: []` → `Err(…RbacMetadataMatcherInvalid)` (caught by the same
    `len != 1` check).
  - `rbac_metadata_principal_empty_filter_is_fatal`: the same empty-`filter` matcher under `principals` → `Err`
    (the symmetric Principal validator arm).
  - `rbac_metadata_valid_single_segment_ok`: a well-formed single-segment matcher → `Ok` (the positive control).
- [ ] **Step 2: Run to verify they fail.** `cargo test -p envoy-config rbac_metadata_empty rbac_metadata_multi rbac_metadata_principal rbac_metadata_valid` → FAIL.
- [ ] **Step 3: Implement.** In `lib.rs` `ConfigError` (near `RbacTreeTooDeep`):
  ```rust
  /// Phase 35 (§A4/A5-LOCKED): an RBAC `metadata` matcher is malformed — an empty `filter`
  /// (Envoy: PGV min_len 1) or a `path` whose length is not exactly 1 (Envoy accepts a
  /// multi-segment path; envoy-rust's flat string store cannot resolve it → stricter boot-fatal).
  /// Both are config-load-time fatal (ADR-0049).
  #[error("HCM listener {listener:?}: RBAC policy {policy_name:?} metadata matcher at {path:?} is invalid: {detail}")]
  RbacMetadataMatcherInvalid {
      listener: String,
      policy_name: String,
      path: String,
      detail: String,
  },
  ```
  In `bootstrap.rs`, add the `Metadata` arm to `validate_permission_tree` (a leaf — no recursion):
  ```rust
  crate::Permission::Metadata(m) => validate_metadata_matcher(m, listener_name, policy_name, path),
  ```
  and the symmetric arm to `validate_principal_tree` (`crate::Principal::Metadata(m) => validate_metadata_matcher(...)`).
  Add the shared helper:
  ```rust
  fn validate_metadata_matcher(
      m: &crate::MetadataMatcher,
      listener_name: &str,
      policy_name: &str,
      path: &str,
  ) -> Result<(), crate::ConfigError> {
      let bad = |detail: String| crate::ConfigError::RbacMetadataMatcherInvalid {
          listener: listener_name.to_string(),
          policy_name: policy_name.to_string(),
          path: path.to_string(),
          detail,
      };
      if m.filter.is_empty() {
          return Err(bad("metadata matcher `filter` must not be empty".into()));
      }
      if m.path.len() != 1 {
          return Err(bad(format!(
              "metadata matcher path must have exactly one segment (got {}); multi-segment/nested paths are deferred",
              m.path.len()
          )));
      }
      Ok(())
  }
  ```
- [ ] **Step 4: Run to verify pass.** `cargo test -p envoy-config rbac_metadata` → PASS (all parse + validator tests).
- [ ] **Step 5: Commit.** `git add crates/envoy-config/src/lib.rs crates/envoy-config/src/bootstrap.rs && git commit -m "phase 35 T2: validate metadata matcher (empty filter + path-len!=1 boot-fatal) + ConfigError::RbacMetadataMatcherInvalid [ADR-0086]"`

---

## Task 3: Runtime variants + eval + lowering (`rbac.rs`)

> §A3: `RuntimePermission::Metadata`/`RuntimePrincipal::Metadata` holding the config `MetadataMatcher`; eval reads
> `req.dynamic_metadata.get(&m.filter)?.get(&m.path[0].key)` and applies `m.value.matches`; lowered by a clone.

**Files:** Modify `crates/envoy-filter/src/rbac.rs` (the two runtime enums + the two evaluators + the two lowering fns).

- [ ] **Step 1: Write the failing tests** in `rbac.rs` test module (extend the `req_with` helper to accept metadata):
  ```rust
  fn metadata_matcher(filter: &str, key: &str, exact: &str) -> envoy_config::MetadataMatcher {
      envoy_config::MetadataMatcher {
          filter: filter.to_string(),
          path: vec![envoy_config::MetadataPathSegment { key: key.to_string() }],
          value: envoy_config::ValueMatcher::StringMatch(envoy_config::StringMatcher {
              mode: envoy_config::StringMatcherMode::Exact(exact.to_string()),
              ignore_case: false,
          }),
      }
  }
  // a req helper carrying dynamic_metadata[ns][key] = val
  fn req_with_md(ns: &str, key: &str, val: &str) -> FilterRequest { /* build BTreeMap */ }

  #[test]
  fn metadata_permission_matches_present_value() {
      let req = req_with_md("envoy.filters.http.header_to_metadata", "tier", "prod");
      let perm = RuntimePermission::Metadata(metadata_matcher("envoy.filters.http.header_to_metadata", "tier", "prod"));
      assert!(eval_permission(&perm, &req));
  }
  #[test]
  fn metadata_permission_no_match_on_value_mismatch() { /* tier=dev vs exact prod → false */ }
  #[test]
  fn metadata_permission_no_match_on_absent_namespace() { /* req has no such ns → false */ }
  #[test]
  fn metadata_permission_no_match_on_absent_key() { /* ns present, key absent → false */ }
  #[test]
  fn metadata_principal_mirrors_permission() { /* RuntimePrincipal::Metadata, same matrix */ }
  ```
- [ ] **Step 2: Run to verify they fail.** `cargo test -p envoy-filter metadata_permission metadata_principal` → FAIL
  (no `Metadata` runtime variant).
- [ ] **Step 3: Implement.** Add `Metadata(envoy_config::MetadataMatcher)` to `RuntimePermission` (after `NotRule`) and
  `Metadata(envoy_config::MetadataMatcher)` to `RuntimePrincipal` (after `NotId`) — holding the config type directly, the
  `Header(HeaderMatcher)` precedent. Add the shared eval helper + the two eval arms:
  ```rust
  /// Phase 35: resolve the single-segment metadata path and apply the ValueMatcher. Absent
  /// namespace OR absent key → no match (false). The validator guarantees `path.len() == 1`.
  fn eval_metadata(m: &envoy_config::MetadataMatcher, req: &FilterRequest) -> bool {
      req.dynamic_metadata
          .get(&m.filter)
          .and_then(|ns| ns.get(&m.path[0].key))
          .is_some_and(|v| m.value.matches(v))
  }
  ```
  In `eval_permission`: `RuntimePermission::Metadata(m) => eval_metadata(m, req),`. In `eval_principal`:
  `RuntimePrincipal::Metadata(m) => eval_metadata(m, req),`. In `lower_permission`:
  `envoy_config::Permission::Metadata(m) => RuntimePermission::Metadata(m.clone()),`. In `lower_principal`:
  `envoy_config::Principal::Metadata(m) => RuntimePrincipal::Metadata(m.clone()),`.
- [ ] **Step 4: Run to verify pass.** `cargo test -p envoy-filter metadata_permission metadata_principal` → PASS. Then
  `cargo build -p envoy-filter && cargo test -p envoy-filter rbac` → PASS (the cross-crate red window from T1 closes here).
- [ ] **Step 5: Commit.** `git add crates/envoy-filter/src/rbac.rs && git commit -m "phase 35 T3: RuntimePermission/Principal::Metadata + eval_metadata + lowering (single-segment, absent→no-match) [ADR-0086]"`

---

## Task 4: In-process producer→consumer backstop (the rich deterministic complement)

> §5 SPEC: the richer combinator/Principal/DENY-inversion/mid-chain-thread coverage lives in-process; the cross-proxy
> fixture (Task 5) proves Envoy's EXACT verdict. This task proves the load-bearing mid-chain thread: a `header_to_metadata`
> producer writes metadata that the `rbac` consumer reads IN THE SAME decode pass.

**Files:** Modify `crates/envoy-filter/src/rbac.rs` (or a small backstop module) — an in-process pipeline test using the
real `HeaderToMetadataFilter` + `RbacFilter` through the `FilterPipeline`/`HttpFilterInstance` decode path.

- [ ] **Step 1: Write the failing test(s)** — build a `FilterPipeline` (via `test_from_instances`) with
  `[HttpFilterInstance::HeaderToMetadata(...), HttpFilterInstance::Rbac(...)]` where `header_to_metadata` extracts
  `x-tier`→`<ns>:tier` and RBAC has an `action: ALLOW` policy with a `metadata` Permission requiring `tier == prod`:
  - `mid_chain_producer_then_consumer_allows_prod`: a request `x-tier: prod` driven through `pipeline.decode_headers`
    → `Decision::Continue` (the consumer read the producer's mid-pass write).
  - `mid_chain_producer_then_consumer_denies_dev`: `x-tier: dev` → `Decision::StopAndSend` 403 + `RBAC: access denied`.
  - `mid_chain_absent_header_denies`: no `x-tier` → 403 (key unset → no match).
  - `metadata_composes_in_and_rules`: a `metadata` matcher nested in an `and_rules` with an `any: true` → matches when
    the metadata matches (composition with the recursive combinators).
  - `metadata_principal_and_deny_inversion`: a `metadata` matcher as a Principal under `action: DENY` → inverts (match →
    DENY, no-match → ALLOW).
  (Reuse the phase-34 `HeaderToMetadataFilter` test helpers + the `metadata_matcher` helper from Task 3. **Drive the
  `and_rules`/Principal/DENY composition through the config→`RbacFilter::build_from_config` lowering path** — build an
  `envoy_config::RbacConfig` with the nested `Permission::AndRules(PermissionSet { rules: [...] })` / `Principal::Metadata`
  / `action: Deny` and call `build_from_config`, mirroring the existing `rbac.rs` tests at `build_from_config_*` — NOT by
  hand-constructing `RuntimePermission` variants, so the lowering arms are exercised too.)
- [ ] **Step 2: Run to verify they fail.** `cargo test -p envoy-filter mid_chain metadata_composes metadata_principal_and_deny` → FAIL.
- [ ] **Step 3: Implement.** No new production code — the tests exercise the Task 1–3 surface end-to-end through the
  existing `FilterPipeline::decode_headers` (`pipeline.rs:77`). If `test_from_instances` is `#[cfg(feature = "test-util")]`,
  gate the test module on that feature (confirm against the existing instance.rs integration tests).
- [ ] **Step 4: Run to verify pass.** `cargo test -p envoy-filter` → PASS (full crate, no regression).
- [ ] **Step 5: Commit.** `git add crates/envoy-filter/src/rbac.rs && git commit -m "phase 35 T4: in-process producer->consumer backstop (mid-chain thread, compose, Principal, DENY-inversion) [ADR-0085]"`

---

## Task 5: Fixture `0043` differential (header-driven metadata → RBAC verdict)

> §A3: the STRONG cross-proxy byte-exact target. `[header_to_metadata, rbac, router]` (producer BEFORE consumer — §A2,
> REQUIRED), `direct_response`, `action: ALLOW` single-policy `metadata` Permission requiring `tier == prod`. The
> present-match + mismatch + absent probe TRIO guards the store round-trip (the deny probes reach the SAME lookup path
> and fail the match — not a trivial allow-all).

**Files:** Create the 4 fixture files + `tests/differential/tests/rbac_dynamic_metadata.rs`.

- [ ] **Step 1: Write the fixture + the differential test (the "failing test").** `envoy.yaml` (mirror 0017's RBAC
  per-side convention — `0.0.0.0` bind + admin port 0 + `generate_request_id: false`; H1 `direct_response` 200 `ok\n`):
  filter chain `[header_to_metadata, rbac, router]` where `header_to_metadata` has one rule `{ header: x-tier,
  on_header_present: { metadata_namespace: envoy.filters.http.header_to_metadata, key: tier } }` and `rbac` has
  `action: ALLOW` + one policy `tier_prod` with a `metadata` Permission `{ filter:
  envoy.filters.http.header_to_metadata, path: [{ key: tier }], value: { string_match: { exact: "prod" } } }` paired
  with an `any: true` Principal. **(NOTE: quote `"prod"` and the header values — a bare `prod` is fine but keep the
  fixture quoting consistent with 0017's `string_match: { exact: "yes" }`.)** `envoy-rust.yaml` = the 0017 per-side
  convention (`127.0.0.1`, no admin block). `expectations.yaml` (`kind: http1_probe_list`; the fixture-0017 driver; **3
  probes via `extra_headers`**):
  ```yaml
  driver:
    kind: http1_probe_list
    probes:
      - name: probe-1-allow-tier-prod
        method: get
        path: /
        host: envoy-rust.test
        extra_headers: [[x-tier, "prod"]]
        expected_status: 200
        expected_body: { kind: byte_exact, body: "ok\n" }
        expected_headers: set_equal_modulo_allow_list
      - name: probe-2-deny-tier-dev
        method: get
        path: /
        host: envoy-rust.test
        extra_headers: [[x-tier, "dev"]]
        expected_status: 403
        expected_body: { kind: byte_exact, body: "RBAC: access denied" }
        expected_headers: set_equal_modulo_allow_list
      - name: probe-3-deny-header-absent
        method: get
        path: /
        host: envoy-rust.test
        extra_headers: []
        expected_status: 403
        expected_body: { kind: byte_exact, body: "RBAC: access denied" }
        expected_headers: set_equal_modulo_allow_list
  ```
  (Confirm the exact driver field names against `tests/fixtures/0017-http-filter-rbac/expectations.yaml` +
  `tests/differential/src/lib.rs` — the `http1_probe_list` shape is reused verbatim.) `README.md`: pin §A1 (the wire
  shape), §A2 (the `filter`=default-namespace correspondence + the REQUIRED producer-before-consumer order), §A3 (the
  byte-exact `200`/`403`+19B verdicts + the StringMatcher reuse), the §A4/A5/A6 config-validity divergences, and document
  the present/mismatch/absent probe trio as the anti-trivial guard. The test `rbac_dynamic_metadata.rs` mirrors the
  fixture-0017 RBAC differential test (`differential::run_fixture` on the `0043-…` dir).
- [ ] **Step 2: Run to verify (Docker-gated).** `cargo test -p differential rbac_dynamic_metadata` (the differential
  tests are NOT `#[ignore]`-gated — they self-skip when Docker is unavailable, like the fixture-0017 `http_filter_rbac`
  test; no `--include-ignored` needed). Expected on this Docker host: GREEN once Tasks 1–3 are in. **REBUILD `envoy-bin` first** (`cargo build -p envoy-bin`) —
  a stale binary fails with `unknown filter`/`unknown field` (the phase-33 T11 / phase-34 T6 lesson). The §A facts
  predict a clean byte-exact match; this differential is LOCALLY authoritative (no reload trigger — NOT Linux-CI-only,
  unlike 26/27).
- [ ] **Step 3: (Implementation already done in Tasks 1–3.)** Adjust the config/probes only if a live byte mismatch
  surfaces; the §A facts predict a clean match.
- [ ] **Step 4: Run to verify pass.** Confirm both proxies return byte-identical status+body for all 3 probes. Run
  `cargo build -p differential --tests` (no other fixture regressed at compile).
- [ ] **Step 5: Commit.** `git add tests/fixtures/0043-http-rbac-dynamic-metadata tests/differential/tests/rbac_dynamic_metadata.rs && git commit -m "phase 35 T5: fixture 0043 RBAC dynamic-metadata byte-exact differential (prod->200 / dev|absent->403) [ADR-0086]"`

---

## Task 6: BEHAVIOR_CONTRACT extension + `parse_bootstrap` fuzz seed

> Document the new RBAC `metadata` condition + the §A facts; add a fuzz seed to the EXISTING `parse_bootstrap` target
> (NO new target — §A §3.7).

**Files:** Modify `docs/envoy-rust/BEHAVIOR_CONTRACT.md`; create a seed under
`crates/envoy-config/fuzz/corpus/parse_bootstrap/`.

- [ ] **Step 1: (No code test — doc + seed task.)** Verify the seed parses: a throwaway
  `envoy_config::parse_bootstrap(seed)` → `Ok` (used + removed; NOT committed).
- [ ] **Step 2: Implement the docs + seed.**
  - **BEHAVIOR_CONTRACT.md:** under the "HTTP filters" section (after the phase-34 `header_to_metadata` subsection), add a
    `### Phase 35 (ADR-0086): the RBAC `metadata` Permission/Principal condition` subsection documenting: the `metadata: {
    filter, path: [{ key }], value: { string_match } }` wire shape (§A1); the `filter`→namespace correspondence (default
    `envoy.filters.http.header_to_metadata`, custom matchable) + the REQUIRED producer-before-consumer chain order (§A2);
    the absent-namespace/absent-key → no-match + the full StringMatcher modes + the byte-exact `200`/`403`+19B verdicts
    (§A3); the boot-fatal config-validity — empty `filter`, missing `value`, empty `path` (§A4); the THREE divergences —
    multi-segment path stricter-reject (§A5), non-`string_match` value stricter-reject (§A6), the deprecated-but-functional
    field (§A7, non-differential); and the §2.2 deferrals (non-string Values, nested path, `invert`, shadow_rules,
    per-route, other producers/consumers).
  - **Fuzz seed:** `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_rbac_metadata.yaml` — a minimal valid bootstrap
    (concrete `port_value`, no `{{PORT}}`) with a `[header_to_metadata, rbac, router]` chain where the `rbac` policy has a
    `metadata` Permission. Modeled on fixture 0043's `envoy-rust.yaml`. (NO new fuzz target; NO ci.yml change — memory
    `new-fuzz-target-needs-a-ci-yml-step` satisfied because the seed reuses the existing `parse_bootstrap` target; confirm
    by inspection. Verify the seed is tracked: `git ls-files` after `git add` — memory `fuzz-corpus-seed-gitignored-by-default`
    warns the corpus dir is `*`-ignored, so confirm the new seed lands as a tracked file [the corpus already has tracked
    seeds, so the `!`-un-ignore is already present — verify].)
- [ ] **Step 3: Run to verify.** `cargo build -p envoy-config` (the seed is data, not code — confirm the crate builds).
  The §7.5 gate (d) short-budget fuzz run happens at state-4.
- [ ] **Step 4: (No test run beyond the build.)** Optionally `cargo +nightly fuzz run parse_bootstrap -- -runs=0` loads
  the corpus (confirms the seed parses).
- [ ] **Step 5: Commit.** `git add docs/envoy-rust/BEHAVIOR_CONTRACT.md crates/envoy-config/fuzz/corpus/parse_bootstrap/ && git commit -m "phase 35 T6: BEHAVIOR_CONTRACT RBAC metadata condition + parse_bootstrap fuzz seed [ADR-0086]"`

---

## §7.5 acceptance gate (state-4 verification — previewed)

A phase-35 state-4 verification (the next-but-one session) must show ALL green:
- **(a)** fixture `0043` green (cross-proxy byte-identical verdict: `X-Tier: prod` → `200` + `ok\n`; `X-Tier: dev` /
  absent → `403` + `RBAC: access denied` 19B).
- **(b)** all of `0001`–`0042` green (incl. `0017` rbac header-only + `0012` default-format + `0041` set_metadata +
  `0042` header_to_metadata UNCHANGED — the regression-equivalence witnesses; the `metadata` matcher is an additive
  enum variant no existing config uses).
- **(c)** h2spec ≥95% (unchanged — no HTTP/2 codec change). **NOTE (memory `h2spec-3-5-2-preface-host-sensitive`):** the
  §3.5/2 known-failure false-REDs LOCALLY; CI is authoritative — NEVER trim known-failures.txt from local evidence.
- **(d)** the EXISTING `parse_bootstrap` (+ `accesslog_format_parse`, unchanged) fuzz targets clean for the short-budget
  CI run (with the new `metadata`-matcher seed) — **NO new fuzz target** (§A §3.7); confirm NO ci.yml change.
- **(e)** `cargo build --workspace --all-targets` / `cargo clippy --workspace --all-targets --all-features -- -D
  warnings` / `cargo fmt --all -- --check` / `cargo test --workspace` / `cargo deny check` all clean. **Per memory
  `envoy-rust-state4-ci-first-execution`: state-4 is CI's first real execution — budget a possible cross-crate clippy
  nit / fmt drift; fix in-place.** **PUSH per-session from state-3 onward + confirm CI green** (do NOT let code commits
  accumulate unpushed — the phase-33 lesson).
- **(f)** `REVIEW.md` approved.

`#![forbid(unsafe_code)]` holds (D-3.8). No new crate, no new dependency, no new `HttpFilterInstance` variant (D-3.2).
The backend-routing/upstream/admin-dump differential fixtures false-RED LOCALLY (Docker bridge IP `192.168.65.2`); CI is
authoritative (memories `differential-host-bridge-ip-192-168-65-2`, `host-docker-desktop-virtiofs-no-inotify`).

---

_§A facts locked by **ADR-0086** (the §6.2 reconciliation). Scope locked by **ADR-0085**. The §6.1 split did NOT fire
(~6 tasks). The state-3 implementation is the next session (`superpowers:subagent-driven-development`)._
