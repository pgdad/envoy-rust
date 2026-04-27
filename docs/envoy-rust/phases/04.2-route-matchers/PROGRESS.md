# Phase 04.2 Progress

## Task 1 — ADR-0021 (regex permitted) + envoy-config Cargo dep + 4 ConfigError variants stub (2026-04-27)

- Commit: 984aedded536d5cce0114f9fbeb814bb0846a337
- Change: appended ADR-0021 to docs/envoy-rust/DECISIONS.md (regex = "1" narrowly permitted as a foundation for header / route matching at config-load time); added `regex = "1"` to crates/envoy-config/Cargo.toml [dependencies]; added `Unlicense` to deny.toml [licenses] allow list (transitive aho-corasick + memchr are MIT/Unlicense dual-licensed); added 4 ConfigError variants in lib.rs (EmptyHeaderName, InvalidRegex, InvalidInt64Range, UnknownHeaderMatcherMode); Cargo.lock updated with regex + regex-syntax + aho-corasick + memchr entries.
- Verification: `cargo build --workspace --all-targets` → clean; `cargo deny check` → advisories ok, bans ok, licenses ok, sources ok; `cargo clippy --workspace --all-targets --all-features -- -D warnings` → clean; `cargo fmt --all -- --check` → clean; `cargo test -p envoy-config --lib` → 75 passed (unchanged from 04.1 close).
- Tests added: none in Task 1.
- ADRs: ADR-0021 (this task). ADR ledger head: 21.
- Deviations from PLAN:
  1. **Cargo.lock landed inline at Task 1 rather than at the state-4 dedicated-commit cadence.** ADR-0021's Consequences section (now landed and append-only per D-3.5) states the lock will sync "as a dedicated commit at the 04.2 state-4 phase-done gate per established phase precedent." Reality: PLAN.md Task 1 Step 11's `git add ... Cargo.lock` stages the lock inline, so it landed in commit `984aedde` alongside the ADR + Cargo.toml + deny.toml + lib.rs changes. The PLAN itself anticipates this branch — Task 12 Step 2 says "If Cargo.lock has been clean since Task 1 — possible if Task 1 already committed it inline — skip this step and document in PROGRESS that the inline commit at Task 1 satisfied the sync, deviating from the M5-recommended dedicated cadence. Either path is doctrine-conformant." Per D-3.5 ADR-0021 itself remains unedited; this PROGRESS note is the audit trail. The 04.1 phase used the same inline-at-scaffold cadence (per 04.1 PROGRESS Task 4 / Task 17 + 04.1 REVIEW M5).

## Task 2 — envoy-config schema: Int64Range + SafeRegex (2026-04-27)

- Commit: d6f034450a8ab102919ef6a6fcc8cd209b83bbd1
- Change: appended Int64Range (i64 half-open range struct, deny_unknown_fields) and SafeRegex (regex: String + non-serde compiled: Option<Arc<regex::Regex>>) types after DirectResponse in bootstrap.rs. SafeRegex carries a hand-rolled `impl<'de> Deserialize<'de>` that reads only `regex: String`, rejects any other key, and sets compiled: None (validator extension in Task 5 will fill compiled). SafeRegex's custom PartialEq compares only the regex String — matches SPEC §6 signpost 17 (regex::Regex has no stable equality).
- Tests added (4): parses_int64_range, rejects_unknown_field_in_int64_range, parses_safe_regex, safe_regex_partial_eq_compares_only_regex_string.
- Verification: `cargo test -p envoy-config --lib` → 79 passed; clippy + fmt + build clean.
- Deviations: none.

### Task 2 follow-up — review fix (2026-04-27)

