# Phase 32 — Access-log command-operator formatter — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Every task is TDD (`superpowers:test-driven-development`): write the failing test, watch it fail, implement minimally, watch it pass, commit.

**Goal:** Generalize the hardcoded `envoy-accesslog` Envoy-v3 default-format emitter into a configurable command-operator substitution engine driven by a per-`FileAccessLog` `log_format` text-format string, behaviorally equivalent to upstream Envoy v1.33.0 (differential: a byte-exact custom-format access-log line cross-proxy).

**Architecture:** A new command-operator **parser** compiles a format string into a `Vec<Segment>` (literals + typed operators) once at config-load; a **evaluator** renders the compiled format against the existing 15-field `AccessLogRecord` per request. `FileSink` carries the `CompiledFormat` and emits it VERBATIM (no auto-newline). The Envoy default format is re-expressed AS a parsed default-format string (incl. its trailing `\n`) so fixture `0012` stays byte-identical. `envoy-config`'s `FileAccessLog` gains a `log_format` field + a boot-fatal validator.

**Tech Stack:** Rust (workspace crates `envoy-accesslog`, `envoy-config`, `envoy-http1`, `envoy-http2`), `serde`/`serde_yaml`, `tokio`, `cargo fuzz` (libfuzzer), the `testcontainers` differential harness. `#![forbid(unsafe_code)]` holds throughout (D-3.8).

---

## §A — §6.2-LOCKED facts (ADR-0079; empirically verified against live `envoyproxy/envoy:v1.33.0`)

These are the GROUND TRUTH the implementation and the differential assert against. They are not projections.

1. **Wire path:** model ONLY `FileAccessLog.log_format.text_format_source.inline_string` (canonical — zero-warning, verbatim `/config_dump` round-trip). The deprecated `text_format` (inline) and top-level `format` are §2.2-DEFERRED.
2. **Absent value = `-`** (single dash, NEVER empty): a missing `%REQ(NAME)%`/`%RESP(NAME)%`, a no-upstream `%UPSTREAM_HOST%`, a clean-200 `%RESPONSE_FLAGS%` all render `-`. (Matches the existing `default_format.rs::push_or_dash`.)
3. **Operator byte forms:** `%REQ(:METHOD)%`→method; `%REQ(:AUTHORITY)%`→`host:port`; `%REQ(:PATH)%`→path; `%PROTOCOL%`→`HTTP/1.1`; `%RESPONSE_CODE%`→decimal; `%RESPONSE_FLAGS%`→`-` (clean); `%BYTES_RECEIVED%`/`%BYTES_SENT%`→decimal byte counts; `%UPSTREAM_HOST%`→`ip:port` (real upstream) / `-` (direct_response); `%RESP(NAME)%`→header value or `-`.
4. **Arg grammar:** `%OP%`, `%OP(ARG)%`, `%OP(ARG):N%`. `:N` is a BYTE-count truncation AFTER the closing paren. `?`-alternate: `%REQ(PRIMARY?ALT)%` uses `ALT` (another header name, or a pseudo-header like `:PATH`) when `PRIMARY` is absent; `:N` truncates the RESOLVED value. `%%` → a literal `%`.
5. **Boot-fatal (exit 1) at config-load:** an unknown operator keyword, a malformed/unterminated `%REQ(`, an empty `%()%`, AND a stray/lone/trailing single `%` (a single `%` is parsed as a command opener — to emit a literal `%` you MUST write `%%`). The Rust impl rejects all of these at config-parse with a new `ConfigError` variant.
6. **Default format string** (used when no `log_format`), byte-for-byte:
   `[%START_TIME%] "%REQ(:METHOD)% %REQ(X-ENVOY-ORIGINAL-PATH?:PATH)% %PROTOCOL%" %RESPONSE_CODE% %RESPONSE_FLAGS% %BYTES_RECEIVED% %BYTES_SENT% %DURATION% %RESP(X-ENVOY-UPSTREAM-SERVICE-TIME)% "%REQ(X-FORWARDED-FOR)%" "%REQ(USER-AGENT)%" "%REQ(X-REQUEST-ID)%" "%REQ(:AUTHORITY)%" "%UPSTREAM_HOST%"` + trailing `\n`.
