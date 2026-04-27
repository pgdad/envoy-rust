# Phase 04.2 — HTTP route header matcher fan-out (all 7 modes) + ADR-0021 (regex permitted) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` to implement this plan task-by-task (fresh subagent per task + two-stage review). Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **For every code-writing task:** REQUIRED SUB-SKILL: `superpowers:test-driven-development` — failing test first, verify fails, minimal implementation, verify passes, commit. No exceptions (doctrine D-3.1).
>
> **Source of truth:** `docs/envoy-rust/phases/04.2-route-matchers/SPEC.md`. This plan operationalizes SPEC §§D1–D5. Where the plan and the SPEC disagree, the SPEC wins — flag the drift, land an ADR per D-3.5, and continue. The parent phase-04 SPEC at `docs/envoy-rust/phases/04-http1/SPEC.md` (committed at SHA `805433e`) is preserved unedited as a historical artifact; for execution it is superseded by sub-phase SPECs (`04.1-hcm-direct-response/SPEC.md` for 04.1, this 04.2 sibling SPEC for 04.2, `04.3-router-upstream/SPEC.md` for 04.3).

**Goal:** Land all 7 of Envoy's `HeaderMatcher` modes (`exact_match`, `prefix_match`, `suffix_match`, `safe_regex_match`, `range_match`, `present_match`, `string_match`) plus the `StringMatcher` tagged union (5 variants: `Exact`, `Prefix`, `Suffix`, `SafeRegex`, `Contains`) and `invert_match: bool`, exposed additively on `RouteMatch.headers: Vec<HeaderMatcher>` in `envoy-config`. Add a runtime `HeaderMatcher::matches(headers) -> bool` per-matcher predicate plus `StringMatcher::matches(value) -> bool`. Wire HCM's route walker (`crates/envoy-http1/src/hcm.rs::route_matches`) to AND-combine the per-matcher results across `route.match.headers`. Land **ADR-0021** at Task 1 narrowly permitting `regex = "1"` as a runtime dep on `envoy-config` for `HeaderMatcher::SafeRegex` + `StringMatcher::SafeRegex` compilation at config-load time. Extend `tests/fixtures/0007-http1-direct-response/` (landed in 04.1) with a second route demonstrating production matcher use: `match: { prefix: "/api/", headers: [{ name: "x-foo", exact_match: "bar" }] }` returns `direct_response 418 "teapot\n"`; the existing default route stays as `prefix: "/"` returning `direct_response 200 "ok\n"`. Two probes (one default-route fall-through, one matcher-route hit via `X-Foo: bar`) drive the amended fixture; both proxies must select the same route on each probe — the differential property 04.2 newly exercises. ~20 envoy-config validator unit tests + ~25 matcher-runtime unit tests + ~5 HCM route-walker tests + 1 fuzz-corpus seed extension + harness `Driver::Http1ProbeList` extension. No new fixture; matchers are config-side. ~1300 LoC total per SPEC §5; comfortably under both `BOOTSTRAP_PROMPT.md` §6.1 split-gates (~25 tasks / ~1500 LoC).