- Commit: 5a6b950db5da2191e220e20ac4f7422b506313b0
- Change: amended the `SafeRegex` Deserialize doc-comment in bootstrap.rs to replace the misleading "`#[serde(skip)]` would leave the field absent" justification with the accurate template-setting rationale (Tasks 3+4 reuse the visitor pattern for field-name oneof discrimination on StringMatcher / HeaderMatcher, where `#[serde(untagged)]` + `#[serde(tag)]` are both wrong; the two-phase init contract is named explicitly). Comment-only change; no semantic impact on Task 2 code.
- Verification: gate commands clean (build / clippy / fmt / test / deny all exit 0; 79 tests passing — unchanged from Task 2).

## Task 3 — envoy-config schema: StringMatcher + StringMatcherMode (2026-04-27)

- Commit: 90dcb47afdc076bcb1e4537a1fe80803fba50fd0
- Change: appended StringMatcher (mode + ignore_case) and StringMatcherMode (5 variants: Exact, Prefix, Suffix, SafeRegex, Contains) types after SafeRegex in bootstrap.rs. StringMatcher carries a hand-rolled `impl<'de> Deserialize<'de>` for the field-name oneof: collects all keys; allows at most one mode key; accepts ignore_case as a peer (default false); rejects unknown keys via M::Error::unknown_field. Added the 5th ConfigError variant UnknownStringMatcherMode in lib.rs (sibling of the 4 added in Task 1).
- Tests added (5): parses_string_matcher_exact, parses_string_matcher_contains_with_ignore_case, parses_string_matcher_safe_regex, rejects_unknown_string_matcher_mode_key, rejects_two_string_matcher_mode_keys.
- Verification: `cargo test -p envoy-config --lib` → 84 passed; clippy + fmt + build clean.
- Deviations: two test assertions were written as `assert_eq!(sm.ignore_case, false/true)` and had to be updated to `assert!(!sm.ignore_case)` / `assert!(sm.ignore_case)` to satisfy `clippy::bool_assert_comparison` (-D warnings). The PLAN's test code used assert_eq! with literal bools; functionally identical after fix.

### Task 3 follow-up — review fix (2026-04-27)

- Commit: 3d9f985815649e7ea4c021236a9804f09905cb1f
- Change: tightened the `rejects_two_string_matcher_mode_keys` assertion to verify the error message contains "multiple mode keys" or "mutually exclusive" (was only checking is_err()). Closes the assertion gap noted in code quality review — a future error-shape regression would now be caught. Mirrors the existing `rejects_unknown_string_matcher_mode_key` assertion style. Note: the plan specified `.err().expect()` but clippy::err_expect (-D warnings) requires `.expect_err()` instead; semantically identical.
- Verification: `cargo test -p envoy-config rejects_two_string_matcher_mode_keys` → 1 passed; full crate test 84 passed; all gate commands clean.

## Task 4 — envoy-config schema: HeaderMatcher + RouteMatch.headers (2026-04-27)

- Commit: 6237b0a
- Change: appended HeaderMatcher (name + mode + invert_match) and HeaderMatcherMode (7 variants: ExactMatch, PrefixMatch, SuffixMatch, SafeRegexMatch, RangeMatch, PresentMatch, StringMatch) types after StringMatcher in bootstrap.rs. HeaderMatcher carries a hand-rolled `impl<'de> Deserialize<'de>` for the field-name oneof: collects all keys; uses an inline `set_mode` helper to short-circuit on multi-mode-key collision; accepts invert_match as a peer (default false); rejects unknown keys via M::Error::unknown_field. Added `headers: Vec<HeaderMatcher>` field with #[serde(default)] to RouteMatch; added Clone derive on RouteMatch for matcher-runtime test ergonomics. Re-exported HeaderMatcher, HeaderMatcherMode, SafeRegex, Int64Range, StringMatcher, StringMatcherMode from lib.rs's pub use bootstrap{...} list. Extended all 6 RouteMatch literal sites in crates/envoy-http1/src/hcm.rs to include `headers: vec![]` or `headers: r.r#match.headers.clone()` — minimum compile-cascade fix; matching logic extension stays in Task 7.
- Tests added (6): parses_header_matcher_exact, parses_header_matcher_with_invert_match_true, parses_header_matcher_present_match_true, parses_header_matcher_string_match_contains, rejects_unknown_header_matcher_mode_key, parses_route_match_with_headers_vec_and_invert_match_default.
- Verification: `cargo test -p envoy-config --lib` → 90 passed; `cargo build --workspace --all-targets` clean (envoy-http1 hcm.rs updated for all 6 RouteMatch literal sites — 1 in clone_route_config + 5 in tests); clippy + fmt clean.
- Deviations: PLAN mentioned only the `clone_route_config` site in hcm.rs, but the compile cascade affected 5 additional RouteMatch literal sites in the test section of hcm.rs (lines 353, 427, 464, 478, 527). All 6 sites were patched with `headers: vec![]` / `.headers.clone()` as appropriate. Additionally, rustfmt reformatted one long `assert_eq!` line in the new test (`parses_route_match_with_headers_vec_and_invert_match_default`) into multi-line form; `cargo fmt --all` applied to satisfy the fmt gate.