7. **Trailing newline:** Envoy emits the format string VERBATIM — NO auto-appended `\n` for a custom `inline_string`. The default carries its own `\n`. ⇒ the engine renders verbatim; the default-format STRING includes `\n`; **`FileSink::emit` STOPS appending its own `\n`**. Fixture `0012` total bytes (line + `\n`) unchanged.

## §B — Operator support matrix (the curated DETERMINISTIC set; name→field allow-list)

The `AccessLogRecord` (`crates/envoy-accesslog/src/record.rs`) has NO generic header map — it has 15 named fields. `%REQ`/`%RESP` resolve via a FIXED allow-list. **Any header name / operator outside this matrix is a config-load error (Fact 5).**

| Operator (as written) | Backing `AccessLogRecord` field | Notes |
|---|---|---|
| `%REQ(:METHOD)%` | `method` | pseudo-header |
| `%REQ(:AUTHORITY)%` | `authority` | pseudo-header (Option → `-`) |
| `%REQ(:PATH)%` | `path` | pseudo-header (path already = x-envoy-original-path?:path at record-build) |
| `%REQ(X-ENVOY-ORIGINAL-PATH?:PATH)%` | `path` | the default-format spelling; the `?:PATH` alt resolves to the same `path` field |
| `%REQ(X-FORWARDED-FOR)%` | `forwarded_for` | Option → `-` |
| `%REQ(USER-AGENT)%` | `user_agent` | Option → `-` |
| `%REQ(X-REQUEST-ID)%` | `request_id` | Option → `-` (NON-deterministic — backstop-only, never in fixture 0040) |
| `%RESP(X-ENVOY-UPSTREAM-SERVICE-TIME)%` | `upstream_service_time` (ms) | Option → `-` (timing — backstop-only) |
| `%PROTOCOL%` | `protocol` | |
| `%RESPONSE_CODE%` | `response_code` | u16 decimal |
| `%RESPONSE_FLAGS%` | `response_flags` | String (`-` when clean) |
| `%BYTES_RECEIVED%` | `bytes_received` | u64 decimal |
| `%BYTES_SENT%` | `bytes_sent` | u64 decimal |
| `%UPSTREAM_HOST%` | `upstream_host` | Option → `-` |
| `%START_TIME%` | `start_time` | ISO-8601 via `format_iso8601` (NON-deterministic — backstop-only) |
| `%DURATION%` | `duration` (ms) | u128 decimal (NON-deterministic — backstop-only) |

**Header-name matching is case-insensitive** (ASCII), per the existing `access_log_header_value` lookup. **`:N` truncation** applies to the resolved string value (byte-count). **`?`-alt**: the alternate token is itself one of the above operator-arg names (header name or pseudo-header). Header names NOT in this matrix (e.g. `%REQ(X-CUSTOM)%`) → config-load error (no backing field — the generic header map is §2.2-deferred new plumbing).

> Deferred (ADR-0078 §2.2): `json_format`/`typed_json_format`; the deprecated wire paths; `%DYNAMIC_METADATA%`/`%FILTER_STATE%`; `%ROUTE_NAME%`/`%UPSTREAM_CLUSTER%`/`%REQUESTED_SERVER_NAME%`/`%RESPONSE_CODE_DETAILS%`/`%TRAILER%`/address operators (new record plumbing); `omit_empty_values`/`content_type`/custom `formatters`; a generic request/response header map.

---

## File Structure (decomposition)

