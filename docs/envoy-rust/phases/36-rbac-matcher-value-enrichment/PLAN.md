# Phase 36 — `36-rbac-matcher-value-enrichment` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
> Every task is TDD: write the failing test FIRST, watch it fail, implement minimally, watch it pass, commit.

**Goal:** Enrich the EXISTING phase-10 `envoy.filters.http.rbac` filter's matcher-VALUE surface with (F1) the
`present_match` `ValueMatcher` variant on the phase-35 `metadata` condition and (F2) `safe_regex` `StringMatcher`
compilation on the RBAC path (header + metadata values) — closing carry-forward M35-1 (a latent runtime panic) —
proven by a byte-exact cross-proxy verdict (fixture `0044`: F1 header-present→`200`+`ok\n` / absent→`403`+`RBAC: access
denied`; F2 regex-match→`200` / miss→`403`), with all 43 pre-existing fixtures unchanged.

**Architecture:** ADDS NO new `HttpFilterInstance` variant (RBAC is the phase-10 variant) and NO new infrastructure.
F1 — config: a `PresentMatch(bool)` arm on the hand-rolled `ValueMatcher` enum (`crates/envoy-config/src/bootstrap.rs`)
+ its `Deserialize`/`Serialize`/`KEYS`; runtime: a presence-aware `eval_metadata` (`crates/envoy-filter/src/rbac.rs`)
that compares KEY PRESENCE against the bool — `match = present && want` (§A1, NOT the SPEC's projected `present == want`
and NOT the `HeaderMatcherMode::PresentMatch` `want ? present : true` precedent). F2 — compile every RBAC-reachable
`SafeRegex` into `SafeRegex::compiled` at `rbac.rs` LOWERING time (`lower_permission`/`lower_principal`, made fallible),
covering BOTH the `Permission::Header`/`Principal::Header` `safe_regex_match` AND the `metadata` value's
`string_match.safe_regex`; a malformed RBAC regex becomes BOOT-fatal (not a first-request panic). The RBAC decision
matrix + the `403` + `b"RBAC: access denied"` local reply + the stats + the store + the producers + the decode
pipeline are UNCHANGED. The load-bearing mechanism is the decode pipeline's shared `&mut FilterRequest` threading
(`pipeline.rs:77`): a consumer reads what an earlier producer wrote IN THE SAME PASS, so the fixture chain order
`[header_to_metadata, rbac, router]` (producer before consumer) is REQUIRED for the metadata-driven probes.

**Tech Stack:** Rust (pinned toolchain, `#![forbid(unsafe_code)]` workspace-wide), `serde`/`serde_yaml` (hand-rolled
visitors + `deny_unknown_fields`), `thiserror`, the phase-10 RBAC engine + the 04.x `StringMatcher`/`SafeRegex` + the
`regex` foundation (ADR-0021, reused), the phase-33 store + the phase-34 `header_to_metadata` producer + the
decode-pipeline threading (reused UNCHANGED), `tests/differential` (the fixture-`0017`/`0043` status+body comparator
with the `extra_headers` probe), `cargo fuzz` (existing `parse_bootstrap` target — NO new target).

---

## §A — Empirically-locked facts (the §6.2 ground truth; reconciled by ADR-0088)

Run LOCALLY at this PLAN-write against live `envoyproxy/envoy:v1.33.0` (the ENVOY_TARGET pin `sha256:56da5afd…`) — an
H1 `direct_response` (200 `ok\n`) listener, a `[header_to_metadata, rbac, router]` chain (`x-tier`→`tier`, default
namespace); 9 RBAC `rules:` variants — runtime `curl` probes (`od -c` on the body) for `present_match: true`/`false`, a
`metadata` `string_match.safe_regex` AND a `Permission::Header` `safe_regex_match` (`prod|staging`), anchoring-boundary
probes (`staging-2`/`xstaging`/`production`); `--mode validate` for `null_match`/`bool_match`/`double_match`/`or_match`
+ a malformed `safe_regex`; `/config_dump` round-trip. **ADR-0088 FIRES** — **TWO MATERIAL divergences** from the SPEC
projection (the `present_match` semantics; the `safe_regex` full-vs-partial anchoring), plus the headline
confirmations. These facts are LOCKED; the tasks below encode them. Do NOT re-derive.

**A1 — F1 `present_match` semantics (MATERIAL DIVERGENCE).** Empirical truth table (`present` = `filter:key` resolves
to a stored value): `(want=true, present=true)→MATCH`; `(want=true, present=false)→NO match`; `(want=false,
present=true)→NO match`; `(want=false, present=false)→NO match`. ⇒ **`match = present && want`**. `present_match: false`
NEVER matches. This DIVERGES from the SPEC §2.1.2 projection (`present == want`, which would match-when-absent for
`want=false`) AND from the existing `HeaderMatcherMode::PresentMatch` (`matcher.rs:42-47`, `want ? value.is_some() :
true`). The new `ValueMatcher::PresentMatch(want)` MUST implement `present && want` — do NOT copy the header precedent
(the SPEC §2.1.2 / spec-review I-1 foot-gun, resolved empirically).

**A2 — F1 present-empty + verdicts (CONFIRMED byte-exact).** `present_match: true`: any NON-empty `X-Tier` → `tier`
stored → present → **`200` + `ok\n`** (3 bytes, the `direct_response` body); `X-Tier;` (present-but-empty) →
`header_to_metadata` writes nothing (ADR-0084) → key UNSET → present=false → **`403` + `RBAC: access denied`** (19
bytes, NO trailing newline, `od -c`); absent `X-Tier` → `403`+19B. So present-but-empty is treated as ABSENT (the SPEC
"may be moot" projection CONFIRMED). `/config_dump` round-trips `"present_match": true` verbatim (snake_case).

