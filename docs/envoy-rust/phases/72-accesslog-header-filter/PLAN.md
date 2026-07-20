# Phase 72 — access-log `header_filter` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Every task is TDD: write the failing test → run it RED → implement → run it GREEN → commit. Do NOT skip the RED step.

**Goal:** Land the third `AccessLogFilter` oneof arm, `header_filter` (`envoy.config.accesslog.v3.AccessLogFilter.header_filter`), gating a per-sink access-log record on whether a named REQUEST HEADER matches a `HeaderMatcher`, behaviorally equivalent to `envoyproxy/envoy:v1.33.0`.

**Architecture:** Reuse the phase-04.2 `HeaderMatcher` config type (`bootstrap.rs:3022`) + its 7-mode `matches(&[(String,String)])` engine (`matcher.rs:21`) VERBATIM. Add a `HeaderFilter { header: HeaderMatcher }` schema + the `header_filter: Option<HeaderFilter>` oneof arm; extend the compiler-forcing `set_arms` cardinality destructure + the `compile_access_log_filter` tuple match from 2 to 3 arms; add a `LogFilter::Header { matcher }` runtime variant and WIDEN `should_log` a SECOND time to carry the request-header slice (the one genuinely-new seam); thread the already-in-scope `req.headers` / `envoy_req.headers` at BOTH HCM emit gates. Witness byte-exact by new fixture `0078`.

**Tech Stack:** Rust (workspace crates `envoy-config`, `envoy-accesslog`, `envoy-http1`, `envoy-http2`, `differential`), `serde`/`serde_yaml`, the `regex` crate (via the existing `SafeRegex` path), `testcontainers` differential harness, `cargo fuzz` (libfuzzer).

## Global Constraints

- `#![forbid(unsafe_code)]` at every crate root (D-3.8). No new `unsafe`.
- Reference pin is `envoyproxy/envoy:v1.33.0` (D-3.7). Never change the pin.
- `cargo build -p envoy-bin` (the DEBUG binary) BEFORE any local differential run — the harness runs `target/debug/envoy-bin` and REDs on a stale binary with a new config key (memory `differential-harness-uses-debug-envoy-bin`).
- ADR-0049: config-validity divergences are ALL fail-loud (envoy-rust refuses to BOOT; runtime behavior never silently differs). Error-message byte-parity with upstream is WAIVED (native `ConfigError` messages are fine).
- `#[serde(deny_unknown_fields)]` on every config struct; `ConfigError` is grow-only (append variants, never renumber/repurpose).
- Never weaken a fixture; never trim `known-failures.txt` (memory `h2spec-3-5-2-preface-host-sensitive`). Do NOT disturb the 31 existing access-log fixtures (incl. `0076`/`0077`).
- ROADMAP row edits preserve 6 cells + escape `\|`; rows `36/38/39/52/54` are already malformed — do NOT "fix" them (append-only).
- `next-prompt.txt` is gitignored — never `git add` it. `DECISIONS.md` is NOT chronological. `envoy-bin` writes `ConfigError` to STDOUT.
- The documented host-flake set is CI-authoritative, never a regression; adjudicate REDs with `--no-fail-fast` + full-output redirect, never `tail` (memory `never-pipe-verification-runs-through-tail`).

**Measured facts this plan rests on** (state-2 §6.2 reconciliation, ADR-0149; live `envoyproxy/envoy:v1.33.0`):

- `HeaderFilter = { header: HeaderMatcher (PGV-required) }`. Empty `header_filter: {}` is REJECTED upstream (`HeaderFilterValidationError.Header: value is required`); envoy-rust rejects it fail-loud via the required serde field → `ConfigError::Yaml` (no new variant).
- `header_filter`, `status_code_filter`, `response_flag_filter` are mutually-exclusive oneof arms (exactly one).
- Runtime keep/drop (backend-free `direct_response`, graceful-stop flush): a record is KEPT iff the named request header matches; present-mismatch AND absent both DROP. Confirmed byte-exact single line `STATUS=200 PATH=/x H=yes`.
- **PV-4 (MEASURED both paths):** upstream ROUTE header matching AND access-log `header_filter` BOTH drop absent+`invert_match` (route: `GET /` with no header + inverted matcher → 202 fallback, not the 201 inverted route; access-log: absent+invert → not logged). The in-tree shared engine's `mode_result ^ invert_match` (`matcher.rs:51`) yields absent+invert = KEEP → a **latent SHARED bug on BOTH the route and access-log paths** (not access-log-specific). Scoped OUT of phase 72; documented; owner is new carry-forward **CF-72-1**.
- **PV-5 (MEASURED):** upstream accepts name-only `header: { name }` (presence match) and `treat_missing_header_as_empty: true`; the in-tree `HeaderMatcher` deserializer REJECTS both (name-only → "missing mode key" at `bootstrap.rs:3175`; `treat_missing_header_as_empty` → unknown field at `bootstrap.rs:3168`). Inherited phase-04.2 boundaries, kept fail-loud per ADR-0049; owner is new carry-forward **CF-72-2**. The opener uses explicit `string_match: { exact }`.
- `should_log` widening touches 2 production gates + ~45 test call sites (all enumerated in Task 4). `LogFilter` currently derives `Eq`; `HeaderMatcher` is only `PartialEq` (it holds `SafeRegex`/`StringMatcher`) → adding a `HeaderMatcher`-carrying arm forces DROPPING `Eq` from `LogFilter` (no `LogFilter: Eq` consumer exists — grep-confirmed).
- `validate_hcm(hcm: &mut HttpConnectionManagerConfig)` (`bootstrap.rs:3787`) is `&mut` and is the SOLE caller of `validate_access_logs(&hcm.access_log)` (`bootstrap.rs:3822`) → changing `validate_access_logs` to `&mut [AccessLog]` lets the header_filter arm compile its `SafeRegex` in place via the existing `validate_header_matcher(&mut hf.header)` (`bootstrap.rs:5367`).

## File Structure

- **`crates/envoy-config/src/bootstrap.rs`** — add `HeaderFilter` struct (near the `ResponseFlagFilter` at 779); add `header_filter: Option<HeaderFilter>` to `AccessLogFilter` (713-728); extend the `set_arms` destructure + add the header_filter validation delegation in `validate_access_logs` (5136-5182), change its signature to `&mut`; refresh its docstring (5109-5136); update the caller at 3822.
- **`crates/envoy-config/src/lib.rs`** — export `HeaderFilter` (the `pub use bootstrap::{...}` block at 14-22). No new `ConfigError` variant.
- **`crates/envoy-accesslog/src/filter.rs`** — add `LogFilter::Header { matcher }`; widen `should_log` (33-59); drop `Eq` from the `LogFilter` derive (23); update the ~22 in-crate test call sites.
- **`crates/envoy-accesslog/src/file_sink.rs`** — widen `FileSink::should_log` (99-107); update the 4 test call sites.
- **`crates/envoy-http1/src/hcm.rs`** — extend `compile_access_log_filter` (1735-1762) to a 3-tuple; thread `&req.headers` at the emit gate (1512); update the ~19 test call sites.
- **`crates/envoy-http2/src/hcm.rs`** — thread `&envoy_req.headers` at the emit gate (1135). (Shared compile via `config.inner` — no H2 compile change.)
- **`tests/differential/src/lib.rs`** — add an ordering-aware settle (CF-71-1) to both `run_http1_access_log_byte_exact_arm` (6249) and `run_http2_access_log_byte_exact_arm` (6426); fix the `CF70_3_SETTLE` doc phrase (1679).
- **`tests/differential/tests/access_log_response_flag_filter.rs`** — fix the stale "ordering witness" module doc (line 10).
- **`tests/differential/tests/access_log_header_filter.rs`** — CREATE (the fixture-0078 `run_fixture` wrapper).
- **`tests/fixtures/0078-accesslog-header-filter/`** — CREATE `envoy.yaml`, `envoy-rust.yaml`, `expectations.yaml`, `README.md`.
- **`crates/envoy-config/fuzz/corpus/parse_bootstrap/header_filter.yaml`** — CREATE (corpus seed); **`crates/envoy-config/fuzz/.gitignore`** — add one `!`-un-ignore line.
- **`docs/envoy-rust/BEHAVIOR_CONTRACT.md`** — add the `header_filter` subsection (after 2328); fix the §F "ordering witness" phrase (2319).