**Architecture:** envoy-config grows a tight, self-contained matcher type tree in `crates/envoy-config/src/bootstrap.rs` (or a sibling `matcher.rs` module — module decomposition decided per SPEC §6 signpost 19; this plan **inlines schema in `bootstrap.rs` and places matcher runtime in a sibling `crates/envoy-config/src/matcher.rs` module** because the 25-test matcher-runtime suite reads cleaner alongside the runtime impl, and `bootstrap.rs` is already 2909 LoC at 04.1 close). The schema additions are six new types: `HeaderMatcher` (`name: String`, `mode: HeaderMatcherMode`, `invert_match: bool`), `HeaderMatcherMode` (7 variants — `ExactMatch(String)`, `PrefixMatch(String)`, `SuffixMatch(String)`, `SafeRegexMatch(SafeRegex)`, `RangeMatch(Int64Range)`, `PresentMatch(bool)`, `StringMatch(StringMatcher)`), `SafeRegex` (`regex: String` + non-serde `compiled: Option<Arc<regex::Regex>>` filled by validator), `Int64Range` (`start: i64`, `end: i64`; half-open), `StringMatcher` (`mode: StringMatcherMode`, `ignore_case: bool`), and `StringMatcherMode` (5 variants — `Exact(String)`, `Prefix(String)`, `Suffix(String)`, `SafeRegex(SafeRegex)`, `Contains(String)`). Per SPEC §6 signpost 1, the field-name oneof shape (Envoy proto's discriminator is *which* mode key is present, not a `@type`-tagged value) requires hand-rolled `impl<'de> Deserialize<'de>` for `HeaderMatcher`, `StringMatcher`, and `SafeRegex` — serde's `#[serde(tag = "...")]` doesn't model field-name discrimination, and `#[serde(untagged)]` would silently pick the first parsing variant. The hand-rolled visitors collect YAML map keys, verify exactly one mode key is present, and emit `ConfigError::UnknownHeaderMatcherMode { got }` (or its `StringMatcher` peer `UnknownStringMatcherMode`) on failure. `RouteMatch` (currently `prefix: Option<String>` + `path: Option<String>`) gains a third field `headers: Vec<HeaderMatcher>` with `#[serde(default)]` — empty Vec means "no header constraints," non-empty means ALL HeaderMatchers must match per Envoy v1.33.0 default `headers_match_options: ALL`. The validator (`validate_hcm` in `bootstrap.rs:541-618`) gains a recursive matcher-walk that, for each `SafeRegex` in `HeaderMatcherMode::SafeRegexMatch` or `StringMatcherMode::SafeRegex`, calls `regex::Regex::new(&safe_regex.regex)`, wraps `Ok(re)` in `Arc::new` and stores it back on `safe_regex.compiled`; on `Err(e)` returns `ConfigError::InvalidRegex { source: e }`. The validator also enforces `EmptyHeaderName` (HeaderMatcher.name not empty), `InvalidInt64Range { start, end }` (start < end), and rejects empty matcher modes via the hand-rolled Deserialize's `UnknownHeaderMatcherMode { got }`. envoy-http1's `crates/envoy-http1/src/hcm.rs::route_matches` (currently a 6-line path-only check at lines 263-269) is extended to AND-combine `route.r#match.headers.iter().all(|m| m.matches(&req.headers))` after the path-side oneof match; ~5 new HCM unit tests cover no-headers (unchanged behavior), single-header-matcher selected, single-header-matcher-skipped, multi-header AND-success, multi-header AND-fail. The `clone_route_config` hand-clone helper in `hcm.rs:45-77` is extended to clone the new `headers: Vec<HeaderMatcher>` field (cheap — `HeaderMatcher` derives `Clone`; the `SafeRegex.compiled: Option<Arc<regex::Regex>>` is `Arc::clone`-cheap). The differential harness gains a new `Driver::Http1ProbeList { probes: Vec<Http1Probe> }` variant on `tests/differential/src/lib.rs`'s `Driver` enum (sibling of the existing `Http1` and `TlsTcpProbeList` variants per SPEC §3 D3 + §6 signpost mirroring the established `TlsTcpProbeList` shape from 03.2). Each `Http1Probe` carries `name: String`, `method: Http1Method`, `path: String`, `host: String`, `extra_headers: Vec<(String, String)>` (default empty), `expected_status: Option<u16>`, `expected_body: Option<Http1BodyRule>`, `expected_headers: Option<Http1HeaderRule>`. The `drive_http1` async helper at `tests/differential/src/lib.rs:661-741` gains an `extra_headers: &[(String, String)]` parameter (~10 LoC change to the request-line construction); the existing single-probe `Driver::Http1` callsites pass `&[]`. `run_fixture`'s dispatch arm gains a `Driver::Http1ProbeList` branch that iterates the probes, calling `drive_http1` per probe per side and applying the per-probe equivalence cascade (status / body / headers — the same 5-axis discipline the existing `Driver::Http1` arm applies). Fixture `0007-http1-direct-response/` is amended: `envoy.yaml` and `envoy-rust.yaml` add a 04.2 NEW route at the head of `routes:` with `match: { prefix: "/api/", headers: [{ name: "x-foo", exact_match: "bar" }] }` returning `direct_response: { status: 418, body: { inline_string: "teapot\n" } }`; route ordering matters because the route walker is single-pass first-match-wins (the matcher route must precede the `prefix: "/"` catch-all). `inputs/payload-matcher.bin` is a new binary file shaped like 04.1's `payload.bin` (placeholder, empty content for forward-compat with 04.3); `inputs/payload.bin` stays unchanged. `expectations.yaml` switches from the 04.1 single-`Driver::Http1` shape to the new `Driver::Http1ProbeList { probes: [...] }` shape with two probes. `README.md` gains one paragraph on the matcher route + ADR-0021 on the references list. The fuzz corpus gains one new seed `route_with_header_matchers.yaml` exercising 5 of the 7 HeaderMatcher modes simultaneously (exact_match, safe_regex_match, range_match, present_match, string_match-with-contains-and-ignore_case) inside a single Route's `headers:` Vec; `crates/envoy-config/fuzz/.gitignore`'s allow-list and the in-tree `bootstrap.rs::tests::fuzz_corpus_seeds_parse_or_reject_cleanly` corpus-walk test (line 1549+) both gain the new entry. `Cargo.lock` syncs as a dedicated post-state-4 commit per the established phase-precedent (phase-01 `4955252`, phase-02.1 `dea4d16`, phase-02.2 `2146014`, phase-03.1 `eb039e6`, phase-03.2 `85633e7`); the new transitive surface is `regex` + `regex-syntax` + `aho-corasick` + `memchr`. ADR-0021 lands inline with Task 1 (mirrors phase-03.1 Task 1's ADR-0018 + ADR-0019 inline-landing pattern); no other ADRs anticipated.

**Tech stack:** Rust edition 2024 on pinned stable `1.95.0` (D-3.9). New runtime dep on `envoy-config`: `regex = "1"` (under ADR-0021, narrowly scoped to header / route matching at config-load time). No new envoy-config dev-deps. No changes to the `envoy-http1` crate's deps (the route walker integration uses already-imported `envoy_config::*` types). No changes to `tests/differential/Cargo.toml` (the `Http1Probe` extension is in-crate type additions; serde + serde_yaml + tokio + httparse already present). No changes to `envoy-bin`. No changes to `.github/workflows/ci.yml` (per SPEC §3 D5: existing `cargo test --workspace` + fuzz job pick up additions automatically).

---

## File structure (created / modified / not touched)

**Created:**

- `crates/envoy-config/src/matcher.rs` — new module owning `HeaderMatcher::matches` + `StringMatcher::matches` runtime + ~25 matcher-runtime unit tests. Schema types stay in `bootstrap.rs` (close to existing `RouteMatch`); runtime moves to its sibling per SPEC §6 signpost 19.
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/route_with_header_matchers.yaml` — new fuzz seed exercising 5 of the 7 modes simultaneously per SPEC §3 D1 ("fuzz corpus extension").
- `tests/fixtures/0007-http1-direct-response/inputs/payload-matcher.bin` — new probe input file (empty per the 04.1 `payload.bin` placeholder convention; the harness `drive_http1` constructs the wire request from `Http1Probe.{method, path, host, extra_headers}`).
- `docs/envoy-rust/phases/04.2-route-matchers/PROGRESS.md` (appended once per task during execution).

**Modified:**

- `crates/envoy-config/Cargo.toml` — add `regex = "1"` to `[dependencies]` (sibling of the existing `serde`, `serde_yaml`, `thiserror` entries) under ADR-0021.
- `crates/envoy-config/src/bootstrap.rs` — add `HeaderMatcher` + `HeaderMatcherMode` + `SafeRegex` + `Int64Range` + `StringMatcher` + `StringMatcherMode` types after the existing `DirectResponse` block (current line 327+); add hand-rolled `impl<'de> Deserialize<'de>` for `HeaderMatcher`, `StringMatcher`, `SafeRegex`; add `headers: Vec<HeaderMatcher>` field to the existing `RouteMatch` struct (line 318); extend `validate_hcm` (line 541) with the matcher-walk that compiles regexes / validates Int64Range bounds / rejects empty header names; add `mod matcher;` declaration; extend `bootstrap.rs::tests` with ~20 new validator + parse-shape tests and 1 corpus-walk allow-list addition for the new fuzz seed.
- `crates/envoy-config/src/lib.rs` — add `pub mod matcher;`; extend the `pub use bootstrap::{...}` re-export list with `HeaderMatcher`, `HeaderMatcherMode`, `SafeRegex`, `Int64Range`, `StringMatcher`, `StringMatcherMode`; add 4 new `ConfigError` variants `EmptyHeaderName`, `InvalidRegex`, `InvalidInt64Range`, `UnknownHeaderMatcherMode` (and the `StringMatcher` peer `UnknownStringMatcherMode` per the field-name-oneof discipline).
- `crates/envoy-http1/src/hcm.rs` — extend `route_matches` (line 263-269) to AND-combine `route.r#match.headers.iter().all(|m| m.matches(&req.headers))` after the existing path-side oneof match; extend `clone_route_config` (line 45-77) to clone the new `headers: Vec<HeaderMatcher>` field; add ~5 new HCM unit tests covering no-headers (unchanged), single-matcher-selected, single-matcher-skipped, multi-AND-success, multi-AND-fail.
- `crates/envoy-config/fuzz/.gitignore` — append one allow-list entry `!corpus/parse_bootstrap/route_with_header_matchers.yaml`.
- `tests/differential/src/lib.rs` — add `Driver::Http1ProbeList { probes: Vec<Http1Probe> }` variant on the existing `Driver` enum; add `Http1Probe` struct (`name`, `method`, `path`, `host`, `extra_headers: Vec<(String, String)>` with `#[serde(default)]`, `expected_status`, `expected_body`, `expected_headers`); extend `drive_http1` signature to take `extra_headers: &[(String, String)]` (existing single-probe `Driver::Http1` callsites at lines 1054, 1057 pass `&[]`); add `Driver::Http1ProbeList` dispatch arm in `run_fixture` per SPEC §3 D3; add ~2 new harness unit tests asserting the `expectations.yaml` round-trip parses and the dispatch arm threads `extra_headers` through.
- `tests/fixtures/0007-http1-direct-response/envoy.yaml` — add a 04.2 NEW route at the head of `routes:` (matcher route, 418 teapot); existing `prefix: "/"` catch-all stays second.
- `tests/fixtures/0007-http1-direct-response/envoy-rust.yaml` — same `routes:` shape (the file divergences from 04.1 — bind address `127.0.0.1`, no admin block — are unchanged).
- `tests/fixtures/0007-http1-direct-response/expectations.yaml` — switch from `Driver::Http1` to `Driver::Http1ProbeList` with two probes (`default-route` and `matcher-route`); equivalence rules unchanged.
- `tests/fixtures/0007-http1-direct-response/README.md` — append one paragraph on the 04.2-added matcher route + add ADR-0021 to the ADR-references list.
- `docs/envoy-rust/DECISIONS.md` — ADR-0021 appended at Task 1.
- `docs/envoy-rust/ROADMAP.md` — at state 6 only, flip row `04.2` `status` `in-progress` → `done`. (Parent row `04` stays `in-progress`; flips at 04.3's final commit per the schema invariant.)
- `docs/envoy-rust/STATE.md` — at state 6 only, advance active phase id `04.3`, slug `04.3-router-upstream`, lifecycle state 2 (SPEC.md exists from parent-04 state-2 commit `1d9740d`, PLAN.md does not), next-skill `superpowers:writing-plans`.
- `Cargo.lock` — synced as a dedicated commit at the state-4 phase-done gate per the established phase-precedent. Expected new entries: `regex` + `regex-syntax` + `aho-corasick` + `memchr`.
- `deny.toml` — likely no-op at Task 1 (per ADR-0021 consequences: `regex` is dual-licensed MIT/Apache-2.0, already on the allow-list since phase 00; transitives `regex-syntax` MIT/Apache-2.0, `aho-corasick` MIT/Unlicense, `memchr` MIT/Unlicense are also covered). Cross-check at Task 1; if a fresh license surfaces, the `[licenses]` allow-list extension lands in the same Task-1 commit as ADR-0021.

**Not touched in 04.2** (belong to 04.1, 04.3, earlier phases, or are frozen):

- `docs/envoy-rust/phases/04-http1/SPEC.md` (parent) — unedited; remains the design artifact committed at SHA `805433e`.
- `docs/envoy-rust/phases/04.1-hcm-direct-response/{SPEC,PLAN,PROGRESS,REVIEW}.md` — closed at the 04.1 phase-done commit `c5c40ec`; unedited in 04.2.
- `docs/envoy-rust/phases/04.3-router-upstream/SPEC.md` — landed at parent-04 state-2 commit `1d9740d`; unedited in 04.2 (PLAN/PROGRESS/REVIEW land in 04.3 execution).
- `docs/envoy-rust/phases/03-tls-tcp/SPEC.md`, `phases/03.1-tls-foundation-downstream/`, `phases/03.2-tls-upstream-sni/` — closed in phase 03.
- `docs/envoy-rust/phases/02-tcp-proxy/SPEC.md`, `phases/02.1-config-cluster/`, `phases/02.2-listener-tcp-proxy/` — closed in phase 02.
- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — no edits in 04.2 (per SPEC §2: matchers are config-side; no new response headers; `server` + `date` allow-list table populated in 04.1 stays as-is). The `x-envoy-upstream-service-time` row lands in 04.3 per parent SPEC §2.
- `crates/envoy-tls/`, `crates/envoy-tcp/`, `crates/envoy-listener/`, `crates/envoy-cluster/`, `crates/envoy-bin/` — unchanged; the matcher work is purely in `envoy-config` + a ~10 LoC route-walker tweak in `envoy-http1`.
- `crates/envoy-http1/Cargo.toml` — unchanged; the matcher integration uses already-imported `envoy_config::*` types (`HeaderMatcher` + `RouteMatch.headers`).
- `crates/envoy-bin/src/main.rs`, `crates/envoy-bin/tests/http1_direct_response.rs` — unchanged; the matcher integration is transparent to envoy-bin's listener-walk.
- `tests/fixtures/0001-tcp-echo/`, `tests/fixtures/0002-static-admin-ready/`, `tests/fixtures/0003-tcp-proxy/`, `tests/fixtures/0004-tls-downstream/`, `tests/fixtures/0005-tls-upstream/`, `tests/fixtures/0006-tls-sni/` — unedited; their fixtures must remain green at the 04.2 state-4 gate.
- `tests/helpers/{tcp,tls}-echo-server/` — unchanged; the fixture-0007 amendment does not introduce an upstream backend (the matcher route returns `direct_response`, not `route` per ADR-0020's split — `route` action lands in 04.3).
- `tests/helpers/http1-echo-server/` — does not exist yet; lands in 04.3.
- `tests/differential/Cargo.toml` — unchanged; `serde`, `serde_yaml`, `tokio`, `httparse` already present; the new `Http1Probe` type uses existing serde derive.
- `crates/envoy-http1/src/{codec,date,error,headers,response}.rs` — unchanged.
- `rust-toolchain.toml`, `docs/envoy-rust/ENVOY_TARGET.md` — frozen per D-3.7 / D-3.9.
- `.github/workflows/ci.yml` — untouched (SPEC §3 D5: no CI workflow changes in 04.2).

---

## Task index

Each task ends with a commit. `PROGRESS.md` gets a new section per task in the phase-04.1 / phase-03.2 style (task id, commit SHA, change summary, verification tail, deviations from PLAN). Use either the `sed`-then-amend idiom or the follow-up `phase 04.2: progress note (task N)` commit convention — whichever is picked for Task 1 stays consistent through Task 12. (04.1 used the follow-up-progress-note convention; 04.2 plan-writer recommends the same for consistency.)

Ordering rationale (SPEC §6 signposts 1, 9, 16, 19, 20):

- **ADR-0021 + the `regex` Cargo dep** lands first (Task 1) so subsequent tasks can name the dep at compile time.
- **Schema additions** (Tasks 2–4) precede **the validator extension** (Task 5) because the validator references the new types; tests on the schema's parse shape land alongside the schema (TDD red→green).
- **Matcher runtime** (Task 6 — `HeaderMatcher::matches` + `StringMatcher::matches` in `matcher.rs`) precedes **HCM route-walker integration** (Task 7) because the HCM consumes the per-matcher predicate.
- **Fuzz seed extension** (Task 8) lands after the schema + validator are in place because the seed must round-trip through `parse_bootstrap` cleanly.
- **Differential harness extensions** (Task 9 — `Driver::Http1ProbeList` + `Http1Probe` + `extra_headers`) precede **fixture-0007 amendment** (Task 10) because the fixture's `expectations.yaml` references the new `Driver::Http1ProbeList` schema.
- **State-4 phase-done gate** (Task 12) lands last after the in-process and Docker-gated tests are wired through.

Tasks 11 (carryforward / pre-flight) is a small task documented for completeness — it captures any 04.1 REVIEW M-track items that surface during execution. Per SPEC §3 D4 the standard expectation is "no action in 04.2" because 04.1's M-track items (M1–M7 from REVIEW §3) are scoped forward to 04.3, phase 05, or hardening — but the task slot exists so a deviation can land cleanly if a 04.1 carryforward turns out to be on the critical path.

1. **ADR-0021 (`regex` permitted) + `crates/envoy-config/Cargo.toml` runtime dep + 4 `ConfigError` variants stub**
2. **`envoy-config` schema — `Int64Range` + `SafeRegex` (with hand-rolled `Deserialize` for the non-serde `compiled` field) + 4 parse-shape tests**
3. **`envoy-config` schema — `StringMatcher` + `StringMatcherMode` (hand-rolled `Deserialize` for the field-name oneof) + 5 parse-shape tests**
4. **`envoy-config` schema — `HeaderMatcher` + `HeaderMatcherMode` (hand-rolled `Deserialize` for the field-name oneof) + `RouteMatch.headers` extension + 6 parse-shape tests**
5. **`envoy-config` validator — regex compile pass + Int64Range bounds + EmptyHeaderName + UnknownHeaderMatcherMode + ~10 validator tests**
6. **`envoy-config::matcher` — `HeaderMatcher::matches` + `StringMatcher::matches` runtime + ~25 matcher-runtime unit tests**
7. **`envoy-http1::hcm` — route walker integration (~10 LoC) + `clone_route_config` extension + ~5 HCM unit tests**
8. **`envoy-config` fuzz corpus — `route_with_header_matchers.yaml` seed + `.gitignore` allow-list + `bootstrap.rs::tests` corpus-walk extension**
9. **Differential harness — `Driver::Http1ProbeList` + `Http1Probe` + `drive_http1` `extra_headers` parameter + dispatch arm + 2 harness unit tests**
10. **Fixture `0007-http1-direct-response` amendment — envoy.yaml + envoy-rust.yaml + new probe input + expectations.yaml restructure + README.md paragraph**
11. **04.1 REVIEW M-track carryforward check (status = expected no action; document in PROGRESS)**
12. **State 4 phase-done gate — run all 5 stable commands + observe CI; quote outputs into PROGRESS.md; sync `Cargo.lock` as a dedicated commit**

Estimated total: 12 tasks, ~1300 LoC. Both `BOOTSTRAP_PROMPT.md` §6.1 gates (~25 tasks / ~1500 LoC) hold comfortably (12 < 25, ~1300 < 1500). **Do not split 04.2 further.** Per parent-04 SPEC §5 + the parent-04 state-1 brainstorm's express avoidance of nested splits, a 04.2.1 / 04.2.2 split would be a strong scope-creep signal and warrants `superpowers:systematic-debugging` first per `BOOTSTRAP_PROMPT.md` §6.1. The most plausible scope-creep vector is the matcher-runtime test count (Task 6 ~25 tests is an estimate); per SPEC §5 closing paragraph, the planner may opt for a more terse table-driven test shape (one test fn iterating a `(mode, value, expected)` triple list) if the test count alone pushes the LoC gate — coverage-preserving refactor, not a split trigger.

---

### Task 1: ADR-0021 (`regex` permitted) + `crates/envoy-config/Cargo.toml` runtime dep + 4 `ConfigError` variants stub

**Files:**
- Modify (append): `docs/envoy-rust/DECISIONS.md` (append ADR-0021 verbatim from SPEC §7 below the current ADR-0020)
- Modify: `crates/envoy-config/Cargo.toml` (add `regex = "1"` to `[dependencies]`)
- Modify: `crates/envoy-config/src/lib.rs` (add 4 new `ConfigError` variants — `EmptyHeaderName`, `InvalidRegex`, `InvalidInt64Range`, `UnknownHeaderMatcherMode`; the 5th `UnknownStringMatcherMode` lands in Task 3 alongside the StringMatcher hand-rolled Deserialize that emits it)
- Modify: `Cargo.lock` (auto-updated by `cargo build`; expected new entries `regex`, `regex-syntax`, `aho-corasick`, `memchr`)
- Verify: `deny.toml` (cross-check; expected no-op)
- Create: `docs/envoy-rust/phases/04.2-route-matchers/PROGRESS.md` (new file with Task 1 section)

**Why first:** every subsequent task that names `regex::Regex` (Tasks 2, 5, 6) needs the dep present; the ADR establishes the foundation grant; the four `ConfigError` variants are referenced by Tasks 5–6 (the 5th `UnknownStringMatcherMode` is added at Task 3 because it's emitted by the StringMatcher hand-rolled Deserialize, which lives in Task 3's scope). Mirrors phase 03.1 Task 1's ADR-0018 + ADR-0019 inline-landing pattern.

**Scope.** ADR-0021 markdown text + one runtime-dep Cargo.toml line + 4 enum variants on `ConfigError`. No new Rust code other than the enum variants (no usages yet — Tasks 2–6 wire them in). No source changes to `bootstrap.rs` or `lib.rs`'s `pub use bootstrap::{...}` block. The 5th `UnknownStringMatcherMode` variant is intentionally deferred to Task 3 to keep Task 1's Cargo.toml-and-ADR scope clean; both tasks' commits cite ADR-0021.

**Pre-flight check.**

- [ ] **Step 1: Verify the ADR ledger head + STATE.md routing.**

```bash
grep -c '^## ADR-00' docs/envoy-rust/DECISIONS.md
grep -n '^## ADR-00' docs/envoy-rust/DECISIONS.md | tail -3
grep -A2 '^## Active phase' docs/envoy-rust/STATE.md | head -5
```

Expected: count `20`; last three are `ADR-0018`, `ADR-0019`, `ADR-0020`. STATE.md `Active phase: id: 04.2`, `slug: 04.2-route-matchers`, `lifecycle state 2`. If any unexpected `ADR-00NN` appears, debug per `superpowers:systematic-debugging` before continuing — phase 04.2 anticipates exactly one new ADR (ADR-0021) at this task and none thereafter (per SPEC §7).

- [ ] **Step 2: Verify `regex` license + transitives are on the deny.toml allow-list.**

```bash
grep -E 'MIT|Apache-2.0|Unlicense' deny.toml | head -10
```

Expected: `MIT`, `Apache-2.0`, and `Unlicense` are all on the `[licenses] allow` list. Per ADR-0021 consequences: `regex` is dual-licensed MIT/Apache-2.0 (covered); `regex-syntax` is MIT/Apache-2.0 (covered); `aho-corasick` is MIT/Unlicense (`Unlicense` must be on the allow-list); `memchr` is MIT/Unlicense (same). Currently `Unlicense` is NOT on the allow-list (verified in the deny.toml head-50 read at PLAN write time — listed: `Apache-2.0`, `Apache-2.0 WITH LLVM-exception`, `MIT`, `BSD-2-Clause`, `BSD-3-Clause`, `ISC`, `Unicode-DFS-2016`, `Unicode-3.0`, `Zlib`, `CC0-1.0`, `MPL-2.0`, `0BSD`). **`Unlicense` must be added to the `[licenses] allow` list as part of Task 1**, alongside ADR-0021's landing — per SPEC §3 D2 / §7 consequences ("Plan-writer cross-checks `deny.toml` at 04.2 Task 1 alongside the ADR landing; updates the `[licenses]` allow-list only if a fresh transitive license surfaces"). Document the addition in ADR-0021's Consequences section and in PROGRESS.md Task 1.

If `cargo deny check` after Step 4 below surfaces any *additional* license beyond `Unlicense`, evaluate per ADR-0005's discipline and document in PROGRESS.md.

- [ ] **Step 3: Append ADR-0021 to `docs/envoy-rust/DECISIONS.md`.**

Append after the existing ADR-0020 block (which ends around line 392 — the `## ADR-0020` ... ending at `--- ` separator). Use the verbatim text from SPEC §7, with `Date: 2026-04-27` (today's date, per `currentDate` in this session). The exact text:

```markdown
## ADR-0021: `regex` permitted as a foundation for header / route matching

- Date: 2026-04-27
- Status: accepted
- Context: Phase 04.2 lands all 7 of Envoy's `HeaderMatcher` modes — `exact_match`, `prefix_match`, `suffix_match`, `safe_regex_match`, `range_match`, `present_match`, `string_match` (with the `StringMatcher` tagged union which itself has a `safe_regex` variant). Two of those modes — `safe_regex_match` and `string_match.safe_regex` — require a regex implementation. The Rust `regex` crate is the de-facto ecosystem default (RE2-compatible NFA engine, no backtracking, no catastrophic regex blow-ups; well-maintained; first-party `rust-lang` org). Not on the D-3.2 permitted-foundations list at phase-03.2 close (ADR-0019 was the latest ADR; the latest pre-04 permitted-foundations grant covered `tokio-rustls` + `rustls-pemfile` under the rustls grant).
- Options considered: (i) **defer `safe_regex_match` to a later phase** — rejected; the parent-04 brainstorm decision (per ADR-0020's context section + parent-04 SPEC §3 D6.2) was to land all 7 HeaderMatcher modes in 04.2 coherently; deferring one mode would scatter the matcher coverage across phases for arbitrary reasons; (ii) **hand-roll a regex engine** — rejected; reinvents wheels D-3.2 explicitly tells us not to; the `regex` crate is mature and ecosystem-standard; (iii) **add `regex = "1"` to the permitted-foundations list narrowly scoped to header / route matching at config-load time** (decision); (iv) **add `regex = "1"` to the permitted-foundations list with broad scope** — rejected; D-3.2's spirit is one-foundation-per-purpose; broader scopes warrant their own scope-extension ADRs at the time the broader use surfaces.
- Decision: extend the D-3.2 permitted-foundations list to cover `regex = "1"` as a runtime dep on `crates/envoy-config/`, narrowly scoped to `HeaderMatcher::SafeRegex` + `StringMatcher::SafeRegex` compilation at config-load time. NOT permitted for general-purpose use elsewhere; future filter-framework regex needs (URL path templates in a future router-knob phase, header-rewrite patterns in a future filter-framework phase, Lua filter `string.find` in a future Lua-filter phase) require an explicit scope-extension ADR that names this ADR and broadens the grant.
- Rationale: removes the per-phase-ADR churn that would otherwise dog later regex-using phases (HCM-internal regex would still warrant its own ADR if/when it surfaces — the narrow scope here is deliberate). `regex` is the Rust-ecosystem default; treating its first use as the foundation grant is the cheapest, most honest formalization. Compiling regexes at config-load time (validator pass) means unparseable patterns are caught before any request is served.
- Consequences: `crates/envoy-config/Cargo.toml`'s `[dependencies]` section gains `regex = "1"` at this commit. `Cargo.lock` gains `regex` + transitive surface (`regex-syntax`, `aho-corasick`, `memchr`) as a dedicated commit at the 04.2 state-4 phase-done gate per established phase precedent (phase-01 `4955252`, phase-02.1 `dea4d16`, phase-02.2 `2146014`, phase-03.1 `eb039e6`, phase-03.2 `85633e7`). `cargo deny check` requires the `Unlicense` license to be added to `deny.toml`'s `[licenses] allow` list (transitive `aho-corasick` + `memchr` are MIT/Unlicense dual-licensed); that addition lands in this same Task-1 commit. `regex` itself is dual-licensed MIT/Apache-2.0 (already on the allow-list since phase 00); `regex-syntax` is MIT/Apache-2.0 (already covered). Future scope-extension ADRs that broaden the grant (HCM internal regex, filter-framework regex) name this ADR explicitly.
- Provenance: this ADR was projected as the next-sequential available ADR number in parent-04 SPEC §7 (`docs/envoy-rust/phases/04-http1/SPEC.md`, committed at SHA `805433e`); ADR-0020 (parent-04 split decision) lands at parent-04 state-2 commit `1d9740d`; ADR-0021 lands at this commit (04.2 Task 1).

---
```

If at execution time the date drifts past 2026-04-27, use the actual landing date.

- [ ] **Step 4: Add `regex = "1"` to `crates/envoy-config/Cargo.toml`.**

Edit `crates/envoy-config/Cargo.toml`'s `[dependencies]` section (currently `serde`, `serde_yaml`, `thiserror`):

```toml
[dependencies]
regex = "1"
serde = { version = "1", features = ["derive"] }
serde_yaml = "0.9"
thiserror = "2"
```

Alphabetic ordering (`regex` first lexicographically) matches existing convention.

- [ ] **Step 5: Add `Unlicense` to `deny.toml`'s `[licenses] allow` list.**

Edit `deny.toml` to insert `"Unlicense",` into the `allow = [...]` block. Place it alphabetically (between `MPL-2.0` and `Unicode-3.0`, or at the end — match the file's existing ordering convention).

- [ ] **Step 6: Add 4 `ConfigError` variants to `crates/envoy-config/src/lib.rs`.**

Locate the existing `pub enum ConfigError { ... }` block (currently lines 34–128). Append after the existing `MultipleHttpFilters { count: usize }` variant (the last 04.1-added variant):

```rust
    /// HeaderMatcher.name was empty. Phase 04.2.
    #[error("HeaderMatcher.name must be non-empty")]
    EmptyHeaderName,

    /// SafeRegex.regex failed `regex::Regex::new`. Phase 04.2 (under ADR-0021).
    #[error("invalid regex `{regex}`: {source}")]
    InvalidRegex {
        regex: String,
        #[source]
        source: regex::Error,
    },

    /// Int64Range.start >= Int64Range.end (the half-open interval would be
    /// empty). Phase 04.2.
    #[error("invalid Int64Range: start {start} must be < end {end}")]
    InvalidInt64Range { start: i64, end: i64 },

    /// HeaderMatcher's hand-rolled Deserialize encountered an unrecognized mode
    /// key. Phase 04.2; the seven recognized keys are `exact_match`,
    /// `prefix_match`, `suffix_match`, `safe_regex_match`, `range_match`,
    /// `present_match`, `string_match`.
    #[error(
        "unknown HeaderMatcher mode key: {got:?}; expected one of exact_match, prefix_match, suffix_match, safe_regex_match, range_match, present_match, string_match"
    )]
    UnknownHeaderMatcherMode { got: String },
```

These variants reference `regex::Regex` / `regex::Error` — `regex = "1"` from Step 4 makes that compile.

- [ ] **Step 7: Build the workspace to verify `Cargo.lock` updates and license check passes.**

```bash
cargo build --workspace --all-targets
```

Expected: clean build; `Cargo.lock` updates with new entries for `regex`, `regex-syntax`, `aho-corasick`, `memchr` (and possibly `unicode-ident` etc. transitively). Document the diff in PROGRESS.md.

```bash
cargo deny check
```

Expected: `advisories ok, bans ok, licenses ok, sources ok`. The previously-warning `license-not-encountered` set may shift (e.g. `Unlicense` no longer warned because it's now used). If a fresh license surfaces, evaluate per ADR-0005 and document in PROGRESS.md — likely path is to add the license to `deny.toml` allow-list.

- [ ] **Step 8: Run lints + format check.**

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: both exit 0. The new `ConfigError` variants are unused (consumed in Tasks 5–6) but `pub`-visible enum variants are exempt from `dead_code` lint per the established workspace pattern.

- [ ] **Step 9: Run `envoy-config` tests to verify no regression.**

```bash
cargo test -p envoy-config --lib
```

Expected: previous count from 04.1 close (75 per PROGRESS Task 17) passes unchanged. No new tests added in Task 1.

- [ ] **Step 10: Create `docs/envoy-rust/phases/04.2-route-matchers/PROGRESS.md` with a Task 1 section.**

```markdown
# Phase 04.2 Progress

## Task 1 — ADR-0021 (regex permitted) + envoy-config Cargo dep + 4 ConfigError variants stub (2026-04-27)

- Commit: <SHA>
- Change: appended ADR-0021 to docs/envoy-rust/DECISIONS.md (regex = "1" narrowly permitted as a foundation for header / route matching at config-load time); added `regex = "1"` to crates/envoy-config/Cargo.toml [dependencies]; added `Unlicense` to deny.toml [licenses] allow list (transitive aho-corasick + memchr are MIT/Unlicense dual-licensed); added 4 ConfigError variants in lib.rs (EmptyHeaderName, InvalidRegex, InvalidInt64Range, UnknownHeaderMatcherMode); Cargo.lock updated with regex + regex-syntax + aho-corasick + memchr entries.
- Verification: `cargo build --workspace --all-targets` → clean; `cargo deny check` → advisories ok, bans ok, licenses ok, sources ok; `cargo clippy --workspace --all-targets --all-features -- -D warnings` → clean; `cargo fmt --all -- --check` → clean; `cargo test -p envoy-config --lib` → 75 passed (unchanged from 04.1 close).
- Tests added: none in Task 1.
- ADRs: ADR-0021 (this task). ADR ledger head: 21.
- Deviations from PLAN: <document any>.
```

Replace `<SHA>` with the commit hash from Step 11.

- [ ] **Step 11: Commit.**

```bash
git add docs/envoy-rust/DECISIONS.md crates/envoy-config/Cargo.toml crates/envoy-config/src/lib.rs deny.toml Cargo.lock
git status   # confirm only intended files
git commit -m "phase 04.2: ADR-0021 (regex permitted) + envoy-config Cargo dep + 4 ConfigError variants stub (task 1) [ADR-0021]"
```

Then commit PROGRESS.md as a follow-up note (mirror 04.1's `phase 04.1: progress note (task N)` cadence):

```bash
git add docs/envoy-rust/phases/04.2-route-matchers/PROGRESS.md
git commit -m "phase 04.2: progress note (task 1)"
```

(Or fold both into the same commit if the executor prefers; either pattern is acceptable per `BOOTSTRAP_PROMPT.md` and the 04.1 precedent. Keep the choice consistent through Task 12.)

---

### Task 2: `envoy-config` schema — `Int64Range` + `SafeRegex` (with hand-rolled Deserialize for the non-serde `compiled` field) + 4 parse-shape tests

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (append `Int64Range` + `SafeRegex` types after the existing `DirectResponse` block at line 327; add hand-rolled `impl<'de> Deserialize<'de> for SafeRegex`; add `impl PartialEq for SafeRegex` comparing only the `regex: String` field)
- Modify (append): `docs/envoy-rust/phases/04.2-route-matchers/PROGRESS.md`

**Why now:** `Int64Range` is the smallest matcher type (one struct, two fields) and `SafeRegex` introduces the hand-rolled-Deserialize pattern that Tasks 3 + 4 reuse for `StringMatcher` and `HeaderMatcher`. Landing them first keeps Task 3 + 4 focused on the field-name oneof discrimination rather than re-explaining the `compiled: None` workflow. No validator wiring yet (Task 5) — these types parse via serde, but `validate` does not yet check them.

**Scope.** Two new types + one hand-rolled `Deserialize` impl + one custom `PartialEq` impl. `SafeRegex.compiled: Option<Arc<regex::Regex>>` is a non-serde field set to `None` at deserialize time and filled by the validator (Task 5). `regex::Regex` doesn't impl `PartialEq`, so the `SafeRegex` `PartialEq` compares only the `regex: String` field per SPEC §6 signpost 17. 4 parse-shape unit tests.

- [ ] **Step 1: Read the current bootstrap.rs shape so the additions slot in cleanly.**

```bash
grep -n '^pub struct DirectResponse\|^pub struct RouteMatch\|^pub struct DataSource' crates/envoy-config/src/bootstrap.rs
```

Expected: `DataSource` at 240, `RouteMatch` at 318, `DirectResponse` at 327. The new types append after `DirectResponse` (around line 331+). The existing `RouteMatch` struct gets its `headers: Vec<HeaderMatcher>` field in Task 4 (after `HeaderMatcher` is declared); Task 2 doesn't touch `RouteMatch` yet.

- [ ] **Step 2: Write 2 failing parse-shape tests for `Int64Range`.**

Append to `crates/envoy-config/src/bootstrap.rs::tests` (the existing `#[cfg(test)] mod tests { ... }` block):

```rust
#[test]
fn parses_int64_range() {
    let yaml = r#"
start: 1
end: 100
"#;
    let r: Int64Range = serde_yaml::from_str(yaml).expect("parses");
    assert_eq!(r.start, 1);
    assert_eq!(r.end, 100);
}

#[test]
fn rejects_unknown_field_in_int64_range() {
    let yaml = r#"
start: 1
end: 100
step: 5
"#;
    let res: Result<Int64Range, _> = serde_yaml::from_str(yaml);
    assert!(res.is_err(), "deny_unknown_fields should reject `step`");
}
```

- [ ] **Step 3: Run the tests to verify they fail.**

```bash
cargo test -p envoy-config parses_int64_range rejects_unknown_field_in_int64_range
```

Expected: FAIL with compile error referencing unknown name `Int64Range`.

- [ ] **Step 4: Add the `Int64Range` type.**

Append after the existing `DirectResponse` struct (around line 331):

```rust
/// Half-open i64 range. Validator rejects start >= end with
/// ConfigError::InvalidInt64Range. Phase 04.2.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Int64Range {
    pub start: i64,
    pub end: i64,
}
```

- [ ] **Step 5: Run the Int64Range tests; expect green.**

```bash
cargo test -p envoy-config parses_int64_range rejects_unknown_field_in_int64_range
```

Expected: 2 passed.

- [ ] **Step 6: Write 2 failing parse-shape tests for `SafeRegex`.**

Append to the same `tests` block:

```rust
#[test]
fn parses_safe_regex() {
    let yaml = r#"
regex: "^v[0-9]+$"
"#;
    let sr: SafeRegex = serde_yaml::from_str(yaml).expect("parses");
    assert_eq!(sr.regex, "^v[0-9]+$");
    assert!(sr.compiled.is_none(), "compiled set to None pre-validate");
}

#[test]
fn safe_regex_partial_eq_compares_only_regex_string() {
    let a = SafeRegex { regex: "x".into(), compiled: None };
    let b = SafeRegex {
        regex: "x".into(),
        compiled: Some(std::sync::Arc::new(regex::Regex::new("x").unwrap())),
    };
    assert_eq!(a, b, "compiled field is opaque to PartialEq");
}
```

- [ ] **Step 7: Run the tests to verify they fail.**

```bash
cargo test -p envoy-config parses_safe_regex safe_regex_partial_eq_compares_only_regex_string
```

Expected: FAIL with compile error referencing unknown name `SafeRegex`.

- [ ] **Step 8: Add the `SafeRegex` type with hand-rolled Deserialize + PartialEq.**

Append after the `Int64Range` struct:

```rust
/// Reference to a regex pattern. Held both as the original String (for
/// re-serialization / equality / debugging) and the compiled Arc<regex::Regex>
/// (for cheap clone + zero-cost matching). The compiled form is *not* a serde
/// field; it's filled in by the envoy-config validator after deserialization.
/// Phase 04.2 (under ADR-0021).
///
/// PartialEq compares only the `regex: String` field. The compiled regex has
/// no stable equality (regex::Regex doesn't impl PartialEq), and PartialEq is
/// useful for assert_eq! shape comparisons in tests where pre-validate values
/// (compiled == None) and post-validate values (compiled == Some) should be
/// considered equal if they came from the same pattern.
#[derive(Debug, Clone)]
pub struct SafeRegex {
    pub regex: String,
    /// Filled in by the validator (`crate::bootstrap::validate`). At
    /// deserialization time this is None; after a successful validate() call
    /// it's Some(Arc<regex::Regex>). Consumers (the route walker in HCM via
    /// HeaderMatcher::matches) take the .as_ref().expect("validator ensured
    /// compiled") shape, mirroring phase 02.1's "validator ensured cluster
    /// present" precedent.
    pub compiled: Option<std::sync::Arc<regex::Regex>>,
}

impl PartialEq for SafeRegex {
    fn eq(&self, other: &Self) -> bool {
        self.regex == other.regex
    }
}

/// Hand-rolled Deserialize: only reads `regex: String`; sets `compiled: None`.
/// The validator extension (Task 5) fills the compiled form. The hand-rolled
/// shape (rather than `#[derive(Deserialize)] + #[serde(skip)]`) is mandatory
/// because the validator needs to *write* `compiled`, but `serde(skip)` would
/// leave the field absent from any auto-generated value-construction path —
/// and we additionally enforce `deny_unknown_fields` semantics here (reject
/// any key other than `regex`).
impl<'de> serde::Deserialize<'de> for SafeRegex {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{Error, MapAccess, Visitor};
        use std::fmt;

        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = SafeRegex;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a SafeRegex map with a `regex: String` field")
            }
            fn visit_map<M>(self, mut map: M) -> Result<SafeRegex, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut regex: Option<String> = None;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "regex" => {
                            if regex.is_some() {
                                return Err(M::Error::duplicate_field("regex"));
                            }
                            regex = Some(map.next_value::<String>()?);
                        }
                        other => {
                            return Err(M::Error::unknown_field(other, &["regex"]));
                        }
                    }
                }
                let regex = regex.ok_or_else(|| M::Error::missing_field("regex"))?;
                Ok(SafeRegex {
                    regex,
                    compiled: None,
                })
            }
        }
        deserializer.deserialize_map(V)
    }
}
```

- [ ] **Step 9: Run the SafeRegex tests; expect green.**

```bash
cargo test -p envoy-config parses_safe_regex safe_regex_partial_eq_compares_only_regex_string
```

Expected: 2 passed.

- [ ] **Step 10: Run the full crate test to verify no regression.**

```bash
cargo test -p envoy-config --lib
```

Expected: previous 75 + 4 = 79 tests, all passing.

- [ ] **Step 11: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: all three exit 0. The two new types are not yet re-exported from `lib.rs` (Tasks 4 + 5 will batch the re-exports); within the crate they are visible.

- [ ] **Step 12: Append a Task 2 section to PROGRESS.md.**

```markdown
## Task 2 — envoy-config schema: Int64Range + SafeRegex (2026-04-27)

- Commit: <SHA>
- Change: appended Int64Range (i64 half-open range struct, deny_unknown_fields) and SafeRegex (regex: String + non-serde compiled: Option<Arc<regex::Regex>>) types after DirectResponse in bootstrap.rs. SafeRegex carries a hand-rolled `impl<'de> Deserialize<'de>` that reads only `regex: String`, rejects any other key, and sets compiled: None (validator extension in Task 5 will fill compiled). SafeRegex's custom PartialEq compares only the regex String — matches SPEC §6 signpost 17 (regex::Regex has no stable equality).
- Tests added (4): parses_int64_range, rejects_unknown_field_in_int64_range, parses_safe_regex, safe_regex_partial_eq_compares_only_regex_string.
- Verification: `cargo test -p envoy-config --lib` → 79 passed; clippy + fmt + build clean.
- Deviations: <document any>.
```

- [ ] **Step 13: Commit.**

```bash
git add crates/envoy-config/src/bootstrap.rs
git commit -m "phase 04.2: envoy-config — Int64Range + SafeRegex schema + hand-rolled Deserialize (task 2)"
git add docs/envoy-rust/phases/04.2-route-matchers/PROGRESS.md
git commit -m "phase 04.2: progress note (task 2)"
```

---

### Task 3: `envoy-config` schema — `StringMatcher` + `StringMatcherMode` (hand-rolled `Deserialize` for the field-name oneof) + 5 parse-shape tests

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (append `StringMatcher` + `StringMatcherMode` after the `SafeRegex` block from Task 2; add hand-rolled `impl<'de> Deserialize<'de> for StringMatcher`)
- Modify: `crates/envoy-config/src/lib.rs` (add the 5th `ConfigError::UnknownStringMatcherMode { got: String }` variant referenced by the StringMatcher visitor's error path)
- Modify (append): `docs/envoy-rust/phases/04.2-route-matchers/PROGRESS.md`

**Why now:** `StringMatcher` carries the field-name oneof shape (`exact` / `prefix` / `suffix` / `safe_regex` / `contains` discriminator keys) plus the peer field `ignore_case: bool`. Landing it before `HeaderMatcher` in Task 4 keeps the visitor pattern small and self-contained — Task 4's `HeaderMatcher` visitor reuses the same error idioms (key collection + duplicate-mode rejection + missing-mode rejection) and embeds `StringMatcher` as one of the seven mode variants. The `UnknownStringMatcherMode` `ConfigError` variant lands here because it's emitted by the visitor; Task 1 stubbed only the four variants relevant to HeaderMatcher's outer level.

**Scope.** Two new types (`StringMatcher` struct + `StringMatcherMode` enum); one hand-rolled `Deserialize` impl that handles the field-name oneof discrimination; one new `ConfigError` variant; 5 parse-shape unit tests covering the 5 variants + the unknown-mode error path. ~60 LoC visitor + ~20 LoC type defs + ~50 LoC tests.

- [ ] **Step 1: Add the `UnknownStringMatcherMode` variant to `ConfigError`.**

Locate `crates/envoy-config/src/lib.rs`'s `pub enum ConfigError { ... }` block. Append after the `UnknownHeaderMatcherMode` variant (added in Task 1):

```rust
    /// StringMatcher's hand-rolled Deserialize encountered an unrecognized mode
    /// key. Phase 04.2; the five recognized keys are `exact`, `prefix`, `suffix`,
    /// `safe_regex`, `contains`. (`ignore_case` is a peer of the mode key, not a
    /// mode key itself; it does not trip this error.)
    #[error(
        "unknown StringMatcher mode key: {got:?}; expected one of exact, prefix, suffix, safe_regex, contains"
    )]
    UnknownStringMatcherMode { got: String },
