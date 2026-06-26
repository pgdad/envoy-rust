# Phase 38 — `38-accesslog-json-format` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for
> tracking. TDD per task (`superpowers:test-driven-development`): test first, watch it fail, minimal impl,
> watch it pass, commit. Read with zero prior context (D-3.4).

**Goal:** Add the `json_format` access-log output mode to `envoy.extensions.access_loggers.file.v3.FileAccessLog`
— a sorted JSON object (one per request) over the EXISTING phase-32 command-operator engine + the EXISTING
`AccessLogRecord` + the verbatim-`FileSink`, behaviorally byte-equivalent to upstream Envoy v1.33.0.

**Architecture:** Widen `SubstitutionFormatString` into a `{text_format_source | json_format}` oneof
(`json_format` = a `BTreeMap<String,String>` of key → command-operator value string). A NEW sibling
compiled type `CompiledJsonFormat` compiles each value through the EXISTING `parse_format` and renders a
single sorted JSON object — TYPE-INFERRING single-operator values (number / string / `null`) per the locked
v1.33.0 behavior, hand-rolling the JSON escaping (no new dependency). A NEW `LogFormat` enum
(`Text(CompiledFormat) | Json(CompiledJsonFormat)`) on `FileSink.format` leaves the existing text/default
render path BYTE-FROZEN (the 45 existing fixtures stay green). One byte-exact differential fixture (`0046`)
proves the JSON envelope cross-proxy.

**Tech Stack:** Rust (pinned toolchain); `serde`/`serde_yaml` (existing) for config; `envoy-accesslog` (the
hand-rolled engine); `tests/differential` (testcontainers, the `Driver::Http1WithAccessLog` byte-exact
scrape). NO new crate, NO new dependency (D-3.2). `#![forbid(unsafe_code)]` holds (D-3.8).

**Empirical grounding (ADR-0092 — all facts locked against live `envoyproxy/envoy:v1.33.0`, digest
`sha256:56da5afd…0c2`):**
- **§A Key ordering = SORTED by UTF-8 bytes** (digits < uppercase < lowercase) = exactly `BTreeMap<String>`
  iteration order → the config model is `BTreeMap<String,String>` (NOT the once-projected `Vec`; NO custom
  serde).
- **§B Values are TYPE-INFERRED**, not all-strings: a value that is EXACTLY one operator → its native JSON
  type (numeric op present → unquoted number; string op present → quoted string; op absent → `null`); a
  value with literals/multi-segment → a quoted string via the existing engine (absent op → the `-`
  sentinel inside the string); a literal-only value → a quoted string (only OPERATORS are typed).
- **§C `typed_json_format` is NOT a v1.33.0 field** — the typed behavior is inherent to plain `json_format`;
  type inference is folded INTO this phase (mandatory for byte-exactness), not deferred.