**A3 — F2 `safe_regex` verdicts (CONFIRMED byte-exact; metadata + header IDENTICAL).** With pattern `prod|staging`:
`staging`/`prod` → `200`+`ok\n`; `dev` → `403`+19B; present-empty/absent → `403`+19B. A `safe_regex` value on the
`metadata` `string_match` AND on a `Permission::Header` `safe_regex_match` produced byte-identical verdicts — F2 must
compile BOTH paths.

**A3b — F2 anchoring (MATERIAL DIVERGENCE; cross-cutting, pre-existing → carry-forward M36-1).** Envoy `safe_regex` is
**RE2 FULL match (anchored)**: with `prod|staging`, `staging-2`/`xstaging`/`production` → `403` (the WHOLE string must
match). envoy-rust's `StringMatcher::matches` SafeRegex (`matcher.rs:87-91`) uses `regex::Regex::is_match` =
**PARTIAL** (substring), so `is_match("prod|staging", "staging-2") == true` — a DIVERGENCE for UNANCHORED patterns.
PRE-EXISTING (phase 04.2), cross-cutting (the route-config header `safe_regex` shares this path), masked because every
existing fixture uses an anchored pattern (`^v[0-9]+$`). **Disposition: fixture 0044 + every phase-36 backstop LOCK an
ANCHORED pattern (`^(prod|staging)$` — Envoy accepts the redundant anchors; envoy-rust `is_match` of an `^…$` pattern
is whole-string-equivalent), so partial==full and the differential is byte-identical WITHOUT a SafeRegex-semantics
change.** The unanchored partial-vs-full gap is **new carry-forward M36-1 — explicitly OUT of phase-36 scope** (NOT
RBAC-specific; a proper full-match fix touches the shared route-config SafeRegex and warrants its own phase). DISTINCT
from M35-1 (the latent panic), which F2 DOES close.

**A4 — F2 mechanism + malformed-regex disposition (boot-fatal, ADR-0049).** A malformed RBAC `safe_regex` (`(`) is
BOOT-FATAL on Envoy (`--mode validate` exit 1, "missing ): ("). envoy-rust must reject at BOOT, not first-request
panic. **Mechanism (the SPEC §3.4 call, LOCKED):** compile every RBAC-reachable `SafeRegex` at `rbac.rs` lowering time
on the owned clone, making `lower_permission`/`lower_principal` **fallible** (`-> Result<_, FilterError>`) and threading
the `Result` up to the already-fallible `RbacFilter::build_from_config` → `FilterPipeline::build_from_config`, which
runs at listener-build (server startup) → a returned `Err` fails the boot. Expose focused public envoy-config compile
helpers (`HeaderMatcher::compile_safe_regexes` + `ValueMatcher::compile_safe_regexes`, reusing the existing private
`compile_safe_regex(&mut SafeRegex)` + `ConfigError::InvalidRegex`); `lower_*` clone the matcher, call the helper, map
`ConfigError::InvalidRegex → FilterError::InvalidConfig`. A naïve in-`lower` `Regex::new().unwrap()` is NOT acceptable
(re-introduces a boot panic). REJECTED alternative — making the immutable RBAC config-validation path
(`validate_http_filters` → `validate_rbac_config` → tree validators) mutable to compile at parse time — too invasive
(the whole `&[HttpFilter]` chain up to the bootstrap would become `&mut`); lowering-time compile is the SPEC's named
M35-1 fix home and surgical. (Boot-fatal at pipeline-build is a slightly later boot STAGE than the route walk's
parse-time rejection, but both are pre-traffic startup → differentially both proxies fail to boot → equivalent.) NO
new `ConfigError`/`FilterError` variant.

**A5 — config-validity for non-`string_match`/`present_match` keys (CONFIRMED stricter).** Envoy ACCEPTS
`null_match`/`bool_match`/`double_match`/`or_match`/`list_match` (the full `ValueMatcher` oneof). envoy-rust ADDS ONLY
`present_match` to the hand-rolled "exactly one key" `ValueMatcher` visitor (`KEYS = ["string_match", "present_match"]`);
every other oneof key stays `unknown_field` → BOOT-FATAL (the ADR-0086 §A6 stricter-than-Envoy posture, unchanged).
**Two existing in-code tests MUST be repurposed by this phase:** (i) `rbac_metadata_rejects_present_match_value`
(`bootstrap.rs:12186`), which today asserts `present_match` is REJECTED, MUST be FLIPPED to assert ACCEPTANCE (F1,
Task 1); (ii) `rbac_metadata_value_safe_regex_is_parse_accepted` (`bootstrap.rs:~12514`), whose comment documents the
M35-1 "would panic at runtime" limitation, MUST be updated (F2 closes the limitation — Task 3/4).

---

## File structure (what each file is responsible for)

- **`crates/envoy-config/src/bootstrap.rs`** — F1: the `PresentMatch(bool)` arm on the `ValueMatcher` enum (line
  ~1348) + its `Deserialize` visitor (`"present_match"` arm + `KEYS`, line ~1362-1379) + the `Serialize` rename. F2:
  the public `HeaderMatcher::compile_safe_regexes` + `ValueMatcher::compile_safe_regexes` methods (reuse the private
  `compile_safe_regex`, line ~4486). The two repurposed in-code tests (lines ~12186, ~12514).
- **`crates/envoy-config/src/matcher.rs`** — F1: `ValueMatcher::matches` (line ~108) gains the `PresentMatch(want) =>
  *want` arm (exhaustiveness; correct because `matches(&str)` is only reached when a value is present) + a new
  `ValueMatcher::matches_resolved(resolved: Option<&str>) -> bool` (the presence-aware entry: `StringMatch =>
  resolved.is_some_and(|v| sm.matches(v))`, `PresentMatch(want) => resolved.is_some() && *want`).