### Task 4 follow-up — review fix (2026-04-27)

- Commit: 17f991ac1401328f1e551276412b0939e473ff7b
- Change: addressed 2 Important findings from code quality review:
  1. Added `rejects_two_header_matcher_mode_keys` test exercising `set_mode`'s guard branch (the schema-layer test for the multi-mode-key collision invariant; mirrors Task 3's `rejects_two_string_matcher_mode_keys`).
  2. Introduced `MODE_KEYS` const inside HeaderMatcher's Deserialize impl (sibling to ALL_KEYS) and refactored the missing-mode error to use `format!("...{MODE_KEYS:?}")` — matches StringMatcher's pattern and prevents the error string from drifting if a future variant is added.
- Verification: `cargo test -p envoy-config --lib` → 91 passed (was 90; +1); all gate commands clean.

## Task 5 — envoy-config validator: regex compile + Int64Range bounds + matcher walk (2026-04-27)

- Commit: 18a3313
- Change: extended `validate_hcm` (now `&mut`-signature) to walk `route.r#match.headers` Vec per route. Added `validate_header_matcher` helper (rejects empty name; dispatches by mode; range validates start < end; SafeRegex compile-pass via `compile_safe_regex` helper). Added `compile_safe_regex` helper that wraps `regex::Regex::new(&sr.regex)` and stores the compiled regex back on `safe_regex.compiled = Some(Arc::new(re))`. Switched `validate` and `validate_hcm` from `&Bootstrap`/`&HttpConnectionManagerConfig` to `&mut` to allow the compile-pass mutation; HCM dispatch arm in `validate` switched from `as_ref()` to `as_mut()`. `parse_bootstrap` absorbs the `&mut` internally; envoy-bin needs no change. `parse_then_validate` test helper updated. The TCP_PROXY_FILTER arm in `validate` was refactored to clone the cluster name before the immutable cluster lookup to satisfy the borrow checker (the `&mut listeners` loop held a mutable borrow on `bootstrap`; looking into `bootstrap.static_resources.clusters` requires resolving the borrows correctly).
- Tests added (10): rejects_empty_header_name, rejects_invalid_regex_in_safe_regex_match, rejects_invalid_regex_in_string_match_safe_regex, rejects_invalid_int64_range_start_eq_end, rejects_invalid_int64_range_start_gt_end, validator_compiles_safe_regex_match_into_arc, validator_accepts_all_seven_modes, validator_accepts_empty_headers_vec, validator_accepts_invert_match_true, validator_compiles_string_match_safe_regex_into_arc.
- Verification: `cargo test -p envoy-config --lib` → 101 passed; `cargo build --workspace --all-targets` clean (envoy-bin absorbs the &mut transparently); clippy + fmt clean.
- Deviations: The borrow checker required a small structural change in `validate`'s TCP_PROXY_FILTER arm: `tp.cluster.clone()` is extracted into `cluster_name` before the mutable listener borrow is active, and the cluster lookup uses `bootstrap.static_resources.clusters.iter()` directly (rather than the `clusters` alias that existed in the old code). This is semantically identical but necessary because the `&mut bootstrap.static_resources.listeners` loop borrow and a simultaneous `&bootstrap.static_resources.clusters` access would conflict under two-phase borrow rules. The PLAN did not anticipate this specific borrow conflict; the fix preserves the original check semantics exactly.