- **§D** Compact separators `{"k":v,"k2":v2}`; one trailing `\n` per object; escaping = standard JSON
  (`"`→`\"`, `\`→`\\`, `\n`/`\t` short escapes, other C0 control → `\u00XX`, non-ASCII verbatim UTF-8, `/`
  NOT escaped) = byte-identical to `serde_json` defaults → hand-rolled.
- **§E Validity:** empty `json_format: {}` is VALID → emits `{}\n` (accept, no error); exactly-one-of
  `{text_format_source, json_format}` (BOTH-set AND NEITHER-set are boot-fatal); unknown key under
  `log_format` boot-fatal (existing `deny_unknown_fields`); malformed value-operator boot-fatal (reuse
  `InvalidAccessLogFormat`). ONE new `ConfigError` variant for the cardinality. All fatal (ADR-0049).
- **§F Authoritative fixture-`0046` line** (bare `GET /`, Host `envoy-rust.test`, `direct_response`
  `{status:200, body:"ok\n"}`):
  `{"bytes_rcvd":0,"bytes_sent":3,"flags":"-","method":"GET","mixed":"code-200","path":"/","protocol":"HTTP/1.1","status":200,"upstream":null}\n`

**Reuse map (do NOT rebuild — SPEC §4 / ADR-0092):** `crates/envoy-accesslog/src/command_operator.rs`
(`parse_format` `:161`, `Segment`/`Op` `:24-91`, `CompiledFormat` `:403`, `resolve_req` `:512`,
`resolve_resp` `:530`, `truncate_bytes` `:545`, `render_op` `:457`); `record.rs` (`AccessLogRecord`, 16
fields); `file_sink.rs` (`FileSink` `:34`, `new` `:47`, `emit` `:97` — writes `format.render(record)`
verbatim); `crates/envoy-config/src/bootstrap.rs` (`FileAccessLog` `:687`, `SubstitutionFormatString`
`:699`, `DataSourceInline` `:707`, `validate_access_logs` `:4350`, the `validate_data_source`
exactly-one-of template `:4442`); `crates/envoy-config/src/lib.rs` (`ConfigError` `:355-366`);
`crates/envoy-http1/src/hcm.rs` (`compiled_log_format` `:1254`, the sink-build loop `:201-217`);
`crates/envoy-http2/src/hcm.rs` (the `CompiledFormat::default()` site `:2155-2157`);
`tests/differential/src/access_log.rs` + `tests/differential/src/lib.rs:1015` (`AccessLogByteExactProbe`,
`assert_access_log_lines_byte_identical`); fixture `0040-accesslog-command-operators` (the `0046` template).

**Split gate (§6.1):** ~9 tasks / ~650–900 LoC — under the ~25-task / ~1500-LoC gate. ADR-0093 (split)
does NOT fire (confirmed by ADR-0092).

---

## File Structure

| File | Responsibility | Change |
|---|---|---|
| `crates/envoy-config/src/bootstrap.rs` | `SubstitutionFormatString` oneof; exactly-one + per-value validator | modify (`:699`, `:4350`) |
| `crates/envoy-config/src/lib.rs` | new `ConfigError::AmbiguousLogFormat` variant | modify (`:355`) |
| `crates/envoy-accesslog/src/json_format.rs` | NEW: `CompiledJsonFormat`, the typed single-op encoder, the hand-rolled JSON escaper | create |
| `crates/envoy-accesslog/src/command_operator.rs` | expose `Segment`/`Op`/`parse_format`/resolve helpers to the new module (visibility only) | modify |
| `crates/envoy-accesslog/src/log_format.rs` | NEW: `LogFormat` enum (`Text|Json`) + `render` + `From` impls | create |
| `crates/envoy-accesslog/src/file_sink.rs` | `FileSink.format: LogFormat`; `new(path, impl Into<LogFormat>)` | modify (`:34`,`:47`) |
| `crates/envoy-accesslog/src/lib.rs` | wire the two new modules + re-exports | modify |
| `crates/envoy-http1/src/hcm.rs` | `compiled_log_format` → `LogFormat` (Text|Json arm) | modify (`:1254`) |
| `crates/envoy-http2/src/hcm.rs` | default site already coerces via `Into` (no change); test ctors at `:1350`/`:1460` need `json_format: None` | modify (tests only) |
| `tests/fixtures/0046-accesslog-json-format/` | NEW differential fixture (4 files) | create |
| `docs/envoy-rust/BEHAVIOR_CONTRACT.md` | "Access log field mapping" → JSON subsection | modify |
| `crates/envoy-config/fuzz/corpus/parse_bootstrap/` | NEW `json_format` seed | create |

---

### Task 1: Config schema — widen `SubstitutionFormatString` to the `{text_format_source | json_format}` oneof

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs:697-701` (the struct) + `:4362-4368` (the validator reader)
- Modify (compile-fix — every reader of `.text_format_source` becomes `Option`; `grep -rn
  text_format_source crates/` to be exhaustive): `crates/envoy-http1/src/hcm.rs:1259` (the
  `compiled_log_format` reader) + the struct-literal constructors at `crates/envoy-http1/src/hcm.rs:1767`,
  `crates/envoy-http2/src/hcm.rs:1350`, `crates/envoy-http2/src/hcm.rs:1460` (each needs
  `text_format_source: Some(...), json_format: None`) + the assert reader at
  `crates/envoy-config/src/bootstrap.rs:~11009` (`fmt.text_format_source` → `.as_ref().unwrap()`)
- Test: `crates/envoy-config/src/bootstrap.rs` `#[cfg(test)]` (inline, the existing module)

- [ ] **Step 1: Write the failing test** — a `json_format` config parses into a sorted `BTreeMap`, and the
  existing `text_format_source` arm still parses.

```rust
#[test]
fn json_format_parses_into_sorted_btreemap() {
    let yaml = r#"
text_format_source: null
json_format:
  zebra: "%PROTOCOL%"
  alpha: "%RESPONSE_CODE%"
"#;
    let sfs: SubstitutionFormatString = serde_yaml::from_str(yaml).unwrap();
    let jf = sfs.json_format.expect("json_format set");
    // BTreeMap iteration is sorted by key (ADR-0092 §A): alpha before zebra.
    let keys: Vec<&str> = jf.keys().map(String::as_str).collect();
    assert_eq!(keys, vec!["alpha", "zebra"]);
    assert!(sfs.text_format_source.is_none());
}

#[test]
fn text_format_source_arm_still_parses() {
    let yaml = r#"text_format_source: { inline_string: "%RESPONSE_CODE%" }"#;
    let sfs: SubstitutionFormatString = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(sfs.text_format_source.unwrap().inline_string, "%RESPONSE_CODE%");
    assert!(sfs.json_format.is_none());
}
```

- [ ] **Step 2: Run to verify failure** — `cargo test -p envoy-config json_format_parses_into_sorted_btreemap`
  → FAIL (field `json_format` does not exist).

- [ ] **Step 3: Widen the struct.** Replace `bootstrap.rs:697-701` with:

```rust
/// Models `envoy.config.core.v3.SubstitutionFormatString` — the
/// `{text_format_source | json_format}` oneof (phase 38, ADR-0092). Exactly one
/// arm must be set; the cardinality is enforced by `validate_access_logs`
/// (`ConfigError::AmbiguousLogFormat`), not by serde. `json_format` is a
/// `google.protobuf.Struct` modelled as a `BTreeMap<String,String>` — the keys
/// emit in sorted (UTF-8-byte) order, matching v1.33.0's `json_format` wire
/// order (ADR-0092 §A); each value is a command-operator format string compiled
/// per-value via `envoy_accesslog::parse_format`. The deprecated `text_format`
/// scalar + `json_format_options`/`omit_empty_values`/`content_type` are deferred.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SubstitutionFormatString {
    #[serde(default)]
    pub text_format_source: Option<DataSourceInline>,
    #[serde(default)]
    pub json_format: Option<std::collections::BTreeMap<String, String>>,
}
```

- [ ] **Step 4: Make the two existing readers compile** (behavior unchanged until Task 2/7):
  - `validate_access_logs` (`:4362`): `if let Some(fmt) = &cfg.log_format` → guard the text arm with
    `if let Some(ds) = &fmt.text_format_source { parse_format(&ds.inline_string)…?; }` (json arm added in
    Task 2).
  - `compiled_log_format` (H1 `hcm.rs:1257`): `Some(s) => match &s.text_format_source { Some(ds) =>
    CompiledFormat::from_inline(&ds.inline_string)…, None => Ok(CompiledFormat::default()) }` — temporary;
    Task 7 returns `LogFormat`. (If any other site reads `.text_format_source`, `grep -rn
    text_format_source crates/` and adjust.)
  - Update any test constructing `SubstitutionFormatString { text_format_source: DataSourceInline {…} }`
    to `{ text_format_source: Some(DataSourceInline {…}), json_format: None }`.

- [ ] **Step 5: Run** — `cargo test -p envoy-config` (the two new tests PASS) AND `cargo build --workspace
  --all-targets` (widening the PUBLIC struct breaks downstream crates — the task must leave a GREEN tree,
  not surface the break later at Task 7).
- [ ] **Step 6: Commit** — `git add -A && git commit -m "phase 38 task 1: SubstitutionFormatString {text_format_source|json_format} oneof (BTreeMap, sorted) [ADR-0092]"`

---

### Task 2: Config validator — exactly-one-of cardinality + per-value `parse_format` + the new `ConfigError`

**Files:**
- Modify: `crates/envoy-config/src/lib.rs:355` (new variant), `crates/envoy-config/src/bootstrap.rs:4350`
  (`validate_access_logs`)
- Test: `bootstrap.rs` `#[cfg(test)]`

- [ ] **Step 1: Write the failing tests** (the §E dispositions):

```rust
// helper: build an AccessLog FileAccessLog with the given log_format YAML fragment, run validate.
// (Mirror the existing access-log validator tests in this module.)
#[test] fn both_arms_set_is_ambiguous() { /* text_format_source + json_format both Some → Err(AmbiguousLogFormat) */ }
#[test] fn neither_arm_set_is_ambiguous() { /* log_format present, both None → Err(AmbiguousLogFormat) */ }
#[test] fn empty_json_format_map_is_valid() { /* json_format: {} → Ok (ADR-0092 §E: Envoy boots, emits `{}\n`) */ }
#[test] fn malformed_json_format_value_is_invalid_format() { /* json_format: {a: "%NOPE%"} → Err(InvalidAccessLogFormat) */ }
#[test] fn valid_json_format_passes() { /* json_format: {code: "%RESPONSE_CODE%"} → Ok */ }
```

- [ ] **Step 2: Run to verify failure** — `cargo test -p envoy-config both_arms_set_is_ambiguous` → FAIL.

- [ ] **Step 3: Add the `ConfigError` variant** (`lib.rs`, after `InvalidAccessLogFormat:366`):

```rust
/// Phase 38 (ADR-0092 §E): a `log_format` (`SubstitutionFormatString`) set
/// NEITHER or BOTH of `{text_format_source, json_format}`. Exactly one arm is
/// required (the v1.33.0 oneof — both-set and neither-set are both boot-fatal).
#[error("log_format must set exactly one of text_format_source or json_format: {detail}")]
AmbiguousLogFormat { detail: String },
```

- [ ] **Step 4: Extend `validate_access_logs`** (`:4362`), replacing the temporary Task-1 guard:

```rust
if let Some(fmt) = &cfg.log_format {
    match (&fmt.text_format_source, &fmt.json_format) {
        (Some(ds), None) => {
            envoy_accesslog::parse_format(&ds.inline_string)
                .map_err(|e| crate::ConfigError::InvalidAccessLogFormat { detail: e.to_string() })?;
        }
        (None, Some(map)) => {
            // Empty map is VALID (ADR-0092 §E → emits `{}\n`); validate each value-operator.
            for value in map.values() {
                envoy_accesslog::parse_format(value)
                    .map_err(|e| crate::ConfigError::InvalidAccessLogFormat { detail: e.to_string() })?;
            }
        }
        (Some(_), Some(_)) => return Err(crate::ConfigError::AmbiguousLogFormat {
            detail: "both text_format_source and json_format are set".into() }),
        (None, None) => return Err(crate::ConfigError::AmbiguousLogFormat {
            detail: "neither text_format_source nor json_format is set".into() }),
    }
}
```

- [ ] **Step 5: Run** — `cargo test -p envoy-config` (5 new PASS; all existing pass).
- [ ] **Step 6: Commit** — `git commit -am "phase 38 task 2: exactly-one-of log_format validator + per-value parse + ConfigError::AmbiguousLogFormat [ADR-0092]"`

---

### Task 3: Hand-rolled JSON string escaper (`envoy-accesslog`)

**Files:**
- Create: `crates/envoy-accesslog/src/json_format.rs` (start the module with just the escaper)
- Modify: `crates/envoy-accesslog/src/lib.rs` (add `mod json_format;`)
- Test: `json_format.rs` `#[cfg(test)]`

- [ ] **Step 1: Write the failing test** (ADR-0092 §D — byte-identical to serde_json defaults):

```rust
#[test]
fn escapes_per_json_rules() {
    let cases = [
        ("ab", "ab"),                  // plain
        ("a\"b", "a\\\"b"),            // quote
        ("a\\b", "a\\\\b"),            // backslash
        ("a\nb", "a\\nb"),            // newline short escape
        ("a\tb", "a\\tb"),            // tab short escape
        ("a\u{0001}b", "a\\u0001b"),  // other C0 control → \u00XX
        ("a/b", "a/b"),                // forward slash NOT escaped
        ("café", "café"),             // non-ASCII verbatim UTF-8
    ];
    for (input, want) in cases {
        let mut out = String::new();
        json_escape_into(&mut out, input);
        assert_eq!(out, want, "input {input:?}");
    }
}
```

- [ ] **Step 2: Run to verify failure** — FAIL (`json_escape_into` undefined).

- [ ] **Step 3: Implement the escaper.** In `json_format.rs`:

```rust
//! json_format — the `json_format` access-log encoder (ADR-0092). Compiles a
//! sorted `BTreeMap<String,String>` of key → command-operator value string into
//! a `CompiledJsonFormat` that renders ONE sorted JSON object per request,
//! type-inferring single-operator values (number / string / null) per the
//! v1.33.0 wire behavior. Hand-rolled JSON escaping (no new dependency, D-3.2).
use std::fmt::Write as _;
use crate::command_operator::{parse_format, render_value_segments, Op, Segment};
use crate::record::AccessLogRecord;

/// Append `s` to `out` with JSON string-body escaping (ADR-0092 §D — matches
/// serde_json: short escapes for `\b \t \n \f \r \" \\`; `\u00XX` for other C0
/// controls; non-ASCII emitted as verbatim UTF-8; `/` NOT escaped). The caller
/// supplies the surrounding `"`.
pub(crate) fn json_escape_into(out: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{0009}' => out.push_str("\\t"),
            '\u{000A}' => out.push_str("\\n"),
            '\u{000C}' => out.push_str("\\f"),
            '\u{000D}' => out.push_str("\\r"),
            c if (c as u32) < 0x20 => { let _ = write!(out, "\\u{:04x}", c as u32); }
            c => out.push(c),
        }
    }
}
```

- [ ] **Step 4: Run** — `cargo test -p envoy-accesslog escapes_per_json_rules` → PASS.
- [ ] **Step 5: Commit** — `git commit -am "phase 38 task 3: hand-rolled JSON string escaper [ADR-0092]"`

---

### Task 4: Per-operator typed-value encoder (single-operator number/string/null rule, ADR-0092 §B)

**Files:**
- Modify: `crates/envoy-accesslog/src/command_operator.rs` — make `Segment`, `Op`, `parse_format`,
  `resolve_req`, `resolve_resp`, `truncate_bytes` reachable from `json_format.rs` (change `fn resolve_req`
  /`resolve_resp`/`truncate_bytes` to `pub(crate)`; `Segment`/`Op`/`parse_format` are already `pub`). Add a
  `pub(crate) fn render_value_segments(segments: &[Segment], record) -> String` that renders a
  `&[Segment]` exactly as `CompiledFormat::render` does (factor the existing loop body, or call a shared
  helper) — used for the mixed/string case.
- Modify: `crates/envoy-accesslog/src/json_format.rs` (add the typed encoder)
- Test: `json_format.rs` `#[cfg(test)]`

