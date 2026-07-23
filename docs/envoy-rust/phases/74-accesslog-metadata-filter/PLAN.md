# Phase 74 — access-log `metadata_filter` (the DYNAMIC-METADATA emission gate) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Every task is TDD (superpowers:test-driven-development) — write the failing test, watch it fail, implement, watch it pass, commit.

**Goal:** Land the SIXTH `envoy.config.accesslog.v3.AccessLogFilter` oneof arm — `metadata_filter` — so an access-log sink emits a record iff the request's DYNAMIC METADATA, resolved at `matcher.filter` → `matcher.path[0].key`, matches `matcher.value`; or, when the path does not resolve, iff `match_if_key_not_found` (default **`true`**), behaviorally equivalent to `envoyproxy/envoy:v1.33.0`.

**Architecture:** Extend the existing phase-70/71/72/73 access-log FILTER seam. Add one config struct (`MetadataFilter { matcher: Option<MetadataMatcher>, match_if_key_not_found: Option<bool> }` — BOTH fields `Option`, for measured reasons), an access-log-scoped fail-loud matcher validator with ONE new `ConfigError`, a new `MetadataMatch` trait-object seam in `envoy-accesslog` (the ADR-0150 cycle pattern, returning **`Option<bool>`** so the not-found policy stays in `LogFilter`), the `LogFilter::Metadata` runtime variant, and a 4th `should_log` parameter carrying the metadata store. Two backend-free byte-exact H1 differential fixtures witness the value keep/drop and the `match_if_key_not_found: false` absent-drop cross-proxy. The phase-35/36 `MetadataMatcher`/`ValueMatcher`/`StringMatcher` config types, the `ValueMatcher::matches` engine, and the entire byte-exact differential driver are reused UNCHANGED.

**Tech Stack:** Rust (workspace crates `envoy-config`, `envoy-accesslog`, `envoy-http1`, `envoy-http2`), serde/serde_yaml, thiserror, the `testcontainers` differential harness, `cargo fuzz` (libfuzzer).

## Global Constraints

- **Target parity:** `envoyproxy/envoy:v1.33.0` (`docs/envoy-rust/ENVOY_TARGET.md`, digest `sha256:56da5afd…770c2`). Do NOT read Envoy C++ source to decide equivalence — the differential harness + `BEHAVIOR_CONTRACT.md` are the contract (D-3.3).
- **`#![forbid(unsafe_code)]`** holds at every crate root (D-3.8). No `unsafe`.
- **ADR-0150 seam is load-bearing:** `envoy-accesslog` has ZERO workspace dependencies (`crates/envoy-accesslog/Cargo.toml`: only `tokio`, `bytes`, `tracing`, `thiserror`) and `envoy-config` already depends on it (`crates/envoy-config/Cargo.toml:14`), so the reverse edge is a hard Cargo CYCLE. The new metadata matcher MUST be an injected **trait object** (`Arc<dyn MetadataMatch>`), exactly like `LogFilter::Header`'s `Arc<dyn HeaderMatch>`. `LogFilter` derives ONLY `Debug, Clone` — introduce **NO** `Eq`/`PartialEq` and **NO** `envoy-config` dependency.
- **`AccessLogFilter` does NOT derive `Clone`** — every consumer takes `&AccessLogFilter`. `MetadataFilter` matches that (no `Clone`). The nested `MetadataMatcher` DOES derive `Clone` (`bootstrap.rs:1632`) — that is what the compile step clones into the `Arc`.
- **`matcher` is OPTIONAL (a load-parity trap, MEASURED R-0.2):** upstream ACCEPTS a matcher-less `metadata_filter: {}` (`configuration OK`). envoy-rust MUST NOT reject it. A matcher-less filter keeps every record (`match_if_key_not_found`, default `true`).
- **`match_if_key_not_found` is a `google.protobuf.BoolValue` WRAPPER (MEASURED R-0.2):** absent and explicit-`false` are DISTINCT on the wire, so the field is `Option<bool>`, NOT `bool`. `None` resolves to `true` at compile (MEASURED R-0.4, via the absent → explicit-`false` polarity flip that `--mode validate` provably cannot reach).
- **`matcher.invert` stays BOOT-FATAL (CF-74-1).** MEASURED R-0.5: upstream ACCEPTS `invert` on this path but it is **INERT** (reproduced twice; an `invertBOGUS` control is REJECTED, proving the field is genuine). The in-tree `MetadataMatcher` has no `invert` field under `#[serde(deny_unknown_fields)]`, so a config carrying it is rejected at load. Do NOT "implement" `invert` here — doing so would CREATE a divergence (ADR-0049 fail-loud posture).
- **Two DIFFERENT `invert` fields — do not conflate them.** `HeaderMatcher.invert_match` (CF-72-1, `matcher.rs:51`) is MODE-SCOPED and guarded by two parity pins; this phase does NOT touch the header-match engine.
- **Fixture log FORMAT constraint:** envoy-rust's `%REQ(NAME)%` supports only `REQ_ALLOW_LIST` = `:method`, `:authority`, `:path`, `x-envoy-original-path`, `x-forwarded-for`, `user-agent`, `x-request-id` (`crates/envoy-accesslog/src/command_operator.rs:95-103`); a non-allow-listed `%REQ(X-A)%` is boot-fatal (`FormatParseError::UnsupportedHeader` → `ConfigError::InvalidAccessLogFormat`, ADR-0153 PV-6). **`%DYNAMIC_METADATA(ns:key)%` is a SEPARATE `Op` with its own parser and is NOT allow-list-gated** — so this phase's fixtures CAN render the gating value directly. Format: `STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)% M=%DYNAMIC_METADATA(com.example:k)%\n`.
- **`on_header_missing` REQUIRES a `value` in envoy-rust** (`validate_header_to_metadata_config`, `bootstrap.rs:~4548`: `"on_header_missing for header '{}' requires a 'value'"`). Fixture `0082` must therefore OMIT `on_header_missing` entirely so the key is genuinely ABSENT on the no-header probe — it must NOT copy fixture `0042`'s `on_header_missing` block.
- **Kept-LAST authoring convention (ADR-0147):** in every fixture the DROPPED probe(s) come FIRST, the KEPT probe(s) LAST, so the driver's ordering-aware `suppression_settle` (`tests/differential/src/lib.rs:1694-1699`) pays only the cheap 2 s `CF70_3_SETTLE` instead of the 12 s `CF71_1_SETTLE`.
- **Do not disturb** the 34 existing access-log fixtures (`0076`–`0080` included), `known-failures.txt`, or the phase-73 `< 2` composition-arm rule. Any ROADMAP row edit escapes `\|` and preserves 6 cells; rows `36`/`38`/`39`/`52`/`54` are already malformed — do NOT "fix" them (append-only).
- **envoy-bin writes `ConfigError` to STDOUT** and takes only `-c <path>`. `cargo build -p envoy-bin` before ANY local differential (memory `differential-harness-uses-debug-envoy-bin`). `cargo fuzz` runs from the crate dir. A new corpus seed needs a `!`-un-ignore line (memory `fuzz-corpus-seed-gitignored-by-default`).
- **Every line number below was re-grepped at this state-2 PLAN-write. They DRIFT as tasks land — re-grep before each edit; never trust a quoted offset.**

---

## §6.2 empirical reconciliation — the SPEC §3 PLAN-VERIFY lock-ins (recorded in ADR-0155)

Every item below was RE-MEASURED against the LIVE tree this session (five read-only recon fan-outs + main-session verification). Where a measurement corrects the SPEC, the correction is named.

### PV-1 — serde model: **CONFIRMED as specified.**

`AccessLogFilter` (`crates/envoy-config/src/bootstrap.rs:713-742`; doc 713-720, struct 721-742) is
`#[derive(Debug, Default, Serialize, Deserialize, PartialEq)] #[serde(default, deny_unknown_fields)]` with FIVE `Option` arms and **NO `Clone`**. `AndFilter`/`OrFilter`/`ResponseFlagFilter` (all `#[derive(Debug, Default, Serialize, Deserialize, PartialEq)] #[serde(default, deny_unknown_fields)]`) are the template for `MetadataFilter`, whose fields are both defaultable.

`MetadataMatcher` (`:1628-1638`) = `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)] #[serde(deny_unknown_fields)] { filter: String, path: Vec<MetadataPathSegment>, value: ValueMatcher }` — all three REQUIRED (no `#[serde(default)]`). `MetadataPathSegment` (`:1640-1644`), `ValueMatcher` (`:1658-1670`), `StringMatcher` (`:2915-2944`) **all derive `Clone`** — so cloning a `MetadataMatcher` into an `Arc` needs no derive change.

`ValueMatcher`'s `Deserialize` is macro-generated (`impl_single_key_oneof!`, macro at `:1672-1742`, invocation at `:1744-1752`): a map with EXACTLY ONE key ∈ `{string_match, present_match}`; zero keys, >1 key, or any other key → error. `StringMatcher` has its own hand-rolled `Deserialize` (`:2951-3043`). **Both are self-contained `impl<'de> Deserialize<'de>` blocks** — container attributes (`default`/`deny_unknown_fields`) on a PARENT struct are consumed by the parent's DERIVED impl only and never reach a nested type's own impl. They therefore compose UNCHANGED under the new `MetadataFilter`. Precedent in-tree: `Permission::Metadata(MetadataMatcher)` (`:1885`) and `Principal::Metadata(MetadataMatcher)` (`:1942`) already nest the same type inside single-key-oneof parents.

`MetadataMatcher`/`MetadataPathSegment`/`ValueMatcher`/`StringMatcher`/`StringMatcherMode` are **ALREADY re-exported** from `crates/envoy-config/src/lib.rs:14-40` — only `MetadataFilter` needs adding (alphabetically between `MetadataEntry` and `MetadataMatcher` on line 30).

**Construction-site census** (`rg -n "AccessLogFilter\s*\{" crates/`): **13 FULL struct literals** that will fail to compile when a 6th field lands — `crates/envoy-http1/src/hcm.rs` ×10 (`4535`, `4681`, `4756`, `4785`, `4800`, `4820`, `4827`, `4858`, `4997`, `5016`) and `crates/envoy-config/src/bootstrap.rs` ×3 (`13197`, `13234`, `13293`) — plus **6** `..AccessLogFilter::default()` shorthand sites (`bootstrap.rs` `13251`, `13267`, `13280`, `13306`, `13320`, `13326`) that need NO change, plus **1** destructure (`bootstrap.rs:5254`).

### PV-2 — the access-log-scoped matcher validator: **CONFIRMED; option (a) chosen.**

`validate_metadata_matcher` (`bootstrap.rs:4824-4858`) is `fn validate_metadata_matcher(m: &crate::MetadataMatcher, listener_name: &str, policy_name: &str, path: &str)` — private, **immutable** `&`, and it yields ONLY `ConfigError::RbacMetadataMatcherInvalid { listener, policy_name, path, detail }` (`lib.rs:664-680`), a variant that structurally CARRIES `listener`/`policy_name`. The access-log path has neither in scope (`validate_access_log_filter` receives only `filter: &mut AccessLogFilter`). Its single call site is inside the `define_rbac_tree_validator!` macro (`bootstrap.rs:4668-4670`). **6 tests couple to `RbacMetadataMatcherInvalid`** (`bootstrap.rs:6757`, `15525`, `15545`, `15563`, `15582`; `crates/envoy-bin/tests/network_filter_rbac.rs:1121`).

→ **Decision: (a) a NEW access-log-scoped validator + ONE new `ConfigError` variant.** Refactoring the RBAC one would change an RBAC error shape across 6 tests and two subsystems for zero gain. The new variant lands with the other `AccessLogFilter`-arm variants, after `UnknownResponseFlag` (`lib.rs:475-479`) and before `Http2ClusterFromHttp1Listener` (`lib.rs:481`), and carries only `{ detail: String }` — matching `AmbiguousAccessLogFilter`/`InsufficientCompositeFilters` (no listener/policy fields, because none are available).

**SafeRegex in-place compile CONFIRMED workable.** `ValueMatcher::compile_safe_regexes` (`bootstrap.rs:5539-5547`) is `pub fn compile_safe_regexes(&mut self) -> Result<(), crate::ConfigError>`. `validate_access_log_filter` (`bootstrap.rs:5253`) already takes `&mut AccessLogFilter`, exactly as the `header_filter` arm exploits with `validate_header_matcher(&mut hf.header)` (`bootstrap.rs:5294-5300`). The RBAC path canNOT do this (immutable borrow) and defers to filter-lowering, where `compile_metadata` (`crates/envoy-filter/src/rbac.rs:231-241`) clones first — that asymmetry is real and is why the access-log path gets its own validator.

**MEASURED CORRECTION to the SPEC's inherited-behavior claim:** `validate_metadata_matcher` checks only `filter.is_empty()` and `path.len() != 1`. It does **NOT** check that a path segment's `key` is non-empty. Upstream's PGV DOES (`key` `min_len: 1`, R-0.2). The new access-log validator therefore adds that third check (the SPEC §2.1 item 3(b) list is correct; the "inherited verbatim from RBAC" reading is not). The RBAC gap is NOT fixed here (out of scope — a different error shape and six coupled tests); it is opened as **CF-74-4**.

### PV-3 — the 6-arm cardinality: **CONFIRMED exactly as specified.**

The destructure at `bootstrap.rs:5254-5260` names every field with **NO `..`** (deliberate — the doc at `:5247` says "no `..`, so a future arm cannot be added without updating this [M70-R1]"), so adding `metadata_filter` breaks the build until handled. The `set_arms` array at `:5261-5270` is a plain array literal and is **NOT length-checked by the compiler** — an arm added to the struct but forgotten in the array would silently count as ZERO (turning a valid single-arm filter into `AmbiguousAccessLogFilter{"no filter variant is set"}`). It must be grown to 6 by hand and pinned by a test that asserts EACH of the six arms ALONE validates.

### PV-4 — the `MetadataMatch` seam: **`Option<bool>` CONFIRMED; the recon's alternative REJECTED on a measured basis.**

`envoy-accesslog` has zero workspace deps; `envoy-config` depends on it — the cycle is real, the trait-object seam is mandatory. `pub trait HeaderMatch: std::fmt::Debug + Send + Sync` (`filter.rs:32-35`) is the exact template; its sole impl is `impl envoy_accesslog::HeaderMatch for HeaderMatcher` (`crates/envoy-config/src/matcher.rs:55-68`).

There is **no inherent `impl MetadataMatcher`** anywhere in the tree (`rg "impl MetadataMatcher" crates/` → no hits), so — unlike `HeaderMatch`, which relies on method-call syntax preferring the inherent method — the new trait impl needs no delegation trick and no recursion hazard.

**A recon fan-out proposed reusing `ValueMatcher::matches_resolved(Option<&str>) -> bool` (`matcher.rs:132-139`) with a plain `bool` return, since no `Option<bool>` contract exists anywhere in the tree. MEASURED-REJECTED:** `matches_resolved` maps an UNRESOLVED path to `false` (`StringMatch(sm) => resolved.is_some_and(|v| sm.matches(v))`). The access-log rule maps an unresolved path to `match_if_key_not_found`, whose default is **`true`** (MEASURED R-0.4: the identical no-header probe was KEPT under the default and DROPPED under explicit `false`). Collapsing "unresolved" into `false` would therefore DROP every key-absent record — the exact opposite of the measured upstream behavior. `Option<bool>` is the contract that preserves the three-way distinction.

The other SPEC alternative — a `bool`-returning trait taking the default as a 2nd argument — is rejected because it moves the `match_if_key_not_found` policy into `envoy-config`'s impl, away from `LogFilter` where the field lives, and would express R-0.4's rule twice.

**Final shape:** `pub trait MetadataMatch: std::fmt::Debug + Send + Sync { fn matches(&self, dynamic_metadata: &BTreeMap<String, BTreeMap<String, String>>) -> Option<bool>; }`. `None` iff the path did not resolve. `LogFilter::Metadata { matcher: Option<Arc<dyn MetadataMatch>>, match_if_key_not_found: bool }` applies the default. No `Eq`/`PartialEq`; no new dependency.