### Task 5 follow-up — review fix (2026-04-27)

- Commit: 48e615c
- Change: added a 3-line comment block to the TCP_PROXY_FILTER arm in `validate` clarifying that the `as_ref()` borrow is intentional (TCP_PROXY validation is read-only) — closes the asymmetry-readability gap with the HCM_FILTER arm's `as_mut()` (introduced in this Task 5). Comment-only change.
- Verification: gate commands clean; 101 tests still passing.

## Task 6 — envoy-config::matcher runtime + 28 matcher tests (2026-04-27)

- Commit: dfac122
- Change: created crates/envoy-config/src/matcher.rs with `impl HeaderMatcher::matches(&self, headers: &[(String, String)]) -> bool` and `impl StringMatcher::matches(&self, value: &str) -> bool`. Header name lookup uses eq_ignore_ascii_case (HTTP/1.1 §3.2). XOR with invert_match. SafeRegex variants take `safe_regex.compiled.as_ref().expect("validator ensured compiled")`. StringMatcher.ignore_case affects Exact/Prefix/Suffix/Contains (case-folded comparison) but not SafeRegex (Envoy proto: regex callers use `(?i)`). Half-open i64 range. Non-parseable RangeMatch values fail the match (not an error). present_match: false is "no presence requirement" (always true) per SPEC §6 signpost 7. Added pub mod matcher; to lib.rs.
- Tests added (28): per-mode boolean truth tables (3 each for ExactMatch / PrefixMatch / SuffixMatch / SafeRegexMatch; 5 for RangeMatch boundary; 4 for PresentMatch; 3 for StringMatch); cross-cuts (header name case-insensitivity; header value case-sensitivity by default; invert_match for ExactMatch + PresentMatch).
- Verification: `cargo test -p envoy-config --lib` → 129 passed; clippy + fmt + build clean.
- Deviations: `cargo fmt` reformatted several lines in matcher.rs (SuffixMatch arm collapsed to single line; Prefix ignore_case joined to one line; Contains ignore_case chain split differently; 8 test `hm(...)` calls split to multi-line). Semantically identical; `cargo fmt --all` applied before committing to satisfy the fmt gate.

### Task 6 follow-up — review fix (2026-04-27)

- Commit: 81c6dde
- Change: addressed 3 findings from code quality review of the matcher runtime:
  1. **I1 (panic safety):** replaced bare `str` byte-slice indexing in `StringMatcher::matches` Prefix/Suffix `ignore_case: true` branches with `str::get(..)` + `.is_some_and(...)` — would have panicked on multi-byte UTF-8 input where `lit.len()` falls mid-codepoint. Defensive fix; prevents trivial panics from non-ASCII header values once Task 7 wires this into the HCM hot path.
  2. **I2 (panic message disambiguation):** the two `expect("validator ensured compiled")` sites now read "validator ensured HeaderMatcher SafeRegex compiled" and "validator ensured StringMatcher SafeRegex compiled" so a future regression panic localizes to the correct call site without symbol information.
  3. **M2 (test coverage gap):** added `string_match_prefix_with_ignore_case_matches` and `string_match_suffix_with_ignore_case_matches` tests — these exercise the Prefix/Suffix branches in `StringMatcher::matches` (only reachable through `HeaderMatcherMode::StringMatch`), which previously had no direct coverage and now serve as regression tests for the I1 fix.
- Verification: `cargo test -p envoy-config --lib` → 131 passed (was 129; +2); all 5 gate commands clean.

## Task 7 — envoy-http1::hcm route walker integration + 5 HCM tests (2026-04-27)