- [ ] **Step 1: Write the failing test** (the §B classification — assert the FULLY-ENCODED JSON token):

```rust
// rec(): a deterministic record (copy the one in command_operator.rs tests):
//   method POST, path /p, protocol HTTP/1.1, response_code 200, response_flags "-",
//   bytes_received 16, bytes_sent 433, duration 0ms, user_agent Some, authority Some,
//   upstream_host Some("1.2.3.4:80"), forwarded_for None, upstream_service_time None.
fn enc(value_fmt: &str, r: &AccessLogRecord) -> String {
    let mut out = String::new();
    encode_json_value(&mut out, &parse_format(value_fmt).unwrap(), r);
    out
}
#[test]
fn single_numeric_operator_emits_unquoted_number() {
    let r = rec();
    assert_eq!(enc("%RESPONSE_CODE%", &r), "200");
    assert_eq!(enc("%BYTES_SENT%", &r), "433");
    assert_eq!(enc("%BYTES_RECEIVED%", &r), "16");
    assert_eq!(enc("%DURATION%", &r), "0");
}
#[test]
fn single_string_operator_emits_quoted_string() {
    let r = rec();
    assert_eq!(enc("%REQ(:METHOD)%", &r), "\"POST\"");
    assert_eq!(enc("%PROTOCOL%", &r), "\"HTTP/1.1\"");
    assert_eq!(enc("%RESPONSE_FLAGS%", &r), "\"-\"");
}
#[test]
fn single_absent_operator_emits_null() {
    let r = rec(); // forwarded_for None, upstream_service_time None
    assert_eq!(enc("%REQ(X-FORWARDED-FOR)%", &r), "null");
    assert_eq!(enc("%RESP(X-ENVOY-UPSTREAM-SERVICE-TIME)%", &r), "null");
    let mut r2 = rec(); r2.upstream_host = None;
    assert_eq!(enc("%UPSTREAM_HOST%", &r2), "null");
}
#[test]
fn mixed_or_literal_emits_quoted_string_with_dash_sentinel() {
    let r = rec();
    assert_eq!(enc("code-%RESPONSE_CODE%", &r), "\"code-200\"");      // mixed → string
    assert_eq!(enc("x=%REQ(X-FORWARDED-FOR)%", &r), "\"x=-\"");        // absent op inside string → `-`
    assert_eq!(enc("1", &r), "\"1\"");                                  // literal-only → quoted string
}
```

