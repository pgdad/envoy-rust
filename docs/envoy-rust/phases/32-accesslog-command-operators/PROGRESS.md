# Phase 32 — Access-log command-operator formatter — PROGRESS

> State-3 implementation running record (`superpowers:subagent-driven-development`).
> 8 PLAN.md tasks, SERIAL on `main`, each TDD + two-stage-reviewed (spec-compliance
> THEN code-quality, fresh subagents) + committed separately. Base commit
> `783d29f` (phase-32 state-2 PLAN-write). Per §5.1 this session does NOT advance
> STATE past state-3 and does NOT run the state-4 §7.5 verification gate.

---

## Task 1 — Command-operator PARSER (the correctness gate) — DONE

- **Commit:** `7917c8ab61fdf4a743e905630176698f5a4cf813` — `phase 32 t1: access-log command-operator parser (the correctness gate) [ADR-0079]`
- **Files:** created `crates/envoy-accesslog/src/command_operator.rs` (parser only); `crates/envoy-accesslog/src/lib.rs` (+`pub mod command_operator;`).
- **What landed:** `parse_format(&str) -> Result<Vec<Segment>, FormatParseError>` compiling a format string into `Vec<Segment>` (`Literal`/`Op`). `Op` carries the 10 §B-matrix operators; `Req`/`Resp` carry `{ name, alt, truncate }` (name/alt ASCII-lowercased). `thiserror`-based `FormatParseError` with human-readable detail (destined for the boot-fatal config error in Task 4). `%%`→literal `%`; case-sensitive keywords; `KEYWORD(ARG):N` grammar; adjacent-literal coalescing. `REQ_ALLOW_LIST`/`RESP_ALLOW_LIST` consts (reused by Task 2's field mapping). `#![forbid(unsafe_code)]` holds.
- **TDD:** 9 plan-spec tests written first → RED → impl → GREEN. (+2 non-ASCII regression tests, +specific-error-variant assertions — see below.)
- **Reconciliation recorded (plan-test tension):** the PLAN's Task-1 tests require `%REQ(X-MISSING?:PATH):5%` to parse OK while `%REQ(X-CUSTOM)%` errors. Both reconcile under ONE rule, which is what shipped: **a REQ/RESP op is valid iff at least one branch is a §B-backed field** — `name ∈ allow-list` OR (`alt` is `Some` and `∈ allow-list`); else config error. (X-MISSING is rescued by the backed `:path` alt; X-CUSTOM has no backed branch.) This affects only the internal parser; fixture 0040 uses only backed names, so no differential impact.

### Two-stage review
- **Stage 1 — spec-compliance:** ✅ Spec compliant. Found one Important latent bug **outside the spec surface**: literal text used `bytes[i] as char` → non-ASCII literals corrupted (mojibake), which would hard-fail the Task-6 byte-exact differential. **FIXED** (literal path now pushes `&s[run_start..i]` slices; `%` boundaries are ASCII so slices are valid UTF-8) + 2 regression tests (`literal_preserves_non_ascii`, `literal_non_ascii_with_percent_escape`). Re-verified.
- **Stage 2 — code-quality:** ✅ Approve. All findings Minor. Folded **#2** (error-path tests now assert the specific `FormatParseError` variant via `matches!`, not just `.is_err()` — pins the classification that is this module's core value).
- **Carry-forward Minors (NOT folded — cheap polish for whenever `command_operator.rs` is next touched):**
  - **C1** — `side: &'static str` ("REQ"/"RESP") is a stringly-typed pseudo-enum with a `_ =>` fallthrough conflating RESP with "anything else"; a 2-variant `enum Side { Req, Resp }` would make allow-list selection + `Op` construction total. Functionally correct today.
  - **C2** — `%REQ(:path?)%` yields `alt: Some("")` (harmless — empty never in allow-list, primary is backed); `:0` truncate accepted. A test pinning each would remove ambiguity for the Task-2 evaluator (whether zero-truncate is meaningful).
  - **C3** — `MalformedArgument(String, String)` is a positional 2-tuple; named fields would match the `UnsupportedHeader { .. }` struct-variant style. Cosmetic.
- **Verification at task close:** `cargo test -p envoy-accesslog command_operator` → 11 passed; `cargo clippy -p envoy-accesslog --all-targets -- -D warnings` clean; `cargo fmt -p envoy-accesslog -- --check` clean. (Workspace-wide clippy/fmt/test + the Docker differential are the state-4 §7.5 gate.)

---

## Task 2 — Compiled-format EVALUATOR (`render`) — DONE

- **Commit:** `cd73763974be66cefa34cc91bf79fbc6795aab67` — `phase 32 t2: compiled-format evaluator (render) [ADR-0079]`
- **Files:** `crates/envoy-accesslog/src/command_operator.rs` (+`CompiledFormat` + `render`/`render_op`/`resolve_req`/`resolve_resp`/`truncate_bytes`); `crates/envoy-accesslog/src/lib.rs` (+`pub use command_operator::{CompiledFormat, FormatParseError, parse_format};`).
- **What landed:** `CompiledFormat(pub(crate) Vec<Segment>)` (inner field gated from external tuple-construction — they use Task 3's `Default`/`from_inline`); `pub fn render(&self, &AccessLogRecord) -> String` — infallible (parser already rejected unbacked names). §B mapping: numeric ops decimal; `StartTime`→`crate::format_iso8601`; `Duration`→`as_millis()`; absent Option → `-`. `resolve_req` returns borrowed `&str` (zero-alloc hot path); `resolve_resp` owns the `Duration→ms` String. `?`-alt fires only when primary `None`. `:N` = at-most-N **bytes**, rounding DOWN via `str::floor_char_boundary` (stable ≥1.82; toolchain 1.95.0) — panic-safe on multi-byte, documented.
- **TDD:** 4 plan-spec tests (deterministic line, absent→`-`, byte-truncate, alt-fallback) → RED → impl → GREEN.
- **Two-stage review:** Stage-1 spec-compliance ✅ (verified mappings + 0-line diff to `default_format.rs`/`file_sink.rs`; no `Default`/`from_inline` over-build). Stage-2 code-quality ✅ Approve, all Minor; **folded #1** (added 4 coverage tests: RESP present→`7`/absent→`-`, multi-byte truncate `café:4`→`caf` & `:5`→`café`, alt+`:N` `xff?ua:4`→`curl`).
- **Verification at task close:** `cargo test -p envoy-accesslog` green; clippy `-D warnings` clean; `cargo fmt --check` clean.

---

## Task 3 — Default re-expression + `FileSink` verbatim-emit refactor — DONE

- **Commit:** `b5666eeb052959fb74c448b394820a8e1d0174d1` — `phase 32 t3: re-express default format through the engine + FileSink verbatim-emit refactor [ADR-0079]`
- **Files:** `crates/envoy-accesslog/src/{default_format.rs, command_operator.rs, file_sink.rs}`, `crates/envoy-http1/src/hcm.rs`, `crates/envoy-http2/src/hcm.rs`.
- **What landed:** `pub const DEFAULT_FORMAT` (byte-exact, **trailing `\n`**, md5-verified vs canonical). Old `format()` concatenator → `#[cfg(test)] fn legacy_format` (equivalence oracle); `push_or_dash` + the `AccessLogRecord` import also `#[cfg(test)]`-gated (production `default_format.rs` is now timestamp-helpers + the const); `format_iso8601`/calendar stay production. `CompiledFormat::from_inline` + `impl Default` (parses `DEFAULT_FORMAT`). `FileSink` gains `format: CompiledFormat`; `new(path, format)` + `from_file_for_test(path, file, format)`; **`emit` renders `self.format.render(record)` verbatim and DROPS the separate `write_all(b"\n")`** — the `\n` now rides in the format string. Net byte-identical for the default (proven below).
- **TDD:** `compiled_default_matches_legacy_concatenator` (engine default == `legacy_format + "\n"` → **fixture 0012 stays byte-identical**); `file_sink_writes_custom_format_verbatim` (`%RESPONSE_CODE%` → file == `"200"`, no newline). RED→GREEN.
- **DONE_WITH_CONCERNS resolved (both verified correct by Stage-1):**
  - **`emit` gained `file.flush().await`** after the single write — collapsing two writes to one hid an OS error on an `O_RDONLY` FD that two H1 fire-and-forget tests assert; `flush()` (buffer-flush, NOT fsync; durability still rides on close) restores in-`emit` error surfacing. Verified: does NOT change emitted bytes. Rationale documented at `file_sink.rs:109-122`.
  - **⚠ CARRY-FORWARD for Task 5:** the h2 `hcm.rs:1914` `FileSink::new` is `#[cfg(test)]` (helper `serve_one_h2_request_with_access_log`), NOT production — diverges from PLAN §A. **The ONLY production `FileSink::new` site is H1 `hcm.rs:206`.** H2 has NO production file-access-log construction in the tree. **Task 5 must FIRST investigate the real H2 production access-log path** (does H2 build sinks at all in production? does it route through H1's HCMConfig? is H2 file-access-log production-unwired?) before wiring config→sink. Do not assume a symmetric H2 production site exists.
- **Call sites updated (all pass `CompiledFormat::default()`):** H1 production `hcm.rs:206` (+`// Task 5 replaces…` comment); H1 test sites `:3580/3705/3908/4281`; H1 `from_file_for_test` `:3822/3976`; in-crate `file_sink.rs:154/189/209/261`; H2 test helper `:1914`. No `envoy-bin` sites (confirmed).
- **Two-stage review:** Stage-1 spec ✅ (DEFAULT_FORMAT byte-exact; items A/B verified). Stage-2 code-quality ✅ Approve, 2 Minor folded: **M1** dedicated `default_format_parses_successfully` guard test; **M2** merged the two inherent `impl CompiledFormat` blocks.
- **Verification at task close:** `cargo test -p envoy-accesslog` 37→ passed; `cargo build --workspace --all-targets` clean; `cargo test -p envoy-http1 -p envoy-http2` green (H1 125, H2 72/+1 pre-existing ignore); `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo fmt --all -- --check` clean.

---

## Task 4 — `FileAccessLog.log_format` config field + boot-fatal validator — DONE

- **Commit:** `c869f916d7f3e94b002781131168b278fb495786` — `phase 32 t4: FileAccessLog.log_format config field + boot-fatal format validator [ADR-0079]`
- **Files:** `crates/envoy-config/Cargo.toml` (+`envoy-accesslog` path-dep — direction safe, no cycle); `crates/envoy-config/src/bootstrap.rs` (struct + field + validator + tests); `crates/envoy-config/src/lib.rs` (`ConfigError::InvalidAccessLogFormat`); `Cargo.lock`.
- **What landed:** `FileAccessLog` gains `#[serde(default)] pub log_format: Option<SubstitutionFormatString>`. New `SubstitutionFormatString { text_format_source: DataSourceInline }` + `DataSourceInline { inline_string: String }` (model `envoy.config.core.v3.SubstitutionFormatString` / inline `DataSource`); all three `#[serde(deny_unknown_fields)]`. `ConfigError::InvalidAccessLogFormat { detail }` (provenance: Phase 32 / ADR-0079; boot-fatal per ADR-0049). `validate_access_logs` (`bootstrap.rs:~4049`): when `log_format` is `Some`, calls `envoy_accesslog::parse_format(&fmt.text_format_source.inline_string)` and maps `Err`→`InvalidAccessLogFormat { detail: e.to_string() }` (human-readable boot message). Stale `FileAccessLog` doc-comment ("format customization OUT of scope") rewritten.
- **TDD:** 4 plan tests via `crate::parse_bootstrap` (real validation path): valid `%RESPONSE_CODE%` OK; `%NOPE%`→`InvalidAccessLogFormat`; `%REQ(`→`InvalidAccessLogFormat`; absent→OK (`log_format.is_none()`). RED→GREEN. No downstream `FileAccessLog{...}` literals needed fixing (only the def exists).
- **Two-stage review:** Stage-1 spec ✅ (wire path 3-level exact; validator calls the real parser; `deny_unknown_fields` on all 3; build clean). Stage-2 code-quality ✅ Approve, 3 Minor — **#2 folded** (`rejects_hcm_with_unknown_nested_log_format_key`: a misspelled `inline_strings` key surfaces as `ConfigError::Yaml(_)` at *deserialization*, distinct from the validation error — locks the `deny_unknown_fields` contract).
  - **Minor #1 (dropped `Clone` on the 2 new structs) — NOT folded, deliberate:** the containing `FileAccessLog` is itself NOT `Clone` (pre-existing), and Task 5 only borrows (`&str`) the `inline_string`; matching the container's derive set is the consistent local choice; compiles workspace-wide. If a future phase needs to clone the config wire model, add `Clone` then.
  - **Minor #3 (mild per-test YAML duplication) — NOT folded:** reviewer judged it not worth refactoring (the shared `hcm_with_access_log_yaml` helper already removes the bulk; explicitness aids per-case readability).
- **Verification at task close:** `cargo test -p envoy-config` 481+ passed (incl. 5 new); `cargo build --workspace --all-targets` clean; `cargo clippy -p envoy-config --all-targets -- -D warnings` clean; `cargo fmt --all -- --check` clean.

---

## Task 5 — Thread the compiled `log_format` from config into `FileSink` (H1+H2) — DONE

- **Commit:** `174344e6fa3a4b7982367a0dcfe86dfffd22a22d` — `phase 32 t5: thread the compiled log_format from config into FileSink (H1+H2 HCM) [ADR-0079]`
- **Files:** `crates/envoy-http1/src/hcm.rs` (helper + loop wiring + 2 tests), `crates/envoy-http1/src/error.rs` (`Http1Error::AccessLogFormat`), `crates/envoy-config/src/lib.rs` (publish `DataSourceInline`/`SubstitutionFormatString`).
- **⚠ PLAN call-site inventory CORRECTED (the PLAN flagged §A as "verify"):** there is NO separate H2 production sink site. `envoy_http2::HCMConfig { pub inner: Arc<Http1HCMConfig> }`; `wrap(inner, h2_pool_mgr)` wraps the SAME `Arc<Http1HCMConfig>` built by `envoy_http1::HCMConfig::from_config`; envoy-bin builds ONE `hcm_config` (H1) and wraps it for HTTP2; H2 emission reads `config.inner.access_log`. ⇒ **the single production sink-construction is the H1 `from_config` loop (`hcm.rs:206`); H2 inherits the config-derived format via `wrap` with ZERO new H2 code.** The H2 `from_config` "production" site the PLAN named (`:1914`) is `#[cfg(test)]`. (Factual call-site correction, not a design decision — no ADR.)
- **What landed:** free fn `compiled_log_format(&FileAccessLog) -> Result<CompiledFormat, Http1Error>` — `Some(s)`→`CompiledFormat::from_inline(&s.text_format_source.inline_string)` (defensively `.map_err`→`Http1Error::AccessLogFormat`, NOT panic; the Task-4 validator already guarantees parse success, so this re-parse is unreachable-fail in practice — documented); `None`→`CompiledFormat::default()`. Called in the loop (`let format = compiled_log_format(file_cfg)?;`), placeholder gone. Dedicated `Http1Error::AccessLogFormat { message }` (mirrors `AccessLogOpen`). Published the two Task-4 wire structs (needed by the H1 wiring + tests; correct completion of Task 4's public surface).
- **TDD:** `compiled_log_format_uses_config_string_when_present` (`%REQ(:METHOD)% %RESPONSE_CODE%`→`"GET 200"`, verbatim, no `\n`); `compiled_log_format_falls_back_to_default_when_absent` (default render: ISO-8601 prefix + `"\"-\"\n"` suffix). RED→GREEN.
- **Two-stage review:** Stage-1 spec ✅ (loop calls helper, placeholder gone, no panic path, NO H2 production wiring, re-export benign, both branches real-`render` asserted). Stage-2 code-quality ✅ Approve, 2 Minor both **optional polish, NOT folded** (#1 default-branch test asserts prefix+suffix not full line — deliberate non-brittle: the full-default-line oracle lives in envoy-accesslog/Task-3; #2 an optional clarifying comment that inline formats render verbatim while only the default appends `\n`).
- **Verification at task close:** `cargo test -p envoy-http1` 127 passed; `cargo test -p envoy-http2` 72 passed (+1 pre-existing ignore — H2 inherits); `cargo build --workspace --all-targets` clean; `cargo clippy -p envoy-http1 -p envoy-http2 --all-targets -- -D warnings` clean; `cargo fmt --all -- --check` clean.

---

## Task 6 — Fixture 0040 + whole-line byte-exact access-log comparator — DONE (differential GREEN locally)

- **Commit:** `aad0c163eb8c69549f344ab648a203991eb7769d` — `phase 32 t6: fixture 0040 + whole-line byte-exact access-log comparator [ADR-0079]`
- **Files:** `tests/differential/src/access_log.rs` (comparator + 3 unit tests), `tests/differential/src/lib.rs` (`Driver::Http1AccessLogByteExact` + `AccessLogByteExactProbe` + dispatch + `wait_file_lines` helper + `ACCESS_LOG_FLUSH_WAIT` const), `tests/differential/tests/access_log_command_operators.rs` (test entry), `tests/fixtures/0040-accesslog-command-operators/{envoy.yaml,envoy-rust.yaml,inputs/payload.bin,expectations.yaml,README.md}`.
- **Design (decided up front — direct_response, deterministic-only):** `direct_response` route (model on 0012), NOT a backend → ZERO `{{BACKEND_IP}}` complexity. `%UPSTREAM_HOST%`→`-` (no upstream, byte-identical; the real `ip:port` render is backstop-proven by the Task-2 evaluator test). Format = `m=%REQ(:METHOD)% p=%REQ(:PATH)% proto=%PROTOCOL% code=%RESPONSE_CODE% flags=%RESPONSE_FLAGS% rx=%BYTES_RECEIVED% tx=%BYTES_SENT% ua=%REQ(USER-AGENT)% xff=%REQ(X-FORWARDED-FOR)% auth=%REQ(:AUTHORITY)% up=%UPSTREAM_HOST%\n`. **NO timing/UUID operators** (`%START_TIME%`/`%DURATION%`/`%REQ(X-REQUEST-ID)%`/`%RESP(X-ENVOY-UPSTREAM-SERVICE-TIME)%` are backstop-only, NEVER in the fixture).
- **New driver:** `kind: http1_access_log_byte_exact` → `Http1AccessLogByteExact { probes: Vec<AccessLogByteExactProbe>, expected_access_log_paths }`. Drives each probe via the shared `drive_http1`, waits (while containers alive) for both files to reach N lines, scrapes, asserts **line-count == probe-count (`bail!`) + whole-line byte-identical** over ALL lines. Two probes: bare `GET /` (`ua=- xff=-`) and `GET /` + `user-agent: curl/8.0`/`x-forwarded-for: 203.0.113.7`.
- **★ DOCKER DIFFERENTIAL RAN LOCALLY → GREEN** (`cargo test -p differential --test access_log_command_operators` → 1 passed, ~10s). Both lines byte-identical cross-proxy:
  - `m=GET p=/ proto=HTTP/1.1 code=200 flags=- rx=0 tx=3 ua=- xff=- auth=envoy-rust.test up=-`
  - `m=GET p=/ proto=HTTP/1.1 code=200 flags=- rx=0 tx=3 ua=curl/8.0 xff=203.0.113.7 auth=envoy-rust.test up=-`
  - This is the phase's CORE differential target. (Authoritative re-run is the state-4 §7.5 Linux-CI gate — but it already passes on this host.)
- **In-task harness fix (no engine change):** Envoy's ~10s `FileAccessLog` flush timer + testcontainers' `docker rm -f` SIGKILL drop buffered lines → the scrape must wait for N lines WHILE the container is alive (before `drop(upstream)`), budget `ACCESS_LOG_FLUSH_WAIT=15s`. envoy-rust emitted both lines immediately; the gap was purely Envoy flush cadence.
- **Two-stage review:** Stage-1 spec ✅ — **critical scrutiny PASSED**: the timing fix is additive (hard `bail!` on line-count mismatch + full N-line byte compare; a wait-timeout FAILS, never silently passes); comparator is true byte-equality (no trim/normalize); determinism + direct_response confirmed; NO engine-crate edits. Stage-2 code-quality ✅ Approve, 5 Minor — **#1/#2/#3 folded** (shared tested `wait_file_lines` helper; `ACCESS_LOG_FLUSH_WAIT` const + interpolated warn!; corrected the "Box" comment). #4 (unexercised `expected_status` default — harness affordance) / #5 (0-byte `inputs/payload.bin` — fixture convention, matches 0007/0012) — no change.
- **Verification at task close:** Docker differential GREEN (above); `cargo build -p differential --all-targets` clean; `cargo test -p differential --lib` 151 passed (incl. comparator + `wait_file_lines` tests); `cargo clippy -p differential --all-targets -- -D warnings` clean; `cargo fmt --all -- --check` clean. envoy-rust parses the `log_format`+trailing-`\n` YAML (`parse_bootstrap` PARSE_OK). No edits to `crates/*`.

---

## Task 7 — `accesslog_format_parse` fuzz target + ci.yml wiring + parse_bootstrap seed (§7.4) — DONE (200k runs / 0 crashes)

- **Commit:** `9539796` — `phase 32 t7: accesslog_format_parse fuzz target + ci.yml wiring + parse_bootstrap seed (§7.4) [ADR-0079]`
- **Files:** NEW `crates/envoy-accesslog/fuzz/{Cargo.toml, fuzz_targets/accesslog_format_parse.rs, .gitignore, corpus/accesslog_format_parse/*}` (envoy-accesslog's first `fuzz/`); root `Cargo.toml` (+`crates/envoy-accesslog/fuzz` in `[workspace] exclude`); `.github/workflows/ci.yml` (3 edits); NEW `crates/envoy-config/fuzz/corpus/parse_bootstrap/accesslog_log_format.yaml` (+ its `.gitignore` allow-list); `crates/envoy-config/fuzz/Cargo.lock` (legitimate sync — envoy-config now depends on envoy-accesslog since Task 4).
- **Fuzz target:** `#![no_main]` + `#![forbid(unsafe_code)]`; `fuzz_target!(|data| if let Ok(s)=from_utf8(data) { let _ = envoy_accesslog::parse_format(s); })` — UTF-8-gated (parser takes `&str`; correct, not under-fuzzing). Mirrors the `cdn_loop_parse`/`parse_bootstrap` precedent (self-`[workspace]`, libfuzzer-sys 0.4).
- **★ MEMORY DISCIPLINE — ci.yml wired BY HAND (a new fuzz target is NOT auto-discovered):** 3 edits to the `fuzz:` job — (a) job `name` += `accesslog_format_parse`; (b) cache `workspaces:` += `crates/envoy-accesslog/fuzz -> target`; (c) new step `cargo +nightly fuzz run accesslog_format_parse -- -max_total_time=30` (`working-directory: crates/envoy-accesslog`, sibling-indentation-exact). §7.5 gate (d) now genuinely wired (NOT silently unmet). YAML validated.
- **7 seed corpus** (valid default / simple / truncate+alt / `%%` escape / malformed-Err / empty / **multibyte** — the last folded from review #1, bootstraps the historically-mojibake-prone multibyte-literal path). parse_bootstrap seed exercises the new `log_format` config surface (verified `parse_bootstrap` accepts it).
- **TDD/verify:** fuzzer ran LOCALLY `cargo +nightly fuzz run accesslog_format_parse -- -runs=200000 -max_total_time=30` → **200000 runs, 0 crashes** (parser never panics). (Authoritative short-budget run is the state-4 CI job just wired.)
- **Two-stage review:** Stage-1 spec ✅ (the 3 ci.yml edits valid+correct, `working-directory` = crate root, corpus = only intended seeds no cruft, Cargo.lock sync legitimate, no `crates/*/src` change). Stage-2 code-quality ✅ Approve, 2 Minor — **#1 folded** (multibyte UTF-8 seed); #2 (trailing-newline cosmetic) skipped.
- **Verification at task close:** `cargo build --workspace --all-targets` clean; fuzzer 0 crashes; `cargo fmt --all -- --check` clean; ci.yml valid YAML. No `crates/*/src` edits.

---

## Task 8 — BEHAVIOR_CONTRACT extension + state-3 close — DONE

- **Commit:** `be82cbae1bcdd6a962890b38201cb1f272b7aaf1` — `phase 32 t8: BEHAVIOR_CONTRACT access-log operator extension + state-3 close [ADR-0079]` (THIS commit also tracks this PROGRESS.md for the first time).
- **Files:** `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (extend "Access log field mapping"); `docs/envoy-rust/phases/32-accesslog-command-operators/PROGRESS.md` (this file — finalized).
- **What landed:** a new `### Phase 32 (ADR-0079): configurable command-operator format engine` subsection documenting (reference-grade, D-3.4): the grammar (`%OP%`/`%OP(ARG)%`/`%OP(ARG):N%`/`%%`; `:N` byte-trunc rounded to char boundary; `?`-alt); absent-value `-`; the boot-fatal config-validity list (`ConfigError::InvalidAccessLogFormat`); the §B name→field allow-list matrix (15-field record, no generic header map; case-insensitive; valid iff ≥1 backed branch); the trailing-newline rule (Fact 7 → fixture 0012 byte-identical); and the DETERMINISTIC (fixture 0040) vs NON-DETERMINISTIC (backstop-only) classification + witness fixtures. The stale 06.2 "format customization OUT of scope / not in the struct" paragraph was CORRECTED (superseded for the modern `log_format.text_format_source.inline_string` path; `json_format`/`typed_json_format`/deprecated `text_format`/top-level `format` remain deferred + rejected).
- **Two-stage review:** Stage-1 spec ✅ — **every claim cross-checked against the actual code** (`command_operator.rs`/`default_format.rs`/`file_sink.rs`/`bootstrap.rs` + the 0040 fixture); zero inaccuracies, no missing facts, the §B table exactly matches `REQ_ALLOW_LIST`/`RESP_ALLOW_LIST`, the 0040 format string contains exactly the deterministic operators and none of the non-deterministic. Stage-2 doc-quality ✅ Approve, 4 Minor — **#1/#2 folded** (made `%REQ(X-ENVOY-ORIGINAL-PATH?:PATH)%` determinism explicit in the table; tightened the `%RESPONSE_FLAGS%` "no flags set (Envoy's no-flags sentinel)" wording); #3 (15-count magic number) / #4 (optional signpost) skipped as optional.
- **state-3 close:** this PROGRESS.md update IS the state-3 close. **STATE.md is NOT advanced** (stays `32` state-3 / state-4-next per §5.1 — the state-4 §7.5 verification gate is the NEXT session). No ADR added (all work under ADR-0079). `#![forbid(unsafe_code)]` holds throughout. ADR-0014 in force; ADR-0028 open. Ledger head unchanged: ADR-0079 (ADR-0080 reserved-but-UNFIRED — §6.1 split did not fire).

---

## Phase-32 state-3 summary (8/8 tasks DONE, SERIAL on `main`)

| Task | Commit | Two-stage review |
|---|---|---|
| 1 parser | `7917c8a` | spec ✅ (+UTF-8 mojibake fix) / quality ✅ (+error-variant tests) |
| 2 evaluator | `cd73763` | spec ✅ / quality ✅ (+RESP/multibyte/alt coverage) |
| 3 default re-expr + FileSink refactor | `b5666ee` | spec ✅ (DEFAULT_FORMAT byte-exact; `flush()` justified) / quality ✅ (+M1/M2) |
| 4 config field + validator | `c869f91` | spec ✅ / quality ✅ (+deny_unknown_fields test) |
| 5 config→FileSink wiring | `174344e` | spec ✅ (H2 inherits via `wrap`; PLAN call-site corrected) / quality ✅ |
| 6 fixture 0040 + comparator | `aad0c16` | spec ✅ (timing-fix preserves byte-exact) / quality ✅ (+helper/const) — **DOCKER DIFFERENTIAL GREEN LOCALLY** |
| 7 fuzz + ci.yml + seed | `9539796` | spec ✅ (ci.yml gate (d) wired) / quality ✅ (+multibyte seed) — **200k runs / 0 crashes** |
| 8 BEHAVIOR_CONTRACT + close | `be82cbae1bcdd6a962890b38201cb1f272b7aaf1` | spec ✅ (every claim code-verified) / quality ✅ (+clarity) |

**Phase outcome:** the hardcoded Envoy-v3 default-format emitter is now a configurable command-operator substitution engine driven by a per-`FileAccessLog` `log_format`; the cross-proxy byte-exact custom-format differential (fixture 0040) is GREEN; fixture 0012 is byte-preserved; all 8 tasks TDD + two-stage-reviewed + committed separately. **STATE stays `32` state-3 / state-4-next** — the §7.5 verification gate (`cargo build/clippy/fmt/test/deny` workspace-wide + `cargo fuzz` short-budget + the full Docker differential suite incl. all `0001`–`0040` + h2spec + `REVIEW.md`) is the NEXT session per §5.1.