**One residual, recorded honestly:** R-0.3/R-0.4 measured the rule with a `string_match` value. Applying `ValueMatcher::matches` (not `matches_resolved`) on the RESOLVED branch means `present_match: true` → KEEP and `present_match: false` → DROP for a resolved key, with an ABSENT key going to `match_if_key_not_found` in both cases. That is the structural consequence of the measured rule (the value matcher is only consulted when the path resolves), but the `present_match` composition itself was NOT separately live-probed. It is pinned in-process only and opened as **CF-74-5**.

**A public `DynamicMetadata` type alias in `envoy-accesslog` was considered and rejected** — `record.rs:111-112` spells the type out today; adding an alias would either leave two spellings or force unrelated churn. The four signatures spell it explicitly.

### PV-5 — the `should_log` widening: **census re-confirmed; the 4th-parameter widening chosen; the record-based alternative measured-rejected.**

`rg -n "should_log" crates/ tests/` → **104 raw hits**, of which **102 are code** — `crates/envoy-http1/src/hcm.rs` 49, `crates/envoy-accesslog/src/filter.rs` 42, `crates/envoy-accesslog/src/file_sink.rs` 7, `crates/envoy-http2/src/hcm.rs` 4 — and 2 are README prose (`tests/fixtures/0076-…/README.md`, `tests/fixtures/0077-…/README.md`). **The SPEC's "102 call sites" figure is exactly the code count; it is CONFIRMED, not corrected.**

Genuine call/definition sites: the definition (`filter.rs:71`), the two internal `And`/`Or` recursive calls (`filter.rs:105-110` — these must thread the new argument too), the `FileSink::should_log` wrapper (`file_sink.rs:99-112`) and its one internal delegation, the H1 emit gate (`hcm.rs:1515`) and the H2 emit gate (`envoy-http2/src/hcm.rs:1138`). Of `envoy-http2`'s 4 hits, only ONE is a call — the other three are doc comments. Everything else is in-process test code.

**Both emit gates VERIFIED to hold the built record.** H1: `let record = build_access_log_record(…);` completes at `hcm.rs:1507` and the `for sink in &config.access_log` loop begins at `:1508`, calling `sink.should_log(record.response_code, &record.response_flags, &req.headers)` at `:1515` — `&record.dynamic_metadata` is trivially available. H2: the `AccessLogRecord { … dynamic_metadata }` struct literal completes at `envoy-http2/src/hcm.rs:1130` and the loop begins at `:1131`, calling `should_log` at `:1138` — same. Both build the record BEFORE the loop.

**The record-based consolidation `should_log(&AccessLogRecord, &[(String,String)])` is REJECTED on a measured blocker:** `AccessLogRecord` **deliberately does NOT implement `Default`** (`record.rs:27-42` doc: "Intentionally does NOT implement `Default` … every field must be populated explicitly … so silent omissions can't ship"), and its only test constructor is `#[cfg(test)] pub(crate) fn test_baseline()` — `pub(crate)` to `envoy-accesslog`, therefore INVISIBLE to `envoy-http1`'s 49 test sites. Consolidation would force a new public test-only record builder plus a full-literal rewrite of ~98 call sites, versus appending one argument. Option (a) it is.

**Single compile site confirmed:** `envoy-http2` wraps the H1 config (`HCMConfig { inner: Arc<Http1HCMConfig> }`, `envoy-http2/src/hcm.rs:37-38`) and reads `config.inner.access_log`, so `compile_access_log_filter` in `envoy-http1` remains the ONLY compile site. No H2 compile work.

### PV-6 — driver reuse + fixture shape: **ZERO driver change CONFIRMED; one MEASURED correction to the fixture recipe.**

`Driver::Http1AccessLogByteExact { probes, expected_access_log_paths }` (`tests/differential/src/lib.rs:159-165`), `AccessLogByteExactProbe` (`:1102-1120`, `#[serde(deny_unknown_fields)]`, fields `method`/`path`/`host`/`extra_headers`/`body`/`expected_status`/`expect_logged`), `expected_logged_count` (`:1130-1136`) and `run_http1_access_log_byte_exact_arm` (`:6265-6442`) need **ZERO** change. Fixtures are auto-discovered by path (no registry). `suppression_settle` (`:1694-1699`) picks `CF71_1_SETTLE` (12 s) iff the LAST probe is dropped, else `CF70_3_SETTLE` (2 s) — both new fixtures order kept-LAST and pay 2 s.

`%DYNAMIC_METADATA(namespace:key)%` (`command_operator.rs:79-86` variant, `:367-418` parser, `:549-556` renderer) requires exactly two non-empty `:`-separated segments, is case-sensitive, is NOT gated by `REQ_ALLOW_LIST`, renders the raw unquoted value when present, and renders `-` when EITHER the namespace OR the key is absent (pinned by `renders_absent_key_and_namespace_dash`, `command_operator.rs:~1015`). `com.example:k` is a valid argument.

`header_to_metadata`'s in-tree surface (`HeaderToMetadataConfig`/`HeaderToMetadataRule`/`HeaderToMetadataKeyValue`, `bootstrap.rs:1410-1453`) matches fixture `0042`'s YAML exactly: `request_rules: [{ header, on_header_present?: { metadata_namespace?, key, value?, type? }, on_header_missing?: {…} }]`.

**MEASURED CORRECTION — fixture `0082` must NOT clone `0042` wholesale.** `validate_header_to_metadata_config` (`bootstrap.rs:~4548-4589`) enforces `"on_header_missing for header '{}' requires a 'value'"`, and fixture `0042` supplies one (`value: "missing"`). If `0082` carried that block, the no-header probe would WRITE `com.example:k = "missing"` — the key would RESOLVE, the `exact: "1"` value matcher would fail, and the probe would be dropped by the VALUE path, not the key-not-found path. The fixture would then pass while testing the wrong thing. **`0082` therefore omits `on_header_missing` entirely**, exactly as the R-0.3/R-0.4 live recon config did.

`tests/fixtures/` holds exactly 80 directories, the highest being `0080-accesslog-or-filter`; `0081`/`0082` do not exist. `rg -n "metadata_filter" tests/` → no hits.

### PV-7 — fuzz: **CONFIRMED — seed only, no new target, no `ci.yml` edit.**

`crates/envoy-config/fuzz/fuzz_targets/` holds exactly ONE target (`parse_bootstrap.rs`, a 10-line `fuzz_target!` over `envoy_config::parse_bootstrap`). The CI fuzz job's step (`.github/workflows/ci.yml:102-107`) is `cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30` — it names NO corpus path, so `cargo fuzz` globs `crates/envoy-config/fuzz/corpus/parse_bootstrap/` implicitly and any newly-tracked seed is picked up with **zero workflow change** (ADR-0137 precedent). `crates/envoy-config/fuzz/.gitignore` line 1 is `corpus/parse_bootstrap/*` followed by 62 `!`-un-ignore lines (2-63); `git ls-files` returns exactly those 62 files while ~10 584 libFuzzer hash-named artifacts sit invisibly ignored. A 63rd `!` line is mandatory.

### PV-8 — the §6.1 split gate: **re-derived at ~880 net LoC / 10 tasks → NO SPLIT. ADR-0156 stays UNFIRED.**

Table below (`§6.1 Split gate — re-derived`). Comfortably under the ~1500 LoC / ~25 task gate. The SPEC's ~935 estimate is confirmed within noise; the `should_log` widening remains the single largest line item and carries no design risk.

---

## File Structure

| File | Responsibility | Tasks |
|---|---|---|
| `crates/envoy-config/src/bootstrap.rs` | `MetadataFilter` struct, the `metadata_filter` oneof field, the N73-R1 doc fix, the 6-arm destructure + `set_arms`, the access-log-scoped `validate_access_log_metadata_matcher` | T1, T2 |
| `crates/envoy-config/src/lib.rs` | re-export `MetadataFilter`; the new `ConfigError::AccessLogMetadataMatcherInvalid` variant | T1, T2 |
| `crates/envoy-accesslog/src/filter.rs` | the 4th `should_log` parameter; `pub trait MetadataMatch`; `LogFilter::Metadata` + its `should_log` arm | T3, T4 |
| `crates/envoy-accesslog/src/lib.rs` | re-export `MetadataMatch` | T4 |
| `crates/envoy-accesslog/src/file_sink.rs` | the `FileSink::should_log` wrapper's 4th parameter | T3 |
| `crates/envoy-http2/src/hcm.rs` | pass `&record.dynamic_metadata` at the H2 emit gate | T3 |
| `crates/envoy-config/src/matcher.rs` | the sole `impl envoy_accesslog::MetadataMatch for MetadataMatcher` | T5 |
| `crates/envoy-http1/src/hcm.rs` | the 13→10 `AccessLogFilter` test literals; `&record.dynamic_metadata` at the H1 emit gate; the 6-tuple `compile_access_log_filter` | T1, T3, T6 |
| `tests/fixtures/0081-accesslog-metadata-filter/` | value keep/drop differential fixture | T7 |
| `tests/fixtures/0082-accesslog-metadata-filter-key-not-found/` | `match_if_key_not_found: false` absent-drop fixture | T8 |
| `tests/differential/tests/access_log_metadata_filter.rs` | 0081 entrypoint | T7 |
| `tests/differential/tests/access_log_metadata_filter_key_not_found.rs` | 0082 entrypoint | T8 |
| `crates/envoy-config/fuzz/corpus/parse_bootstrap/metadata_filter.yaml` + `fuzz/.gitignore` | fuzz seed + `!`-un-ignore | T9 |
| `docs/envoy-rust/BEHAVIOR_CONTRACT.md` | the phase-74 `metadata_filter` subsection | T10 |

**Task ordering is load-bearing:** the config model lands first (its compiler-forced destructure is the fail-loud guard); then the validator; then the `should_log` widening as a PURE mechanical refactor with no behavior change; then the trait + runtime variant; then the config-side impl; then the compile step (which is what makes a `metadata_filter` config non-panicking); only then the fixtures, fuzz seed, and contract.

> **Interim-state note (the phase-73 T1/T4 precedent).** Between T1 and T6, a config that sets `metadata_filter` parses and validates but falls into `compile_access_log_filter`'s `_ => unreachable!(…)` and PANICS. That window closes in T6, before any fixture (T7/T8) exercises the path. Do not "fix" it early by weakening the `unreachable!` — it is the guard that proves the validator and the compile match stay in lockstep.

---

### Task 1: Config model — `MetadataFilter`, the `metadata_filter` oneof field, the 6-arm cardinality, re-export, the N73-R1 doc fix, and the construction-site fan-out

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (doc `713-720`; struct `721-742`; new struct after `OrFilter` at `759`; destructure `5254-5260`; `set_arms` `5261-5270`; test literals `13197`, `13234`, `13293`)
- Modify: `crates/envoy-config/src/lib.rs` (re-export block `14-40`)
- Modify: `crates/envoy-http1/src/hcm.rs` (10 test literals: `4535`, `4681`, `4756`, `4785`, `4800`, `4820`, `4827`, `4858`, `4997`, `5016`)

**Interfaces:**
- Produces: `pub struct MetadataFilter { pub matcher: Option<MetadataMatcher>, pub match_if_key_not_found: Option<bool> }`; a new field `pub metadata_filter: Option<MetadataFilter>` on `AccessLogFilter`; `envoy_config::MetadataFilter` re-export.
- Consumes: the existing `MetadataMatcher` / `MetadataPathSegment` / `ValueMatcher` / `StringMatcher` (already re-exported, unchanged).

**Context:** `AccessLogFilter` (`bootstrap.rs:721-742`) is `#[derive(Debug, Default, Serialize, Deserialize, PartialEq)] #[serde(default, deny_unknown_fields)]` with five `Option` arms and NO `Clone`. `AndFilter` (`:744-751`) is the model for the new struct — `Default` + `#[serde(default)]` is REQUIRED so a matcher-less `metadata_filter: {}` deserializes to `{ matcher: None, match_if_key_not_found: None }` and PASSES validation (the R-0.2 load-parity trap). Adding the field breaks the no-`..` destructure at `:5254` AND the 13 full struct literals — all fixed in this task so the tree compiles.

- [ ] **Step 1: Write the failing serde + cardinality tests**

Add to the `#[cfg(test)] mod tests` in `crates/envoy-config/src/bootstrap.rs`, next to the phase-73 access-log-filter tests (around `13185`-`13340`):

```rust
    // --- phase 74 t1: MetadataFilter + metadata_filter oneof arm ---

    #[test]
    fn metadata_filter_deserialize_round_trip_and_defaults() {
        // The full shape. `match_if_key_not_found` is a BoolValue WRAPPER
        // (MEASURED R-0.2) written as a bare bool in YAML.
        let yaml = r#"
metadata_filter:
  matcher:
    filter: com.example
    path:
      - key: k
    value:
      string_match: { exact: "1" }
  match_if_key_not_found: false
"#;
        let f: AccessLogFilter = serde_yaml::from_str(yaml).expect("deserializes");
        let mf = f.metadata_filter.as_ref().expect("metadata_filter present");
        let m = mf.matcher.as_ref().expect("matcher present");
        assert_eq!(m.filter, "com.example");
        assert_eq!(m.path.len(), 1);
        assert_eq!(m.path[0].key, "k");
        assert_eq!(m.value, ValueMatcher::StringMatch(StringMatcher {
            mode: StringMatcherMode::Exact("1".into()),
            ignore_case: false,
        }));
        assert_eq!(mf.match_if_key_not_found, Some(false));

        // MEASURED R-0.2 LOAD-PARITY TRAP: upstream ACCEPTS a matcher-less
        // `metadata_filter: {}` (`configuration OK`), so both fields default.
        let empty: MetadataFilter = serde_yaml::from_str("{}").expect("empty metadata_filter");
        assert_eq!(empty, MetadataFilter::default());
        assert!(empty.matcher.is_none());
        // Absent is DISTINCT from explicit `false` on the wire (BoolValue
        // wrapper) — `None` means "default", resolved to `true` at compile
        // (MEASURED R-0.4). Modelling this as a bare `bool` would lose it.
        assert_eq!(empty.match_if_key_not_found, None);

        // The message is CLOSED upstream (R-0.2); `deny_unknown_fields` mirrors it.
        assert!(serde_yaml::from_str::<MetadataFilter>("bogus_field: true").is_err());

        // CF-74-1: `matcher.invert` is ACCEPTED-but-INERT upstream (MEASURED
        // R-0.5) and has no in-tree field — it stays BOOT-FATAL here. Adding it
        // would CREATE a divergence.
        assert!(
            serde_yaml::from_str::<MetadataFilter>(
                "matcher: { filter: f, path: [{key: k}], value: { present_match: true }, invert: true }"
            )
            .is_err(),
            "matcher.invert must stay boot-fatal (CF-74-1)"
        );
    }

    #[test]
    fn six_arm_cardinality_counts_every_arm() {
        // PV-3: the destructure is compiler-forced (no `..`), but the `set_arms`
        // ARRAY is not length-checked — an arm present in the struct yet missing
        // from the array would count as ZERO, turning a valid single-arm filter
        // into `AmbiguousAccessLogFilter{"no filter variant is set"}`. Assert
        // each of the SIX arms ALONE validates, and that all six together are
        // "more than one".
        let single_arms: Vec<AccessLogFilter> = vec![
            AccessLogFilter {
                status_code_filter: Some(StatusCodeFilter {
                    comparison: ComparisonFilter {
                        op: ComparisonOp::Ge,
                        value: RuntimeUInt32 {
                            default_value: 500,
                            runtime_key: "rk".into(),
                        },
                    },
                }),
                ..AccessLogFilter::default()
            },
            AccessLogFilter {
                response_flag_filter: Some(ResponseFlagFilter {
                    flags: vec!["NR".into()],
                }),
                ..AccessLogFilter::default()
            },
            exact_header("x-a", "1"),
            AccessLogFilter {
                and_filter: Some(AndFilter {
                    filters: vec![exact_header("x-a", "1"), exact_header("x-b", "1")],
                }),
                ..AccessLogFilter::default()
            },
            AccessLogFilter {
                or_filter: Some(OrFilter {
                    filters: vec![exact_header("x-a", "1"), exact_header("x-b", "1")],
                }),
                ..AccessLogFilter::default()
            },
            AccessLogFilter {
                metadata_filter: Some(MetadataFilter::default()),
                ..AccessLogFilter::default()
            },
        ];
        assert_eq!(single_arms.len(), 6, "six arms must be covered");
        for (idx, f) in single_arms.into_iter().enumerate() {
            validate_access_logs(&mut file_log_with_filter(f))
                .unwrap_or_else(|e| panic!("arm {idx} alone must validate, got {e:?}"));
        }

        let all_six = AccessLogFilter {
            status_code_filter: Some(StatusCodeFilter {
                comparison: ComparisonFilter {
                    op: ComparisonOp::Ge,
                    value: RuntimeUInt32 {
                        default_value: 500,
                        runtime_key: "rk".into(),
                    },
                },
            }),
            response_flag_filter: Some(ResponseFlagFilter {
                flags: vec!["NR".into()],
            }),
            header_filter: Some(HeaderFilter {
                header: HeaderMatcher {
                    name: "x-a".into(),
                    mode: HeaderMatcherMode::ExactMatch("1".into()),
                    invert_match: false,
                },
            }),
            and_filter: Some(AndFilter {
                filters: vec![exact_header("x-a", "1"), exact_header("x-b", "1")],
            }),
            or_filter: Some(OrFilter {
                filters: vec![exact_header("x-a", "1"), exact_header("x-b", "1")],
            }),
            metadata_filter: Some(MetadataFilter::default()),
        };
        let err = validate_access_logs(&mut file_log_with_filter(all_six)).expect_err("ambiguous");
        assert!(matches!(
            err,
            crate::ConfigError::AmbiguousAccessLogFilter { ref detail } if detail.contains("more than one")
        ));
    }
```