- Commit: 7abe78d
- Change: extended `route_matches` signature in `crates/envoy-http1/src/hcm.rs` from `(r, path)` to `(r, path, headers: &[(String, String)])` and AND-combined `r.r#match.headers.iter().all(|m| m.matches(headers))` after the existing path-side oneof match — short-circuits on first non-match per Envoy default `headers_match_options: ALL`. Updated the single call site in `build_response` to pass `&req.headers` (rustfmt expanded to method-chain style). The matcher's inherent `HeaderMatcher::matches` method (from envoy_config::matcher, landed in Task 6) is reachable without explicit import since no new trait is involved — only an inherent method call on a type already in scope via `use envoy_config::Route`.
- Tests added (5): route_with_no_headers_matches_unchanged (regression baseline — empty headers Vec is a no-op, path matching unchanged), single_header_matcher_route_selected_when_match, single_header_matcher_route_skipped_when_no_match, multi_header_matcher_and_combination_all_match, multi_header_matcher_and_combination_one_fails. Added `build_test_config(routes: Vec<Route>) -> Arc<HCMConfig>` test helper alongside the existing `hcm_config_single_route` helper (not a duplicate — `build_test_config` accepts an arbitrary routes Vec, enabling the multi-route matcher tests).
- Verification: `cargo test -p envoy-http1` → 24 passed (was 19; +5); `cargo test -p envoy-config` → 131 passed (unchanged); clippy + fmt + build clean.
- Deviations: rustfmt required the call site in `build_response` to be expanded to method-chain style (`.routes / .iter() / .find(...)`) rather than the single-line form shown in the PLAN step — semantically identical, fmt gate enforced the reformat. Two byte-string literals in tests were also wrapped to two lines by rustfmt to satisfy line-length limits.

### Task 7 follow-up — review fix (2026-04-27)

- Commit: 8330a86
- Change: appended `Connection: close\r\n` to each of the 5 new HCM test request literals (route_with_no_headers_matches_unchanged, single_header_matcher_route_selected_when_match, single_header_matcher_route_skipped_when_no_match, multi_header_matcher_and_combination_all_match, multi_header_matcher_and_combination_one_fails). Without it, the server-side serve_connection loop kept the connection open until the 5s idle timeout fired before the test's `read_to_end()` returned, costing 5s per test in isolation. Aligns the 04.2 tests with the established 04.1 cadence (every 04.1 test in hcm.rs sends Connection: close).
- Verification: all 5 new tests pass in <1s each (was ~5s); 24 envoy-http1 tests passing; gate clean.

## Task 8 — fuzz corpus extension (route_with_header_matchers seed) (2026-04-27)

- Commit: 132d55f
- Change: created crates/envoy-config/fuzz/corpus/parse_bootstrap/route_with_header_matchers.yaml exercising 5 of the 7 HeaderMatcher modes simultaneously (exact_match, safe_regex_match, range_match, present_match, string_match-with-contains-and-ignore_case) inside a single Route's headers Vec. Added the corresponding allow-list entry to crates/envoy-config/fuzz/.gitignore. Extended bootstrap.rs::tests::fuzz_corpus_seeds_parse_or_reject_cleanly's parse-Ok list with the new seed.
- Verification: `cargo test -p envoy-config fuzz_corpus_seeds_parse_or_reject_cleanly` → PASS; `cargo test -p envoy-config --lib` → 131 passed (unchanged — corpus-walk test was already counted; only its enumeration grew); clippy + fmt + build clean.
- Deviations: none.

## Task 9 — differential harness: Driver::Http1ProbeList + Http1Probe + drive_http1 extra_headers (2026-04-27)