- [ ] **Step 2: Run to verify failure** — FAIL (`encode_json_value` undefined).

- [ ] **Step 3: Implement.** In `json_format.rs`:

```rust
/// Encode ONE compiled value (`&[Segment]`) as a JSON value token into `out`
/// (ADR-0092 §B). Single-operator → the operator's native JSON type (numeric →
/// unquoted number; string present → quoted+escaped; absent → `null`).
/// Otherwise (literals / multi-segment / literal-only) → a quoted+escaped string
/// rendered through the existing engine (absent operator → the `-` sentinel).
pub(crate) fn encode_json_value(out: &mut String, segments: &[Segment], r: &AccessLogRecord) {
    if let [Segment::Op(op)] = segments {
        encode_single_op(out, op, r);
    } else {
        let s = render_value_segments(segments, r); // existing render semantics, `-` for absent
        out.push('"'); json_escape_into(out, &s); out.push('"');
    }
}

fn quote(out: &mut String, s: &str) { out.push('"'); json_escape_into(out, s); out.push('"'); }
fn quote_opt(out: &mut String, v: Option<&str>) { match v { Some(s) => quote(out, s), None => out.push_str("null") } }

fn encode_single_op(out: &mut String, op: &Op, r: &AccessLogRecord) {
    use std::fmt::Write as _;
    match op {
        // numeric, always present → unquoted number
        Op::ResponseCode  => { let _ = write!(out, "{}", r.response_code); }
        Op::BytesReceived => { let _ = write!(out, "{}", r.bytes_received); }
        Op::BytesSent     => { let _ = write!(out, "{}", r.bytes_sent); }
        Op::Duration      => { let _ = write!(out, "{}", r.duration.as_millis()); }
        // always-present strings → quoted
        Op::Protocol      => quote(out, &r.protocol),
        Op::ResponseFlags => quote(out, &r.response_flags),
        Op::StartTime     => quote(out, &crate::format_iso8601(r.start_time)),
        // Option-backed → null when absent, else quoted
        Op::UpstreamHost  => quote_opt(out, r.upstream_host.as_deref()),
        Op::DynamicMetadata { namespace, key } =>
            quote_opt(out, r.dynamic_metadata.get(namespace).and_then(|m| m.get(key)).map(String::as_str)),
        Op::Req { name, alt, truncate } => {
            let v = crate::command_operator::resolve_req(name, r)
                .or_else(|| alt.as_deref().and_then(|a| crate::command_operator::resolve_req(a, r)))
                .map(|s| crate::command_operator::truncate_bytes(s, *truncate));
            quote_opt(out, v);
        }
        Op::Resp { name, alt, truncate } => {
            let owned = crate::command_operator::resolve_resp(name, r)
                .or_else(|| alt.as_deref().and_then(|a| crate::command_operator::resolve_resp(a, r)));
            let v = owned.as_deref().map(|s| crate::command_operator::truncate_bytes(s, *truncate));
            quote_opt(out, v);
        }
    }
}
```

  > NOTE on `render_value_segments`: factor the existing `CompiledFormat::render` segment loop
  > (`command_operator.rs:434-440` + `render_op`) into a `pub(crate) fn render_value_segments(&[Segment],
  > &AccessLogRecord) -> String` and have `CompiledFormat::render` delegate to it — so the text path stays
  > byte-identical (verify the existing `command_operator`/`file_sink` tests still pass after the
  > extraction). CARRY FORWARD the M32-6 capacity pre-allocation (the `literal_len + 64` `with_capacity`)
  > into the extracted helper so `CompiledFormat::render` keeps its sizing. Do this extraction as the FIRST
  > edit of Step 3.
  > NOTE on `Op::DynamicMetadata`: its single-operator JSON classification (quoted-when-present /
  > `null`-when-absent) follows §B's general rule and was NOT separately recon'd in ADR-0092 §B (it is not
  > in fixture `0046`); the backstop test asserts our rule. If the state-4 verification ever exercises it
  > cross-proxy and Envoy diverges, record a carry-forward — do not block.