> `file_log_with_filter` (`bootstrap.rs:13222`) and `exact_header` (`:13233`) already exist in that test module. If `StatusCodeFilter` / `ComparisonFilter` / `ComparisonOp` / `RuntimeUInt32` / `ResponseFlagFilter` / `MetadataFilter` / `ValueMatcher` / `StringMatcher` / `StringMatcherMode` are not in scope, add them to the module's `use super::*;` imports — all live in `bootstrap`.

- [ ] **Step 2: Run the tests to verify they fail (compile error — no such field/type)**

Run: `cargo test -p envoy-config --lib metadata_filter_deserialize_round_trip_and_defaults six_arm_cardinality_counts_every_arm 2>&1 | tail -30`
Expected: FAIL — `error[E0433]: failed to resolve: use of undeclared type 'MetadataFilter'` and `error[E0560]: struct 'AccessLogFilter' has no field named 'metadata_filter'`.

- [ ] **Step 3: Add the struct, the oneof field, the re-export, and the N73-R1 doc fix**

In `crates/envoy-config/src/bootstrap.rs`, replace the STALE doc comment (`713-720`, which still says "THREE oneof arms" — carry-forward **N73-R1**) and extend the struct:

```rust
/// Models `envoy.config.accesslog.v3.AccessLogFilter` — the per-record emission
/// predicate carried by an `AccessLog` entry. This type models SIX oneof arms —
/// `status_code_filter` (phase 70), `response_flag_filter` (phase 71),
/// `header_filter` (phase 72), the recursive `and_filter` / `or_filter`
/// composition (phase 73), and `metadata_filter` (phase 74); future
/// filter-family phases add further `Option` arms here rather than reshaping the
/// type. Cardinality (exactly one arm set) is enforced by `validate_access_logs`
/// (`ConfigError::AmbiguousAccessLogFilter`), NOT by serde — mirroring the
/// `SubstitutionFormatString` oneof precedent above.
#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct AccessLogFilter {
    pub status_code_filter: Option<StatusCodeFilter>,
    /// Phase 71: the SECOND `AccessLogFilter` arm — gates emission on the
    /// record's response-flag token. Mutually exclusive with
    /// `status_code_filter` (cardinality enforced by `validate_access_logs`).
    pub response_flag_filter: Option<ResponseFlagFilter>,
    /// Phase 72: the THIRD `AccessLogFilter` arm — gates emission on whether a
    /// named request header matches `header`. Mutually exclusive with the other
    /// arms (cardinality enforced by `validate_access_logs`).
    pub header_filter: Option<HeaderFilter>,
    /// Phase 73: the FOURTH `AccessLogFilter` arm — the recursive AND
    /// composition. Emit iff ALL nested child predicates match. `filters` is
    /// PGV `min_items = 2` (enforced by `validate_access_logs`). Mutually
    /// exclusive with the other arms.
    pub and_filter: Option<AndFilter>,
    /// Phase 73: the FIFTH `AccessLogFilter` arm — the recursive OR composition.
    /// Emit iff ANY nested child predicate matches. `min_items = 2`. Mutually
    /// exclusive with the other arms.
    pub or_filter: Option<OrFilter>,
    /// Phase 74: the SIXTH `AccessLogFilter` arm — gates emission on the
    /// request's DYNAMIC METADATA. Mutually exclusive with the other arms.
    pub metadata_filter: Option<MetadataFilter>,
}
```

Then add the new struct immediately after `OrFilter` (`:753-759`):

```rust
/// Phase 74: `metadata_filter` — the SIXTH `AccessLogFilter` arm. Emits a record
/// iff the request's dynamic metadata, resolved at `matcher.filter` →
/// `matcher.path[0].key`, matches `matcher.value`; when the path does NOT
/// resolve, iff `match_if_key_not_found` (MEASURED, `envoyproxy/envoy:v1.33.0`,
/// SPEC §0 R-0.3/R-0.4).
///
/// BOTH fields are `Option` for MEASURED reasons:
///   - `matcher` — upstream ACCEPTS a matcher-less `metadata_filter: {}`
///     (`configuration OK`, R-0.2); rejecting it would break LOAD PARITY. A
///     matcher-less filter keeps every record.
///   - `match_if_key_not_found` — it is a `google.protobuf.BoolValue` WRAPPER
///     (R-0.2: `{ value: true }` is accepted alongside a bare `true`), so absent
///     and explicit-`false` are DISTINCT on the wire. `None` means "default",
///     resolved to `true` at compile (R-0.4). A bare `bool` would lose that.
///
/// `MetadataMatcher` is REUSED VERBATIM from the phase-35/36 RBAC `metadata`
/// condition — upstream's `metadata_filter.matcher` is the same
/// `type.matcher.v3.MetadataMatcher` message. No `Clone` (all consumers take
/// `&AccessLogFilter`); the nested `MetadataMatcher` derives `Clone` and is what
/// the HCM compile step clones into the `MetadataMatch` trait object.
#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct MetadataFilter {
    pub matcher: Option<MetadataMatcher>,
    pub match_if_key_not_found: Option<bool>,
}
```

In `crates/envoy-config/src/lib.rs`, insert `MetadataFilter` into the sorted `pub use bootstrap::{…}` block on line 30, between `MetadataEntry` and `MetadataMatcher`:

```rust
    LocalityLbEndpoints, MetadataEntry, MetadataFilter, MetadataMatcher, MetadataPathSegment,
    Mutations,
```

(Let `cargo fmt` re-wrap the block.)

- [ ] **Step 4: Extend the destructure + `set_arms` to 6 arms (cardinality only)**

In `crates/envoy-config/src/bootstrap.rs`, `validate_access_log_filter` (`5253`), extend the destructure (still NO `..`) and the array:

```rust
    let AccessLogFilter {
        status_code_filter,
        response_flag_filter,
        header_filter,
        and_filter,
        or_filter,
        metadata_filter,
    } = filter;
    let set_arms = [
        status_code_filter.is_some(),
        response_flag_filter.is_some(),
        header_filter.is_some(),
        and_filter.is_some(),
        or_filter.is_some(),
        metadata_filter.is_some(),
    ]
    .iter()
    .filter(|set| **set)
    .count();
```

Also update the helper's doc comment (`:5246-5252`) from "all FIVE arms" to "all SIX arms".

> Leave the per-arm leaf checks alone for this task — Task 2 adds the `metadata_filter` body. The `metadata_filter` binding is USED by `set_arms` (`.is_some()`), so there is no unused-variable warning. (Task 1 accepts a `metadata_filter` with a bad matcher; that gap closes in Task 2.)

- [ ] **Step 5: Fix the 13 full `AccessLogFilter { … }` literals so the tree compiles**

Re-grep (offsets drift as Steps 3-4 land):

`rg -n "AccessLogFilter\s*\{" crates/ | grep -v "let AccessLogFilter"`

To every FULL literal — `crates/envoy-http1/src/hcm.rs` ×10 (`4535`, `4681`, `4756`, `4785`, `4800`, `4820`, `4827`, `4858`, `4997`, `5016`) and `crates/envoy-config/src/bootstrap.rs` ×3 (`13197`, `13234` [`fn exact_header`], `13293`) — add:

```rust
                metadata_filter: None,
```

The 6 `..AccessLogFilter::default()` shorthand sites (`bootstrap.rs` `13251`, `13267`, `13280`, `13306`, `13320`, `13326`) need NO change. All 13 are `#[cfg(test)]` code; the edits are purely mechanical.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p envoy-config --lib metadata_filter_deserialize_round_trip_and_defaults six_arm_cardinality_counts_every_arm 2>&1 | tail -20`
Expected: PASS (`2 passed`).
Also: `cargo build -p envoy-config -p envoy-http1 --all-targets 2>&1 | tail -5` → clean.
Also re-run the phase-70/71/72/73 arm tests: `cargo test -p envoy-config --lib access_log 2>&1 | tail -10` → all green.

- [ ] **Step 7: Commit**

```bash
git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs crates/envoy-http1/src/hcm.rs
git commit -m "phase 74 T1: MetadataFilter config struct + 6-arm cardinality destructure (folds N73-R1)"
```

---

### Task 2: Access-log-scoped matcher validation — `validate_access_log_metadata_matcher` + `ConfigError::AccessLogMetadataMatcherInvalid` + the in-place SafeRegex compile

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (`validate_access_log_filter` body; new helper beside it)
- Modify: `crates/envoy-config/src/lib.rs` (new `ConfigError` variant after `UnknownResponseFlag`, `475-479`)

**Interfaces:**
- Consumes: `MetadataFilter` (T1); the existing `ValueMatcher::compile_safe_regexes(&mut self)` (`bootstrap.rs:5539-5547`).
- Produces: `fn validate_access_log_metadata_matcher(m: &mut MetadataMatcher) -> Result<(), crate::ConfigError>`; `ConfigError::AccessLogMetadataMatcherInvalid { detail: String }`.

**Context:** The existing `validate_metadata_matcher` (`bootstrap.rs:4824-4858`) is RBAC-scoped — it takes `listener_name`/`policy_name`/`path` and yields `RbacMetadataMatcherInvalid` (a variant that CARRIES `listener`/`policy_name`, `lib.rs:664-680`), and 6 tests couple to it. PV-2 chose a NEW access-log-scoped validator over refactoring it. Unlike the RBAC path (an immutable-borrow walk that defers SafeRegex compilation to filter-lowering), `validate_access_log_filter` already holds `&mut`, so it compiles the `value`'s SafeRegex IN PLACE — exactly as the `header_filter` arm does with `validate_header_matcher(&mut hf.header)`. It also adds the empty-segment-`key` check that the RBAC validator omits (upstream PGV enforces `key` `min_len: 1`, R-0.2).

- [ ] **Step 1: Write the failing tests**

Add to `crates/envoy-config/src/bootstrap.rs` tests:

```rust
    // --- phase 74 t2: access-log-scoped MetadataMatcher validation ---

    fn md_filter(matcher: Option<MetadataMatcher>) -> AccessLogFilter {
        AccessLogFilter {
            metadata_filter: Some(MetadataFilter {
                matcher,
                match_if_key_not_found: None,
            }),
            ..AccessLogFilter::default()
        }
    }

    fn md_matcher(filter: &str, keys: &[&str]) -> MetadataMatcher {
        MetadataMatcher {
            filter: filter.into(),
            path: keys
                .iter()
                .map(|k| MetadataPathSegment { key: (*k).into() })
                .collect(),
            value: ValueMatcher::StringMatch(StringMatcher {
                mode: StringMatcherMode::Exact("1".into()),
                ignore_case: false,
            }),
        }
    }

    #[test]
    fn matcher_less_metadata_filter_is_accepted() {
        // LOAD-PARITY PIN (MEASURED R-0.2): upstream `metadata_filter: {}`
        // reports `configuration OK`. Rejecting it would break load parity.
        validate_access_logs(&mut file_log_with_filter(md_filter(None)))
            .expect("matcher-less metadata_filter must be accepted");
    }

    #[test]
    fn metadata_filter_empty_namespace_is_rejected() {
        let err = validate_access_logs(&mut file_log_with_filter(md_filter(Some(md_matcher(
            "",
            &["k"],
        )))))
        .expect_err("empty filter");
        assert!(matches!(
            err,
            crate::ConfigError::AccessLogMetadataMatcherInvalid { ref detail }
                if detail.contains("filter")
        ));
    }

    #[test]
    fn metadata_filter_empty_path_is_rejected() {
        let err =
            validate_access_logs(&mut file_log_with_filter(md_filter(Some(md_matcher(
                "com.example",
                &[],
            )))))
            .expect_err("empty path");
        assert!(matches!(
            err,
            crate::ConfigError::AccessLogMetadataMatcherInvalid { ref detail }
                if detail.contains("segment")
        ));
    }

    #[test]
    fn metadata_filter_multi_segment_path_is_rejected() {
        // CF-74-2: upstream ACCEPTS a multi-segment path (R-0.2); envoy-rust's
        // FLAT string-only metadata store cannot represent one, so it is
        // fail-loud (stricter, never silently different — ADR-0049).
        let err = validate_access_logs(&mut file_log_with_filter(md_filter(Some(md_matcher(
            "com.example",
            &["a", "b"],
        )))))
        .expect_err("multi-segment path");
        assert!(matches!(
            err,
            crate::ConfigError::AccessLogMetadataMatcherInvalid { ref detail }
                if detail.contains("segment")
        ));
    }

    #[test]
    fn metadata_filter_empty_segment_key_is_rejected() {
        // Upstream PGV enforces `key` min_len 1 (R-0.2). NB the RBAC-scoped
        // `validate_metadata_matcher` does NOT check this (CF-74-4).
        let err = validate_access_logs(&mut file_log_with_filter(md_filter(Some(md_matcher(
            "com.example",
            &[""],
        )))))
        .expect_err("empty segment key");
        assert!(matches!(
            err,
            crate::ConfigError::AccessLogMetadataMatcherInvalid { ref detail }
                if detail.contains("key")
        ));
    }

    #[test]
    fn metadata_filter_safe_regex_compiles_in_place_and_rejects_bad_pattern() {
        // The access-log path takes `&mut`, so — unlike the RBAC path — it can
        // compile the value's SafeRegex IN PLACE, so the runtime `matches` never
        // hits its `.expect()`.
        let ok = MetadataMatcher {
            filter: "com.example".into(),
            path: vec![MetadataPathSegment { key: "k".into() }],
            value: ValueMatcher::StringMatch(StringMatcher {
                mode: StringMatcherMode::SafeRegex(SafeRegex {
                    regex: "^[0-9]+$".into(),
                    compiled: None,
                }),
                ignore_case: false,
            }),
        };
        let mut logs = file_log_with_filter(md_filter(Some(ok)));
        validate_access_logs(&mut logs).expect("valid regex compiles");
        let compiled_in_place = match &logs[0]
            .filter
            .as_ref()
            .unwrap()
            .metadata_filter
            .as_ref()
            .unwrap()
            .matcher
            .as_ref()
            .unwrap()
            .value
        {
            ValueMatcher::StringMatch(sm) => match &sm.mode {
                StringMatcherMode::SafeRegex(sr) => sr.compiled.is_some(),
                _ => false,
            },
            _ => false,
        };
        assert!(compiled_in_place, "SafeRegex must be compiled in place");

        let bad = MetadataMatcher {
            filter: "com.example".into(),
            path: vec![MetadataPathSegment { key: "k".into() }],
            value: ValueMatcher::StringMatch(StringMatcher {
                mode: StringMatcherMode::SafeRegex(SafeRegex {
                    regex: "([".into(),
                    compiled: None,
                }),
                ignore_case: false,
            }),
        };
        let err = validate_access_logs(&mut file_log_with_filter(md_filter(Some(bad))))
            .expect_err("bad regex");
        assert!(matches!(err, crate::ConfigError::InvalidRegex { .. }));
    }

    #[test]
    fn metadata_filter_nested_in_or_filter_surfaces_through_recursion() {
        // The phase-73 recursion must reach the new arm: a bad metadata matcher
        // nested inside an `or_filter` child still fails-loud.
        let f = AccessLogFilter {
            or_filter: Some(OrFilter {
                filters: vec![
                    exact_header("x-a", "1"),
                    md_filter(Some(md_matcher("", &["k"]))),
                ],
            }),
            ..AccessLogFilter::default()
        };
        let err = validate_access_logs(&mut file_log_with_filter(f)).expect_err("nested bad");
        assert!(matches!(
            err,
            crate::ConfigError::AccessLogMetadataMatcherInvalid { .. }
        ));
    }