```

- [ ] **Step 2: Write 5 failing parse-shape tests for `StringMatcher`.**

Append to `bootstrap.rs::tests`:

```rust
#[test]
fn parses_string_matcher_exact() {
    let yaml = r#"
exact: "foo"
"#;
    let sm: StringMatcher = serde_yaml::from_str(yaml).expect("parses");
    assert_eq!(sm.mode, StringMatcherMode::Exact("foo".into()));
    assert_eq!(sm.ignore_case, false);
}

#[test]
fn parses_string_matcher_contains_with_ignore_case() {
    let yaml = r#"
contains: "beta"
ignore_case: true
"#;
    let sm: StringMatcher = serde_yaml::from_str(yaml).expect("parses");
    assert_eq!(sm.mode, StringMatcherMode::Contains("beta".into()));
    assert_eq!(sm.ignore_case, true);
}

#[test]
fn parses_string_matcher_safe_regex() {
    let yaml = r#"
safe_regex:
  regex: "^v[0-9]+$"
"#;
    let sm: StringMatcher = serde_yaml::from_str(yaml).expect("parses");
    match sm.mode {
        StringMatcherMode::SafeRegex(sr) => {
            assert_eq!(sr.regex, "^v[0-9]+$");
            assert!(sr.compiled.is_none());
        }
        other => panic!("expected SafeRegex variant, got {other:?}"),
    }
}

#[test]
fn rejects_unknown_string_matcher_mode_key() {
    let yaml = r#"
weird: "x"
"#;
    let res: Result<StringMatcher, _> = serde_yaml::from_str(yaml);
    assert!(res.is_err(), "unknown mode key should error");
    let err = res.err().unwrap().to_string();
    assert!(
        err.contains("weird") || err.contains("unknown"),
        "error mentions unknown key: {err}"
    );
}

#[test]
fn rejects_two_string_matcher_mode_keys() {
    let yaml = r#"
exact: "a"
prefix: "b"
"#;
    let res: Result<StringMatcher, _> = serde_yaml::from_str(yaml);
    assert!(
        res.is_err(),
        "two mode keys should be rejected (each variant is mutually exclusive)"
    );
}
```

- [ ] **Step 3: Run the tests to verify they fail.**

```bash
cargo test -p envoy-config parses_string_matcher_exact parses_string_matcher_contains_with_ignore_case parses_string_matcher_safe_regex rejects_unknown_string_matcher_mode_key rejects_two_string_matcher_mode_keys
```

Expected: FAIL with compile errors referencing unknown names `StringMatcher` and `StringMatcherMode`.

- [ ] **Step 4: Add the `StringMatcher` + `StringMatcherMode` types with hand-rolled Deserialize.**

Append after `SafeRegex` in `bootstrap.rs`:

```rust
/// Envoy's modern generic StringMatcher (proto:
/// `envoy.type.matcher.v3.StringMatcher`). Field-name oneof shape: the
/// discriminator is *which* of `exact` / `prefix` / `suffix` / `safe_regex` /
/// `contains` is the present key. `ignore_case` is a peer of the mode key
/// (not a per-variant field) controlling case sensitivity of the value match.
/// Defaults to false. Has no effect on the SafeRegex variant per Envoy proto
/// (regex callers express case insensitivity via the `(?i)` inline flag).
/// Phase 04.2.
#[derive(Debug, Clone, PartialEq)]
pub struct StringMatcher {
    pub mode: StringMatcherMode,
    pub ignore_case: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StringMatcherMode {
    /// `exact: <string>`.
    Exact(String),
    /// `prefix: <string>`.
    Prefix(String),
    /// `suffix: <string>`.
    Suffix(String),
    /// `safe_regex: { regex: "<pattern>" }`.
    SafeRegex(SafeRegex),
    /// `contains: <string>` — substring match. Only reachable through
    /// HeaderMatcherMode::StringMatch(StringMatcher::Contains(...)); there is
    /// no top-level HeaderMatcherMode::ContainsMatch (Envoy v1.33.0 only
    /// supports Contains via the modern string_match field; SPEC §6 signpost 8).
    Contains(String),
}

impl<'de> serde::Deserialize<'de> for StringMatcher {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{Error, MapAccess, Visitor};
        use std::fmt;

        const MODE_KEYS: &[&str] = &["exact", "prefix", "suffix", "safe_regex", "contains"];
        const ALL_KEYS: &[&str] = &["exact", "prefix", "suffix", "safe_regex", "contains", "ignore_case"];

        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = StringMatcher;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a StringMatcher map with exactly one mode key plus optional ignore_case")
            }
            fn visit_map<M>(self, mut map: M) -> Result<StringMatcher, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut mode: Option<StringMatcherMode> = None;
                let mut ignore_case: Option<bool> = None;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "exact" => {
                            if mode.is_some() {
                                return Err(M::Error::custom(
                                    "StringMatcher: multiple mode keys (each variant is mutually exclusive)",
                                ));
                            }
                            mode = Some(StringMatcherMode::Exact(map.next_value::<String>()?));
                        }
                        "prefix" => {
                            if mode.is_some() {
                                return Err(M::Error::custom(
                                    "StringMatcher: multiple mode keys (each variant is mutually exclusive)",
                                ));
                            }
                            mode = Some(StringMatcherMode::Prefix(map.next_value::<String>()?));
                        }
                        "suffix" => {
                            if mode.is_some() {
                                return Err(M::Error::custom(
                                    "StringMatcher: multiple mode keys (each variant is mutually exclusive)",
                                ));
                            }
                            mode = Some(StringMatcherMode::Suffix(map.next_value::<String>()?));
                        }
                        "safe_regex" => {
                            if mode.is_some() {
                                return Err(M::Error::custom(
                                    "StringMatcher: multiple mode keys (each variant is mutually exclusive)",
                                ));
                            }
                            mode = Some(StringMatcherMode::SafeRegex(map.next_value::<SafeRegex>()?));
                        }
                        "contains" => {
                            if mode.is_some() {
                                return Err(M::Error::custom(
                                    "StringMatcher: multiple mode keys (each variant is mutually exclusive)",
                                ));
                            }
                            mode = Some(StringMatcherMode::Contains(map.next_value::<String>()?));
                        }
                        "ignore_case" => {
                            if ignore_case.is_some() {
                                return Err(M::Error::duplicate_field("ignore_case"));
                            }
                            ignore_case = Some(map.next_value::<bool>()?);
                        }
                        other => {
                            return Err(M::Error::unknown_field(other, ALL_KEYS));
                        }
                    }
                }
                let mode = mode.ok_or_else(|| {
                    M::Error::custom(format!(
                        "StringMatcher: missing mode key (expected one of {MODE_KEYS:?})"
                    ))
                })?;
                Ok(StringMatcher {
                    mode,
                    ignore_case: ignore_case.unwrap_or(false),
                })
            }
        }
        deserializer.deserialize_map(V)
    }
}
```

- [ ] **Step 5: Run the StringMatcher tests; expect green.**

```bash
cargo test -p envoy-config parses_string_matcher_exact parses_string_matcher_contains_with_ignore_case parses_string_matcher_safe_regex rejects_unknown_string_matcher_mode_key rejects_two_string_matcher_mode_keys
```

Expected: 5 passed.

- [ ] **Step 6: Run the full crate test.**

```bash
cargo test -p envoy-config --lib
```

Expected: 79 + 5 = 84 passed.

- [ ] **Step 7: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: all three exit 0. Note: the `UnknownStringMatcherMode` variant added in Step 1 may not yet have a usage site in code (the visitor uses `M::Error::custom` / `unknown_field` rather than constructing the typed `ConfigError` variant directly — that's fine, the variant is reserved for future paths that need typed propagation). `pub`-visible enum variants are exempt from `dead_code`.

- [ ] **Step 8: Append a Task 3 section to PROGRESS.md.**

```markdown
## Task 3 — envoy-config schema: StringMatcher + StringMatcherMode (2026-04-27)

- Commit: <SHA>
- Change: appended StringMatcher (mode + ignore_case) and StringMatcherMode (5 variants: Exact, Prefix, Suffix, SafeRegex, Contains) types after SafeRegex in bootstrap.rs. StringMatcher carries a hand-rolled `impl<'de> Deserialize<'de>` for the field-name oneof: collects all keys; allows at most one mode key; accepts ignore_case as a peer (default false); rejects unknown keys via M::Error::unknown_field. Added the 5th ConfigError variant UnknownStringMatcherMode in lib.rs (sibling of the 4 added in Task 1).
- Tests added (5): parses_string_matcher_exact, parses_string_matcher_contains_with_ignore_case, parses_string_matcher_safe_regex, rejects_unknown_string_matcher_mode_key, rejects_two_string_matcher_mode_keys.
- Verification: `cargo test -p envoy-config --lib` → 84 passed; clippy + fmt + build clean.
- Deviations: <document any>.
```

- [ ] **Step 9: Commit.**

```bash
git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs
git commit -m "phase 04.2: envoy-config — StringMatcher + hand-rolled Deserialize (task 3)"
git add docs/envoy-rust/phases/04.2-route-matchers/PROGRESS.md
git commit -m "phase 04.2: progress note (task 3)"
```

---

### Task 4: `envoy-config` schema — `HeaderMatcher` + `HeaderMatcherMode` (hand-rolled `Deserialize` for the field-name oneof) + `RouteMatch.headers` extension + 6 parse-shape tests

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (append `HeaderMatcher` + `HeaderMatcherMode` after the `StringMatcher` block from Task 3; add hand-rolled `impl<'de> Deserialize<'de> for HeaderMatcher`; add `headers: Vec<HeaderMatcher>` field with `#[serde(default)]` to the existing `RouteMatch` struct at line 318)
- Modify: `crates/envoy-config/src/lib.rs` (extend `pub use bootstrap::{...}` re-export list with `HeaderMatcher`, `HeaderMatcherMode`, `SafeRegex`, `Int64Range`, `StringMatcher`, `StringMatcherMode`)
- Modify (append): `docs/envoy-rust/phases/04.2-route-matchers/PROGRESS.md`

**Why now:** Task 4 closes the schema layer. After this task, every YAML shape in SPEC §3 D1's worked example deserializes cleanly; Task 5 wires the validator pass; Task 6 + 7 wire the runtime.

**Scope.** Two new types (`HeaderMatcher` struct + `HeaderMatcherMode` enum with 7 variants); one hand-rolled `Deserialize` impl for the field-name oneof; one additive field on `RouteMatch`; 6 public-symbol re-exports in `lib.rs`; 6 parse-shape tests covering 5 of the 7 modes + the multi-matcher Vec round-trip + invert_match default. ~80 LoC visitor + ~30 LoC type defs + ~80 LoC tests.

- [ ] **Step 1: Read the current `RouteMatch` shape.**

```bash
sed -n '316,323p' crates/envoy-config/src/bootstrap.rs
```

Expected: 8 lines showing `pub struct RouteMatch { #[serde(default)] pub prefix: Option<String>, #[serde(default)] pub path: Option<String>, }` and the surrounding `#[derive(Debug, Deserialize, PartialEq)] #[serde(deny_unknown_fields)]` attributes. The Task 4 edit adds a third `headers: Vec<HeaderMatcher>` field with `#[serde(default)]`.

- [ ] **Step 2: Write 6 failing parse-shape tests for `HeaderMatcher` + `RouteMatch.headers` round-trip.**

Append to `bootstrap.rs::tests`:

```rust
#[test]
fn parses_header_matcher_exact() {
    let yaml = r#"
name: "x-foo"
exact_match: "bar"
"#;
    let m: HeaderMatcher = serde_yaml::from_str(yaml).expect("parses");
    assert_eq!(m.name, "x-foo");
    assert_eq!(m.mode, HeaderMatcherMode::ExactMatch("bar".into()));
    assert_eq!(m.invert_match, false);
}

#[test]
fn parses_header_matcher_with_invert_match_true() {
    let yaml = r#"
name: "x-foo"
exact_match: "bar"
invert_match: true
"#;
    let m: HeaderMatcher = serde_yaml::from_str(yaml).expect("parses");
    assert_eq!(m.invert_match, true);
}

#[test]
fn parses_header_matcher_present_match_true() {
    let yaml = r#"
name: "authorization"
present_match: true
"#;
    let m: HeaderMatcher = serde_yaml::from_str(yaml).expect("parses");
    assert_eq!(m.mode, HeaderMatcherMode::PresentMatch(true));
}

#[test]
fn parses_header_matcher_string_match_contains() {
    let yaml = r#"
name: "x-tag"
string_match:
  contains: "beta"
  ignore_case: true
"#;
    let m: HeaderMatcher = serde_yaml::from_str(yaml).expect("parses");
    match m.mode {
        HeaderMatcherMode::StringMatch(sm) => {
            assert_eq!(sm.mode, StringMatcherMode::Contains("beta".into()));
            assert_eq!(sm.ignore_case, true);
        }
        other => panic!("expected StringMatch variant, got {other:?}"),
    }
}

#[test]
fn rejects_unknown_header_matcher_mode_key() {
    let yaml = r#"
name: "x-foo"
weird_match: "bar"
"#;
    let res: Result<HeaderMatcher, _> = serde_yaml::from_str(yaml);
    assert!(res.is_err(), "unknown mode key should error");
    let err = res.err().unwrap().to_string();
    assert!(
        err.contains("weird_match") || err.contains("unknown"),
        "error mentions unknown key: {err}"
    );
}

#[test]
fn parses_route_match_with_headers_vec_and_invert_match_default() {
    let yaml = r#"
prefix: "/api/"
headers:
  - name: "x-foo"
    exact_match: "bar"
  - name: "x-version"
    range_match: { start: 1, end: 100 }
"#;
    let rm: RouteMatch = serde_yaml::from_str(yaml).expect("parses");
    assert_eq!(rm.prefix.as_deref(), Some("/api/"));
    assert_eq!(rm.headers.len(), 2);
    assert_eq!(rm.headers[0].name, "x-foo");
    assert_eq!(rm.headers[0].invert_match, false);
    assert_eq!(rm.headers[0].mode, HeaderMatcherMode::ExactMatch("bar".into()));
    assert_eq!(rm.headers[1].name, "x-version");
    assert_eq!(
        rm.headers[1].mode,
        HeaderMatcherMode::RangeMatch(Int64Range { start: 1, end: 100 })
    );
}
```

- [ ] **Step 3: Run the tests to verify they fail.**

```bash
cargo test -p envoy-config parses_header_matcher_exact parses_header_matcher_with_invert_match_true parses_header_matcher_present_match_true parses_header_matcher_string_match_contains rejects_unknown_header_matcher_mode_key parses_route_match_with_headers_vec_and_invert_match_default
```

Expected: FAIL with compile errors referencing unknown names `HeaderMatcher`, `HeaderMatcherMode`, and the new `RouteMatch.headers` field.

- [ ] **Step 4: Add the `HeaderMatcher` + `HeaderMatcherMode` types with hand-rolled Deserialize.**

Append after `StringMatcher` in `bootstrap.rs`:

```rust
/// One header-matching predicate. AND-combined with sibling HeaderMatchers
/// in `RouteMatch.headers` per Envoy v1.33.0 default `headers_match_options:
/// ALL`. Phase 04.2.
#[derive(Debug, Clone, PartialEq)]
pub struct HeaderMatcher {
    /// Header name. Matched case-insensitively against the request's header
    /// names per HTTP/1.1 RFC 7230 §3.2. Empty string is rejected by the
    /// validator with ConfigError::EmptyHeaderName.
    pub name: String,
    /// The mode discriminator. The Envoy proto uses field-name oneof shape
    /// (the discriminator is *which* of the seven mode fields is present);
    /// serde tagged-enum doesn't directly model this, so the parsed form goes
    /// through a hand-rolled Deserialize impl that inspects the YAML mapping
    /// keys and dispatches to the matching variant. SPEC §6 signpost 1.
    pub mode: HeaderMatcherMode,
    /// If true, the entire mode-specific match result is inverted (XOR after
    /// the mode match runs, before AND-combination across sibling
    /// HeaderMatchers). SPEC §6 signpost 5.
    pub invert_match: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HeaderMatcherMode {
    /// `exact_match: <string>` — value equals literal (case-sensitive on the
    /// value; the header name match is always case-insensitive per HTTP/1.1).
    ExactMatch(String),
    /// `prefix_match: <string>` — value starts with literal.
    PrefixMatch(String),
    /// `suffix_match: <string>` — value ends with literal.
    SuffixMatch(String),
    /// `safe_regex_match: { regex: "<pattern>" }` — value matches the regex.
    /// Compiled at config-load time into Arc<regex::Regex>; the validator
    /// rejects unparseable patterns with ConfigError::InvalidRegex.
    SafeRegexMatch(SafeRegex),
    /// `range_match: { start: <i64>, end: <i64> }` — value parses as i64
    /// (decimal) and falls in [start, end). Non-parseable values fail the
    /// match (NOT an error). SPEC §6 signpost 6.
    RangeMatch(Int64Range),
    /// `present_match: <bool>` — header presence (true) or "no presence
    /// requirement" (false; SPEC §6 signpost 7 for the subtle false semantics).
    PresentMatch(bool),
    /// `string_match: <StringMatcher>` — Envoy's modern generic tagged-union
    /// (the only path to Contains; SPEC §6 signpost 8).
    StringMatch(StringMatcher),
}

impl<'de> serde::Deserialize<'de> for HeaderMatcher {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{Error, MapAccess, Visitor};
        use std::fmt;

        const ALL_KEYS: &[&str] = &[
            "name",
            "exact_match",
            "prefix_match",
            "suffix_match",
            "safe_regex_match",
            "range_match",
            "present_match",
            "string_match",
            "invert_match",
        ];

        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = HeaderMatcher;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(
                    "a HeaderMatcher map with `name`, exactly one mode key, and optional invert_match",
                )
            }
            fn visit_map<M>(self, mut map: M) -> Result<HeaderMatcher, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut name: Option<String> = None;
                let mut mode: Option<HeaderMatcherMode> = None;
                let mut invert_match: Option<bool> = None;