- [ ] **Step 4: Run** — `cargo test -p envoy-accesslog` (4 new PASS; ALL existing `command_operator`/
  `file_sink` tests still PASS — the text path is byte-frozen).
- [ ] **Step 5: Commit** — `git commit -am "phase 38 task 4: per-operator typed JSON value encoder (number/string/null) [ADR-0092]"`

---

### Task 5: `CompiledJsonFormat` — compile the map + render the sorted object

**Files:**
- Modify: `crates/envoy-accesslog/src/json_format.rs`
- Modify: `crates/envoy-accesslog/src/lib.rs` (re-export `CompiledJsonFormat`)
- Test: `json_format.rs` `#[cfg(test)]`

- [ ] **Step 1: Write the failing test** — the authoritative §F line + empty map + key escaping:

```rust
#[test]
fn renders_authoritative_fixture_line() {
    // Record matching the fixture-0046 probe (ADR-0092 §F): GET /, HTTP/1.1, 200,
    // flags "-", bytes_received 0, bytes_sent 3, upstream_host None.
    let r = fixture_record();
    let mut map = std::collections::BTreeMap::new();
    for (k, v) in [
        ("method","%REQ(:METHOD)%"), ("path","%REQ(:PATH)%"), ("protocol","%PROTOCOL%"),
        ("status","%RESPONSE_CODE%"), ("flags","%RESPONSE_FLAGS%"), ("bytes_rcvd","%BYTES_RECEIVED%"),
        ("bytes_sent","%BYTES_SENT%"), ("upstream","%UPSTREAM_HOST%"), ("mixed","code-%RESPONSE_CODE%"),
    ] { map.insert(k.to_string(), v.to_string()); }
    let cjf = CompiledJsonFormat::from_map(&map).unwrap();
    assert_eq!(cjf.render(&r),
        "{\"bytes_rcvd\":0,\"bytes_sent\":3,\"flags\":\"-\",\"method\":\"GET\",\"mixed\":\"code-200\",\"path\":\"/\",\"protocol\":\"HTTP/1.1\",\"status\":200,\"upstream\":null}\n");
}
#[test]
fn empty_map_renders_empty_object() {
    let cjf = CompiledJsonFormat::from_map(&std::collections::BTreeMap::new()).unwrap();
    assert_eq!(cjf.render(&fixture_record()), "{}\n"); // ADR-0092 §E
}
#[test]
fn key_is_json_escaped() {
    let mut map = std::collections::BTreeMap::new();
    map.insert("a\"b".to_string(), "%PROTOCOL%".to_string());
    let cjf = CompiledJsonFormat::from_map(&map).unwrap();
    assert_eq!(cjf.render(&fixture_record()), "{\"a\\\"b\":\"HTTP/1.1\"}\n");
}
#[test]
fn from_map_rejects_malformed_operator() {
    let mut map = std::collections::BTreeMap::new();
    map.insert("a".to_string(), "%NOPE%".to_string());
    assert!(CompiledJsonFormat::from_map(&map).is_err());
}
```