```

> If `SafeRegex` / `MetadataPathSegment` / `MetadataMatcher` are not in the test module's scope, add them to `use super::*;`. Check `SafeRegex`'s exact field set with `rg -n -A6 "pub struct SafeRegex" crates/envoy-config/src/bootstrap.rs` before writing the literal — it is `{ regex: String, compiled: Option<Arc<regex::Regex>> }` at the time of writing, but confirm.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p envoy-config --lib metadata_filter_ matcher_less_metadata_filter_is_accepted 2>&1 | tail -30`
Expected: FAIL — first a compile error (`no variant named 'AccessLogMetadataMatcherInvalid'`), then, once the variant exists, every negative test fails because T1's validator performs NO matcher checks (`matcher_less_metadata_filter_is_accepted` passes from the start — it is the load-parity pin, and it must NEVER go red).

- [ ] **Step 3: Add the `ConfigError` variant**

In `crates/envoy-config/src/lib.rs`, immediately after `UnknownResponseFlag { token: String }` (`475-479`) and before the `Http2ClusterFromHttp1Listener` doc comment (`481`):

```rust
    /// Phase 74: an access-log `metadata_filter.matcher` (`MetadataMatcher`) is
    /// malformed — an empty `filter` namespace (upstream PGV `min_len 1`), a
    /// `path` whose length is not exactly 1 (upstream accepts multi-segment;
    /// envoy-rust's FLAT string-only metadata store cannot resolve one →
    /// stricter boot-fatal, CF-74-2), or an empty path-segment `key` (upstream
    /// PGV `min_len 1`). Distinct from `RbacMetadataMatcherInvalid`, which is
    /// RBAC-scoped and carries `listener`/`policy_name` — the access-log
    /// validator has neither in scope. Config-load-time fatal (ADR-0049).
    #[error("access_log metadata_filter matcher is invalid: {detail}")]
    AccessLogMetadataMatcherInvalid { detail: String },
```

- [ ] **Step 4: Add the validator and wire it into `validate_access_log_filter`**

In `crates/envoy-config/src/bootstrap.rs`, add the `metadata_filter` body to `validate_access_log_filter`, after the `or_filter` block and before the final `Ok(())`:

```rust
    // Phase 74: the `metadata_filter` arm. `matcher` is OPTIONAL upstream
    // (MEASURED R-0.2 — `metadata_filter: {}` validates), so a matcher-less
    // filter passes; when present it is validated fail-loud and its value's
    // SafeRegex is compiled IN PLACE (this path holds `&mut`, unlike the RBAC
    // one).
    if let Some(mf) = metadata_filter
        && let Some(mm) = mf.matcher.as_mut()
    {
        validate_access_log_metadata_matcher(mm)?;
    }
    Ok(())
```

Add the helper immediately below `validate_access_log_filter`:

```rust
/// Phase 74: validate an ACCESS-LOG `metadata_filter.matcher`. Mirrors upstream's
/// PGV bounds (MEASURED, SPEC §0 R-0.2): `filter` `min_len 1`, `path`
/// `min_items 1`, each segment `key` `min_len 1`; plus envoy-rust's inherited
/// single-segment restriction (CF-74-2 — the record's metadata store is a FLAT
/// `BTreeMap<String, BTreeMap<String, String>>`, in which a nested path is
/// unrepresentable). Also compiles the value's SafeRegex IN PLACE so the runtime
/// `ValueMatcher::matches` never hits its `.expect()`.
///
/// Deliberately NOT the RBAC `validate_metadata_matcher`: that one is
/// RBAC-scoped (it takes `listener`/`policy_name` and yields
/// `RbacMetadataMatcherInvalid`, whose variant carries both), it is called from
/// an immutable-borrow walk so it cannot compile in place, and it does not check
/// the segment `key` (CF-74-4).
fn validate_access_log_metadata_matcher(
    m: &mut MetadataMatcher,
) -> Result<(), crate::ConfigError> {
    let bad = |detail: String| crate::ConfigError::AccessLogMetadataMatcherInvalid { detail };
    if m.filter.is_empty() {
        return Err(bad("metadata matcher `filter` must not be empty".into()));
    }
    if m.path.len() != 1 {
        return Err(bad(format!(
            "metadata matcher path must have exactly one segment (got {}); multi-segment/nested paths are deferred (CF-74-2)",
            m.path.len()
        )));
    }
    if m.path[0].key.is_empty() {
        return Err(bad(
            "metadata matcher path segment `key` must not be empty".into(),
        ));
    }
    m.value.compile_safe_regexes()?;
    Ok(())
}
```

> Update `validate_access_logs`'s doc comment (`bootstrap.rs:~5154-5191`) with a new item 7: "Phase 74 — the `metadata_filter.matcher`, when present, validates via `validate_access_log_metadata_matcher` (empty namespace / non-single-segment path / empty segment key → `AccessLogMetadataMatcherInvalid`; bad regex → `InvalidRegex`) and its SafeRegex compiles in place. A matcher-less `metadata_filter` is ACCEPTED (upstream load parity, MEASURED)." Do NOT delete the M70-R1 no-`..` rationale.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p envoy-config --lib metadata_filter matcher_less 2>&1 | tail -20`
Expected: PASS (`8 passed` — T1's two plus T2's six... count precisely from the output; every named test green).
Also re-run the phase-73 recursion tests: `cargo test -p envoy-config --lib nested_ and_filter or_filter 2>&1 | tail -10` → green.

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs
git commit -m "phase 74 T2: access-log-scoped metadata matcher validation + AccessLogMetadataMatcherInvalid"
```

---

### Task 3: Widen `should_log` with the dynamic-metadata store (pure mechanical refactor — no behavior change)

**Files:**
- Modify: `crates/envoy-accesslog/src/filter.rs` (`should_log` `71-112`, incl. the two recursive calls at `105-110`; ~40 test call sites)
- Modify: `crates/envoy-accesslog/src/file_sink.rs` (wrapper `99-112`; test call sites)
- Modify: `crates/envoy-http1/src/hcm.rs` (emit gate `1515`; ~47 test call sites)
- Modify: `crates/envoy-http2/src/hcm.rs` (emit gate `1138`)

**Interfaces:**
- Produces: `LogFilter::should_log(&self, status: u16, response_flags: &str, headers: &[(String, String)], dynamic_metadata: &BTreeMap<String, BTreeMap<String, String>>) -> bool` and the identically-widened `FileSink::should_log`.

**Context:** MEASURED census (PV-5): 102 code occurrences of `should_log` — `envoy-http1/src/hcm.rs` 49, `envoy-accesslog/src/filter.rs` 42, `envoy-accesslog/src/file_sink.rs` 7, `envoy-http2/src/hcm.rs` 4 (of which only 1 is a call, 3 are doc comments). Both emit gates already hold the fully-built `record` BEFORE the per-sink loop, so `&record.dynamic_metadata` is in scope at each. **This task adds NO new arm and changes NO verdict** — every existing arm ignores the new argument. The RED is a compile-arity error plus a behavioral pin that non-empty metadata does not perturb the existing arms.

- [ ] **Step 1: Write the failing test**

Add to `crates/envoy-accesslog/src/filter.rs`'s `#[cfg(test)] mod tests` (which already defines `ge`/`eq`/`le`/`rf` helpers and the `HasHeaderValue` stub):

```rust
    /// Phase 74 T3: `should_log` carries the per-request dynamic-metadata store
    /// as a 4th argument. Every PRE-74 arm ignores it — this pins that the
    /// widening is behavior-neutral.
    #[test]
    fn existing_arms_ignore_the_dynamic_metadata_argument() {
        use std::collections::BTreeMap;
        let mut md: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        md.entry("com.example".into())
            .or_default()
            .insert("k".into(), "1".into());
        let empty: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();

        // StatusCode arm: identical verdict with and without metadata.
        assert!(ge(500).should_log(503, "-", &[], &md));
        assert!(ge(500).should_log(503, "-", &[], &empty));
        assert!(!ge(500).should_log(499, "-", &[], &md));

        // ResponseFlag arm.
        assert!(rf(&["NR"]).should_log(404, "NR", &[], &md));
        assert!(!rf(&["UH"]).should_log(404, "NR", &[], &md));

        // Header arm (via the local stub).
        let h = LogFilter::Header {
            matcher: std::sync::Arc::new(HasHeaderValue("x-log", "yes")),
        };
        assert!(h.should_log(200, "-", &[("x-log".to_string(), "yes".to_string())], &md));
        assert!(!h.should_log(200, "-", &[], &md));

        // Composition arms thread the new argument through the recursion.
        let and = LogFilter::And(vec![ge(200), le(299)]);
        assert!(and.should_log(204, "-", &[], &md));
        assert!(!and.should_log(500, "-", &[], &md));
        let or = LogFilter::Or(vec![le(199), ge(500)]);
        assert!(or.should_log(503, "-", &[], &md));
        assert!(!or.should_log(200, "-", &[], &md));
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p envoy-accesslog --lib existing_arms_ignore_the_dynamic_metadata_argument 2>&1 | tail -20`
Expected: FAIL — `error[E0061]: this method takes 3 arguments but 4 arguments were supplied`.

- [ ] **Step 3: Widen the two signatures and both emit gates**

In `crates/envoy-accesslog/src/filter.rs`, add the import beside `use std::sync::Arc;` (line 6):

```rust
use std::collections::BTreeMap;
```

and widen `should_log` (`71-112`):

```rust
    /// Returns `true` iff a record with the given final response `status`,
    /// `response_flags` token, request `headers`, and per-request
    /// `dynamic_metadata` should be emitted. The `StatusCode` arm reads only
    /// `status`; the `ResponseFlag` arm only `response_flags`; the `Header` arm
    /// only `headers`; the phase-74 `Metadata` arm only `dynamic_metadata`. The
    /// status comparison is widened to `u32` (lossless; status is always in
    /// `u16` range).
    pub fn should_log(
        &self,
        status: u16,
        response_flags: &str,
        headers: &[(String, String)],
        dynamic_metadata: &BTreeMap<String, BTreeMap<String, String>>,
    ) -> bool {
```

and thread it through the two recursive arms:

```rust
            LogFilter::And(filters) => filters
                .iter()
                .all(|f| f.should_log(status, response_flags, headers, dynamic_metadata)),
            LogFilter::Or(filters) => filters
                .iter()
                .any(|f| f.should_log(status, response_flags, headers, dynamic_metadata)),
```

In `crates/envoy-accesslog/src/file_sink.rs` (`99-112`):

```rust
    /// Phase 70/71/72/73/74: returns `true` iff a record with final response
    /// `status`, `response_flags` token, request `headers`, and per-request
    /// `dynamic_metadata` should be emitted to this sink. A sink with no filter
    /// always logs.
    pub fn should_log(
        &self,
        status: u16,
        response_flags: &str,
        headers: &[(String, String)],
        dynamic_metadata: &std::collections::BTreeMap<
            String,
            std::collections::BTreeMap<String, String>,
        >,
    ) -> bool {
        match &self.filter {
            Some(f) => f.should_log(status, response_flags, headers, dynamic_metadata),
            None => true,
        }
    }
```

In `crates/envoy-http1/src/hcm.rs` (`1515`):

```rust
                // Phase 74: thread the record's dynamic-metadata store for the
                // `metadata_filter` arm (already built above — the record is
                // constructed BEFORE this loop). The other arms ignore it.
                if !sink.should_log(
                    record.response_code,
                    &record.response_flags,
                    &req.headers,
                    &record.dynamic_metadata,
                ) {
                    continue;
                }
```

In `crates/envoy-http2/src/hcm.rs` (`1138`):

```rust
            if !sink.should_log(
                record.response_code,
                &record.response_flags,
                &envoy_req.headers,
                &record.dynamic_metadata,
            ) {
                continue;
            }
```

- [ ] **Step 4: Update every remaining call site (compiler-driven)**

Run `cargo build --workspace --all-targets 2>&1 | grep -E "^error\[E0061\]" -A3 | head -60` and fix each reported site by appending a 4th argument. The mechanical edit for the ~98 in-process test sites is `&Default::default()` (the parameter type is known, so inference is unambiguous):

```rust
        assert!(!ge(500).should_log(499, "-", &[], &Default::default()));
```

Where a test module already carries local helpers, a `fn no_md() -> BTreeMap<String, BTreeMap<String, String>> { BTreeMap::new() }` helper reads better; either is acceptable. Iterate until `cargo build --workspace --all-targets` is clean.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p envoy-accesslog --lib 2>&1 | tail -10` → all green (incl. the new test).
Run: `cargo test -p envoy-http1 --lib access_log 2>&1 | tail -10` → green.
Run: `cargo test -p envoy-http2 --lib 2>&1 | tail -10` → green.
Run: `cargo clippy -p envoy-accesslog -p envoy-http1 -p envoy-http2 --all-targets --all-features -- -D warnings 2>&1 | tail -10` → clean.

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-accesslog/src/filter.rs crates/envoy-accesslog/src/file_sink.rs crates/envoy-http1/src/hcm.rs crates/envoy-http2/src/hcm.rs
git commit -m "phase 74 T3: widen should_log with the dynamic-metadata store (behavior-neutral)"
```

---

### Task 4: The `MetadataMatch` trait seam + `LogFilter::Metadata` + its `should_log` arm

**Files:**
- Modify: `crates/envoy-accesslog/src/filter.rs` (trait beside `HeaderMatch` `24-35`; enum `37-63`; `should_log` match)
- Modify: `crates/envoy-accesslog/src/lib.rs` (re-export line `31`)

**Interfaces:**
- Produces: `pub trait MetadataMatch: std::fmt::Debug + Send + Sync { fn matches(&self, dynamic_metadata: &BTreeMap<String, BTreeMap<String, String>>) -> Option<bool>; }`; `LogFilter::Metadata { matcher: Option<Arc<dyn MetadataMatch>>, match_if_key_not_found: bool }`; `envoy_accesslog::MetadataMatch`.
- Consumes: the T3 4th parameter.

**Context:** `envoy-accesslog` must NOT depend on `envoy-config` (ADR-0150 cycle), so the resolution logic — which needs `MetadataMatcher`'s `filter`/`path` fields — lives in the impl on the `envoy-config` side (T5). The `match_if_key_not_found` policy lives on the FILTER, not the matcher, so the trait returns **`Option<bool>`** (`None` = the path did not resolve) and `LogFilter` applies the default. This expresses the MEASURED R-0.4 rule exactly once, in the crate that owns the field. `LogFilter` derives ONLY `Debug, Clone` — do not add `Eq`/`PartialEq`. The crate cannot construct a real `MetadataMatcher`, so the test uses a local stub, exactly as `HasHeaderValue` does for `HeaderMatch` (`filter.rs:~243-250`).