                fn set_mode<E: Error>(
                    slot: &mut Option<HeaderMatcherMode>,
                    new: HeaderMatcherMode,
                ) -> Result<(), E> {
                    if slot.is_some() {
                        return Err(E::custom(
                            "HeaderMatcher: multiple mode keys (each variant is mutually exclusive)",
                        ));
                    }
                    *slot = Some(new);
                    Ok(())
                }

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "name" => {
                            if name.is_some() {
                                return Err(M::Error::duplicate_field("name"));
                            }
                            name = Some(map.next_value::<String>()?);
                        }
                        "exact_match" => set_mode(
                            &mut mode,
                            HeaderMatcherMode::ExactMatch(map.next_value::<String>()?),
                        )?,
                        "prefix_match" => set_mode(
                            &mut mode,
                            HeaderMatcherMode::PrefixMatch(map.next_value::<String>()?),
                        )?,
                        "suffix_match" => set_mode(
                            &mut mode,
                            HeaderMatcherMode::SuffixMatch(map.next_value::<String>()?),
                        )?,
                        "safe_regex_match" => set_mode(
                            &mut mode,
                            HeaderMatcherMode::SafeRegexMatch(map.next_value::<SafeRegex>()?),
                        )?,
                        "range_match" => set_mode(
                            &mut mode,
                            HeaderMatcherMode::RangeMatch(map.next_value::<Int64Range>()?),
                        )?,
                        "present_match" => set_mode(
                            &mut mode,
                            HeaderMatcherMode::PresentMatch(map.next_value::<bool>()?),
                        )?,
                        "string_match" => set_mode(
                            &mut mode,
                            HeaderMatcherMode::StringMatch(map.next_value::<StringMatcher>()?),
                        )?,
                        "invert_match" => {
                            if invert_match.is_some() {
                                return Err(M::Error::duplicate_field("invert_match"));
                            }
                            invert_match = Some(map.next_value::<bool>()?);
                        }
                        other => {
                            return Err(M::Error::unknown_field(other, ALL_KEYS));
                        }
                    }
                }

                let name = name.ok_or_else(|| M::Error::missing_field("name"))?;
                let mode = mode.ok_or_else(|| {
                    M::Error::custom(
                        "HeaderMatcher: missing mode key (expected one of exact_match, prefix_match, suffix_match, safe_regex_match, range_match, present_match, string_match)",
                    )
                })?;
                Ok(HeaderMatcher {
                    name,
                    mode,
                    invert_match: invert_match.unwrap_or(false),
                })
            }
        }
        deserializer.deserialize_map(V)
    }
}
```

- [ ] **Step 5: Add `headers: Vec<HeaderMatcher>` to `RouteMatch`.**

Locate `RouteMatch` (line 316–323). Edit to:

```rust
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RouteMatch {
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub headers: Vec<HeaderMatcher>,
}
```

Note the additional `Clone` derive — the existing `RouteMatch` doesn't derive Clone but `HeaderMatcher` does, and the matcher-runtime tests in Task 6 want to compose `RouteMatch` values directly. Adding `Clone` is forward-compat; if `Clone` introduces issues with downstream callsites, drop it from `RouteMatch` and the tests construct `RouteMatch` values fresh per test (no clone needed at runtime since `clone_route_config` in `hcm.rs` already hand-clones).

- [ ] **Step 6: Re-export the new public types from `crates/envoy-config/src/lib.rs`.**

Locate the existing `pub use bootstrap::{...}` block (lines 9–17). Append the 6 new names to the alphabetic list:

```rust
pub use bootstrap::{
    Address, Admin, Bootstrap, CertificateValidationContext, Cluster, ClusterType, CodecType,
    CommonTlsContext, DataSource, DirectResponse, DownstreamTlsContext, Endpoint, FilterChain,
    FilterChainMatch, HeaderMatcher, HeaderMatcherMode, HttpConnectionManagerConfig, HttpFilter,
    HttpFilterTypedConfig, Int64Range, LbEndpoint, LbPolicy, Listener, LoadAssignment,
    LocalityLbEndpoints, NetworkFilter, Node, Route, RouteConfiguration, RouteMatch, RouterConfig,
    SafeRegex, SocketAddress, StaticResources, StringMatcher, StringMatcherMode, TcpProxyConfig,
    TlsCertificate, TransportSocket, TransportSocketTypedConfig, TypedConfig, UpstreamTlsContext,
    VirtualHost,
};
```

(The actual sort may differ slightly per cargo fmt's stable output; rustfmt will normalize.)

- [ ] **Step 7: Run the HeaderMatcher tests; expect green.**

```bash
cargo test -p envoy-config parses_header_matcher_exact parses_header_matcher_with_invert_match_true parses_header_matcher_present_match_true parses_header_matcher_string_match_contains rejects_unknown_header_matcher_mode_key parses_route_match_with_headers_vec_and_invert_match_default
```

Expected: 6 passed.

- [ ] **Step 8: Run the full crate test.**

```bash
cargo test -p envoy-config --lib
```

Expected: 84 + 6 = 90 passed.

- [ ] **Step 9: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: all three exit 0. Adding `Clone` derive on `RouteMatch` may surface issues if downstream code (notably `crates/envoy-http1/src/hcm.rs::clone_route_config`) was relying on the absence of `Clone` (e.g. using a hand-clone pattern that is now redundant). The hand-clone in `hcm.rs` walks all fields explicitly; the new `headers` field needs to be added to the clone walk in **Task 7** — for Task 4, the hand-clone helper still works because the new `headers` field has a default, but if `cargo build` surfaces "missing field `headers`" in the hand-clone constructor, fix in Task 4 by adding `headers: rm.headers.clone()` to the `RouteMatch { ... }` literal at `hcm.rs:62-64`. Simpler path: if `Clone` is derivable, the hand-clone helper retires entirely and Task 7 just deletes it. Document either approach in PROGRESS.md.

- [ ] **Step 10: Append a Task 4 section to PROGRESS.md.**

```markdown
## Task 4 — envoy-config schema: HeaderMatcher + RouteMatch.headers (2026-04-27)

- Commit: <SHA>
- Change: appended HeaderMatcher (name + mode + invert_match) and HeaderMatcherMode (7 variants: ExactMatch, PrefixMatch, SuffixMatch, SafeRegexMatch, RangeMatch, PresentMatch, StringMatch) types after StringMatcher in bootstrap.rs. HeaderMatcher carries a hand-rolled `impl<'de> Deserialize<'de>` for the field-name oneof: collects all keys; validates exactly one mode key is present; accepts invert_match as a peer (default false); rejects unknown keys via M::Error::unknown_field. Added `headers: Vec<HeaderMatcher>` field with #[serde(default)] to RouteMatch (pre-existing prefix + path fields unchanged); added Clone derive on RouteMatch for forward-compat with matcher-runtime test ergonomics. Re-exported HeaderMatcher, HeaderMatcherMode, SafeRegex, Int64Range, StringMatcher, StringMatcherMode from lib.rs's pub use bootstrap{...} list.
- Tests added (6): parses_header_matcher_exact, parses_header_matcher_with_invert_match_true, parses_header_matcher_present_match_true, parses_header_matcher_string_match_contains, rejects_unknown_header_matcher_mode_key, parses_route_match_with_headers_vec_and_invert_match_default.
- Verification: `cargo test -p envoy-config --lib` → 90 passed; clippy + fmt + build clean.
- Deviations: <document any — note especially whether RouteMatch's new Clone derive caused any downstream changes>.
```

- [ ] **Step 11: Commit.**

```bash
git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs
git commit -m "phase 04.2: envoy-config — HeaderMatcher + RouteMatch.headers + hand-rolled Deserialize (task 4)"
git add docs/envoy-rust/phases/04.2-route-matchers/PROGRESS.md
git commit -m "phase 04.2: progress note (task 4)"
```

---

### Task 5: `envoy-config` validator — regex compile pass + Int64Range bounds + EmptyHeaderName + UnknownHeaderMatcherMode + ~10 validator tests

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (extend `validate_hcm` with a matcher-walk that visits each `Route`'s `r#match.headers` Vec, compiles regex patterns into `Arc<regex::Regex>`, validates Int64Range bounds, rejects empty header names; ~10 validator unit tests)
- Modify (append): `docs/envoy-rust/phases/04.2-route-matchers/PROGRESS.md`

**Why now:** Task 4 landed the schema types but `validate_hcm` does not yet walk them. Without this, envoy-bin would consume malformed-but-deserializable configs (unparseable regex patterns, empty header names, inverted Int64Ranges). Task 5 closes the gap so Tasks 6 + 7 can rely on validator-already-rejected guarantees (notably: `safe_regex.compiled.as_ref().expect("validator ensured compiled")` in `HeaderMatcher::matches`).

**Scope.** Add a recursive matcher-walk inside `validate_hcm`'s existing route-iteration loop (after the existing `direct_response.status` + `direct_response.body` checks at lines 603–614). The walk visits each `HeaderMatcher` in `r.r#match.headers`, dispatches on the mode, and applies per-mode validation:

- `ExactMatch | PrefixMatch | SuffixMatch | PresentMatch` → no further validation beyond the always-required `EmptyHeaderName` check.
- `SafeRegexMatch(safe_regex)` → call `regex::Regex::new(&safe_regex.regex)`; on `Ok(re)` mutate `safe_regex.compiled = Some(Arc::new(re))`; on `Err(e)` return `ConfigError::InvalidRegex { regex: safe_regex.regex.clone(), source: e }`.
- `RangeMatch(r)` → `if r.start >= r.end` return `ConfigError::InvalidInt64Range { start, end }`.
- `StringMatch(sm)` → walk `sm.mode`; if `StringMatcherMode::SafeRegex(safe_regex)`, same compile-pass as above (recurse helper).

The `safe_regex.compiled` mutation requires the walk to take `&mut HttpConnectionManagerConfig` (or specifically `&mut HeaderMatcher`). The existing `validate_hcm` takes `&HttpConnectionManagerConfig` (line 541); extending the signature to `&mut` requires changes to the caller chain. Two design choices:

1. **Mutate at validate time** — change `validate_hcm`'s signature to `&mut`, and change the outer `validate(bootstrap: &Bootstrap)` similarly. Implications: every test calling `validate(&bs)` needs to switch to `validate(&mut bs)`; the public `parse_bootstrap` function (lib.rs line 130) already takes `bootstrap: Bootstrap` (owned) so its body becomes `bootstrap::validate(&mut bootstrap)?` — fine.
2. **Validate twice** — pre-compile pass (mut) followed by validate-only pass (immut). More moving parts; rejected.

**Decision: option 1** (single-pass mutating validator). The `parse_bootstrap` entrypoint already moves the bootstrap into and out of itself; `validate(&mut bootstrap)` is a 1-character public-API change that doesn't ripple beyond `lib.rs::parse_bootstrap`. The non-test internal callsite in `crates/envoy-bin/src/main.rs` calls `parse_bootstrap`, not `validate` directly, so envoy-bin is unaffected.

- [ ] **Step 1: Cross-check the existing `validate_hcm` + `parse_bootstrap` callsites.**

```bash
grep -rn 'bootstrap::validate\|validate(&\|validate(&mut' crates/envoy-config/src/ crates/envoy-bin/src/
```

Expected: `parse_bootstrap` in `crates/envoy-config/src/lib.rs:130-134` is the sole non-test caller; tests in `bootstrap.rs::tests` call `validate` directly. The Step 4 signature change ripples only into these.

- [ ] **Step 2: Write 10 failing validator tests.**

Append to `bootstrap.rs::tests`. Reuse the existing `parse_then_validate` helper (added in Task 2 of phase 04.1, available at the established `tests::` mod scope) and the `make_hcm_listener_yaml` helper. The tests need a HeaderMatcher inside the existing HCM route shape:

```rust
#[test]
fn rejects_empty_header_name() {
    // RouteMatch.headers carrying a HeaderMatcher with name = "" → EmptyHeaderName.
    let yaml = make_hcm_listener_yaml(
        r#"
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match:
                            prefix: "/"
                            headers:
                              - name: ""
                                exact_match: "bar"
                          direct_response:
                            status: 200
                            body: { inline_string: "ok" }
"#,
    );
    let err = parse_then_validate(&yaml).err().expect("validator rejects");
    assert!(
        matches!(err, crate::ConfigError::EmptyHeaderName),
        "expected EmptyHeaderName, got {err:?}"
    );
}

#[test]
fn rejects_invalid_regex_in_safe_regex_match() {
    let yaml = make_hcm_listener_yaml(
        r#"
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match:
                            prefix: "/"
                            headers:
                              - name: "x-foo"
                                safe_regex_match:
                                  regex: "[unclosed"
                          direct_response:
                            status: 200
                            body: { inline_string: "ok" }
"#,
    );
    let err = parse_then_validate(&yaml).err().expect("validator rejects");
    assert!(
        matches!(err, crate::ConfigError::InvalidRegex { .. }),
        "expected InvalidRegex, got {err:?}"
    );
}

#[test]
fn rejects_invalid_regex_in_string_match_safe_regex() {
    let yaml = make_hcm_listener_yaml(
        r#"
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match:
                            prefix: "/"
                            headers:
                              - name: "x-foo"
                                string_match:
                                  safe_regex:
                                    regex: "(?P<oops"
                          direct_response:
                            status: 200
                            body: { inline_string: "ok" }
"#,
    );
    let err = parse_then_validate(&yaml).err().expect("validator rejects");
    assert!(
        matches!(err, crate::ConfigError::InvalidRegex { .. }),
        "expected InvalidRegex, got {err:?}"
    );
}

#[test]
fn rejects_invalid_int64_range_start_eq_end() {
    let yaml = make_hcm_listener_yaml(
        r#"
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match:
                            prefix: "/"
                            headers:
                              - name: "x-version"
                                range_match: { start: 100, end: 100 }
                          direct_response:
                            status: 200
                            body: { inline_string: "ok" }
"#,
    );
    let err = parse_then_validate(&yaml).err().expect("validator rejects");
    assert!(
        matches!(
            err,
            crate::ConfigError::InvalidInt64Range { start: 100, end: 100 }
        ),
        "expected InvalidInt64Range {{100,100}}, got {err:?}"
    );
}

#[test]
fn rejects_invalid_int64_range_start_gt_end() {
    let yaml = make_hcm_listener_yaml(
        r#"
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match:
                            prefix: "/"
                            headers:
                              - name: "x-version"
                                range_match: { start: 200, end: 100 }
                          direct_response:
                            status: 200
                            body: { inline_string: "ok" }
"#,
    );
    let err = parse_then_validate(&yaml).err().expect("validator rejects");
    assert!(matches!(err, crate::ConfigError::InvalidInt64Range { .. }));
}

#[test]
fn validator_compiles_safe_regex_match_into_arc() {
    let yaml = make_hcm_listener_yaml(
        r#"
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match:
                            prefix: "/"
                            headers:
                              - name: "x-version"
                                safe_regex_match:
                                  regex: "^v[0-9]+$"
                          direct_response:
                            status: 200
                            body: { inline_string: "ok" }
"#,
    );
    let bs = crate::parse_bootstrap(&yaml).expect("parses + validates");
    let listener = &bs.static_resources.listeners[0];
    let TypedConfig::HttpConnectionManager(hcm) = listener.filter_chains[0].filters[0]
        .typed_config
        .as_ref()
        .unwrap()
    else {
        panic!("not HCM");
    };
    let header_matcher = &hcm.route_config.virtual_hosts[0].routes[0].r#match.headers[0];
    let HeaderMatcherMode::SafeRegexMatch(sr) = &header_matcher.mode else {
        panic!("not SafeRegexMatch");
    };
    assert!(
        sr.compiled.is_some(),
        "validator should have compiled the regex"
    );
}

#[test]
fn validator_accepts_all_seven_modes() {
    let yaml = make_hcm_listener_yaml(
        r#"
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match:
                            prefix: "/"
                            headers:
                              - { name: "h1", exact_match: "x" }
                              - { name: "h2", prefix_match: "p" }
                              - { name: "h3", suffix_match: "s" }
                              - { name: "h4", safe_regex_match: { regex: "^v[0-9]+$" } }
                              - { name: "h5", range_match: { start: 1, end: 100 } }
                              - { name: "h6", present_match: true }
                              - { name: "h7", string_match: { contains: "c", ignore_case: true } }
                          direct_response:
                            status: 200
                            body: { inline_string: "ok" }
"#,
    );
    let bs = crate::parse_bootstrap(&yaml).expect("parses + validates");
    let listener = &bs.static_resources.listeners[0];
    let TypedConfig::HttpConnectionManager(hcm) = listener.filter_chains[0].filters[0]
        .typed_config
        .as_ref()
        .unwrap()
    else {
        panic!("not HCM");
    };
    assert_eq!(hcm.route_config.virtual_hosts[0].routes[0].r#match.headers.len(), 7);
}

#[test]
fn validator_accepts_empty_headers_vec() {
    // `headers: []` (or absent — equivalent via #[serde(default)]) means
    // "no header constraints"; validator passes.
    let yaml = make_hcm_listener_yaml(
        r#"
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response:
                            status: 200
                            body: { inline_string: "ok" }
"#,
    );
    crate::parse_bootstrap(&yaml).expect("parses + validates");
}

#[test]
fn validator_accepts_invert_match_true() {
    let yaml = make_hcm_listener_yaml(
        r#"
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match:
                            prefix: "/"
                            headers:
                              - name: "x-foo"
                                exact_match: "bar"
                                invert_match: true
                          direct_response:
                            status: 200
                            body: { inline_string: "ok" }
"#,
    );
    crate::parse_bootstrap(&yaml).expect("parses + validates");
}

#[test]
fn validator_compiles_string_match_safe_regex_into_arc() {
    let yaml = make_hcm_listener_yaml(
        r#"
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match:
                            prefix: "/"
                            headers:
                              - name: "x-tag"
                                string_match:
                                  safe_regex:
                                    regex: "^beta$"
                          direct_response:
                            status: 200
                            body: { inline_string: "ok" }
"#,
    );
    let bs = crate::parse_bootstrap(&yaml).expect("parses + validates");
    let listener = &bs.static_resources.listeners[0];
    let TypedConfig::HttpConnectionManager(hcm) = listener.filter_chains[0].filters[0]
        .typed_config
        .as_ref()
        .unwrap()
    else {
        panic!("not HCM");
    };
    let header_matcher = &hcm.route_config.virtual_hosts[0].routes[0].r#match.headers[0];
    let HeaderMatcherMode::StringMatch(sm) = &header_matcher.mode else {
        panic!("not StringMatch");
    };
    let StringMatcherMode::SafeRegex(sr) = &sm.mode else {
        panic!("not SafeRegex");
    };
    assert!(
        sr.compiled.is_some(),
        "validator should have compiled the nested regex"
    );
}
```

- [ ] **Step 3: Run the tests to verify they fail.**

```bash
cargo test -p envoy-config rejects_empty_header_name rejects_invalid_regex_in_safe_regex_match rejects_invalid_regex_in_string_match_safe_regex rejects_invalid_int64_range_start_eq_end rejects_invalid_int64_range_start_gt_end validator_compiles_safe_regex_match_into_arc validator_accepts_all_seven_modes validator_accepts_empty_headers_vec validator_accepts_invert_match_true validator_compiles_string_match_safe_regex_into_arc
```

Expected: FAIL — the validator currently doesn't walk `route.r#match.headers` at all, so all the rejection-path tests pass parse_bootstrap silently and assert on the absent error; the `compiled.is_some()` tests fail with `None` post-validate.

- [ ] **Step 4: Switch `validate` + `validate_hcm` to `&mut` signatures.**

Locate `crates/envoy-config/src/bootstrap.rs::validate` (line 332). Change:

```rust
pub(crate) fn validate(bootstrap: &Bootstrap) -> Result<(), crate::ConfigError> {
```

to:

```rust
pub(crate) fn validate(bootstrap: &mut Bootstrap) -> Result<(), crate::ConfigError> {
```

The existing dispatch loop is at `bootstrap.rs:433-477` (verified at PLAN-write time). The structure is:

```rust
for filter in &chain.filters {
    match filter.name.as_str() {
        crate::ECHO_FILTER => { ... }
        crate::TCP_PROXY_FILTER => { ... }
        crate::HCM_FILTER => {
            let typed = filter.typed_config.as_ref().ok_or(...)?;
            let TypedConfig::HttpConnectionManager(hcm) = typed else { return Err(...); };
            validate_hcm(hcm)?;
        }
        _ => { return Err(UnsupportedFilter(...)); }
    }
}
```

The minimal-ripple edit:

1. Walk up the borrow chain. The outer loops are `for listener in &bootstrap.static_resources.listeners { for chain in &listener.filter_chains { ... } }`. Switch each to `&mut`:
   - `for listener in &mut bootstrap.static_resources.listeners`
   - `for chain in &mut listener.filter_chains`
   - `for filter in &mut chain.filters`

2. Other arms (ECHO_FILTER, TCP_PROXY_FILTER, catch-all) keep their existing logic. `&mut` reborrows-as-`&` are implicit in Rust, so the immutable arms compile unchanged.

3. The HCM arm changes its `as_ref()` to `as_mut()`:
   ```rust
   crate::HCM_FILTER => {
       let typed = filter.typed_config.as_mut().ok_or(
           crate::ConfigError::MissingTypedConfig(crate::HCM_FILTER),
       )?;
       let TypedConfig::HttpConnectionManager(hcm) = typed else {
           return Err(crate::ConfigError::MissingTypedConfig(crate::HCM_FILTER));
       };
       validate_hcm(hcm)?;
   }
   ```

The TCP_PROXY arm's `let typed = filter.typed_config.as_ref().ok_or(...)?` continues to compile under the `&mut filter` outer borrow because Rust allows immutable reborrows from mutable references.

Locate `validate_hcm` at line 541. Change:

```rust
fn validate_hcm(hcm: &HttpConnectionManagerConfig) -> Result<(), crate::ConfigError> {
```

to:

```rust
fn validate_hcm(hcm: &mut HttpConnectionManagerConfig) -> Result<(), crate::ConfigError> {
```

Inside `validate_hcm`, change the route-iteration loop's borrow form. The current loop walks `&hcm.route_config.virtual_hosts`; change to `&mut hcm.route_config.virtual_hosts` (and propagate `&mut vh`, `&mut r`):

