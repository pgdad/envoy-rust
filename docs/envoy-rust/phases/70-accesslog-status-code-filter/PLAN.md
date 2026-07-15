# Phase 70 — access-log `status_code_filter` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Every task is TDD (D-3.1): RED → GREEN → commit, no exceptions.

**Goal:** Open the access-log **FILTER** subsystem by landing `status_code_filter`
(`envoy.config.accesslog.v3.AccessLogFilter.status_code_filter`) — a per-`AccessLog`-entry
predicate that emits a log record to its sink only when the final response code satisfies
`op(status, default_value)` for `op ∈ {EQ, GE, LE}` — behaviorally equivalent to
`envoyproxy/envoy:v1.33.0` under the differential contract.

**Architecture:** A new `AccessLog.filter: Option<AccessLogFilter>` config field (serde
`Option`-arm oneof + a cardinality validator, following the in-tree `SubstitutionFormatString`
precedent) is compiled — at HCM config-load time, exactly like the existing
`compiled_log_format` — into a NEW runtime predicate type owned by `crates/envoy-accesslog`
(`LogFilter`, with `should_log(status: u16) -> bool`). `FileSink` carries the compiled
`Option<LogFilter>`; both the HTTP/1.1 and HTTP/2 HCM per-sink emit loops gate emission on
`sink.should_log(record.response_code)`. The differential harness gains one bounded extension
(`expect_logged: bool` on the byte-exact probe) so a suppressed probe contributes no log line.

**Tech Stack:** Rust (workspace crates `envoy-config`, `envoy-accesslog`, `envoy-http1`,
`envoy-http2`, `tests/differential`), `serde`/`serde_yaml`, `thiserror`, `testcontainers`
(Docker differential against `envoyproxy/envoy:v1.33.0`).

## Global Constraints

- `#![forbid(unsafe_code)]` holds at every crate root (D-3.8) — do not add `unsafe`.
- Reference Envoy is pinned to `envoyproxy/envoy:v1.33.0` (D-3.7) — do not change the pin.
- **`cargo build -p envoy-bin` before ANY local differential run** — the harness runs
  `target/debug/envoy-bin`; a stale binary REDs with `unknown field` on the new `filter` key.
- Config-validity is all-fatal (ADR-0049): native (non-identical) `ConfigError` messages are
  permitted; the requirement is that the SAME class of configs is rejected/accepted as upstream.
- New config structs carry `#[serde(deny_unknown_fields)]` (the crate-wide convention; the only
  documented exception is `Node`). Every new oneof-arm field is `#[serde(default)]`.
- `ConfigError` is grow-only (D-3.5): append new variants, never rename/remove existing ones.
- Never weaken a fixture; never trim `tests/conformance/.../known-failures.txt`.
- Any `ROADMAP.md` row edit preserves all 6 cells and escapes a literal `|` as `\|`; rows
  `36`/`38`/`39`/`52`/`54` are already malformed — do NOT "fix" them (append-only).
- `next-prompt.txt` is gitignored (`.gitignore:9`) — never `git add` it.
- The response status is `u16` end-to-end (`AccessLogRecord.response_code: u16`); the comparison
  value is `u32` — compare by widening `status as u32` (lossless), never narrow `default_value`.
- CI is authoritative for the documented host-flake set (see `STATE.md` standing traps); a local
  differential RED in that set is NOT a regression.

**Measured facts this plan rests on** (from `SPEC.md` §0 + the state-2 PV recon, recorded in
ADR-0140 + ADR-0141):
- `status_code_filter.comparison { op: <EQ|GE|LE>, value: RuntimeUInt32 { default_value: u32,
  runtime_key: string } }`. `op ∈ {EQ, GE, LE}` exactly (NE / bogus REJECTED).
- `runtime_key` is PGV-mandatory (`min_len 1`, REJECTED when empty) but RTDS-inert (no runtime
  subsystem exists — the comparison always uses `default_value`).
- The gate reads the FINAL response code (`GE 500` keeps a 503, drops a 200). `direct_response`
  responses ARE logged; a `direct_response` 503 carries `%RESPONSE_FLAGS% = -`. NO backend needed.

---

## File Structure

**`crates/envoy-config/src/bootstrap.rs`** — add the serde types (`AccessLogFilter`,
`StatusCodeFilter`, `ComparisonFilter`, `ComparisonOp`, `RuntimeUInt32`) + the `AccessLog.filter`
field; extend `validate_access_logs` (`bootstrap.rs:5038`) with the filter cardinality +
empty-`runtime_key` checks.

**`crates/envoy-config/src/lib.rs`** — add the two new `ConfigError` variants
(`AmbiguousAccessLogFilter`, `EmptyStatusCodeFilterRuntimeKey`).

**`crates/envoy-accesslog/src/filter.rs`** (NEW module) — the runtime predicate `LogFilter` +
`StatusCodeComparison { op, threshold: u32 }` + `should_log(status: u16) -> bool`. Re-exported
from `crates/envoy-accesslog/src/lib.rs`.

**`crates/envoy-accesslog/src/file_sink.rs`** — `FileSink` gains `filter: Option<LogFilter>`;
`FileSink::new` gains a `filter` parameter; a `FileSink::should_log(status: u16) -> bool` method.

**`crates/envoy-http1/src/hcm.rs`** — compile `entry.filter` → `Option<LogFilter>` in
`HCMConfig::from_config` (`hcm.rs:203-217`); gate the per-sink emit loop (`hcm.rs:1508-1518`).

**`crates/envoy-http2/src/hcm.rs`** — gate the sibling per-sink emit loop (`hcm.rs:1135-1146`).

**`tests/differential/src/lib.rs`** — `expect_logged: bool` on `AccessLogByteExactProbe`
(`lib.rs:1104`); a shared `expected_logged_count` helper feeding the H1 arm
(`run_http1_access_log_byte_exact_arm`, `lib.rs:6225`) and the H2 arm (`lib.rs:6365`).