- **`crates/envoy-filter/src/rbac.rs`** — F1: `eval_metadata` (line ~88) restructured to call
  `m.value.matches_resolved(...)`. F2: `lower_permission`/`lower_principal` (line ~229/249) made fallible + compile
  SafeRegex on the cloned `Header`/`Metadata` matcher; `build_from_config` (line ~147) threads the `Result`. The
  in-process backstop tests.
- **`tests/fixtures/0044-http-rbac-matcher-value-enrichment/`** — the 4 fixture files (`envoy.yaml`, `envoy-rust.yaml`,
  `expectations.yaml`, `README.md`): F1 present/absent + F2 regex-match/miss probe pairs.
- **`tests/differential/tests/rbac_matcher_value_enrichment.rs`** — the differential test entry (mirrors the
  fixture-0043 RBAC test; `differential::run_fixture` on the `0044-…` dir).
- **`docs/envoy-rust/BEHAVIOR_CONTRACT.md`** — the phase-36 "HTTP filters" subsection (the `present_match` presence
  semantics + the now-compiled RBAC SafeRegex, superseding the M35-1 limitation note).
- **`crates/envoy-config/fuzz/corpus/parse_bootstrap/`** — two seeds (a `present_match` `metadata` condition; a RBAC
  `safe_regex` matcher value). NO new fuzz target. NOTE (memory `fuzz-corpus-seed-gitignored-by-default`): the corpus
  dir is `*`-ignored — each new seed needs an explicit `!`-un-ignore line in the fuzz `.gitignore`, else it is
  silently untracked + invisible to CI (verify via `git ls-files`).

**Sequencing note (cross-crate red window).** Task 1 adds the `ValueMatcher::PresentMatch` variant; `matcher.rs`'s
`ValueMatcher::matches` (same crate) becomes non-exhaustive until Task 1 closes it — so Task 1 updates BOTH
`bootstrap.rs` and `matcher.rs` to keep `envoy-config` green. `crates/envoy-filter`'s `eval_metadata` keeps compiling
(it calls `m.value.matches(v)` until Task 2 switches it to `matches_resolved`). Task 4 makes `lower_*` fallible — a
localized `rbac.rs` change. Gate Task 1 on `cargo test -p envoy-config`, Tasks 2/4 on `cargo test -p envoy-filter
rbac`. If the harness demands a workspace-green commit per task, the natural fold is T1+T2 (F1) and T3+T4 (F2).

---

## Task 1: F1 config — the `present_match` `ValueMatcher` variant (+ flip the reject test)

> §A1/A5: add `PresentMatch(bool)` to `ValueMatcher`; `KEYS = ["string_match", "present_match"]`; every OTHER oneof key
> stays boot-fatal. Flip `rbac_metadata_rejects_present_match_value` → ACCEPT. Keep `matcher.rs` exhaustive + add
> `matches_resolved`.

**Files:** Modify `crates/envoy-config/src/bootstrap.rs` (the `ValueMatcher` enum + `Deserialize` + `Serialize` + the
flipped test); `crates/envoy-config/src/matcher.rs` (`matches` exhaustiveness + `matches_resolved`).

- [ ] **Step 1: Write the failing tests.** In `bootstrap.rs` test module add:
  ```rust
  #[test]
  fn rbac_metadata_accepts_present_match_value() {
      // F1 §A5: present_match is now an ACCEPTED ValueMatcher variant.
      let yaml = r#"
  rules:
    action: ALLOW
    policies:
      p:
        permissions:
          - metadata:
              filter: envoy.filters.http.header_to_metadata
              path: [ { key: tier } ]
              value: { present_match: true }
        principals:
          - any: true
  "#;
      let cfg: RbacConfig = serde_yaml::from_str(yaml).expect("present_match accepted");
      match &cfg.rules.policies["p"].permissions[0] {
          Permission::Metadata(m) => assert_eq!(m.value, ValueMatcher::PresentMatch(true)),
          other => panic!("expected Metadata, got {other:?}"),
      }
  }
  #[test]
  fn rbac_metadata_present_match_false_parses() {
      let yaml = r#"
  rules: { action: ALLOW, policies: { p: { permissions: [ { metadata: { filter: f, path: [ { key: tier } ], value: { present_match: false } } } ], principals: [ { any: true } ] } } }
  "#;
      let cfg: RbacConfig = serde_yaml::from_str(yaml).expect("present_match:false accepted");
      assert_eq!(cfg.rules.policies["p"].permissions[0],
                 Permission::Metadata(MetadataMatcher { filter: "f".into(),
                     path: vec![MetadataPathSegment { key: "tier".into() }],
                     value: ValueMatcher::PresentMatch(false) }));
  }
  #[test]
  fn rbac_metadata_rejects_other_value_matcher_keys() {
      // §A5: null_match/bool_match/etc. stay boot-fatal (stricter than Envoy).
      for key in ["null_match: {}", "bool_match: true", "double_match: { exact: 1.0 }"] {
          let yaml = format!(r#"
  rules: {{ action: ALLOW, policies: {{ p: {{ permissions: [ {{ metadata: {{ filter: f, path: [ {{ key: tier }} ], value: {{ {key} }} }} }} ], principals: [ {{ any: true }} ] }} }} }}
  "#);
          serde_yaml::from_str::<RbacConfig>(&yaml).expect_err("non-string/present value rejected");
      }
  }
  ```
  And in `matcher.rs` test module add:
  ```rust
  #[test]
  fn value_matcher_present_match_resolved_semantics() {
      // §A1: match = present && want.  present_match:false NEVER matches.
      let t = ValueMatcher::PresentMatch(true);
      assert!(t.matches_resolved(Some("anything")));   // present && true
      assert!(!t.matches_resolved(None));              // absent
      let f = ValueMatcher::PresentMatch(false);
      assert!(!f.matches_resolved(Some("anything")));  // present && false → false
      assert!(!f.matches_resolved(None));              // absent
      // StringMatch via matches_resolved:
      let sm = ValueMatcher::StringMatch(StringMatcher {
          mode: StringMatcherMode::Exact("prod".into()), ignore_case: false });
      assert!(sm.matches_resolved(Some("prod")));
      assert!(!sm.matches_resolved(Some("dev")));
      assert!(!sm.matches_resolved(None));
  }
  ```