- [ ] **Step 2: Run to verify failure** — FAIL (`CompiledJsonFormat` undefined).

- [ ] **Step 3: Implement.** In `json_format.rs`:

```rust
use crate::command_operator::FormatParseError;

/// A compiled `json_format`: sorted (BTreeMap) key → compiled value segments
/// (ADR-0092 §A). `render` assembles ONE sorted JSON object per record.
#[derive(Debug, Clone, PartialEq)]
pub struct CompiledJsonFormat(std::collections::BTreeMap<String, Vec<Segment>>);

impl CompiledJsonFormat {
    /// Compile each value string via `parse_format`. Returns the first
    /// `FormatParseError` (surfaced at config-load as `InvalidAccessLogFormat`).
    pub fn from_map(map: &std::collections::BTreeMap<String, String>)
        -> Result<Self, FormatParseError> {
        let mut compiled = std::collections::BTreeMap::new();
        for (k, v) in map { compiled.insert(k.clone(), parse_format(v)?); }
        Ok(Self(compiled))
    }

    /// Render ONE sorted JSON object + trailing `\n` (ADR-0092 §A/§B/§D/§F).
    pub fn render(&self, record: &AccessLogRecord) -> String {
        let mut out = String::with_capacity(64 + self.0.len() * 16);
        out.push('{');
        for (i, (key, segments)) in self.0.iter().enumerate() {
            if i > 0 { out.push(','); }
            out.push('"'); json_escape_into(&mut out, key); out.push_str("\":");
            encode_json_value(&mut out, segments, record);
        }
        out.push_str("}\n");
        out
    }
}
```

- [ ] **Step 4: Run** — `cargo test -p envoy-accesslog` (4 new PASS).
- [ ] **Step 5: Commit** — `git commit -am "phase 38 task 5: CompiledJsonFormat compile + sorted-object render [ADR-0092]"`

---

### Task 6: `LogFormat` enum + `FileSink` wiring (keep existing call sites compiling via `Into`)

**Files:**
- Create: `crates/envoy-accesslog/src/log_format.rs`
- Modify: `crates/envoy-accesslog/src/lib.rs` (`mod log_format;` + re-export `LogFormat`)
- Modify: `crates/envoy-accesslog/src/file_sink.rs:34,47,76` (`format: LogFormat`; `new(path, impl Into<LogFormat>)`)
- Test: `file_sink.rs` `#[cfg(test)]`

- [ ] **Step 1: Write the failing test** — a `FileSink` built with a `CompiledJsonFormat` emits the JSON line:

```rust
#[tokio::test]
async fn file_sink_emits_json_object() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("access.log");
    let mut map = std::collections::BTreeMap::new();
    map.insert("status".to_string(), "%RESPONSE_CODE%".to_string());
    let fmt = crate::CompiledJsonFormat::from_map(&map).unwrap();
    let sink = FileSink::new(path.clone(), fmt).await.unwrap(); // CompiledJsonFormat: Into<LogFormat>
    sink.emit(&make_record()).await.unwrap();
    drop(sink);
    assert_eq!(read_to_string(&path).await, "{\"status\":200}\n");
}
```

- [ ] **Step 2: Run to verify failure** — FAIL (`FileSink::new` rejects `CompiledJsonFormat`).

- [ ] **Step 3: Implement.** `log_format.rs`:

