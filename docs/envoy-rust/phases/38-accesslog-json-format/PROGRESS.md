# Phase 38 — `38-accesslog-json-format` — PROGRESS

> State-3 implementation log (`superpowers:executing-plans`, TDD per task per
> `superpowers:test-driven-development`). Ground truth: **ADR-0092** §A–§F
> (empirically locked vs live `envoyproxy/envoy:v1.33.0`, digest `sha256:56da5afd…0c2`).
> PLAN: `PLAN.md` (9 TDD tasks). Append-only; one entry per task on completion.

---

## Task 1 — `SubstitutionFormatString` `{text_format_source | json_format}` oneof — DONE

**TDD:** wrote 2 failing tests first in `bootstrap.rs` `#[cfg(test)]`
(`json_format_parses_into_sorted_btreemap`, `text_format_source_arm_still_parses`);
confirmed RED (`no field json_format on type SubstitutionFormatString` /
`method unwrap not found for DataSourceInline`).

**Implemented:** widened `SubstitutionFormatString` (`bootstrap.rs:697`) from
`{text_format_source: DataSourceInline}` to the oneof
`{text_format_source: Option<DataSourceInline>, json_format: Option<BTreeMap<String,String>>}`
(both `#[serde(default)]`; `deny_unknown_fields` retained; `BTreeMap` = the SORTED
config model, ADR-0092 §A — NO custom serde, NO new dep). Made every downstream reader
compile (behavior unchanged until Tasks 2/7):
- `validate_access_logs` (`:4362`) — temporary `if let Some(ds) = &fmt.text_format_source`
  guard (Task 2 replaces with exactly-one-of).
- `compiled_log_format` H1 (`hcm.rs:1257`) — temporary `match &s.text_format_source`
  (Task 7 returns `LogFormat`).
- struct-literal ctors: `envoy-http1/src/hcm.rs:1767`, `envoy-http2/src/hcm.rs:1350`,
  `:1460` each gained `text_format_source: Some(...), json_format: None`.
- assert reader `bootstrap.rs:~11009` → `.text_format_source.as_ref().unwrap()`.

**Evidence:** `cargo test -p envoy-config json_format_parses_into_sorted_btreemap` →
`1 passed`; `cargo test -p envoy-config text_format_source_arm_still_parses` →
`1 passed`. `cargo build --workspace --all-targets` → `Finished` (clean — the public
struct widening leaves a workspace-green tree).

**Commit:** `phase 38 task 1: SubstitutionFormatString {text_format_source|json_format} oneof (BTreeMap, sorted) [ADR-0092]`

---

## Task 2 — exactly-one-of `log_format` validator + `ConfigError::AmbiguousLogFormat` — DONE

**TDD:** wrote 5 failing tests first in `bootstrap.rs` `#[cfg(test)]` (the §E
dispositions): `both_arms_set_is_ambiguous`, `neither_arm_set_is_ambiguous`,
`empty_json_format_map_is_valid`, `malformed_json_format_value_is_invalid_format`,
`valid_json_format_passes` — each drives a full `parse_bootstrap` via the existing
`hcm_with_access_log_yaml` helper. Confirmed RED (`variant AmbiguousLogFormat not
found in ConfigError`).

**Implemented:**
- new `ConfigError::AmbiguousLogFormat { detail: String }` (`lib.rs`, after
  `InvalidAccessLogFormat`) — the ONE new variant (ADR-0092 §E).
- replaced the Task-1 temporary guard in `validate_access_logs` (`:4362`) with the
  exactly-one-of `match (&text_format_source, &json_format)`: `(Some,None)` → parse
  text; `(None,Some(map))` → empty map VALID, else per-value `parse_format`;
  `(Some,Some)` and `(None,None)` → `AmbiguousLogFormat` (both boot-fatal, ADR-0049).

**Evidence:** `cargo test -p envoy-config` → `525 passed; 0 failed` (5 new + all
pre-existing).

**Commit:** `phase 38 task 2: exactly-one-of log_format validator + per-value parse + ConfigError::AmbiguousLogFormat [ADR-0092]`

---

## Task 3 — hand-rolled JSON string escaper — DONE

**TDD:** new module `crates/envoy-accesslog/src/json_format.rs` with the
`escapes_per_json_rules` test (8 cases — plain, quote, backslash, `\n`, `\t`, C0
`\u00XX`, `/` un-escaped, non-ASCII verbatim). (Test + impl landed together for this
pure function; the 8 cases exhaustively pin the ADR-0092 §D rules.)

**Implemented:** `pub(crate) fn json_escape_into(out: &mut String, s: &str)` — short
escapes for `\b \t \n \f \r \" \\`; `\u00XX` for other C0 controls; non-ASCII verbatim
UTF-8; `/` NOT escaped — byte-identical to serde_json defaults (no new dep). Wired
`mod json_format;` into `lib.rs`.