- [ ] **Step 1: Write the failing test**

Add to `crates/envoy-accesslog/src/filter.rs`'s test module:

```rust
    // --- phase 74: LogFilter::Metadata + the injected MetadataMatch seam ---

    /// A local `MetadataMatch` stub. The accesslog crate cannot build a real
    /// `envoy_config::MetadataMatcher` (it must not depend on `envoy-config` —
    /// ADR-0150 cycle), so this proves the `should_log` PLUMBING and the
    /// `Option<bool>` contract: `None` iff the path did not resolve, so
    /// `LogFilter` applies `match_if_key_not_found`. The real resolution +
    /// value-matcher coverage lives in `envoy-config` (T5) and `envoy-http1`
    /// (T6) over the actual engine.
    #[derive(Debug)]
    struct NsKeyEquals(&'static str, &'static str, &'static str);
    impl MetadataMatch for NsKeyEquals {
        fn matches(
            &self,
            dynamic_metadata: &BTreeMap<String, BTreeMap<String, String>>,
        ) -> Option<bool> {
            let v = dynamic_metadata.get(self.0)?.get(self.1)?;
            Some(v == self.2)
        }
    }

    fn md(pairs: &[(&str, &str, &str)]) -> BTreeMap<String, BTreeMap<String, String>> {
        let mut m: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        for (ns, k, v) in pairs {
            m.entry((*ns).to_string())
                .or_default()
                .insert((*k).to_string(), (*v).to_string());
        }
        m
    }

    #[test]
    fn metadata_arm_implements_the_measured_decision_rule() {
        // MEASURED (SPEC §0 R-0.3/R-0.4, `envoyproxy/envoy:v1.33.0`):
        //   resolved = dynamic_metadata[filter][path[0].key]
        //   None    => match_if_key_not_found     (DEFAULT true)
        //   Some(v) => value.matches(v)
        let keep_default = LogFilter::Metadata {
            matcher: Some(std::sync::Arc::new(NsKeyEquals("com.example", "k", "1"))),
            match_if_key_not_found: true,
        };
        let drop_default = LogFilter::Metadata {
            matcher: Some(std::sync::Arc::new(NsKeyEquals("com.example", "k", "1"))),
            match_if_key_not_found: false,
        };

        // Value MATCHES → KEEP, regardless of the not-found policy.
        let hit = md(&[("com.example", "k", "1")]);
        assert!(keep_default.should_log(200, "-", &[], &hit));
        assert!(drop_default.should_log(200, "-", &[], &hit));

        // Value MISMATCH → DROP, regardless of the not-found policy (the value
        // matcher is only consulted when the path RESOLVES).
        let miss = md(&[("com.example", "k", "2")]);
        assert!(!keep_default.should_log(200, "-", &[], &miss));
        assert!(!drop_default.should_log(200, "-", &[], &miss));

        // KEY absent inside a PRESENT namespace → the not-found policy decides.
        let other_key = md(&[("com.example", "other", "1")]);
        assert!(keep_default.should_log(200, "-", &[], &other_key));
        assert!(!drop_default.should_log(200, "-", &[], &other_key));

        // NAMESPACE absent behaves IDENTICALLY to a missing key (MEASURED R-0.4).
        let other_ns = md(&[("com.other", "k", "1")]);
        assert!(keep_default.should_log(200, "-", &[], &other_ns));
        assert!(!drop_default.should_log(200, "-", &[], &other_ns));

        // Wholly empty store → same not-found path.
        let empty = md(&[]);
        assert!(keep_default.should_log(200, "-", &[], &empty));
        assert!(!drop_default.should_log(200, "-", &[], &empty));

        // MATCHER-LESS filter (upstream accepts `metadata_filter: {}`, R-0.2):
        // every record takes the not-found policy.
        let no_matcher_keep = LogFilter::Metadata {
            matcher: None,
            match_if_key_not_found: true,
        };
        let no_matcher_drop = LogFilter::Metadata {
            matcher: None,
            match_if_key_not_found: false,
        };
        assert!(no_matcher_keep.should_log(200, "-", &[], &hit));
        assert!(!no_matcher_drop.should_log(200, "-", &[], &hit));

        // The arm ignores status / response_flags / headers.
        assert!(keep_default.should_log(503, "UF", &[("x-a".into(), "1".into())], &hit));
    }

    #[test]
    fn metadata_arm_composes_under_and_or() {
        // The phase-73 composition arms thread the store through the recursion.
        let meta = LogFilter::Metadata {
            matcher: Some(std::sync::Arc::new(NsKeyEquals("com.example", "k", "1"))),
            match_if_key_not_found: false,
        };
        let and = LogFilter::And(vec![meta.clone(), ge(500)]);
        let hit = md(&[("com.example", "k", "1")]);
        assert!(and.should_log(503, "-", &[], &hit)); // both true
        assert!(!and.should_log(200, "-", &[], &hit)); // status false
        assert!(!and.should_log(503, "-", &[], &md(&[]))); // metadata false

        let or = LogFilter::Or(vec![meta, ge(500)]);
        assert!(or.should_log(200, "-", &[], &hit)); // metadata true
        assert!(or.should_log(503, "-", &[], &md(&[]))); // status true
        assert!(!or.should_log(200, "-", &[], &md(&[]))); // neither
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p envoy-accesslog --lib metadata_arm 2>&1 | tail -20`
Expected: FAIL — `error[E0405]: cannot find trait 'MetadataMatch' in this scope` and `no variant named 'Metadata' found for enum 'LogFilter'`.

- [ ] **Step 3: Add the trait, the variant, and the `should_log` arm**

In `crates/envoy-accesslog/src/filter.rs`, add the trait immediately after `HeaderMatch` (`32-35`):

```rust
/// Phase 74 (ADR-0150/ADR-0155): the runtime seam for the `metadata_filter` arm.
/// Same cycle constraint as `HeaderMatch` — this crate CANNOT depend on
/// `envoy-config`, so the resolution engine is injected as a trait object:
/// `envoy-config` impls `MetadataMatch` for its `MetadataMatcher` (resolving
/// `filter` → `path[0].key` and delegating to `ValueMatcher::matches` VERBATIM),
/// and the HCM compile step in `envoy-http1` boxes it into
/// `LogFilter::Metadata`. `Send + Sync` because sinks cross async await points.
///
/// **Returns `Option<bool>`, NOT `bool`.** `None` iff the metadata path did NOT
/// resolve, so the `match_if_key_not_found` policy — which lives on the FILTER,
/// not the matcher — stays in `LogFilter`, expressing the MEASURED rule (SPEC §0
/// R-0.4) exactly once. Collapsing `None` into `false` (as
/// `ValueMatcher::matches_resolved` does for the RBAC path) would DROP every
/// key-absent record, the opposite of the measured upstream default (`true`).
pub trait MetadataMatch: std::fmt::Debug + Send + Sync {
    /// `None` iff the configured metadata path did not resolve; otherwise
    /// `Some(value_matcher_verdict)`.
    fn matches(
        &self,
        dynamic_metadata: &BTreeMap<String, BTreeMap<String, String>>,
    ) -> Option<bool>;
}
```

Add the variant to `LogFilter` (after `Or(Vec<LogFilter>)`):

```rust
    /// Phase 74: emit a record iff its DYNAMIC METADATA satisfies the matcher —
    /// or, when the metadata path does not resolve, iff
    /// `match_if_key_not_found`. `matcher` is `Option` because upstream ACCEPTS
    /// a matcher-less `metadata_filter: {}` (MEASURED R-0.2), in which case
    /// every record takes the not-found policy. `match_if_key_not_found` is
    /// already resolved to a concrete `bool` by the compile step
    /// (`Option<bool>::unwrap_or(true)` — the MEASURED wrapper default, R-0.4).
    /// Introduces no `Eq`/`PartialEq` and no `envoy-config` dep (ADR-0150 holds).
    Metadata {
        matcher: Option<Arc<dyn MetadataMatch>>,
        match_if_key_not_found: bool,
    },
```

Add the `should_log` arm (after the `Header` arm, before the composition arms):

```rust
            // Phase 74: the MEASURED decision rule (SPEC §0 R-0.3/R-0.4) —
            // resolve `dynamic_metadata[filter][path[0].key]`; unresolved (or no
            // matcher at all) → `match_if_key_not_found`; resolved →
            // `value.matches(v)`. A missing NAMESPACE behaves identically to a
            // missing KEY (the trait impl returns `None` for both).
            LogFilter::Metadata {
                matcher,
                match_if_key_not_found,
            } => match matcher {
                None => *match_if_key_not_found,
                Some(m) => m
                    .matches(dynamic_metadata)
                    .unwrap_or(*match_if_key_not_found),
            },
```

In `crates/envoy-accesslog/src/lib.rs`, extend the re-export (line `31`):

```rust
pub use filter::{FilterOp, HeaderMatch, LogFilter, MetadataMatch, StatusCodeComparison};
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p envoy-accesslog --lib metadata_arm 2>&1 | tail -20`
Expected: PASS (`2 passed`).
Run: `cargo test -p envoy-accesslog --lib 2>&1 | tail -5` → all green.
Run: `cargo clippy -p envoy-accesslog --all-targets --all-features -- -D warnings 2>&1 | tail -5` → clean.

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-accesslog/src/filter.rs crates/envoy-accesslog/src/lib.rs
git commit -m "phase 74 T4: MetadataMatch trait seam + LogFilter::Metadata + should_log arm"
```

---

### Task 5: The sole `impl MetadataMatch for MetadataMatcher` in `envoy-config`

**Files:**
- Modify: `crates/envoy-config/src/matcher.rs` (append after the `impl envoy_accesslog::HeaderMatch for HeaderMatcher` block, `55-68`)

**Interfaces:**
- Consumes: `envoy_accesslog::MetadataMatch` (T4); the existing `ValueMatcher::matches(&self, value: &str) -> bool` (`matcher.rs:125-130`).
- Produces: `impl envoy_accesslog::MetadataMatch for MetadataMatcher`.

**Context:** `crates/envoy-config/src/matcher.rs` currently imports `use crate::bootstrap::{HeaderMatcher, HeaderMatcherMode, StringMatcher, StringMatcherMode, ValueMatcher};` (`:9-11`) and hosts the phase-72 `HeaderMatch` impl (`:55-68`). There is **no inherent `impl MetadataMatcher`** in the tree, so the trait method needs no delegation trick. `ValueMatcher::matches` is reused VERBATIM (it is `pub`, takes a bare `&str`, and is store-agnostic). `matches_resolved` is deliberately NOT used — it collapses "unresolved" into `false`, which the `Option<bool>` contract must keep distinct (PV-4).

- [ ] **Step 1: Write the failing test**

Add a `#[cfg(test)] mod` at the end of `crates/envoy-config/src/matcher.rs` (or extend the existing test module there if present — check with `rg -n "mod tests" crates/envoy-config/src/matcher.rs`):

```rust
#[cfg(test)]
mod metadata_match_tests {
    use crate::bootstrap::{
        MetadataMatcher, MetadataPathSegment, StringMatcher, StringMatcherMode, ValueMatcher,
    };
    use envoy_accesslog::MetadataMatch;
    use std::collections::BTreeMap;

    fn store(pairs: &[(&str, &str, &str)]) -> BTreeMap<String, BTreeMap<String, String>> {
        let mut m: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        for (ns, k, v) in pairs {
            m.entry((*ns).to_string())
                .or_default()
                .insert((*k).to_string(), (*v).to_string());
        }
        m
    }

    fn matcher(value: ValueMatcher) -> MetadataMatcher {
        MetadataMatcher {
            filter: "com.example".into(),
            path: vec![MetadataPathSegment { key: "k".into() }],
            value,
        }
    }

    fn exact(v: &str) -> ValueMatcher {
        ValueMatcher::StringMatch(StringMatcher {
            mode: StringMatcherMode::Exact(v.into()),
            ignore_case: false,
        })
    }

    #[test]
    fn resolution_contract_is_option_bool() {
        let m = matcher(exact("1"));
        // Resolved + matches → Some(true).
        assert_eq!(m.matches(&store(&[("com.example", "k", "1")])), Some(true));
        // Resolved + mismatch → Some(false) — NOT None. The caller must be able
        // to distinguish "value said no" from "path did not resolve", because
        // only the latter falls back to `match_if_key_not_found` (R-0.4).
        assert_eq!(m.matches(&store(&[("com.example", "k", "2")])), Some(false));
        // Missing KEY → None.
        assert_eq!(m.matches(&store(&[("com.example", "other", "1")])), None);
        // Missing NAMESPACE → None (MEASURED R-0.4: identical to a missing key).
        assert_eq!(m.matches(&store(&[("com.other", "k", "1")])), None);
        // Empty store → None.
        assert_eq!(m.matches(&store(&[])), None);
    }

    #[test]
    fn reuses_the_value_matcher_engine_verbatim() {
        // Every modelled StringMatcher mode routes through ValueMatcher::matches.
        let md = store(&[("com.example", "k", "prod-1")]);
        let case = |mode: StringMatcherMode, ignore_case: bool| {
            matcher(ValueMatcher::StringMatch(StringMatcher { mode, ignore_case })).matches(&md)
        };
        assert_eq!(case(StringMatcherMode::Exact("prod-1".into()), false), Some(true));
        assert_eq!(case(StringMatcherMode::Prefix("prod".into()), false), Some(true));
        assert_eq!(case(StringMatcherMode::Suffix("-1".into()), false), Some(true));
        assert_eq!(case(StringMatcherMode::Contains("od-".into()), false), Some(true));
        assert_eq!(case(StringMatcherMode::Exact("PROD-1".into()), true), Some(true));
        assert_eq!(case(StringMatcherMode::Exact("PROD-1".into()), false), Some(false));

        // present_match (phase-36 §A1 semantics `match = present && want`): the
        // path RESOLVED here, so it reduces to `want`. An ABSENT key returns
        // None and takes `match_if_key_not_found` instead (CF-74-5: the
        // present_match composition is pinned in-process, not live-probed).
        assert_eq!(matcher(ValueMatcher::PresentMatch(true)).matches(&md), Some(true));
        assert_eq!(matcher(ValueMatcher::PresentMatch(false)).matches(&md), Some(false));
        assert_eq!(
            matcher(ValueMatcher::PresentMatch(true)).matches(&store(&[])),
            None
        );
    }

    #[test]
    fn empty_path_resolves_to_none_rather_than_panicking() {
        // The validator guarantees `path.len() == 1` for every matcher that can
        // reach this impl (T2), so this is unreachable in a booted proxy — but
        // the impl uses `path.first()?` so a mis-wired caller degrades to
        // "unresolved" rather than panicking.
        let m = MetadataMatcher {
            filter: "com.example".into(),
            path: vec![],
            value: exact("1"),
        };
        assert_eq!(m.matches(&store(&[("com.example", "k", "1")])), None);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p envoy-config --lib metadata_match_tests 2>&1 | tail -20`
Expected: FAIL — `error[E0599]: no method named 'matches' found for struct 'MetadataMatcher'` (the trait is in scope but unimplemented).

- [ ] **Step 3: Add the impl**

Append to `crates/envoy-config/src/matcher.rs`, after the `HeaderMatch` impl:

```rust
/// Phase 74 (ADR-0150/ADR-0155): the sole `MetadataMatch` impl — the access-log
/// `metadata_filter` resolution engine. `envoy-accesslog` cannot see
/// `MetadataMatcher`'s `filter`/`path` fields (it must not depend on
/// `envoy-config` — cycle), so resolution happens HERE and only the verdict
/// crosses the seam.
///
/// The MEASURED rule (SPEC §0 R-0.3/R-0.4, `envoyproxy/envoy:v1.33.0`):
/// `resolved = dynamic_metadata[filter][path[0].key]`; unresolved → `None` (the
/// caller applies `match_if_key_not_found`, whose measured default is `true`);
/// resolved → `Some(value.matches(v))`, reusing the phase-35/36
/// `ValueMatcher::matches` engine VERBATIM.
///
/// NB `ValueMatcher::matches_resolved` — the RBAC-path sibling — is deliberately
/// NOT used: it maps an unresolved path to `false`, which would drop every
/// key-absent record instead of deferring to `match_if_key_not_found`.
///
/// `path.first()?` (rather than `path[0]`) keeps this total: the T2 validator
/// guarantees `path.len() == 1` for every matcher that can reach here, so an
/// empty path is unreachable in a booted proxy, and degrading to "unresolved"
/// beats a panic.
impl envoy_accesslog::MetadataMatch for MetadataMatcher {
    fn matches(
        &self,
        dynamic_metadata: &std::collections::BTreeMap<
            String,
            std::collections::BTreeMap<String, String>,
        >,
    ) -> Option<bool> {
        let key = &self.path.first()?.key;
        let resolved = dynamic_metadata.get(&self.filter)?.get(key)?;
        Some(self.value.matches(resolved))
    }
}
```

Add `MetadataMatcher` (and `MetadataPathSegment` if the tests need it at module scope) to the file's `use crate::bootstrap::{…}` import list (`:9-11`).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p envoy-config --lib metadata_match_tests 2>&1 | tail -20`
Expected: PASS (`3 passed`).
Run: `cargo clippy -p envoy-config --all-targets --all-features -- -D warnings 2>&1 | tail -5` → clean.

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-config/src/matcher.rs
git commit -m "phase 74 T5: impl MetadataMatch for MetadataMatcher (reuses ValueMatcher::matches verbatim)"
```

---

### Task 6: The 6-tuple `compile_access_log_filter` + the `unwrap_or(true)` wrapper default

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` (`compile_access_log_filter` `1739-1786`)

**Interfaces:**
- Consumes: `MetadataFilter` (T1), `LogFilter::Metadata` + `MetadataMatch` (T4), the `envoy-config` impl (T5).
- Produces: a 6-arm `compile_access_log_filter(&AccessLogFilter) -> LogFilter`.

**Context:** `compile_access_log_filter` (`hcm.rs:1739-1786`) matches a 5-tuple with `_ => unreachable!("validated by validate_access_logs: exactly one filter arm is set")`. T1 added the field without breaking that match — so a `metadata_filter` config currently PANICS at the `_` arm. This task closes that window. The single production call site (`hcm.rs:208`, `entry.filter.as_ref().map(compile_access_log_filter)`) is unchanged; `envoy-http2` reuses the same compiled sinks via `HCMConfig { inner: Arc<Http1HCMConfig> }`, so there is no second compile site.

- [ ] **Step 1: Write the failing test**

Add to `crates/envoy-http1/src/hcm.rs` tests, next to `compile_access_log_filter_builds_composition_arms_recursively` (~`4785`):

```rust
    /// Phase 74 T6: `compile_access_log_filter` builds the `metadata_filter`
    /// arm — boxing the config `MetadataMatcher` into the injected
    /// `MetadataMatch` seam and resolving the BoolValue-wrapper default
    /// (`match_if_key_not_found: None` → `true`, MEASURED SPEC §0 R-0.4).
    #[test]
    fn compile_access_log_filter_builds_metadata_arm_with_wrapper_default() {
        use std::collections::BTreeMap;

        let md = |ns: &str, k: &str, v: &str| {
            let mut m: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
            m.entry(ns.to_string())
                .or_default()
                .insert(k.to_string(), v.to_string());
            m
        };
        let empty: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();

        let matcher = envoy_config::MetadataMatcher {
            filter: "com.example".into(),
            path: vec![envoy_config::MetadataPathSegment { key: "k".into() }],
            value: envoy_config::ValueMatcher::StringMatch(envoy_config::StringMatcher {
                mode: envoy_config::StringMatcherMode::Exact("1".into()),
                ignore_case: false,
            }),
        };

        // (a) `match_if_key_not_found` ABSENT → compiled to `true` (the MEASURED
        //     BoolValue-wrapper default). Key-absent records are KEPT.
        let default_cfg = envoy_config::AccessLogFilter {
            metadata_filter: Some(envoy_config::MetadataFilter {
                matcher: Some(matcher.clone()),
                match_if_key_not_found: None,
            }),
            ..Default::default()
        };
        let compiled = compile_access_log_filter(&default_cfg);
        assert!(matches!(
            compiled,
            envoy_accesslog::LogFilter::Metadata {
                matcher: Some(_),
                match_if_key_not_found: true
            }
        ));
        assert!(compiled.should_log(200, "-", &[], &md("com.example", "k", "1"))); // match
        assert!(!compiled.should_log(200, "-", &[], &md("com.example", "k", "2"))); // mismatch
        assert!(compiled.should_log(200, "-", &[], &empty)); // absent → default true

        // (b) explicit `false` → key-absent records are DROPPED (the R-0.4
        //     polarity flip that `--mode validate` cannot reach).
        let explicit_false = envoy_config::AccessLogFilter {
            metadata_filter: Some(envoy_config::MetadataFilter {
                matcher: Some(matcher),
                match_if_key_not_found: Some(false),
            }),
            ..Default::default()
        };
        let compiled = compile_access_log_filter(&explicit_false);
        assert!(matches!(
            compiled,
            envoy_accesslog::LogFilter::Metadata {
                match_if_key_not_found: false,
                ..
            }
        ));
        assert!(compiled.should_log(200, "-", &[], &md("com.example", "k", "1")));
        assert!(!compiled.should_log(200, "-", &[], &empty)); // absent → drop

        // (c) MATCHER-LESS (upstream accepts `metadata_filter: {}`, R-0.2) →
        //     `matcher: None`, every record takes the not-found policy.
        let matcher_less = envoy_config::AccessLogFilter {
            metadata_filter: Some(envoy_config::MetadataFilter::default()),
            ..Default::default()
        };
        let compiled = compile_access_log_filter(&matcher_less);
        assert!(matches!(
            compiled,
            envoy_accesslog::LogFilter::Metadata {
                matcher: None,
                match_if_key_not_found: true
            }
        ));
        assert!(compiled.should_log(200, "-", &[], &empty));

        // (d) nested inside a composition arm (phase-73 recursion).
        let nested = envoy_config::AccessLogFilter {
            or_filter: Some(envoy_config::OrFilter {
                filters: vec![
                    envoy_config::AccessLogFilter {
                        metadata_filter: Some(envoy_config::MetadataFilter {
                            matcher: None,
                            match_if_key_not_found: false,
                        }),
                        ..Default::default()
                    },
                    envoy_config::AccessLogFilter {
                        metadata_filter: Some(envoy_config::MetadataFilter::default()),
                        ..Default::default()
                    },
                ],
            }),
            ..Default::default()
        };
        let compiled = compile_access_log_filter(&nested);
        assert!(matches!(compiled, envoy_accesslog::LogFilter::Or(ref v) if v.len() == 2));
        assert!(compiled.should_log(200, "-", &[], &empty)); // second child keeps
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p envoy-http1 --lib compile_access_log_filter_builds_metadata_arm_with_wrapper_default 2>&1 | tail -20`
Expected: FAIL — panics at `unreachable!("validated by validate_access_logs: exactly one filter arm is set")` (the 5-tuple match ignores `metadata_filter`, so a set `metadata_filter` falls into the `_` arm).

- [ ] **Step 3: Widen the match to a 6-tuple**

In `crates/envoy-http1/src/hcm.rs`, widen the tuple and add the arm (extend the fn doc comment to say SIX arms ship):

```rust
fn compile_access_log_filter(f: &envoy_config::AccessLogFilter) -> envoy_accesslog::LogFilter {
    match (
        &f.status_code_filter,
        &f.response_flag_filter,
        &f.header_filter,
        &f.and_filter,
        &f.or_filter,
        &f.metadata_filter,
    ) {
        (Some(scf), None, None, None, None, None) => {
            // ... unchanged
        }
        (None, Some(rff), None, None, None, None) => envoy_accesslog::LogFilter::ResponseFlag {
            flags: rff.flags.clone(),
        },
        (None, None, Some(hf), None, None, None) => envoy_accesslog::LogFilter::Header {
            matcher: std::sync::Arc::new(hf.header.clone()),
        },
        (None, None, None, Some(af), None, None) => envoy_accesslog::LogFilter::And(
            af.filters.iter().map(compile_access_log_filter).collect(),
        ),
        (None, None, None, None, Some(of), None) => envoy_accesslog::LogFilter::Or(
            of.filters.iter().map(compile_access_log_filter).collect(),
        ),
        // Phase 74 (ADR-0150/ADR-0155): box the config `MetadataMatcher` into
        // the injected `MetadataMatch` seam (the validator already compiled its
        // SafeRegex, so the runtime `matches` never hits its `.expect()`), and
        // resolve the `google.protobuf.BoolValue` wrapper default — absent means
        // `true` (MEASURED, SPEC §0 R-0.4; `--mode validate` provably cannot
        // reach this). A matcher-less `metadata_filter` (accepted upstream,
        // R-0.2) compiles to `matcher: None`, so every record takes the
        // not-found policy.
        (None, None, None, None, None, Some(mf)) => envoy_accesslog::LogFilter::Metadata {
            matcher: mf
                .matcher
                .as_ref()
                .map(|m| std::sync::Arc::new(m.clone()) as std::sync::Arc<dyn envoy_accesslog::MetadataMatch>),
            match_if_key_not_found: mf.match_if_key_not_found.unwrap_or(true),
        },
        _ => unreachable!("validated by validate_access_logs: exactly one filter arm is set"),
    }
}
```

> The explicit `as Arc<dyn MetadataMatch>` cast inside `.map(...)` is required — without it the closure's return type infers as `Arc<MetadataMatcher>` and the field type will not unify. Let `cargo fmt` re-wrap the long line.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p envoy-http1 --lib compile_access_log_filter 2>&1 | tail -20`
Expected: PASS — the new test plus every pre-existing `compile_access_log_filter_*` test green.
Run: `cargo test -p envoy-http1 --lib 2>&1 | tail -5` → green.
Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -10` → clean.

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-http1/src/hcm.rs
git commit -m "phase 74 T6: 6-tuple compile_access_log_filter + BoolValue-wrapper default true"
```

---

### Task 7: Differential fixture `0081-accesslog-metadata-filter` + entrypoint

**Files:**
- Create: `tests/fixtures/0081-accesslog-metadata-filter/envoy.yaml`
- Create: `tests/fixtures/0081-accesslog-metadata-filter/envoy-rust.yaml`
- Create: `tests/fixtures/0081-accesslog-metadata-filter/expectations.yaml`
- Create: `tests/fixtures/0081-accesslog-metadata-filter/README.md`
- Create: `tests/differential/tests/access_log_metadata_filter.rs`

**Context:** Fixtures are auto-discovered by path (no registry). The `http1_access_log_byte_exact` driver reads the three YAML files and asserts each side's access-log file holds exactly `expected_logged_count(probes)` byte-identical lines. Clone `0080-accesslog-or-filter` exactly, swapping the filter for a `metadata_filter`, adding an `envoy.filters.http.header_to_metadata` filter BEFORE the router, and extending the format with `%DYNAMIC_METADATA(com.example:k)%` (which is NOT `REQ_ALLOW_LIST`-gated, so the line CAN echo the gating value — unlike `0079`/`0080`). Per-side divergences (identical to `0080`): `envoy.yaml` adds `admin:` + `generate_request_id: false`, binds `0.0.0.0`, mounts `/tmp/0081-envoy-mount/`; `envoy-rust.yaml` omits admin/generate_request_id, binds `127.0.0.1`, mounts `/tmp/0081-envoy-rust-mount/`.

- [ ] **Step 1: Write the differential test entrypoint (the "test")**

Create `tests/differential/tests/access_log_metadata_filter.rs`:

```rust
//! Docker-gated differential test for fixture 0081-accesslog-metadata-filter.
//! Phase 74 (ADR-0154 / ADR-0155) — the SIXTH access-log FILTER witness (arm
//! #6) and the FIRST to gate a sink on DYNAMIC METADATA: an `AccessLog` entry
//! carrying `filter.metadata_filter` emits a record iff the request's dynamic
//! metadata, resolved at `matcher.filter` → `matcher.path[0].key`, matches
//! `matcher.value`. One HCM listener with an
//! `envoy.filters.http.header_to_metadata` filter mapping request header `x-a`
//! into dynamic metadata `com.example:k`, a `text_format_source` file sink
//! (`STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)% M=%DYNAMIC_METADATA(com.example:k)%`)
//! filtered on
//! `metadata_filter { matcher: { filter: com.example, path: [{key: k}], value: { string_match: { exact: "1" } } } }`,
//! and ONE `direct_response` route (`/x` → 200 `hi`). Unlike 0079/0080 the LINE
//! itself echoes the gating value — `%DYNAMIC_METADATA(...)%` is a distinct
//! command operator and is NOT gated by `REQ_ALLOW_LIST` (`%REQ(X-A)%` would be
//! boot-fatal). Two probes, kept-LAST (ADR-0147): (1) `GET /x` with `x-a: 2`
//! (metadata `k="2"` → value mismatch) → SUPPRESSED (`expect_logged: false`);
//! (2) `GET /x` with `x-a: 1` (metadata `k="1"` → value matches) → KEPT. Each
//! side's file holds EXACTLY ONE byte-identical line
//! `STATUS=200 PATH=/x M=1`. `clusters: []`; no backend spawns. PURE
//! cross-proxy equality: both proxies must agree on the KEPT half AND the
//! DROPPED half.

use std::path::PathBuf;

#[tokio::test]
async fn access_log_metadata_filter() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0081-accesslog-metadata-filter");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
```

- [ ] **Step 2: Create the fixture config files**

`tests/fixtures/0081-accesslog-metadata-filter/envoy-rust.yaml`:

```yaml
node: { id: envoy-rust-phase-74-fixture-0081, cluster: envoy-rust-phase-74 }
static_resources:
  listeners:
    - name: http1_listener
      address: { socket_address: { address: 127.0.0.1, port_value: {{PORT}} } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/0081-envoy-rust-mount/access.log
                      log_format:
                        text_format_source:
                          inline_string: "STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)% M=%DYNAMIC_METADATA(com.example:k)%\n"
                    # Phase 74 (ADR-0154/0155): emit only when the request's
                    # dynamic metadata com.example:k equals "1". The metadata is
                    # produced by the header_to_metadata filter below from the
                    # `x-a` request header. `match_if_key_not_found` is ABSENT
                    # here — its MEASURED default is `true`, but every probe in
                    # this fixture sets `x-a`, so the key always resolves and the
                    # not-found path is never taken (fixture 0082 covers it).
                    filter:
                      metadata_filter:
                        matcher:
                          filter: com.example
                          path:
                            - key: k
                          value:
                            string_match: { exact: "1" }
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: backend_vh
                      domains: ["*"]
                      routes:
                        - match: { path: "/x" }
                          direct_response:
                            status: 200
                            body: { inline_string: "hi\n" }
                http_filters:
                  - name: envoy.filters.http.header_to_metadata
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.header_to_metadata.v3.Config
                      request_rules:
                        - header: x-a
                          on_header_present:
                            metadata_namespace: com.example
                            key: k
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
```

`tests/fixtures/0081-accesslog-metadata-filter/envoy.yaml` — identical EXCEPT the four per-side divergences (add `admin`, add `generate_request_id: false`, bind `0.0.0.0`, mount `/tmp/0081-envoy-mount/`):

```yaml
node: { id: envoy-rust-phase-74-fixture-0081, cluster: envoy-rust-phase-74 }
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
                      path: /tmp/0081-envoy-mount/access.log
                      log_format:
                        text_format_source:
                          inline_string: "STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)% M=%DYNAMIC_METADATA(com.example:k)%\n"
                    filter:
                      metadata_filter:
                        matcher:
                          filter: com.example
                          path:
                            - key: k
                          value:
                            string_match: { exact: "1" }
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: backend_vh
                      domains: ["*"]
                      routes:
                        - match: { path: "/x" }
                          direct_response:
                            status: 200
                            body: { inline_string: "hi\n" }
                http_filters:
                  - name: envoy.filters.http.header_to_metadata
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.header_to_metadata.v3.Config
                      request_rules:
                        - header: x-a
                          on_header_present:
                            metadata_namespace: com.example
                            key: k
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
```