- [ ] **Step 2: Run to verify they fail.** `cargo test -p envoy-config present_match value_matcher_present`
  Expected: FAIL — no `PresentMatch` variant / no `matches_resolved`. Also DELETE the now-obsolete
  `rbac_metadata_rejects_present_match_value` (line ~12186) — its assertion (reject) is now false.
- [ ] **Step 3: Implement.** In `bootstrap.rs`, the `ValueMatcher` enum + visitor + serialize:
  ```rust
  #[derive(Debug, Clone, PartialEq, Serialize)]
  pub enum ValueMatcher {
      #[serde(rename = "string_match")]
      StringMatch(StringMatcher),
      /// §A1 (phase 36): match on KEY PRESENCE. Semantics `match = present && want`
      /// (`present_match: false` NEVER matches — NOT the HeaderMatcher `present_match` precedent).
      #[serde(rename = "present_match")]
      PresentMatch(bool),
  }
  ```
  In the `Deserialize` visitor: `const KEYS: &[&str] = &["string_match", "present_match"];` and add the arm
  `"present_match" => ValueMatcher::PresentMatch(map.next_value::<bool>()?),` (before the `other =>` fallthrough).
  In `matcher.rs`, make `ValueMatcher::matches` exhaustive and add `matches_resolved`:
  ```rust
  impl ValueMatcher {
      /// Match against a PRESENT value. `present_match` returns its bool (the value is present,
      /// so `present && want == want`). Kept for the value-present call sites.
      pub fn matches(&self, value: &str) -> bool {
          match self {
              ValueMatcher::StringMatch(sm) => sm.matches(value),
              ValueMatcher::PresentMatch(want) => *want,
          }
      }
      /// Presence-aware entry (§A1). `resolved` is `Some(v)` iff the metadata key resolved.
      /// `present_match`: `match = resolved.is_some() && want`. `string_match`: value present AND matches.
      pub fn matches_resolved(&self, resolved: Option<&str>) -> bool {
          match self {
              ValueMatcher::StringMatch(sm) => resolved.is_some_and(|v| sm.matches(v)),
              ValueMatcher::PresentMatch(want) => resolved.is_some() && *want,
          }
      }
  }
  ```
- [ ] **Step 4: Run to verify pass.** `cargo test -p envoy-config` — all green (incl. the existing
  `rbac_metadata_rejects_unknown_field` + the phase-35 string_match tests, unchanged).
- [ ] **Step 5: Commit.** `git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/matcher.rs && git commit -m "phase 36 T1: present_match ValueMatcher variant + matches_resolved [ADR-0088]"`

---

## Task 2: F1 runtime — presence-aware `eval_metadata` (`present && want`)

> §A1/A2: `eval_metadata` routes through `matches_resolved` so `present_match` observes presence. The fixture-driving
> mechanism (`present_match: true` matches a present key, denies absent/present-empty) is byte-exact.

**Files:** Modify `crates/envoy-filter/src/rbac.rs` (`eval_metadata`, line ~88; + backstop tests).

- [ ] **Step 1: Write the failing tests** in `rbac.rs` test module (reuse the `req_with_md` / `metadata_matcher`
  helpers; add a `present_matcher`):
  ```rust
  fn present_matcher(filter: &str, key: &str, want: bool) -> MetadataMatcher {
      MetadataMatcher { filter: filter.into(),
          path: vec![MetadataPathSegment { key: key.into() }],
          value: ValueMatcher::PresentMatch(want) }
  }
  #[test]
  fn metadata_present_match_true_matches_present_key() {
      let ns = "envoy.filters.http.header_to_metadata";
      let req = req_with_md(ns, "tier", "staging");           // any value
      assert!(eval_permission(&RuntimePermission::Metadata(present_matcher(ns, "tier", true)), &req));
  }
  #[test]
  fn metadata_present_match_true_no_match_when_absent() {
      let ns = "envoy.filters.http.header_to_metadata";
      let req = req_with_md(ns, "other", "x");                // key tier absent
      assert!(!eval_permission(&RuntimePermission::Metadata(present_matcher(ns, "tier", true)), &req));
  }
  #[test]
  fn metadata_present_match_false_never_matches() {
      // §A1: present_match:false → present && false → never matches, even when present.
      let ns = "envoy.filters.http.header_to_metadata";
      let present = req_with_md(ns, "tier", "staging");
      let absent = req_with(vec![]);
      assert!(!eval_permission(&RuntimePermission::Metadata(present_matcher(ns, "tier", false)), &present));
      assert!(!eval_permission(&RuntimePermission::Metadata(present_matcher(ns, "tier", false)), &absent));
  }
  ```