- Commit: 42b96f3
- Change: added Http1Probe struct (name, method, path, host, extra_headers, expected_*) and Driver::Http1ProbeList { probes: Vec<Http1Probe> } variant on the Driver enum (mirrors TlsTcpProbeList shape from 03.2). Extended drive_http1 signature with extra_headers: &[(String, String)] parameter; existing single-probe Driver::Http1 callsites pass &[]. Added Driver::Http1ProbeList dispatch arm in run_fixture iterating probes and applying per-probe equivalence cascade; subject.shutdown + drop(upstream) move to AFTER the probe loop. Extended the listener-port substitution arm to include Http1ProbeList.
- Tests added (2): parses_expectations_with_http1_probe_list, http1_probe_extra_headers_default_empty.
- Verification: `cargo test -p differential --lib` → 49 passed (was 47; +2); clippy + fmt + build clean.
- Deviations: rustfmt required Http1ProbeList variant to be expanded to multi-line form (`{ probes: Vec<Http1Probe>, }` across 3 lines) rather than the single-line form in the PLAN — fmt gate caught and fixed before commit.

## Task 10 — fixture 0007 amendment (matcher route) (2026-04-27)

- Commit: 64e269f
- Change: amended tests/fixtures/0007-http1-direct-response/{envoy.yaml,envoy-rust.yaml} to add a 04.2 NEW route at the head of routes: with `match: { prefix: "/api/", headers: [{ name: "x-foo", exact_match: "bar" }] }` returning direct_response 418 "teapot\n"; existing `prefix: "/"` catch-all stays second per first-match-wins discipline. Created tests/fixtures/0007-http1-direct-response/inputs/payload-matcher.bin (empty file; placeholder per 04.1 payload.bin convention). Restructured expectations.yaml from single-Driver::Http1 to Driver::Http1ProbeList with two probes (default-route, matcher-route). Appended a "04.2 amendment — header-matcher route" section to README.md and added ADR-0021 to the ADR references list.
- Verification: `cargo test -p envoy-bin --test http1_direct_response` → PASS (in-process backstop: GET /healthz still falls through to default route 200 OK); `cargo test -p differential --test http1_direct_response` → Docker not available (Socket not found: /var/run/docker.sock) — environment issue, not a fixture problem; workspace gate (build/clippy/fmt/test --lib --bins) clean.
- Deviations: none.

## Task 11 — 04.1 REVIEW M-track carryforward check (2026-04-27)

Per SPEC §3 D4 + the standing posture from STATE.md "Phase-04.1 rollovers": none of M1–M7 are critical-path for 04.2; all defer per their established annotations.

- M1 (`diff_headers` duplicate-header semantics): A. No action in 04.2 — fixture 0007 amendment introduces no duplicate response headers; track forward.
- M2 (body-drain idle timeout silent close): A. No action — matcher route returns direct_response (no body-drain); track forward to 04.3 or hardening.
- M3 (envoy-cluster path-dep with no 04.1 consumer): A. No action — 04.2 adds no cluster consumer; track forward to 04.3.
- M4 (strip_port IPv6 correctness): A. No action — 04.2 fixture uses `Host: envoy-rust.test` (not IPv6); track forward.
- M5 (Cargo.lock sync cadence): partially addressed via Task 1 review-fix's PROGRESS disclosure. PLAN Step 11 of Task 1 stages Cargo.lock inline at the ADR-0021-bearing commit, deviating from the M5-recommended dedicated-tail-commit cadence; ADR-0021's Consequences section's "dedicated state-4 commit" prose is now contradicted but per D-3.5 the ADR text is append-only and remains untouched. The Task 1 review-fix's PROGRESS note is the audit trail. Task 12 (state-4 gate) will likely show Cargo.lock clean (already synced); if dirty, that's a fresh sync at state-4 per the established phase-precedent.
- M6 (drive_http1 per-function unit test): A. No action in 04.2 for M6 specifically — Task 9's tests cover Http1Probe parsing + extra_headers default but not drive_http1 itself; track forward to 04.3.
- M7 (TlsAcceptingHandler generalization for HCM+TLS): A. No action — 04.2 introduces no TLS-bearing HCM fixtures; track forward to phase 05+.

