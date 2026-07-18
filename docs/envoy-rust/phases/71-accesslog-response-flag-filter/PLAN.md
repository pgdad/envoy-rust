# Phase 71 — access-log `response_flag_filter` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Every task is TDD (D-3.1): RED → GREEN → commit, no exceptions.

**Goal:** Land the SECOND `AccessLogFilter` oneof arm, `response_flag_filter`
(`envoy.config.accesslog.v3.AccessLogFilter.response_flag_filter`) — a per-`AccessLog`-sink
predicate that emits a record to its sink only when the record's single `%RESPONSE_FLAGS%` token is
one of the configured `flags` (an EMPTY `flags` matches any record that HAS a flag set) — behaviorally
equivalent to `envoyproxy/envoy:v1.33.0` under the differential contract.

**Architecture:** Reuse the ENTIRE phase-70 `filter` seam. Add a `ResponseFlagFilter { flags:
Vec<String> }` config type + the `response_flag_filter: Option<ResponseFlagFilter>` arm on
`AccessLogFilter`; validate each token against the MEASURED 29-token v1.33.0 PGV `in`-list
(fail-loud on unknown). Add a `LogFilter::ResponseFlag { flags }` runtime variant in
`crates/envoy-accesslog` and **widen `should_log`** so it sees the record's response-flag token
(the one genuinely-new seam). Discharge the two deferred arm-#2 obligations: convert
`compile_access_log_filter`'s zero-arm `expect()` into a full 2-arm `match` (CF-70-1) and the
one-element `set_arms` cardinality array into a compiler-forcing destructuring (M70-R1). Witness
byte-exact by a new fixture `0077` (a no-route 404 `NR` KEPT, a `direct_response` 503 `-`
SUPPRESSED → one byte-identical line, no backend), and close CF-70-3 (the FileAccessLog async-flush
false-pass window) with a suppression-only ordering witness in the differential driver.

**Tech Stack:** Rust (workspace crates `envoy-config`, `envoy-accesslog`, `envoy-http1`,
`envoy-http2`, `tests/differential`), `serde`/`serde_yaml`, `thiserror`, `testcontainers`
(Docker differential against `envoyproxy/envoy:v1.33.0`).

## Global Constraints

- `#![forbid(unsafe_code)]` holds at every crate root (D-3.8) — do not add `unsafe`.
- Reference Envoy is pinned to `envoyproxy/envoy:v1.33.0` (D-3.7) — do not change the pin.
- **`cargo build -p envoy-bin` before ANY local differential run** — the harness runs
  `target/debug/envoy-bin`; a stale binary REDs with `unknown field` on the new `response_flag_filter` key.
- Config-validity is all-fatal (ADR-0049): native (non-identical) `ConfigError` messages are
  permitted; the requirement is that the SAME class of configs is rejected/accepted as upstream.
- New config structs carry `#[serde(deny_unknown_fields)]` (the crate-wide convention; the only
  documented exception is `Node`). Every new oneof-arm field is `#[serde(default)]`.
- `ConfigError` is grow-only (D-3.5): append new variants, never rename/remove existing ones.
- Never weaken a fixture; never trim `tests/conformance/.../known-failures.txt`. Do NOT disturb the
  30 existing access-log fixtures (29 pre-phase-70 + `0076`).
- Any `ROADMAP.md` row edit preserves all 6 cells and escapes a literal `|` as `\|`; rows
  `36`/`38`/`39`/`52`/`54` are already malformed — do NOT "fix" them (append-only).
- `next-prompt.txt` is gitignored (`.gitignore:9`) — never `git add` it.
- CI is authoritative for the documented host-flake set (see `STATE.md` standing traps); a local
  differential RED in that set is NOT a regression.

**Measured facts this plan rests on** (SPEC.md §0 + the state-2 PV recon + PV-6 live measurement,
recorded in ADR-0144 + **ADR-0145**):
- **The accepted-token set is EXACTLY 29 tokens** (re-measured this session; ADR-0144's "30"/"24"
  is an off-by-one — corrected to **29 / 6 producible / 23 inert** in ADR-0145): `LH UH UT LR UR UF
  UC UO NR DI FI RL UAEX RLSE DC URX SI IH DPE UMSDR RFCF NFCF DT UPE NC OM DF DO DR`. envoy-rust
  produces only `{NR, UH, UO, UC, UF, URX}`; the other 23 are parsed-but-inert. `BOGUS`/lowercase
  are fail-loud (parity with the PGV `in`-list).
- The match is a token-membership test over the SINGLE rendered `%RESPONSE_FLAGS%` token
  (`record.response_flags: String`, always one token, brace-free; `-` is the no-flag sentinel and
  ∉ the 29-token set, so a non-empty `flags` never matches a `-` record).
- **Empty/absent `flags` (MEASURED PV-6):** `flags: []` and `response_flag_filter: {}` both parse,
  are ACTIVE, and match a record iff it HAS a flag set — the no-route 404 `NR` is KEPT, the clean
  `direct_response` 503 `-` is DROPPED. Load-parity forbids rejecting the upstream-accepted empty list.
- `response_flag_filter` and `status_code_filter` are mutually-exclusive oneof arms — a `filter`
  carrying both is REJECTED (R-0.3).
- Fixture `0077` needs NO backend: a `direct_response` 503 (dropped, flag `-`) + a no-route 404
  (kept, flag `NR`) → one byte-identical `STATUS=404 PATH=/nowhere FLAGS=NR` line.

---

## File Structure

**`crates/envoy-config/src/bootstrap.rs`** — add the `ResponseFlagFilter` type + the
`response_flag_filter` arm on `AccessLogFilter` (near the `AccessLogFilter` block at `713-765`); add
the `RESPONSE_FLAG_TOKENS` const; convert the `set_arms` array (`5118-5121`) to a destructuring and
add the token-validation branch inside `validate_access_logs` (`5109`+).

**`crates/envoy-config/src/lib.rs`** — add `ConfigError::UnknownResponseFlag { token }` (after
`EmptyStatusCodeFilterRuntimeKey` at `464`); add `ResponseFlagFilter` to the `bootstrap` re-export
(mirror how `StatusCodeFilter`/`ComparisonOp` are re-exported).

**`crates/envoy-accesslog/src/filter.rs`** — add `LogFilter::ResponseFlag { flags: Vec<String> }`
(`21-26`); widen `should_log` to `(status: u16, response_flags: &str)` (`28-44`) with the
empty-`flags` semantics.

**`crates/envoy-accesslog/src/file_sink.rs`** — widen `FileSink::should_log` to `(status,
response_flags)` (`99-106`); update in-crate test callers (`~352-361`).