- [ ] **Step 2: Run to verify they fail.** `cargo test -p envoy-filter present_match`
  Expected: FAIL — `eval_metadata` still routes through `is_some_and(|v| m.value.matches(v))`, which never observes the
  absent/false cases correctly (e.g. `present_match:false` with a present value would return `*want`=false via matches,
  but the absent-key path can't reach `present_match` at all — confirm the false case compiles/fails as predicted).
- [ ] **Step 3: Implement.** Restructure `eval_metadata`:
  ```rust
  /// Phase 35/36: resolve the single-segment metadata path and apply the ValueMatcher.
  /// §A1: routed through `matches_resolved` so `present_match` observes KEY PRESENCE
  /// (`match = present && want`); `string_match` keeps present-AND-value-matches.
  /// The validator guarantees `path.len() == 1`, so `path[0]` is safe.
  fn eval_metadata(m: &envoy_config::MetadataMatcher, req: &FilterRequest) -> bool {
      let resolved = req
          .dynamic_metadata
          .get(&m.filter)
          .and_then(|ns| ns.get(&m.path[0].key))
          .map(String::as_str);
      m.value.matches_resolved(resolved)
  }
  ```
- [ ] **Step 4: Run to verify pass.** `cargo test -p envoy-filter rbac` — all green (incl. the phase-35 string_match
  metadata tests, unchanged — `matches_resolved` is equivalent for `StringMatch`).
- [ ] **Step 5: Commit.** `git add crates/envoy-filter/src/rbac.rs && git commit -m "phase 36 T2: presence-aware eval_metadata (present && want) [ADR-0088]"`

---

## Task 3: F2 config — public SafeRegex compile helpers

> §A4: expose a focused public compile surface on `HeaderMatcher` + `ValueMatcher`, reusing the private
> `compile_safe_regex(&mut SafeRegex)`. A malformed pattern → `Err(ConfigError::InvalidRegex)`. NO behavior change to
> the existing route-config `validate_header_matcher` compiler.

**Files:** Modify `crates/envoy-config/src/bootstrap.rs` (the two public methods, near `compile_safe_regex` line ~4486;
+ repurpose the `rbac_metadata_value_safe_regex_is_parse_accepted` test, line ~12514).

- [ ] **Step 1: Write the failing tests** in `bootstrap.rs` test module:
  ```rust
  #[test]
  fn header_matcher_compile_safe_regexes_compiles_and_rejects() {
      let mut ok = HeaderMatcher { name: "x".into(),
          mode: HeaderMatcherMode::SafeRegexMatch(SafeRegex { regex: "^(prod|staging)$".into(), compiled: None }),
          invert_match: false };
      ok.compile_safe_regexes().expect("valid regex compiles");
      assert!(matches!(&ok.mode, HeaderMatcherMode::SafeRegexMatch(sr) if sr.compiled.is_some()));
      let mut bad = HeaderMatcher { name: "x".into(),
          mode: HeaderMatcherMode::SafeRegexMatch(SafeRegex { regex: "(".into(), compiled: None }),
          invert_match: false };
      assert!(matches!(bad.compile_safe_regexes(), Err(crate::ConfigError::InvalidRegex { .. })));
  }
  #[test]
  fn value_matcher_compile_safe_regexes_compiles_string_and_noops_present() {
      let mut v = ValueMatcher::StringMatch(StringMatcher {
          mode: StringMatcherMode::SafeRegex(SafeRegex { regex: "^(prod|staging)$".into(), compiled: None }),
          ignore_case: false });
      v.compile_safe_regexes().expect("compiles");
      if let ValueMatcher::StringMatch(sm) = &v {
          assert!(matches!(&sm.mode, StringMatcherMode::SafeRegex(sr) if sr.compiled.is_some()));
      }
      // present_match has nothing to compile → Ok.
      ValueMatcher::PresentMatch(true).compile_safe_regexes().expect("noop ok");
  }
  ```
- [ ] **Step 2: Run to verify they fail.** `cargo test -p envoy-config compile_safe_regexes`
  Expected: FAIL — no such methods.
- [ ] **Step 3: Implement.** In `bootstrap.rs`, near `compile_safe_regex`:
  ```rust
  impl HeaderMatcher {
      /// Compile any SafeRegex reachable from this matcher (top-level `safe_regex_match` or a
      /// nested `string_match.safe_regex`) into `SafeRegex::compiled`. §A4 (phase 36) — the
      /// RBAC lowering path calls this on its owned clone (the route-config walk uses
      /// `validate_header_matcher`, UNCHANGED). Boot-fatal `InvalidRegex` on a bad pattern.
      pub fn compile_safe_regexes(&mut self) -> Result<(), crate::ConfigError> {
          match &mut self.mode {
              HeaderMatcherMode::SafeRegexMatch(sr) => compile_safe_regex(sr),
              HeaderMatcherMode::StringMatch(sm) => sm.compile_safe_regex(),
              _ => Ok(()),
          }
      }
  }
  impl StringMatcher {
      /// Compile the SafeRegex mode (if any) into `SafeRegex::compiled`. §A4.
      pub fn compile_safe_regex(&mut self) -> Result<(), crate::ConfigError> {
          if let StringMatcherMode::SafeRegex(sr) = &mut self.mode {
              compile_safe_regex(sr)?;
          }
          Ok(())
      }
  }
  impl ValueMatcher {
      /// Compile any SafeRegex reachable from this value matcher. §A4. `present_match` → no-op.
      pub fn compile_safe_regexes(&mut self) -> Result<(), crate::ConfigError> {
          match self {
              ValueMatcher::StringMatch(sm) => sm.compile_safe_regex(),
              ValueMatcher::PresentMatch(_) => Ok(()),
          }
      }
  }
  ```
  Then UPDATE `rbac_metadata_value_safe_regex_is_parse_accepted` (line ~12514): its comment claims a runtime
  `ValueMatcher::matches` on a SafeRegex value "would panic" — now FALSE. Rename to
  `rbac_metadata_value_safe_regex_parse_accepted_and_compilable` and assert that, after `compile_safe_regexes()` on the
  parsed value, `compiled.is_some()` (the limitation is closed; lowering compiles it — Task 4 wires the call site).
- [ ] **Step 4: Run to verify pass.** `cargo test -p envoy-config compile_safe_regexes safe_regex` — green.
- [ ] **Step 5: Commit.** `git add crates/envoy-config/src/bootstrap.rs && git commit -m "phase 36 T3: public SafeRegex compile helpers (HeaderMatcher/ValueMatcher) [ADR-0088]"`

---

## Task 4: F2 runtime — fallible lowering + RBAC SafeRegex compilation (closes M35-1)

> §A3/A3b/A4: `lower_permission`/`lower_principal` become fallible and compile the `Header`/`Metadata` SafeRegex on the
> owned clone; `build_from_config` threads the `Result`. The header-`safe_regex` test is the PANIC-REGRESSION GUARD
> (MUST panic/fail on the pre-fix tree). A malformed regex → boot-fatal `FilterError`, not a first-request panic.

**Files:** Modify `crates/envoy-filter/src/rbac.rs` (`lower_permission`/`lower_principal` line ~229/249;
`build_from_config` line ~147; + backstop tests). All patterns ANCHORED (§A3b).

- [ ] **Step 1: Write the failing tests** in `rbac.rs` test module:
  ```rust
  fn safe_regex_string_matcher(pattern: &str) -> StringMatcher {
      StringMatcher { mode: StringMatcherMode::SafeRegex(envoy_config::SafeRegex {
          regex: pattern.into(), compiled: None }), ignore_case: false }
  }
  #[test]
  fn metadata_safe_regex_value_matches_without_panic() {
      // §A3: ANCHORED ^(prod|staging)$ → staging matches, dev misses. No panic (M35-1 closed).
      let ns = "envoy.filters.http.header_to_metadata";
      let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());
      let mut policies = std::collections::BTreeMap::new();
      policies.insert("p".into(), envoy_config::Policy {
          permissions: vec![envoy_config::Permission::Metadata(MetadataMatcher {
              filter: ns.into(), path: vec![MetadataPathSegment { key: "tier".into() }],
              value: ValueMatcher::StringMatch(safe_regex_string_matcher("^(prod|staging)$")) })],
          principals: vec![envoy_config::Principal::Any(true)] });
      let cfg = envoy_config::RbacConfig { rules: envoy_config::Rules {
          action: envoy_config::Action::Allow, policies } };
      let mut f = RbacFilter::build_from_config(&cfg, &registry, "ingress_http").expect("builds");
      let mut ok = req_with_md(ns, "tier", "staging");
      assert!(matches!(f.decode_headers(&mut ok), crate::pipeline::Decision::Continue));
      let mut miss = req_with_md(ns, "tier", "dev");
      match f.decode_headers(&mut miss) {
          crate::pipeline::Decision::StopAndSend(r) => assert_eq!(r.status, 403),
          other => panic!("expected 403, got {other:?}"),
      }
  }
  #[test]
  fn header_safe_regex_matches_without_panic() {     // PANIC-REGRESSION GUARD (M35-1)
      let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());
      let mut policies = std::collections::BTreeMap::new();
      policies.insert("p".into(), envoy_config::Policy {
          permissions: vec![envoy_config::Permission::Header(HeaderMatcher { name: "x-tier".into(),
              mode: HeaderMatcherMode::SafeRegexMatch(envoy_config::SafeRegex {
                  regex: "^(prod|staging)$".into(), compiled: None }), invert_match: false })],
          principals: vec![envoy_config::Principal::Any(true)] });
      let cfg = envoy_config::RbacConfig { rules: envoy_config::Rules {
          action: envoy_config::Action::Allow, policies } };
      let mut f = RbacFilter::build_from_config(&cfg, &registry, "ingress_http").expect("builds");
      let mut ok = req_with(vec![("x-tier", "staging")]);
      assert!(matches!(f.decode_headers(&mut ok), crate::pipeline::Decision::Continue));
      let mut miss = req_with(vec![("x-tier", "dev")]);
      assert!(matches!(f.decode_headers(&mut miss), crate::pipeline::Decision::StopAndSend(_)));
  }
  #[test]
  fn malformed_rbac_safe_regex_is_boot_fatal_not_panic() {
      let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());
      let mut policies = std::collections::BTreeMap::new();
      policies.insert("p".into(), envoy_config::Policy {
          permissions: vec![envoy_config::Permission::Header(HeaderMatcher { name: "x".into(),
              mode: HeaderMatcherMode::SafeRegexMatch(envoy_config::SafeRegex {
                  regex: "(".into(), compiled: None }), invert_match: false })],
          principals: vec![envoy_config::Principal::Any(true)] });
      let cfg = envoy_config::RbacConfig { rules: envoy_config::Rules {
          action: envoy_config::Action::Allow, policies } };
      assert!(RbacFilter::build_from_config(&cfg, &registry, "ingress_http").is_err());
  }
  ```
- [ ] **Step 2: Run to verify they fail.** `cargo test -p envoy-filter safe_regex` — Expected: the `metadata_*` and
  `header_*` tests **PANIC** at `matcher.rs:90`'s `.expect(...)` (the M35-1 latent panic — proving the guard fires on
  the pre-fix tree); the malformed test fails (build currently succeeds, no compile attempt).
