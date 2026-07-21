# Phase 73 — access-log `and_filter` / `or_filter` (recursive composition) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Every task is TDD (superpowers:test-driven-development) — write the failing test, watch it fail, implement, watch it pass, commit.

**Goal:** Land the FOURTH and FIFTH `AccessLogFilter` oneof arms — the recursive `and_filter` / `or_filter` composition — so an access-log sink emits a record iff **all** (`and_filter`) / **any** (`or_filter`) of its nested child predicates match, behaviorally equivalent to `envoyproxy/envoy:v1.33.0`.

**Architecture:** Extend the existing phase-70/71/72 access-log FILTER seam. Add two recursive config structs (`AndFilter`/`OrFilter { filters: Vec<AccessLogFilter> }`, NO `Box` — the `Vec` breaks the recursion), two `LogFilter` runtime variants (`And(Vec<LogFilter>)`/`Or(Vec<LogFilter>)` evaluating `.all()`/`.any()`), a recursive `compile_access_log_filter`, and a recursive `&mut`-taking validation helper (extracted from the currently-inline per-filter body). Two backend-free byte-exact differential fixtures witness the keep/drop set cross-proxy, one at depth-2. Every leaf predicate and the entire byte-exact differential driver are reused UNCHANGED.

**Tech Stack:** Rust (workspace crates `envoy-config`, `envoy-accesslog`, `envoy-http1`), serde/serde_yaml, thiserror, the `testcontainers` differential harness, `cargo fuzz` (libfuzzer).

## Global Constraints

- **Target parity:** `envoyproxy/envoy:v1.33.0` (`docs/envoy-rust/ENVOY_TARGET.md`, digest `sha256:56da5afd…770c2`). Do NOT read Envoy C++ source to decide equivalence — the differential harness + `BEHAVIOR_CONTRACT.md` are the contract (D-3.3).
- **`#![forbid(unsafe_code)]`** holds at every crate root (D-3.8). No `unsafe`.
- **ADR-0150 seam is load-bearing:** `envoy-accesslog` MUST NOT depend on `envoy-config` (a cycle — `envoy-config` already depends on `envoy-accesslog`). `LogFilter` has **NO `Eq`/`PartialEq`**. The new `And`/`Or` variants recurse through `Vec<LogFilter>` (NO `Box`), introduce **NO** `Eq`/`PartialEq` and **NO** `envoy-config` dep.
- **`AccessLogFilter` does NOT derive `Clone`** — every consumer takes `&AccessLogFilter`. `AndFilter`/`OrFilter` match that (no `Clone`).
- **No `Box` at either layer:** both the config `AccessLogFilter` and the runtime `LogFilter` recurse through `Vec<_>` (a fixed-size heap pointer → the type stays finite-size).
- **PGV `min_items = 2`** on `filters` (MEASURED R-0.2): `and_filter`/`or_filter` with fewer than 2 filters is fail-loud. Our error text need not byte-match upstream's `value must contain at least 2 item(s)` (ADR-0049 fail-loud class parity, D-3.3) — the REJECTION must occur.
- **Fixture log FORMAT constraint (MEASURED this session — §6.2 PV-6 correction):** envoy-rust's `%REQ(NAME)%` operator supports only `REQ_ALLOW_LIST` = `:method`, `:authority`, `:path`, `x-envoy-original-path`, `x-forwarded-for`, `user-agent`, `x-request-id` (`crates/envoy-accesslog/src/command_operator.rs:92-100`); a non-allow-listed `%REQ(X-A)%` returns `FormatParseError::UnsupportedHeader` → boot-fatal `ConfigError::InvalidAccessLogFormat`. **The fixtures MUST NOT use `%REQ(X-A)%`/`%REQ(X-B)%`/`%REQ(X-C)%` (SPEC §5's illustrative upstream-only format).** They use the 0078-style allow-listed format `STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)%\n`. The composition gates on `x-a`/`x-b`/`x-c` request headers at the emit gate (via `HeaderMatch::matches` over the raw request-header slice), INDEPENDENT of the format; the differential witness is the keep/drop line COUNT + byte-identical line content (the phase-72 `0078` precedent + `BEHAVIOR_CONTRACT.md` §F).
- **Kept-LAST authoring convention (ADR-0147):** in every fixture the DROPPED probe(s) come FIRST, the KEPT probe(s) LAST, so the driver's ordering-aware settle pays only the cheap `CF70_3_SETTLE` (the long `CF71_1_SETTLE` protects dropped-LAST fixtures like `0076`).
- **Do not disturb** the 32 existing access-log fixtures (`0076`/`0077`/`0078` included) or `known-failures.txt`. Any ROADMAP row edit escapes `\|` and preserves 6 cells; rows `36`/`38`/`39`/`52`/`54` are already malformed — do NOT "fix" them (append-only).
- **envoy-bin writes `ConfigError` to STDOUT.** `cargo build -p envoy-bin` before ANY local differential (`differential-harness-uses-debug-envoy-bin`). `cargo fuzz` runs from the crate dir. A new corpus seed needs a `!`-un-ignore line (`fuzz-corpus-seed-gitignored-by-default`).

---

## File Structure

| File | Responsibility | Tasks |
|---|---|---|
| `crates/envoy-config/src/bootstrap.rs` | `AndFilter`/`OrFilter` structs, the two `AccessLogFilter` oneof fields, the 5-arm destructure + `set_arms`, the extracted recursive `validate_access_log_filter` helper + `filters.len() >= 2` | T1, T2 |
| `crates/envoy-config/src/lib.rs` | re-export `AndFilter`/`OrFilter`; the new `ConfigError` variant | T1, T2 |
| `crates/envoy-accesslog/src/filter.rs` | `LogFilter::And`/`Or` variants + `should_log` `.all()`/`.any()` arms | T3 |
| `crates/envoy-http1/src/hcm.rs` | the recursive 5-tuple `compile_access_log_filter`; update 6 `AccessLogFilter{…}` test construction sites | T1, T4 |
| `tests/fixtures/0079-accesslog-and-filter/` | and_filter differential fixture (config + expectations + README) | T5 |
| `tests/fixtures/0080-accesslog-or-filter/` | or_filter depth-2 differential fixture | T6 |
| `tests/differential/tests/access_log_and_filter.rs` | 0079 entrypoint (~10-line clone) | T5 |
| `tests/differential/tests/access_log_or_filter.rs` | 0080 entrypoint | T6 |
| `crates/envoy-config/fuzz/corpus/parse_bootstrap/and_or_filter.yaml` | fuzz seed (depth-2 recursion) | T7 |
| `crates/envoy-config/fuzz/.gitignore` | `!`-un-ignore the new seed | T7 |
| `docs/envoy-rust/BEHAVIOR_CONTRACT.md` | phase-73 `and_filter`/`or_filter` subsection | T8 |

**Task ordering is load-bearing** (from the SPEC §6.2 handoff): the config model + the validator recursion refactor land FIRST (the extraction is the single expensive item, pinned by a nested-negative test), THEN the runtime variants + recursive compile, THEN the fixtures, docs, and fuzz.

---

### Task 1: Config model — `AndFilter`/`OrFilter` structs, the two oneof fields, the 5-arm cardinality destructure, re-exports, and the construction-site fan-out

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (struct at `721-733`; destructure at `5172-5184`)
- Modify: `crates/envoy-config/src/lib.rs` (re-export block `14-40`)
- Modify: `crates/envoy-http1/src/hcm.rs` (6 test construction sites: `4524`, `4668`, `4741`, `4772`, `4909`, `4926`)

**Interfaces:**
- Produces: `pub struct AndFilter { pub filters: Vec<AccessLogFilter> }`, `pub struct OrFilter { pub filters: Vec<AccessLogFilter> }`; two new fields `pub and_filter: Option<AndFilter>`, `pub or_filter: Option<OrFilter>` on `AccessLogFilter`; both re-exported from `envoy_config::{AndFilter, OrFilter}`.