**`tests/fixtures/0076-accesslog-status-code-filter/`** (NEW) — `envoy.yaml`, `envoy-rust.yaml`,
`expectations.yaml`, `README.md`; plus the `#[test]` wiring in `tests/differential/`.

**`crates/envoy-config/fuzz/corpus/parse_bootstrap/status_code_filter.yaml`** (NEW seed) +
one `!`-un-ignore line in `crates/envoy-config/fuzz/.gitignore`.

**`docs/envoy-rust/BEHAVIOR_CONTRACT.md`** — a `status_code_filter` subsection under access-log.

---

## Task 1: Config schema — `AccessLogFilter` / `StatusCodeFilter` / `ComparisonFilter` / `ComparisonOp` / `RuntimeUInt32` + `AccessLog.filter`

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (add types near the `AccessLog` block at ~701-766; add the `filter` field to `AccessLog` at ~703-706)
- Test: `crates/envoy-config/src/bootstrap.rs` (`#[cfg(test)]` module — mirror the access-log parse tests)

**Interfaces:**
- Produces:
  - `pub struct AccessLog { pub name: String, pub typed_config: AccessLogTypedConfig, pub filter: Option<AccessLogFilter> }`
  - `pub struct AccessLogFilter { pub status_code_filter: Option<StatusCodeFilter> }`
  - `pub struct StatusCodeFilter { pub comparison: ComparisonFilter }`
  - `pub struct ComparisonFilter { pub op: ComparisonOp, pub value: RuntimeUInt32 }`
  - `pub enum ComparisonOp { Eq, Ge, Le }` (serde-renamed to `EQ`/`GE`/`LE`)
  - `pub struct RuntimeUInt32 { pub default_value: u32, pub runtime_key: String }`

- [ ] **Step 1: Write the failing parse test**

Add to the `bootstrap.rs` test module (mirror `rejects_hcm_with_empty_access_log_path` at
`bootstrap.rs:12803` for the YAML-builder shape — an HCM carrying an `access_log` entry):

```rust
#[test]
fn parses_status_code_filter_ge_500() {
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
                          value:
                            default_value: 500
                            runtime_key: unused
"#,
    );
    let bootstrap = crate::parse_bootstrap(&yaml).expect("should parse");
    let hcm_access_log = /* navigate to the HCM's access_log[0] */ first_access_log(&bootstrap);
    let filter = hcm_access_log.filter.as_ref().expect("filter present");
    let scf = filter.status_code_filter.as_ref().expect("status_code_filter present");
    assert_eq!(scf.comparison.op, crate::bootstrap::ComparisonOp::Ge);
    assert_eq!(scf.comparison.value.default_value, 500);
    assert_eq!(scf.comparison.value.runtime_key, "unused");
}
```