- [ ] **Step 3: Implement.** Make `lower_permission`/`lower_principal` fallible and compile SafeRegex on the clone:
  ```rust
  fn lower_permission(p: &envoy_config::Permission) -> Result<RuntimePermission, FilterError> {
      Ok(match p {
          envoy_config::Permission::Any(b) => RuntimePermission::Any(*b),
          envoy_config::Permission::Header(m) => {
              let mut m = m.clone();
              m.compile_safe_regexes().map_err(|e| FilterError::InvalidConfig { message: e.to_string() })?;
              RuntimePermission::Header(m)
          }
          envoy_config::Permission::AndRules(set) =>
              RuntimePermission::AndRules(set.rules.iter().map(lower_permission).collect::<Result<_, _>>()?),
          envoy_config::Permission::OrRules(set) =>
              RuntimePermission::OrRules(set.rules.iter().map(lower_permission).collect::<Result<_, _>>()?),
          envoy_config::Permission::NotRule(inner) =>
              RuntimePermission::NotRule(Box::new(lower_permission(inner)?)),
          envoy_config::Permission::Metadata(m) => {
              let mut m = m.clone();
              m.value.compile_safe_regexes().map_err(|e| FilterError::InvalidConfig { message: e.to_string() })?;
              RuntimePermission::Metadata(m)
          }
      })
  }
  ```
  Mirror for `lower_principal` (Header + Metadata arms identical; `AndIds`/`OrIds`/`NotId` thread `?`). Update
  `build_from_config` (line ~156): the `.map(|(name, policy)| …)` closure becomes fallible —
  ```rust
  let policies: Vec<RuntimePolicy> = cfg.rules.policies.iter()
      .map(|(name, policy)| -> Result<RuntimePolicy, FilterError> {
          Ok(RuntimePolicy {
              name: name.clone(),
              permissions: policy.permissions.iter().map(lower_permission).collect::<Result<_, _>>()?,
              principals: policy.principals.iter().map(lower_principal).collect::<Result<_, _>>()?,
          })
      })
      .collect::<Result<_, _>>()?;
  ```
  (`MetadataMatcher` has no top-level `compile_safe_regexes`; call it on `m.value` — only the value carries a
  SafeRegex. Confirm `SafeRegex` is re-exported from `envoy_config` for the test; it is used by `matcher.rs` tests
  already.)