---

## Task 1: Config schema — `HeaderFilter { header: HeaderMatcher }` + the `header_filter` oneof arm

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (add `HeaderFilter` near 779; add the arm to `AccessLogFilter` at 713-728)
- Modify: `crates/envoy-config/src/lib.rs:14-22` (export `HeaderFilter`)
- Test: `crates/envoy-config/src/bootstrap.rs` (unit tests, in the existing `#[cfg(test)]` module)

**Interfaces:**
- Consumes: the existing `HeaderMatcher` (`bootstrap.rs:3022`, hand-rolled `Deserialize`/`Serialize`).
- Produces: `pub struct HeaderFilter { pub header: HeaderMatcher }`; `AccessLogFilter.header_filter: Option<HeaderFilter>`.

- [ ] **Step 1: Write the failing test** — a `header_filter` config parses into the new arm; an empty `header_filter: {}` is rejected.

Add to the `envoy-config` test module (near the other `AccessLogFilter` tests):

```rust
#[test]
fn header_filter_parses_into_the_arm() {
    let yaml = access_log_filter_yaml(
        r#"header_filter: { header: { name: "x-log", string_match: { exact: "yes" } } }"#,
    );
    let bs = crate::parse_bootstrap(&yaml).expect("header_filter must parse");
    let hcm = first_hcm(&bs);
    let f = hcm.access_log[0].filter.as_ref().expect("filter present");
    let hf = f.header_filter.as_ref().expect("header_filter arm set");
    assert_eq!(hf.header.name, "x-log");
    assert!(matches!(
        hf.header.mode,
        crate::HeaderMatcherMode::StringMatch(_)
    ));
}

#[test]
fn empty_header_filter_is_rejected() {
    let yaml = access_log_filter_yaml("header_filter: {}");
    let err = crate::parse_bootstrap(&yaml).expect_err("empty header_filter must reject");
    // A required `header` field is missing → serde surfaces `ConfigError::Yaml`
    // (fail-loud; ADR-0049 waives error-message byte-parity vs upstream's
    // `HeaderFilterValidationError.Header: value is required`).
    assert!(matches!(err, crate::ConfigError::Yaml(_)), "got {err:?}");
}
```

> If helpers `access_log_filter_yaml(arm: &str) -> String` and `first_hcm(&Bootstrap)` do not already exist in the test module, reuse the exact pattern the phase-70/71 tests use (grep `fn access_log_filter_yaml` / how `rejects_access_log_filter_with_both_arms` builds its YAML at bootstrap.rs:13020) and inline an equivalent local `format!` that embeds `{arm}` into a minimal HCM with a file access log. Do NOT invent a new fixture file.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p envoy-config header_filter_parses_into_the_arm empty_header_filter_is_rejected 2>&1 | tail -20`
Expected: FAIL — `header_filter` is an unknown field on the `deny_unknown_fields` `AccessLogFilter` (compile error `no field header_filter` / the field does not exist yet).

- [ ] **Step 3: Add the `HeaderFilter` struct and the oneof arm**

Add the struct near `ResponseFlagFilter` (bootstrap.rs ~779). Mirror `StatusCodeFilter` (required field, no `Default`, `deny_unknown_fields`):

```rust
/// Models `envoy.config.accesslog.v3.HeaderFilter` — the THIRD `AccessLogFilter`
/// arm (phase 72). Gates emission on whether a named REQUEST HEADER matches
/// `header`. REUSES the phase-04.2 `HeaderMatcher` verbatim. `header` is
/// PGV-required — an empty `header_filter: {}` is rejected fail-loud at
/// deserialize (missing field → `ConfigError::Yaml`), matching upstream's
/// `HeaderFilterValidationError.Header: value is required`. Mutually exclusive
/// with `status_code_filter` / `response_flag_filter` (cardinality enforced by
/// `validate_access_logs`).
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HeaderFilter {
    pub header: HeaderMatcher,
}
```

Add the arm to `AccessLogFilter` (after `response_flag_filter`) and refresh the type's doc comment to say "THREE oneof arms":

```rust
    /// Phase 72: the THIRD `AccessLogFilter` arm — gates emission on whether a
    /// named request header matches `header`. Mutually exclusive with the other
    /// two arms (cardinality enforced by `validate_access_logs`).
    pub header_filter: Option<HeaderFilter>,
```

Export it from `lib.rs` (add `HeaderFilter` to the `pub use bootstrap::{...}` list, alphabetically near `HeaderMatcher`).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p envoy-config header_filter_parses_into_the_arm empty_header_filter_is_rejected 2>&1 | tail -20`
Expected: both PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs
git commit -m "phase 72 T1: HeaderFilter schema + header_filter oneof arm [ADR-0149]"
```

---

## Task 2: 3-arm `set_arms` compiler-forcing destructuring + M71-1 / M71-4 folds

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (the destructure at 5146-5164; the docstring at 5109-5136; the test at 13020)
- Test: `crates/envoy-config/src/bootstrap.rs` (`rejects_access_log_filter_with_both_arms` + a new precedence test)

**Interfaces:**
- Consumes: `AccessLogFilter { status_code_filter, response_flag_filter, header_filter }`.
- Produces: an updated `validate_access_logs` whose cardinality check covers 3 arms; both `>1` and `==0` → `ConfigError::AmbiguousAccessLogFilter { detail }`.

- [ ] **Step 1: Write the failing tests** — (a) M71-1: the both-arms rejection asserts `detail`; (b) a header-inclusive two-arm pair rejects; (c) precedence: a both-arms + bad-`HeaderMatcher` config fails on CARDINALITY first.

Replace the body of `rejects_access_log_filter_with_both_arms` (13020) to assert `detail`, and add two tests:

```rust
#[test]
fn rejects_access_log_filter_with_both_arms() {
    let yaml = access_log_filter_yaml(
        r#"status_code_filter: { comparison: { op: GE, value: { default_value: 500, runtime_key: k } } }
                      response_flag_filter: { flags: ["NR"] }"#,
    );
    let err = crate::parse_bootstrap(&yaml).expect_err("both arms must be rejected");
    match err {
        crate::ConfigError::AmbiguousAccessLogFilter { detail } => {
            assert!(detail.contains("more than one"), "detail was {detail:?}");
        }
        other => panic!("expected AmbiguousAccessLogFilter, got {other:?}"),
    }
}

#[test]
fn rejects_header_filter_paired_with_another_arm() {
    let yaml = access_log_filter_yaml(
        r#"header_filter: { header: { name: "x-log", present_match: true } }
                      status_code_filter: { comparison: { op: GE, value: { default_value: 500, runtime_key: k } } }"#,
    );
    let err = crate::parse_bootstrap(&yaml).expect_err("two arms must be rejected");
    assert!(
        matches!(err, crate::ConfigError::AmbiguousAccessLogFilter { detail } if detail.contains("more than one")),
        "got {err:?}"
    );
}