(Use the existing test helper that reaches the HCM `access_log` vec — grep the test module for how
`rejects_hcm_with_empty_access_log_path` builds its YAML and reads the parsed structure; reuse it.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p envoy-config parses_status_code_filter_ge_500`
Expected: FAIL — `AccessLog` has no field `filter` (unknown field under `deny_unknown_fields`), or
`ComparisonOp` does not exist.

- [ ] **Step 3: Add the types + the `filter` field**

In `bootstrap.rs`, add the `filter` field to `AccessLog` (after `typed_config`):

```rust
#[serde(deny_unknown_fields)]
pub struct AccessLog {
    pub name: String,
    pub typed_config: AccessLogTypedConfig,
    /// Phase 70: the per-record emission predicate (`AccessLogFilter` oneof).
    /// Absent → the sink logs every record (today's behavior). Present → the
    /// record is emitted to this sink only when the filter matches.
    #[serde(default)]
    pub filter: Option<AccessLogFilter>,
}
```

Add the new types (near the `SubstitutionFormatString` precedent at ~751-766):

```rust
/// The `AccessLogFilter` proto oneof. This phase models ONLY the
/// `status_code_filter` arm; future variants add more `Option` arms here.
/// Cardinality (exactly one arm set) is enforced by `validate_access_logs`,
/// NOT by serde — the `SubstitutionFormatString` precedent (`bootstrap.rs`).
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct AccessLogFilter {
    pub status_code_filter: Option<StatusCodeFilter>,
}

impl Default for AccessLogFilter {
    fn default() -> Self {
        Self { status_code_filter: None }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StatusCodeFilter {
    pub comparison: ComparisonFilter,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ComparisonFilter {
    pub op: ComparisonOp,
    pub value: RuntimeUInt32,
}

/// The upstream `ComparisonFilter.Op` enum. Exactly `{EQ, GE, LE}` (measured:
/// NE / bogus REJECTED). serde has no catch-all → any other token is a fatal
/// deserialize error (parity with upstream's unknown-enum rejection).
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Copy)]
pub enum ComparisonOp {
    #[serde(rename = "EQ")]
    Eq,
    #[serde(rename = "GE")]
    Ge,
    #[serde(rename = "LE")]
    Le,
}

/// The upstream `RuntimeUInt32`. `runtime_key` is PGV-mandatory (`min_len 1`)
/// but RTDS-inert here (no runtime subsystem) — the comparison always uses
/// `default_value`. Empty `runtime_key` is rejected by `validate_access_logs`.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RuntimeUInt32 {
    pub default_value: u32,
    pub runtime_key: String,
}
```

- [ ] **Step 4: Add a parse-rejection test for an unknown op token (confirms R-0.3)**

```rust
#[test]
fn rejects_status_code_filter_unknown_op() {
    let yaml = hcm_with_access_log_yaml(
        r#"                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/al.log
                    filter:
                      status_code_filter:
                        comparison:
                          op: NE
                          value: { default_value: 500, runtime_key: unused }
"#,
    );
    // Unknown enum token → serde deserialize error → ConfigError::Yaml.
    let err = crate::parse_bootstrap(&yaml).expect_err("NE must be rejected");
    assert!(matches!(err, crate::ConfigError::Yaml(_)), "got {err:?}");
}
```

- [ ] **Step 5: Run both tests to verify they pass**

Run: `cargo test -p envoy-config status_code_filter`
Expected: PASS (both `parses_status_code_filter_ge_500` and `rejects_status_code_filter_unknown_op`).

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-config/src/bootstrap.rs
git commit -m "phase 70 T1: AccessLogFilter/StatusCodeFilter/ComparisonFilter/ComparisonOp/RuntimeUInt32 serde schema + AccessLog.filter"
```

---

## Task 2: Validator — `AccessLogFilter` oneof cardinality (zero-variant fail-loud)

**Files:**
- Modify: `crates/envoy-config/src/lib.rs` (add `ConfigError::AmbiguousAccessLogFilter`)
- Modify: `crates/envoy-config/src/bootstrap.rs` (extend `validate_access_logs` at ~5038)
- Test: `crates/envoy-config/src/bootstrap.rs` (test module)

**Interfaces:**
- Consumes: `AccessLog.filter: Option<AccessLogFilter>` (Task 1).
- Produces: `ConfigError::AmbiguousAccessLogFilter { detail: String }`.

- [ ] **Step 1: Write the failing test (zero-variant filter is rejected)**

```rust
#[test]
fn rejects_access_log_filter_with_no_variant() {
    let yaml = hcm_with_access_log_yaml(
        r#"                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/al.log
                    filter: {}
"#,
    );
    let err = crate::parse_bootstrap(&yaml).expect_err("empty filter must be rejected");
    assert!(matches!(err, crate::ConfigError::AmbiguousAccessLogFilter { .. }), "got {err:?}");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p envoy-config rejects_access_log_filter_with_no_variant`
Expected: FAIL — `filter: {}` parses (all arms `Option`, default `None`) and is NOT yet rejected;
`AmbiguousAccessLogFilter` does not exist.

- [ ] **Step 3: Add the `ConfigError` variant**

In `lib.rs`, mirroring `AmbiguousLogFormat { detail: String }` (`lib.rs:447-451`):

```rust
    /// Phase 70: an `AccessLog.filter` (`AccessLogFilter` oneof) sets neither
    /// (or, in a future multi-variant phase, more than one) filter variant.
    /// Exactly one variant is required. Mirrors `AmbiguousLogFormat`.
    #[error("access_log filter must set exactly one filter variant: {detail}")]
    AmbiguousAccessLogFilter { detail: String },
```

- [ ] **Step 4: Extend `validate_access_logs`**

In `bootstrap.rs`, inside the `for entry in access_logs` loop of `validate_access_logs`
(`bootstrap.rs:5038`), after the `entry.name` allow-list check (~5044), add:

```rust
        if let Some(filter) = &entry.filter {
            // Count the set oneof arms (this phase has exactly one arm).
            let set_arms = [filter.status_code_filter.is_some()]
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
        }
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p envoy-config rejects_access_log_filter_with_no_variant`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-config/src/lib.rs crates/envoy-config/src/bootstrap.rs
git commit -m "phase 70 T2: AccessLogFilter oneof cardinality validator (zero-variant fail-loud)"
```

---

## Task 3: Validator — empty `runtime_key` fail-loud (PGV `min_len 1` parity)

**Files:**
- Modify: `crates/envoy-config/src/lib.rs` (add `ConfigError::EmptyStatusCodeFilterRuntimeKey`)
- Modify: `crates/envoy-config/src/bootstrap.rs` (extend `validate_access_logs`)
- Test: `crates/envoy-config/src/bootstrap.rs`

**Interfaces:**
- Consumes: `RuntimeUInt32.runtime_key: String` (Task 1), the `filter` cardinality path (Task 2).
- Produces: `ConfigError::EmptyStatusCodeFilterRuntimeKey`.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn rejects_status_code_filter_empty_runtime_key() {
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
                          value: { default_value: 500, runtime_key: "" }
"#,
    );
    let err = crate::parse_bootstrap(&yaml).expect_err("empty runtime_key must be rejected");
    assert!(
        matches!(err, crate::ConfigError::EmptyStatusCodeFilterRuntimeKey),
        "got {err:?}"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p envoy-config rejects_status_code_filter_empty_runtime_key`
Expected: FAIL — empty `runtime_key` parses and is not yet rejected; the variant does not exist.

- [ ] **Step 3: Add the `ConfigError` variant**

In `lib.rs`, mirroring `InvalidAccessLogPath` (`lib.rs:439`) / `EmptyNetworkRbacStatPrefix`:

```rust
    /// Phase 70: a `status_code_filter.comparison.value.runtime_key` is present
    /// but empty. Upstream enforces `min_len 1` (`RuntimeUInt32ValidationError`).
    /// The key is RTDS-inert here (the comparison always uses `default_value`),
    /// but load-time parity requires a non-empty key.
    #[error("status_code_filter runtime_key must be non-empty")]
    EmptyStatusCodeFilterRuntimeKey,
```

- [ ] **Step 4: Extend `validate_access_logs`**

Inside the `if let Some(filter) = &entry.filter` block (Task 2), after the cardinality check:

```rust
            if let Some(scf) = &filter.status_code_filter {
                if scf.comparison.value.runtime_key.is_empty() {
                    return Err(crate::ConfigError::EmptyStatusCodeFilterRuntimeKey);
                }
            }
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p envoy-config rejects_status_code_filter_empty_runtime_key`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-config/src/lib.rs crates/envoy-config/src/bootstrap.rs
git commit -m "phase 70 T3: empty runtime_key fail-loud (PGV min_len 1 parity, RTDS-inert)"
```

---

## Task 4: Runtime predicate — `LogFilter` + `should_log(status)` (EQ/GE/LE semantics)

**Files:**
- Create: `crates/envoy-accesslog/src/filter.rs`
- Modify: `crates/envoy-accesslog/src/lib.rs` (`pub mod filter;` + re-exports)
- Test: `crates/envoy-accesslog/src/filter.rs` (`#[cfg(test)]`)

**Interfaces:**
- Produces:
  - `pub enum FilterOp { Eq, Ge, Le }`
  - `pub struct StatusCodeComparison { pub op: FilterOp, pub threshold: u32 }`
  - `pub enum LogFilter { StatusCode(StatusCodeComparison) }`
  - `impl LogFilter { pub fn should_log(&self, status: u16) -> bool }`

This type is owned by `envoy-accesslog` (NOT `envoy-config`) so the emitter crate keeps NO
dependency on the config crate — the config→runtime translation happens in `envoy-http1`'s
`HCMConfig::from_config` (Task 6), exactly like `compiled_log_format`.

- [ ] **Step 1: Write the failing boundary tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn ge(t: u32) -> LogFilter { LogFilter::StatusCode(StatusCodeComparison { op: FilterOp::Ge, threshold: t }) }
    fn eq(t: u32) -> LogFilter { LogFilter::StatusCode(StatusCodeComparison { op: FilterOp::Eq, threshold: t }) }
    fn le(t: u32) -> LogFilter { LogFilter::StatusCode(StatusCodeComparison { op: FilterOp::Le, threshold: t }) }

    #[test]
    fn ge_500_boundary() {
        assert!(!ge(500).should_log(499));
        assert!(ge(500).should_log(500));
        assert!(ge(500).should_log(503));
    }

    #[test]
    fn eq_404_boundary() {
        assert!(!eq(404).should_log(403));
        assert!(eq(404).should_log(404));
        assert!(!eq(404).should_log(405));
    }

    #[test]
    fn le_200_boundary() {
        assert!(le(200).should_log(200));
        assert!(!le(200).should_log(201));
        assert!(le(200).should_log(100));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p envoy-accesslog filter::`
Expected: FAIL — module `filter` does not exist.

- [ ] **Step 3: Implement `filter.rs`**

```rust
//! Phase 70: the access-log FILTER predicate — the per-record emission gate
//! compiled from `envoy_config::AccessLogFilter`. This phase implements the
//! single `status_code_filter` variant.

/// The comparison operator (`ComparisonFilter.Op`): exactly `{EQ, GE, LE}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterOp {
    Eq,
    Ge,
    Le,
}

/// A `status_code_filter` comparison: `op(status, threshold)`. `threshold` is
/// `RuntimeUInt32.default_value` (the `runtime_key` override is RTDS-inert).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusCodeComparison {
    pub op: FilterOp,
    pub threshold: u32,
}