- [ ] **Step 4: Run to verify pass.** `cargo test -p envoy-filter rbac` — all green; the two no-panic tests pass and
  the malformed test returns `Err`. The phase-10/35 tests (Any/Header-exact/And/Or/Not/Metadata-exact) unchanged.
- [ ] **Step 5: Commit.** `git add crates/envoy-filter/src/rbac.rs && git commit -m "phase 36 T4: fallible RBAC lowering compiles SafeRegex (closes M35-1) [ADR-0088]"`

---

## Task 5: Differential fixture `0044` (F1 present/absent + F2 regex-match/miss)

> §A2/A3/A3b/§3.5 (LOCKED): ONE fixture, ONE `action: ALLOW` RBAC with TWO policies — `f2_regex` (the F2 metadata
> `safe_regex` permission) and `f1_present` (the F1 `present_match` permission). RBAC `policies` are OR'd (any policy
> match → ALLOW), so each probe selects which policy fires via its headers. This keeps the F1 and F2 probe pairs
> independent without `and_rules`/`or_rules` nesting.

**Files:** Create `tests/fixtures/0044-http-rbac-matcher-value-enrichment/{envoy.yaml,envoy-rust.yaml,expectations.yaml,README.md}`;
create `tests/differential/tests/rbac_matcher_value_enrichment.rs`.

- [ ] **Step 1: Write the fixture.** `envoy.yaml` (mirror `0043`, the `admin`/`generate_request_id` upstream-side
  asymmetry): chain `[header_to_metadata (x-tier→tier), rbac, router]`, route `direct_response { status: 200, body:
  { inline_string: "ok\n" } }`. RBAC `action: ALLOW` with TWO policies:
  ```yaml
  policies:
    "f2_regex":                       # F2: metadata value safe_regex (ANCHORED §A3b)
      permissions:
        - metadata: { filter: envoy.filters.http.header_to_metadata, path: [ { key: tier } ],
                      value: { string_match: { safe_regex: { regex: "^(prod|staging)$" } } } }
      principals: [ { any: true } ]
    "f1_present":                     # F1: present_match on a SEPARATE key written by a 2nd rule
      permissions:
        - metadata: { filter: envoy.filters.http.header_to_metadata, path: [ { key: present_probe } ],
                      value: { present_match: true } }
      principals: [ { any: true } ]
  ```
  Add a SECOND `header_to_metadata` request_rule writing `x-present` → `present_probe` so the F1 probe is independent
  of the F2 `x-tier` value. Probes (`expectations.yaml`, the fixture-0043 status+body comparator + `extra_headers`):
  - probe a (F2 match): `x-tier: staging` → policy `f2_regex` matches → `200` + `ok\n`.
  - probe b (F2 miss): `x-tier: dev` (no `x-present`) → neither policy matches → `403` + `RBAC: access denied`.
  - probe c (F1 present): `x-present: 1` (any value), `x-tier: dev` → policy `f1_present` matches → `200` + `ok\n`.
  - probe d (F1 absent): no `x-present`, `x-tier: dev` → neither matches → `403` + `RBAC: access denied`.
  `envoy-rust.yaml` identical minus the upstream-only `admin`/`generate_request_id` (the 0043 asymmetry). README
  explains the chain + the 4 probes + the ANCHORED-pattern rationale (§A3b: partial==full) + the 19-byte body.
- [ ] **Step 2: Write the differential test** `rbac_matcher_value_enrichment.rs`, copying the 0043 entry verbatim
  (`tests/differential/tests/rbac_dynamic_metadata.rs`) and re-pointing the fixture dir. It is a plain `#[tokio::test]`
  with NO per-test `#[ignore]` — the differential harness skips at the cluster level when Docker is unavailable:
  ```rust
  use std::path::PathBuf;
  #[tokio::test]
  async fn rbac_matcher_value_enrichment() {
      let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
          .join("..").join("..")
          .join("tests/fixtures/0044-http-rbac-matcher-value-enrichment");
      differential::run_fixture(&dir).await.expect("fixture passes");
  }
  ```