- **Create** `crates/envoy-accesslog/src/command_operator.rs` — the parser (`parse_format` → `Vec<Segment>`) + the `Segment`/`Op` types + the compiled-format evaluator (`render(&CompiledFormat, &AccessLogRecord) -> String`) + the `FormatParseError`. One file: the format engine has one responsibility.
- **Modify** `crates/envoy-accesslog/src/default_format.rs` — the canonical default-format STRING constant (incl. `\n`) + `format()` becomes a thin wrapper over the engine (`render(&CompiledFormat::default(), record)`), preserving its existing tests as the equivalence oracle. `format_iso8601` UNCHANGED.
- **Modify** `crates/envoy-accesslog/src/file_sink.rs` — `FileSink` carries a `CompiledFormat`; `FileSink::new(path, format)`; `emit` renders via the compiled format and writes the bytes VERBATIM (drop the separate `\n` write at `:98`).
- **Modify** `crates/envoy-accesslog/src/lib.rs` — `pub mod command_operator;` + re-exports (`CompiledFormat`, `parse_format`, `FormatParseError`).
- **Create** `crates/envoy-accesslog/fuzz/` (Cargo.toml + `fuzz_targets/accesslog_format_parse.rs`) — the crate's first fuzz dir; fuzzes `parse_format`.
- **Modify** `crates/envoy-config/src/bootstrap.rs` — `FileAccessLog` gains `log_format: Option<SubstitutionFormatString>` (modeling `text_format_source.inline_string`); `validate_access_logs` compiles the format at boot → `ConfigError::InvalidAccessLogFormat`.
- **Modify** `crates/envoy-http1/src/hcm.rs` + `crates/envoy-http2/src/hcm.rs` — build the `CompiledFormat` from `file_cfg.log_format` (or default) and pass it to `FileSink::new` at the construction sites.
- **Create** `tests/fixtures/0040-accesslog-command-operators/` — `envoy.yaml`, `envoy-rust.yaml`, `inputs/`, `expectations.yaml`, `README.md`.
- **Modify** `tests/differential/src/access_log.rs` — add `assert_access_log_lines_byte_identical(envoy, envoy_rust)` (whole-line exact).
- **Modify** `tests/differential/src/lib.rs` — a `Driver` custom-format variant (scrape both files, whole-line compare) + register fixture 0040.
- **Modify** `.github/workflows/ci.yml` — wire the `accesslog_format_parse` fuzz short-budget step.
- **Modify** `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — extend "Access log field mapping" with the operator grammar + the deterministic/non-deterministic classification + the trailing-newline rule.

> **§6.1 split gate:** 8 tasks / ~650–950 LoC — UNDER the ~25-task / ~1500-LoC threshold. ADR-0080 (split) does NOT fire. If contact-with-reality blows a task past ~10 sub-steps, STOP and split per §6.2 of `BOOTSTRAP_PROMPT.md`.

---

## Task 1: Command-operator PARSER (the correctness gate)

**Files:**
- Create: `crates/envoy-accesslog/src/command_operator.rs`
- Modify: `crates/envoy-accesslog/src/lib.rs` (add `pub mod command_operator;`)

Parse a format string into segments. The `Op` enum carries ONLY the §B matrix operators; `Req`/`Resp` carry `{ name: String, alt: Option<String>, truncate: Option<usize> }`.

- [ ] **Step 1: Write failing tests** in `command_operator.rs` `#[cfg(test)]`:

```rust
// Literals + a simple operator.
#[test] fn parses_literal_and_operator() {
    let segs = parse_format("code=%RESPONSE_CODE% done").unwrap();
    assert_eq!(segs, vec![
        Segment::Literal("code=".into()),
        Segment::Op(Op::ResponseCode),
        Segment::Literal(" done".into()),
    ]);
}
// %% escape → literal '%'.
#[test] fn double_percent_is_literal() {
    assert_eq!(parse_format("a%%b").unwrap(), vec![Segment::Literal("a%b".into())]);
}
// REQ with pseudo-header, alt, and :N truncation.
#[test] fn parses_req_with_alt_and_truncate() {
    let segs = parse_format("%REQ(X-MISSING?:PATH):5%").unwrap();
    assert_eq!(segs, vec![Segment::Op(Op::Req {
        name: "x-missing".into(), alt: Some(":path".into()), truncate: Some(5),
    })]);
}
// Boot-fatal cases (Fact 5).
#[test] fn lone_percent_is_error()        { assert!(parse_format("50%done").is_err()); }
#[test] fn trailing_percent_is_error()    { assert!(parse_format("x%").is_err()); }
#[test] fn empty_operator_is_error()      { assert!(parse_format("%()%").is_err()); }
#[test] fn unterminated_is_error()        { assert!(parse_format("x=%REQ(").is_err()); }
#[test] fn unknown_operator_is_error()    { assert!(parse_format("%TOTALLY_UNKNOWN%").is_err()); }
// Unsupported (well-formed) header name → error (no backing field, §B).
#[test] fn unsupported_req_header_is_error() { assert!(parse_format("%REQ(X-CUSTOM)%").is_err()); }
```