/// The compiled access-log filter. `None`-carrying sinks skip this type
/// entirely (they log every record); a `Some(LogFilter)` gates emission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogFilter {
    StatusCode(StatusCodeComparison),
}

impl LogFilter {
    /// Returns `true` iff a record with the given final response `status`
    /// should be emitted. Comparison is widened to `u32` (lossless; status is
    /// always in `u16` range).
    pub fn should_log(&self, status: u16) -> bool {
        match self {
            LogFilter::StatusCode(c) => {
                let s = status as u32;
                match c.op {
                    FilterOp::Eq => s == c.threshold,
                    FilterOp::Ge => s >= c.threshold,
                    FilterOp::Le => s <= c.threshold,
                }
            }
        }
    }
}
```

Add to `crates/envoy-accesslog/src/lib.rs` (near the other `pub mod` lines at ~16-23):

```rust
pub mod filter;
pub use filter::{FilterOp, LogFilter, StatusCodeComparison};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p envoy-accesslog filter::`
Expected: PASS (all three boundary tests).

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-accesslog/src/filter.rs crates/envoy-accesslog/src/lib.rs
git commit -m "phase 70 T4: envoy-accesslog LogFilter predicate + should_log (EQ/GE/LE)"
```

---

## Task 5: `FileSink` carries `Option<LogFilter>` + `should_log`

**Files:**
- Modify: `crates/envoy-accesslog/src/file_sink.rs`
- Test: `crates/envoy-accesslog/src/file_sink.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `LogFilter` (Task 4).
- Produces:
  - `FileSink::new(path: PathBuf, format: impl Into<LogFormat>, filter: Option<LogFilter>) -> Result<Self, AccessLogError>` (async — new `filter` param)
  - `FileSink::should_log(&self, status: u16) -> bool` (true when `filter` is `None`)

> **Blast-radius note:** `FileSink::new` gains a third parameter. Grep every caller
> (`grep -rn "FileSink::new" crates tests` — envoy-bin, envoy-http1 `from_config`, and any test
> sites) and pass `None` at each existing call in THIS task so the workspace still builds; Task 6
> then threads the real compiled filter at the `from_config` call. The test-only
> `from_file_for_test` constructor (`file_sink.rs:76`) also gains the `filter` param (pass `None`
> at its existing callers).

- [ ] **Step 1: Write the failing test**

```rust
#[tokio::test]
async fn should_log_gates_on_filter() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("al.log");
    let filter = Some(crate::LogFilter::StatusCode(crate::StatusCodeComparison {
        op: crate::FilterOp::Ge,
        threshold: 500,
    }));
    let sink = FileSink::new(path, crate::log_format::LogFormat::default(), filter)
        .await
        .unwrap();
    assert!(!sink.should_log(200));
    assert!(sink.should_log(503));

    // A sink with no filter logs everything.
    let dir2 = tempfile::tempdir().unwrap();
    let sink2 = FileSink::new(dir2.path().join("al2.log"), crate::log_format::LogFormat::default(), None)
        .await
        .unwrap();
    assert!(sink2.should_log(200));
    assert!(sink2.should_log(503));
}
```

(Adjust `LogFormat::default()` construction to the crate's actual default-format entry point —
grep the existing `file_sink.rs` tests for how they build a `FileSink` today and reuse that.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p envoy-accesslog should_log_gates_on_filter`
Expected: FAIL — `FileSink::new` takes 2 args, `should_log` does not exist.

- [ ] **Step 3: Implement**

Add the field to the struct (`file_sink.rs:34-38`):