```rust
for vh in &mut hcm.route_config.virtual_hosts {
    // ... existing checks (domains.is_empty(), domains validity, routes.is_empty()) ...
    for r in &mut vh.routes {
        // ... existing match-cases (prefix/path oneof + status range + body datasource) ...

        // 04.2 NEW: walk the headers Vec.
        for hm in &mut r.r#match.headers {
            validate_header_matcher(hm)?;
        }
    }
}
```

- [ ] **Step 5: Add the `validate_header_matcher` helper.**

After the existing `is_valid_dns_name` (line 689):

```rust
/// Validate a single HeaderMatcher and, for SafeRegex modes (top-level or
/// nested via StringMatcher), compile the regex pattern into Arc<regex::Regex>
/// stored back on the SafeRegex.compiled field. Phase 04.2 (under ADR-0021).
fn validate_header_matcher(hm: &mut HeaderMatcher) -> Result<(), crate::ConfigError> {
    if hm.name.is_empty() {
        return Err(crate::ConfigError::EmptyHeaderName);
    }
    match &mut hm.mode {
        HeaderMatcherMode::ExactMatch(_)
        | HeaderMatcherMode::PrefixMatch(_)
        | HeaderMatcherMode::SuffixMatch(_)
        | HeaderMatcherMode::PresentMatch(_) => {}
        HeaderMatcherMode::SafeRegexMatch(sr) => compile_safe_regex(sr)?,
        HeaderMatcherMode::RangeMatch(r) => {
            if r.start >= r.end {
                return Err(crate::ConfigError::InvalidInt64Range {
                    start: r.start,
                    end: r.end,
                });
            }
        }
        HeaderMatcherMode::StringMatch(sm) => match &mut sm.mode {
            StringMatcherMode::Exact(_)
            | StringMatcherMode::Prefix(_)
            | StringMatcherMode::Suffix(_)
            | StringMatcherMode::Contains(_) => {}
            StringMatcherMode::SafeRegex(sr) => compile_safe_regex(sr)?,
        },
    }
    Ok(())
}

fn compile_safe_regex(sr: &mut SafeRegex) -> Result<(), crate::ConfigError> {
    match regex::Regex::new(&sr.regex) {
        Ok(re) => {
            sr.compiled = Some(std::sync::Arc::new(re));
            Ok(())
        }
        Err(e) => Err(crate::ConfigError::InvalidRegex {
            regex: sr.regex.clone(),
            source: e,
        }),
    }
}
```

- [ ] **Step 6: Update `parse_bootstrap` in `crates/envoy-config/src/lib.rs` to take `&mut` to `validate`.**

Locate the function (lines 130-134):

```rust
pub fn parse_bootstrap(yaml: &str) -> Result<Bootstrap, ConfigError> {
    let bootstrap: Bootstrap = serde_yaml::from_str(yaml)?;
    bootstrap::validate(&bootstrap)?;
    Ok(bootstrap)
}
```

Change `let bootstrap` to `let mut bootstrap` and `validate(&bootstrap)` to `validate(&mut bootstrap)`:

```rust
pub fn parse_bootstrap(yaml: &str) -> Result<Bootstrap, ConfigError> {
    let mut bootstrap: Bootstrap = serde_yaml::from_str(yaml)?;
    bootstrap::validate(&mut bootstrap)?;
    Ok(bootstrap)
}
```

- [ ] **Step 7: Update existing tests' `validate` callsites to mutable references.**

Search:

```bash
grep -n 'validate(&\b\|validate(&\(bs\|bootstrap\)' crates/envoy-config/src/bootstrap.rs
```

Expected: a number of test callsites in `bootstrap.rs::tests` use `validate(&bs)` or `validate(&parsed)`. Update each to `validate(&mut bs)` (and change `let bs` to `let mut bs` where needed). Also: the helper `parse_then_validate` (added in 04.1 Task 2) that wraps `validate` for tests — its body also needs the `&mut` switch.

```rust
// Updated parse_then_validate:
fn parse_then_validate(yaml: &str) -> Result<Bootstrap, crate::ConfigError> {
    let mut bs: Bootstrap = serde_yaml::from_str(yaml)?;
    validate(&mut bs)?;
    Ok(bs)
}
```

(If `parse_then_validate` is in `bootstrap.rs::tests` already with a `&` borrow, switch to `&mut`. The 04.1 Task 2 PROGRESS notes that the helper exists and parses+validates.)

- [ ] **Step 8: Run the full crate test.**

```bash
cargo test -p envoy-config --lib
```

Expected: 90 + 10 = 100 passes (all previous green tests survive the `&mut` migration; all 10 new validator tests green). If a pre-existing test fails on the new mut signature, it's almost certainly a compile error rather than a behavior change — fix the borrow form and continue.

- [ ] **Step 9: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: all three exit 0. The `&mut Bootstrap` ripple into envoy-bin is contained: envoy-bin's `crates/envoy-bin/src/main.rs` calls `envoy_config::parse_bootstrap`, which absorbs the `&mut` internally; no envoy-bin source change required.

- [ ] **Step 10: Append a Task 5 section to PROGRESS.md.**

```markdown
## Task 5 — envoy-config validator: regex compile + Int64Range bounds + EmptyHeaderName + matcher walk (2026-04-27)

- Commit: <SHA>
- Change: extended `validate_hcm` (now `&mut`-signature) to walk `route.r#match.headers` Vec per route. Added validate_header_matcher helper (rejects empty name; dispatches by mode; range validates start < end; SafeRegex compile-pass via compile_safe_regex helper). Added compile_safe_regex helper that wraps `regex::Regex::new(&sr.regex)` and stores the compiled regex back on `safe_regex.compiled = Some(Arc::new(re))`. Switched `validate` and `validate_hcm` from `&Bootstrap`/`&HttpConnectionManagerConfig` to `&mut` to allow the compile-pass mutation. parse_bootstrap absorbs the &mut internally; envoy-bin needs no change. parse_then_validate test helper updated.
- Tests added (10): rejects_empty_header_name, rejects_invalid_regex_in_safe_regex_match, rejects_invalid_regex_in_string_match_safe_regex, rejects_invalid_int64_range_start_eq_end, rejects_invalid_int64_range_start_gt_end, validator_compiles_safe_regex_match_into_arc, validator_accepts_all_seven_modes, validator_accepts_empty_headers_vec, validator_accepts_invert_match_true, validator_compiles_string_match_safe_regex_into_arc.
- Verification: `cargo test -p envoy-config --lib` → 100 passed; `cargo build --workspace --all-targets` clean (envoy-bin absorbs the &mut transparently); clippy + fmt clean.
- Deviations: <document any — especially the &mut signature change scope if it ripples beyond expectation>.
```

- [ ] **Step 11: Commit.**

```bash
git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs
git commit -m "phase 04.2: envoy-config — validator regex compile + Int64Range bounds + matcher walk (task 5)"
git add docs/envoy-rust/phases/04.2-route-matchers/PROGRESS.md
git commit -m "phase 04.2: progress note (task 5)"
```

---

### Task 6: `envoy-config::matcher` — `HeaderMatcher::matches` + `StringMatcher::matches` runtime + ~25 matcher-runtime unit tests

**Files:**
- Create: `crates/envoy-config/src/matcher.rs`
- Modify: `crates/envoy-config/src/lib.rs` (add `pub mod matcher;` declaration)
- Modify (append): `docs/envoy-rust/phases/04.2-route-matchers/PROGRESS.md`

**Why now:** Task 5 landed the validator that compiles regex patterns; Task 6 lands the per-matcher truth predicate consumed by HCM's route walker (Task 7). Keeping the matcher runtime in a sibling `matcher.rs` (per SPEC §6 signpost 19) keeps `bootstrap.rs` (already 2909 LoC at 04.1 close + ~300 added in Tasks 2–5) from growing unwieldy and groups the ~25 matcher-runtime tests with the impl they cover.

**Scope.** New `matcher.rs` module with `impl HeaderMatcher { pub fn matches(&self, headers: &[(String, String)]) -> bool { ... } }` and `impl StringMatcher { pub fn matches(&self, value: &str) -> bool { ... } }`. ~80 LoC impl + ~120 LoC tests covering: per-mode boolean truth tables (3 tests each for ExactMatch / PrefixMatch / SuffixMatch / SafeRegexMatch / RangeMatch / PresentMatch / StringMatch); cross-cuts (header NAME case-insensitivity; header VALUE case-sensitivity by default; invert_match XOR semantics for ExactMatch and PresentMatch; StringMatcher.ignore_case effect on Exact/Prefix/Suffix/Contains and lack of effect on SafeRegex per Envoy proto; range-half-open boundary tests).

- [ ] **Step 1: Create `crates/envoy-config/src/matcher.rs` with a stub.**

```rust
//! HeaderMatcher / StringMatcher runtime. Per-matcher truth predicate
//! consumed by HCM's route walker (in crates/envoy-http1/src/hcm.rs).
//!
//! AND-combination across multiple HeaderMatchers on the same Route lives
//! in the route walker, not here — `HeaderMatcher::matches` is per-matcher.
//!
//! Phase 04.2.

use crate::bootstrap::{HeaderMatcher, HeaderMatcherMode, StringMatcher, StringMatcherMode};

impl HeaderMatcher {
    /// Returns true iff this matcher matches the given header set.
    ///
    /// Header NAME matching is case-insensitive per HTTP/1.1 RFC 7230 §3.2.
    /// Header VALUE matching is case-sensitive by default; the StringMatcher
    /// variant's `ignore_case` flips it for the value (Exact/Prefix/Suffix/
    /// Contains only — SafeRegex callers express case insensitivity via the
    /// `(?i)` inline flag; SPEC §6 signpost 15).
    pub fn matches(&self, headers: &[(String, String)]) -> bool {
        let value = headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(&self.name))
            .map(|(_, v)| v.as_str());

        let mode_result = match &self.mode {
            HeaderMatcherMode::ExactMatch(lit) => value == Some(lit.as_str()),
            HeaderMatcherMode::PrefixMatch(lit) => {
                value.is_some_and(|v| v.starts_with(lit.as_str()))
            }
            HeaderMatcherMode::SuffixMatch(lit) => {
                value.is_some_and(|v| v.ends_with(lit.as_str()))
            }
            HeaderMatcherMode::SafeRegexMatch(sr) => value.is_some_and(|v| {
                sr.compiled
                    .as_ref()
                    .expect("validator ensured compiled")
                    .is_match(v)
            }),
            HeaderMatcherMode::RangeMatch(r) => value
                .and_then(|v| v.parse::<i64>().ok())
                .is_some_and(|n| n >= r.start && n < r.end),
            HeaderMatcherMode::PresentMatch(want_present) => {
                // present_match: true  → header must be present
                // present_match: false → no presence requirement (always true)
                // SPEC §6 signpost 7.
                if *want_present { value.is_some() } else { true }
            }
            HeaderMatcherMode::StringMatch(sm) => value.is_some_and(|v| sm.matches(v)),
        };

        mode_result ^ self.invert_match
    }
}