**Context:** `AccessLogFilter` today (`bootstrap.rs:721-733`) is `#[derive(Debug, Default, Serialize, Deserialize, PartialEq)] #[serde(default, deny_unknown_fields)]` with three `Option<…>` arms and NO `Clone`. `ResponseFlagFilter` (`bootstrap.rs:798-802`) is the model for the new structs — `#[derive(Debug, Default, Serialize, Deserialize, PartialEq)] #[serde(default, deny_unknown_fields)]` over a `Vec`. `Default` + `#[serde(default)]` is REQUIRED so an empty `and_filter: {}` deserializes to `filters: []` (len 0), which Task 2 rejects fail-loud. Adding the two fields breaks the no-`..` full-struct destructure at `bootstrap.rs:5172` AND every `AccessLogFilter { … }` struct-literal (missing-field compile errors) — all fixed in this task so the tree compiles.

- [ ] **Step 1: Write the failing serde round-trip + cardinality test**

Add to the `#[cfg(test)] mod tests` in `crates/envoy-config/src/bootstrap.rs` (near the other access-log-filter validation tests around line 13084):

```rust
#[test]
fn and_or_filter_deserialize_round_trip_and_default() {
    // `and_filter`/`or_filter` deserialize as `{ filters: [<AccessLogFilter>, …] }`
    // (no Box; the recursion runs through Vec). An empty `and_filter: {}` yields
    // `filters: []` via `#[serde(default)]` (Task 2 rejects len < 2).
    let yaml = r#"
status_code_filter: null
response_flag_filter: null
header_filter: null
and_filter:
  filters:
    - header_filter: { header: { name: x-a, string_match: { exact: "1" } } }
    - header_filter: { header: { name: x-b, string_match: { exact: "1" } } }
or_filter: null
"#;
    let f: AccessLogFilter = serde_yaml::from_str(yaml).expect("deserializes");
    let af = f.and_filter.as_ref().expect("and_filter present");
    assert_eq!(af.filters.len(), 2);
    // recursive PartialEq composes across Vec<AccessLogFilter>.
    assert_eq!(f, f);
    // empty `{}` → Default filters (empty vec).
    let empty: AndFilter = serde_yaml::from_str("{}").expect("empty and_filter");
    assert_eq!(empty, AndFilter::default());
    assert!(empty.filters.is_empty());
}

#[test]
fn and_filter_alongside_header_filter_is_ambiguous() {
    // A composition arm set alongside a leaf arm violates the oneof cardinality
    // (exactly one across all FIVE arms).
    let mut logs = vec![AccessLog {
        name: "envoy.access_loggers.file".into(),
        typed_config: AccessLogTypedConfig::FileAccessLog(FileAccessLog {
            path: "/tmp/x.log".into(),
            log_format: None,
        }),
        filter: Some(AccessLogFilter {
            status_code_filter: None,
            response_flag_filter: None,
            header_filter: Some(HeaderFilter {
                header: HeaderMatcher {
                    name: "x-log".into(),
                    mode: HeaderMatcherMode::ExactMatch("yes".into()),
                    invert_match: false,
                },
            }),
            and_filter: Some(AndFilter {
                filters: vec![
                    AccessLogFilter::default(),
                    AccessLogFilter::default(),
                ],
            }),
            or_filter: None,
        }),
    }];
    let err = validate_access_logs(&mut logs).expect_err("ambiguous");
    assert!(matches!(
        err,
        crate::ConfigError::AmbiguousAccessLogFilter { ref detail } if detail.contains("more than one")
    ));
}
```

> If `AccessLog` / `FileAccessLog` / `AccessLogTypedConfig` / `HeaderFilter` / `HeaderMatcher` / `HeaderMatcherMode` are not already in scope in the test module, add them to its `use super::*;` imports (they live in `bootstrap`). Match the exact field set of the surrounding tests (e.g. the ambiguity test at `bootstrap.rs:13101`).

- [ ] **Step 2: Run the tests to verify they fail (compile error — no such field)**

Run: `cargo test -p envoy-config --lib and_or_filter_deserialize_round_trip_and_default and_filter_alongside_header_filter_is_ambiguous 2>&1 | tail -30`
Expected: FAIL — `error[E0560]: struct 'AccessLogFilter' has no field named 'and_filter'` (and `cannot find type 'AndFilter'`).

- [ ] **Step 3: Add the two structs, the two oneof fields, and the re-exports**

In `crates/envoy-config/src/bootstrap.rs`, extend `AccessLogFilter` (currently `721-733`) with two fields after `header_filter`:

```rust
#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct AccessLogFilter {
    pub status_code_filter: Option<StatusCodeFilter>,
    /// Phase 71: ... (unchanged)
    pub response_flag_filter: Option<ResponseFlagFilter>,
    /// Phase 72: ... (unchanged)
    pub header_filter: Option<HeaderFilter>,
    /// Phase 73: the FOURTH `AccessLogFilter` arm — the recursive AND
    /// composition. Emit iff ALL nested child predicates match. `filters` is
    /// PGV `min_items = 2` (enforced by `validate_access_logs`). Mutually
    /// exclusive with the other four arms.
    pub and_filter: Option<AndFilter>,
    /// Phase 73: the FIFTH `AccessLogFilter` arm — the recursive OR composition.
    /// Emit iff ANY nested child predicate matches. `min_items = 2`. Mutually
    /// exclusive with the other four arms.
    pub or_filter: Option<OrFilter>,
}

/// Phase 73: `and_filter` — a boolean-AND composition of nested `AccessLogFilter`
/// predicates. Recurses through `Vec` (a fixed-size pointer → NO `Box`). No
/// `Clone` (all consumers take `&AccessLogFilter`).
#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct AndFilter {
    pub filters: Vec<AccessLogFilter>,
}