`tests/fixtures/0081-accesslog-metadata-filter/expectations.yaml`:

```yaml
driver:
  kind: http1_access_log_byte_exact
  expected_access_log_paths:
    envoy: /tmp/0081-envoy-mount/access.log
    envoy_rust: /tmp/0081-envoy-rust-mount/access.log
  probes:
    # Probe 1 — DROPPED, and FIRST. `x-a: 2` → header_to_metadata writes
    # com.example:k = "2" → the metadata path RESOLVES but the value matcher
    # (`exact: "1"`) says no → the sink emits NOTHING on EITHER proxy.
    # `expect_logged: false` removes it from the line count. (Kept-LAST is the
    # sound convention — ADR-0147; this fixture pays the short 2s settle
    # because the LAST probe is KEPT.)
    - method: get
      path: /x
      host: envoy-rust.test
      extra_headers:
        - ["x-a", "2"]
      expected_status: 200
      expect_logged: false
    # Probe 2 — KEPT, and LAST. `x-a: 1` → com.example:k = "1" → the value
    # matcher matches → the record IS emitted. Expected line (byte-identical on
    # both sides): STATUS=200 PATH=/x M=1
    - method: get
      path: /x
      host: envoy-rust.test
      extra_headers:
        - ["x-a", "1"]
      expected_status: 200
      expect_logged: true
  # ASSERTION = PURE CROSS-PROXY EQUALITY (whole-line ==). Each side's file
  # holds EXACTLY ONE line (MEASURED, SPEC §0 R-0.3, graceful-stop flush):
  #   STATUS=200 PATH=/x M=1
  # Both proxies must agree on the kept `x-a: 1` line AND on the absence of any
  # line for the value-mismatching `x-a: 2` probe. The only route is a
  # direct_response → clusters: [], no backend spawns.
```

- [ ] **Step 3: Write the README**

Create `tests/fixtures/0081-accesslog-metadata-filter/README.md`, modelled on `tests/fixtures/0080-accesslog-or-filter/README.md`, documenting:
- the arm-#6 `metadata_filter` dynamic-metadata gate and the MEASURED decision rule (`resolved = dynamic_metadata[filter][path[0].key]`; `None` → `match_if_key_not_found` [default `true`]; `Some(v)` → `value.matches(v)`);
- the keep/drop table:

| # | request | `com.example:k` | `exact: "1"` matches? | emitted? |
|---|---|---|---|---|
| 1 | `GET /x` `x-a: 2` | `"2"` | no | **DROPPED** |
| 2 | `GET /x` `x-a: 1` | `"1"` | yes | **KEPT** |

- the single byte-identical line `STATUS=200 PATH=/x M=1`;
- why the line CAN echo the gating value here (unlike `0079`/`0080`): `%DYNAMIC_METADATA(ns:key)%` is a distinct `Op` with its own parser and is NOT gated by `REQ_ALLOW_LIST`, whereas `%REQ(X-A)%` is boot-fatal (`BEHAVIOR_CONTRACT.md` §F and the phase-73 §D note);
- that `match_if_key_not_found` is deliberately ABSENT here — sibling fixture `0082` witnesses it;
- that `matcher.invert` is accepted-but-INERT upstream and boot-fatal here (CF-74-1) so no fixture may use it;
- the per-side divergence table (`admin`, listener bind, `generate_request_id`, access-log mount path);
- kept-LAST + `CF70_3_SETTLE`;
- cross-references: ADR-0154 (pick), ADR-0155 (§6.2 reconciliation), sibling fixture `0082`, and fixture `0042` (the `header_to_metadata` producer precedent).

- [ ] **Step 4: Build the debug binary and run the fixture locally (Docker-gated)**

```bash
cargo build -p envoy-bin
cargo test -p differential --test access_log_metadata_filter -- --nocapture 2>&1 | tail -40
```

Expected: PASS (`test access_log_metadata_filter ... ok`). Byte-exact single line `STATUS=200 PATH=/x M=1` on both proxies. If Docker is down (mass `client error (Connect)`), see memory `docker-desktop-down-after-reboot-kvm-acl`. This test is authoritative on CI; the documented local host-flake families are not regressions.

- [ ] **Step 5: Commit**

```bash
git add tests/fixtures/0081-accesslog-metadata-filter/ tests/differential/tests/access_log_metadata_filter.rs
git commit -m "phase 74 T7: fixture 0081-accesslog-metadata-filter (value keep/drop differential)"
```

---

### Task 8: Differential fixture `0082-accesslog-metadata-filter-key-not-found` + entrypoint

**Files:**
- Create: `tests/fixtures/0082-accesslog-metadata-filter-key-not-found/{envoy.yaml,envoy-rust.yaml,expectations.yaml,README.md}`
- Create: `tests/differential/tests/access_log_metadata_filter_key_not_found.rs`

**Context:** Same shape as Task 7 with `match_if_key_not_found: false` added to the filter, probing the ABSENT-KEY arm — the R-0.4 observable that fixture `0081` cannot carry, because `match_if_key_not_found` is per-sink and the driver asserts over ONE log file per side. **The `header_to_metadata` rule MUST omit `on_header_missing` entirely** (Global Constraints / PV-6): envoy-rust requires a `value` on that block, and supplying one would WRITE `com.example:k` on the no-header probe, so the key would RESOLVE and the probe would be dropped by the VALUE path instead of the key-not-found path — the fixture would pass while testing the wrong thing. Mounts `/tmp/0082-envoy-mount/` and `/tmp/0082-envoy-rust-mount/`; node id `envoy-rust-phase-74-fixture-0082`.

- [ ] **Step 1: Write the entrypoint**

Create `tests/differential/tests/access_log_metadata_filter_key_not_found.rs` — clone Task 7's entrypoint, changing the fixture dir to `0082-accesslog-metadata-filter-key-not-found`, the fn name to `access_log_metadata_filter_key_not_found`, and the doc comment to describe: the same shape plus `match_if_key_not_found: false`; two probes, kept-LAST — (1) `GET /x` with NO `x-a` (the `header_to_metadata` rule has no `on_header_missing`, so `com.example:k` is never written → the path does not resolve → `match_if_key_not_found: false` → SUPPRESSED) then (2) `GET /x` with `x-a: 1` (`k="1"` → value matches → KEPT); each side's file holds EXACTLY ONE byte-identical line `STATUS=200 PATH=/x M=1`; this witnesses the `google.protobuf.BoolValue` wrapper semantics that `--mode validate` provably cannot reach (SPEC §0 R-0.2/R-0.4 — under the ABSENT default the same no-header probe was KEPT).

- [ ] **Step 2: Create the config files**

`tests/fixtures/0082-accesslog-metadata-filter-key-not-found/envoy-rust.yaml` — identical to Task 7's `envoy-rust.yaml` EXCEPT: node id `envoy-rust-phase-74-fixture-0082`, mount `/tmp/0082-envoy-rust-mount/access.log`, and the filter stanza gains one line:

```yaml
                    filter:
                      metadata_filter:
                        matcher:
                          filter: com.example
                          path:
                            - key: k
                          value:
                            string_match: { exact: "1" }
                        # MEASURED (SPEC §0 R-0.4): `match_if_key_not_found` is a
                        # google.protobuf.BoolValue WRAPPER whose DEFAULT is
                        # `true`. Setting it explicitly to `false` FLIPS the
                        # key-absent probe from KEPT (fixture 0081's default
                        # behavior) to DROPPED — the observable `--mode validate`
                        # provably cannot reach.
                        match_if_key_not_found: false
```

The `header_to_metadata` stanza is byte-identical to Task 7's — in particular it has **`on_header_present` ONLY, no `on_header_missing`**, so a request without `x-a` writes nothing and the key is genuinely absent.

`tests/fixtures/0082-accesslog-metadata-filter-key-not-found/envoy.yaml` — the same with the four per-side divergences (`admin`, `generate_request_id: false`, bind `0.0.0.0`, mount `/tmp/0082-envoy-mount/access.log`).

`tests/fixtures/0082-accesslog-metadata-filter-key-not-found/expectations.yaml`:

```yaml
driver:
  kind: http1_access_log_byte_exact
  expected_access_log_paths:
    envoy: /tmp/0082-envoy-mount/access.log
    envoy_rust: /tmp/0082-envoy-rust-mount/access.log
  probes:
    # Probe 1 — DROPPED, and FIRST. NO `x-a` header. The header_to_metadata rule
    # has ONLY `on_header_present`, so com.example:k is never written → the
    # metadata path does NOT resolve → `match_if_key_not_found: false` → the
    # record is SUPPRESSED on BOTH proxies. Under the ABSENT default (`true`,
    # MEASURED R-0.4) this identical probe would be KEPT — that polarity flip is
    # exactly what this fixture witnesses.
    - method: get
      path: /x
      host: envoy-rust.test
      expected_status: 200
      expect_logged: false
    # Probe 2 — KEPT, and LAST. `x-a: 1` → com.example:k = "1" → the value
    # matcher matches → emitted. Expected line (byte-identical on both sides):
    #   STATUS=200 PATH=/x M=1
    - method: get
      path: /x
      host: envoy-rust.test
      extra_headers:
        - ["x-a", "1"]
      expected_status: 200
      expect_logged: true
  # ASSERTION = PURE CROSS-PROXY EQUALITY. Each side's file holds EXACTLY ONE
  # byte-identical line (MEASURED, SPEC §0 R-0.4, graceful-stop flush):
  #   STATUS=200 PATH=/x M=1
  # clusters: [], no backend spawns.
```

- [ ] **Step 3: Write the README**