impl StringMatcher {
    /// Returns true iff this matcher matches the given value. Case sensitivity
    /// of value comparison follows `self.ignore_case` for Exact / Prefix /
    /// Suffix / Contains; SafeRegex ignores `ignore_case` (regex callers use
    /// `(?i)` inline flag instead) per Envoy proto.
    pub fn matches(&self, value: &str) -> bool {
        match &self.mode {
            StringMatcherMode::Exact(lit) => {
                if self.ignore_case {
                    value.eq_ignore_ascii_case(lit)
                } else {
                    value == lit.as_str()
                }
            }
            StringMatcherMode::Prefix(lit) => {
                if self.ignore_case {
                    value.len() >= lit.len()
                        && value[..lit.len()].eq_ignore_ascii_case(lit)
                } else {
                    value.starts_with(lit.as_str())
                }
            }
            StringMatcherMode::Suffix(lit) => {
                if self.ignore_case {
                    value.len() >= lit.len()
                        && value[value.len() - lit.len()..].eq_ignore_ascii_case(lit)
                } else {
                    value.ends_with(lit.as_str())
                }
            }
            StringMatcherMode::SafeRegex(sr) => sr
                .compiled
                .as_ref()
                .expect("validator ensured compiled")
                .is_match(value),
            StringMatcherMode::Contains(lit) => {
                if self.ignore_case {
                    value.to_ascii_lowercase().contains(&lit.to_ascii_lowercase())
                } else {
                    value.contains(lit.as_str())
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bootstrap::{Int64Range, SafeRegex};

    fn h(name: &str, value: &str) -> (String, String) {
        (name.to_string(), value.to_string())
    }

    fn compile(pattern: &str) -> SafeRegex {
        SafeRegex {
            regex: pattern.to_string(),
            compiled: Some(std::sync::Arc::new(regex::Regex::new(pattern).unwrap())),
        }
    }

    fn hm(name: &str, mode: HeaderMatcherMode) -> HeaderMatcher {
        HeaderMatcher {
            name: name.to_string(),
            mode,
            invert_match: false,
        }
    }

    fn hm_inverted(name: &str, mode: HeaderMatcherMode) -> HeaderMatcher {
        HeaderMatcher {
            name: name.to_string(),
            mode,
            invert_match: true,
        }
    }

    // ExactMatch: 3 cells.
    #[test]
    fn exact_match_matches_value() {
        let m = hm("x-foo", HeaderMatcherMode::ExactMatch("bar".into()));
        assert!(m.matches(&[h("x-foo", "bar")]));
    }
    #[test]
    fn exact_match_rejects_value() {
        let m = hm("x-foo", HeaderMatcherMode::ExactMatch("bar".into()));
        assert!(!m.matches(&[h("x-foo", "baz")]));
    }
    #[test]
    fn exact_match_absent_returns_false() {
        let m = hm("x-foo", HeaderMatcherMode::ExactMatch("bar".into()));
        assert!(!m.matches(&[h("x-other", "bar")]));
    }

    // PrefixMatch: 3 cells.
    #[test]
    fn prefix_match_matches_value() {
        let m = hm("x-foo", HeaderMatcherMode::PrefixMatch("ba".into()));
        assert!(m.matches(&[h("x-foo", "bar")]));
    }
    #[test]
    fn prefix_match_rejects_value() {
        let m = hm("x-foo", HeaderMatcherMode::PrefixMatch("ba".into()));
        assert!(!m.matches(&[h("x-foo", "qux")]));
    }
    #[test]
    fn prefix_match_absent_returns_false() {
        let m = hm("x-foo", HeaderMatcherMode::PrefixMatch("ba".into()));
        assert!(!m.matches(&[]));
    }

    // SuffixMatch: 3 cells.
    #[test]
    fn suffix_match_matches_value() {
        let m = hm("x-foo", HeaderMatcherMode::SuffixMatch("ar".into()));
        assert!(m.matches(&[h("x-foo", "bar")]));
    }
    #[test]
    fn suffix_match_rejects_value() {
        let m = hm("x-foo", HeaderMatcherMode::SuffixMatch("ar".into()));
        assert!(!m.matches(&[h("x-foo", "qux")]));
    }
    #[test]
    fn suffix_match_absent_returns_false() {
        let m = hm("x-foo", HeaderMatcherMode::SuffixMatch("ar".into()));
        assert!(!m.matches(&[]));
    }

    // SafeRegexMatch: 3 cells.
    #[test]
    fn safe_regex_match_matches_value() {
        let m = hm("x-version", HeaderMatcherMode::SafeRegexMatch(compile("^v[0-9]+$")));
        assert!(m.matches(&[h("x-version", "v42")]));
    }
    #[test]
    fn safe_regex_match_rejects_value() {
        let m = hm("x-version", HeaderMatcherMode::SafeRegexMatch(compile("^v[0-9]+$")));
        assert!(!m.matches(&[h("x-version", "vBETA")]));
    }
    #[test]
    fn safe_regex_match_absent_returns_false() {
        let m = hm("x-version", HeaderMatcherMode::SafeRegexMatch(compile("^v[0-9]+$")));
        assert!(!m.matches(&[]));
    }

    // RangeMatch: 5 cells (boundary checks per SPEC §6 signpost 6).
    #[test]
    fn range_match_value_in_range_returns_true() {
        let m = hm("x-version", HeaderMatcherMode::RangeMatch(Int64Range { start: 1, end: 100 }));
        assert!(m.matches(&[h("x-version", "42")]));
    }
    #[test]
    fn range_match_value_at_start_returns_true() {
        let m = hm("x-version", HeaderMatcherMode::RangeMatch(Int64Range { start: 1, end: 100 }));
        assert!(m.matches(&[h("x-version", "1")]));
    }
    #[test]
    fn range_match_value_at_end_returns_false() {
        // Half-open: end is exclusive.
        let m = hm("x-version", HeaderMatcherMode::RangeMatch(Int64Range { start: 1, end: 100 }));
        assert!(!m.matches(&[h("x-version", "100")]));
    }
    #[test]
    fn range_match_value_below_start_returns_false() {
        let m = hm("x-version", HeaderMatcherMode::RangeMatch(Int64Range { start: 1, end: 100 }));
        assert!(!m.matches(&[h("x-version", "0")]));
    }
    #[test]
    fn range_match_non_parseable_value_returns_false() {
        // Non-parseable values fail the match (NOT an error). SPEC §6 signpost 6.
        let m = hm("x-version", HeaderMatcherMode::RangeMatch(Int64Range { start: 1, end: 100 }));
        assert!(!m.matches(&[h("x-version", "vBETA")]));
    }

    // PresentMatch: 4 cells (true × present, true × absent, false × present, false × absent).
    #[test]
    fn present_match_true_returns_true_when_present() {
        let m = hm("authorization", HeaderMatcherMode::PresentMatch(true));
        assert!(m.matches(&[h("authorization", "Bearer x")]));
    }
    #[test]
    fn present_match_true_returns_false_when_absent() {
        let m = hm("authorization", HeaderMatcherMode::PresentMatch(true));
        assert!(!m.matches(&[]));
    }
    #[test]
    fn present_match_false_returns_true_when_present() {
        // Subtle: present_match: false is "no presence requirement", always true.
        let m = hm("authorization", HeaderMatcherMode::PresentMatch(false));
        assert!(m.matches(&[h("authorization", "Bearer x")]));
    }
    #[test]
    fn present_match_false_returns_true_when_absent() {
        let m = hm("authorization", HeaderMatcherMode::PresentMatch(false));
        assert!(m.matches(&[]));
    }

    // StringMatch: 3 representative cells (Contains + ignore_case; SafeRegex ignore_case no-op).
    #[test]
    fn string_match_contains_returns_true() {
        let sm = StringMatcher {
            mode: StringMatcherMode::Contains("beta".into()),
            ignore_case: false,
        };
        let m = hm("x-tag", HeaderMatcherMode::StringMatch(sm));
        assert!(m.matches(&[h("x-tag", "release-beta-1")]));
    }
    #[test]
    fn string_match_contains_with_ignore_case_returns_true_on_uppercase() {
        let sm = StringMatcher {
            mode: StringMatcherMode::Contains("beta".into()),
            ignore_case: true,
        };
        let m = hm("x-tag", HeaderMatcherMode::StringMatch(sm));
        assert!(m.matches(&[h("x-tag", "RELEASE-BETA-1")]));
    }
    #[test]
    fn string_match_safe_regex_ignore_case_no_effect() {
        // ignore_case: true does NOT affect the SafeRegex variant per Envoy proto.
        let sm = StringMatcher {
            mode: StringMatcherMode::SafeRegex(compile("^beta$")),
            ignore_case: true,
        };
        let m = hm("x-tag", HeaderMatcherMode::StringMatch(sm));
        // Pattern is case-sensitive; "BETA" should not match despite ignore_case.
        assert!(!m.matches(&[h("x-tag", "BETA")]));
        assert!(m.matches(&[h("x-tag", "beta")]));
    }

    // Cross-cutting tests.
    #[test]
    fn header_name_match_is_case_insensitive() {
        let m = hm("X-Foo", HeaderMatcherMode::ExactMatch("bar".into()));
        assert!(m.matches(&[h("x-foo", "bar")]));
    }
    #[test]
    fn header_value_match_is_case_sensitive_by_default() {
        let m = hm("x-foo", HeaderMatcherMode::ExactMatch("bar".into()));
        assert!(!m.matches(&[h("x-foo", "BAR")]));
    }
    #[test]
    fn invert_match_inverts_exact_match_result() {
        let m = hm_inverted("x-foo", HeaderMatcherMode::ExactMatch("bar".into()));
        assert!(m.matches(&[h("x-foo", "baz")]));
        assert!(!m.matches(&[h("x-foo", "bar")]));
    }
    #[test]
    fn invert_match_inverts_present_match_result() {
        let m = hm_inverted("authorization", HeaderMatcherMode::PresentMatch(true));
        assert!(m.matches(&[]));
        assert!(!m.matches(&[h("authorization", "x")]));
    }
}
```

(Total: ~28 tests; the SPEC's "~25" was an estimate and the boundary-test discipline pushes the actual count slightly higher. Acceptable per SPEC §5 closing paragraph: coverage-preserving.)

- [ ] **Step 2: Add `pub mod matcher;` to `crates/envoy-config/src/lib.rs`.**

Locate `pub mod bootstrap;` (line 7). Append:

```rust
pub mod bootstrap;
pub mod matcher;
```

The `impl HeaderMatcher` and `impl StringMatcher` blocks in `matcher.rs` make the inherent methods reachable on the existing public types — no additional re-exports needed.

- [ ] **Step 3: Run the matcher tests.**

```bash
cargo test -p envoy-config matcher::
```

Expected: ~28 passed (count exactly per the test list above). Document the actual count in PROGRESS.

- [ ] **Step 4: Run the full crate test.**

```bash
cargo test -p envoy-config --lib
```

Expected: 100 + ~28 = ~128 passes.

- [ ] **Step 5: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: all three exit 0.

- [ ] **Step 6: Append a Task 6 section to PROGRESS.md.**

```markdown
## Task 6 — envoy-config::matcher runtime + ~28 matcher tests (2026-04-27)

- Commit: <SHA>
- Change: created crates/envoy-config/src/matcher.rs with `impl HeaderMatcher::matches(&self, headers: &[(String, String)]) -> bool` and `impl StringMatcher::matches(&self, value: &str) -> bool`. Header name lookup uses eq_ignore_ascii_case (HTTP/1.1 §3.2). XOR with invert_match. SafeRegex variants take `safe_regex.compiled.as_ref().expect("validator ensured compiled")`. StringMatcher.ignore_case affects Exact/Prefix/Suffix/Contains (case-folded comparison) but not SafeRegex (Envoy proto: regex callers use `(?i)`). Half-open i64 range. Non-parseable RangeMatch values fail the match (not an error). present_match: false is "no presence requirement" (always true) per SPEC §6 signpost 7. Added pub mod matcher; to lib.rs.
- Tests added (~28): per-mode boolean truth tables (3 each for ExactMatch / PrefixMatch / SuffixMatch / SafeRegexMatch; 5 for RangeMatch boundary; 4 for PresentMatch; 3 for StringMatch); cross-cuts (header name case-insensitivity; header value case-sensitivity by default; invert_match for ExactMatch + PresentMatch).
- Verification: `cargo test -p envoy-config --lib` → ~128 passed; clippy + fmt + build clean.
- Deviations: <document actual test count if it differs; document any edge cases that surfaced>.
```

- [ ] **Step 7: Commit.**

```bash
git add crates/envoy-config/src/matcher.rs crates/envoy-config/src/lib.rs
git commit -m "phase 04.2: envoy-config::matcher — HeaderMatcher::matches + StringMatcher::matches + ~28 tests (task 6)"
git add docs/envoy-rust/phases/04.2-route-matchers/PROGRESS.md
git commit -m "phase 04.2: progress note (task 6)"
```

---

### Task 7: `envoy-http1::hcm` — route walker integration (~10 LoC) + `clone_route_config` extension + ~5 HCM unit tests

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` (extend `route_matches` at line 263-269 to AND-combine `route.r#match.headers.iter().all(|m| m.matches(headers))`; extend `clone_route_config` at line 45-77 to clone the new `headers: Vec<HeaderMatcher>` field; add ~5 HCM unit tests in the existing `hcm.rs::tests` block)
- Modify (append): `docs/envoy-rust/phases/04.2-route-matchers/PROGRESS.md`

**Why now:** Task 6 landed the per-matcher predicate; Task 7 wires it into the route walker so the matcher integration exercises end-to-end. The HCM crate is the only consumer of `HeaderMatcher::matches` in 04.x (per SPEC §3 D1: "consumed by the route walker in HCM"). The route-walker change is small (~10 LoC) per SPEC §6 signpost 16; the `clone_route_config` extension is mechanical.

**Scope.** Two source edits in `hcm.rs` (route_matches + clone_route_config) + ~5 unit tests. The unit tests cover: no-headers (unchanged behavior); single-header-matcher selected; single-header-matcher skipped; multi-header AND-success (both match → selected); multi-header AND-fail (one fails → skipped). Tests follow the established pattern in `hcm.rs::tests` (the `drive(config, req_bytes) -> Vec<u8>` helper from 04.1 Task 10 — see PROGRESS Task 10 line 113).

- [ ] **Step 1: Read the current `route_matches` + `clone_route_config` + the in-tree HeaderMatcher import path.**

```bash
sed -n '45,77p;263,269p' crates/envoy-http1/src/hcm.rs
```

Confirm the existing shape per the inspection done at PLAN-write time. The new `route_matches` signature also needs access to the request's headers; currently it's `fn route_matches(r: &Route, path: &str) -> bool`. Extend to `fn route_matches(r: &Route, path: &str, headers: &[(String, String)]) -> bool`. Update the call site at `hcm.rs:227` (`vh.routes.iter().find(|r| route_matches(r, &req.path))`) to pass the request headers (`route_matches(r, &req.path, &req.headers)`).

Also extend the `envoy_config::*` import block at the top of `hcm.rs` (lines 11-14) to add `HeaderMatcher`:

```rust
use envoy_config::{
    DataSource, DirectResponse, HeaderMatcher, HttpConnectionManagerConfig, Route,
    RouteConfiguration, RouteMatch, VirtualHost,
};
```

- [ ] **Step 2: Write 5 failing HCM route-walker tests.**

The 04.1 PROGRESS Task 10 indicates `hcm.rs::tests` already has a `drive(config, req_bytes) -> Vec<u8>` helper. Reuse it:

```rust
#[tokio::test]
async fn route_with_no_headers_matches_unchanged() {
    // 04.2 regression: a route with empty headers Vec matches based on path only.
    // Mirrors 04.1's first_match_wins_on_routes posture.
    let cfg = build_test_config(vec![Route {
        r#match: RouteMatch {
            prefix: Some("/".into()),
            path: None,
            headers: vec![],
        },
        direct_response: DirectResponse {
            status: 200,
            body: DataSource {
                filename: None,
                inline_string: Some("ok\n".into()),
            },
        },
    }]);
    let req = b"GET /healthz HTTP/1.1\r\nHost: x.test\r\nContent-Length: 0\r\n\r\n";
    let resp = drive(cfg, req).await;
    assert!(std::str::from_utf8(&resp).unwrap().contains("200 OK"));
}

#[tokio::test]
async fn single_header_matcher_route_selected_when_match() {
    let matcher_route = Route {
        r#match: RouteMatch {
            prefix: Some("/api/".into()),
            path: None,
            headers: vec![HeaderMatcher {
                name: "x-foo".into(),
                mode: HeaderMatcherMode::ExactMatch("bar".into()),
                invert_match: false,
            }],
        },
        direct_response: DirectResponse {
            status: 418,
            body: DataSource {
                filename: None,
                inline_string: Some("teapot\n".into()),
            },
        },
    };
    let default_route = Route {
        r#match: RouteMatch {
            prefix: Some("/".into()),
            path: None,
            headers: vec![],
        },
        direct_response: DirectResponse {
            status: 200,
            body: DataSource {
                filename: None,
                inline_string: Some("ok\n".into()),
            },
        },
    };
    let cfg = build_test_config(vec![matcher_route, default_route]);
    let req = b"GET /api/widgets HTTP/1.1\r\nHost: x.test\r\nX-Foo: bar\r\nContent-Length: 0\r\n\r\n";
    let resp = drive(cfg, req).await;
    let s = std::str::from_utf8(&resp).unwrap();
    assert!(s.contains("418"), "expected 418 teapot, got: {s}");
    assert!(s.contains("teapot\n"));
}

#[tokio::test]
async fn single_header_matcher_route_skipped_when_no_match() {
    // Same config as the previous test but X-Foo header absent → falls
    // through to default route (200 OK).
    let matcher_route = Route {
        r#match: RouteMatch {
            prefix: Some("/api/".into()),
            path: None,
            headers: vec![HeaderMatcher {
                name: "x-foo".into(),
                mode: HeaderMatcherMode::ExactMatch("bar".into()),
                invert_match: false,
            }],
        },
        direct_response: DirectResponse {
            status: 418,
            body: DataSource {
                filename: None,
                inline_string: Some("teapot\n".into()),
            },
        },
    };
    let default_route = Route {
        r#match: RouteMatch {
            prefix: Some("/".into()),
            path: None,
            headers: vec![],
        },
        direct_response: DirectResponse {
            status: 200,
            body: DataSource {
                filename: None,
                inline_string: Some("ok\n".into()),
            },
        },
    };
    let cfg = build_test_config(vec![matcher_route, default_route]);
    // Request to /api/widgets but no X-Foo header.
    let req = b"GET /api/widgets HTTP/1.1\r\nHost: x.test\r\nContent-Length: 0\r\n\r\n";
    let resp = drive(cfg, req).await;
    let s = std::str::from_utf8(&resp).unwrap();
    assert!(s.contains("200 OK"), "expected 200, got: {s}");
}

#[tokio::test]
async fn multi_header_matcher_and_combination_all_match() {
    let matcher_route = Route {
        r#match: RouteMatch {
            prefix: Some("/".into()),
            path: None,
            headers: vec![
                HeaderMatcher {
                    name: "x-a".into(),
                    mode: HeaderMatcherMode::ExactMatch("1".into()),
                    invert_match: false,
                },
                HeaderMatcher {
                    name: "x-b".into(),
                    mode: HeaderMatcherMode::ExactMatch("2".into()),
                    invert_match: false,
                },
            ],
        },
        direct_response: DirectResponse {
            status: 418,
            body: DataSource {
                filename: None,
                inline_string: Some("teapot\n".into()),
            },
        },
    };
    let cfg = build_test_config(vec![matcher_route]);
    let req = b"GET / HTTP/1.1\r\nHost: x.test\r\nX-A: 1\r\nX-B: 2\r\nContent-Length: 0\r\n\r\n";
    let resp = drive(cfg, req).await;
    assert!(std::str::from_utf8(&resp).unwrap().contains("418"));
}

#[tokio::test]
async fn multi_header_matcher_and_combination_one_fails() {
    let matcher_route = Route {
        r#match: RouteMatch {
            prefix: Some("/api/".into()),
            path: None,
            headers: vec![
                HeaderMatcher {
                    name: "x-a".into(),
                    mode: HeaderMatcherMode::ExactMatch("1".into()),
                    invert_match: false,
                },
                HeaderMatcher {
                    name: "x-b".into(),
                    mode: HeaderMatcherMode::ExactMatch("2".into()),
                    invert_match: false,
                },
            ],
        },
        direct_response: DirectResponse {
            status: 418,
            body: DataSource {
                filename: None,
                inline_string: Some("teapot\n".into()),
            },
        },
    };
    let default_route = Route {
        r#match: RouteMatch {
            prefix: Some("/".into()),
            path: None,
            headers: vec![],
        },
        direct_response: DirectResponse {
            status: 200,
            body: DataSource {
                filename: None,
                inline_string: Some("ok\n".into()),
            },
        },
    };
    let cfg = build_test_config(vec![matcher_route, default_route]);
    // X-A matches, X-B does not → matcher route fails, fall through to default.
    let req = b"GET /api/widgets HTTP/1.1\r\nHost: x.test\r\nX-A: 1\r\nX-B: WRONG\r\nContent-Length: 0\r\n\r\n";
    let resp = drive(cfg, req).await;
    assert!(std::str::from_utf8(&resp).unwrap().contains("200 OK"));
}
```

These tests need an `HeaderMatcherMode` import in the test module:

```rust
#[cfg(test)]
mod tests {
    // ... existing imports ...
    use envoy_config::{HeaderMatcher, HeaderMatcherMode};
    // ...
}
```

The `build_test_config(routes: Vec<Route>) -> Arc<HCMConfig>` helper may not exist verbatim in 04.1 — the 04.1 PROGRESS Task 10 mentions a `drive(config, req_bytes)` helper. If `build_test_config` doesn't exist, add it as a small private test helper (mirrors the inline test-setup pattern in 04.1 hcm tests):

```rust
fn build_test_config(routes: Vec<Route>) -> Arc<HCMConfig> {
    Arc::new(HCMConfig {
        stat_prefix: "test".into(),
        route_config: Arc::new(RouteConfiguration {
            name: "test_rc".into(),
            virtual_hosts: vec![VirtualHost {
                name: "test_vh".into(),
                domains: vec!["*".into()],
                routes,
            }],
        }),
    })
}
```

- [ ] **Step 3: Run the tests to verify they fail.**

```bash
cargo test -p envoy-http1 hcm:: 2>&1 | tail -30
```

Expected: 5 new tests fail. Modes:
- `route_with_no_headers_matches_unchanged` — fails to compile because `RouteMatch { ..., headers: vec![] }` references the new field but the route walker doesn't yet AND-combine — wait, the test should still pass since `headers: vec![]` means "no header constraints"; `route_matches` currently doesn't look at headers. Actually, since Task 4 added the `headers` field to `RouteMatch`, the existing route walker is unaffected by an empty Vec — this test should pass even before the route-walker change. Use it as a baseline regression.
- The 4 other tests all fail (the route walker doesn't currently consult `headers`, so the matcher route gets selected even when the matcher would reject, or vice versa).

If `route_with_no_headers_matches_unchanged` passes pre-change, that's the regression-baseline; if it fails to compile, the existing 04.1 test setup needs minor tweaks (e.g., the `Route { ... }` literal previously had no `headers` field — now adding `headers: vec![]` makes it explicit and compatible). Document either case in PROGRESS.

- [ ] **Step 4: Extend `route_matches` to AND-combine header matchers.**

Locate `route_matches` (line 263-269):

```rust
fn route_matches(r: &Route, path: &str) -> bool {
    match (&r.r#match.prefix, &r.r#match.path) {
        (Some(p), None) => path.starts_with(p),
        (None, Some(p)) => path == p,
        _ => false, // validator rejects (Some, Some) and (None, None).
    }
}
```

Replace with:

```rust
fn route_matches(r: &Route, path: &str, headers: &[(String, String)]) -> bool {
    let path_match = match (&r.r#match.prefix, &r.r#match.path) {
        (Some(p), None) => path.starts_with(p),
        (None, Some(p)) => path == p,
        _ => false, // validator rejects (Some, Some) and (None, None).
    };
    if !path_match {
        return false;
    }
    // 04.2: AND-combine HeaderMatchers per Envoy default headers_match_options: ALL.
    r.r#match.headers.iter().all(|m| m.matches(headers))
}
```

Update the call site at line 227 in `build_response`:

```rust
let route = match vh.routes.iter().find(|r| route_matches(r, &req.path, &req.headers)) {
```

- [ ] **Step 5: Extend `clone_route_config` to clone the new `headers` field.**

Locate `clone_route_config` (lines 45-77). The current `RouteMatch { ... }` literal at lines 61-64 has only `prefix` and `path`; add the `headers` field:

```rust
r#match: RouteMatch {
    prefix: r.r#match.prefix.clone(),
    path: r.r#match.path.clone(),
    headers: r.r#match.headers.clone(),
},
```

`HeaderMatcher` derives `Clone` (per Task 4); the `Vec<HeaderMatcher>::clone()` is mechanical and `SafeRegex.compiled: Option<Arc<regex::Regex>>` clones cheaply via `Arc::clone`.

(Alternative path: if `RouteMatch`, `Route`, `VirtualHost`, `RouteConfiguration`, `DirectResponse`, `DataSource` all derive `Clone` after Task 4 — the additive `Clone` derive on `RouteMatch` may have rippled — `clone_route_config` retires entirely. If so, replace it with `Arc::new(rc.clone())` at the call site `hcm.rs:40`. Document the choice in PROGRESS.)

- [ ] **Step 6: Run the HCM tests.**

```bash
cargo test -p envoy-http1 hcm::
```

Expected: previous 19 + 5 new = 24 passes (or 19 + 5 + 1 + 1 if the 04.1 review-fix test count is still in tree). The 5 new tests now green.

- [ ] **Step 7: Run the full envoy-http1 test.**

```bash
cargo test -p envoy-http1
```

Expected: all green; total count rises by 5 from 04.1 close.

- [ ] **Step 8: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: all three exit 0.

- [ ] **Step 9: Append a Task 7 section to PROGRESS.md.**

```markdown
## Task 7 — envoy-http1::hcm route walker integration + 5 HCM tests (2026-04-27)

- Commit: <SHA>
- Change: extended `route_matches` (signature gains `headers: &[(String, String)]`) to AND-combine `r.r#match.headers.iter().all(|m| m.matches(headers))` after the existing path-side oneof match — short-circuits on first non-match (Envoy default `headers_match_options: ALL`). Updated the single call site in `build_response` to pass `&req.headers`. Extended `clone_route_config` (or retired it; document the path taken) to clone the new RouteMatch.headers Vec — Vec<HeaderMatcher>::clone() is cheap (HeaderMatcher derives Clone; SafeRegex.compiled: Option<Arc<regex::Regex>> clones via Arc::clone). Added envoy_config::HeaderMatcher to the import block.
- Tests added (5): route_with_no_headers_matches_unchanged (regression baseline), single_header_matcher_route_selected_when_match, single_header_matcher_route_skipped_when_no_match, multi_header_matcher_and_combination_all_match, multi_header_matcher_and_combination_one_fails. Added private build_test_config(routes) test helper.
- Verification: `cargo test -p envoy-http1` → 19 + 5 = 24 passed; clippy + fmt + build clean.
- Deviations: <document any — especially whether clone_route_config retired or stayed>.
```

- [ ] **Step 10: Commit.**

```bash
git add crates/envoy-http1/src/hcm.rs
git commit -m "phase 04.2: envoy-http1::hcm — route walker AND-combines headers (task 7)"
git add docs/envoy-rust/phases/04.2-route-matchers/PROGRESS.md
git commit -m "phase 04.2: progress note (task 7)"
```

---

### Task 8: `envoy-config` fuzz corpus — `route_with_header_matchers.yaml` seed + `.gitignore` allow-list + `bootstrap.rs::tests` corpus-walk extension

**Files:**
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/route_with_header_matchers.yaml`
- Modify: `crates/envoy-config/fuzz/.gitignore` (append `!corpus/parse_bootstrap/route_with_header_matchers.yaml`)
- Modify: `crates/envoy-config/src/bootstrap.rs` (add the new seed to the parse-Ok list in `fuzz_corpus_seeds_parse_or_reject_cleanly` at line 1549+)
- Modify (append): `docs/envoy-rust/phases/04.2-route-matchers/PROGRESS.md`

**Why now:** Task 5's validator landed; the seed must round-trip through `parse_bootstrap` cleanly so the corpus-walk test can assert it. Per SPEC §3 D1 + §6 signpost 11, the seed exercises 5 of the 7 modes simultaneously inside one Route's `headers:` Vec; the fuzzer mutates the YAML to surface unexpected serde + validator paths.

**Scope.** One new YAML file (~30 LoC); one allow-list line in `.gitignore`; one new entry in the parse-Ok loop of the corpus-walk test.

- [ ] **Step 1: Create `crates/envoy-config/fuzz/corpus/parse_bootstrap/route_with_header_matchers.yaml`.**

```yaml
node:
  id: x
  cluster: y
static_resources:
  listeners:
    - name: hcm_listener
      address:
        socket_address: { address: 0.0.0.0, port_value: 8080 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: default
                      domains: ["*"]
                      routes:
                        - match:
                            prefix: "/api/"
                            headers:
                              - name: "x-foo"
                                exact_match: "bar"
                              - name: "x-version"
                                safe_regex_match:
                                  regex: "^v[0-9]+$"
                              - name: "x-build"
                                range_match: { start: 1, end: 100 }
                              - name: "authorization"
                                present_match: true
                              - name: "x-tag"
                                string_match:
                                  contains: "beta"
                                  ignore_case: true
                          direct_response:
                            status: 200
                            body: { inline_string: "ok\n" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
admin:
  address:
    socket_address: { address: 0.0.0.0, port_value: 0 }
```

This seed exercises 5 of the 7 modes (exact_match, safe_regex_match, range_match, present_match, string_match-with-contains-and-ignore_case). The other 2 modes (`prefix_match`, `suffix_match`) are exercised by the existing `hcm_direct_response_happy.yaml` seed under fuzzer mutation — adding them here too would be redundant and would inflate the seed past the `parse_bootstrap` target's typical seed size.

- [ ] **Step 2: Add the seed to `.gitignore`'s allow-list.**

Edit `crates/envoy-config/fuzz/.gitignore`. After the existing `!corpus/parse_bootstrap/hcm_invalid_codec_type.yaml` line (the last 04.1 addition):

```
!corpus/parse_bootstrap/route_with_header_matchers.yaml
```

- [ ] **Step 3: Verify the seed is now tracked by git.**

```bash
git status crates/envoy-config/fuzz/corpus/parse_bootstrap/
```

Expected: `route_with_header_matchers.yaml` shows up as a new untracked file (visible to git after the `.gitignore` allow-list extension).

- [ ] **Step 4: Write the corpus-walk extension as a single-line edit.**

The existing test at `crates/envoy-config/src/bootstrap.rs::fuzz_corpus_seeds_parse_or_reject_cleanly` (line 1549+) hand-lists seeds in two groups (parse-Ok and reject-cleanly). Append to the parse-Ok group (after `"fuzz/corpus/parse_bootstrap/hcm_direct_response_happy.yaml",`):

```rust
            "fuzz/corpus/parse_bootstrap/route_with_header_matchers.yaml",
```

- [ ] **Step 5: Run the corpus-walk test.**

```bash
cargo test -p envoy-config fuzz_corpus_seeds_parse_or_reject_cleanly
```

Expected: PASS — the new seed parses + validates Ok per the regex compile-pass + the all-7-modes acceptance from Task 5.

If the test fails because the seed's regex pattern is unparseable or its YAML shape is wrong, debug per `superpowers:systematic-debugging` and fix the seed YAML.

- [ ] **Step 6: Run the full crate test.**

```bash
cargo test -p envoy-config --lib
```

Expected: still ~128 passing (the corpus-walk test was already counted; only its enumeration grew).

- [ ] **Step 7: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: all three exit 0.

- [ ] **Step 8: (Optional, if cargo-fuzz available locally) Smoke-run the fuzz target.**

```bash
cd crates/envoy-config/fuzz
cargo +nightly fuzz run parse_bootstrap -- -max_total_time=10
```

Expected: 0 crashes in 10 seconds; the fuzzer picks up the new seed and mutates from it. Skip if cargo-fuzz is not installed; CI exercises the `-max_total_time=30` budget.

- [ ] **Step 9: Append a Task 8 section to PROGRESS.md.**

```markdown
## Task 8 — fuzz corpus extension (route_with_header_matchers seed) (2026-04-27)

- Commit: <SHA>
- Change: created crates/envoy-config/fuzz/corpus/parse_bootstrap/route_with_header_matchers.yaml exercising 5 of the 7 HeaderMatcher modes simultaneously (exact_match, safe_regex_match, range_match, present_match, string_match-with-contains-and-ignore_case) inside a single Route's headers Vec. Added the corresponding allow-list entry to crates/envoy-config/fuzz/.gitignore. Extended crates/envoy-config/src/bootstrap.rs::tests::fuzz_corpus_seeds_parse_or_reject_cleanly's parse-Ok list with the new seed.
- Verification: `cargo test -p envoy-config fuzz_corpus_seeds_parse_or_reject_cleanly` → PASS; `cargo test -p envoy-config --lib` → ~128 passed (test count unchanged — corpus-walk test was already counted; only enumeration grew); clippy + fmt + build clean. Local cargo-fuzz smoke-run: <document if run + result; CI runs the standard -max_total_time=30 budget>.
- Deviations: <document any>.
```

- [ ] **Step 10: Commit.**

```bash
git add crates/envoy-config/fuzz/corpus/parse_bootstrap/route_with_header_matchers.yaml crates/envoy-config/fuzz/.gitignore crates/envoy-config/src/bootstrap.rs
git commit -m "phase 04.2: envoy-config fuzz seed — route_with_header_matchers (task 8)"
git add docs/envoy-rust/phases/04.2-route-matchers/PROGRESS.md
git commit -m "phase 04.2: progress note (task 8)"
```

---

### Task 9: Differential harness — `Driver::Http1ProbeList` + `Http1Probe` + `drive_http1` `extra_headers` parameter + dispatch arm + 2 harness unit tests

**Files:**
- Modify: `tests/differential/src/lib.rs` (add `Driver::Http1ProbeList` variant; add `Http1Probe` struct; extend `drive_http1` signature to take `extra_headers: &[(String, String)]`; add `Driver::Http1ProbeList` dispatch arm in `run_fixture`; add 2 unit tests asserting expectations.yaml round-trip + the `extra_headers` request-line construction)
- Modify (append): `docs/envoy-rust/phases/04.2-route-matchers/PROGRESS.md`

**Why now:** Task 10 amends fixture 0007 to a two-probe shape per SPEC §3 D3; the harness must support multi-probe iteration first. The `Driver::Http1ProbeList` variant mirrors the established `TlsTcpProbeList` shape (03.2). The `extra_headers` extension to `drive_http1` lets the matcher probe inject `X-Foo: bar`.

**Scope.** ~40 LoC harness extensions: `Http1Probe` struct (~20 LoC), `Driver::Http1ProbeList` variant (~10 LoC), `drive_http1` parameter extension (~10 LoC), `Driver::Http1ProbeList` dispatch arm in `run_fixture` (~30 LoC mirror of the existing `Driver::Http1` arm but iterating per-probe). Existing `Driver::Http1` callsites (lines 1054, 1057) pass `&[]` for `extra_headers`. 2 unit tests.

- [ ] **Step 1: Read the current `Driver` enum + `drive_http1` shape.**

The PLAN-write inspection captured:
- `Driver::Http1 { method, path, host, expected_status?, expected_body?, expected_headers? }` at lines 65-75
- `drive_http1(addr, method, path, host)` at line 661, request-line construction at lines 671-676
- `Driver::Http1` dispatch arm at lines 1046-1133

- [ ] **Step 2: Write 2 failing harness unit tests.**

Append to `tests/differential/src/lib.rs::tests` (the existing `#[cfg(test)] mod tests { ... }` block):

```rust
#[test]
fn parses_expectations_with_http1_probe_list() {
    // 04.2 NEW: Driver::Http1ProbeList shape parses round-trip from YAML.
    let yaml = r#"
driver:
  kind: http1_probe_list
  probes:
    - name: default-route
      method: get
      path: "/healthz"
      host: "envoy-rust.test"
      expected_status: 200
      expected_body:
        kind: byte_exact
        body: "ok\n"
      expected_headers: set_equal_modulo_allow_list
    - name: matcher-route
      method: get
      path: "/api/widgets"
      host: "envoy-rust.test"
      extra_headers:
        - ["X-Foo", "bar"]
      expected_status: 418
      expected_body:
        kind: byte_exact
        body: "teapot\n"
      expected_headers: set_equal_modulo_allow_list
equivalence:
  response_status: exact
  response_body: byte_exact
"#;
    let e: Expectations = serde_yaml::from_str(yaml).expect("parses");
    let Driver::Http1ProbeList { probes } = e.driver else {
        panic!("expected Http1ProbeList");
    };
    assert_eq!(probes.len(), 2);
    assert_eq!(probes[0].name, "default-route");
    assert_eq!(probes[0].extra_headers.len(), 0);
    assert_eq!(probes[1].name, "matcher-route");
    assert_eq!(probes[1].extra_headers.len(), 1);
    assert_eq!(probes[1].extra_headers[0].0, "X-Foo");
    assert_eq!(probes[1].extra_headers[0].1, "bar");
}

#[test]
fn http1_probe_extra_headers_default_empty() {
    // 04.2: Http1Probe.extra_headers has #[serde(default)] so probes that don't
    // carry the field deserialize cleanly with extra_headers: vec![].
    let yaml = r#"
name: simple
method: get
path: "/"
host: "x.test"
expected_status: 200
expected_body:
  kind: byte_exact
  body: ""
expected_headers: set_equal_modulo_allow_list
"#;
    let p: Http1Probe = serde_yaml::from_str(yaml).expect("parses");
    assert_eq!(p.extra_headers.len(), 0);
}
```

- [ ] **Step 3: Run the tests to verify they fail.**

```bash
cargo test -p differential parses_expectations_with_http1_probe_list http1_probe_extra_headers_default_empty
```

Expected: FAIL with compile errors referencing unknown names `Driver::Http1ProbeList` and `Http1Probe`.

- [ ] **Step 4: Add the `Http1Probe` struct.**

Append to `tests/differential/src/lib.rs` (near the existing `Http1Method` / `Http1BodyRule` / `Http1HeaderRule` block at lines 108-152):

```rust
/// 04.2 NEW: one probe entry inside `Driver::Http1ProbeList`. Each probe drives
/// one HTTP/1.1 request through both upstream Envoy and envoy-rust, applying
/// the same 5-axis equivalence cascade the single-probe `Driver::Http1` does.
/// Extra request headers (e.g. `X-Foo: bar`) inject through `extra_headers`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Http1Probe {
    /// Human-readable label for this probe (appears in failure messages).
    pub name: String,
    pub method: Http1Method,
    pub path: String,
    pub host: String,
    /// Extra request headers beyond the harness-emitted defaults
    /// (`Host`, `Connection: close`). Empty Vec means no extras.
    #[serde(default)]
    pub extra_headers: Vec<(String, String)>,
    #[serde(default)]
    pub expected_status: Option<u16>,
    #[serde(default)]
    pub expected_body: Option<Http1BodyRule>,
    #[serde(default)]
    pub expected_headers: Option<Http1HeaderRule>,
}
```

- [ ] **Step 5: Add the `Driver::Http1ProbeList` variant.**

Locate the `Driver` enum at line 38. Append after the existing `Http1 { ... }` variant (line 75):

```rust
    /// 04.2 NEW: drive a sequence of HTTP/1.1 probes against a single listener
    /// address. Each probe runs an independent request/response cycle and
    /// applies the per-probe equivalence cascade. Mirrors the established
    /// `TlsTcpProbeList` shape (03.2). Per SPEC §3 D3.
    Http1ProbeList { probes: Vec<Http1Probe> },
```

- [ ] **Step 6: Extend `drive_http1` to accept `extra_headers: &[(String, String)]`.**

Locate `drive_http1` (line 661). Change the signature:

```rust
pub async fn drive_http1(
    addr: SocketAddr,
    method: &Http1Method,
    path: &str,
    host: &str,
    extra_headers: &[(String, String)],
) -> Result<DriveHttp1Result> {
```

In the request-line construction (lines 671-676), append the extra headers after the `Host:` and before `Connection: close`:

```rust
    let mut req = format!(
        "{} {} HTTP/1.1\r\nHost: {}\r\n",
        method.as_str(),
        path,
        host,
    );
    for (n, v) in extra_headers {
        req.push_str(&format!("{n}: {v}\r\n"));
    }
    req.push_str("Connection: close\r\n\r\n");
    stream.write_all(req.as_bytes()).await?;
```

The `req_line` variable rename (`req`) is internal-only; if the existing variable is reused later in the function, keep the original name (`req_line`) and only modify the construction shape.

- [ ] **Step 7: Update existing `Driver::Http1` callsites to pass `&[]`.**

Locate the two callsites in `run_fixture` (lines 1054, 1057):

```rust
let upstream_resp = drive_http1(upstream_addr, method, path, host, &[])
let subject_resp = drive_http1(subject_addr, method, path, host, &[])
```

- [ ] **Step 8: Add the `Driver::Http1ProbeList` dispatch arm.**

In `run_fixture`, mirror the existing `Driver::Http1` arm structure for the new variant. Locate the closing `}` of the `Driver::Http1 { ... }` arm (around line 1133), then add a sibling arm:

```rust
        Driver::Http1ProbeList { probes } => {
            // Iterate probes; per-probe equivalence cascade mirrors the
            // single-probe Driver::Http1 arm. Subject + upstream tear down
            // AFTER all probes have run.
            for probe in probes {
                let upstream_resp = drive_http1(
                    upstream_addr,
                    &probe.method,
                    &probe.path,
                    &probe.host,
                    &probe.extra_headers,
                )
                .await
                .with_context(|| format!("upstream envoy http1 drive (probe {})", probe.name))?;
                let subject_resp = drive_http1(
                    subject_addr,
                    &probe.method,
                    &probe.path,
                    &probe.host,
                    &probe.extra_headers,
                )
                .await
                .with_context(|| format!("envoy-rust http1 drive (probe {})", probe.name))?;

                // Status: envoy ↔ envoy-rust under `response_status: exact`.
                if matches!(
                    expectations.equivalence.response_status,
                    Some(StatusRule::Exact)
                ) && upstream_resp.status != subject_resp.status
                {
                    bail!(
                        "probe {}: response status mismatch under `response_status: exact`\n  \
                         upstream: {}\n  subject:  {}",
                        probe.name,
                        upstream_resp.status,
                        subject_resp.status,
                    );
                }
                if let Some(es) = probe.expected_status {
                    if upstream_resp.status != es {
                        bail!(
                            "probe {}: upstream status {} != expected {}",
                            probe.name,
                            upstream_resp.status,
                            es,
                        );
                    }
                    if subject_resp.status != es {
                        bail!(
                            "probe {}: subject status {} != expected {}",
                            probe.name,
                            subject_resp.status,
                            es,
                        );
                    }
                }

                // Body.
                if matches!(
                    expectations.equivalence.response_body,
                    Some(BodyRule::ByteExact)
                ) && upstream_resp.body != subject_resp.body
                {
                    bail!(
                        "probe {}: byte-exact body mismatch\n  upstream: {:?}\n  subject:  {:?}",
                        probe.name,
                        upstream_resp.body,
                        subject_resp.body,
                    );
                }
                if let Some(Http1BodyRule::ByteExact { body }) = &probe.expected_body {
                    let expected = body.as_bytes();
                    if upstream_resp.body != expected {
                        bail!(
                            "probe {}: upstream body != expected\n  upstream: {:?}\n  expected: {:?}",
                            probe.name,
                            upstream_resp.body,
                            expected,
                        );
                    }
                    if subject_resp.body != expected {
                        bail!(
                            "probe {}: subject body != expected\n  subject:  {:?}\n  expected: {:?}",
                            probe.name,
                            subject_resp.body,
                            expected,
                        );
                    }
                }

                // Headers.
                if matches!(
                    probe.expected_headers,
                    Some(Http1HeaderRule::SetEqualModuloAllowList)
                ) {
                    diff_headers(
                        &upstream_resp.headers,
                        &subject_resp.headers,
                        HEADER_ALLOW_LIST,
                    )
                    .with_context(|| format!("probe {}: diff_headers", probe.name))?;
                }
            }
            subject.shutdown(Duration::from_secs(5)).await.ok();
            drop(upstream);
        }
```

Also: extend the listener-port substitution map at line 798-802 so `Driver::Http1ProbeList` substitutes `{{PORT}}`:

```rust
        Driver::TcpEcho
        | Driver::TlsTcp { .. }
        | Driver::TlsTcpProbeList { .. }
        | Driver::Http1 { .. }
        | Driver::Http1ProbeList { .. } => "PORT",
```

- [ ] **Step 9: Run the harness tests.**

```bash
cargo test -p differential parses_expectations_with_http1_probe_list http1_probe_extra_headers_default_empty
```

Expected: 2 passed.

- [ ] **Step 10: Run the full differential lib test.**

```bash
cargo test -p differential --lib
```

Expected: previous count + 2 = previous + 2.

- [ ] **Step 11: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: all three exit 0.

- [ ] **Step 12: Append a Task 9 section to PROGRESS.md.**

```markdown
## Task 9 — differential harness: Driver::Http1ProbeList + Http1Probe + drive_http1 extra_headers (2026-04-27)

- Commit: <SHA>
- Change: added Http1Probe struct (name, method, path, host, extra_headers, expected_*) and Driver::Http1ProbeList { probes: Vec<Http1Probe> } variant on the Driver enum (mirrors TlsTcpProbeList shape from 03.2). Extended drive_http1 signature with extra_headers: &[(String, String)] parameter; existing single-probe Driver::Http1 callsites pass &[]. Added Driver::Http1ProbeList dispatch arm in run_fixture iterating probes and applying per-probe equivalence cascade; subject.shutdown + drop(upstream) move to AFTER the probe loop (single teardown per fixture run). Extended the listener-port substitution arm at lines ~798-802 to include Http1ProbeList.
- Tests added (2): parses_expectations_with_http1_probe_list, http1_probe_extra_headers_default_empty.
- Verification: `cargo test -p differential --lib` → previous + 2 passed; clippy + fmt + build clean.
- Deviations: <document any>.
```

- [ ] **Step 13: Commit.**

```bash
git add tests/differential/src/lib.rs
git commit -m "phase 04.2: differential harness — Driver::Http1ProbeList + extra_headers (task 9)"
git add docs/envoy-rust/phases/04.2-route-matchers/PROGRESS.md
git commit -m "phase 04.2: progress note (task 9)"
```

---

### Task 10: Fixture `0007-http1-direct-response` amendment — envoy.yaml + envoy-rust.yaml + new probe input + expectations.yaml restructure + README.md paragraph

**Files:**
- Modify: `tests/fixtures/0007-http1-direct-response/envoy.yaml` (add 04.2 NEW route at head of `routes:`)
- Modify: `tests/fixtures/0007-http1-direct-response/envoy-rust.yaml` (same `routes:` shape; per-side divergences from 04.1 unchanged)
- Create: `tests/fixtures/0007-http1-direct-response/inputs/payload-matcher.bin` (empty file; placeholder per 04.1 `payload.bin` convention)
- Modify: `tests/fixtures/0007-http1-direct-response/expectations.yaml` (switch from `Driver::Http1` to `Driver::Http1ProbeList` with 2 probes)
- Modify: `tests/fixtures/0007-http1-direct-response/README.md` (append one paragraph on the matcher route + ADR-0021 reference)
- Modify (append): `docs/envoy-rust/phases/04.2-route-matchers/PROGRESS.md`

**Why now:** Tasks 1–9 land everything needed to drive the amended fixture end-to-end. Task 10 is the differential gate for 04.2's new feature surface — both proxies must select the same route on each probe.

**Scope.** Five files touched + one new file. Per SPEC §3 D3 + SPEC §1 acceptance signal (a). The matcher route MUST come first in `routes:` because the route walker is single-pass first-match-wins (per parent SPEC §3 D3.1); the catch-all `prefix: "/"` second.

- [ ] **Step 1: Modify `tests/fixtures/0007-http1-direct-response/envoy.yaml`.**

Replace the existing `routes:` block (lines 18-22 of the current envoy.yaml) with:

```yaml
                      routes:
                        # 04.2 NEW route — placed first so first-match-wins reaches it
                        # before the catch-all. Matcher selects this route only when the
                        # request path starts with "/api/" AND the X-Foo header equals "bar".
                        - match:
                            prefix: "/api/"
                            headers:
                              - name: "x-foo"
                                exact_match: "bar"
                          direct_response:
                            status: 418
                            body:
                              inline_string: "teapot\n"
                        # 04.1 PRE-EXISTING route — unchanged. Catch-all default.
                        - match: { prefix: "/" }
                          direct_response:
                            status: 200
                            body: { inline_string: "ok\n" }
```

Final shape (full file):

```yaml
static_resources:
  listeners:
    - name: hcm_listener
      address:
        socket_address: { address: 0.0.0.0, port_value: {{PORT}} }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: default
                      domains: ["*"]
                      routes:
                        - match:
                            prefix: "/api/"
                            headers:
                              - name: "x-foo"
                                exact_match: "bar"
                          direct_response:
                            status: 418
                            body:
                              inline_string: "teapot\n"
                        - match: { prefix: "/" }
                          direct_response:
                            status: 200
                            body: { inline_string: "ok\n" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
admin:
  address:
    socket_address: { address: 0.0.0.0, port_value: 0 }
```

- [ ] **Step 2: Modify `tests/fixtures/0007-http1-direct-response/envoy-rust.yaml`.**

Same `routes:` shape; the per-side divergences (bind `127.0.0.1`, no admin block, has the `node:` block) stay unchanged. Final shape:

```yaml
node:
  id: x
  cluster: y
static_resources:
  listeners:
    - name: hcm_listener
      address:
        socket_address: { address: 127.0.0.1, port_value: {{PORT}} }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: default
                      domains: ["*"]
                      routes:
                        - match:
                            prefix: "/api/"
                            headers:
                              - name: "x-foo"
                                exact_match: "bar"
                          direct_response:
                            status: 418
                            body:
                              inline_string: "teapot\n"
                        - match: { prefix: "/" }
                          direct_response:
                            status: 200
                            body: { inline_string: "ok\n" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
```

- [ ] **Step 3: Create `tests/fixtures/0007-http1-direct-response/inputs/payload-matcher.bin` as an empty file.**

```bash
: > tests/fixtures/0007-http1-direct-response/inputs/payload-matcher.bin
```

(The harness `drive_http1` constructs the wire request from `Http1Probe.{method, path, host, extra_headers}` per Task 9; the `inputs/payload-matcher.bin` file is a placeholder per the 04.1 `inputs/payload.bin` convention — kept so the directory shape is consistent across phases that may need probe-payload inputs. Empty content is fine for 04.x's GET-only probes.)

- [ ] **Step 4: Replace `tests/fixtures/0007-http1-direct-response/expectations.yaml`.**

```yaml
driver:
  kind: http1_probe_list
  probes:
    - name: default-route
      method: get
      path: "/healthz"
      host: "envoy-rust.test"
      expected_status: 200
      expected_body:
        kind: byte_exact
        body: "ok\n"
      expected_headers: set_equal_modulo_allow_list
    - name: matcher-route
      method: get
      path: "/api/widgets"
      host: "envoy-rust.test"
      extra_headers:
        - ["X-Foo", "bar"]
      expected_status: 418
      expected_body:
        kind: byte_exact
        body: "teapot\n"
      expected_headers: set_equal_modulo_allow_list
equivalence:
  response_status: exact
  response_body: byte_exact
```

- [ ] **Step 5: Append a paragraph to `tests/fixtures/0007-http1-direct-response/README.md`.**

After the existing closing paragraph (the ADR-references list), insert before the ADR list (or after it — choose for readability):

```markdown
## 04.2 amendment — header-matcher route

Phase 04.2 added a second route at the head of `routes:` (so first-match-wins
reaches it before the catch-all): `match: { prefix: "/api/", headers: [{ name:
"x-foo", exact_match: "bar" }] }` returning `direct_response: { status: 418,
body: { inline_string: "teapot\n" } }`. The original `prefix: "/"` catch-all
stays second; both proxies must select the same route on each probe — the new
differential property 04.2 exercises.

The fixture now drives two probes via the harness's `Driver::Http1ProbeList`:

- `default-route` — `GET /healthz Host: envoy-rust.test` (no `X-Foo`); falls
  through to the catch-all 200 OK.
- `matcher-route` — `GET /api/widgets Host: envoy-rust.test X-Foo: bar`; hits
  the matcher route 418 teapot.

Each probe applies the same 5-axis equivalence cascade as the 04.1 single-probe
shape (status exact, body byte_exact, headers set_equal_modulo_allow_list).

The matcher route demonstrates production matcher use across all 7 of Envoy's
`HeaderMatcher` modes (which all 7 modes land in 04.2 — `exact_match`,
`prefix_match`, `suffix_match`, `safe_regex_match`, `range_match`,
`present_match`, `string_match`); this fixture exercises only `exact_match` for
maximum minimum-viable coverage. Per-mode runtime behavior is exercised by
the matcher-runtime unit tests in `crates/envoy-config/src/matcher.rs::tests`
(~28 tests covering all 7 modes + invert_match XOR + StringMatcher.ignore_case).
```

Update the ADR-references list to include ADR-0021:

```markdown
ADR references: ADR-0011 (response-header equivalence deferral closes here
via the BEHAVIOR_CONTRACT.md `Header allow-list` table populated at this
phase), ADR-0014 (`typed_config` deserialization), ADR-0020 (split phase 04
into 04.1 + 04.2 + 04.3), ADR-0021 (`regex` permitted as a foundation for
header / route matching at config-load time).
```

- [ ] **Step 6: Run the in-process integration test for fixture 0007.**

The 04.1-landed `crates/envoy-bin/tests/http1_direct_response.rs` is in-process (no Docker). The amended fixture YAML is consumed by the same listener wiring; the matcher route adds zero new envoy-bin behavior — envoy-bin parses the YAML through `envoy_config::parse_bootstrap` (validates the matcher per Task 5), constructs HCMConfig (clones the matcher per Task 7's `clone_route_config` extension), and serves connections. The integration test's `GET /healthz` should still pass (falls through to default route).

```bash
cargo test -p envoy-bin --test http1_direct_response
```

Expected: PASS — the existing `GET /healthz` probe falls through to the catch-all 200 OK exactly as in 04.1; no new request shape exercised. If this fails, the matcher route is somehow being selected for `/healthz` (unexpected — the prefix is `/api/`); debug per `superpowers:systematic-debugging`.

- [ ] **Step 7: Run the Docker-gated differential acceptance test (locally if Docker is available).**

```bash
cargo test -p differential --test http1_direct_response
```

If Docker is available: expected to PASS — both proxies select the same route on each of the two probes. If not, debug per `superpowers:systematic-debugging`. Common failure modes:

- **Envoy v1.33.0 emits different headers for the `direct_response 418` shape than for `direct_response 200`.** Cross-check at execution time. The header allow-list (`server`, `date`) covers value diffs but not name-set diffs; if Envoy emits an additional header (e.g., `vary`), evaluate per ADR-0011's discipline.
- **Envoy v1.33.0 rejects the `headers:` matcher YAML shape.** The schema is well-established in v1.33.0; this should not fire. If it does, cross-check the YAML key spelling + the route ordering.
- **Envoy v1.33.0 returns `200` for `GET /api/widgets X-Foo: bar` instead of `418`.** Indicates the matcher isn't being honored on the upstream side (Envoy bug? unlikely) or the YAML shape is wrong (probable). Cross-check.

If Docker is not available locally: same posture as the other Docker-gated tests; CI runs it on `ubuntu-latest`.

- [ ] **Step 8: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace --lib --bins
```

Expected: all four exit 0. The Docker-gated `http1_direct_response_fixture` is excluded by `--lib --bins` and runs only via `cargo test --workspace` (CI).

- [ ] **Step 9: Append a Task 10 section to PROGRESS.md.**

```markdown
## Task 10 — fixture 0007 amendment (matcher route) (2026-04-27)

- Commit: <SHA>
- Change: amended tests/fixtures/0007-http1-direct-response/{envoy.yaml,envoy-rust.yaml} to add a 04.2 NEW route at the head of routes: with `match: { prefix: "/api/", headers: [{ name: "x-foo", exact_match: "bar" }] }` returning direct_response 418 "teapot\n"; existing `prefix: "/"` catch-all stays second per first-match-wins discipline. Created tests/fixtures/0007-http1-direct-response/inputs/payload-matcher.bin (empty file; placeholder per 04.1 payload.bin convention). Restructured expectations.yaml from single-Driver::Http1 to Driver::Http1ProbeList with two probes (default-route, matcher-route). Appended a "04.2 amendment — header-matcher route" paragraph to README.md and added ADR-0021 to the ADR references list.
- Verification: `cargo test -p envoy-bin --test http1_direct_response` → PASS (in-process backstop: GET /healthz still falls through to default route 200 OK); `cargo test -p differential --test http1_direct_response` → <PASS if Docker available; report Docker availability either way>; workspace gate (build/clippy/fmt/test --lib --bins) clean.
- Deviations: <document any — especially if Envoy emits unexpected headers on 418 direct_response, or if the matcher YAML shape needed adjustment>.
```

- [ ] **Step 10: Commit.**

```bash
git add tests/fixtures/0007-http1-direct-response
git commit -m "phase 04.2: fixture 0007 amendment — matcher route + Driver::Http1ProbeList (task 10)"
git add docs/envoy-rust/phases/04.2-route-matchers/PROGRESS.md
git commit -m "phase 04.2: progress note (task 10)"
```

---

### Task 11: 04.1 REVIEW M-track carryforward check (status = expected no action; document in PROGRESS)

**Files:**
- Modify (append): `docs/envoy-rust/phases/04.2-route-matchers/PROGRESS.md`

**Why now:** Per SPEC §3 D4 the 04.2 plan evaluates 04.1's REVIEW.md carryforwards. The 04.1 REVIEW landed with 7 Minor items (M1–M7); per the SPEC, none block 04.2. The standard expectation is "no action in 04.2" — the task slot exists so a deviation can land cleanly if a 04.1 carryforward turns out to be on the critical path during execution.

**Scope.** Pre-flight check only — no source code changes. The task records the evaluation in PROGRESS.md so the audit trail is complete; the actual decisions for each M-item are reproduced for ease of cross-reference.

- [ ] **Step 1: Re-read 04.1 REVIEW §3–§4 to refresh the M-track items.**

```bash
sed -n '/### Minor/,/^---/p' docs/envoy-rust/phases/04.1-hcm-direct-response/REVIEW.md | head -120
```

Expected: M1–M7 enumerated as in REVIEW.md and STATE.md "Phase-04.1 rollovers" section.

- [ ] **Step 2: Evaluate each M-item against 04.2 work.**

For each item, decide one of:

- **A. No action in 04.2** (default per SPEC §3 D4) — defer per the SPEC's annotation.
- **B. Opportunistic close-out in 04.2** — only if a 04.2 task naturally surfaces the relevant code path.
- **C. Critical-path block** — escalate to `superpowers:systematic-debugging` first.

Expected outcomes per the standing posture (verified at PLAN-write time):

- **M1 (`diff_headers` duplicate-header semantics)** — 04.2's fixture adds `extra_headers` request-side, but the response shape is unchanged from 04.1 (no duplicate response headers). **A. No action in 04.2**; track forward to whichever later phase first surfaces a duplicate-header response (likely 04.3's upstream proxy or phase 06's access log).
- **M2 (body-drain idle timeout silent close)** — 04.2 doesn't touch HCM body-drain logic; the matcher route returns `direct_response` (no body-drain involvement). **A. No action in 04.2**; track forward to 04.3 or hardening pass.
- **M3 (envoy-cluster path-dep with no 04.1 consumer)** — 04.2 doesn't add an envoy-cluster consumer; the dep stays forward-looking. **A. No action in 04.2**; track forward to 04.3 (which wires cluster_mgr).
- **M4 (`strip_port` IPv6 correctness)** — 04.2 doesn't add IPv6-Host fixtures (`/api/widgets` request still uses `Host: envoy-rust.test`). **A. No action in 04.2**; track forward to 04.3 or hardening pass.
- **M5 (Cargo.lock sync cadence)** — 04.2 lands `regex` + transitives; the standing precedent is dedicated post-state-4 commit. The 04.2 plan's Task 12 explicitly follows the dedicated-commit precedent (matches phase 01/02.x/03.x; deviates from 04.1 Task 4's inline-at-scaffold). **A. No action beyond Task 12's standing approach** — Task 12 already follows the M5-recommended standardization.
- **M6 (`drive_http1` per-function unit test)** — 04.2 Task 9 adds 2 harness unit tests on `Http1Probe` parsing + extra_headers default; this is *not* a unit test on `drive_http1` itself, so M6 is not closed in 04.2. **A. No action in 04.2 for M6 specifically**; track forward to 04.3 when the 3rd Driver::Http1* consumer arrives (the http1-echo-server fixture).
- **M7 (`TlsAcceptingHandler` generalization for HCM+TLS)** — 04.2 does not introduce HCM+TLS fixtures. **A. No action in 04.2**; track forward to phase 05+ brainstorm.

If any of the above evaluations changes during execution (e.g., the route-walker integration in Task 7 surfaces a duplicate-header semantic that retroactively makes M1 critical), invoke `superpowers:systematic-debugging` and document the escalation in PROGRESS.md.

- [ ] **Step 3: Append a Task 11 section to PROGRESS.md.**

```markdown
## Task 11 — 04.1 REVIEW M-track carryforward check (2026-04-27)

Per SPEC §3 D4 + the standing posture from STATE.md "Phase-04.1 rollovers": none of M1–M7 are critical-path for 04.2; all defer per their established annotations.

- M1 (`diff_headers` duplicate-header semantics): A. No action in 04.2 — fixture 0007 amendment introduces no duplicate response headers; track forward.
- M2 (body-drain idle timeout silent close): A. No action — matcher route returns direct_response (no body-drain); track forward to 04.3 or hardening.
- M3 (envoy-cluster path-dep with no 04.1 consumer): A. No action — 04.2 adds no cluster consumer; track forward to 04.3.
- M4 (strip_port IPv6 correctness): A. No action — 04.2 fixture uses Host: envoy-rust.test (not IPv6); track forward.
- M5 (Cargo.lock sync cadence): A. No action beyond Task 12 — Task 12 follows the dedicated-post-state-4-commit precedent (phase-01/02.x/03.x), explicitly addressing M5's "standardize" recommendation.
- M6 (drive_http1 per-function unit test): A. No action in 04.2 for M6 specifically — Task 9's tests cover Http1Probe parsing + extra_headers default but not drive_http1 itself; track forward to 04.3.
- M7 (TlsAcceptingHandler generalization for HCM+TLS): A. No action — 04.2 introduces no TLS-bearing HCM fixtures; track forward to phase 05+.

No code changes in this task. If any item escalates during execution, invoke superpowers:systematic-debugging and document the escalation here.
```

- [ ] **Step 4: Commit.**

```bash
git add docs/envoy-rust/phases/04.2-route-matchers/PROGRESS.md
git commit -m "phase 04.2: 04.1 REVIEW M-track carryforward check — no action in 04.2 (task 11)"
```

(No follow-up `progress note` commit; this task IS the progress note.)

---

### Task 12: State 4 phase-done gate

**Files:**
- Modify (append): `docs/envoy-rust/phases/04.2-route-matchers/PROGRESS.md`
- Modify: `Cargo.lock` (synced as a dedicated commit per the established phase-precedent)

**Per `docs/envoy-rust/SKILL_ROUTING.md` state 4.** Run the full local stable-toolchain gate, observe both CI jobs (build+test+lint, fuzz), quote outputs into PROGRESS.md. The plan does not advance ROADMAP.md or STATE.md here — those flip in state 6 (the phase-done commit), not now (BOOTSTRAP_PROMPT.md §5.1: one state per session).

`Cargo.lock` is expected to be dirty after `cargo build` due to ADR-0021's `regex` + transitive surface; per SPEC §6 signpost 20 + ADR-0021's consequences, the sync lands as a dedicated `phase 04.2: sync Cargo.lock with phase 04.2 dep graph (regex + transitives)` commit immediately after Task 12's progress note. Phase precedents: phase-01 `4955252`, phase-02.1 `dea4d16`, phase-02.2 `2146014`, phase-03.1 `eb039e6`, phase-03.2 `85633e7`. (04.1 Task 4 inlined the lock at scaffold-time per PROGRESS Task 17 deviation; 04.2 returns to the dedicated-commit cadence per the M5 carryforward recommendation in Task 11.)

- [ ] **Step 1: Run the local stable-toolchain gate, capturing each command's output.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace --lib --bins
cargo deny check
```

Expected: all five exit 0. Quote tails into PROGRESS.md.

The `cargo test --workspace --lib --bins` count expands from phase 04.1's tally:

- `envoy-config`: previous tally (75 from 04.1 close + Task 2 (4) + Task 3 (5) + Task 4 (6) + Task 5 (10) + Task 6 (~28) = 75 + 53 = 128).
- `envoy-cluster`: unchanged.
- `envoy-listener`: unchanged.
- `envoy-tcp`: unchanged.
- `envoy-tls`: unchanged.
- `envoy-bin`: unchanged at lib-test count; 1 integration test (http1_direct_response from 04.1) still passing (Task 10 verified).
- `envoy-http1`: previous 19 + Task 7 (5) = 24.
- `tcp-echo-server`: unchanged.
- `tls-echo-server`: unchanged.
- `differential` lib: previous tally + Task 9 (2) = previous + 2. Docker-gated integration tests now total 7 (no new Docker test added in 04.2).

- [ ] **Step 2: Sync `Cargo.lock` as a dedicated commit (if dirty).**

```bash
git status
git diff Cargo.lock | head -50
```

Expected: dirty. Diff should add `[[package]]` stanzas for `regex`, `regex-syntax`, `aho-corasick`, `memchr`, plus possibly `unicode-ident` etc. transitively. No version regressions on existing direct deps; no surprising new transitives beyond the regex chain.

```bash
git add Cargo.lock
git commit -m "phase 04.2: sync Cargo.lock with phase 04.2 dep graph (regex + transitives)"
```

(If Cargo.lock has been clean since Task 1 — possible if Task 1 already committed it inline — skip this step and document in PROGRESS that the inline commit at Task 1 satisfied the sync, deviating from the M5-recommended dedicated cadence. Either path is doctrine-conformant.)

- [ ] **Step 3: Trigger CI and observe both jobs.**

After committing all task commits, push the branch and observe the CI run:

```bash
git push origin <branch>
gh run list --workflow=ci.yml -L 1
gh run watch <run-id>
```

Expected: both `build + test + lint` (now also runs the amended `http1_direct_response_fixture` with two probes) and `fuzz (parse_bootstrap, 30s)` jobs succeed. The fuzz job exercises the extended `parse_bootstrap` corpus (1 new HeaderMatcher-shaped seed) automatically.

- [ ] **Step 4: Append the State-4 section to PROGRESS.md.**

Use the phase-04.1 PROGRESS Task 17 section as the precedent shape. Quote the local-gate command outputs (per-crate test tails are the most informative), the CI run number + URL, and document any fix-during-gate commits (the goal is zero — phase 04.1 cleared on first attempt).

```markdown
## Task 12 / State 4 — phase-done gate verification (2026-04-27)

Per `docs/envoy-rust/SKILL_ROUTING.md` state 4: <gate result on first / Nth attempt>. ROADMAP.md and STATE.md are NOT advanced here per `BOOTSTRAP_PROMPT.md` §5.1 (one state per session); those flip in state 6 (the phase-done commit) after state 5's `REVIEW.md` is approved.

### Local stable-toolchain gate

`cargo build --workspace --all-targets`:
```
<tail>
```

`cargo clippy --workspace --all-targets --all-features -- -D warnings`:
```
<tail>
```

`cargo fmt --all -- --check`:
```
(no output — clean)
```

`cargo test --workspace --lib --bins`:
```
<per-crate tails>
```

Total: <N> tests, 0 failed, <ignored>. Expected count: ~128 envoy-config + 24 envoy-http1 + previous-other-crates (24/19/8/15/8/5 from 04.1 close).

`cargo deny check`:
```
advisories ok, bans ok, licenses ok, sources ok
```

### Cargo.lock sync

<note: dirty/clean; if dirty, the SHA of the dedicated sync commit. Per M5 recommendation 04.2 returns to the dedicated-commit cadence.>

### CI

`gh run watch <run-id>`: <result>. Both jobs (build + test + lint, fuzz) succeed.

### Outstanding for state 5/6

State 5 (`superpowers:requesting-code-review`) writes `REVIEW.md` for this phase. State 6 (the phase-done commit) flips ROADMAP row `04.2` `status` → `done` (parent row `04` stays `in-progress` until 04.3 lands per the schema invariant) and advances STATE.md to phase `04.3-router-upstream` (lifecycle state 2; SPEC.md exists from the parent-04 state-2 split commit `1d9740d`, PLAN.md does not; next-skill `superpowers:writing-plans`).
```

- [ ] **Step 5: Commit the PROGRESS update.**

```bash
git add docs/envoy-rust/phases/04.2-route-matchers/PROGRESS.md
git commit -m "phase 04.2: state-4 phase-done gate verification (task 12)"
```

State 4 verification complete. Next session enters state 5 via `superpowers:requesting-code-review` (writing `REVIEW.md`); state 6 then ships the phase-done commit per `BOOTSTRAP_PROMPT.md` §5.3, flipping ROADMAP row `04.2` to `done` and advancing STATE.md to phase `04.3-router-upstream` at lifecycle state 2 with next-skill `superpowers:writing-plans`.

---

## Out-of-plan execution contingencies

These are NOT plan steps; they are decision rules for situations the SPEC and plan jointly anticipate but cannot pin at planning time. Per D-3.5, execution lands an ADR and proceeds when any trigger fires.

1. **Envoy v1.33.0 emits an unexpected response header on the `direct_response 418` shape.** SPEC §2 anticipates 04.2 introduces no new response headers because matchers are config-side. If Envoy emits an extra header (e.g. `vary`, `cache-control`) that envoy-rust's HCM does not, the differential fixture fails on the header-set comparison. Cross-check at Task 10 execution. If true, evaluate per ADR-0011 — either add the header to envoy-rust's HCM `synth_direct_response` (preferred — closes the gap) or extend BEHAVIOR_CONTRACT.md's `Header allow-list` (with an ADR if the extension is policy-laden).

2. **Envoy v1.33.0's matcher implementation diverges from envoy-rust's on the case-sensitivity rule.** envoy-rust's `HeaderMatcher::matches` does ASCII case-insensitive name lookup + case-sensitive value compare (per HTTP/1.1 §3.2 + SPEC §6 signpost 4). If Envoy's behavior differs (e.g. value compare also case-insensitive on `exact_match`), debug per `superpowers:systematic-debugging`. Most likely envoy-rust matches; if Envoy diverges, the `BEHAVIOR_CONTRACT.md` is the source of truth — envoy-rust changes match the contract, not Envoy.

3. **`cargo deny check` flips red on a fresh transitive license from the `regex` chain beyond `Unlicense`.** Step 5 of Task 1 anticipates `Unlicense` addition; if a fresh license surfaces (e.g. `OpenSSL`-license on some deeply-transitive crate), evaluate per ADR-0005's discipline. Most likely a no-op given the well-known regex graph.

4. **The `&mut Bootstrap` signature change in Task 5 ripples into envoy-bin or other crates.** envoy-bin calls `parse_bootstrap` (which absorbs the `&mut` internally), but if some other crate calls `validate` directly, it needs the `&mut` switch too. Search at execution time:

```bash
grep -rn 'envoy_config::validate\|bootstrap::validate' crates/ tests/
```

If non-test callsites surface, update them in lockstep with Task 5's commit.

5. **`hcm.rs::clone_route_config` retires entirely after Task 4's `Clone` derive on `RouteMatch` enables `RouteConfiguration` to derive `Clone`.** This is a coverage-preserving simplification — if all dependent types now derive `Clone`, replace the hand-clone helper with `Arc::new(rc.clone())` at the call site `hcm.rs:40`. Document the path taken in Task 7 PROGRESS.

6. **The 04.1 PROGRESS Task 10 `drive(config, req_bytes)` test helper has different signature than expected at PLAN-write time.** Cross-check at Task 7 execution. If the helper takes `(Arc<HCMConfig>, &[u8])` use it as-is; if the helper signature differs (e.g. takes `HCMConfig` by value), adapt the test code or extend the helper.

7. **Envoy v1.33.0's `present_match: false` semantic differs from envoy-rust's "always true" interpretation.** SPEC §6 signpost 7 names this; both semantics exist in different Envoy code paths historically. The chosen envoy-rust semantic is "no presence requirement" per the proto. If a probe surfaces a divergence, debug + land a contract clarification ADR if the disambiguation is policy-laden.

8. **A task's scope balloons past ~10 sub-steps.** Invoke `superpowers:systematic-debugging` before splitting. Phase 04.2 has already been split (it's a sub-phase of 04); a nested split is anticipated as an anti-pattern per SPEC §5 closing paragraph + parent-04 SPEC §5.

9. **A second ADR lands during 04.2 execution beyond ADR-0021.** SPEC §7 anticipates none beyond ADR-0021. If `cargo deny check` requires a license-allow-list extension that's policy-laden enough to warrant its own ADR (rather than inline `deny.toml` change under ADR-0005), land it as ADR-0022. Document in PROGRESS and reference in the state-6 commit message.

10. **Hand-rolled Deserialize visitors at Tasks 2/3/4 surface a serde ergonomic snag.** The `MapAccess` pattern with `next_key::<String>` + per-key dispatch + final option-take-or-error is the canonical shape and well-trodden. If a future serde change makes this brittle (unlikely on serde 1.x stable), the fallback is a `serde_yaml::Value`-then-rebuild approach (parse to a generic `Value`, inspect keys manually, construct the typed value). ~30% slower at deserialize time but mechanically simpler. Decide at execution time per concrete failure modes.

11. **The `Driver::Http1ProbeList` dispatch arm at Task 9 needs the listener-port substitution map updated for the new variant.** Step 8 of Task 9 names this explicitly; if the substitution arm is missed, the harness fails to render `{{PORT}}` in the fixture YAML and falls back to the default (likely `0` or empty). Symptom: `Address already in use` or YAML parse error on `port_value: `. Catch via Task 10's Docker-gated test.

12. **Test count drift in PROGRESS reports.** PLAN-time estimates are `~128 envoy-config + 24 envoy-http1 + 2 differential = ~154 new` totals; actual counts may drift ±5 due to test-list refactoring or table-driven test consolidation per SPEC §5. Document the actual counts in PROGRESS Task 12.

13. **The 04.1 `parse_then_validate` test helper signature update in Task 5 ripples into Task 2's tests.** Tasks 2/3/4 introduce parse-shape tests that don't call `validate` (parse-only); only Task 5's tests call `parse_then_validate`. So the ripple is contained to Task 5. If Tasks 2/3/4 need `validate` paths after all (to surface the `compiled: Some(...)` post-validate state), they call `parse_bootstrap` directly (which absorbs the `&mut` internally).

14. **`crates/envoy-config/src/lib.rs`'s `pub use bootstrap::{...}` block reorders alphabetically after Task 4's 6-symbol addition.** rustfmt enforces alphabetic ordering on use-list items; the as-committed order may differ from the PLAN-shown order. Apply `cargo fmt` and let rustfmt normalize. No semantic change.

15. **Fixture-0007 amendment surfaces a probe-ordering bug in `Driver::Http1ProbeList` dispatch.** The probes run sequentially in YAML order; if the dispatch arm parallelizes (it shouldn't), state from probe 1 could leak into probe 2 (e.g. socket reuse). The Task 9 plan code uses `for probe in probes { ... }` — strictly sequential. Verify by running both probes against the in-process integration test if Docker is unavailable; cross-check at Task 10.

---

## Final commit message format (state 6 — NOT this state)

The state-6 phase-done commit shape, per SPEC §9. Do NOT land this commit during plan execution; it lands at state 6 (after REVIEW.md is approved at state 5):

```
phase 04.2: HTTP route header matchers + ADR-0021 (regex permitted) [ADR-0021]

All 7 of Envoy's HeaderMatcher modes (exact_match, prefix_match, suffix_match,
safe_regex_match, range_match, present_match, string_match) plus the
StringMatcher tagged union + invert_match: bool land additively on
RouteMatch.headers in envoy-config. ADR-0021 lands regex = "1" as a runtime
dep on envoy-config narrowly scoped to header / route matching at config-load
time; cargo deny check stays clean (Unlicense added to allow-list for the
aho-corasick + memchr dual-license). Hand-rolled Deserialize impls model the
field-name oneof shape (Envoy's HeaderMatcher proto uses field-name
discrimination, not @type tagged-union — different from phase 03.1's
TransportSocketTypedConfig). SafeRegex compiles at validate time into
Arc<regex::Regex>; non-parseable patterns surface as
ConfigError::InvalidRegex. envoy-config's validate signature is now &mut
Bootstrap to support the compile-pass mutation; parse_bootstrap absorbs it
internally. envoy-config::matcher module exposes
HeaderMatcher::matches(headers) -> bool + StringMatcher::matches(value) ->
bool; multi-matcher AND semantics live in envoy-http1::hcm::route_matches per
Envoy default headers_match_options: ALL. ~10 new envoy-config validator
tests + ~28 matcher-runtime unit tests + 5 HCM route-walker tests + 1 new
fuzz seed (route_with_header_matchers). Differential harness gains
Driver::Http1ProbeList + Http1Probe types + extra_headers parameter on
drive_http1; existing single-probe Driver::Http1 callsites pass &[]. Fixture
0007 (landed in 04.1) gains a second route demonstrating production matcher
use: prefix /api/ + X-Foo: bar selects a 418 teapot; the existing GET
/healthz probe still falls through to the 200 default route. Both proxies
select the same route on each probe.

Differential surface: tests/fixtures/0001-tcp-echo green (unchanged);
  tests/fixtures/0002-static-admin-ready green (unchanged);
  tests/fixtures/0003-tcp-proxy green (unchanged);
  tests/fixtures/0004-tls-downstream green (unchanged);
  tests/fixtures/0005-tls-upstream green (unchanged);
  tests/fixtures/0006-tls-sni green (unchanged);
  tests/fixtures/0007-http1-direct-response green (HTTP/1.1 listener;
  direct_response route action; matcher fan-out exercised via the amended
  matcher-bearing route — both proxies select the same route given the same
  request).
Conformance: none.
```

The state-6 commit also flips:
- `docs/envoy-rust/ROADMAP.md` row `04.2` `status` → `done`. (Row `04` parent stays `in-progress`; flips at 04.3's final commit per the schema invariant.)
- `docs/envoy-rust/STATE.md` → active id `04.3`, slug `04.3-router-upstream`, lifecycle state 2 (SPEC.md exists from parent-04 state-2 commit `1d9740d`, PLAN.md does not), next-skill `superpowers:writing-plans`.
- Appends a final State-6 section to PROGRESS.md.