/// Phase 73: `or_filter` — a boolean-OR composition of nested `AccessLogFilter`
/// predicates. Same shape as `AndFilter`.
#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct OrFilter {
    pub filters: Vec<AccessLogFilter>,
}
```

In `crates/envoy-config/src/lib.rs`, add `AndFilter` and `OrFilter` to the `pub use bootstrap::{…}` block (keep it sorted — `AndFilter` near line 15 after `AccessLogTypedConfig`/`Action`; `OrFilter` alphabetically). Example:

```rust
pub use bootstrap::{
    AccessLog, AccessLogFilter, AccessLogTypedConfig, Action, Address, Admin, AndFilter, AppendAction,
    // ...
    ObjectMeta, OrFilter, OutlierDetection, // (insert OrFilter in its sorted slot)
    // ...
};
```

- [ ] **Step 4: Extend the `validate_access_logs` destructure + `set_arms` to 5 arms (cardinality only)**

In `crates/envoy-config/src/bootstrap.rs`, the full-struct destructure at `5172-5184` currently binds three arms. Extend it (still NO `..` — the compiler must force future arms):

```rust
            let AccessLogFilter {
                status_code_filter,
                response_flag_filter,
                header_filter,
                and_filter,
                or_filter,
            } = filter;
            let set_arms = [
                status_code_filter.is_some(),
                response_flag_filter.is_some(),
                header_filter.is_some(),
                and_filter.is_some(),
                or_filter.is_some(),
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

> Leave the existing per-arm leaf checks (status_code / response_flag / header) as they are for this task — Task 2 replaces this whole inline body with the recursive helper. The `and_filter`/`or_filter` bindings are USED by `set_arms` (`.is_some()`), so there is no unused-variable warning. (Task 1 accepts an `and_filter` with <2 filters or a bad nested leaf — that gap is closed in Task 2.)

- [ ] **Step 5: Fix the 6 `AccessLogFilter { … }` construction sites so the tree compiles**

Every full struct-literal must now set the two new fields. Run:

`rg -n "AccessLogFilter\s*\{" crates/envoy-http1/src/hcm.rs | grep -v "let AccessLogFilter"`

and to each of the 6 literals (`hcm.rs:4524`, `4668`, `4741`, `4772`, `4909`, `4926`) add, alongside the existing `status_code_filter`/`response_flag_filter`/`header_filter` fields:

```rust
                and_filter: None,
                or_filter: None,
```

(These are all in `#[cfg(test)]` code; the additions are purely mechanical.)

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p envoy-config --lib and_or_filter_deserialize_round_trip_and_default and_filter_alongside_header_filter_is_ambiguous 2>&1 | tail -20`
Expected: PASS (`2 passed`). Also `cargo build -p envoy-config -p envoy-http1 2>&1 | tail -5` → clean (the destructure + all construction sites compile).

- [ ] **Step 7: Commit**

```bash
git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs crates/envoy-http1/src/hcm.rs
git commit -m "phase 73 T1: AndFilter/OrFilter config structs + 5-arm cardinality destructure"
```

---

### Task 2: Validator recursion refactor — extract `validate_access_log_filter(&mut)` + `filters.len() >= 2` + recursive per-child validation

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (`validate_access_logs` body `5166-5215`; new helper below it)
- Modify: `crates/envoy-config/src/lib.rs` (new `ConfigError` variant near `458`)

**Interfaces:**
- Consumes: `AndFilter`/`OrFilter` (T1), the existing `validate_header_matcher(&mut HeaderMatcher)` (`bootstrap.rs:5403`).
- Produces: `fn validate_access_log_filter(filter: &mut AccessLogFilter) -> Result<(), crate::ConfigError>` (recursive); `ConfigError::InsufficientCompositeFilters { count: usize }`.

**Context:** The per-filter validation body currently lives INLINE in the `for entry` loop (`bootstrap.rs:5172-5214`). Recursion requires extracting it into a `&mut`-taking helper that calls itself over `filters`. `validate_header_matcher` already takes `&mut` (compiles the nested `header_filter` SafeRegex in place), so it composes cleanly under recursion. This is the single expensive mechanical item; pin the recursion with a nested-negative test (a bad leaf nested inside `filters` must surface the existing leaf error THROUGH the recursion).

- [ ] **Step 1: Write the failing tests (RED: Task 1 does NOT yet recurse or len-check)**

Add to `crates/envoy-config/src/bootstrap.rs` tests:

```rust
fn file_log_with_filter(filter: AccessLogFilter) -> Vec<AccessLog> {
    vec![AccessLog {
        name: "envoy.access_loggers.file".into(),
        typed_config: AccessLogTypedConfig::FileAccessLog(FileAccessLog {
            path: "/tmp/x.log".into(),
            log_format: None,
        }),
        filter: Some(filter),
    }]
}

fn exact_header(name: &str, val: &str) -> AccessLogFilter {
    AccessLogFilter {
        status_code_filter: None,
        response_flag_filter: None,
        header_filter: Some(HeaderFilter {
            header: HeaderMatcher {
                name: name.into(),
                mode: HeaderMatcherMode::ExactMatch(val.into()),
                invert_match: false,
            },
        }),
        and_filter: None,
        or_filter: None,
    }
}

#[test]
fn and_filter_with_one_child_is_rejected() {
    let f = AccessLogFilter {
        and_filter: Some(AndFilter { filters: vec![exact_header("x-a", "1")] }),
        ..AccessLogFilter::default()
    };
    let err = validate_access_logs(&mut file_log_with_filter(f)).expect_err("min_items");
    assert!(matches!(
        err,
        crate::ConfigError::InsufficientCompositeFilters { count: 1 }
    ));
}

#[test]
fn empty_and_filter_is_rejected() {
    // `and_filter: {}` → `filters: []` (len 0) via serde default → reject.
    let f = AccessLogFilter {
        and_filter: Some(AndFilter::default()),
        ..AccessLogFilter::default()
    };
    let err = validate_access_logs(&mut file_log_with_filter(f)).expect_err("min_items");
    assert!(matches!(
        err,
        crate::ConfigError::InsufficientCompositeFilters { count: 0 }
    ));
}

#[test]
fn or_filter_with_two_children_is_accepted() {
    let f = AccessLogFilter {
        or_filter: Some(OrFilter {
            filters: vec![exact_header("x-a", "1"), exact_header("x-b", "1")],
        }),
        ..AccessLogFilter::default()
    };
    validate_access_logs(&mut file_log_with_filter(f)).expect("valid");
}

#[test]
fn nested_bad_leaf_surfaces_through_recursion() {
    // A nested `header_filter` with an EMPTY name must fail-loud via the
    // recursion (EmptyHeaderName), proving the descent into children.
    let bad = AccessLogFilter {
        status_code_filter: None,
        response_flag_filter: None,
        header_filter: Some(HeaderFilter {
            header: HeaderMatcher {
                name: "".into(),
                mode: HeaderMatcherMode::ExactMatch("1".into()),
                invert_match: false,
            },
        }),
        and_filter: None,
        or_filter: None,
    };
    let f = AccessLogFilter {
        or_filter: Some(OrFilter { filters: vec![exact_header("x-a", "1"), bad] }),
        ..AccessLogFilter::default()
    };
    let err = validate_access_logs(&mut file_log_with_filter(f)).expect_err("nested bad leaf");
    assert!(matches!(err, crate::ConfigError::EmptyHeaderName));
}

#[test]
fn nested_composition_cardinality_surfaces_through_recursion() {
    // A nested `and_filter` whose child has fewer than 2 filters fails-loud
    // through the recursion.
    let inner = AccessLogFilter {
        and_filter: Some(AndFilter { filters: vec![exact_header("x-a", "1")] }),
        ..AccessLogFilter::default()
    };
    let f = AccessLogFilter {
        or_filter: Some(OrFilter { filters: vec![exact_header("x-b", "1"), inner] }),
        ..AccessLogFilter::default()
    };
    let err = validate_access_logs(&mut file_log_with_filter(f)).expect_err("nested cardinality");
    assert!(matches!(
        err,
        crate::ConfigError::InsufficientCompositeFilters { count: 1 }
    ));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p envoy-config --lib and_filter_with_one_child_is_rejected empty_and_filter_is_rejected nested_bad_leaf_surfaces_through_recursion nested_composition_cardinality_surfaces_through_recursion 2>&1 | tail -30`
Expected: FAIL — `cannot find ... InsufficientCompositeFilters` (compile error), and once the variant exists, the `and_filter_with_one_child`/`nested_*` tests fail because Task 1's validator neither len-checks nor recurses (`or_filter_with_two_children` passes). This is the RED for the recursion.

- [ ] **Step 3: Add the `ConfigError` variant**

In `crates/envoy-config/src/lib.rs`, after `AmbiguousAccessLogFilter { detail }` (line `458`):

```rust
    /// Phase 73: an `and_filter`/`or_filter` (`AndFilter`/`OrFilter`) `filters`
    /// list has fewer than 2 entries. Upstream enforces PGV `min_items = 2`
    /// (`AndFilterValidationError.Filters: value must contain at least 2
    /// item(s)`). Our text differs (ADR-0049 fail-loud class parity) but the
    /// rejection is mandatory.
    #[error("and_filter/or_filter must have at least 2 filters, got {count}")]
    InsufficientCompositeFilters { count: usize },
```

- [ ] **Step 4: Extract the recursive helper and rewrite the inline body**

In `crates/envoy-config/src/bootstrap.rs`, replace the inline per-filter body inside `if let Some(filter) = &mut entry.filter { … }` (the destructure-through-`validate_header_matcher` span, `5172-5214`) with a single call:

```rust
        if let Some(filter) = &mut entry.filter {
            validate_access_log_filter(filter)?;
        }
```

Add the recursive helper immediately below `validate_access_logs` (before `validate_header_matcher`):

```rust
/// Phase 73: recursively validate one `AccessLogFilter` oneof. Enforces exactly
/// one arm is set (cardinality, all FIVE arms — no `..`, so a future arm cannot
/// be added without updating this), the per-leaf checks (status-code runtime_key,
/// response-flag token membership, header-matcher compile-in-place), and — for
/// the composition arms — `filters.len() >= 2` plus a recursive descent into
/// every nested child (so a nested `header_filter` SafeRegex still compiles in
/// place and a nested bad leaf / nested under-2 composition still fails-loud).
fn validate_access_log_filter(filter: &mut AccessLogFilter) -> Result<(), crate::ConfigError> {
    let AccessLogFilter {
        status_code_filter,
        response_flag_filter,
        header_filter,
        and_filter,
        or_filter,
    } = filter;
    let set_arms = [
        status_code_filter.is_some(),
        response_flag_filter.is_some(),
        header_filter.is_some(),
        and_filter.is_some(),
        or_filter.is_some(),
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
    if let Some(scf) = status_code_filter
        && scf.comparison.value.runtime_key.is_empty()
    {
        return Err(crate::ConfigError::EmptyStatusCodeFilterRuntimeKey);
    }
    if let Some(rff) = response_flag_filter {
        for token in &rff.flags {
            if !RESPONSE_FLAG_TOKENS.contains(&token.as_str()) {
                return Err(crate::ConfigError::UnknownResponseFlag { token: token.clone() });
            }
        }
    }
    if let Some(hf) = header_filter {
        validate_header_matcher(&mut hf.header)?;
    }
    if let Some(af) = and_filter {
        if af.filters.len() < 2 {
            return Err(crate::ConfigError::InsufficientCompositeFilters { count: af.filters.len() });
        }
        for child in af.filters.iter_mut() {
            validate_access_log_filter(child)?;
        }
    }
    if let Some(of) = or_filter {
        if of.filters.len() < 2 {
            return Err(crate::ConfigError::InsufficientCompositeFilters { count: of.filters.len() });
        }
        for child in of.filters.iter_mut() {
            validate_access_log_filter(child)?;
        }
    }
    Ok(())
}
```

> Update the `validate_access_logs` doc comment (`bootstrap.rs:~5143`) item 3 to note it now delegates per-filter validation to the recursive `validate_access_log_filter`, which additionally enforces `filters.len() >= 2` for the composition arms and recurses into children. Do NOT delete the M70-R1 no-`..` rationale — it now lives on the helper.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p envoy-config --lib and_filter_with_one_child_is_rejected empty_and_filter_is_rejected or_filter_with_two_children_is_accepted nested_bad_leaf_surfaces_through_recursion nested_composition_cardinality_surfaces_through_recursion 2>&1 | tail -20`
Expected: PASS (`5 passed`). Re-run Task 1's two tests too — still green.

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs
git commit -m "phase 73 T2: recursive validate_access_log_filter helper + filters.len()>=2 fail-loud"
```

---

### Task 3: Runtime `LogFilter::And`/`Or` variants + `should_log` `.all()`/`.any()`

**Files:**
- Modify: `crates/envoy-accesslog/src/filter.rs` (enum `43-57`; `should_log` `65-96`)

**Interfaces:**
- Produces: `LogFilter::And(Vec<LogFilter>)`, `LogFilter::Or(Vec<LogFilter>)`; `should_log` handles both (recursively).

**Context:** `LogFilter` (`filter.rs:43-57`) derives ONLY `Debug, Clone` (NO `Eq`/`PartialEq` — ADR-0150). `should_log(&self, status: u16, response_flags: &str, headers: &[(String, String)]) -> bool` (`59-97`). The new variants recurse through `Vec<LogFilter>` (NO `Box`), add NO `Eq`/`PartialEq`, and add NO `envoy-config` dep. AND = `.all()`, OR = `.any()` (MEASURED R-0.3/R-0.4).

- [ ] **Step 1: Write the failing test**

Add to `crates/envoy-accesslog/src/filter.rs` `#[cfg(test)] mod tests` (it already has `ge`/`eq`/`le`/`rf` helpers and the `HasHeaderValue` stub):

```rust
    #[test]
    fn and_or_should_log_all_any_and_empty_boundary() {
        // AND = all children match; OR = any child matches. Uses status-code
        // children (ge/le) so the test needs no header stub.
        let and = LogFilter::And(vec![ge(200), le(299)]); // 2xx band
        assert!(and.should_log(200, "-", &[])); // both true
        assert!(and.should_log(299, "-", &[]));
        assert!(!and.should_log(500, "-", &[])); // le(299) false → AND false

        let or = LogFilter::Or(vec![le(199), ge(500)]); // 1xx OR 5xx
        assert!(or.should_log(100, "-", &[])); // le(199) true
        assert!(or.should_log(503, "-", &[])); // ge(500) true
        assert!(!or.should_log(200, "-", &[])); // neither → OR false

        // Nested composition recurses.
        let nested = LogFilter::Or(vec![
            LogFilter::And(vec![ge(200), le(299)]),
            ge(500),
        ]);
        assert!(nested.should_log(204, "-", &[])); // AND-child true
        assert!(nested.should_log(500, "-", &[])); // leaf true
        assert!(!nested.should_log(404, "-", &[])); // AND-child false, leaf false

        // Empty-vec boundary (unreachable via config's min_items=2, pinned as a
        // semantic invariant): all([]) = true, any([]) = false.
        assert!(LogFilter::And(vec![]).should_log(200, "-", &[]));
        assert!(!LogFilter::Or(vec![]).should_log(200, "-", &[]));
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p envoy-accesslog --lib and_or_should_log_all_any_and_empty_boundary 2>&1 | tail -20`
Expected: FAIL — `no variant named 'And' found for enum 'LogFilter'` (compile error).

- [ ] **Step 3: Add the variants and the `should_log` arms**

In `crates/envoy-accesslog/src/filter.rs`, add to the enum (after `Header { … }`):

```rust
    /// Phase 73: emit iff ALL nested child predicates match (`and_filter`).
    /// Recurses through `Vec<LogFilter>` (NO `Box`). Introduces no `Eq`/`PartialEq`
    /// and no `envoy-config` dep (ADR-0150 holds).
    And(Vec<LogFilter>),
    /// Phase 73: emit iff ANY nested child predicate matches (`or_filter`).
    Or(Vec<LogFilter>),
```

Add to the `should_log` `match self` (after the `Header` arm):

```rust
            // Phase 73: boolean composition over the nested predicates. The
            // config validator's `min_items = 2` makes the empty-vec edge
            // (all→true, any→false) unreachable at runtime; the semantics are
            // pinned in-process regardless.
            LogFilter::And(filters) => {
                filters.iter().all(|f| f.should_log(status, response_flags, headers))
            }
            LogFilter::Or(filters) => {
                filters.iter().any(|f| f.should_log(status, response_flags, headers))
            }
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p envoy-accesslog --lib and_or_should_log_all_any_and_empty_boundary 2>&1 | tail -20`
Expected: PASS (`1 passed`).

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-accesslog/src/filter.rs
git commit -m "phase 73 T3: LogFilter::And/Or runtime variants + should_log all/any"
```

---

### Task 4: Recursive `compile_access_log_filter` — the 5-tuple match

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` (`compile_access_log_filter` `1745-1775`)

**Interfaces:**
- Consumes: `AndFilter`/`OrFilter` (T1), `LogFilter::And`/`Or` (T3).
- Produces: `compile_access_log_filter(&AccessLogFilter) -> LogFilter` mapping all five arms.

**Context:** `compile_access_log_filter` (`hcm.rs:1745-1775`) matches a 3-tuple `(&status_code_filter, &response_flag_filter, &header_filter)` with `_ => unreachable!()`. Adding `and_filter`/`or_filter` fields did NOT break the 3-tuple match (T1) — but an `and_filter` config now falls into `_ => unreachable!()` and PANICS. Widen to a 5-tuple; map the composition arms recursively via `.iter().map(compile_access_log_filter).collect()`. The single call site (`hcm.rs:208`, `entry.filter.as_ref().map(compile_access_log_filter)`) is unchanged.

- [ ] **Step 1: Write the failing test**

Add to `crates/envoy-http1/src/hcm.rs` tests (near `compile_access_log_filter_builds_header_arm`, `4736`):

```rust
    /// Phase 73 T4: `compile_access_log_filter` builds the `and_filter`/`or_filter`
    /// arms recursively. The and-fixture (0079) keeps only the both-match probe;
    /// the depth-2 or-fixture (0080) keeps the AND-child-true and the leaf-true
    /// probes and drops the rest.
    #[test]
    fn compile_access_log_filter_builds_composition_arms_recursively() {
        let hdr = |name: &str, val: &str| envoy_config::AccessLogFilter {
            status_code_filter: None,
            response_flag_filter: None,
            header_filter: Some(envoy_config::HeaderFilter {
                header: envoy_config::HeaderMatcher {
                    name: name.into(),
                    mode: envoy_config::HeaderMatcherMode::ExactMatch(val.into()),
                    invert_match: false,
                },
            }),
            and_filter: None,
            or_filter: None,
        };

        // and_filter { [x-a=1, x-b=1] } → LogFilter::And([Header, Header]).
        let and = envoy_config::AccessLogFilter {
            status_code_filter: None,
            response_flag_filter: None,
            header_filter: None,
            and_filter: Some(envoy_config::AndFilter {
                filters: vec![hdr("x-a", "1"), hdr("x-b", "1")],
            }),
            or_filter: None,
        };
        let compiled = compile_access_log_filter(&and);
        assert!(matches!(compiled, envoy_accesslog::LogFilter::And(ref v) if v.len() == 2));
        let a = [("x-a".to_string(), "1".to_string())];
        let ab = [("x-a".to_string(), "1".to_string()), ("x-b".to_string(), "1".to_string())];
        assert!(!compiled.should_log(200, "-", &a)); // only x-a → AND false → drop
        assert!(compiled.should_log(200, "-", &ab)); // both → AND true → keep

        // or_filter { [ and_filter{[x-a,x-b]}, header{x-c} ] } (depth-2).
        let or = envoy_config::AccessLogFilter {
            status_code_filter: None,
            response_flag_filter: None,
            header_filter: None,
            and_filter: None,
            or_filter: Some(envoy_config::OrFilter {
                filters: vec![
                    envoy_config::AccessLogFilter {
                        status_code_filter: None,
                        response_flag_filter: None,
                        header_filter: None,
                        and_filter: Some(envoy_config::AndFilter {
                            filters: vec![hdr("x-a", "1"), hdr("x-b", "1")],
                        }),
                        or_filter: None,
                    },
                    hdr("x-c", "1"),
                ],
            }),
        };
        let compiled = compile_access_log_filter(&or);
        assert!(matches!(compiled, envoy_accesslog::LogFilter::Or(ref v) if v.len() == 2));
        let c = [("x-c".to_string(), "1".to_string())];
        assert!(compiled.should_log(200, "-", &ab)); // AND-child true → OR keep
        assert!(compiled.should_log(200, "-", &c)); // leaf true → OR keep
        assert!(!compiled.should_log(200, "-", &a)); // AND-child false, leaf false → drop
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p envoy-http1 --lib compile_access_log_filter_builds_composition_arms_recursively 2>&1 | tail -20`
Expected: FAIL — panics at `unreachable!("validated by validate_access_logs: exactly one filter arm is set")` (the 3-tuple match ignores `and_filter`, so a set `and_filter` hits the `_` arm).

- [ ] **Step 3: Widen the match to a 5-tuple**

In `crates/envoy-http1/src/hcm.rs`, replace the `compile_access_log_filter` `match` (`1746-1774`) tuple and add the two arms:

```rust
fn compile_access_log_filter(f: &envoy_config::AccessLogFilter) -> envoy_accesslog::LogFilter {
    match (
        &f.status_code_filter,
        &f.response_flag_filter,
        &f.header_filter,
        &f.and_filter,
        &f.or_filter,
    ) {
        (Some(scf), None, None, None, None) => {
            let op = match scf.comparison.op {
                envoy_config::ComparisonOp::Eq => envoy_accesslog::FilterOp::Eq,
                envoy_config::ComparisonOp::Ge => envoy_accesslog::FilterOp::Ge,
                envoy_config::ComparisonOp::Le => envoy_accesslog::FilterOp::Le,
            };
            envoy_accesslog::LogFilter::StatusCode(envoy_accesslog::StatusCodeComparison {
                op,
                threshold: scf.comparison.value.default_value,
            })
        }
        (None, Some(rff), None, None, None) => envoy_accesslog::LogFilter::ResponseFlag {
            flags: rff.flags.clone(),
        },
        (None, None, Some(hf), None, None) => envoy_accesslog::LogFilter::Header {
            matcher: std::sync::Arc::new(hf.header.clone()),
        },
        // Phase 73: the two composition arms map each child recursively.
        (None, None, None, Some(af), None) => envoy_accesslog::LogFilter::And(
            af.filters.iter().map(compile_access_log_filter).collect(),
        ),
        (None, None, None, None, Some(of)) => envoy_accesslog::LogFilter::Or(
            of.filters.iter().map(compile_access_log_filter).collect(),
        ),
        _ => unreachable!("validated by validate_access_logs: exactly one filter arm is set"),
    }
}
```

> Update the fn doc comment to say five arms ship (add the phase-73 composition arms to the "Three arms ship" line).

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p envoy-http1 --lib compile_access_log_filter_builds_composition_arms_recursively 2>&1 | tail -20`
Expected: PASS (`1 passed`). Also re-run the existing arm tests: `cargo test -p envoy-http1 --lib compile_access_log_filter 2>&1 | tail -10` → all green.

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-http1/src/hcm.rs
git commit -m "phase 73 T4: recursive 5-tuple compile_access_log_filter for and/or arms"
```

---

### Task 5: Differential fixture `0079-accesslog-and-filter` + entrypoint

**Files:**
- Create: `tests/fixtures/0079-accesslog-and-filter/envoy.yaml`
- Create: `tests/fixtures/0079-accesslog-and-filter/envoy-rust.yaml`
- Create: `tests/fixtures/0079-accesslog-and-filter/expectations.yaml`
- Create: `tests/fixtures/0079-accesslog-and-filter/README.md`
- Create: `tests/differential/tests/access_log_and_filter.rs`

**Context:** Fixtures are auto-discovered by path (no registry). The `http1_access_log_byte_exact` driver reads `envoy.yaml` + `envoy-rust.yaml` + `expectations.yaml` and asserts each side's access-log file lines are byte-identical (with a per-side line-count == `expected_logged_count`). Clone `0078` exactly, swapping the filter for `and_filter { [header{x-a=1}, header{x-b=1}] }` and the probes. **Format is the allow-listed `STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)%\n`** (Global Constraints — `%REQ(X-A)%` is boot-fatal). Per-side divergences: `envoy.yaml` adds `admin:` + `generate_request_id: false`, binds `0.0.0.0`, mounts `/tmp/0079-envoy-mount/`; `envoy-rust.yaml` omits admin/generate_request_id, binds `127.0.0.1`, mounts `/tmp/0079-envoy-rust-mount/`.

- [ ] **Step 1: Write the differential test entrypoint (the "test")**

Create `tests/differential/tests/access_log_and_filter.rs`:

```rust
//! Docker-gated differential test for fixture 0079-accesslog-and-filter.
//! Phase 73 (ADR-0152 / ADR-0153) — the FOURTH access-log FILTER witness (arm
//! #4): an `AccessLog` entry carrying `filter.and_filter.filters` gates the
//! sink's per-record emission on the boolean AND of its nested child predicates.
//! One HCM listener with a `text_format_source` file sink
//! (`STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)%`) filtered on
//! `and_filter { filters: [ header_filter{x-a=1}, header_filter{x-b=1} ] }`, and
//! ONE `direct_response` route (`/x` → 200 `hi`). NB the format renders only
//! STATUS+PATH — the composition gates on the `x-a`/`x-b` request headers (read
//! from the raw request-header slice), but the log LINE does not echo them
//! (envoy-rust's `%REQ(NAME)%` supports only an allow-list; `%REQ(X-A)%` is
//! boot-fatal). Two probes, kept-LAST (ADR-0147): (1) `GET /x` with `x-a:1` only
//! (AND false) → SUPPRESSED (`expect_logged: false`); (2) `GET /x` with
//! `x-a:1 x-b:1` (AND true) → KEPT. Each side's file holds EXACTLY ONE
//! byte-identical line `STATUS=200 PATH=/x`. `clusters: []`; no backend spawns.
//! PURE cross-proxy equality: both proxies must agree on the KEPT half AND the
//! DROPPED half.

use std::path::PathBuf;

#[tokio::test]
async fn access_log_and_filter() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0079-accesslog-and-filter");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
```

- [ ] **Step 2: Create the fixture config files**

`tests/fixtures/0079-accesslog-and-filter/envoy-rust.yaml`:

```yaml
node: { id: envoy-rust-phase-73-fixture-0079, cluster: envoy-rust-phase-73 }
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
                      path: /tmp/0079-envoy-rust-mount/access.log
                      log_format:
                        text_format_source:
                          inline_string: "STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)%\n"
                    # Phase 73 (ADR-0152/0153): emit only when BOTH `x-a` AND
                    # `x-b` request headers equal "1" (boolean AND of two nested
                    # header_filter predicates). PGV min_items = 2.
                    filter:
                      and_filter:
                        filters:
                          - header_filter: { header: { name: x-a, string_match: { exact: "1" } } }
                          - header_filter: { header: { name: x-b, string_match: { exact: "1" } } }
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

`tests/fixtures/0079-accesslog-and-filter/envoy.yaml` — identical EXCEPT the four per-side divergences (add `admin`, add `generate_request_id: false`, bind `0.0.0.0`, mount `/tmp/0079-envoy-mount/`):

```yaml
node: { id: envoy-rust-phase-73-fixture-0079, cluster: envoy-rust-phase-73 }
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
                      path: /tmp/0079-envoy-mount/access.log
                      log_format:
                        text_format_source:
                          inline_string: "STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)%\n"
                    filter:
                      and_filter:
                        filters:
                          - header_filter: { header: { name: x-a, string_match: { exact: "1" } } }
                          - header_filter: { header: { name: x-b, string_match: { exact: "1" } } }
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

`tests/fixtures/0079-accesslog-and-filter/expectations.yaml`:

```yaml
driver:
  kind: http1_access_log_byte_exact
  expected_access_log_paths:
    envoy: /tmp/0079-envoy-mount/access.log
    envoy_rust: /tmp/0079-envoy-rust-mount/access.log
  probes:
    # Probe 1 — DROPPED, and FIRST. Only `x-a:1` is present; `x-b` is absent →
    # the second nested header_filter is false → AND false → the sink emits
    # NOTHING on EITHER proxy. `expect_logged: false` removes it from the line
    # count. (Kept-LAST is the sound convention — ADR-0147; this fixture pays the
    # short settle because the LAST probe is KEPT.)
    - method: get
      path: /x
      host: envoy-rust.test
      extra_headers:
        - ["x-a", "1"]
      expected_status: 200
      expect_logged: false
    # Probe 2 — KEPT, and LAST. Both `x-a:1` AND `x-b:1` → AND true → the record
    # IS emitted. Expected line (byte-identical on both sides): STATUS=200 PATH=/x
    - method: get
      path: /x
      host: envoy-rust.test
      extra_headers:
        - ["x-a", "1"]
        - ["x-b", "1"]
      expected_status: 200
      expect_logged: true
  # ASSERTION = PURE CROSS-PROXY EQUALITY (whole-line ==). Each side's file holds
  # EXACTLY ONE line (MEASURED, SPEC §0 R-0.3, graceful-stop flush):
  #   STATUS=200 PATH=/x
  # Both proxies must agree on the kept `x-a:1 x-b:1` line AND the absence of any
  # line for the AND-false `x-a:1`-only probe. The only route is a
  # direct_response → clusters: [], no backend spawns.
```

- [ ] **Step 3: Write the README**

Create `tests/fixtures/0079-accesslog-and-filter/README.md` documenting: the arm-#4 `and_filter` boolean-AND gate; the keep/drop table (`x-a:1`→DROP, `x-a:1 x-b:1`→KEEP); the single byte-identical line `STATUS=200 PATH=/x`; the format-allow-list note (why the line does not echo `x-a`/`x-b`; `%REQ(X-A)%` is boot-fatal, per `BEHAVIOR_CONTRACT.md` §F); the per-side divergence table; kept-LAST + `CF70_3_SETTLE`; cross-references ADR-0152 (pick), ADR-0153 (§6.2 reconciliation). Model it on `tests/fixtures/0078-accesslog-header-filter/README.md`.

- [ ] **Step 4: Build the debug binary and run the fixture locally (Docker-gated)**

```bash
cargo build -p envoy-bin
cargo test -p differential --test access_log_and_filter -- --nocapture 2>&1 | tail -40
```

Expected: PASS (`test access_log_and_filter ... ok`). If Docker is down (mass `client error (Connect)`), see `docker-desktop-down-after-reboot-kvm-acl`. This test is authoritative on CI; local host-flake families are documented (STATE.md standing traps). Byte-exact single line `STATUS=200 PATH=/x` on both proxies.

- [ ] **Step 5: Commit**

```bash
git add tests/fixtures/0079-accesslog-and-filter/ tests/differential/tests/access_log_and_filter.rs
git commit -m "phase 73 T5: fixture 0079-accesslog-and-filter (and_filter differential)"
```

---

### Task 6: Differential fixture `0080-accesslog-or-filter` (depth-2) + entrypoint

**Files:**
- Create: `tests/fixtures/0080-accesslog-or-filter/{envoy.yaml,envoy-rust.yaml,expectations.yaml,README.md}`
- Create: `tests/differential/tests/access_log_or_filter.rs`

**Context:** Same shape as Task 5 but the filter is a DEPTH-2 `or_filter { filters: [ and_filter{[x-a=1, x-b=1]}, header_filter{x-c=1} ] }`, witnessing the recursion differentially (SPEC R-0.5). Three probes, kept-LAST: one dropped first, two kept last. Format still `STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)%\n`. Mounts `/tmp/0080-envoy-mount/` and `/tmp/0080-envoy-rust-mount/`.

- [ ] **Step 1: Write the entrypoint**

Create `tests/differential/tests/access_log_or_filter.rs` (clone Task 5's entrypoint; change fixture dir to `0080-accesslog-or-filter`, fn name `access_log_or_filter`, and the doc comment to describe the depth-2 `or_filter{[and{[x-a,x-b]}, header{x-c}]}` with three probes: `x-a:1`→DROP, `x-a:1 x-b:1`→KEEP (AND-child true), `x-c:1`→KEEP (leaf true); TWO byte-identical `STATUS=200 PATH=/x` lines per side).

- [ ] **Step 2: Create the config files**

`tests/fixtures/0080-accesslog-or-filter/envoy-rust.yaml` — identical skeleton to 0079's envoy-rust.yaml (node id `envoy-rust-phase-73-fixture-0080`, mount `/tmp/0080-envoy-rust-mount/access.log`) with the filter stanza:

```yaml
                    filter:
                      or_filter:
                        filters:
                          - and_filter:
                              filters:
                                - header_filter: { header: { name: x-a, string_match: { exact: "1" } } }
                                - header_filter: { header: { name: x-b, string_match: { exact: "1" } } }
                          - header_filter: { header: { name: x-c, string_match: { exact: "1" } } }
```

`tests/fixtures/0080-accesslog-or-filter/envoy.yaml` — the same with the four per-side divergences (`admin`, `generate_request_id: false`, bind `0.0.0.0`, mount `/tmp/0080-envoy-mount/access.log`).

`tests/fixtures/0080-accesslog-or-filter/expectations.yaml`:

```yaml
driver:
  kind: http1_access_log_byte_exact
  expected_access_log_paths:
    envoy: /tmp/0080-envoy-mount/access.log
    envoy_rust: /tmp/0080-envoy-rust-mount/access.log
  probes:
    # Probe 1 — DROPPED, and FIRST. `x-a:1` only: the nested and_filter is false
    # (x-b absent) AND the leaf header_filter{x-c} is false (x-c absent) → OR
    # false → SUPPRESSED on both proxies.
    - method: get
      path: /x
      host: envoy-rust.test
      extra_headers:
        - ["x-a", "1"]
      expected_status: 200
      expect_logged: false
    # Probe 2 — KEPT. `x-a:1 x-b:1`: the nested and_filter is TRUE → OR true → KEPT.
    - method: get
      path: /x
      host: envoy-rust.test
      extra_headers:
        - ["x-a", "1"]
        - ["x-b", "1"]
      expected_status: 200
      expect_logged: true
    # Probe 3 — KEPT, and LAST. `x-c:1`: the leaf header_filter{x-c} is TRUE → OR
    # true → KEPT. (Last probe kept → short settle, ADR-0147.)
    - method: get
      path: /x
      host: envoy-rust.test
      extra_headers:
        - ["x-c", "1"]
      expected_status: 200
      expect_logged: true
  # ASSERTION = PURE CROSS-PROXY EQUALITY. Each side's file holds EXACTLY TWO
  # byte-identical lines (MEASURED, SPEC §0 R-0.5, graceful-stop flush):
  #   STATUS=200 PATH=/x
  #   STATUS=200 PATH=/x
  # Witnesses OR-of-(nested-AND, leaf) recursion depth-2. clusters: [], no backend.
```

- [ ] **Step 3: Write the README**

Create `tests/fixtures/0080-accesslog-or-filter/README.md` documenting the depth-2 recursion witness (the OR-of-(nested-AND, leaf) truth table over the three probes), the two byte-identical kept lines, the format-allow-list note, per-side divergences, kept-LAST, and ADR-0152/0153 cross-refs. Model on 0078/0079.

- [ ] **Step 4: Run the fixture locally**

```bash
cargo build -p envoy-bin
cargo test -p differential --test access_log_or_filter -- --nocapture 2>&1 | tail -40
```

Expected: PASS. Two byte-identical `STATUS=200 PATH=/x` lines on each side.

- [ ] **Step 5: Commit**

```bash
git add tests/fixtures/0080-accesslog-or-filter/ tests/differential/tests/access_log_or_filter.rs
git commit -m "phase 73 T6: fixture 0080-accesslog-or-filter (depth-2 or_filter differential)"
```

---

### Task 7: Fuzz corpus seed for the recursive filter + `!`-un-ignore

**Files:**
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/and_or_filter.yaml`
- Modify: `crates/envoy-config/fuzz/.gitignore` (add one `!`-un-ignore line)

**Context:** The `and_filter`/`or_filter` config rides the existing `parse_bootstrap` fuzz target (a recursive sub-message over the already-fuzz-reachable `access_log[].filter` path) — NO new target, NO `ci.yml` edit (ADR-0137 config-sub-message precedent; PV-7). The corpus dir is `*`-ignored (`crates/envoy-config/fuzz/.gitignore:1`), so a new seed needs an explicit `!`-un-ignore line or it is untracked and invisible to CI (`fuzz-corpus-seed-gitignored-by-default`). One seed carrying a depth-2 `or_filter{[and_filter{[…]}, header_filter{…}]}` exercises both composition arms + the recursion.

- [ ] **Step 1: Create the seed**

`crates/envoy-config/fuzz/corpus/parse_bootstrap/and_or_filter.yaml`:

```yaml
node: { id: fuzz-73, cluster: fuzz-73 }
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
                      or_filter:
                        filters:
                          - and_filter:
                              filters:
                                - header_filter: { header: { name: x-a, string_match: { exact: "1" } } }
                                - header_filter: { header: { name: x-b, string_match: { exact: "1" } } }
                          - header_filter: { header: { name: x-c, string_match: { exact: "1" } } }
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

- [ ] **Step 2: Un-ignore the seed**

In `crates/envoy-config/fuzz/.gitignore`, add (next to the sibling `!corpus/parse_bootstrap/header_filter.yaml` line):

```
!corpus/parse_bootstrap/and_or_filter.yaml
```

- [ ] **Step 3: Verify the seed is tracked and parses**

```bash
git add crates/envoy-config/fuzz/.gitignore crates/envoy-config/fuzz/corpus/parse_bootstrap/and_or_filter.yaml
git ls-files crates/envoy-config/fuzz/corpus/parse_bootstrap/and_or_filter.yaml
```

Expected: the path prints (tracked). Optionally, from `crates/envoy-config` (memory `cargo-fuzz-runs-from-crate-dir-not-repo-root`): `cd crates/envoy-config && cargo +nightly fuzz run parse_bootstrap corpus/parse_bootstrap/and_or_filter.yaml -- -runs=1 2>&1 | tail -5` → parses clean (no crash). (The full 30s CI run is the state-4 gate.)

- [ ] **Step 4: Commit**

```bash
git commit -m "phase 73 T7: parse_bootstrap fuzz seed and_or_filter.yaml + un-ignore"
```

---

### Task 8: `BEHAVIOR_CONTRACT.md` `and_filter`/`or_filter` subsection

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (insert after the phase-72 §F block, before the `---` at line `2399`)

**Context:** Record the MEASURED facts (SPEC R-0.2..R-0.6) as a sibling subsection to the phase-70/71/72 access-log filter subsections. No behavior claim beyond what the fixtures + in-process tests prove.

- [ ] **Step 1: Add the subsection**

Insert into `docs/envoy-rust/BEHAVIOR_CONTRACT.md` after line `2398`:

```markdown
### Phase 73 (ADR-0152/0153): `and_filter` / `or_filter` — the FOURTH & FIFTH emission-gate arms (recursive composition)

> Fixtures `0079-accesslog-and-filter` + `0080-accesslog-or-filter` (depth-2).
> `filter: { and_filter: { filters: [<AccessLogFilter>, …] } }` (and `or_filter`
> likewise) gates a sink: a record is emitted iff **all** (`and_filter`) / **any**
> (`or_filter`) of the nested child predicates match (MEASURED, graceful-stop
> flush, `envoyproxy/envoy:v1.33.0`).

**§A Schema.** `and_filter`/`or_filter` are `{ filters: repeated AccessLogFilter }`.
`filters` is PGV `min_items = 2` — fewer than 2 (including empty `and_filter: {}`
→ `filters: []`) is fail-loud (envoy-rust: `ConfigError::InsufficientCompositeFilters`;
upstream: `AndFilterValidationError.Filters: value must contain at least 2 item(s)`).
Children may be ANY `AccessLogFilter` — a leaf (`status_code_filter`/
`response_flag_filter`/`header_filter`) OR another composition — to arbitrary
depth (NO depth guard, matching upstream; carry-forward CF-73-1).

**§B Decision.** Compiled to `LogFilter::And(Vec<LogFilter>)` /
`LogFilter::Or(Vec<LogFilter>)`; the runtime gate is `filters.iter().all(…)` /
`filters.iter().any(…)` over the same `should_log(status, response_flags, headers)`
already threaded at the HCM emit gates (no signature change). Both the config
`AccessLogFilter` and the runtime `LogFilter` recurse through `Vec<_>` (NO `Box`);
the runtime variants introduce NO `Eq`/`PartialEq` and NO `envoy-config`
dependency (ADR-0150 holds). Validation recurses via `validate_access_log_filter`
(the extracted `&mut` helper): the min-items check + a descent into every child
(so a nested `header_filter` SafeRegex compiles in place and a nested bad leaf /
nested under-2 composition fails-loud).

**§C Mutual exclusion.** `and_filter`, `or_filter`, `header_filter`,
`status_code_filter`, `response_flag_filter` are the five mutually-exclusive
`AccessLogFilter` oneof arms — exactly one may be set at each level
(`ConfigError::AmbiguousAccessLogFilter`).

**§D Format-allow-list note.** As with `0078` (§F above), the fixtures render only
`STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)%` — the composition gates on the `x-a`/
`x-b`/`x-c` request headers (read from the raw request-header slice at the emit
gate), but the LINE does not echo them, because envoy-rust's `%REQ(NAME)%` supports
only an allow-list (`%REQ(X-A)%` is boot-fatal). The keep/drop line COUNT + the
byte-identical content are the differential witnesses.

**§E Authoritative fixtures.** `0079`: `and_filter{[x-a=1, x-b=1]}` — `GET /x`
with `x-a:1` only → DROPPED, with `x-a:1 x-b:1` → KEPT (one line
`STATUS=200 PATH=/x`). `0080` (depth-2): `or_filter{[ and_filter{[x-a,x-b]},
header_filter{x-c} ]}` — `x-a:1` only → DROPPED, `x-a:1 x-b:1` → KEPT (AND-child
true), `x-c:1` → KEPT (leaf true) (two lines). Both `direct_response` 200, no
backend. Pure cross-proxy equality on the kept lines AND the dropped absences.
```

- [ ] **Step 2: Commit**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 73 T8: BEHAVIOR_CONTRACT and_filter/or_filter subsection"
```

---

## §7.5 phase-done gate (state-4 verification — a LATER session)

Not part of state-3. Recorded here so the state-4 session runs them (D-3.6 / §7.5):
- (a) new fixtures `0079`/`0080` green; (b) all `0001`–`0078` still green;
- (c) no new conformance suite (access-log is not codec-conformance-gated);
- (d) the `parse_bootstrap` fuzz short-budget CI run clean (existing step covers the new seed — no `ci.yml` edit);
- (e) `cargo build --workspace --all-targets` + `cargo clippy --workspace --all-targets --all-features -- -D warnings` + `cargo fmt --all -- --check` + `cargo test --workspace` + `cargo deny check` all clean;
- (f) `REVIEW.md` approved (state-5).

Watch for: the port-reuse / parallel-load startup-race host-flake families and the `tcpclosebackend-ipv6-unreachable` witnesses (CI-authoritative; adjudicate with `--no-fail-fast` + full-output redirect, never `tail`; isolation re-runs name the `--test <binary>`). Rebuild `envoy-bin` before any local differential. `cargo deny` may red on a fresh unrelated RustSec advisory (patch-bump, not a regression).

---

## §6.1 Split gate — re-derived (PV-8)

| Task | Net LoC (rough) |
|---|---|
| T1 config structs + 2 oneof fields + re-exports + 5-arm destructure + 6 construction sites | ~55 |
| T2 recursive `validate_access_log_filter` helper + `filters.len() >= 2` + 1 error variant | ~70 |
| T3 `LogFilter::And`/`Or` + 2 `should_log` arms | ~25 |
| T4 recursive 5-tuple `compile_access_log_filter` | ~20 |
| T5 fixture `0079` (2× config + expectations + README + ~30-line test) | ~150 |
| T6 fixture `0080` (depth-2) | ~150 |
| T7 fuzz seed + un-ignore | ~35 |
| T8 `BEHAVIOR_CONTRACT.md` subsection | ~45 |
| in-process tests (folded into T1–T4) | ~120 |
| **Total** | **~670 net LoC / 8 tasks** |

Well UNDER the ~1500 LoC / ~25 task gate → **single phase, NO split** (ADR-0154 stays UNFIRED). Comparable to phase 70 (~670) / 71 (~630) / 72 (~725). The single expensive item (the T2 validator recursion refactor) is paid once and serves both arms.

## Carry-forward disposition (recorded in ADR-0153)

- **M71-3** (all-suppressed `expected_logged_count == 0` driver shape) — **NOT folded; carry forward.** Folding it soundly needs a DEDICATED all-drop differential fixture (a third container run + flake surface) — the actual gap is the driver's line-count-0 path, which an in-process assertion does not exercise. `0079`/`0080` both keep ≥1 line (kept-LAST convention). Weigh again whenever the next access-log phase adds a naturally all-suppressed fixture.
- **M70-R4** (`"filter": null` serialization) + **M70-R9** (provenance note) — **NOT folded; carry forward.** Not on this phase's core path.
- **CF-73-1** (arbitrary nesting depth, NO stack guard) — **OPENED** (parity with upstream; deferred non-goal; owner = a future stack-safety/DoS-hardening phase).
- **NOT consumed (still live):** CF-72-1 / CF-72-2 (`HeaderMatcher`-parity — a composition arm does not touch the match engine), M71-6/7/8, M69-A..I, CF-69-1/2/3/5, M68-1, M-1, CF-67-3/5/6/7, the older Minors + the HTTP-filters-family (1)–(4).

---

## Self-Review

**Spec coverage** (SPEC §2.1 in-scope items → tasks):
1. `AndFilter`/`OrFilter` config structs → **T1** ✅
2. the two oneof arms + re-exports → **T1** ✅
3. fail-loud validation (5-arm cardinality + `filters.len() >= 2` + recursive per-child) → **T1** (cardinality) + **T2** (recursion + min-items) ✅
4. `LogFilter::And`/`Or` + `should_log` `.all()`/`.any()` → **T3** ✅
5. recursive `compile_access_log_filter` → **T4** ✅
6. fixture `0079` → **T5** ✅
7. fixture `0080` (depth-2) → **T6** ✅
8. in-process tests (should_log all/any incl. boundaries, recursive compile, min-items both arms, nested-cardinality + nested-bad-leaf, 5-arm oneof cardinality) → folded into **T1–T4** ✅
9. `BEHAVIOR_CONTRACT.md` subsection → **T8** ✅
- §2.3 fuzz disposition → **T7** ✅ (seed extension, no new target — PV-7).

**Regression coverage:** the no-`filter`-still-logs and leaf-arm-unchanged regressions are already pinned by the existing phase-70/71/72 tests, which T1's construction-site edits keep compiling and green (re-run in T1 Step 6 / T4 Step 4).

**Placeholder scan:** none — every code step shows complete code (the two READMEs, T6's config skeleton, and T6's entrypoint are described as clones with the exact deltas named; each has a fully-specified sibling template already in-tree).

**Type consistency:** `AndFilter`/`OrFilter { filters: Vec<AccessLogFilter> }`, `ConfigError::InsufficientCompositeFilters { count: usize }`, `validate_access_log_filter(&mut AccessLogFilter)`, `LogFilter::And(Vec<LogFilter>)`/`Or(Vec<LogFilter>)`, `compile_access_log_filter(&AccessLogFilter) -> LogFilter` — names/signatures are consistent across T1→T4 and match the recon'd existing types.