```rust
//! LogFormat — the sink-level access-log format: the EXISTING text/default
//! `CompiledFormat` or the phase-38 `CompiledJsonFormat` (ADR-0092). `FileSink`
//! holds one of these and renders each record through it verbatim. The text arm
//! is byte-frozen — the JSON arm is a strict sibling.
use crate::command_operator::CompiledFormat;
use crate::json_format::CompiledJsonFormat;
use crate::record::AccessLogRecord;

#[derive(Debug, Clone, PartialEq)]
pub enum LogFormat { Text(CompiledFormat), Json(CompiledJsonFormat) }

impl LogFormat {
    pub fn render(&self, record: &AccessLogRecord) -> String {
        match self { LogFormat::Text(f) => f.render(record), LogFormat::Json(f) => f.render(record) }
    }
}
impl From<CompiledFormat> for LogFormat { fn from(f: CompiledFormat) -> Self { LogFormat::Text(f) } }
impl From<CompiledJsonFormat> for LogFormat { fn from(f: CompiledJsonFormat) -> Self { LogFormat::Json(f) } }
```

  Then in `file_sink.rs`: change field `format: CompiledFormat` → `format: LogFormat`; change `new(path:
  PathBuf, format: CompiledFormat)` → `new(path: PathBuf, format: impl Into<LogFormat>)` storing
  `format.into()`; same for `from_file_for_test`. `emit` is UNCHANGED (`self.format.render(record)`).
  The existing `CompiledFormat::default()`/`from_inline(...)` call sites in `file_sink.rs` tests keep
  compiling (their `CompiledFormat` coerces via `Into`).

- [ ] **Step 4: Run** — `cargo test -p envoy-accesslog` (the new test PASS; ALL existing `file_sink` tests
  PASS — text path byte-frozen).
- [ ] **Step 5: Commit** — `git commit -am "phase 38 task 6: LogFormat enum (Text|Json) on FileSink via Into [ADR-0092]"`

---

### Task 7: HCM construction wiring (H1 `compiled_log_format` → `LogFormat`; H2 default)

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs:1254-1266` (`compiled_log_format` returns `LogFormat`)
- Modify: `crates/envoy-http2/src/hcm.rs:2155-2157` (wrap default in `LogFormat::Text`)
- Test: `crates/envoy-http1/src/hcm.rs` `#[cfg(test)]` (extend the existing `compiled_log_format_*` tests)

- [ ] **Step 1: Write the failing test** — `compiled_log_format` picks the Json arm for a `json_format`
  config, the Text arm for `text_format_source`, and the default when `log_format` is absent:

```rust
#[test]
fn compiled_log_format_picks_json_arm() {
    let mut map = std::collections::BTreeMap::new();
    map.insert("c".to_string(), "%RESPONSE_CODE%".to_string());
    let file_cfg = envoy_config::FileAccessLog {
        path: "/tmp/x".into(),
        log_format: Some(envoy_config::SubstitutionFormatString {
            text_format_source: None, json_format: Some(map) }),
    };
    assert!(matches!(compiled_log_format(&file_cfg).unwrap(), envoy_accesslog::LogFormat::Json(_)));
}
// + compiled_log_format_picks_text_arm + compiled_log_format_falls_back_to_default (Text) —
//   update the EXISTING two tests for the new Option/return shape.
```

- [ ] **Step 2: Run to verify failure** — FAIL.

- [ ] **Step 3: Implement.** Rewrite `compiled_log_format` (`hcm.rs:1254`):

```rust
fn compiled_log_format(
    file_cfg: &envoy_config::FileAccessLog,
) -> Result<envoy_accesslog::LogFormat, Http1Error> {
    let map_err = |err: envoy_accesslog::FormatParseError| Http1Error::AccessLogFormat { message: err.to_string() };
    match &file_cfg.log_format {
        // exactly-one-of already enforced by the envoy-config validator (Task 2);
        // this build is defense-in-depth — prefer the set arm, default if neither.
        Some(s) => match (&s.text_format_source, &s.json_format) {
            (Some(ds), _) => Ok(envoy_accesslog::CompiledFormat::from_inline(&ds.inline_string).map_err(map_err)?.into()),
            (None, Some(map)) => Ok(envoy_accesslog::CompiledJsonFormat::from_map(map).map_err(map_err)?.into()),
            (None, None) => Ok(envoy_accesslog::CompiledFormat::default().into()),
        },
        None => Ok(envoy_accesslog::CompiledFormat::default().into()),
    }
}
```

  The sink-build loop (`:205`) is unchanged (`let format = compiled_log_format(file_cfg)?;
  FileSink::new(path, format)` — `format` is now `LogFormat`, accepted directly). Ensure
  `envoy_accesslog::{LogFormat, CompiledJsonFormat, FormatParseError}` are exported from
  `envoy-accesslog/src/lib.rs`. For H2 (`hcm.rs:2155`): `FileSink::new(path,
  envoy_accesslog::CompiledFormat::default())` already coerces via `Into` — no change needed unless the H2
  site also reads config `log_format` (it uses `default()`); leave as-is (the `.into()` is implicit).

- [ ] **Step 4: Run** — `cargo test -p envoy-http1 -p envoy-http2` → PASS. Then
  `cargo build --workspace --all-targets` clean.
- [ ] **Step 5: Commit** — `git commit -am "phase 38 task 7: HCM wires LogFormat (Text|Json) from log_format config [ADR-0092]"`

---

### Task 8: Differential fixture `0046-accesslog-json-format` (byte-exact JSON line)

**Files:**
- Create: `tests/fixtures/0046-accesslog-json-format/{envoy.yaml, envoy-rust.yaml, expectations.yaml, README.md}`
- Reference: fixture `0040-accesslog-command-operators` (the template — same `Driver::Http1WithAccessLog`
  `kind: http1_access_log_byte_exact`)

- [ ] **Step 1: Rebuild the debug binary** (the differential runs `target/debug/envoy-bin`; a new config
  key needs a fresh build — `host-docker-desktop` memory):

```bash
cargo build -p envoy-bin
```