No code changes in this task. All 7 M-track items (other than M5's partial address above) remain on the carryforward ledger for 04.3 / phase 05 / hardening pass.

## Task 12 / State 4 — phase-done gate verification (2026-04-27)

Per `docs/envoy-rust/SKILL_ROUTING.md` state 4: the local stable-toolchain gate ran clean on first attempt. ROADMAP.md and STATE.md are NOT advanced here per `BOOTSTRAP_PROMPT.md` §5.1 (one state per session); those flip in state 6 (the phase-done commit) after state 5's `REVIEW.md` is approved.

### State-2 PLAN.md late-landing

PLAN.md was committed at `160caf0` immediately before the state-4 gate as a dedicated commit `phase 04.2: state-2 PLAN.md (late-landing per 04.1 inline-at-Task-1 precedent)`. Mirrors the 04.1 pattern of PLAN.md landing inline rather than at a clean state-2 close-out (PROGRESS Task 1 of 04.1 documents the same well-disclosed deviation). Reviewer-of-state-5 may flag this as a process consistency concern.

### Local stable-toolchain gate

`cargo build --workspace --all-targets`:
```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.09s
```

`cargo clippy --workspace --all-targets --all-features -- -D warnings`:
```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.10s
```

`cargo fmt --all -- --check`:
```
(no output — clean)
```

`cargo test --workspace --lib --bins`:
```
running 49 tests        (differential lib: 48 passed; 1 ignored — pre-existing 02.2 TcpProxyBackend smoke)
running 19 tests        (envoy-bin: 19 passed)
running 8 tests         (envoy-cluster: 8 passed)
running 131 tests       (envoy-config: 131 passed)
running 24 tests        (envoy-http1: 24 passed)
running 6 tests         (envoy-listener: 6 passed)
running 11 tests        (envoy-tcp: 11 passed)
running 15 tests        (envoy-tls: 15 passed)
running 8 tests         (tcp-echo-server: 8 passed)
running 5 tests         (tls-echo-server: 5 passed)
```

Total: 275 passed, 0 failed, 1 ignored. Test count delta from 04.1 close (212 + 1 ignored): +63 (envoy-config 75→131 = +56, envoy-http1 19→24 = +5, differential 47→49 = +2).

`cargo deny check`:
```
advisories ok, bans ok, licenses ok, sources ok
```

(Pre-existing `license-not-encountered` warnings for `Unicode-DFS-2016` and `Zlib` preserved; not 04.2 regressions.)

### Cargo.lock sync

Clean — no diff at state-4. `Cargo.lock` was synced inline at Task 1 (commit `984aedde`) when ADR-0021's `regex = "1"` runtime dep landed (per PLAN Task 1 Step 11's explicit `git add ... Cargo.lock` instruction). Task 1's review-fix at `def3046` documented the discrepancy with ADR-0021's prose ("dedicated state-4 commit") for D-3.5 audit — ADR text is append-only and not editable. The 04.2 state-4 gate confirms the inline sync was sufficient: no additional Cargo.lock changes accumulated during Tasks 2-11. Mirrors 04.1's same inline-at-scaffold cadence (per 04.1 PROGRESS Task 4 / Task 17 + 04.1 REVIEW M5).

### CI

Local push + `gh run watch` deferred to commit-time; CI exercises the same gate plus the Docker-gated `http1_direct_response_fixture` (now exercising both probes per Task 10's amendment) and the fuzz job (now picks up `route_with_header_matchers.yaml` per Task 8). Reviewer-of-state-5 cross-checks the CI run results.

### Outstanding for state 5/6

State 5 (`superpowers:requesting-code-review`) writes `REVIEW.md` for this phase. State 6 (the phase-done commit) flips ROADMAP row `04.2` `status` → `done` (parent row `04` stays `in-progress` until 04.3 lands per the schema invariant) and advances STATE.md to phase `04.3-router-upstream` (lifecycle state 2; SPEC.md exists from the parent-04 state-2 split commit `1d9740d`, PLAN.md does not; next-skill `superpowers:writing-plans`).