Create `tests/fixtures/0082-accesslog-metadata-filter-key-not-found/README.md` documenting: the `match_if_key_not_found: false` polarity flip and why it is the SPEC R-0.4 observable that `--mode validate` cannot reach; the keep/drop table (no `x-a` → key not found → DROP; `x-a: 1` → DROP-not, KEEP); the single byte-identical line; **an explicit note that `on_header_missing` is deliberately absent** (envoy-rust requires a `value` on it, and supplying one would make the key RESOLVE and silently convert this into a duplicate of `0081`'s value-mismatch test); the per-side divergence table; kept-LAST + `CF70_3_SETTLE`; ADR-0154/0155 cross-refs and the sibling `0081`.

- [ ] **Step 4: Run the fixture locally**

```bash
cargo build -p envoy-bin
cargo test -p differential --test access_log_metadata_filter_key_not_found -- --nocapture 2>&1 | tail -40
```

Expected: PASS. One byte-identical `STATUS=200 PATH=/x M=1` line on each side.

- [ ] **Step 5: Commit**

```bash
git add tests/fixtures/0082-accesslog-metadata-filter-key-not-found/ tests/differential/tests/access_log_metadata_filter_key_not_found.rs
git commit -m "phase 74 T8: fixture 0082 (match_if_key_not_found: false absent-key drop)"
```

---

### Task 9: Fuzz corpus seed for `metadata_filter` + the `!`-un-ignore line

**Files:**
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/metadata_filter.yaml`
- Modify: `crates/envoy-config/fuzz/.gitignore` (one new `!` line after line `63`)

**Context (PV-7):** `metadata_filter` is a config sub-message on the already-fuzz-reachable `access_log[].filter` path and introduces NO new byte-parser, so it rides the existing `parse_bootstrap` target — **no new fuzz target, no `ci.yml` edit** (ADR-0137 precedent; `fuzz_targets/` holds exactly one target, and `.github/workflows/ci.yml:107` names no corpus path, so `cargo fuzz` globs the whole corpus dir). The corpus dir is `*`-ignored (`fuzz/.gitignore:1`) with 62 per-seed `!` lines (2-63); a new seed without a 63rd line is silently untracked and invisible to CI (memory `fuzz-corpus-seed-gitignored-by-default`). One seed carrying a `metadata_filter` with a `string_match` value exercises the new sub-message and the new validator, matching the one-seed-per-arm convention (`status_code_filter.yaml` / `response_flag_filter.yaml` / `header_filter.yaml` / `and_or_filter.yaml`).

- [ ] **Step 1: Create the seed**

`crates/envoy-config/fuzz/corpus/parse_bootstrap/metadata_filter.yaml`:

```yaml
node: { id: fuzz-74, cluster: fuzz-74 }
static_resources:
  listeners:
    - name: l1
      address: { socket_address: { address: 127.0.0.1, port_value: 10000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/fuzz-access.log
                    filter:
                      metadata_filter:
                        matcher:
                          filter: com.example
                          path:
                            - key: k
                          value:
                            string_match: { exact: "1" }
                        match_if_key_not_found: false
                route_config:
                  name: r
                  virtual_hosts:
                    - name: v
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response: { status: 503, body: { inline_string: "fuzz\n" } }
                http_filters:
                  - name: envoy.filters.http.header_to_metadata
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.header_to_metadata.v3.Config
                      request_rules:
                        - header: x-a
                          on_header_present:
                            metadata_namespace: com.example
                            key: k
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
```

- [ ] **Step 2: Un-ignore the seed**

In `crates/envoy-config/fuzz/.gitignore`, add after the `!corpus/parse_bootstrap/and_or_filter.yaml` line (line `63`, immediately before `artifacts/`):

```
!corpus/parse_bootstrap/metadata_filter.yaml
```

- [ ] **Step 3: Verify the seed is tracked and parses**

```bash
git add crates/envoy-config/fuzz/.gitignore crates/envoy-config/fuzz/corpus/parse_bootstrap/metadata_filter.yaml
git ls-files crates/envoy-config/fuzz/corpus/parse_bootstrap/metadata_filter.yaml
```

Expected: the path prints (tracked). If it prints nothing, the `!` line is wrong — fix it before committing.

Optionally, from the crate dir (memory `cargo-fuzz-runs-from-crate-dir-not-repo-root`):
`cd crates/envoy-config && cargo +nightly fuzz run parse_bootstrap corpus/parse_bootstrap/metadata_filter.yaml -- -runs=1 2>&1 | tail -5` → parses clean (no crash). The full 30 s CI run is the state-4 gate.

- [ ] **Step 4: Commit**

```bash
git commit -m "phase 74 T9: parse_bootstrap fuzz seed metadata_filter.yaml + un-ignore"
```

---

### Task 10: `BEHAVIOR_CONTRACT.md` `metadata_filter` subsection

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (insert immediately after the phase-73 subsection's closing `---` at line `2447`)

**Context:** Record the MEASURED facts (SPEC §0 R-0.2..R-0.5, R-0.8) as a sibling to the phase-70 (`2140`), phase-71 (`2256`), phase-72 (`2334`) and phase-73 (`2401-2446`) access-log-filter subsections. No behavior claim beyond what the fixtures + in-process tests prove; the one derived-not-measured item (`present_match` on the resolved branch) is labelled as such.

- [ ] **Step 1: Add the subsection**

Insert into `docs/envoy-rust/BEHAVIOR_CONTRACT.md` after line `2447` (re-grep: `rg -n "^### Phase 73 .*and_filter" docs/envoy-rust/BEHAVIOR_CONTRACT.md` then find the next `---`):

```markdown
### Phase 74 (ADR-0154/0155): `metadata_filter` — the SIXTH emission-gate arm (the DYNAMIC-METADATA gate)

> Fixtures `0081-accesslog-metadata-filter` + `0082-accesslog-metadata-filter-key-not-found`.
> `filter: { metadata_filter: { matcher: <type.matcher.v3.MetadataMatcher>, match_if_key_not_found: <BoolValue> } }`
> gates a sink on the request's DYNAMIC METADATA (MEASURED against
> `envoyproxy/envoy:v1.33.0`, port-mapped runtime probes with graceful-stop flush).

**§A Schema.** `matcher` is **OPTIONAL** upstream and here — `metadata_filter: {}`
reports `configuration OK` and keeps every record; rejecting it would break LOAD
PARITY. Inside `matcher`, upstream PGV requires `filter` (`min_len 1`), `path`
(`min_items 1`, each segment `key` `min_len 1`) and `value`
(`message.required`); envoy-rust enforces the same three fail-loud via
`ConfigError::AccessLogMetadataMatcherInvalid` (plus `ConfigError::Yaml` for a
missing `value`, which is a non-`Option` field). `match_if_key_not_found` is a
`google.protobuf.BoolValue` **WRAPPER** (`{ value: true }` is accepted alongside
a bare `true`), so absent and explicit-`false` are DISTINCT on the wire —
modelled as `Option<bool>`. The message is CLOSED (an unknown key is a hard
error on both sides). The `filter:` namespace is an OPAQUE, unvalidated string:
neither proxy checks that any filter ever writes it.

**§B Decision.** `resolved = dynamic_metadata[matcher.filter][matcher.path[0].key]`;
`None` (unresolved, or no `matcher` at all) → `match_if_key_not_found`;
`Some(v)` → `matcher.value.matches(v)`. **The `match_if_key_not_found` default is
`true`** — MEASURED live by flipping absent → explicit `false` and watching the
identical key-absent probe go KEPT → DROPPED (`--mode validate` provably cannot
reach a proto3 wrapper default). A missing NAMESPACE behaves identically to a
missing KEY. Compiled to `LogFilter::Metadata { matcher: Option<Arc<dyn MetadataMatch>>,
match_if_key_not_found: bool }` over an injected trait object whose `matches`
returns **`Option<bool>`** (`None` = unresolved), so the not-found policy is
expressed exactly once, in the crate that owns the field; the variant introduces
NO `Eq`/`PartialEq` and NO `envoy-config` dependency (ADR-0150 holds).
`should_log` gains a 4th parameter carrying the record's metadata store, threaded
at both HCM emit gates and through the phase-73 `And`/`Or` recursion.

**§C `invert` is accepted-but-INERT upstream on this path** (reproduced twice:
`invert: true` produced a keep/drop set byte-identical to the non-inverted run,
where honoring it would have produced the exact opposite; an `invertBOGUS`
control field is REJECTED, proving `invert` is a genuine recognised field of the
message that this evaluation path then ignores). envoy-rust's `MetadataMatcher`
has no `invert` field under `deny_unknown_fields`, so a config carrying it is
BOOT-FATAL here — a load-parity gap in the REJECT direction (ADR-0049 posture,
carry-forward CF-74-1). **"Implementing" `invert` here would CREATE a
divergence.** Note this is a DIFFERENT field on a DIFFERENT message from
`HeaderMatcher.invert_match` (CF-72-1), whose divergence is mode-scoped.

**§D Where envoy-rust is STRICTER** (the §E.1 precedent — all fail-loud at config
load, never a silent runtime difference): no `invert` field (§C); single-segment
`path` only (upstream accepts multi-segment; envoy-rust's metadata store is a
FLAT `namespace → key → string`, in which a nested path is unrepresentable —
CF-74-2); `ValueMatcher` limited to `string_match`/`present_match` (upstream also
accepts `bool_match`, `double_match`, `list_match`, `null_match`, `or_match` —
CF-74-3, blocked on the same string-only store).

**§E Mutual exclusion.** `metadata_filter` joins the SIX-arm `AccessLogFilter`
oneof (`status_code_filter`, `response_flag_filter`, `header_filter`,
`and_filter`, `or_filter`, `metadata_filter`) — exactly one may be set at each
level; zero arms and more-than-one arm are each
`ConfigError::AmbiguousAccessLogFilter` (upstream rejects the multi-arm case one
layer above PGV, in the JSON→proto parser).

**§F Rendering the gating value.** Unlike the phase-72/73 fixtures, `0081`/`0082`
render the gated metadata directly with `%DYNAMIC_METADATA(com.example:k)%` —
that operator has its own parser and is NOT constrained by `REQ_ALLOW_LIST`
(a `%REQ(X-A)%` would be boot-fatal, §F above / phase-73 §D). It renders the raw
unquoted value when present and `-` when either the namespace or the key is
absent, on both proxies.

**§G Derived, not separately measured.** R-0.3/R-0.4 measured the decision rule
with a `string_match` value. Because the value matcher is consulted ONLY when the
path resolves, `present_match: true` → KEEP and `present_match: false` → DROP for
a RESOLVED key, while an ABSENT key takes `match_if_key_not_found` in both cases.
That composition is pinned in-process only — carry-forward CF-74-5.

**§H Authoritative fixtures.** `0081`: `metadata_filter { matcher: { filter:
com.example, path: [{key: k}], value: { string_match: { exact: "1" } } } }` over a
`header_to_metadata` rule mapping `x-a` → `com.example:k` — `GET /x` with
`x-a: 2` → DROPPED (value mismatch), with `x-a: 1` → KEPT (one line
`STATUS=200 PATH=/x M=1`). `0082`: the same with `match_if_key_not_found: false`
and NO `on_header_missing` — `GET /x` with no `x-a` → DROPPED (key not found),
with `x-a: 1` → KEPT (one line). Both `direct_response` 200, `clusters: []`, no
backend. Pure cross-proxy equality on the kept lines AND the dropped absences.
```

- [ ] **Step 2: Commit**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 74 T10: BEHAVIOR_CONTRACT metadata_filter subsection"
```

---

## §7.5 phase-done gate (state-4 verification — a LATER session)

Not part of state-3. Recorded here so the state-4 session runs them (D-3.6 / §7.5):
- (a) new fixtures `0081`/`0082` green; (b) all `0001`–`0080` still green;
- (c) no new conformance suite (access-log is not codec-conformance-gated);
- (d) the `parse_bootstrap` fuzz short-budget CI run clean (the existing `ci.yml:102-107` step covers the new seed — **no `ci.yml` edit**; verify the seed is tracked with `git ls-files`);
- (e) `cargo build --workspace --all-targets` + `cargo clippy --workspace --all-targets --all-features -- -D warnings` + `cargo fmt --all -- --check` + `cargo test --workspace` + `cargo deny check` all clean;
- (f) `REVIEW.md` approved (state-5).

Watch for: the port-reuse / parallel-load startup-race host-flake families and the `tcpclosebackend-ipv6-unreachable` witnesses (CI-authoritative; adjudicate with `--no-fail-fast` + a full-output redirect, never `tail`; isolation re-runs must name the `--test <binary>` and assert on the `N passed` count, never the exit code). Rebuild `envoy-bin` before any local differential. `cargo deny` may red on a fresh unrelated RustSec advisory (patch-bump the dep — not a phase regression).

---

## §6.1 Split gate — re-derived (PV-8)

| Task | Net LoC (rough) |
|---|---|
| T1 `MetadataFilter` + oneof field + re-export + 6-arm destructure/`set_arms` + 13 construction sites + the N73-R1 doc fix (+ its 2 tests) | ~145 |
| T2 access-log-scoped matcher validator + 1 `ConfigError` variant (+ its 7 tests) | ~145 |
| T3 `should_log` 4th-parameter widening across `filter.rs`/`file_sink.rs`/both HCM gates + ~98 test call sites (+ its pin) | ~120 |
| T4 `MetadataMatch` trait + `LogFilter::Metadata` + the `should_log` arm + re-export (+ its 2 tests) | ~120 |
| T5 `impl MetadataMatch for MetadataMatcher` (+ its 3 tests) | ~90 |
| T6 6-tuple `compile_access_log_filter` + `unwrap_or(true)` (+ its test) | ~110 |
| T7 fixture `0081` (2× config + expectations + README + entrypoint) | ~145 |
| T8 fixture `0082` (2× config + expectations + README + entrypoint) | ~145 |
| T9 fuzz seed + un-ignore | ~45 |
| T10 `BEHAVIOR_CONTRACT.md` subsection | ~70 |
| **Total** | **~1135 net LoC / 10 tasks** |

Under the ~1500 LoC / ~25 task gate → **single phase, NO split** (**ADR-0156 stays UNFIRED**). Larger than phases 70–73 (~630–725) for two no-design-risk reasons: the ~98 mechanical `should_log` test call-site edits (T3), and the six-way in-process decision matrix the new arm's three-valued rule demands. The gross figure exceeds the SPEC §8 projection (~935) mainly because the tables above count the in-process tests inside each task rather than as one lump; the code-only subtotal is ~700.

## Carry-forward disposition (recorded in ADR-0155)

- **N73-R1** (the stale "THREE oneof arms" doc at `bootstrap.rs:714`) — **FOLDED** into T1 Step 3 (rewritten to SIX arms, enumerating all six with their phases). **CONSUMED.**
- **M71-3** (the all-suppressed `expected_logged_count == 0` driver shape untested) — **NOT folded; carry forward.** Unchanged from the ADR-0153 reasoning: closing it soundly needs a DEDICATED all-drop differential fixture (a third container run + flake surface), because the actual gap is the driver's line-count-0 path, which no in-process assertion exercises. Both new fixtures keep exactly one line (kept-LAST).
- **M73-R2** (no committed fixture pins the mixed-leaf / depth-3 / `response_flag_filter`-child compositions) — **PARTIALLY advanced, NOT closed; carry forward.** T6's `compile_access_log_filter` test adds a mixed-arm composition (`or_filter` over two `metadata_filter` children) and T4 pins `Metadata` under both `And` and `Or` — but these are in-process, and M73-R2 asks for a committed FIXTURE. A third fixture is out of scope here.
- **M70-R4** (`"filter": null` serialization) + **M70-R9** (provenance note) — **NOT folded; carry forward.** Not on this phase's path.
- **OPENED by this phase:**
  - **CF-74-1** — `matcher.invert` accepted-but-INERT upstream (R-0.5), boot-fatal here (a load-parity gap in the REJECT direction). Owner = a future `MetadataMatcher`-parity phase, which must ALSO measure whether `invert` is honored on the **RBAC** path before adding the field to the shared type.
  - **CF-74-2** — multi-segment `path` (upstream accepts; envoy-rust rejects), blocked on the FLAT string-only metadata store shared by `envoy-filter`, both HCMs and `envoy-accesslog`. Owner = a future metadata-store-typing phase.
  - **CF-74-3** — the unmodelled `ValueMatcher` arms (`bool_match`, `double_match`, `list_match`, `null_match`, `or_match`), same blocker, same owner.
  - **CF-74-4** *(new, opened at this PLAN-write)* — the RBAC-scoped `validate_metadata_matcher` (`bootstrap.rs:4836-4858`) does NOT check that a path segment's `key` is non-empty, though upstream PGV enforces `min_len 1`. The access-log validator added here DOES. Fixing the RBAC side means touching `RbacMetadataMatcherInvalid`'s six coupled tests — out of scope. Owner = the next RBAC-matcher phase.
  - **CF-74-5** *(new, opened at this PLAN-write)* — `present_match` on the RESOLVED branch of `metadata_filter` is DERIVED from the measured rule (the value matcher is consulted only when the path resolves), not separately live-probed. Pinned in-process only. Owner = the next phase that live-probes `metadata_filter`.
- **NOT consumed (still live):** CF-72-1 / CF-72-2 (`HeaderMatcher`-parity — this phase does not touch the header-match engine; still the strongest NEXT candidate), CF-73-1, N73-R2, M73-R1, M71-6/7/8, M69-A..I, CF-69-1/2/3/5, M68-1, M-1, CF-67-3/5/6/7, the older Minors + the HTTP-filters-family (1)–(4).

---

## Self-Review

**Spec coverage** (SPEC §2.1 in-scope items → tasks):
1. `MetadataFilter` config struct (both fields `Option`) → **T1** ✅
2. the `metadata_filter` oneof arm + re-export → **T1** ✅
3. fail-loud validation — (a) 6-arm destructure + `set_arms` cardinality → **T1**; (b) access-log-scoped matcher check + 1 new `ConfigError` → **T2**; (c) SafeRegex in-place compile → **T2**; (d) matcher-less ACCEPTS → **T2** (`matcher_less_metadata_filter_is_accepted`) ✅
4. `MetadataMatch` trait (`Option<bool>`, `Debug + Send + Sync`) → **T4**; sole impl reusing `ValueMatcher::matches` verbatim → **T5** ✅
5. `LogFilter::Metadata` + its `should_log` arm → **T4** ✅
6. the `should_log` widening + `&record.dynamic_metadata` at both gates → **T3** ✅
7. the 6-arm `compile_access_log_filter` + `unwrap_or(true)` → **T6** ✅
8. fixture `0081` → **T7** ✅
9. fixture `0082` → **T8** ✅
10. in-process tests (the `should_log` matrix incl. namespace-absent and matcher-absent; the seam's `Option<bool>` contract incl. `present_match` and each `StringMatcher` mode; the 6-arm compile incl. the wrapper default; the validator negatives; the matcher-less load-parity pin; the 6-arm cardinality; the `invert`-rejected strictness pin; the five-existing-arm regressions) → folded into **T1–T6** ✅
11. `BEHAVIOR_CONTRACT.md` subsection → **T10** ✅
12. N73-R1 folded → **T1** Step 3 ✅
- §2.3 fuzz disposition → **T9** ✅ (seed only, no new target, no `ci.yml` edit — PV-7).

**Regression coverage:** the no-`filter`-still-logs and five-existing-arm behaviors are pinned by the existing phase-70/71/72/73 tests, which T1's construction-site edits and T3's call-site edits keep compiling and green (re-run explicitly in T1 Step 6, T3 Step 5 and T6 Step 4). T3's own test additionally pins that the widening is behavior-neutral for every pre-74 arm. The unmodelled-`ValueMatcher`-arm strictness (CF-74-3) is already pinned by the phase-35/36 `ValueMatcher` deserializer tests; the `invert` strictness pin is new in T1.

**Placeholder scan:** none. Every code step shows complete code. The three items described rather than transcribed — the two fixture READMEs and T8's config files — each name their exact deltas from a fully-specified sibling that is quoted verbatim in this plan or exists in-tree (`0080-accesslog-or-filter/README.md`, T7's YAML).

**Type consistency:** `MetadataFilter { matcher: Option<MetadataMatcher>, match_if_key_not_found: Option<bool> }`; `ConfigError::AccessLogMetadataMatcherInvalid { detail: String }`; `fn validate_access_log_metadata_matcher(&mut MetadataMatcher) -> Result<(), ConfigError>`; `trait MetadataMatch { fn matches(&self, &BTreeMap<String, BTreeMap<String, String>>) -> Option<bool> }`; `LogFilter::Metadata { matcher: Option<Arc<dyn MetadataMatch>>, match_if_key_not_found: bool }`; `should_log(&self, u16, &str, &[(String, String)], &BTreeMap<String, BTreeMap<String, String>>) -> bool`; `compile_access_log_filter(&AccessLogFilter) -> LogFilter` — consistent across T1→T6 and matching the re-grepped in-tree types.

**Clippy pre-flight** (memory `plan-md-example-code-trips-clippy` — the plan's own literal Rust must pass the plan's own `-D warnings` gate): `MetadataFilter` uses `#[derive(Default)]`, not a manual impl (no `derivable_impls`); T2's validator wiring is written as a **let-chain** (`if let Some(mf) = metadata_filter && let Some(mm) = mf.matcher.as_mut()`), matching the existing `if let Some(scf) = status_code_filter && …` idiom at `bootstrap.rs:5285`, so there is no `collapsible_if`; T4's `should_log` arm matches on `&Option<Arc<dyn …>>` with both arms live (no `single_match`/`manual_map`); T5's impl is `?`-chained with no `unwrap`/`expect`; T6's `.map(|m| Arc::new(m.clone()) as Arc<dyn MetadataMatch>)` carries the explicit cast the field type requires. Run `cargo clippy --workspace --all-targets --all-features -- -D warnings` at T3 Step 5, T4 Step 4, T5 Step 4 and T6 Step 4 regardless — the gate, not this scan, is authoritative.