- [ ] **Step 2: Author the fixture.** `envoy.yaml` + `envoy-rust.yaml` are IDENTICAL (copy `0040`'s shape;
  swap `text_format_source.inline_string` for the `json_format` map; use distinct mount paths
  `/tmp/0046-envoy-mount/` and `/tmp/0046-envoy-rust-mount/`). The `json_format` map (config order is
  irrelevant — both proxies sort):

```yaml
log_format:
  json_format:
    method: "%REQ(:METHOD)%"
    path: "%REQ(:PATH)%"
    protocol: "%PROTOCOL%"
    status: "%RESPONSE_CODE%"
    flags: "%RESPONSE_FLAGS%"
    bytes_rcvd: "%BYTES_RECEIVED%"
    bytes_sent: "%BYTES_SENT%"
    upstream: "%UPSTREAM_HOST%"
    mixed: "code-%RESPONSE_CODE%"
```

  Route: `direct_response: { status: 200, body: { inline_string: "ok\n" } }` (upstream-independent, like
  `0040`). `expectations.yaml` (`kind: http1_access_log_byte_exact`, one probe — bare `GET /`, Host
  `envoy-rust.test`), with the byte-exact expected line documented (ADR-0092 §F):

  `{"bytes_rcvd":0,"bytes_sent":3,"flags":"-","method":"GET","mixed":"code-200","path":"/","protocol":"HTTP/1.1","status":200,"upstream":null}`

  `README.md`: what the fixture proves (the JSON envelope byte-exact: sorted keys, typed number/string/null,
  compact separators, trailing `\n`), citing ADR-0092 §A/§B/§D/§F.

- [ ] **Step 3: Run the differential** (Docker; `0046` + the regression-equivalence witnesses):

```bash
cargo test -p differential 0046_accesslog_json_format -- --nocapture
cargo test -p differential 0040 0012 0041 0042   # text/default must stay byte-identical
```
  Expected: `0046` PASS (cross-proxy byte-identical JSON line); `0012`/`0040`/`0041`/`0042` PASS unchanged.
  > Documented LOCAL host false-REDs (bridge-IP `192.168.65.2`; differential fixtures under full-workspace
  > parallel load) are NOT regressions — re-run the fixture in isolation; CI is authoritative.

- [ ] **Step 4: Commit** — `git commit -am "phase 38 task 8: fixture 0046 byte-exact json_format access-log line [ADR-0092]"`

---

### Task 9: BEHAVIOR_CONTRACT + `parse_bootstrap` fuzz seed + lib re-exports + close-prep

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` ("Access log field mapping" section)
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/json_format_logger.yaml` (a seed exercising a
  `json_format` logger; `git ls-files` it after — the fuzz corpus `.gitignore` may need a `!`-un-ignore
  line, per the `fuzz-corpus-seed-gitignored` memory)
- Modify: `crates/envoy-accesslog/src/lib.rs` (confirm `CompiledJsonFormat`, `LogFormat`,
  `FormatParseError` are re-exported)

- [ ] **Step 1: Document the JSON wire shape** in BEHAVIOR_CONTRACT.md — a new "Access log `json_format`
  encoding (phase 38, ADR-0092)" subsection: keys SORTED by UTF-8 bytes (§A); single-operator values
  TYPE-INFERRED (numeric→number, string→quoted, absent→`null`) and mixed/literal values quoted strings with
  the `-` sentinel (§B); compact separators + trailing `\n` (§D); JSON escaping rules (§D); empty map →
  `{}\n` (§E); exactly-one-of `{text_format_source, json_format}` boot-fatal (§E). Quote the §F line.

- [ ] **Step 2: Add the fuzz seed** — a minimal bootstrap YAML with a `json_format` file logger. Verify it
  is TRACKED:

```bash
git add -f crates/envoy-config/fuzz/corpus/parse_bootstrap/json_format_logger.yaml
git ls-files crates/envoy-config/fuzz/corpus/parse_bootstrap/json_format_logger.yaml   # must print the path
```
  (No new fuzz TARGET — the existing `parse_bootstrap` + `accesslog_format_parse` cover the surface, ADR-0092.)

- [ ] **Step 3: Commit** — `git commit -am "phase 38 task 9: BEHAVIOR_CONTRACT json_format subsection + parse_bootstrap seed [ADR-0092]"`

---

## State-4 verification gate (next session, NOT this PLAN)

Per §7.5, the state-4 verification session runs and quotes into PROGRESS.md: `cargo build --workspace
--all-targets`; `cargo clippy --workspace --all-targets --all-features -- -D warnings`; `cargo fmt --all --
--check`; `cargo test --workspace`; `cargo deny check`; the existing `parse_bootstrap` +
`accesslog_format_parse` fuzz targets (short-budget, with the new seed); the differential surface (`0046`
green + all `0001`–`0045` green, incl. `0012`/`0040`/`0041`/`0042` byte-identical); h2spec ≥95% (unchanged).
The full §7.5 gate (a)–(f) + the state-5 code-review (REVIEW.md) close the phase.

## Acceptance (§7.5 preview)

(a) `0046` green (byte-exact JSON line) + (b) `0001`–`0045` green (regression-equivalence) + (c) h2spec ≥95%
+ (d) existing fuzz targets clean (new seed) — NO new target + (e) build/clippy/fmt/test/deny clean +
(f) REVIEW.md approved. `#![forbid(unsafe_code)]` holds; NO new crate/dependency. ONE new `ConfigError`
variant (`AmbiguousLogFormat`, ADR-0092 §E).