```rust
pub struct FileSink {
    path: PathBuf,
    handle: Arc<Mutex<File>>,
    format: LogFormat,
    filter: Option<crate::LogFilter>,
}
```

Update `new` (`file_sink.rs:47`) — add the `filter` param and store it:

```rust
    pub async fn new(
        path: PathBuf,
        format: impl Into<LogFormat>,
        filter: Option<crate::LogFilter>,
    ) -> Result<Self, AccessLogError> {
        // ... unchanged file-open body ...
        Ok(Self {
            path,
            handle: Arc::new(Mutex::new(file)),
            format: format.into(),
            filter,
        })
    }
```

Update `from_file_for_test` (`file_sink.rs:76`) similarly (add `filter: Option<crate::LogFilter>`).

Add the method:

```rust
    /// Phase 70: returns `true` iff a record with final response `status`
    /// should be emitted to this sink. A sink with no filter always logs.
    pub fn should_log(&self, status: u16) -> bool {
        match &self.filter {
            Some(f) => f.should_log(status),
            None => true,
        }
    }
```

Then fix the existing `FileSink::new` / `from_file_for_test` callers in this crate's tests to pass
`None`.

- [ ] **Step 4: Run test to verify it passes + crate builds**

Run: `cargo test -p envoy-accesslog`
Expected: PASS (the crate's tests still green; `should_log_gates_on_filter` passes).

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-accesslog/src/file_sink.rs
git commit -m "phase 70 T5: FileSink carries Option<LogFilter> + should_log"
```

---

## Task 6: Compile `entry.filter` → `Option<LogFilter>` in `HCMConfig::from_config`

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` (`from_config` sink-build loop, `hcm.rs:203-217`)
- Test: `crates/envoy-http1/src/hcm.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `envoy_config::AccessLog.filter`, `FileSink::new(.., filter)` (Task 5), `LogFilter`.
- Produces: a private `fn compile_access_log_filter(f: &envoy_config::AccessLogFilter) -> envoy_accesslog::LogFilter` (translates config → runtime predicate). Every built `FileSink` now carries the compiled filter.

- [ ] **Step 1: Write the failing test**

```rust
#[tokio::test]
async fn from_config_compiles_status_code_filter_into_sink() {
    // Build a minimal HCM config with one file access_log carrying a GE 500 filter,
    // reusing the existing test config builder in this module (grep for the helper
    // used by the direct_response access-log tests, e.g. `hcm_config_from_config_*`).
    let cfg = hcm_config_with_filtered_access_log(/* op */ "GE", /* value */ 500);
    let built = HCMConfig::from_config(&cfg, /* ..existing args.. */).await.unwrap();
    let sink = &built.access_log[0];
    assert!(!sink.should_log(200));
    assert!(sink.should_log(503));
}
```

(Author `hcm_config_with_filtered_access_log` next to the existing access-log test builders in
this module; it produces an `envoy_config` HCM with an `access_log[0].filter.status_code_filter`.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p envoy-http1 from_config_compiles_status_code_filter_into_sink`
Expected: FAIL — `FileSink::new` is still called with `None` (Task 5), so `should_log(200)` is `true`.

- [ ] **Step 3: Implement the translation + thread it in**

Add a private helper in `hcm.rs`:

```rust
fn compile_access_log_filter(f: &envoy_config::AccessLogFilter) -> envoy_accesslog::LogFilter {
    // Validated at parse time: exactly one arm set (Task 2). This phase: status_code_filter.
    let scf = f
        .status_code_filter
        .as_ref()
        .expect("validated: exactly one filter arm is set");
    let op = match scf.comparison.op {
        envoy_config::bootstrap::ComparisonOp::Eq => envoy_accesslog::FilterOp::Eq,
        envoy_config::bootstrap::ComparisonOp::Ge => envoy_accesslog::FilterOp::Ge,
        envoy_config::bootstrap::ComparisonOp::Le => envoy_accesslog::FilterOp::Le,
    };
    envoy_accesslog::LogFilter::StatusCode(envoy_accesslog::StatusCodeComparison {
        op,
        threshold: scf.comparison.value.default_value, // runtime_key is RTDS-inert
    })
}
```

(Confirm the exact public path of `ComparisonOp` — export it from `envoy_config` if it is not
already reachable as `envoy_config::bootstrap::ComparisonOp`; add a `pub use` if needed.)

In the sink-build loop (`hcm.rs:204-217`), compile and pass the filter:

```rust
        for entry in &cfg.access_log {
            let filter = entry.filter.as_ref().map(compile_access_log_filter);
            match &entry.typed_config {
                envoy_config::AccessLogTypedConfig::FileAccessLog(file_cfg) => {
                    let format = compiled_log_format(file_cfg)?;
                    let sink = envoy_accesslog::FileSink::new(
                        std::path::PathBuf::from(&file_cfg.path),
                        format,
                        filter,
                    )
                    .await
                    .map_err(|err| Http1Error::AccessLogOpen { message: err.to_string() })?;
                    access_log_sinks.push(Arc::new(sink));
                }
            }
        }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p envoy-http1 from_config_compiles_status_code_filter_into_sink`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-http1/src/hcm.rs
git commit -m "phase 70 T6: compile AccessLog.filter into FileSink at HCMConfig::from_config"
```

---

## Task 7: HTTP/1.1 per-sink emit gate + counter correctness

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` (emit loop, `hcm.rs:1508-1518`)
- Test: `crates/envoy-http1/src/hcm.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `FileSink::should_log` (Task 5), the filter compiled by Task 6, `record.response_code`.

The current loop increments `access_logs_total` by `config.access_log.len()` BEFORE the loop, then
emits to every sink unconditionally. Gate per sink and move the counter INSIDE the gated branch so
suppressed sinks do not over-count (a no-op for unfiltered sinks — same count as today).

- [ ] **Step 1: Write the failing test**

```rust
#[tokio::test]
async fn h1_filtered_sink_suppresses_below_threshold() {
    // A filtered sink (GE 500) + a direct_response 200 route: after driving a 200,
    // the log file must be EMPTY; after driving a 503, it must hold exactly one line.
    // Reuse the in-process HCM harness used by the existing access-log tests
    // (grep for the direct_response 503 access-log test, e.g. around hcm.rs:4540).
    // 200 request → 0 lines; 503 request → 1 line.
}
```

Also assert the regression: a NO-filter sink still logs every record (drive a 200 → 1 line).

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p envoy-http1 h1_filtered_sink_suppresses_below_threshold`
Expected: FAIL — the sink logs the 200 (no gate yet), so the file has 1 line, not 0.

- [ ] **Step 3: Implement the gate**

Replace the pre-loop `add(len)` + unconditional loop (`hcm.rs:1508-1518`) with:

```rust
            for sink in &config.access_log {
                if !sink.should_log(record.response_code) {
                    continue;
                }
                config.stats.access_logs_total.inc();
                if let Err(err) = sink.emit(&record).await {
                    config.stats.access_logs_failed.inc();
                    tracing::warn!(error = ?err, "access log emission failed");
                }
            }
```

(Remove the prior `config.stats.access_logs_total.add(config.access_log.len() as u64);` at
`hcm.rs:1508-1511`. Confirm `access_logs_total` exposes an `inc()` — the stats counter type used
elsewhere in this file does; if only `add` exists, use `.add(1)`.)

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p envoy-http1 h1_filtered_sink_suppresses_below_threshold`
Expected: PASS. Also run `cargo test -p envoy-http1` to confirm no existing access-log/stats test
regressed (unfiltered sinks tick the identical count).

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-http1/src/hcm.rs
git commit -m "phase 70 T7: H1 per-sink should_log emit gate + per-emit access_logs_total"
```

---

## Task 8: HTTP/2 per-sink emit gate (parity; inert-correct)

**Files:**
- Modify: `crates/envoy-http2/src/hcm.rs` (emit loop, `hcm.rs:1135-1146`)
- Test: `crates/envoy-http2/src/hcm.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `FileSink::should_log`, `record.response_code`. The H2 loop reaches sinks via
  `config.inner.access_log`.

The H2 fixtures (`0064`-`0070`) set no filter, so this gate is inert-correct for them; wiring it now
keeps H2 from regressing when a future H2 filtered fixture lands.

- [ ] **Step 1: Write the failing test**

```rust
#[tokio::test]
async fn h2_filtered_sink_suppresses_below_threshold() {
    // H2 analog of the H1 test: a GE 500 filtered sink + a 200 response → 0 lines;
    // a 503 → 1 line. Reuse the H2 access-log in-process harness (grep the tests
    // that drive fixtures 0064-0070 in-process, seeding `built.access_log = vec![sink]`).
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p envoy-http2 h2_filtered_sink_suppresses_below_threshold`
Expected: FAIL — the H2 loop emits unconditionally.

- [ ] **Step 3: Implement the gate**

In the H2 emit loop (`hcm.rs:1135-1146`), mirror the H1 change:

```rust
        for sink in &config.inner.access_log {
            if !sink.should_log(record.response_code) {
                continue;
            }
            config.inner.stats.access_logs_total.inc();
            if let Err(err) = sink.emit(&record).await {
                config.inner.stats.access_logs_failed.inc();
                tracing::warn!(error = ?err, "access log emission failed");
            }
        }
```

(Remove the prior pre-loop `access_logs_total.add(config.inner.access_log.len() as u64)`.)

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p envoy-http2 h2_filtered_sink_suppresses_below_threshold`
Expected: PASS. Run `cargo test -p envoy-http2` to confirm `0064`-`0070` in-process tests still green.

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-http2/src/hcm.rs
git commit -m "phase 70 T8: H2 per-sink should_log emit gate (inert-correct, parity)"
```

---

## Task 9: Differential driver — `expect_logged` suppression extension

**Files:**
- Modify: `tests/differential/src/lib.rs` (`AccessLogByteExactProbe` at ~1104; the H1 arm at ~6225; the H2 arm at ~6365)
- Test: `tests/differential/src/lib.rs` (`#[cfg(test)]` unit test for the count helper)

**Interfaces:**
- Produces: `AccessLogByteExactProbe.expect_logged: bool` (serde default `true`) + a free helper
  `fn expected_logged_count(probes: &[AccessLogByteExactProbe]) -> usize`.

- [ ] **Step 1: Write the failing unit test for the count helper**

```rust
#[test]
fn expected_logged_count_excludes_suppressed() {
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
    assert_eq!(expected_logged_count(&[p(true), p(true)]), 2);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p differential expected_logged_count_excludes_suppressed` (adjust the crate name
to the `tests/differential` package name — grep its `Cargo.toml`).
Expected: FAIL — `expect_logged` field and `expected_logged_count` do not exist.

- [ ] **Step 3: Add the field + helper + rewire the arms**

Add to `AccessLogByteExactProbe` (`lib.rs:1104-1114`), after `expected_status`:

```rust
    /// Phase 70: `false` marks a probe whose response is expected to be
    /// SUPPRESSED by an access-log filter (contributes no log line). Defaults
    /// to `true` so all 27 pre-existing fixtures deserialize unchanged.
    #[serde(default = "default_expect_logged")]
    pub expect_logged: bool,
```

Add the default fn + the count helper (near `default_byte_exact_status` at `lib.rs:1116`):

```rust
fn default_expect_logged() -> bool {
    true
}

fn expected_logged_count(probes: &[AccessLogByteExactProbe]) -> usize {
    probes.iter().filter(|p| p.expect_logged).count()
}
```

In `run_http1_access_log_byte_exact_arm` (`lib.rs:6225`), change the line-count binding
(`lib.rs:6237`) from `let expected_lines = probes.len();` to:

```rust
    let expected_lines = expected_logged_count(probes);
```

This single binding already feeds all four downstream sites: the two `wait_file_lines` polls
(`lib.rs:6290`, `6307`) and the two line-count `bail!` assertions (`lib.rs:6334`, `6342`).
**This edit is mandatory, not cosmetic:** with a stale `probes.len()`, `wait_file_lines` would
never reach its target and burn the full 15s `ACCESS_LOG_FLUSH_WAIT` on every suppressed-probe run.

Make the identical one-line edit in the H2 arm `run_http2_access_log_byte_exact_arm`
(`lib.rs:6365`) for symmetry (a no-op for existing H2 fixtures — all `expect_logged == true`).
`assert_access_log_lines_byte_identical` (`access_log.rs:305`) needs NO change — it compares the
two proxies' line vectors to each other, and both shrink equally under matched suppression.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p differential expected_logged_count_excludes_suppressed`
Expected: PASS. Run `cargo build -p differential --tests` to confirm the arms compile.

- [ ] **Step 5: Commit**

```bash
git add tests/differential/src/lib.rs
git commit -m "phase 70 T9: differential AccessLogByteExact expect_logged suppression extension"
```

---

## Task 10: Differential fixture `0076-accesslog-status-code-filter`

**Files:**
- Create: `tests/fixtures/0076-accesslog-status-code-filter/envoy.yaml`
- Create: `tests/fixtures/0076-accesslog-status-code-filter/envoy-rust.yaml`
- Create: `tests/fixtures/0076-accesslog-status-code-filter/expectations.yaml`
- Create: `tests/fixtures/0076-accesslog-status-code-filter/README.md`
- Modify: the differential test registry (grep for how `0075` / the `Http1AccessLogByteExact`
  fixtures register their `#[test]` — mirror it for `0076`)

**Interfaces:**
- Consumes: `Driver::Http1AccessLogByteExact { probes, expected_access_log_paths }` (Task 9),
  the config schema (Tasks 1-3), the H1 gate (Task 7).

- [ ] **Step 1: Write the fixture configs (the failing differential)**

`envoy.yaml` and `envoy-rust.yaml` (initially identical, per §7.1): ONE HCM listener with a file
access log carrying a deterministic `text_format_source` (e.g.
`STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)% FLAGS=%RESPONSE_FLAGS%\n`) + the filter, and TWO
`direct_response` routes:

```yaml
access_log:
  - name: envoy.access_loggers.file
    typed_config:
      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
      path: <per-proxy path from expectations>
      log_format:
        text_format_source:
          inline_string: "STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)% FLAGS=%RESPONSE_FLAGS%\n"
    filter:
      status_code_filter:
        comparison:
          op: GE
          value:
            default_value: 500
            runtime_key: unused
# routes: /log -> direct_response 503, /nolog -> direct_response 200
```

(Model the listener/route/direct_response skeleton on an existing `Http1AccessLogByteExact` fixture
in `tests/fixtures/` — copy its structure, swap in the two routes + the `filter` block. Use the
per-proxy log paths the driver expects; grep an existing access-log fixture's `expectations.yaml`
for the `envoy`/`envoy_rust` path pair shape.)

`expectations.yaml` — two `AccessLogByteExactProbe`s:

```yaml
probes:
  - { method: GET, path: /log,   host: example.com, expected_status: 503, expect_logged: true }
  - { method: GET, path: /nolog, host: example.com, expected_status: 200, expect_logged: false }
expected_access_log_paths: { envoy: <path>, envoy_rust: <path> }
```

`README.md` — one paragraph: what the fixture proves (`GE 500` keeps the 503, drops the 200 → a
single byte-identical log line across both proxies; no backend), citing ADR-0140/0141.

- [ ] **Step 2: Build envoy-bin, then run the differential to verify it passes**

```bash
cargo build -p envoy-bin
cargo test -p differential fixture_0076   # adjust to the registered test name
```

Expected: PASS — both proxies emit the SAME single `STATUS=503 PATH=/log FLAGS=-` line; the
`/nolog` 200 is suppressed on both. (If Docker/bridge-IP host-flakes hit locally, CI is
authoritative — see the standing-trap flake set; do not weaken the fixture.)

- [ ] **Step 3: Commit**

```bash
git add tests/fixtures/0076-accesslog-status-code-filter/ tests/differential/
git commit -m "phase 70 T10: differential fixture 0076 (status_code_filter GE 500, byte-exact, no backend)"
```

---

## Task 11: In-process regression + RTDS-inert coverage

**Files:**
- Test: `crates/envoy-accesslog/src/filter.rs` (or the crate test module) and/or
  `crates/envoy-config/src/bootstrap.rs`

**Interfaces:**
- Consumes: `LogFilter::should_log` (Task 4), the config parse path (Tasks 1-3).

- [ ] **Step 1: Write the tests**

```rust
// RTDS-inert: a non-"unused" runtime_key still uses default_value (parses; the
// key is never consulted). Parse a filter with runtime_key: "some.key" and
// assert should_log behaves exactly as with runtime_key: "unused".
#[test]
fn runtime_key_is_rtds_inert() {
    // Parse two configs differing ONLY in runtime_key; compile both filters;
    // assert identical should_log outcomes across 200/499/500/503.
}

// Regression: an access_log with NO filter logs every record (compile → None →
// should_log always true). Assert a sink built from a filterless AccessLog
// returns should_log(200) == should_log(503) == true.
#[test]
fn no_filter_logs_every_record() { /* ... */ }
```

- [ ] **Step 2: Run tests to verify (they should pass against the landed impl)**

Run: `cargo test -p envoy-accesslog runtime_key_is_rtds_inert no_filter_logs_every_record`
Expected: PASS. (If either fails, the impl has a real gap — fix under TDD before proceeding.)

- [ ] **Step 3: Commit**

```bash
git add crates/envoy-accesslog/ crates/envoy-config/
git commit -m "phase 70 T11: in-process regression (no-filter logs all) + RTDS-inert runtime_key"
```

---

## Task 12: Fuzz corpus seed for `parse_bootstrap`

**Files:**
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/status_code_filter.yaml`
- Modify: `crates/envoy-config/fuzz/.gitignore` (add one `!`-un-ignore line)

**Interfaces:** none (the existing `parse_bootstrap` fuzz target already drives
`envoy_config::parse_bootstrap`, which now parses + validates the `filter` sub-message). NO new
fuzz target; NO `ci.yml` edit (confirmed by PV-5 / ADR-0141).

- [ ] **Step 1: Add the seed**

Create `status_code_filter.yaml` — a minimal valid bootstrap whose HCM access-log entry carries a
`status_code_filter` (copy `envoy-rust.yaml` from fixture `0076`, trimmed to a bootstrap the fuzz
target accepts; grep an existing `parse_bootstrap` seed such as `hcm_access_log_file.yaml` for the
minimal bootstrap envelope).

- [ ] **Step 2: Un-ignore it (or git will not track it)**

In `crates/envoy-config/fuzz/.gitignore`, add BEFORE the `artifacts/`/`target/` lines (matching the
existing per-seed exceptions):

```
!corpus/parse_bootstrap/status_code_filter.yaml
```

- [ ] **Step 3: Verify it is tracked + the corpus still parses**

```bash
git add crates/envoy-config/fuzz/.gitignore crates/envoy-config/fuzz/corpus/parse_bootstrap/status_code_filter.yaml
git status --porcelain crates/envoy-config/fuzz/corpus/parse_bootstrap/status_code_filter.yaml   # must show it staged, not ignored
git ls-files crates/envoy-config/fuzz/corpus/parse_bootstrap/status_code_filter.yaml              # must print the path
```

Expected: `git ls-files` prints the seed path (it is tracked). Optionally run the short-budget fuzz
locally (`cd crates/envoy-config && cargo +nightly fuzz run parse_bootstrap -- -max_total_time=15`),
noting `cargo fuzz` runs from the crate dir (memory `cargo-fuzz-runs-from-crate-dir-not-repo-root`).

- [ ] **Step 4: Commit**

```bash
git commit -m "phase 70 T12: parse_bootstrap corpus seed carrying status_code_filter (+ un-ignore)"
```

---

## Task 13: `BEHAVIOR_CONTRACT.md` `status_code_filter` subsection

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (a subsection under the access-log section)

- [ ] **Step 1: Add the subsection**

Record the MEASURED facts (R-0.2–R-0.6, ADR-0140/0141): an `AccessLog.filter` gates emission per
sink; `status_code_filter.comparison { op: EQ|GE|LE, value: RuntimeUInt32 { default_value,
runtime_key } }`; `op(status, default_value)` on the FINAL response code decides emission (`GE 500`
drops a 200, keeps a 503); a `direct_response` response IS logged and a `direct_response` 503 carries
`%RESPONSE_FLAGS% = -`; `runtime_key` is REQUIRED non-empty (load-parity, PGV `min_len 1`) but
RTDS-inert (comparison always uses `default_value`); the `AccessLogFilter` oneof cardinality is
fail-loud; an empty `runtime_key` is fail-loud; a sink with no `filter` logs every record (unchanged).

- [ ] **Step 2: Commit**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 70 T13: BEHAVIOR_CONTRACT status_code_filter subsection"
```

---

## Task 14: Verification gate dry-run (§7.5) — pre-state-4 self-check

> This is a fold-in convenience for the executor: the full §7.5 gate is the SEPARATE state-4
> verification session's job. Run it here only to surface breakage early; do NOT treat a green
> dry-run as the state-4 verdict.

**Files:** none (commands only).

- [ ] **Step 1: Run the workspace gate**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-targets
cargo test --workspace 2>&1 | tee /tmp/claude-1000/-home-esa-git-envoy-rust/*/scratchpad/phase70-test.log
cargo deny check
```

Expected: fmt/clippy/build clean; `cargo test --workspace` green modulo the documented host-flake
set (adjudicate any RED with `--no-fail-fast` + full-output redirect, never `tail` — memory
`never-pipe-verification-runs-through-tail`); `cargo deny check` clean (if a fresh unrelated RustSec
advisory REDs, patch-bump the dep per memory `cargo-deny-reds-on-unrelated-advisory`).

- [ ] **Step 2: (No commit — dry-run only.)** Record the outputs into `PROGRESS.md` during the
  state-3 execution session as each task lands.

---

## Self-Review

**Spec coverage** (SPEC §2.1 In scope → task):
1. Config schema (`AccessLog.filter` + the 5 new types) → **Task 1**.
2. Validation (oneof cardinality; empty `runtime_key`; op-enum rejection) → **Tasks 2, 3, 1(step 4)**.
3. Emission gate (`should_log` + per-sink H1 + H2) → **Tasks 4, 5, 6, 7, 8**.
4. Differential fixture `0076` → **Tasks 9, 10**.
5. In-process coverage (EQ/GE/LE boundaries; cardinality; empty `runtime_key`; RTDS-inert;
   no-filter regression) → **Tasks 4, 2, 3, 11, 7**.
6. `BEHAVIOR_CONTRACT.md` subsection → **Task 13**.
7. `known-failures.txt` / conformance unchanged → no task (deliberately untouched).
8. §7.4 fuzz (corpus seed, no new target) → **Task 12**.

**Type consistency:** `ComparisonOp {Eq,Ge,Le}` (config) ↔ `FilterOp {Eq,Ge,Le}` (runtime),
translated in `compile_access_log_filter` (Task 6). `response_code: u16` widened to `u32` in
`LogFilter::should_log` (Task 4). `expect_logged: bool` default `true` (Task 9). `FileSink::new`
3-arity is consistent across Tasks 5/6.

**Split gate (§6.1 / PV-5):** ~670 net LoC across 14 TDD tasks — well under the ~1500 LoC / ~25 task
gate. **No split** (ADR-0142 stays unfired). The finer 14-task granularity vs. SPEC §8's ~9-11
estimate is bite-sizing, not scope growth.

**No new subsystem / crate / dependency / fuzz target** — a config sub-message + a runtime predicate
+ a per-sink gate + one bounded harness extension.