- [ ] **Step 3: Run.** `cargo test -p differential rbac_matcher_value_enrichment` (the harness boots Docker Envoy +
  envoy-rust). Expected: both proxies byte-identical on all 4 probes. NOTE (memory): local Docker-differential
  false-REDs are documented host-only (bridge-IP / parallel-load timing) — CI is authoritative; run in isolation to
  confirm a real RED.
- [ ] **Step 4: Commit.** `git add tests/fixtures/0044-http-rbac-matcher-value-enrichment tests/differential/tests/rbac_matcher_value_enrichment.rs && git commit -m "phase 36 T5: fixture 0044 (F1 present_match + F2 safe_regex) [ADR-0088]"`

---

## Task 6: Fuzz seeds (existing `parse_bootstrap`, NO new target)

> §3.7: the existing `parse_bootstrap` target's reach extends to the new config; add two seeds. NO new fuzz target.

**Files:** Create two seed files under `crates/envoy-config/fuzz/corpus/parse_bootstrap/`; edit the fuzz `.gitignore`.

- [ ] **Step 1:** Add seed `rbac_present_match` — a minimal bootstrap with a `[header_to_metadata, rbac]` chain whose
  RBAC `metadata` value is `present_match: true`. Add seed `rbac_safe_regex` — the same with `value: { string_match:
  { safe_regex: { regex: "^(prod|staging)$" } } }` (anchored). (Reuse an existing `parse_bootstrap` seed as the
  skeleton.)
- [ ] **Step 2:** Per memory `fuzz-corpus-seed-gitignored-by-default`: add `!rbac_present_match` + `!rbac_safe_regex`
  un-ignore lines to the fuzz `.gitignore`. Verify with `git ls-files | grep -E 'rbac_present_match|rbac_safe_regex'`
  (MUST list both).
- [ ] **Step 3: Run the short-budget fuzz** locally if available: `cargo +nightly fuzz run parse_bootstrap -- -runs=50000`
  (or rely on the CI `parse_bootstrap` step — memory `new-fuzz-target-needs-a-ci-yml-step`: NO new target here, so no
  ci.yml change needed; confirm the existing step picks up the new seeds). Expected: clean.
- [ ] **Step 4: Commit.** `git add crates/envoy-config/fuzz && git commit -m "phase 36 T6: parse_bootstrap seeds (present_match + RBAC safe_regex) [ADR-0088]"`

---

## Task 7: BEHAVIOR_CONTRACT extension + full regression sweep

> §7.5 preview: document the new behavior; prove all 43 pre-existing fixtures + the workspace stay green.

**Files:** Modify `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (the phase-36 "HTTP filters" subsection).

- [ ] **Step 1:** Add a "Phase 36 — RBAC matcher-value enrichment" subsection extending the phase-35 RBAC `metadata`
  notes: (a) `present_match` presence semantics `match = present && want` (`false` never matches; present-empty →
  absent, §A1/A2); (b) the now-compiled RBAC SafeRegex (header + metadata) — SUPERSEDES the M35-1 limitation note; (c)
  the `^(prod|staging)$` ANCHORED-pattern note + the M36-1 carry-forward (unanchored partial-vs-full SafeRegex,
  deferred, cross-cutting); (d) `present_match: false`/`null_match`/etc. config-validity (present_match accepted; the
  rest stay boot-fatal).
- [ ] **Step 2: Full verification sweep** (the §7.5 gate (a)-(e) preview — the state-4 session quotes the outputs):
  `cargo build --workspace --all-targets`; `cargo clippy --workspace --all-targets --all-features -- -D warnings`;
  `cargo fmt --all -- --check`; `cargo test --workspace`; `cargo deny check`. Expected: all clean. (CI runs the Docker
  differential + h2spec — memory `envoy-rust-state4-ci-first-execution`: the Docker differential first runs clean on
  CI; budget CI iteration.)
- [ ] **Step 3: Commit.** `git add docs/envoy-rust/BEHAVIOR_CONTRACT.md && git commit -m "phase 36 T7: BEHAVIOR_CONTRACT phase-36 subsection + regression sweep [ADR-0088]"`

---

## Acceptance (the §7.5 phase-done gate, previewed; verified at the SEPARATE state-4 session)

(a) fixture `0044` green (cross-proxy byte-identical: F1 present→`200`+`ok\n` / absent→`403`+19B; F2 match→`200` /
miss→`403`) + (b) all `0001`–`0043` green (incl. `0043` rbac-metadata-`exact` + `0017` rbac header-only +
`0012`/`0041`/`0042`) + (c) h2spec ≥95% (unchanged) + (d) `parse_bootstrap` (+ `accesslog_format_parse`, unchanged)
fuzz clean for the short CI run with the new seeds — NO new fuzz target + (e) `cargo build`/`clippy`/`fmt`/`test
--workspace`/`deny check` clean + (f) `REVIEW.md` approved. `#![forbid(unsafe_code)]` holds (D-3.8); no new crate /
dependency / `HttpFilterInstance` variant / `ConfigError`/`FilterError` variant (D-3.2). **M35-1 CONSUMED** by F2;
**M36-1 NEW** (unanchored SafeRegex partial-vs-full, deferred). The §6.1 split did NOT fire (ADR-0089 reserved, unused).

---

_Scope locked by **ADR-0087**; §6.2 reconciled by **ADR-0088** (the facts in §A). The §6.1 split did NOT fire
(~7 tasks; **ADR-0089 reserved/unused**). The state-3 implementation is the next session
(`superpowers:subagent-driven-development`)._