- [ ] **Step 2: Run, verify FAIL** — `cargo test -p envoy-accesslog command_operator` → FAIL (`parse_format` undefined).

- [ ] **Step 3: Implement** `Segment`, `Op`, `FormatParseError`, and `parse_format`:
  - Scan byte-by-byte. `%` opens: if next byte is `%` → push literal `%`, advance 2. Else read until the matching `%`; the inner text is the operator; an unterminated/empty inner → `Err`.
  - Operator text grammar: `KEYWORD` or `KEYWORD(ARG)` optionally followed by `:N`. Split on the first `(`; the keyword maps to an `Op` variant (case-sensitive keyword per Envoy: `REQ`/`RESP`/`PROTOCOL`/`RESPONSE_CODE`/`RESPONSE_FLAGS`/`BYTES_RECEIVED`/`BYTES_SENT`/`UPSTREAM_HOST`/`START_TIME`/`DURATION`). Unknown keyword → `Err`.
  - For `REQ`/`RESP`: the arg (inside the parens) splits on the first `?` into `name`/`alt`; lowercase both for the case-insensitive lookup; a trailing `:N` after the `)` parses as `truncate` (decimal; non-numeric → `Err`). A `name` not in the §B matrix (for the operator's side) → `Err`.
  - Coalesce adjacent literals.

- [ ] **Step 4: Run, verify PASS** — `cargo test -p envoy-accesslog command_operator` → PASS.

- [ ] **Step 5: Commit** — `git add crates/envoy-accesslog/src/command_operator.rs crates/envoy-accesslog/src/lib.rs && git commit -m "phase 32 t1: access-log command-operator parser (the correctness gate) [ADR-0079]"`

---

## Task 2: Compiled-format EVALUATOR

**Files:**
- Modify: `crates/envoy-accesslog/src/command_operator.rs` (add `CompiledFormat` + `render`)
- Modify: `crates/envoy-accesslog/src/lib.rs` (re-export `CompiledFormat`)

`CompiledFormat(Vec<Segment>)`; `render(&self, &AccessLogRecord) -> String` evaluates each segment. Absent Option fields → `-` (Fact 2). `:N` byte-truncation on the resolved value. `?`-alt fallback.

- [ ] **Step 1: Write failing tests** over a synthetic `AccessLogRecord`:

```rust
fn rec() -> AccessLogRecord { /* method=POST, path=/p, authority=Some("h:1"),
    user_agent=Some("curl/8.20.0"), forwarded_for=None, response_code=200,
    response_flags="-", bytes_received=16, bytes_sent=433, upstream_host=Some("1.2.3.4:80"),
    request_id=None, upstream_service_time=None, protocol="HTTP/1.1", ... */ }

#[test] fn renders_deterministic_line() {
    let f = parse_format("m=%REQ(:METHOD)% code=%RESPONSE_CODE% ua=%REQ(USER-AGENT)% up=%UPSTREAM_HOST%").unwrap();
    assert_eq!(CompiledFormat(f).render(&rec()), "m=POST code=200 ua=curl/8.20.0 up=1.2.3.4:80");
}
#[test] fn absent_header_renders_dash() {
    let f = parse_format("xff=%REQ(X-FORWARDED-FOR)%").unwrap(); // forwarded_for=None
    assert_eq!(CompiledFormat(f).render(&rec()), "xff=-");
}
#[test] fn truncate_is_byte_count() {
    let f = parse_format("%REQ(USER-AGENT):5%").unwrap();
    assert_eq!(CompiledFormat(f).render(&rec()), "curl/"); // first 5 bytes
}
#[test] fn alt_used_when_primary_absent() {
    let f = parse_format("%REQ(X-FORWARDED-FOR?USER-AGENT)%").unwrap(); // xff absent → ua
    assert_eq!(CompiledFormat(f).render(&rec()), "curl/8.20.0");
}
```

- [ ] **Step 2: Run, verify FAIL** — `cargo test -p envoy-accesslog` → FAIL (`render` undefined).
- [ ] **Step 3: Implement** `render`: per-segment; `Op::Req`/`Resp` resolve name (then alt if `None`/absent) to the backing field via the §B map, default `-`, then byte-truncate to `:N` if set (slice on a char boundary — use `s.get(..n)` falling back to a byte-safe floor, OR truncate on `floor_char_boundary`-style logic; Envoy truncates by bytes — match byte-count, but never split mid-UTF-8 in a way that panics; document the boundary choice). Numeric ops format decimal.
- [ ] **Step 4: Run, verify PASS** — `cargo test -p envoy-accesslog` → PASS.
- [ ] **Step 5: Commit** — `git commit -am "phase 32 t2: compiled-format evaluator (render) [ADR-0079]"`

---

## Task 3: Default-format re-expression + `FileSink` newline refactor

**Files:**
- Modify: `crates/envoy-accesslog/src/default_format.rs` (default STRING const + `format()` → engine wrapper)
- Modify: `crates/envoy-accesslog/src/file_sink.rs` (`FileSink` carries `CompiledFormat`; `new(path, format)`; emit verbatim)
- Modify: ALL `FileSink::new` call sites (the signature gains a `format` param — **grep `FileSink::new` first; this list is per the plan-review inventory but verify**): in-crate `file_sink.rs` `#[cfg(test)]` sites (`:154`, `:189`, `:209`, `:261`) + `crates/envoy-http1/src/hcm.rs` test/helper sites (`:3580`, `:3705`, `:3908`, `:4281`) + `from_file_for_test` + `crates/envoy-bin/tests/access_log_file_sink.rs` if it constructs a sink. (The production loop sites — h1 `:206`, h2 `:1914` — are wired in Task 5.)

- [ ] **Step 1: Write failing test** in `default_format.rs` asserting the engine reproduces the old hardcoded output (the equivalence oracle):

```rust
#[test] fn compiled_default_matches_legacy_concatenator() {
    let record = make_baseline_record();
    // The PRESERVED hand-rolled concatenator output (no trailing newline):
    let legacy = legacy_format(&record); // rename old fn to legacy_format, keep it test-only
    // The engine output via the canonical default string (which now ends with '\n'):
    let engine = crate::command_operator::CompiledFormat::default().render(&record);
    assert_eq!(engine, format!("{legacy}\n"));
}
```
  Also add a `file_sink` test asserting a CUSTOM compiled format is written VERBATIM (no extra `\n`): a format with no trailing `\n` produces a file with no trailing `\n`.

- [ ] **Step 2: Run, verify FAIL** — `cargo test -p envoy-accesslog` → FAIL.
- [ ] **Step 3: Implement:**
  - `pub const DEFAULT_FORMAT: &str = "[%START_TIME%] \"%REQ(:METHOD)% %REQ(X-ENVOY-ORIGINAL-PATH?:PATH)% %PROTOCOL%\" %RESPONSE_CODE% %RESPONSE_FLAGS% %BYTES_RECEIVED% %BYTES_SENT% %DURATION% %RESP(X-ENVOY-UPSTREAM-SERVICE-TIME)% \"%REQ(X-FORWARDED-FOR)%\" \"%REQ(USER-AGENT)%\" \"%REQ(X-REQUEST-ID)%\" \"%REQ(:AUTHORITY)%\" \"%UPSTREAM_HOST%\"\n";` (note the trailing `\n`, Fact 6+7).
  - `impl Default for CompiledFormat { fn default() -> Self { parse_format(DEFAULT_FORMAT).expect("default format is valid") } }` (a `parse_format` unit test guards this).
  - `FileSink` gains `format: CompiledFormat`; `FileSink::new(path, format)`; `emit` does `let line = self.format.render(record); file.write_all(line.as_bytes())` and **removes** the separate `write_all(b"\n")` at `:98`.
  - Add a `CompiledFormat::from_inline(s: &str) -> Result<Self, FormatParseError>` constructor (`parse_format(s).map(CompiledFormat)`) for Task 5 + `impl Default`.
  - Update EVERY `FileSink::new` call site enumerated in the Files block above (grep to confirm none missed) to pass a `CompiledFormat` — the test/helper sites pass `CompiledFormat::default()`; the `from_file_for_test` constructor takes a format param. `cargo build --workspace --all-targets` must be clean before commit.
- [ ] **Step 4: Run, verify PASS** — `cargo test -p envoy-accesslog -p envoy-http1` → PASS. Confirm the `default_format::tests` exact-suffix tests still pass (now via the engine path).
- [ ] **Step 5: Commit** — `git commit -am "phase 32 t3: re-express default format through the engine + FileSink verbatim-emit refactor [ADR-0079]"`

---

## Task 4: `log_format` config field + boot-fatal validator

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (`FileAccessLog.log_format`, `SubstitutionFormatString`, `validate_access_logs`, `ConfigError::InvalidAccessLogFormat`)
- Modify: `crates/envoy-config/src/error.rs` (or wherever `ConfigError` lives) for the new variant

Model only the modern path (Fact 1):

```rust
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FileAccessLog {
    pub path: String,
    #[serde(default)]
    pub log_format: Option<SubstitutionFormatString>,
}
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SubstitutionFormatString { pub text_format_source: DataSourceInline }
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DataSourceInline { pub inline_string: String }
```

- [ ] **Step 1: Write failing tests** (in `bootstrap.rs` tests): (a) a YAML with `log_format.text_format_source.inline_string: "%RESPONSE_CODE%"` parses + validates OK; (b) an unknown operator (`"%NOPE%"`) → `ConfigError::InvalidAccessLogFormat`; (c) a malformed `"%REQ("` → same error; (d) absent `log_format` parses OK (default applies).
- [ ] **Step 2: Run, verify FAIL** — `cargo test -p envoy-config access_log` → FAIL.
- [ ] **Step 3: Implement** the structs + add to `validate_access_logs` (`:4009`): when `cfg.log_format` is `Some`, call `envoy_accesslog::parse_format(&s.text_format_source.inline_string)` and map `Err` → `ConfigError::InvalidAccessLogFormat { detail }`. (Add `envoy-accesslog` as an `envoy-config` dev/normal dep IF not already present — check `crates/envoy-config/Cargo.toml`; if a dependency cycle risk, instead expose a lightweight validate entry or move `parse_format` so config can call it. Verify no cycle: envoy-config must not already be a dep of envoy-accesslog — it is not.)
- [ ] **Step 4: Run, verify PASS** — `cargo test -p envoy-config` → PASS.
- [ ] **Step 5: Commit** — `git commit -am "phase 32 t4: FileAccessLog.log_format config field + boot-fatal format validator [ADR-0079]"`

---

## Task 5: Wire the compiled format from config → `FileSink` (H1 + H2 HCM)

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` (`:201-211` construction loop)
- Modify: `crates/envoy-http2/src/hcm.rs` (the symmetric site)

- [ ] **Step 1: Write failing test** — an in-process HCM test (or extend an existing one): build an HCM from a config whose `FileAccessLog` carries `log_format: "%REQ(:METHOD)% %RESPONSE_CODE%"`, drive a request, read the sink's file, assert the line == `"GET 200"` (no `\n`, deterministic). (If an in-process HCM harness is heavy, assert at the construction layer: the built `FileSink` renders the expected line for a synthetic record.)
- [ ] **Step 2: Run, verify FAIL.**
- [ ] **Step 3: Implement** — in the construction loop, compute `let format = match &file_cfg.log_format { Some(s) => CompiledFormat::from_inline(&s.text_format_source.inline_string)?, None => CompiledFormat::default() };` then `FileSink::new(path, format)`. (The config already validated the string, so `from_inline` here is infallible / re-parses; prefer threading the already-parsed form, but a re-parse is acceptable since validation guarantees success — document it.) Apply identically in H2.
- [ ] **Step 4: Run, verify PASS** — `cargo test -p envoy-http1 -p envoy-http2` → PASS.
- [ ] **Step 5: Commit** — `git commit -am "phase 32 t5: thread the compiled log_format from config into FileSink (H1+H2 HCM) [ADR-0079]"`

---

## Task 6: Fixture 0040 + whole-line byte-exact harness comparator

**Files:**
- Create: `tests/fixtures/0040-accesslog-command-operators/{envoy.yaml,envoy-rust.yaml,inputs/,expectations.yaml,README.md}`
- Modify: `tests/differential/src/access_log.rs` (`assert_access_log_lines_byte_identical`)
- Modify: `tests/differential/src/lib.rs` (a custom-format `Driver` variant + fixture registration)

The fixture: an H1 listener + a file access logger with a DETERMINISTIC-ONLY custom `log_format`, routing to the existing `http1-echo-server` cluster (so `%UPSTREAM_HOST%`/`%RESP(...)%` are exercised). Example format (no timing/UUID operators → whole line byte-exact):
`m=%REQ(:METHOD)% p=%REQ(:PATH)% proto=%PROTOCOL% code=%RESPONSE_CODE% flags=%RESPONSE_FLAGS% rx=%BYTES_RECEIVED% tx=%BYTES_SENT% ua=%REQ(USER-AGENT)% xff=%REQ(X-FORWARDED-FOR)% auth=%REQ(:AUTHORITY)% up=%UPSTREAM_HOST%\n`
≥2 probes (vary method GET/POST + a body + present/absent `user-agent`/`x-forwarded-for`) → assert each emitted line is byte-identical between Envoy and envoy-rust.

> NOTE the `%UPSTREAM_HOST%` per-side caveat (ADR-0079 / SPEC §1): the cross-proxy bytes match only if BOTH proxies resolve the SAME backend `ip:port`. Reuse the fixture-0036/0037 `discover_host_lan_ip` / `{{BACKEND_IP}}` single-shared-backend-IP technique so `%UPSTREAM_HOST%` is byte-identical. If a stable shared `ip:port` is not achievable, drop `%UPSTREAM_HOST%` from the fixture format (keep it backstop-only) — confirm at impl time.

- [ ] **Step 1: Write the fixture files + a failing differential test.** Add `assert_access_log_lines_byte_identical(envoy: &[String], envoy_rust: &[String]) -> Result<(), String>` (assert equal len + each line byte-equal). Add the `Driver` variant (reuse the `Http1WithAccessLog` file-wait/scrape machinery; compare whole-line). Register `0040`.
- [ ] **Step 2: Run, verify FAIL** (Docker-gated; locally if Docker available) — the differential RUNS the two proxies; expected FAIL until the impl is wired (it is, after T5 — so this may PASS first try, which is the foundation-slice-exercised-by-consumer signal; if so, assert the comparator catches a mutated line via a unit test).
- [ ] **Step 3: Implement/adjust** any byte mismatch found (e.g. the `%UPSTREAM_HOST%` shared-IP technique, or a truncation/absent-value discrepancy). Re-confirm against live Envoy locally.
- [ ] **Step 4: Run, verify PASS** — `cargo test -p differential http1 -- accesslog` (or the fixture-0040 test name) green LOCALLY; full Docker-gated run deferred to state-4.
- [ ] **Step 5: Commit** — `git commit -am "phase 32 t6: fixture 0040 + whole-line byte-exact access-log comparator [ADR-0079]"`

---

## Task 7: `accesslog_format_parse` fuzz target + ci.yml wiring + parse_bootstrap seed

**Files:**
- Create: `crates/envoy-accesslog/fuzz/Cargo.toml`, `crates/envoy-accesslog/fuzz/fuzz_targets/accesslog_format_parse.rs`
- Create: a seed corpus file under `crates/envoy-accesslog/fuzz/corpus/accesslog_format_parse/`
- Modify: `.github/workflows/ci.yml` (add the short-budget fuzz step — MEMORY: a new fuzz target is NOT auto-discovered; wire it by hand or §7.5 (d) is silently unmet)
- Create/Modify: `crates/envoy-config/fuzz/corpus/parse_bootstrap/` — a seed YAML with a `log_format`-bearing `FileAccessLog`

- [ ] **Step 1: Write the fuzz target** — `accesslog_format_parse.rs`: `fuzz_target!(|data: &[u8]| { if let Ok(s) = std::str::from_utf8(data) { let _ = envoy_accesslog::parse_format(s); } });` (the parser must never panic). Model `fuzz/Cargo.toml` on `crates/envoy-config/fuzz/Cargo.toml` (and `crates/envoy-filter/fuzz/` — the phase-31 `cdn_loop_parse` precedent). Add the new member to the workspace fuzz exclusion if the root `Cargo.toml` lists fuzz crates.
- [ ] **Step 2: Run, verify it builds + runs clean briefly** — `cd crates/envoy-accesslog/fuzz && cargo +nightly fuzz run accesslog_format_parse -- -runs=200000 -max_total_time=30` → 0 crashes.
- [ ] **Step 3: Wire ci.yml** — add a `cargo fuzz run accesslog_format_parse` short-budget step (mirror the `cdn_loop_parse`/`parse_bootstrap` steps; e.g. `-max_total_time=30`). Add the `parse_bootstrap` corpus seed.
- [ ] **Step 4: Verify** — `cargo build` for the fuzz crate; re-confirm `parse_bootstrap` still builds with the new seed. (CI runs the actual short-budget job at state-4.)
- [ ] **Step 5: Commit** — `git commit -am "phase 32 t7: accesslog_format_parse fuzz target + ci.yml wiring + parse_bootstrap seed (§7.4) [ADR-0079]"`

---

## Task 8: BEHAVIOR_CONTRACT extension + state-3 close

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` ("Access log field mapping" section)
- Modify: `docs/envoy-rust/phases/32-accesslog-command-operators/PROGRESS.md` (running record — created/updated each task)

- [ ] **Step 1:** Extend "Access log field mapping" with: the command-operator grammar (`%OP%`/`%OP(ARG)%`/`%OP(ARG):N%`/`%%`); the §B operator support matrix + name→field allow-list; the absent-value `-` sentinel; the boot-fatal config-validity rule; the trailing-newline rule (Fact 7); and the deterministic vs non-deterministic (`%START_TIME%`/`%DURATION%`/`%REQ(X-REQUEST-ID)%`/`%RESP(X-ENVOY-UPSTREAM-SERVICE-TIME)%`) classification with the allow-list equivalence rules. Reference fixture 0040.
- [ ] **Step 2:** Finalize `PROGRESS.md` (per-task SHAs + two-stage-review dispositions + any carry-forwards). Do NOT advance STATE here — state-3 close is a PROGRESS.md update; the state-4 verification gate is the NEXT session.
- [ ] **Step 3: Commit** — `git commit -am "phase 32 t8: BEHAVIOR_CONTRACT access-log operator extension + state-3 close [ADR-0079]"`

---

## Acceptance (the §7.5 phase-done gate — verified at state-4, NOT this PLAN)

(a) fixture `0040` green + (b) all `0001`–`0039` green simultaneously (incl. `0012` byte-identical UNCHANGED) + (c) h2spec ≥95% + (d) the new `accesslog_format_parse` fuzz target (wired into `ci.yml`) + `parse_bootstrap` clean for the short-budget CI run + (e) `cargo build --workspace --all-targets` / `cargo clippy --workspace --all-targets --all-features -- -D warnings` / `cargo fmt --all -- --check` / `cargo test --workspace` / `cargo deny check` all clean + (f) `REVIEW.md` approved. `#![forbid(unsafe_code)]` holds.

---

_Scope locked by ADR-0078; §6.2 facts locked by ADR-0079. §6.1 split (ADR-0080) UNFIRED. The state-3 implementation is the next session (`superpowers:subagent-driven-development`)._