#[test]
fn cardinality_is_checked_before_per_arm_validation() {
    // A both-arms config where the header_filter ALSO has an empty header name:
    // cardinality must fire FIRST (AmbiguousAccessLogFilter), not EmptyHeaderName.
    let yaml = access_log_filter_yaml(
        r#"header_filter: { header: { name: "", present_match: true } }
                      response_flag_filter: { flags: ["NR"] }"#,
    );
    let err = crate::parse_bootstrap(&yaml).expect_err("must reject");
    assert!(
        matches!(err, crate::ConfigError::AmbiguousAccessLogFilter { .. }),
        "cardinality must precede per-arm validation, got {err:?}"
    );
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p envoy-config rejects_access_log_filter_with_both_arms rejects_header_filter_paired_with_another_arm cardinality_is_checked_before_per_arm_validation 2>&1 | tail -30`
Expected: compile error (the destructure at 5150 lacks `header_filter` → `pattern does not mention field header_filter`), and the new tests fail.

- [ ] **Step 3: Extend the destructure + docstring**

In `validate_access_logs` (bootstrap.rs ~5146), extend the destructure and the `set_arms` array to 3 arms (the `no ..` pattern is what forced the compile error — that is intended):

```rust
            let AccessLogFilter {
                status_code_filter,
                response_flag_filter,
                header_filter,
            } = filter;
            let set_arms = [
                status_code_filter.is_some(),
                response_flag_filter.is_some(),
                header_filter.is_some(),
            ]
            .iter()
            .filter(|set| **set)
            .count();
            if set_arms != 1 {
                return Err(crate::ConfigError::AmbiguousAccessLogFilter {
                    detail: if set_arms == 0 {
                        "no filter variant is set".into()
                    } else {
                        "more than one filter variant is set".into()
                    },
                });
            }
```

> Note: `header_filter`'s per-arm validation (the `validate_header_matcher` delegation) is added in **Task 3**, AFTER this cardinality block — which is exactly the precedence `cardinality_is_checked_before_per_arm_validation` pins.

M71-4 fold — refresh the `validate_access_logs` docstring (bootstrap.rs 5109-5136). Change item 3 to stop calling the `>1` branch "unreachable", and add items for the response-flag token check and the header_filter matcher:

```rust
///   3. `AccessLog.filter` (`AccessLogFilter` oneof) cardinality: when a
///      `filter` is present it must set EXACTLY ONE arm. Zero arms (`filter: {}`)
///      and more-than-one arm BOTH surface as
///      `ConfigError::AmbiguousAccessLogFilter { detail }` (the `detail`
///      distinguishes the two). Phases 70/71/72 give three arms, so the
///      more-than-one branch is REACHABLE. Cardinality lives here (mirroring the
///      `SubstitutionFormatString` / `AmbiguousLogFormat` precedent), not serde.
///   4. Phase 70 — non-empty `status_code_filter.comparison.value.runtime_key`
///      (`ConfigError::EmptyStatusCodeFilterRuntimeKey`).
///   5. Phase 71 — every `response_flag_filter.flags` token is a known
///      response-flag token (`ConfigError::UnknownResponseFlag`).
///   6. Phase 72 — the `header_filter.header` HeaderMatcher validates + its
///      SafeRegex compiles, via `validate_header_matcher` (empty name →
///      `EmptyHeaderName`; bad regex → `InvalidRegex`; bad range →
///      `InvalidInt64Range`).
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p envoy-config rejects_access_log_filter_with_both_arms rejects_header_filter_paired_with_another_arm cardinality_is_checked_before_per_arm_validation 2>&1 | tail -20`
Expected: all PASS (Task 3 not yet done, but these three do not depend on the header_filter validation body — the destructure compiles because `header_filter` is now bound, and the `cardinality_is_checked_before_per_arm_validation` test relies only on the cardinality branch firing first).

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-config/src/bootstrap.rs
git commit -m "phase 72 T2: 3-arm set_arms destructure + M71-1 detail assert + M71-4 docstring [ADR-0149]"
```

---

## Task 3: `header_filter` validation — delegate to `validate_header_matcher` (`&mut` plumbing)

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (add the header_filter arm in `validate_access_logs` ~5178; change its signature to `&mut`; update the caller at 3822)
- Test: `crates/envoy-config/src/bootstrap.rs`

**Interfaces:**
- Consumes: `validate_header_matcher(hm: &mut HeaderMatcher) -> Result<(), ConfigError>` (bootstrap.rs:5367).
- Produces: `validate_access_logs(access_logs: &mut [AccessLog])`; the header_filter arm's SafeRegex is compiled in place; delegated `EmptyHeaderName` / `InvalidRegex` / `InvalidInt64Range`.

- [ ] **Step 1: Write the failing tests** — empty header name and bad regex in a `header_filter` are rejected with the delegated errors; a valid SafeRegex header_filter parses AND its regex is compiled.

```rust
#[test]
fn header_filter_empty_name_rejected() {
    let yaml = access_log_filter_yaml(
        r#"header_filter: { header: { name: "", present_match: true } }"#,
    );
    let err = crate::parse_bootstrap(&yaml).expect_err("empty header name must reject");
    assert!(matches!(err, crate::ConfigError::EmptyHeaderName), "got {err:?}");
}

#[test]
fn header_filter_bad_regex_rejected() {
    let yaml = access_log_filter_yaml(
        r#"header_filter: { header: { name: "x-log", safe_regex_match: { regex: "y(" } } }"#,
    );
    let err = crate::parse_bootstrap(&yaml).expect_err("bad regex must reject");
    assert!(matches!(err, crate::ConfigError::InvalidRegex { .. }), "got {err:?}");
}

#[test]
fn header_filter_safe_regex_is_compiled() {
    let yaml = access_log_filter_yaml(
        r#"header_filter: { header: { name: "x-log", safe_regex_match: { regex: "y.*" } } }"#,
    );
    let bs = crate::parse_bootstrap(&yaml).expect("valid regex must parse");
    let hcm = first_hcm(&bs);
    let hf = hcm.access_log[0].filter.as_ref().unwrap().header_filter.as_ref().unwrap();
    let crate::HeaderMatcherMode::SafeRegexMatch(sr) = &hf.header.mode else {
        panic!("expected SafeRegexMatch");
    };
    assert!(sr.compiled.is_some(), "validator must compile the SafeRegex");
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p envoy-config header_filter_empty_name_rejected header_filter_bad_regex_rejected header_filter_safe_regex_is_compiled 2>&1 | tail -20`
Expected: FAIL — without the delegation, an empty-name/bad-regex header_filter currently parses (no per-arm validation), and `sr.compiled` is `None`.

- [ ] **Step 3: Add the delegation + change the signature to `&mut`**

Change the signature (bootstrap.rs:5136):

```rust
fn validate_access_logs(access_logs: &mut [AccessLog]) -> Result<(), crate::ConfigError> {
```

Update the sole caller (bootstrap.rs:3822) — `validate_hcm` already has `&mut hcm`:

```rust
    validate_access_logs(&mut hcm.access_log)?;
```

Update the `for entry in ...` loop to iterate mutably (`for entry in access_logs.iter_mut()` or `for entry in access_logs` if it is a `&mut [AccessLog]`), and the `if let Some(filter) = &entry.filter` to `&mut entry.filter`. Then add the header_filter arm AFTER the existing `response_flag_filter` block (bootstrap.rs ~5178), BEFORE the `match &entry.typed_config`:

```rust
            if let Some(hf) = header_filter {
                // Phase 72: reuse the phase-04.2 HeaderMatcher validator verbatim
                // — empty name → EmptyHeaderName; bad regex → InvalidRegex; bad
                // range → InvalidInt64Range; compiles the SafeRegex in place so
                // the runtime `matches` never hits its `.expect()`.
                validate_header_matcher(&mut hf.header)?;
            }
```

> Because the destructure now binds `&mut` fields, the existing `status_code_filter` / `response_flag_filter` blocks keep working (`.is_some()`, `if let Some(scf) = status_code_filter` reading `scf.comparison...`, `&rff.flags` all compile through `&mut`). If the borrow checker objects to `header_filter.is_some()` after a partial move, take references in the `set_arms` array (`status_code_filter.as_ref().is_some()` is unnecessary — `Option::is_some` takes `&self`, so `header_filter.is_some()` on a `&mut Option<_>` auto-reborrows).

- [ ] **Step 4: Run to verify pass** (and no regression in the existing access-log validation tests)

Run: `cargo test -p envoy-config header_filter_ access_log 2>&1 | tail -30`
Expected: the three new tests PASS; every pre-existing `access_log*` / `status_code_filter` / `response_flag_filter` validation test still PASSES.

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-config/src/bootstrap.rs
git commit -m "phase 72 T3: header_filter validation delegates to validate_header_matcher (&mut) [ADR-0149]"
```

---

## Task 4: Runtime predicate — `LogFilter::Header` + widen `should_log` (2nd widening) + drop `Eq`

**Files:**
- Modify: `crates/envoy-accesslog/src/filter.rs` (enum at 23-31; `should_log` at 33-59; ~22 test call sites)
- Modify: `crates/envoy-accesslog/src/file_sink.rs` (wrapper at 99-107; 4 test call sites)
- Test: `crates/envoy-accesslog/src/filter.rs`

**Interfaces:**
- Consumes: `envoy_config::HeaderMatcher` (already a dependency? — `envoy-accesslog` must NOT gain an `envoy-config` dep per the ADR-0141 "compiled-config" posture; see Step 3 note).
- Produces: `LogFilter::Header { matcher }`; `fn should_log(&self, status: u16, response_flags: &str, headers: &[(String, String)]) -> bool`; `FileSink::should_log(&self, status, response_flags, headers)`.

- [ ] **Step 1: Write the failing test** — the `Header` arm keeps on match, drops on mismatch/absent, over the supported modes.

```rust
#[test]
fn header_filter_should_log_membership() {
    let m = |mode| LogFilter::Header { matcher: hm("x-log", mode) };
    let hdrs_yes = [("x-log".to_string(), "yes".to_string())];
    let hdrs_no = [("x-log".to_string(), "no".to_string())];
    let hdrs_absent: [(String, String); 0] = [];

    // exact:"yes" — kept on "yes", dropped on "no" and absent.
    let f = m(HeaderMatcherMode::ExactMatch("yes".into()));
    assert!(f.should_log(200, "-", &hdrs_yes));
    assert!(!f.should_log(200, "-", &hdrs_no));
    assert!(!f.should_log(200, "-", &hdrs_absent));

    // prefix:"ye" matches "yes"; present matches any value; safe_regex "y.*"
    assert!(m(HeaderMatcherMode::PrefixMatch("ye".into())).should_log(200, "-", &hdrs_yes));
    assert!(m(HeaderMatcherMode::PresentMatch(true)).should_log(200, "-", &hdrs_yes));
    assert!(!m(HeaderMatcherMode::PresentMatch(true)).should_log(200, "-", &hdrs_absent));
}
```

> `hm(name, mode)` builds a `HeaderMatcher` with `invert_match: false`. If the accesslog crate has no such helper, add a local test helper. For SafeRegex modes, the `compiled` Arc must be populated (call the config-side compile in the helper) — but the membership test above deliberately uses non-regex modes; SafeRegex membership is covered end-to-end in Task 9 through the real validate path.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p envoy-accesslog header_filter_should_log_membership 2>&1 | tail -20`
Expected: FAIL — `LogFilter::Header` does not exist; `should_log` has a 2-arg signature.

- [ ] **Step 3: Add the variant, drop `Eq`, widen `should_log`**

In `filter.rs`, drop `Eq` from the `LogFilter` derive (it holds a `HeaderMatcher` which is only `PartialEq`) and add the arm:

```rust
#[derive(Debug, Clone, PartialEq)]   // was: Debug, Clone, PartialEq, Eq — Eq dropped (HeaderMatcher is PartialEq only)
pub enum LogFilter {
    StatusCode(StatusCodeComparison),
    ResponseFlag { flags: Vec<String> },
    /// Phase 72: emit a record iff a named request header matches `matcher`.
    Header { matcher: envoy_config::HeaderMatcher },
}
```

> **Dependency posture check (do NOT skip):** the phase-70 ADR-0141 posture keeps `envoy-accesslog` free of an `envoy-config` dep — the filter was compiled config→runtime into accesslog-owned types (`StatusCodeComparison`, `FilterOp`). Carrying a `HeaderMatcher` (an `envoy-config` type) into `LogFilter` would VIOLATE that. RESOLUTION: define the runtime matcher as an accesslog-owned type OR add the dep. **Preferred:** mirror `StatusCodeComparison` — introduce a minimal accesslog-owned `HeaderPredicate` that the `compile_access_log_filter` in `envoy-http1` (which already depends on BOTH crates) builds from `HeaderMatcher`, carrying only what `matches` needs. **However**, `HeaderMatcher::matches` + the 7 modes + SafeRegex `Arc` is substantial to re-model. CONFIRM the current `envoy-accesslog/Cargo.toml` deps at implementation time: if `envoy-config` is ALREADY a dependency (grep `envoy-config` in `crates/envoy-accesslog/Cargo.toml`), carry `HeaderMatcher` directly (simplest, and `matches` is reused verbatim). If NOT, either (a) add the dep with a one-line ADR note (the engine reuse is the whole point of this phase — a config→runtime re-model of a 7-mode matcher is pure duplication), or (b) re-export just `HeaderMatcher` + `matches` behind an accesslog-owned newtype. **Decide and record in PROGRESS.md before writing Step 3's final form.** The plan's default is (a): add `envoy-config` to `envoy-accesslog` if absent, because re-modeling the matcher would duplicate ~80 lines for zero behavioral gain and the two crates already co-compile in `envoy-http1`.

Widen `should_log` (add the `headers` param; implement the `Header` arm):

```rust
pub fn should_log(&self, status: u16, response_flags: &str, headers: &[(String, String)]) -> bool {
    match self {
        LogFilter::StatusCode(c) => { /* unchanged */ }
        LogFilter::ResponseFlag { flags } => { /* unchanged */ }
        LogFilter::Header { matcher } => matcher.matches(headers),
    }
}
```

Update the ~22 in-crate `should_log(` call sites in `filter.rs` (lines 93-151 per recon) to pass a third arg — an empty slice `&[]` where the test does not exercise a header filter:

```rust
// e.g. every existing StatusCode/ResponseFlag test call:
assert!(f.should_log(503, "-", &[]));
```

- [ ] **Step 4: Widen `FileSink::should_log` + its call sites**

In `file_sink.rs` (99-107):

```rust
pub fn should_log(&self, status: u16, response_flags: &str, headers: &[(String, String)]) -> bool {
    match &self.filter {
        Some(f) => f.should_log(status, response_flags, headers),
        None => true,
    }
}
```

Update the 4 test call sites in `file_sink.rs` (353, 354, 361, 362) to pass `&[]` (or a real header slice where relevant).

- [ ] **Step 5: Run to verify pass**

Run: `cargo test -p envoy-accesslog 2>&1 | tail -20`
Expected: `header_filter_should_log_membership` PASSES; all pre-existing `envoy-accesslog` tests PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-accesslog/src/filter.rs crates/envoy-accesslog/src/file_sink.rs crates/envoy-accesslog/Cargo.toml
git commit -m "phase 72 T4: LogFilter::Header + 2nd should_log widening (+drop Eq) [ADR-0149]"
```

---

## Task 5: H1 compile 3-arm match + thread the widened emit gate

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` (`compile_access_log_filter` 1735-1762; emit gate 1512; ~19 test call sites)
- Test: `crates/envoy-http1/src/hcm.rs`

**Interfaces:**
- Consumes: `envoy_config::AccessLogFilter.header_filter`; `LogFilter::Header`.
- Produces: `compile_access_log_filter` covering 3 arms; the H1 gate calls `sink.should_log(record.response_code, &record.response_flags, &req.headers)`.

- [ ] **Step 1: Write the failing test** — a compiled H1 sink with a `header_filter` keeps a request whose header matches, drops one that does not.

Add an H1 hcm test that builds a `HeaderFilter` config, runs it through `compile_access_log_filter`, and checks `should_log`:

```rust
#[test]
fn compile_access_log_filter_builds_header_arm() {
    let mut hm = envoy_config::HeaderMatcher {
        name: "x-log".into(),
        mode: envoy_config::HeaderMatcherMode::StringMatch(/* exact "yes" */ string_matcher_exact("yes")),
        invert_match: false,
    };
    // compile any SafeRegex (none here) to mirror the validated config.
    let filter = envoy_config::AccessLogFilter {
        status_code_filter: None,
        response_flag_filter: None,
        header_filter: Some(envoy_config::HeaderFilter { header: hm.clone() }),
    };
    let compiled = compile_access_log_filter(&filter);
    assert!(compiled.should_log(200, "-", &[("x-log".into(), "yes".into())]));
    assert!(!compiled.should_log(200, "-", &[("x-log".into(), "no".into())]));
    assert!(!compiled.should_log(200, "-", &[]));
    let _ = &mut hm;
}
```

> Use whatever `StringMatcher`-exact constructor the config crate exposes (grep `StringMatcher` / `StringMatcherMode::Exact`); if none is `pub`, use `HeaderMatcherMode::ExactMatch("yes".into())` instead (simpler and equally valid for this test).

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p envoy-http1 compile_access_log_filter_builds_header_arm 2>&1 | tail -20`
Expected: FAIL — the tuple match is 2-wide; there is no `header_filter` arm; the `should_log` call has 2 args.

- [ ] **Step 3: Extend the compile match to 3 arms**

In `compile_access_log_filter` (hcm.rs:1735):

```rust
    match (
        &f.status_code_filter,
        &f.response_flag_filter,
        &f.header_filter,
    ) {
        (Some(scf), None, None) => { /* unchanged StatusCode build */ }
        (None, Some(rff), None) => envoy_accesslog::LogFilter::ResponseFlag {
            flags: rff.flags.clone(),
        },
        (None, None, Some(hf)) => envoy_accesslog::LogFilter::Header {
            matcher: hf.header.clone(),
        },
        _ => unreachable!("validated by validate_access_logs: exactly one filter arm is set"),
    }
```

- [ ] **Step 4: Thread the widened emit-gate call (H1)**

At the H1 emit gate (hcm.rs:1512), pass the in-scope `req.headers` (a `Vec<(String,String)>`):

```rust
                if !sink.should_log(record.response_code, &record.response_flags, &req.headers) {
                    continue;
                }
```

Update the ~19 H1 test call sites of `should_log` (4625, 4716-4718, 4774-4776, 4793-4796, 4992-4997, 5012-5013, 5053, 5094 per recon) to pass a third arg (`&[]` unless the test exercises a header filter).

- [ ] **Step 5: Run to verify pass**

Run: `cargo test -p envoy-http1 2>&1 | tail -20`
Expected: `compile_access_log_filter_builds_header_arm` PASSES; all pre-existing `envoy-http1` tests PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-http1/src/hcm.rs
git commit -m "phase 72 T5: H1 compile 3-arm match + thread req.headers at emit gate [ADR-0149]"
```

---

## Task 6: H2 emit gate — thread `envoy_req.headers` (parity; inert-correct)

**Files:**
- Modify: `crates/envoy-http2/src/hcm.rs` (emit gate 1135; any H2 test call sites of `should_log`)
- Test: `crates/envoy-http2/src/hcm.rs`

**Interfaces:**
- Consumes: the shared `compile_access_log_filter` (H2 uses the H1-built `config.inner.access_log` sinks — no separate compile).
- Produces: the H2 gate calls `sink.should_log(record.response_code, &record.response_flags, &envoy_req.headers)`.

- [ ] **Step 1: Write the failing test** — mirror the H1 gate test if the H2 module has an equivalent unit harness; otherwise this task's coverage is the H1 compile test (Task 5) + the codec-agnostic in-process test (Task 9). If there is no H2-local `should_log` unit test today, the RED is the compile break from the widened signature.

Run: `cargo test -p envoy-http2 2>&1 | tail -20`
Expected (before the edit): FAIL to compile — the H2 gate call at 1135 passes 2 args to the now-3-arg `should_log`.

- [ ] **Step 2: Thread the widened call (H2)**

At the H2 emit gate (crates/envoy-http2/src/hcm.rs:1135), pass the in-scope `envoy_req.headers` (same `Vec<(String,String)>` type — `envoy_req` is an `envoy_http1::Request`):

```rust
            if !sink.should_log(record.response_code, &record.response_flags, &envoy_req.headers) {
                continue;
            }
```

Update any H2 test call sites of `should_log` in lockstep (grep `should_log` in `crates/envoy-http2`).

- [ ] **Step 3: Run to verify pass**

Run: `cargo test -p envoy-http2 2>&1 | tail -20`
Expected: compiles and all `envoy-http2` tests PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/envoy-http2/src/hcm.rs
git commit -m "phase 72 T6: H2 emit gate threads envoy_req.headers (parity) [ADR-0149]"
```

---

## Task 7: Differential driver — CF-71-1 ordering-aware settle + M71-2 doc-phrase fixes

**Files:**
- Modify: `tests/differential/src/lib.rs` (both byte-exact arms: 6357-6375 H1, 6506-6521 H2; the `CF70_3_SETTLE` doc at 1679; add a `CF71_1_SETTLE` const near 1677)
- Modify: `tests/differential/tests/access_log_response_flag_filter.rs` (module doc line 10)
- Test: `tests/differential/src/lib.rs` (a unit test on the settle-selection helper)

**Interfaces:**
- Consumes: `AccessLogByteExactProbe`, `expected_logged_count`.
- Produces: an ordering-aware settle — when the LAST probe is DROPPED (`!probes.last().map_or(true, |p| p.expect_logged)`), settle ≥ the ~10s flush interval; else keep the cheap 2s settle.

- [ ] **Step 1: Write the failing test** — a helper that picks the settle duration from probe ordering.

```rust
#[test]
fn settle_is_ordering_aware() {
    let kept = |p: &str| AccessLogByteExactProbe { /* … path p, expect_logged: true … */ };
    let dropped = |p: &str| AccessLogByteExactProbe { /* … expect_logged: false … */ };
    // kept-LAST (0077/0078 shape): cheap short settle.
    assert_eq!(suppression_settle(&[dropped("/a"), kept("/b")]), CF70_3_SETTLE);
    // dropped-LAST (0076 shape): long settle ≥ the flush interval.
    assert_eq!(suppression_settle(&[kept("/a"), dropped("/b")]), CF71_1_SETTLE);
    assert!(CF71_1_SETTLE >= std::time::Duration::from_secs(10));
}
```

> Fill the probe constructors from the real `AccessLogByteExactProbe` field set (Task 8 shows the full literal). Keep the helper name `suppression_settle(&[AccessLogByteExactProbe]) -> Duration`.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p differential settle_is_ordering_aware 2>&1 | tail -20`
Expected: FAIL — `suppression_settle` / `CF71_1_SETTLE` do not exist.

- [ ] **Step 3: Add the const + helper; fix the M71-2 doc phrase**

Near `CF70_3_SETTLE` (lib.rs:1677), add and fix the doc (M71-2 stale phrase #1):

```rust
/// Phase 71 (CF-70-3): after the kept-line count is reached, a bounded settle
/// during which a filter-DROPPED record that was merely un-flushed would still
/// surface. The ordering-AGNOSTIC CF-70-3 closure (ADR-0146 retired the earlier
/// hard "last probe must be KEPT" ordering-witness precondition; this settle is
/// the sole closure). Only paid by suppression fixtures.
const CF70_3_SETTLE: std::time::Duration = std::time::Duration::from_secs(2);

/// Phase 72 (CF-71-1): when a suppression fixture's LAST probe is DROPPED
/// (e.g. fixture 0076), a bug-leaked line for that last record only flushes on
/// Envoy's ~10s FileAccessLog timer — past the short `CF70_3_SETTLE`. For that
/// ordering, settle past the flush interval so the reference side is covered.
const CF71_1_SETTLE: std::time::Duration = std::time::Duration::from_secs(12);

/// Pick the suppression settle from probe ORDERING: the long CF-71-1 settle when
/// the last probe is dropped, else the cheap CF-70-3 settle. (Kept-LAST fixtures
/// 0077/0078 pay only the short one; dropped-LAST 0076 pays the long one.)
fn suppression_settle(probes: &[AccessLogByteExactProbe]) -> std::time::Duration {
    match probes.last() {
        Some(p) if !p.expect_logged => CF71_1_SETTLE,
        _ => CF70_3_SETTLE,
    }
}
```

In BOTH byte-exact arms (H1 6362, H2 6508), replace `tokio::time::sleep(CF70_3_SETTLE)` with `tokio::time::sleep(suppression_settle(probes))`, and update the `bail!` messages (6371 / 6517) to interpolate the chosen duration (bind `let settle = suppression_settle(probes);` once and reuse).

M71-2 stale phrase #2 — fix `tests/differential/tests/access_log_response_flag_filter.rs:10`:

```rust
//! ...KEPT. The suppressed probe is FIRST and the kept probe is LAST — the
//! sound authoring convention (ADR-0147); ADR-0146 retired the hard ordering
//! assertion, and the driver's ordering-aware bounded settle is the CF-70-3
//! closure.
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p differential settle_is_ordering_aware 2>&1 | tail -20`
Expected: PASS. Then confirm no differential compile regressions: `cargo build -p differential --tests 2>&1 | tail -5`.

- [ ] **Step 5: Commit**

```bash
git add tests/differential/src/lib.rs tests/differential/tests/access_log_response_flag_filter.rs
git commit -m "phase 72 T7: CF-71-1 ordering-aware settle + M71-2 doc-phrase fixes [ADR-0149]"
```

---

## Task 8: Differential fixture `0078-accesslog-header-filter`

**Files:**
- Create: `tests/fixtures/0078-accesslog-header-filter/{envoy.yaml,envoy-rust.yaml,expectations.yaml,README.md}`
- Create: `tests/differential/tests/access_log_header_filter.rs`

**Interfaces:**
- Consumes: `Driver::Http1AccessLogByteExact` (`kind: http1_access_log_byte_exact`), the existing `run_fixture`.
- Produces: a green cross-proxy byte-exact witness — a single `STATUS=200 PATH=/x H=yes` line, mismatch DROPPED.

- [ ] **Step 1: Create the fixture files**

`envoy.yaml`:

```yaml
node: { id: envoy-rust-phase-72-fixture-0078, cluster: envoy-rust-phase-72 }
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
                      path: /tmp/0078-envoy-mount/access.log
                      log_format:
                        text_format_source:
                          inline_string: "STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)% H=%REQ(X-LOG)%\n"
                    # Phase 72 (ADR-0148/0149): emit only when request header
                    # `x-log` exactly matches "yes". Present-mismatch AND absent
                    # both DROP (MEASURED R-0.4).
                    filter:
                      header_filter:
                        header:
                          name: x-log
                          string_match: { exact: "yes" }
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
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
```

`envoy-rust.yaml` — identical minus the 4 documented per-side deltas (no `admin`; `127.0.0.1` bind; no `generate_request_id`; log path `/tmp/0078-envoy-rust-mount/access.log`):

```yaml
node: { id: envoy-rust-phase-72-fixture-0078, cluster: envoy-rust-phase-72 }
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
                      path: /tmp/0078-envoy-rust-mount/access.log
                      log_format:
                        text_format_source:
                          inline_string: "STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)% H=%REQ(X-LOG)%\n"
                    filter:
                      header_filter:
                        header:
                          name: x-log
                          string_match: { exact: "yes" }
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
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
```

`expectations.yaml` — probes ordered **dropped FIRST, kept LAST** (ADR-0147 convention → the cheap `CF70_3_SETTLE` path):

```yaml
driver:
  kind: http1_access_log_byte_exact
  expected_access_log_paths:
    envoy: /tmp/0078-envoy-mount/access.log
    envoy_rust: /tmp/0078-envoy-rust-mount/access.log
  probes:
    # Probe 1 — DROPPED, FIRST: `x-log: no` is present but mismatches
    # `exact: "yes"` → the sink emits NOTHING on EITHER proxy.
    - method: get
      path: /x
      host: envoy-rust.test
      extra_headers:
        - ["x-log", "no"]
      expected_status: 200
      expect_logged: false
    # Probe 2 — KEPT, LAST: `x-log: yes` matches → one byte-identical line
    #   STATUS=200 PATH=/x H=yes
    - method: get
      path: /x
      host: envoy-rust.test
      extra_headers:
        - ["x-log", "yes"]
      expected_status: 200
      expect_logged: true
```

`README.md` — mirror 0077's structure (title; "What this proves" table with `# | request | x-log | matches exact:"yes"? | emitted?`; the measured single line `STATUS=200 PATH=/x H=yes`; "Probes / driver" naming `Http1AccessLogByteExact` + the dropped-FIRST/kept-LAST ordering and why the cheap settle applies; "Per-side divergences" table [admin, bind IP, generate_request_id, log path]; "Cross-references" ADR-0148/0149; "Deferred" noting absent-drop + invert (CF-72-1) + name-only/treat-missing (CF-72-2) are in-process/documented, not in this differential).

`access_log_header_filter.rs`:

```rust
//! Differential fixture 0078 — access-log `header_filter` (phase 72).
//! A file access log gated on `header_filter { header: { name: x-log,
//! string_match: { exact: "yes" } } }`: `GET /x` with `x-log: yes` is KEPT,
//! with `x-log: no` (present-mismatch) is DROPPED. Asserts the log file across
//! both proxies is the SAME single byte-identical `STATUS=200 PATH=/x H=yes`
//! line. Kept-LAST ordering (ADR-0147). See the fixture README.

#[tokio::test]
async fn access_log_header_filter() {
    let dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../fixtures/0078-accesslog-header-filter"
    );
    differential::run_fixture(std::path::Path::new(dir)).await;
}
```

> Confirm the exact `run_fixture` call shape against `access_log_response_flag_filter.rs` (it may be `run_fixture(&dir)` with a `&str`/`&Path`, sync or async) and match it verbatim.

- [ ] **Step 2: Build the debug binary, then run the fixture**

Run:
```bash
cargo build -p envoy-bin && cargo test -p differential --test access_log_header_filter 2>&1 | tee /tmp/0078.out | tail -30
```
Expected: `test access_log_header_filter ... ok` (`1 passed`). If it REDs on a documented host-flake (bridge-IP / parallel-load), re-run in isolation; a true failure names a byte diff.

- [ ] **Step 3: Commit**

```bash
git add tests/fixtures/0078-accesslog-header-filter tests/differential/tests/access_log_header_filter.rs
git commit -m "phase 72 T8: differential fixture 0078-accesslog-header-filter [ADR-0149]"
```

---

## Task 9: In-process coverage — membership across modes, absent-drop, cardinality, PV-4/PV-5 pins, regressions

**Files:**
- Test: `crates/envoy-config/src/bootstrap.rs` (validation + PV pins), `crates/envoy-accesslog/src/filter.rs` (membership), `crates/envoy-http1/src/hcm.rs` (end-to-end gate + regressions)

**Interfaces:**
- Consumes: everything from Tasks 1-6.
- Produces: the guaranteed coverage listed in SPEC §2.1 item 5 + the PV-4/PV-5 decision pins.

- [ ] **Step 1: Write the tests** — group by surface:

(a) **Membership across supported modes + absent-drop** (accesslog crate) — extend `header_filter_should_log_membership` (Task 4) to cover `exact`/`prefix`/`suffix`/`present`/`string_match` and the absent-drop for each non-`present:false` mode.

(b) **PV-4 pin — the inherited shared-engine absent+invert divergence** (config or accesslog crate). Document + pin the CURRENT behavior (absent+invert = KEEP, the shared-engine XOR), with a comment stating it is a MEASURED divergence from upstream (which DROPS absent+invert on BOTH the route and access-log paths) and is deferred to **CF-72-1**:

```rust
#[test]
fn pv4_absent_plus_invert_is_kept_inherited_shared_engine_boundary() {
    // MEASURED (ADR-0149): upstream DROPS absent+invert on BOTH the route and
    // access-log paths. The in-tree shared engine (matcher.rs:51) does an
    // UNCONDITIONAL `mode_result ^ invert_match`, so absent+invert = KEEP. This
    // pins that INHERITED phase-04.2 boundary (shared with route matching);
    // fixing it is CF-72-1 (a cross-cutting route+access-log change), NOT
    // phase 72. The opener fixture 0078 deliberately uses a NON-inverted matcher.
    let m = LogFilter::Header {
        matcher: hm_inverted("x-log", HeaderMatcherMode::PresentMatch(true)),
    };
    assert!(
        m.should_log(200, "-", &[]),
        "in-tree engine keeps absent+invert (diverges from upstream — CF-72-1)"
    );
}
```

(c) **PV-5 pins — name-only + treat_missing rejection** (config crate). Pin that the shared deserializer REJECTS both (fail-loud, ADR-0049), deferred to **CF-72-2**:

```rust
#[test]
fn pv5_name_only_header_filter_is_rejected_inherited_boundary() {
    // MEASURED: upstream accepts `header: { name }` as presence; the in-tree
    // HeaderMatcher deserializer rejects it ("missing mode key"). Inherited
    // phase-04.2 boundary, fail-loud per ADR-0049. Deferred to CF-72-2.
    let yaml = access_log_filter_yaml(r#"header_filter: { header: { name: "x-log" } }"#);
    assert!(crate::parse_bootstrap(&yaml).is_err(), "name-only must reject (CF-72-2)");
}

#[test]
fn pv5_treat_missing_header_as_empty_is_rejected_inherited_boundary() {
    let yaml = access_log_filter_yaml(
        r#"header_filter: { header: { name: "x-log", string_match: { exact: "yes" }, treat_missing_header_as_empty: true } }"#,
    );
    assert!(crate::parse_bootstrap(&yaml).is_err(), "treat_missing must reject (CF-72-2)");
}
```

(d) **Cardinality / empty-header / delegated rejections** — already in Tasks 1-3; add the zero-arm case if not covered:

```rust
#[test]
fn zero_arm_filter_is_rejected() {
    let yaml = access_log_filter_yaml("{}"); // filter: {} — no arm set
    assert!(matches!(
        crate::parse_bootstrap(&yaml).unwrap_err(),
        crate::ConfigError::AmbiguousAccessLogFilter { detail } if detail.contains("no filter variant")
    ));
}
```

(e) **Regressions** (hcm / accesslog) — a no-`filter` sink logs every record; a `status_code_filter` and a `response_flag_filter` sink behave byte-unchanged (call `should_log` with the new 3-arg signature and `&[]`). Add/confirm one assertion each.

- [ ] **Step 2: Run to verify (RED where new behavior, GREEN after Tasks 1-6)**

Run: `cargo test -p envoy-config pv4 pv5 zero_arm 2>&1 | tail -20` and `cargo test -p envoy-accesslog header_filter 2>&1 | tail -20`
Expected: all PASS (they pin already-implemented behavior; the RED for the membership tests was in Task 4).

- [ ] **Step 3: Commit**

```bash
git add crates/envoy-config/src/bootstrap.rs crates/envoy-accesslog/src/filter.rs crates/envoy-http1/src/hcm.rs
git commit -m "phase 72 T9: in-process coverage + PV-4/PV-5 pins + regressions [ADR-0149]"
```

---

## Task 10: Fuzz corpus seed for `parse_bootstrap`

**Files:**
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/header_filter.yaml`
- Modify: `crates/envoy-config/fuzz/.gitignore` (add one `!`-un-ignore line)

**Interfaces:**
- Consumes: the existing `parse_bootstrap` fuzz target (no new target, no ci.yml change — ADR-0137 precedent).

- [ ] **Step 1: Create the seed** — copy `response_flag_filter.yaml` and swap the filter block:

```yaml
node: { id: fuzz-72, cluster: fuzz-72 }
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
                      header_filter:
                        header:
                          name: x-log
                          string_match: { exact: "yes" }
                route_config:
                  name: r
                  virtual_hosts:
                    - name: v
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response: { status: 503, body: { inline_string: "fuzz\n" } }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
```

- [ ] **Step 2: Un-ignore the seed** — add BEFORE the `artifacts/`/`target/` trailer in `crates/envoy-config/fuzz/.gitignore` (after the `!...response_flag_filter.yaml` line):

```
!corpus/parse_bootstrap/header_filter.yaml
```

- [ ] **Step 3: Verify it is tracked**

Run: `git add crates/envoy-config/fuzz && git ls-files crates/envoy-config/fuzz/corpus/parse_bootstrap/header_filter.yaml`
Expected: the path is printed (tracked). If empty, the `!` line is wrong — fix it (memory `fuzz-corpus-seed-gitignored-by-default`).

- [ ] **Step 4: (optional local) smoke the seed**

Run: `cd crates/envoy-config && cargo +nightly fuzz run parse_bootstrap -- -runs=0 2>&1 | tail -5; cd -`
Expected: loads the corpus (incl. the new seed) with no crash. (CI's 30s run is authoritative; this is a courtesy smoke — memory `cargo-fuzz-runs-from-crate-dir-not-repo-root`.)

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-config/fuzz/corpus/parse_bootstrap/header_filter.yaml crates/envoy-config/fuzz/.gitignore
git commit -m "phase 72 T10: parse_bootstrap corpus seed header_filter.yaml [ADR-0149]"
```

---

## Task 11: `BEHAVIOR_CONTRACT.md` `header_filter` subsection + M71-2 §F fix

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (add the subsection after 2328; fix the §F phrase at 2319)

- [ ] **Step 1: Add the `header_filter` subsection** — right before the `---` at line 2330, mirroring the phase-70/71 subsection structure:

```markdown
### Phase 72 (ADR-0148/0149): header_filter — the THIRD emission-gate arm

> Fixture `0078-accesslog-header-filter`. `filter: { header_filter: { header:
> <HeaderMatcher> } }` gates a sink: a record is emitted iff the named REQUEST
> HEADER matches. Present-mismatch AND absent both DROP (MEASURED, graceful-stop
> flush, `envoyproxy/envoy:v1.33.0`).

**§A Schema.** `header_filter.header` is a `HeaderMatcher` (the phase-04.2 route
type, reused verbatim: `name` + a mode oneof [`exact`/`prefix`/`suffix`/
`safe_regex`/`range`/`present`/`string_match`, with `ignore_case`] + `invert_match`).
`header` is PGV-required — empty `header_filter: {}` is fail-loud (envoy-rust:
`ConfigError::Yaml` missing field; upstream: `HeaderFilterValidationError.Header:
value is required`).

**§B Decision.** Compiled to `LogFilter::Header { matcher }`; the runtime gate is
`matcher.matches(&request_headers)` over the downstream request headers in scope
at both HCM emit gates. Validation delegates to the phase-04.2
`validate_header_matcher` (empty name → `EmptyHeaderName`; bad regex →
`InvalidRegex`; bad range → `InvalidInt64Range`; SafeRegex compiled in place).

**§C Invert + ABSENT (PV-4, MEASURED — inherited SHARED boundary).** Upstream
DROPS an ABSENT header under `invert_match: true` on BOTH the route path (a
`GET /` with an inverted route header matcher and no header falls through to the
fallback route) AND the access-log path. The in-tree shared engine
(`matcher.rs:51`) applies `mode_result ^ invert_match` UNCONDITIONALLY, so
absent+invert = KEEP — a latent divergence shared by route matching and
access-log filtering alike. Phase 72 reuses the engine verbatim (the opener uses
a NON-inverted matcher) and does NOT fix it here; the shared-engine fix is
carry-forward **CF-72-1**.

**§D Name-only + treat_missing_header_as_empty (PV-5, MEASURED — inherited
boundary).** Upstream accepts `header: { name }` (presence match) and
`treat_missing_header_as_empty: true`; the in-tree `HeaderMatcher` deserializer
REJECTS both (name-only → "missing mode key"; `treat_missing_header_as_empty` →
unknown field). Kept fail-loud per ADR-0049; carry-forward **CF-72-2**.

**§E Mutual exclusion.** `header_filter`, `status_code_filter`,
`response_flag_filter` are mutually-exclusive `AccessLogFilter` arms — exactly
one (`ConfigError::AmbiguousAccessLogFilter`).

**§F Authoritative fixture.** `0078`: `GET /x` with `x-log: yes` → KEPT
(`STATUS=200 PATH=/x H=yes`); `x-log: no` (present-mismatch) → DROPPED; a
`direct_response` 200 (no backend).
```

- [ ] **Step 2: Fix the M71-2 §F phrase** (stale phrase #3) at BEHAVIOR_CONTRACT.md:2319 — replace `dropped FIRST + kept LAST as the CF-70-3 ordering witness` with:

```
dropped FIRST + kept LAST per the ADR-0147 authoring convention (ADR-0146
retired the hard ordering assertion; the driver's bounded settle is the CF-70-3
closure); same
```

> Confirm the exact surrounding text at 2319 before editing so the sentence stays grammatical (the recon quoted `"DROPPED (renders \`-\`), ... dropped FIRST + kept LAST as the CF-70-3 ordering witness; same"`).

- [ ] **Step 3: Commit**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 72 T11: BEHAVIOR_CONTRACT header_filter subsection + M71-2 §F fix [ADR-0149]"
```

---

## Task 12: Verification gate dry-run (§7.5) — pre-state-4 self-check

**Files:** none (commands only).

- [ ] **Step 1: Run the full workspace gate** (redirect full output to a file, never `tail` a test run for adjudication — memory `never-pipe-verification-runs-through-tail`):

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tee /tmp/72-clippy.out | tail -20
cargo build --workspace --all-targets 2>&1 | tail -10
cargo build -p envoy-bin
cargo test --workspace --no-fail-fast 2>&1 > /tmp/72-test.out; tail -40 /tmp/72-test.out
cargo deny check 2>&1 | tail -20
```
Expected: fmt clean; clippy clean (`-D warnings`); build clean; workspace tests green modulo the documented host-flake set (adjudicate any RED against the STATE.md standing-traps list; a fresh unrelated RustSec advisory in `cargo deny` is patch-bumped, not a regression — memory `cargo-deny-reds-on-unrelated-advisory`).

- [ ] **Step 2: Run the differential + fuzz surface**

```bash
cargo build -p envoy-bin
cargo test -p differential --test access_log_header_filter 2>&1 | tail -10
cargo test -p differential --test access_log_status_code_filter --test access_log_response_flag_filter 2>&1 | tail -10
cd crates/envoy-config && cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30 2>&1 | tail -10; cd -
```
Expected: `0078` green; `0076`/`0077` still green (CF-71-1 settle change did not regress them); fuzz clean.

- [ ] **Step 3: Commit (if any fmt/clippy fixups were needed)**

```bash
git add -A && git commit -m "phase 72 T12: §7.5 gate dry-run fixups [ADR-0149]"
```

> This task authors NO product code — it is the pre-state-4 self-check. The authoritative §7.5 gate is re-run in the SEPARATE state-4 verification session (per §5.1, one state per session; do NOT run state-4 here). Leave `PROGRESS.md` updated with each task's result.

---

## Self-Review

**Spec coverage (SPEC §2.1 in-scope → task):**
1. Config schema (`HeaderFilter` + `header_filter` arm, reuse `HeaderMatcher`) → **Task 1**.
2. Validation (3-arm destructure + M71-1/M71-4; empty-`header`; delegate to `validate_header_matcher`) → **Tasks 2, 3**.
3. Compile + runtime (`LogFilter::Header`, 2nd `should_log` widening, 3-arm compile, both emit loops) → **Tasks 4, 5, 6**.
4. Differential fixture `0078` → **Task 8**.
5. In-process coverage (membership/absent-drop/cardinality/empty-header/delegated + PV-4/PV-5 pins + regressions) → **Task 9**.
6. CF-71-1 closure + M71-2 doc fixes → **Tasks 7 (driver + 2 code docs), 11 (§F contract phrase)**.
7. `BEHAVIOR_CONTRACT.md` subsection → **Task 11**.
8. `known-failures.txt` / conformance unchanged → no task (correct: no protocol-conformance surface).
9. §7.4 fuzz (seed, no new target) → **Task 10**.

**PV resolutions (ADR-0149):** PV-1 (T1/T3), PV-2 (T2), PV-3 (T4/T5/T6), PV-4 (T9 pin + T11 §C + CF-72-1), PV-5 (T9 pins + T11 §D + CF-72-2), PV-6 (this plan's size re-derivation below + T10), PV-7 (T7 + T11 §F).

**Placeholder scan:** no "TBD"/"handle edge cases"/"similar to Task N" — every code step shows literal code or a precise grep-to-confirm instruction. Two spots explicitly defer a MECHANICAL detail to implementation-time confirmation (the `envoy-accesslog` dependency posture in T4 Step 3; the exact `run_fixture` call shape in T8) — each with a stated default and the grep to resolve it; these are genuine "confirm against the live tree" points, not vague placeholders.

**Type consistency:** `should_log(&self, status: u16, response_flags: &str, headers: &[(String, String)]) -> bool` is used identically in filter.rs, file_sink.rs, and both HCM gates. `LogFilter::Header { matcher: HeaderMatcher }` and `HeaderFilter { header: HeaderMatcher }` field names are consistent across T1/T4/T5. `suppression_settle(&[AccessLogByteExactProbe]) -> Duration` is defined once (T7) and used in both arms.

**§6.1 split gate (PV-6 re-derivation).** Net LoC estimate against the live tree:

| Area | Net LoC |
|---|---|
| `HeaderFilter` type + `header_filter` arm + export | ~35 |
| 3-arm destructure + M71-1/M71-4 + `&mut` plumbing + header_filter delegation | ~55 |
| `LogFilter::Header` + 2nd `should_log` widening + drop `Eq` + ~45 call-site touch-ups | ~120 |
| H1 3-arm compile + emit-gate thread; H2 emit-gate thread | ~40 |
| CF-71-1 ordering-aware settle + `suppression_settle` + M71-2 doc fixes | ~55 |
| fixture `0078` (4 files + `.rs`) | ~130 |
| in-process coverage (membership/absent/cardinality/PV pins/regressions) | ~200 |
| fuzz seed + `.gitignore`; `BEHAVIOR_CONTRACT.md`; README | ~90 |
| **Total** | **~725 net LoC / 12 tasks** |

Well under the ~1500 LoC / ~25 task gate → **single phase, NO split** (ADR-0150 stays UNFIRED). The ~45 call-site widening is mechanical breadth (touch-ups, not new logic), folded into Task 4. Comparable to phase 70 (~670) / phase 71 (~645).