**`crates/envoy-http1/src/hcm.rs`** — convert `compile_access_log_filter`'s `expect()` (`1736-1757`)
to a 2-arm `match` (CF-70-1); thread the widened call at the H1 emit gate (`1512`); update H1 test
call sites (`~4619`, `~4820-4925`).

**`crates/envoy-http2/src/hcm.rs`** — thread the widened call at the H2 emit gate (`1135`); update
H2 test call sites.

**`tests/differential/src/lib.rs`** — CF-70-3 suppression-only ordering-witness hardening in
`run_http1_access_log_byte_exact_arm` (`6243`) and `run_http2_access_log_byte_exact_arm` (`6386`);
a unit test for the `expected_logged_count` helper (`1134`, M70-R2).

**`tests/fixtures/0077-accesslog-response-flag-filter/`** (NEW) — `envoy.yaml`, `envoy-rust.yaml`,
`expectations.yaml`, `README.md`; plus a `tests/differential/tests/access_log_response_flag_filter.rs`
`#[test]` file (mirror `access_log_status_code_filter.rs`).

**`crates/envoy-config/fuzz/corpus/parse_bootstrap/response_flag_filter.yaml`** (NEW seed) + one
`!`-un-ignore line in `crates/envoy-config/fuzz/.gitignore`.

**`docs/envoy-rust/BEHAVIOR_CONTRACT.md`** — a `response_flag_filter` subsection under access-log,
sibling to the phase-70 `status_code_filter` subsection.

---

## Task 1: Config schema — `ResponseFlagFilter` + the `response_flag_filter` oneof arm

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (add `ResponseFlagFilter` after `RuntimeUInt32` at
  `~765`; add the `response_flag_filter` field to `AccessLogFilter` at `713-723`)
- Modify: `crates/envoy-config/src/lib.rs` (add `ResponseFlagFilter` to the `bootstrap` re-export)
- Test: `crates/envoy-config/src/bootstrap.rs` (`#[cfg(test)]`)

**Interfaces:**
- Produces:
  - `pub struct ResponseFlagFilter { pub flags: Vec<String> }`
  - `pub struct AccessLogFilter { pub status_code_filter: Option<StatusCodeFilter>, pub response_flag_filter: Option<ResponseFlagFilter> }`

- [ ] **Step 1: Write the failing parse test**

Add to the `bootstrap.rs` test module (reuse the phase-70 access-log YAML/read helpers — grep
`parses_status_code_filter_ge_500` and `first_access_log` for the builder/reader shape):

```rust
#[test]
fn parses_response_flag_filter_nr() {
    let yaml = hcm_with_access_log_yaml(
        r#"                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/al.log
                    filter:
                      response_flag_filter:
                        flags: ["NR"]
"#,
    );
    let bootstrap = crate::parse_bootstrap(&yaml).expect("should parse");
    let filter = first_access_log(&bootstrap).filter.as_ref().expect("filter present");
    let rff = filter
        .response_flag_filter
        .as_ref()
        .expect("response_flag_filter present");
    assert_eq!(rff.flags, vec!["NR".to_string()]);
    assert!(filter.status_code_filter.is_none());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p envoy-config parses_response_flag_filter_nr`
Expected: FAIL — `AccessLogFilter` has no field `response_flag_filter` (unknown field under
`deny_unknown_fields`), or `ResponseFlagFilter` does not exist.

- [ ] **Step 3: Add the type + the arm**

In `bootstrap.rs`, add the `response_flag_filter` field to `AccessLogFilter` (currently `713-723`):

```rust
#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct AccessLogFilter {
    pub status_code_filter: Option<StatusCodeFilter>,
    /// Phase 71: the SECOND `AccessLogFilter` arm — gates emission on the
    /// record's response-flag token. Mutually exclusive with
    /// `status_code_filter` (cardinality enforced by `validate_access_logs`).
    pub response_flag_filter: Option<ResponseFlagFilter>,
}
```