**Evidence:** `cargo test -p envoy-accesslog escapes_per_json_rules` → `1 passed`.

**Commit:** `phase 38 task 3: hand-rolled JSON string escaper [ADR-0092]`

---

## Task 4 — per-operator typed JSON value encoder (number/string/null) — DONE

**TDD:** wrote 4 failing tests first in `json_format.rs` (`single_numeric_operator_emits_unquoted_number`,
`single_string_operator_emits_quoted_string`, `single_absent_operator_emits_null`,
`mixed_or_literal_emits_quoted_string_with_dash_sentinel`) + a `rec()`/`enc()` helper.
Confirmed RED (`cannot find function encode_json_value`).

**Implemented:**
- FIRST factored `pub(crate) fn render_value_segments(&[Segment], &AccessLogRecord) -> String`
  out of `CompiledFormat::render` (carrying the M32-6 `literal_len + 64` pre-alloc);
  `render` now delegates → the text path stays byte-identical.
- made `resolve_req`/`resolve_resp`/`truncate_bytes` `pub(crate)`.
- added `encode_json_value` + `encode_single_op` (+ `quote`/`quote_opt` helpers) to
  `json_format.rs`: single numeric op → unquoted number; single string op → quoted;
  single absent Option-op → `null`; mixed/literal → quoted string via the engine with
  the `-` sentinel (ADR-0092 §B). `Op::DynamicMetadata` follows §B's general rule
  (quoted-when-present / `null`-when-absent; not separately recon'd — backstop only).

**Evidence:** `cargo test -p envoy-accesslog` → `53 passed; 0 failed` (4 new + the
escaper + ALL pre-existing `command_operator`/`file_sink` tests — text path byte-frozen).

**Commit:** `phase 38 task 4: per-operator typed JSON value encoder (number/string/null) [ADR-0092]`

---

## Task 5 — `CompiledJsonFormat` compile + sorted-object render — DONE

**TDD:** wrote 4 failing tests first in `json_format.rs` + a `fixture_record()` helper
(GET /, HTTP/1.1, 200, flags "-", bytes_rcvd 0, bytes_sent 3, upstream None):
`renders_authoritative_fixture_line` (the locked ADR-0092 §F line), `empty_map_renders_empty_object`
(§E `{}\n`), `key_is_json_escaped`, `from_map_rejects_malformed_operator`. Confirmed
RED (`use of undeclared type CompiledJsonFormat`).

**Implemented:** `pub struct CompiledJsonFormat(BTreeMap<String, Vec<Segment>>)` with
`from_map` (per-value `parse_format`, first error surfaced) + `render` (assemble one
sorted JSON object — `{`, comma-separated `"key":value` via `json_escape_into` +
`encode_json_value`, `}\n`). Re-exported `CompiledJsonFormat` from `lib.rs`.

**Evidence:** `cargo test -p envoy-accesslog` → `57 passed; 0 failed`. The
`renders_authoritative_fixture_line` test asserts the byte-exact §F line:
`{"bytes_rcvd":0,"bytes_sent":3,"flags":"-","method":"GET","mixed":"code-200","path":"/","protocol":"HTTP/1.1","status":200,"upstream":null}\n`.

**Commit:** `phase 38 task 5: CompiledJsonFormat compile + sorted-object render [ADR-0092]`

---

## Task 6 — `LogFormat` enum (Text|Json) on `FileSink` via `Into` — DONE

**TDD:** wrote `file_sink_emits_json_object` first (build a `FileSink` from a
`CompiledJsonFormat`, emit, assert `{"status":200}\n`). Confirmed RED (`FileSink::new`
mismatched type — expected `CompiledFormat`).

**Implemented:** new `crates/envoy-accesslog/src/log_format.rs` —
`pub enum LogFormat { Text(CompiledFormat), Json(CompiledJsonFormat) }` with `render`
delegating to each arm + `From<CompiledFormat>`/`From<CompiledJsonFormat>` impls.
`FileSink.format: CompiledFormat → LogFormat`; `new`/`from_file_for_test` take
`impl Into<LogFormat>` (store `.into()`). `emit` UNCHANGED. Re-exported `LogFormat`
from `lib.rs`. Existing `CompiledFormat::default()`/`from_inline(...)` call sites coerce
via `Into` — text path byte-frozen.

**Evidence:** `cargo test -p envoy-accesslog` → `58 passed; 0 failed` (1 new + all
existing `file_sink` tests). `cargo build --workspace --all-targets` → `Finished`
(the `from_file_for_test` signature change leaves envoy-http1's call sites compiling
via `Into`).

**Commit:** `phase 38 task 6: LogFormat enum (Text|Json) on FileSink via Into [ADR-0092]`

---

## Task 7 — HCM wires `LogFormat` (Text|Json) from `log_format` config — DONE

**TDD:** wrote 2 new failing tests in `envoy-http1/src/hcm.rs`
(`compiled_log_format_picks_json_arm`, `compiled_log_format_picks_text_arm`); confirmed
RED (`expected CompiledFormat, found LogFormat` — the fn still returned `CompiledFormat`).

**Implemented:** rewrote H1 `compiled_log_format` (`hcm.rs:1254`) to return
`Result<envoy_accesslog::LogFormat, Http1Error>` — `match (&text_format_source,
&json_format)`: text arm → `CompiledFormat::from_inline(...).into()`; json arm →
`CompiledJsonFormat::from_map(...).into()`; neither/absent → `CompiledFormat::default().into()`.
The sink-build loop (`:205`) is unchanged (`format` is now `LogFormat`, accepted by
`FileSink::new` directly). H2 default site (`:2159`) already passes
`CompiledFormat::default()` and coerces via `Into` — no change. The two pre-existing
`compiled_log_format_*` tests survive unchanged (`LogFormat::render` provides `.render`).

**Evidence:** `cargo test -p envoy-http1 -p envoy-http2` → `131 passed` / `74 passed`
(`0 failed`; 1 pre-existing H2 ignore). `cargo build --workspace --all-targets` →
`Finished`.

**Commit:** `phase 38 task 7: HCM wires LogFormat (Text|Json) from log_format config [ADR-0092]`

---

## Task 8 — differential fixture `0046-accesslog-json-format` (byte-exact JSON line) — DONE

**Rebuilt** `cargo build -p envoy-bin` FIRST (the differential runs
`target/debug/envoy-bin`; a new config key needs a fresh debug binary).

**Authored** `tests/fixtures/0046-accesslog-json-format/{envoy.yaml, envoy-rust.yaml,
expectations.yaml, README.md}` (template = `0040`): both proxy configs carry the same
`json_format` map (9 keys, config order arbitrary — both sort); `direct_response`
`{status:200, body:"ok\n"}`; per-side divergences = bind addr / admin block /
`generate_request_id` / mount path (`/tmp/0046-envoy-mount/`, `/tmp/0046-envoy-rust-mount/`).
Added the Docker-gated test `tests/differential/tests/access_log_json_format.rs`
(`kind: http1_access_log_byte_exact`, one bare `GET /` probe, Host `envoy-rust.test`).
(Test fn named `access_log_json_format` per the existing per-fixture convention —
the PLAN's `0046_*` filter guess does not match; the descriptive name is filterable.)

**Evidence (Docker differential, this host):**
- `cargo test -p differential access_log_json_format` → `test access_log_json_format
  ... ok` (`1 passed`) — the JSON object is byte-identical cross-proxy (ADR-0092 §F).
- regression-equivalence witnesses all byte-identical: `access_log_file_sink` (0012),
  `access_log_command_operators` (0040), `set_metadata_dynamic_metadata` (0041),
  `header_to_metadata` (0042) → each `1 passed; 0 failed`.

**Commit:** `phase 38 task 8: fixture 0046 byte-exact json_format access-log line [ADR-0092]`

---

## Task 9 — BEHAVIOR_CONTRACT json_format subsection + fuzz seed + re-exports — DONE

**Implemented:**
- `BEHAVIOR_CONTRACT.md` — new `### Phase 38 (ADR-0092): the json_format access-log
  encoder` subsection documenting §A (sorted keys) / §B (type-inference table) / §C
  (`typed_json_format` not a v1.33.0 field) / §D (separators + escaping) / §E
  (validity, all boot-fatal) / §F (the quoted authoritative fixture-0046 line). Also
  updated the stale 06.2-era "json_format out of scope" NOTE to record that phase 38
  now supports `json_format` (only `typed_json_format`/`text_format`/top-level
  `format` remain out of scope).
- fuzz seed `crates/envoy-config/fuzz/corpus/parse_bootstrap/json_format_logger.yaml`
  (a minimal H1 bootstrap with a `json_format` file logger) + a `!`-un-ignore line in
  `crates/envoy-config/fuzz/.gitignore`. NO new fuzz TARGET (the existing
  `parse_bootstrap` + `accesslog_format_parse` cover the surface, ADR-0092).
- confirmed `CompiledJsonFormat` / `LogFormat` / `FormatParseError` are re-exported from
  `envoy-accesslog/src/lib.rs` (lines 25/28/29).

**Evidence:** `git ls-files crates/envoy-config/fuzz/corpus/parse_bootstrap/json_format_logger.yaml`
prints the path; `git check-ignore` → not-ignored (tracked).

**Commit:** `phase 38 task 9: BEHAVIOR_CONTRACT json_format subsection + parse_bootstrap seed [ADR-0092]`