(Also update `AccessLogFilter`'s doc comment: it currently says "This phase models ONLY the
`status_code_filter` arm" — change to note it now models TWO arms and future arms are added here.)

Add the new type after `RuntimeUInt32` (`~765`):

```rust
/// Models `envoy.config.accesslog.v3.ResponseFlagFilter` — the `AccessLogFilter`
/// arm that emits a record only when its single `%RESPONSE_FLAGS%` token is one
/// of `flags`. `flags` accepts the 29-token v1.33.0 PGV `in`-list (validated by
/// `validate_access_logs`; unknown tokens are fail-loud). An EMPTY or absent
/// `flags` matches any record that HAS a response flag set (MEASURED — ADR-0145
/// PV-6). envoy-rust produces only `{NR, UH, UO, UC, UF, URX}`; the other 23
/// tokens are parsed-but-inert (accepted for load-parity, never matched).
#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct ResponseFlagFilter {
    pub flags: Vec<String>,
}
```

In `crates/envoy-config/src/lib.rs`, add `ResponseFlagFilter` to the `pub use bootstrap::{…}` line
that already re-exports `AccessLogFilter`/`StatusCodeFilter`/`ComparisonOp` (grep `AccessLogFilter`
in `lib.rs` to find it).

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p envoy-config parses_response_flag_filter_nr`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs
git commit -m "phase 71 T1: ResponseFlagFilter schema + response_flag_filter oneof arm"
```

---

## Task 2: `set_arms` compiler-forcing destructuring (M70-R1) — both-arm rejection reachable

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (`validate_access_logs`, the filter block at
  `5116-5136`)
- Test: `crates/envoy-config/src/bootstrap.rs`

**Interfaces:**
- Consumes: `AccessLogFilter { status_code_filter, response_flag_filter }` (Task 1),
  `ConfigError::AmbiguousAccessLogFilter { detail }` (exists, phase 70).

- [ ] **Step 1: Write the failing test (a filter with BOTH arms is rejected)**

```rust
#[test]
fn rejects_access_log_filter_with_both_arms() {
    let yaml = hcm_with_access_log_yaml(
        r#"                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/al.log
                    filter:
                      status_code_filter:
                        comparison:
                          op: GE
                          value: { default_value: 500, runtime_key: unused }
                      response_flag_filter:
                        flags: ["NR"]
"#,
    );
    let err = crate::parse_bootstrap(&yaml).expect_err("both arms must be rejected");
    assert!(
        matches!(err, crate::ConfigError::AmbiguousAccessLogFilter { .. }),
        "got {err:?}"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p envoy-config rejects_access_log_filter_with_both_arms`
Expected: FAIL — with the one-element `set_arms` array, only `status_code_filter` is counted, so
`set_arms == 1` and the both-arms config is (wrongly) accepted.

- [ ] **Step 3: Convert the array to a compiler-forcing destructuring**

Replace the filter block in `validate_access_logs` (`5116-5136`) with:

```rust
        if let Some(filter) = &entry.filter {
            // Phase 71 (M70-R1): destructure ALL arms so a future arm cannot be
            // added without updating this count (no `..` — the compiler forces
            // it). With two arms the `> 1` (both-set) branch is now REACHABLE:
            // upstream rejects a `filter` carrying both arms (ADR-0145 R-0.3).
            let AccessLogFilter {
                status_code_filter,
                response_flag_filter,
            } = filter;
            let set_arms = [status_code_filter.is_some(), response_flag_filter.is_some()]
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
            // (Task 3 adds the `response_flag_filter` token validation here,
            // using the `response_flag_filter` binding.)
        }
```

(Note: the pre-existing `runtime_key` check now consumes the `status_code_filter` destructured
binding instead of `&filter.status_code_filter` — behavior is identical.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p envoy-config rejects_access_log_filter_with_both_arms rejects_access_log_filter_with_no_variant`
Expected: PASS both (the new both-arms rejection AND the phase-70 zero-arm rejection regression —
`set_arms == 0` branch unchanged).

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-config/src/bootstrap.rs
git commit -m "phase 71 T2: set_arms compiler-forcing destructuring (M70-R1), both-arm rejection reachable"
```

---

## Task 3: Token validator — `UnknownResponseFlag` fail-loud (29-token PGV `in`-list parity)

**Files:**
- Modify: `crates/envoy-config/src/lib.rs` (add `ConfigError::UnknownResponseFlag { token }`)
- Modify: `crates/envoy-config/src/bootstrap.rs` (add `RESPONSE_FLAG_TOKENS` const; add the
  validation branch inside the Task-2 filter block)
- Test: `crates/envoy-config/src/bootstrap.rs`

**Interfaces:**
- Consumes: `ResponseFlagFilter.flags: Vec<String>` (Task 1), the `response_flag_filter` binding
  (Task 2).
- Produces: `ConfigError::UnknownResponseFlag { token: String }`; `const RESPONSE_FLAG_TOKENS`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn rejects_response_flag_filter_unknown_token() {
    let yaml = hcm_with_access_log_yaml(
        r#"                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/al.log
                    filter:
                      response_flag_filter:
                        flags: ["NR", "ZZZ_BOGUS"]
"#,
    );
    let err = crate::parse_bootstrap(&yaml).expect_err("bogus token must be rejected");
    assert!(
        matches!(&err, crate::ConfigError::UnknownResponseFlag { token } if token == "ZZZ_BOGUS"),
        "got {err:?}"
    );
}

#[test]
fn rejects_response_flag_filter_lowercase_token() {
    let yaml = hcm_with_access_log_yaml(
        r#"                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/al.log
                    filter:
                      response_flag_filter:
                        flags: ["nr"]
"#,
    );
    let err = crate::parse_bootstrap(&yaml).expect_err("lowercase token must be rejected");
    assert!(
        matches!(&err, crate::ConfigError::UnknownResponseFlag { token } if token == "nr"),
        "got {err:?}"
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p envoy-config rejects_response_flag_filter_unknown_token rejects_response_flag_filter_lowercase_token`
Expected: FAIL — no token validation yet; the bogus/lowercase configs parse+validate and the
variant does not exist.

- [ ] **Step 3: Add the const + the `ConfigError` variant + the validation branch**

In `bootstrap.rs`, near the `ResponseFlagFilter` type (`~765`):

```rust
/// The `ResponseFlagFilter.flags` PGV `in`-list as MEASURED against
/// `envoyproxy/envoy:v1.33.0` (`--mode validate`; ADR-0145): exactly 29 tokens.
/// envoy-rust produces only `{NR, UH, UO, UC, UF, URX}`; the other 23 are
/// parsed-but-inert. Order matches the upstream rejection message.
pub(crate) const RESPONSE_FLAG_TOKENS: [&str; 29] = [
    "LH", "UH", "UT", "LR", "UR", "UF", "UC", "UO", "NR", "DI", "FI", "RL",
    "UAEX", "RLSE", "DC", "URX", "SI", "IH", "DPE", "UMSDR", "RFCF", "NFCF",
    "DT", "UPE", "NC", "OM", "DF", "DO", "DR",
];
```

In `lib.rs`, after `EmptyStatusCodeFilterRuntimeKey` (`464`), mirroring its convention:

```rust
    /// Phase 71: a `response_flag_filter.flags` entry is not one of the 29
    /// v1.33.0 response-flag tokens (`BOGUS`, lowercase, etc.). Upstream's PGV
    /// `in`-list rejects the same class; load-parity requires fail-loud here.
    #[error("response_flag_filter flags must be a known response-flag token: {token}")]
    UnknownResponseFlag { token: String },
```

In `bootstrap.rs`, inside the Task-2 filter block (after the `runtime_key` check), add:

```rust
            if let Some(rff) = response_flag_filter {
                for token in &rff.flags {
                    if !RESPONSE_FLAG_TOKENS.contains(&token.as_str()) {
                        return Err(crate::ConfigError::UnknownResponseFlag {
                            token: token.clone(),
                        });
                    }
                }
            }
```

- [ ] **Step 4: Add acceptance tests (empty / absent / inert token all VALIDATE)**

```rust
#[test]
fn accepts_response_flag_filter_empty_and_inert() {
    // Empty flags, absent flags, and an inert-but-valid token all parse+validate
    // (load-parity, ADR-0145 PV-6 / R-0.2). `DI` is a valid token envoy-rust
    // never produces.
    for filter_yaml in [
        "                    filter:\n                      response_flag_filter:\n                        flags: []\n",
        "                    filter:\n                      response_flag_filter: {}\n",
        "                    filter:\n                      response_flag_filter:\n                        flags: [\"DI\"]\n",
    ] {
        let yaml = hcm_with_access_log_yaml(&format!(
            "                access_log:\n                  - name: envoy.access_loggers.file\n                    typed_config:\n                      \"@type\": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog\n                      path: /tmp/al.log\n{filter_yaml}"
        ));
        crate::parse_bootstrap(&yaml).expect("empty/absent/inert flags must validate");
    }
}
```

- [ ] **Step 5: Run all Task-3 tests to verify they pass**

Run: `cargo test -p envoy-config response_flag_filter`
Expected: PASS (`rejects_..._unknown_token`, `rejects_..._lowercase_token`,
`accepts_response_flag_filter_empty_and_inert`, and the Task-1/2 tests).

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-config/src/lib.rs crates/envoy-config/src/bootstrap.rs
git commit -m "phase 71 T3: UnknownResponseFlag fail-loud validator (29-token in-list parity) + empty/inert acceptance"
```

---

## Task 4: Runtime predicate — `LogFilter::ResponseFlag` + widen `should_log` (envoy-accesslog)

**Files:**
- Modify: `crates/envoy-accesslog/src/filter.rs` (add the variant; widen `should_log`)
- Modify: `crates/envoy-accesslog/src/file_sink.rs` (widen `FileSink::should_log`; fix in-crate test
  callers at `~352-361`)
- Test: `crates/envoy-accesslog/src/filter.rs` (`#[cfg(test)]`)

**Interfaces:**
- Produces:
  - `pub enum LogFilter { StatusCode(StatusCodeComparison), ResponseFlag { flags: Vec<String> } }`
  - `impl LogFilter { pub fn should_log(&self, status: u16, response_flags: &str) -> bool }`
  - `impl FileSink { pub fn should_log(&self, status: u16, response_flags: &str) -> bool }`

> **Blast-radius note:** widening `should_log` breaks the two HTTP HCM call sites
> (`envoy-http1/src/hcm.rs:1512`, `envoy-http2/src/hcm.rs:1135`) — those are fixed in Tasks 5 and 6.
> This task keeps `cargo test -p envoy-accesslog` green; the full workspace build completes after
> Task 6. Fix every `FileSink::should_log(...)` caller INSIDE this crate (grep
> `should_log` in `crates/envoy-accesslog`) to pass a second `&str` arg (`"-"` is a safe default).

- [ ] **Step 1: Write the failing boundary tests**

Add to the `filter.rs` test module:

```rust
    fn rf(flags: &[&str]) -> LogFilter {
        LogFilter::ResponseFlag {
            flags: flags.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn response_flag_membership() {
        // The ResponseFlag arm ignores `status`; pass any value.
        assert!(rf(&["NR"]).should_log(404, "NR"));
        assert!(rf(&["UH", "NR"]).should_log(404, "NR"));
        assert!(!rf(&["UH"]).should_log(404, "NR"));
    }

    #[test]
    fn response_flag_dash_sentinel_never_matches_nonempty() {
        // "-" ∉ the 29-token set, so a non-empty `flags` never matches it.
        assert!(!rf(&["NR"]).should_log(503, "-"));
        assert!(!rf(&["UH", "UF"]).should_log(503, "-"));
    }

    #[test]
    fn response_flag_empty_matches_any_flag_set() {
        // MEASURED (ADR-0145 PV-6): empty `flags` keeps records WITH a flag,
        // drops the "-" no-flag sentinel.
        assert!(rf(&[]).should_log(404, "NR"));
        assert!(rf(&[]).should_log(503, "UF"));
        assert!(!rf(&[]).should_log(503, "-"));
    }

    #[test]
    fn response_flag_inert_token_never_matches_produced() {
        // A config may carry an inert token (`DI`); envoy-rust never renders it.
        assert!(!rf(&["DI"]).should_log(404, "NR"));
        assert!(!rf(&["DI"]).should_log(503, "-"));
    }

    #[test]
    fn status_code_arm_ignores_response_flags() {
        let f = LogFilter::StatusCode(StatusCodeComparison { op: FilterOp::Ge, threshold: 500 });
        assert!(f.should_log(503, "-"));
        assert!(f.should_log(503, "NR"));
        assert!(!f.should_log(200, "NR"));
    }
```

(Also update the phase-70 boundary tests `ge_500_boundary`/`eq_404_boundary`/`le_200_boundary` to
pass a second arg, e.g. `ge(500).should_log(499, "-")`.)

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p envoy-accesslog filter::`
Expected: FAIL — `should_log` takes one arg; `LogFilter::ResponseFlag` does not exist.

- [ ] **Step 3: Implement the variant + widen `should_log`**

In `filter.rs`, extend the enum (`21-26`):

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogFilter {
    StatusCode(StatusCodeComparison),
    /// Phase 71: emit a record iff its response-flag token ∈ `flags`. An EMPTY
    /// `flags` matches any record that HAS a flag set (ADR-0145 PV-6).
    ResponseFlag { flags: Vec<String> },
}
```

Widen `should_log` (`28-44`):

```rust
impl LogFilter {
    /// Returns `true` iff a record with the given final response `status` and
    /// `response_flags` token should be emitted. The `StatusCode` arm ignores
    /// `response_flags`; the `ResponseFlag` arm ignores `status`.
    pub fn should_log(&self, status: u16, response_flags: &str) -> bool {
        match self {
            LogFilter::StatusCode(c) => {
                let s = status as u32;
                match c.op {
                    FilterOp::Eq => s == c.threshold,
                    FilterOp::Ge => s >= c.threshold,
                    FilterOp::Le => s <= c.threshold,
                }
            }
            LogFilter::ResponseFlag { flags } => {
                if flags.is_empty() {
                    // MEASURED (ADR-0145 PV-6): an empty `flags` matches any
                    // record that HAS a response flag set. "-" is the no-flag
                    // sentinel; envoy-rust renders a single token otherwise.
                    response_flags != "-"
                } else {
                    flags.iter().any(|f| f == response_flags)
                }
            }
        }
    }
}
```

In `file_sink.rs`, widen `FileSink::should_log` (`99-106`):

```rust
    /// Phase 70/71: returns `true` iff a record with final response `status` and
    /// `response_flags` token should be emitted. A sink with no filter always logs.
    pub fn should_log(&self, status: u16, response_flags: &str) -> bool {
        match &self.filter {
            Some(f) => f.should_log(status, response_flags),
            None => true,
        }
    }
```

Then fix the in-crate `should_log` test callers (`file_sink.rs ~352-361`) to pass a second arg
(`"-"` where the test only cares about the status path; the exact flag where it asserts the
response-flag path).

- [ ] **Step 4: Run tests to verify they pass + crate is green**

Run: `cargo test -p envoy-accesslog`
Expected: PASS (all `filter::` tests + the crate's existing tests).

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-accesslog/src/filter.rs crates/envoy-accesslog/src/file_sink.rs
git commit -m "phase 71 T4: LogFilter::ResponseFlag + widen should_log(status, response_flags) with empty-flags semantics"
```

---

## Task 5: H1 compile match (CF-70-1) + thread the widened emit gate

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` (`compile_access_log_filter` at `1736-1757`; the H1 emit
  gate at `1512`; H1 test call sites at `~4619`, `~4820-4925`)
- Test: `crates/envoy-http1/src/hcm.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `envoy_config::AccessLogFilter { status_code_filter, response_flag_filter }` (Task 1),
  `envoy_config::ResponseFlagFilter`, `envoy_accesslog::LogFilter::ResponseFlag` (Task 4),
  `FileSink::should_log(status, response_flags)` (Task 4), `record.response_flags` (exists).

- [ ] **Step 1: Write the failing test (a response_flag_filter compiles into a gating sink)**

Mirror the phase-70 `from_config_compiles_status_code_filter_into_sink` builder (grep it):

```rust
#[tokio::test]
async fn from_config_compiles_response_flag_filter_into_sink() {
    // Build an HCM config whose access_log[0] carries
    // filter.response_flag_filter.flags = ["NR"], reusing the phase-70 filtered
    // access-log config builder (parameterize it, or add a sibling helper).
    let cfg = hcm_config_with_response_flag_access_log(&["NR"]);
    let built = HCMConfig::from_config(&cfg, /* ..existing args.. */).await.unwrap();
    let sink = &built.access_log[0];
    assert!(sink.should_log(404, "NR"));   // kept
    assert!(!sink.should_log(503, "-"));   // dropped (no flag)
    assert!(!sink.should_log(200, "UH"));  // dropped (UH ∉ ["NR"])
}
```

- [ ] **Step 2: Run test to verify it fails (and the H1 build is currently broken)**

Run: `cargo test -p envoy-http1 from_config_compiles_response_flag_filter_into_sink`
Expected: FAIL to COMPILE — `compile_access_log_filter` still `expect()`s `status_code_filter`
(panics on a `response_flag_filter` config), and the emit gate at `1512` still calls the old
1-arg `should_log`.

- [ ] **Step 3: Convert the compile to a 2-arm match + thread the emit gate**

Replace `compile_access_log_filter` (`1736-1757`) with a full 2-arm match (CF-70-1 — the zero-arm
`expect()` is gone):

```rust
/// Phase 70/71 — translate a config `AccessLogFilter` into the runtime
/// `LogFilter`. `validate_access_logs` enforces exactly one oneof arm is set,
/// so the 0/2-arm cases are `unreachable!`.
fn compile_access_log_filter(f: &envoy_config::AccessLogFilter) -> envoy_accesslog::LogFilter {
    match (&f.status_code_filter, &f.response_flag_filter) {
        (Some(scf), None) => {
            let op = match scf.comparison.op {
                envoy_config::ComparisonOp::Eq => envoy_accesslog::FilterOp::Eq,
                envoy_config::ComparisonOp::Ge => envoy_accesslog::FilterOp::Ge,
                envoy_config::ComparisonOp::Le => envoy_accesslog::FilterOp::Le,
            };
            envoy_accesslog::LogFilter::StatusCode(envoy_accesslog::StatusCodeComparison {
                op,
                // `runtime_key` is RTDS-inert — always uses `default_value`.
                threshold: scf.comparison.value.default_value,
            })
        }
        (None, Some(rff)) => envoy_accesslog::LogFilter::ResponseFlag {
            flags: rff.flags.clone(),
        },
        _ => unreachable!("validated by validate_access_logs: exactly one filter arm is set"),
    }
}
```

Thread the widened emit gate at `1512`:

```rust
                if !sink.should_log(record.response_code, &record.response_flags) {
                    continue;
                }
```

Fix the H1 `should_log` test call sites (`~4619`, `~4820-4925`) to pass `&record.response_flags`
(or a literal token where the test constructs the args directly, e.g. `"-"` for a status-only test).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p envoy-http1 from_config_compiles_response_flag_filter_into_sink`
Expected: PASS. Also run `cargo test -p envoy-http1` — the phase-70 `status_code_filter` in-process
tests must still be green (the `(Some, None)` arm is byte-unchanged).

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-http1/src/hcm.rs
git commit -m "phase 71 T5: compile_access_log_filter 2-arm match (CF-70-1) + H1 widened emit gate"
```

---

## Task 6: H2 emit gate — thread the widened call (parity; inert-correct)

**Files:**
- Modify: `crates/envoy-http2/src/hcm.rs` (the H2 emit gate at `1135`; H2 test call sites)
- Test: `crates/envoy-http2/src/hcm.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `FileSink::should_log(status, response_flags)` (Task 4), `record.response_flags`
  (H2 builds it at `hcm.rs:1084-1104`; on the record at `1111`). The H2 sinks are the SAME compiled
  `FileSink`s (compiled once in envoy-http1's `HCMConfig::from_config`), so no H2 compile change.

- [ ] **Step 1: Write the failing test**

Mirror the phase-70 H2 access-log in-process test (grep `h2_filtered_sink_suppresses_below_threshold`):

```rust
#[tokio::test]
async fn h2_response_flag_filter_suppresses_no_flag() {
    // Seed the H2 config's access_log with a FileSink carrying a
    // ResponseFlag { flags: ["NR"] } filter (reuse the phase-70 H2 access-log
    // harness that builds `built.access_log = vec![sink]`), then assert:
    //   record.response_flags == "NR"  -> logged (1 line)
    //   record.response_flags == "-"   -> suppressed (0 lines)
    // OR, at minimum, assert `sink.should_log(404, "NR")` and
    // `!sink.should_log(503, "-")` through the H2 emit path.
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p envoy-http2 h2_response_flag_filter_suppresses_no_flag`
Expected: FAIL to COMPILE — the H2 emit gate at `1135` still calls the 1-arg `should_log`.

- [ ] **Step 3: Thread the widened H2 emit gate**

At `crates/envoy-http2/src/hcm.rs:1135`:

```rust
            if !sink.should_log(record.response_code, &record.response_flags) {
                continue;
            }
```

Fix the H2 `should_log` test call sites to pass `&record.response_flags` (or a literal token).

- [ ] **Step 4: Run tests to verify they pass + the WORKSPACE builds**

Run: `cargo test -p envoy-http2 h2_response_flag_filter_suppresses_no_flag`
then `cargo build --workspace` (the widening is now fully threaded — the workspace is green again).
Expected: PASS; workspace builds. Confirm the H2 fixtures' in-process tests (`0064`-`0070`) still green.

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-http2/src/hcm.rs
git commit -m "phase 71 T6: H2 widened emit gate (inert-correct parity)"
```

---

## Task 7: Differential driver — CF-70-3 suppression-only ordering-witness hardening (+ M70-R2)

**Files:**
- Modify: `tests/differential/src/lib.rs` (`run_http1_access_log_byte_exact_arm` at `6243`;
  `run_http2_access_log_byte_exact_arm` at `6386`)
- Test: `tests/differential/src/lib.rs` (`#[cfg(test)]` unit test for `expected_logged_count`, M70-R2)

**Interfaces:**
- Consumes: `expected_logged_count(probes)` (`1134`), `AccessLogByteExactProbe.expect_logged`
  (`1119`), `wait_file_lines` (`1682`), `ACCESS_LOG_FLUSH_WAIT` (`1675`).
- Produces: a `const CF70_3_SETTLE: std::time::Duration`; the ordering-witness precondition + a
  bounded suppression-only "no extra line" settle in both arms.

**Design (ADR-0145 PV-7):** the PRIMARY closure is the ordering witness — FileAccessLog flushes in
request order, so a suppression fixture's LAST probe must be a KEPT one; once its line is on disk
(which `wait_file_lines(expected_lines)` already waits for), every earlier record that was NOT
suppressed has ALSO flushed, making the EXISTING exact count-equality (`len() == expected_lines`)
soundly reject a leaked line instead of false-passing on an un-flushed one. A short bounded settle
(both containers still live) is added as defense-in-depth. BOTH guards are gated on
`has_suppression` (`expected_logged_count(probes) < probes.len()`) → the 30 existing all-kept
fixtures see ZERO change.

- [ ] **Step 1: Write the failing unit test (M70-R2 — the helper had no in-process witness)**

```rust
#[test]
fn expected_logged_count_counts_only_kept() {
    let p = |expect_logged: bool| AccessLogByteExactProbe {
        method: Http1Method::Get,
        path: "/x".into(),
        host: "h".into(),
        extra_headers: vec![],
        body: None,
        expected_status: 200,
        expect_logged,
    };
    assert_eq!(expected_logged_count(&[p(true), p(false), p(true)]), 2);
    assert_eq!(expected_logged_count(&[p(false), p(false)]), 0);
    assert_eq!(expected_logged_count(&[p(true)]), 1);
}
```

- [ ] **Step 2: Run test to verify it fails/passes**

Run: `cargo test -p differential expected_logged_count_counts_only_kept` (adjust the crate name to
`tests/differential`'s package name — grep its `Cargo.toml`).
Expected: PASS immediately (the helper exists; this test is the missing M70-R2 witness). If it does
not compile, fix the field list to match the current `AccessLogByteExactProbe`.

- [ ] **Step 3: Add the ordering-witness precondition + settle to both arms**

Add the const near `ACCESS_LOG_FLUSH_WAIT` (`1675`):

```rust
/// Phase 71 (CF-70-3): after the kept-line count is reached, a bounded settle
/// during which a filter-DROPPED record that was merely un-flushed would still
/// surface. Defense-in-depth behind the ordering witness (which is the primary
/// soundness guarantee). Only paid by suppression fixtures.
const CF70_3_SETTLE: std::time::Duration = std::time::Duration::from_secs(2);
```

In `run_http1_access_log_byte_exact_arm`, right after `let expected_lines = expected_logged_count(probes);`
(`6258`), add the ordering-witness precondition:

```rust
    let has_suppression = expected_lines < probes.len();
    if has_suppression {
        // CF-70-3 ordering witness (ADR-0145 PV-7): FileAccessLog flushes in
        // request order, so a suppression fixture's LAST probe must be KEPT —
        // once its line is on disk, every earlier non-suppressed record has
        // also flushed, making the exact count-equality below sound.
        assert!(
            probes.last().map(|p| p.expect_logged).unwrap_or(false),
            "CF-70-3: a suppression fixture's LAST probe must have expect_logged=true (ordering witness)"
        );
    }
```

After BOTH `wait_file_lines` calls (`6311`, `6328`) and BEFORE the shutdown at `6337`, add the
bounded settle (both containers still live):

```rust
    if has_suppression {
        // CF-70-3 defense-in-depth: with both proxies still running, allow a
        // bounded settle and confirm neither file grew past the kept-line count.
        tokio::time::sleep(CF70_3_SETTLE).await;
        let rust_have = std::fs::read_to_string(&envoy_rust_path)
            .map(|s| s.lines().count())
            .unwrap_or(0);
        let envoy_have = std::fs::read_to_string(&envoy_path)
            .map(|s| s.lines().count())
            .unwrap_or(0);
        if rust_have > expected_lines || envoy_have > expected_lines {
            bail!(
                "CF-70-3: an access log grew beyond {expected_lines} lines under a {CF70_3_SETTLE:?} \
                 settle (envoy_rust={rust_have}, envoy={envoy_have}) — a suppressed record leaked"
            );
        }
    }
```

(Confirm the exact local names for the two log paths — the recon shows them read into
`envoy_rust_path` / `envoy_path`; match whatever the current arm binds. The existing exact
count-equality bails at `6355`/`6363` are UNCHANGED — the ordering witness makes them sound.)

Make the IDENTICAL two additions in `run_http2_access_log_byte_exact_arm` (`6386`): the
`has_suppression` precondition after its `expected_lines` binding (`6401`), and the settle after its
two `wait_file_lines` calls (`6441`/`6451`).

- [ ] **Step 4: Run the unit test + build the arms**

Run: `cargo test -p differential expected_logged_count_counts_only_kept`
then `cargo build -p differential --tests`.
Expected: PASS; the arms compile. (The ordering-witness/settle logic is exercised live by fixture
`0077` in Task 8 — it has `has_suppression == true`.)

- [ ] **Step 5: Commit**

```bash
git add tests/differential/src/lib.rs
git commit -m "phase 71 T7: CF-70-3 suppression-only ordering-witness hardening + M70-R2 helper witness"
```

---

## Task 8: Differential fixture `0077-accesslog-response-flag-filter`

**Files:**
- Create: `tests/fixtures/0077-accesslog-response-flag-filter/envoy.yaml`
- Create: `tests/fixtures/0077-accesslog-response-flag-filter/envoy-rust.yaml`
- Create: `tests/fixtures/0077-accesslog-response-flag-filter/expectations.yaml`
- Create: `tests/fixtures/0077-accesslog-response-flag-filter/README.md`
- Create: `tests/differential/tests/access_log_response_flag_filter.rs` (mirror
  `tests/differential/tests/access_log_status_code_filter.rs`)

**Interfaces:**
- Consumes: `Driver::Http1AccessLogByteExact { probes, expected_access_log_paths }`, the config
  schema (Tasks 1-3), the H1 emit gate (Task 5), the CF-70-3 hardening (Task 7).

- [ ] **Step 1: Write the fixture configs (the failing differential)**

Copy the `0076-accesslog-status-code-filter/` files as the template. In BOTH `envoy.yaml` and
`envoy-rust.yaml`: keep ONE HCM listener with a file access log carrying the deterministic
`text_format_source` `STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)% FLAGS=%RESPONSE_FLAGS%\n`, swap the
`filter` block to:

```yaml
    filter:
      response_flag_filter:
        flags: ["NR"]
```

and set the routes to ONE `direct_response` route only (`/direct` → 503); a `/nowhere` request
matches NO route → synth 404 with `%RESPONSE_FLAGS% = NR`:

```yaml
                route_config:
                  name: r
                  virtual_hosts:
                    - name: v
                      domains: ["*"]
                      routes:
                        - match: { path: "/direct" }
                          direct_response: { status: 503, body: { inline_string: "direct\n" } }
```

Update `node.id` to `envoy-rust-phase-71-fixture-0077` in BOTH; set the per-proxy log paths to
`/tmp/0077-envoy-mount/access.log` (`envoy.yaml`) and `/tmp/0077-envoy-rust-mount/access.log`
(`envoy-rust.yaml`), matching the `expectations.yaml` pair. `clusters: []` (no backend). Keep the
`envoy.yaml` admin block / `0.0.0.0` bind / `generate_request_id: false` from `0076`.

`expectations.yaml` — **ordering witness: dropped probe FIRST, kept probe LAST** (Task 7 asserts it):

```yaml
driver:
  kind: http1_access_log_byte_exact
  expected_access_log_paths:
    envoy: /tmp/0077-envoy-mount/access.log
    envoy_rust: /tmp/0077-envoy-rust-mount/access.log
  probes:
    - { method: get, path: /direct,  host: envoy-rust.test, expected_status: 503, expect_logged: false }
    - { method: get, path: /nowhere, host: envoy-rust.test, expected_status: 404, expect_logged: true }
```

(Match the EXACT `expectations.yaml` shape of `0076` — grep it for whether `driver:`/`probes:` nest
as above vs. top-level; mirror it byte-for-structure.)

`README.md` — one section: what the fixture proves (`flags: ["NR"]` KEEPS the no-route 404 `NR`
line, DROPS the `direct_response` 503 `-` line → a single byte-identical `STATUS=404 PATH=/nowhere
FLAGS=NR` line across both proxies; no backend), the ordering-witness note (dropped first, kept last
→ CF-70-3), and cross-references (ADR-0144/ADR-0145).

Create `tests/differential/tests/access_log_response_flag_filter.rs`:

```rust
use std::path::PathBuf;

#[tokio::test]
async fn access_log_response_flag_filter() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0077-accesslog-response-flag-filter");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
```

- [ ] **Step 2: Build envoy-bin, then run the differential to verify it passes**

```bash
cargo build -p envoy-bin
cargo test -p differential access_log_response_flag_filter
```

Expected: PASS — both proxies emit the SAME single `STATUS=404 PATH=/nowhere FLAGS=NR` line; the
`/direct` 503 is suppressed on both; the CF-70-3 settle confirms no leaked line. (If Docker/bridge-IP
host-flakes hit locally, CI is authoritative — do not weaken the fixture. If envoy-bin REDs with
`unknown field response_flag_filter`, rebuild it — the harness runs `target/debug/envoy-bin`.)

- [ ] **Step 3: Commit**

```bash
git add tests/fixtures/0077-accesslog-response-flag-filter/ tests/differential/tests/access_log_response_flag_filter.rs
git commit -m "phase 71 T8: differential fixture 0077 (response_flag_filter flags=[NR], byte-exact, no backend)"
```

---

## Task 9: In-process coverage — end-to-end gate + regressions

**Files:**
- Test: `crates/envoy-http1/src/hcm.rs` (`#[cfg(test)]`) and/or `crates/envoy-accesslog/src/filter.rs`

**Interfaces:**
- Consumes: `HCMConfig::from_config` (Task 5), `FileSink::should_log` (Task 4), the config parse path
  (Tasks 1-3).

This task closes the SPEC §5 in-process coverage items not already asserted in Tasks 1-6: a
no-`filter` sink logs every record (regression) and a `status_code_filter` sink is byte-unchanged
(regression), both under the WIDENED `should_log`.

- [ ] **Step 1: Write the tests**

```rust
#[tokio::test]
async fn no_filter_sink_logs_every_record_after_widening() {
    // A sink built from a filterless AccessLog logs regardless of status/flags.
    let cfg = hcm_config_with_plain_access_log(); // existing filterless builder
    let built = HCMConfig::from_config(&cfg, /* ..args.. */).await.unwrap();
    let sink = &built.access_log[0];
    assert!(sink.should_log(200, "-"));
    assert!(sink.should_log(503, "NR"));
}

#[tokio::test]
async fn status_code_filter_unchanged_under_widening() {
    // Regression: the phase-70 GE-500 filter still gates on status, ignoring flags.
    let cfg = hcm_config_with_filtered_access_log("GE", 500); // phase-70 builder
    let built = HCMConfig::from_config(&cfg, /* ..args.. */).await.unwrap();
    let sink = &built.access_log[0];
    assert!(!sink.should_log(200, "NR"));
    assert!(sink.should_log(503, "-"));
}
```

(Reuse the phase-70 in-process HCM builders — grep `hcm_config_with_filtered_access_log` and the
filterless access-log test in `envoy-http1/src/hcm.rs`. If the filterless builder does not exist,
add a trivial sibling.)

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test -p envoy-http1 no_filter_sink_logs_every_record_after_widening status_code_filter_unchanged_under_widening`
Expected: PASS. (If either fails, the widening changed behavior it should not have — fix before proceeding.)

- [ ] **Step 3: Commit**

```bash
git add crates/envoy-http1/
git commit -m "phase 71 T9: in-process regressions (no-filter logs all; status_code unchanged) under widened should_log"
```

---

## Task 10: Fuzz corpus seed for `parse_bootstrap`

**Files:**
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/response_flag_filter.yaml`
- Modify: `crates/envoy-config/fuzz/.gitignore` (add one `!`-un-ignore line)

**Interfaces:** none (the existing `parse_bootstrap` fuzz target drives `envoy_config::parse_bootstrap`,
which now parses + validates the `response_flag_filter` sub-message). NO new fuzz target; NO
`ci.yml` edit (confirmed — `parse_bootstrap` is the sole config target).

- [ ] **Step 1: Add the seed**

Copy `crates/envoy-config/fuzz/corpus/parse_bootstrap/status_code_filter.yaml` to
`response_flag_filter.yaml`, changing `node.id`/`cluster` to `fuzz-71` and the `filter` block to:

```yaml
                    filter:
                      response_flag_filter:
                        flags: ["NR", "UF"]
```

(Keep it a minimal valid bootstrap the `parse_bootstrap` target accepts — one HCM listener, one file
access log carrying the filter, one `direct_response` route, the router http_filter, `clusters: []`.)

- [ ] **Step 2: Un-ignore it (or git will not track it)**

In `crates/envoy-config/fuzz/.gitignore`, add BEFORE the `artifacts/`/`target/` trailer, next to the
existing per-seed `!` lines:

```
!corpus/parse_bootstrap/response_flag_filter.yaml
```

- [ ] **Step 3: Verify it is tracked**

```bash
git add crates/envoy-config/fuzz/.gitignore crates/envoy-config/fuzz/corpus/parse_bootstrap/response_flag_filter.yaml
git ls-files crates/envoy-config/fuzz/corpus/parse_bootstrap/response_flag_filter.yaml   # must print the path
```

Expected: `git ls-files` prints the seed path (it is tracked). Optionally run the short-budget fuzz
(`cd crates/envoy-config && cargo +nightly fuzz run parse_bootstrap -- -max_total_time=15`), noting
`cargo fuzz` runs from the crate dir (memory `cargo-fuzz-runs-from-crate-dir-not-repo-root`).

- [ ] **Step 4: Commit**

```bash
git commit -m "phase 71 T10: parse_bootstrap corpus seed carrying response_flag_filter (+ un-ignore)"
```

---

## Task 11: `BEHAVIOR_CONTRACT.md` `response_flag_filter` subsection

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (a subsection under the access-log filter section,
  sibling to the phase-70 `status_code_filter` subsection)

- [ ] **Step 1: Add the subsection**

Record the MEASURED facts (ADR-0144 + ADR-0145): `filter: { response_flag_filter: { flags: [...] } }`
gates emission per sink; a record is KEPT iff its single `%RESPONSE_FLAGS%` token ∈ `flags` (the `-`
no-flag sentinel matches nothing non-empty); an EMPTY or absent `flags` matches any record that has
a flag set (keeps `NR`, drops `-`); `flags` accepts the full **29-token** v1.33.0 vocabulary for
load-parity, of which envoy-rust produces only `{NR, UH, UO, UC, UF, URX}` (the other 23
parsed-but-inert); `BOGUS`/lowercase tokens are fail-loud (`ConfigError::UnknownResponseFlag`);
`response_flag_filter` and `status_code_filter` are mutually-exclusive oneof arms (both-set is
fail-loud); a no-route 404 renders `NR` (kept by `flags:["NR"]`), a clean `direct_response` 503
renders `-` (dropped); a sink with no `filter` logs every record (unchanged).

- [ ] **Step 2: Commit**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 71 T11: BEHAVIOR_CONTRACT response_flag_filter subsection"
```

---

## Task 12: Verification gate dry-run (§7.5) — pre-state-4 self-check

> This is a fold-in convenience for the executor: the full §7.5 gate is the SEPARATE state-4
> verification session's job. Run it here only to surface breakage early; do NOT treat a green
> dry-run as the state-4 verdict.

**Files:** none (commands only).

- [ ] **Step 1: Run the workspace gate**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-targets
cargo build -p envoy-bin   # the differential harness runs target/debug/envoy-bin
cargo test --workspace --no-fail-fast 2>&1 | tee /tmp/claude-1000/-home-esa-git-envoy-rust/*/scratchpad/phase71-test.log
cargo deny check
```

Expected: fmt/clippy/build clean; `cargo test --workspace` green modulo the documented host-flake
set (adjudicate any RED with `--no-fail-fast` + full-output redirect, never `tail` — memory
`never-pipe-verification-runs-through-tail`; re-run each RED member in isolation naming its target
binary, `0 passed` is NOT a pass); `cargo deny check` clean (if a fresh unrelated RustSec advisory
REDs, patch-bump the dep per memory `cargo-deny-reds-on-unrelated-advisory`).

- [ ] **Step 2: (No commit — dry-run only.)** Record the outputs into `PROGRESS.md` during the
  state-3 execution session as each task lands.

---

## Self-Review

**Spec coverage** (SPEC §2.1 In scope → task):
1. Config schema (`ResponseFlagFilter` + the `response_flag_filter` arm) → **Task 1**.
2. Validation (29-token fail-loud; oneof cardinality now 2 arms) → **Tasks 3, 2**.
3. Compile + runtime predicate (`LogFilter::ResponseFlag`; widened `should_log`; CF-70-1 2-arm
   match; both HCM emit loops) → **Tasks 4, 5, 6**.
4. Differential fixture `0077` → **Task 8** (driver CF-70-3 hardening → **Task 7**).
5. In-process coverage (membership; `-` sentinel; empty-`flags`; inert token; cardinality;
   unknown-token; no-filter + status_code regressions) → **Tasks 4, 3, 2, 9**.
6. CF-70-3 closure → **Task 7** (ordering witness + settle) + **Task 8** (fixture probe order).
7. `BEHAVIOR_CONTRACT.md` subsection → **Task 11**.
8. `known-failures.txt` / conformance unchanged → no task (deliberately untouched).
9. §7.4 fuzz (corpus seed, no new target) → **Task 10**.

**Type consistency:** `ResponseFlagFilter { flags: Vec<String> }` (config) → `LogFilter::ResponseFlag
{ flags: Vec<String> }` (runtime), translated in `compile_access_log_filter` (Task 5).
`should_log(&self, status: u16, response_flags: &str) -> bool` is consistent across `LogFilter`
(Task 4), `FileSink` (Task 4), and both call sites (Tasks 5, 6). `ConfigError::UnknownResponseFlag {
token: String }` (Task 3). `const RESPONSE_FLAG_TOKENS: [&str; 29]` (Task 3). The empty-`flags`
runtime branch (`response_flags != "-"`) is the ONE MEASURED behavior (Task 4, ADR-0145 PV-6).

**Split gate (§6.1 / PV-5):** ~645 net LoC across 12 TDD tasks — well under the ~1500 LoC / ~25 task
gate. **No split** (ADR-0146 stays unfired). Consumes CF-70-1 + M70-R1, closes CF-70-3, folds in
M70-R2 (the `expected_logged_count` witness). No new subsystem / crate / dependency / fuzz target —
a config sub-message + a predicate variant + a one-`&str` signature widening + one bounded driver
hardening.
